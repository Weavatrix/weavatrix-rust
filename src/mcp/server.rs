use crate::{Analyzer, Weavatrix, tools};
use mcport::{ServerIdentity, ToolReply, ToolServer, Value};
use notify::{EventKind, RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

const DERIVED_DIRECTORIES: &[&str] = &[
    ".git",
    ".weavatrix",
    ".codegraph",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".venv",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "target",
    "vendor",
];

#[derive(Debug)]
pub enum McpError {
    Io(io::Error),
    Repository(crate::Error),
}

impl Display for McpError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "MCP I/O failed: {error}"),
            Self::Repository(error) => {
                write!(formatter, "repository initialization failed: {error}")
            }
        }
    }
}

impl std::error::Error for McpError {}

impl From<io::Error> for McpError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<crate::Error> for McpError {
    fn from(value: crate::Error) -> Self {
        Self::Repository(value)
    }
}

/// Weavatrix tool surface behind the shared `mcport` stdio runtime.
///
/// Graph construction finishes before the handshake so startup does not split
/// CPU between a protocol thread and an analyzer thread. The first tool call
/// runs an incremental catch-up scan before using it, then starts the
/// filesystem watcher in the background. This keeps the full cold boundary
/// deterministic without returning stale evidence or adding an async executor.
struct WeavatrixServer {
    profile: super::McpProfile,
    identity: ServerIdentity,
    catalog: Value,
    tool_names: BTreeSet<String>,
    root: PathBuf,
    engine: Option<Weavatrix>,
    first_tool_call: bool,
    watcher: WatcherState,
}

impl WeavatrixServer {
    fn new(root: PathBuf, profile: super::McpProfile) -> Result<Self, McpError> {
        let definitions = tools::catalog_for_profile(profile);
        let tool_names = definitions
            .iter()
            .map(|definition| definition.name.to_owned())
            .collect();
        let catalog = blazingly_json::to_value(definitions)
            .map_err(|error| McpError::Io(io::Error::other(error)))?;
        let identity = ServerIdentity::new(
            "weavatrix-rust",
            env!("CARGO_PKG_VERSION"),
            "Local read-only repository intelligence. Inferred evidence is explicitly labelled.",
        );
        let engine = Weavatrix::open(&root)?;
        Ok(Self {
            profile,
            identity,
            catalog,
            tool_names,
            root,
            engine: Some(engine),
            first_tool_call: true,
            watcher: WatcherState::NotStarted,
        })
    }

    fn engine(&mut self) -> Result<&mut Weavatrix, crate::Error> {
        if self.engine.is_none() {
            let engine = Weavatrix::open(&self.root)?;
            engine.state().prime_weak_components();
            self.engine = Some(engine);
        }
        let Some(engine) = self.engine.as_mut() else {
            return Err(crate::Error::InvalidRepository(self.root.clone()));
        };
        Ok(engine)
    }

    fn catch_up_graph(&mut self) -> Result<(), String> {
        let engine = self
            .engine
            .as_mut()
            .ok_or_else(|| "repository graph is not initialized".to_owned())?;
        if engine
            .refresh_if_stale()
            .map_err(|error| format!("repository refresh failed: {error}"))?
        {
            engine.state().prime_weak_components();
        }
        Ok(())
    }

    fn start_watcher(&mut self) -> io::Result<()> {
        let root = self.root.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("weavatrix-watch-init".to_owned())
            .spawn(move || {
                let _ = sender.send(RepositoryWatcher::new(&root));
            })?;
        self.watcher = WatcherState::Starting(receiver);
        Ok(())
    }

