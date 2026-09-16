use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use weavatrix_rust::Weavatrix;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct Fixture {
    pub root: PathBuf,
}

impl Fixture {
    pub(crate) fn catalog() -> Self {
        let fixture = Self::empty();
        write_portable(&fixture);
        write_native(&fixture);
        write_limits(&fixture);
        write_contracts(&fixture);
        fixture.write("package.json", r#"{"name":"agent-fixtures"}"#);
        fixture
    }

    pub(crate) fn empty() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "weavatrix-agent-{}-{nonce}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    pub(crate) fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).unwrap();
        fs::write(path, contents).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_portable(fixture: &Fixture) {
    fixture.write(
        "search-a/plugin.json",
        r#"{
          "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
          "name": "search-a",
          "version": "1.0.0",
          "extensions": { "com.example.client": { "setting": true } }
        }"#,
    );
    fixture.write(
        "search-a/mcp.json",
        r#"{
          "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
          "mcpServers": {
            "search": {
              "type": "stdio",
              "command": "./bin/search",
              "args": ["--data", "${PLUGIN_DATA}/index"]
            }
          }
        }"#,
    );
    fixture.write(
        "search-a/skills/summarize/SKILL.md",
        "---\nname: summarize\ndescription: Summarize local notes.\n---\n\nSee [guide](references/guide.md).\n",
    );
    fixture.write(
        "search-b/plugin.json",
        r#"{
          "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
          "name": "search-b"
        }"#,
    );
    fixture.write(
        "search-b/mcp.json",
        r#"{
          "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
          "mcpServers": {
            "search": {
              "type": "streamable-http",
              "url": "https://search.example.com/mcp"
            }
          }
        }"#,
    );
}

fn write_native(fixture: &Fixture) {
    fixture.write(
        "native/.cursor-plugin/plugin.json",
        r#"{
          "name": "lookup",
          "version": "1.0.0",
          "skills": "./skills/",
          "mcpServers": "./mcp.json",
          "interface": { "displayName": "Lookup" }
        }"#,
    );
    fixture.write(
        "native/mcp.json",
        r#"{
          "mcpServers": {
            "lookup": {
              "command": "npx",
              "args": ["-y", "lookup"]
            }
          }
        }"#,
    );
    fixture.write(
        "native/skills/lookup/SKILL.md",
        "---\nname: lookup\ndescription: Look up a customer.\nallowed-tools: Read Grep\n---\n\nUse lookup, then stop.\n",
    );
}

fn write_limits(fixture: &Fixture) {
    fixture.write(
        "broken/plugin.json",
        r#"{ "name": "Broken Name", "skills": "./skills/" }"#,
    );
    fixture.write(
        "escape/mcp.json",
        r#"{
          "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
          "mcpServers": {
            "escaped": {
              "type": "stdio",
              "command": "../bin/server"
            }
          }
        }"#,
    );
    fixture.write(
        "secret/mcp.json",
        r#"{
          "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
          "mcpServers": {
            "remote": {
              "type": "streamable-http",
              "url": "https://api.example.com/mcp",
              "headers": { "Authorization": "Bearer secret-token" }
            }
          }
        }"#,
    );
}

fn write_contracts(fixture: &Fixture) {
    fixture.write(
        "catalogs/before.json",
        r#"{
          "$schema": "https://weavatrix.dev/schemas/agent-catalog/1.json",
          "protocol": "2025-11-25",
          "scope": "sales-before",
          "generation": "1",
          "complete": true,
          "tools": [
            {
              "name": "get_customer",
              "description": "Fetch one customer",
              "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
              }
            },
            {
              "name": "lookup_customer",
              "description": "Agent-facing lookup",
              "inputSchema": {
                "type": "object",
                "properties": { "customer_id": { "type": "string" } },
                "required": ["customer_id"]
              }
            }
          ]
        }"#,
    );
    fixture.write(
        "catalogs/after.json",
        r#"{
          "$schema": "https://weavatrix.dev/schemas/agent-catalog/1.json",
          "protocol": "2026-07-28",
          "scope": "sales-after",
          "generation": "2",
          "complete": true,
          "tools": [
            {
              "name": "get_customer",
              "description": "Fetch one customer",
              "inputSchema": {
                "type": "object",
                "properties": {
                  "id": { "type": "string" },
                  "tenant": { "type": "string" }
                },
                "required": ["id", "tenant"]
              }
            },
            {
              "name": "lookup_customer",
              "description": "Agent-facing lookup",
              "inputSchema": {
                "type": "object",
                "properties": { "customer_id": { "type": "string" } },
                "required": ["customer_id"]
              }
            }
          ],
          "transforms": [
            {
              "exposure": "lookup_customer",
              "upstream": "get_customer",
              "rename": { "customer_id": "id" },
              "inject": ["tenant"]
            }
          ]
        }"#,
    );
    fixture.write(
        "catalogs/origin.json",
        r#"{
          "$schema": "https://weavatrix.dev/schemas/agent-origin/1.json",
          "bindings": [
            { "tool": "get_customer", "handler": "src/customer.rs:get_customer" }
          ]
        }"#,
    );
    fixture.write(
        "catalogs/events.json",
        r#"{
          "$schema": "https://weavatrix.dev/schemas/agent-observation/1.json",
          "events": [
            { "id": "inv-1", "producer": "granttap", "kind": "invocation", "target": "lookup_customer", "result": "success" },
            { "id": "inv-1", "producer": "granttap", "kind": "invocation", "target": "lookup_customer", "result": "success" }
          ]
        }"#,
    );
    fixture.write(
        "src/dispatch.rs",
        "fn dispatch(name: &str) {\n    match name {\n        \"get_customer\" => customer::get_customer(),\n        _ => panic!(\"unknown tool\"),\n    }\n}\n",
    );
    fixture.write("src/factory.py", "@mcp.tool()\nmcp.tool(make_tools)\n");
}

pub(crate) fn engine() -> (Fixture, Weavatrix) {
    let fixture = Fixture::catalog();
    let engine = Weavatrix::open(&fixture.root).unwrap();
    (fixture, engine)
}

#[allow(dead_code)]
pub(crate) fn dump(value: &blazingly_json::Value) -> String {
    blazingly_json::to_string(value).unwrap_or_default()
}
