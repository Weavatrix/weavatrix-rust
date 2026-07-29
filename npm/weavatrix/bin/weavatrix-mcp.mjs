#!/usr/bin/env node
// MCP stdio entry - drop-in compatible with the JavaScript weavatrix-mcp bin:
//   weavatrix-mcp <repoRoot> [--profile=all|code|seo]
// Spawns the native Rust server with stdio inherited; this wrapper adds no
// buffering, no framing, and no event-loop work between client and server.
import { runNative } from './run-native.mjs'

runNative('mcp', 'weavatrix-mcp')