    fn refresh_before_call(&mut self) -> Result<(), String> {
        let state = std::mem::replace(&mut self.watcher, WatcherState::NotStarted);
        let (watcher, catch_up) = match state {
            WatcherState::NotStarted => {
                let watcher = RepositoryWatcher::new(&self.root)
                    .map_err(|error| format!("repository watcher failed: {error}"))?;
                (watcher, true)
            }
            WatcherState::Starting(receiver) => {
                let watcher = receiver
                    .recv()
                    .map_err(|_| "repository watcher startup disconnected".to_owned())?
                    .map_err(|error| format!("repository watcher failed: {error}"))?;
                (watcher, true)
            }
            WatcherState::Ready(watcher) => (watcher, false),
        };
        self.watcher = WatcherState::Ready(watcher);

        let queued_change = self
            .ready_watcher()
            .map_err(|error| format!("repository watcher failed: {error}"))?
            .changed()
            .map_err(|error| format!("repository watcher failed: {error}"))?;
        if !catch_up && !queued_change {
            return Ok(());
        }

        let engine = self
            .engine
            .as_mut()
            .ok_or_else(|| "repository graph is not initialized".to_owned())?;
        if engine
            .refresh_if_stale()
            .map_err(|error| format!("repository refresh failed: {error}"))?
        {
            engine.state().prime_weak_components();
        }

        // Changes made while the catch-up scan was running were already
        // registered by the watcher. Apply one more incremental scan when
        // needed so the tool never observes the pre-change graph.
        if self
            .ready_watcher()
            .map_err(|error| format!("repository watcher failed: {error}"))?
            .changed()
            .map_err(|error| format!("repository watcher failed: {error}"))?
        {
            let engine = self
                .engine
                .as_mut()
                .ok_or_else(|| "repository graph is not initialized".to_owned())?;
            if engine
                .refresh_if_stale()
                .map_err(|error| format!("repository refresh failed: {error}"))?
            {
                engine.state().prime_weak_components();
            }
        }
        Ok(())
    }

    fn ready_watcher(&self) -> io::Result<&RepositoryWatcher> {
        match &self.watcher {
            WatcherState::Ready(watcher) => Ok(watcher),
            WatcherState::NotStarted | WatcherState::Starting(_) => Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "repository watcher is not ready",
            )),
        }
    }
}

enum WatcherState {
    NotStarted,
    Starting(Receiver<io::Result<RepositoryWatcher>>),
    Ready(RepositoryWatcher),
}

struct RepositoryWatcher {
    root: PathBuf,
    _watcher: notify::RecommendedWatcher,
    events: Receiver<notify::Result<notify::Event>>,
}

impl RepositoryWatcher {
    fn new(root: &Path) -> io::Result<Self> {
        let root = root.canonicalize()?;
        let (sender, events) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .map_err(io::Error::other)?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(io::Error::other)?;
        Ok(Self {
            root,
            _watcher: watcher,
            events,
        })
    }

    fn changed(&self) -> io::Result<bool> {
        let mut changed = false;
        loop {
            match self.events.try_recv() {
                Ok(Ok(event)) => {
                    if !matches!(event.kind, EventKind::Access(_))
                        && event
                            .paths
                            .iter()
                            .any(|path| analysis_input_changed(&self.root, path))
                    {
                        changed = true;
                    }
                }
                Ok(Err(error)) => return Err(io::Error::other(error)),
                Err(TryRecvError::Empty) => return Ok(changed),
                Err(TryRecvError::Disconnected) => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "repository filesystem watcher disconnected",
                    ));
                }
            }
        }
    }
}

fn analysis_input_changed(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let normalized = relative.to_string_lossy().replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    let file_name = relative
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if matches!(
        file_name.as_str(),
        ".gitignore" | ".ignore" | ".weavatrixignore"
    ) || matches!(lower.as_str(), ".git/config" | ".git/info/exclude")
    {
        return true;
    }
    if lower
        .split('/')
        .any(|component| DERIVED_DIRECTORIES.contains(&component))
    {
        return false;
    }
    Analyzer::default().supports_path(&normalized)
}

impl ToolServer for WeavatrixServer {
    fn identity(&self) -> ServerIdentity {
        self.identity.clone()
    }

    fn identity_ref(&self) -> Option<&ServerIdentity> {
        Some(&self.identity)
    }

    fn catalog(&mut self) -> Value {
        self.catalog.clone()
    }

