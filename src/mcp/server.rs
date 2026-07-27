use crate::{Weavatrix, tools};
use serde_json::{Value, json};
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use weavatrix_mcp::{ServerIdentity, ToolReply, ToolServer};

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

/// Weavatrix tool surface behind the shared `weavatrix-mcp` stdio runtime.
///
/// Graph construction is deferred to the first tool call; the runtime answers
/// `initialize`, `ping`, and `tools/list` from the catalog alone, so the
/// handshake is instant on repositories of any size and there is no async
/// executor anywhere in the stack.
struct WeavatrixServer {
    profile: super::McpProfile,
    root: PathBuf,
    engine: Option<Weavatrix>,
}

impl WeavatrixServer {
    fn engine(&mut self) -> Result<&mut Weavatrix, crate::Error> {
        if self.engine.is_none() {
            self.engine = Some(Weavatrix::open(&self.root)?);
        }
        Ok(self.engine.as_mut().expect("engine initialized above"))
    }
}

impl ToolServer for WeavatrixServer {
    fn identity(&self) -> ServerIdentity {
        ServerIdentity::new(
            "weavatrix-rust",
            env!("CARGO_PKG_VERSION"),
            "Local read-only repository intelligence. Inferred evidence is explicitly labelled.",
        )
    }

    fn catalog(&mut self) -> Value {
        serde_json::to_value(tools::catalog_for_profile(self.profile)).unwrap_or_else(|_| json!([]))
    }

    fn call(&mut self, name: &str, arguments: Value) -> ToolReply {
        if !self.profile.allows(name) {
            return ToolReply::error(format!(
                "tool {name} is unavailable in the {:?} profile",
                self.profile
            ));
        }
        let engine = match self.engine() {
            Ok(engine) => engine,
            Err(error) => {
                return ToolReply::error(format!("repository initialization failed: {error}"));
            }
        };
        if !matches!(name, "rebuild_graph" | "open_repo")
            && let Err(error) = engine.refresh_if_stale()
        {
            return ToolReply::error(format!("repository refresh failed: {error}"));
        }
        let structured = arguments
            .get("output_format")
            .and_then(Value::as_str)
            .is_none_or(|format| format == "json");
        match tools::call(engine, name, arguments) {
            Ok(value) => ToolReply::Success { value, structured },
            Err(error) => ToolReply::error(error),
        }
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
/// The repository root is validated eagerly so misconfiguration still fails
/// fast, but graph construction is deferred to the first tool call so the
/// `initialize` handshake responds instantly on repositories of any size.
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
    let mut server = WeavatrixServer {
        profile,
        root,
        engine: None,
    };
    weavatrix_mcp::serve(&mut server).map_err(McpError::Io)
}

#[cfg(test)]
mod tests {
    use super::WeavatrixServer;
    use serde_json::json;
    use std::path::PathBuf;
    use weavatrix_mcp::dispatch;

    fn server(profile: crate::mcp::McpProfile) -> WeavatrixServer {
        WeavatrixServer {
            profile,
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            engine: None,
        }
    }

    #[test]
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
            engine.engine.is_none(),
            "initialize must not build the graph"
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
            engine.engine.is_none(),
            "tools/list must not build the graph"
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
        assert_eq!(denied["result"]["isError"], true);
    }
}