    fn catalog_ref(&mut self) -> Option<&Value> {
        Some(&self.catalog)
    }

    fn has_tool(&self, name: &str) -> Option<bool> {
        Some(self.tool_names.contains(name))
    }

    fn call(&mut self, name: &str, arguments: Value) -> ToolReply {
        if !self.profile.allows(name) {
            return ToolReply::error(format!(
                "tool {name} is unavailable in the {:?} profile",
                self.profile
            ));
        }
        let graph_was_loaded = self.engine.is_some();
        let first_tool_call = self.first_tool_call;
        if first_tool_call {
            if let Err(error) = self.catch_up_graph() {
                return ToolReply::error(error);
            }
        } else if graph_was_loaded
            && !matches!(name, "rebuild_graph" | "open_repo")
            && let Err(error) = self.refresh_before_call()
        {
            return ToolReply::error(error);
        }
        let structured = arguments
            .get("output_format")
            .and_then(Value::as_str)
            .is_none_or(|format| format == "json");
        let (reply, opened_root) = {
            let engine = match self.engine() {
                Ok(engine) => engine,
                Err(error) => {
                    return ToolReply::error(format!("repository initialization failed: {error}"));
                }
            };
            match tools::call(engine, name, arguments) {
                Ok(value) => {
                    let opened_root =
                        (name == "open_repo").then(|| engine.state().root().to_path_buf());
                    (ToolReply::Success { value, structured }, opened_root)
                }
                Err(error) => (ToolReply::error(error), None),
            }
        };
        let opened_repository = opened_root.is_some();
        if let Some(root) = opened_root {
            self.root = root;
        }
        if (first_tool_call || !graph_was_loaded || opened_repository)
            && self.engine.is_some()
            && let Err(error) = self.start_watcher()
        {
            return ToolReply::error(format!("repository watcher startup failed: {error}"));
        }
        self.first_tool_call = false;
        reply
    }
}

/// Serves the read-only Weavatrix tool catalog over MCP stdio.
///
/// # Errors
///
/// Returns stdio failures or a missing repository root. Invalid requests are
/// returned as JSON-RPC errors and do not terminate the server.
pub fn serve(root: impl AsRef<Path>) -> Result<(), McpError> {
    serve_with_profile(root, super::McpProfile::All)
}

/// Serves one capability profile over the same read-only MCP runtime.
///
/// The repository root and graph are validated eagerly so misconfiguration
/// fails before the protocol handshake. The first tool call performs an
/// incremental catch-up scan, then later calls use filesystem events.
///
/// # Errors
///
/// Returns stdio failures or a missing repository root.
pub fn serve_with_profile(
    root: impl AsRef<Path>,
    profile: super::McpProfile,
) -> Result<(), McpError> {
    let root = root.as_ref().to_path_buf();
    if !root.is_dir() {
        return Err(McpError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            format!("repository root {} is not a directory", root.display()),
        )));
    }
    let mut server = WeavatrixServer::new(root, profile)?;
    mcport::serve(&mut server).map_err(McpError::Io)
}

#[cfg(test)]
mod tests {
    use super::{WatcherState, WeavatrixServer};
    // The request the runtime dispatches is built with the runtime's own JSON
    // type, so the test exercises the boundary rather than bypassing it.
    use mcport::{MODERN_PROTOCOL_VERSION, dispatch, json};
    use std::path::PathBuf;

    fn server(profile: crate::mcp::McpProfile) -> WeavatrixServer {
        WeavatrixServer::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")), profile).unwrap()
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn negotiates_lists_and_calls_tools() {
        let mut engine = server(crate::mcp::McpProfile::All);
        let initialized = dispatch(
            &mut engine,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-06-18"}
            }),
        )
        .expect("initialize is answered");
        assert_eq!(initialized["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(
            initialized["result"]["serverInfo"]["name"],
            "weavatrix-rust"
        );
        assert!(
            engine.engine.is_some() && matches!(engine.watcher, WatcherState::NotStarted),
            "initialize must use the ready graph without starting the watcher"
        );

        let modern_meta = json!({
            "io.modelcontextprotocol/protocolVersion": MODERN_PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientInfo": {
                "name": "weavatrix-test",
                "version": env!("CARGO_PKG_VERSION")
            },
            "io.modelcontextprotocol/clientCapabilities": {}
        });
        let discovered = dispatch(
            &mut engine,
            &json!({
                "jsonrpc": "2.0",
                "id": "discover",
                "method": "server/discover",
                "params": {"_meta": modern_meta.clone()}
            }),
        )
        .expect("modern server/discover is answered");
        assert_eq!(discovered["result"]["resultType"], "complete");
        assert_eq!(
            discovered["result"]["supportedVersions"][0],
            MODERN_PROTOCOL_VERSION
        );
        assert_eq!(
            discovered["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
            "weavatrix-rust"
        );
        assert!(
            engine.engine.is_some() && matches!(engine.watcher, WatcherState::NotStarted),
            "server/discover must not start the watcher"
        );

        let listed = dispatch(
            &mut engine,
            &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        )
        .expect("tools/list is answered");
        assert_eq!(
            listed["result"]["tools"].as_array().map(Vec::len),
            Some(crate::tools::catalog().len())
        );
        assert!(
            engine.engine.is_some() && matches!(engine.watcher, WatcherState::NotStarted),
            "tools/list must not start the watcher"
        );

        let modern_listed = dispatch(
            &mut engine,
            &json!({
                "jsonrpc": "2.0",
                "id": "modern-list",
                "method": "tools/list",
                "params": {"_meta": modern_meta}
            }),
        )
        .expect("modern tools/list is answered");
        assert_eq!(modern_listed["result"]["resultType"], "complete");
        assert_eq!(
            modern_listed["result"]["tools"].as_array().map(Vec::len),
            Some(crate::tools::catalog().len())
        );

        let called = dispatch(
            &mut engine,
            &json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {"name": "graph_stats", "arguments": {}}
            }),
        )
        .expect("tools/call is answered");
        assert_eq!(called["result"]["isError"], false);
        assert!(
            called["result"]["structuredContent"]["nodes"]
                .as_u64()
                .unwrap()
                > 0
        );

        let mut code = server(crate::mcp::McpProfile::Code);
        let text = dispatch(
            &mut code,
            &json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "graph_stats",
                    "arguments": {"output_format": "text"}
                }
            }),
        )
        .expect("tools/call is answered");
        assert!(text["result"].get("structuredContent").is_none());

        let denied = dispatch(
            &mut code,
            &json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {"name": "seo_link_suggestions", "arguments": {}}
            }),
        )
        .expect("tools/call is answered");
        assert_eq!(denied["error"]["code"], -32_602);
        assert_eq!(
            denied["error"]["message"],
            "unknown tool: seo_link_suggestions"
        );
    }

    #[test]
    fn mcp_refreshes_after_a_real_source_change() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "weavatrix-mcp-watcher-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("source.rs"), "fn first() {}\n").unwrap();
        let mut engine = WeavatrixServer::new(root.clone(), crate::mcp::McpProfile::All).unwrap();
        std::fs::write(root.join("source.rs"), "fn first() {}\nfn second() {}\n").unwrap();
        let first = dispatch(
            &mut engine,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {"name": "graph_stats", "arguments": {}}
            }),
        )
        .unwrap();
        let first_revision = first["result"]["structuredContent"]["revision"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            first["result"]["structuredContent"]["node_kinds"]["function"],
            2
        );

        std::fs::write(
            root.join("source.rs"),
            "fn first() {}\nfn second() {}\nfn third() {}\n",
        )
        .unwrap();
        let second = dispatch(
            &mut engine,
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {"name": "graph_stats", "arguments": {}}
            }),
        )
        .unwrap();
        assert_eq!(
            second["result"]["structuredContent"]["node_kinds"]["function"],
            3
        );
        assert_ne!(
            second["result"]["structuredContent"]["revision"].as_str(),
            Some(first_revision.as_str())
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
