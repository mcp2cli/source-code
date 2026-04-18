# Design: MCP Server→Client Message Handling

> **Status: Implemented and validated.**

## Overview

Route all MCP server→client messages (notifications, requests) through a
unified `ServerMessageHandler` trait, integrating with the existing
`EventBroker` infrastructure for delivery to all configured sinks.

## What Gets Handled

| Direction | Type | Method | Handler |
|-----------|------|--------|---------|
| S→C | Notification | `notifications/progress` | → `RuntimeEvent::Progress` |
| S→C | Notification | `notifications/message` | → `RuntimeEvent::ServerLog` (new) |
| S→C | Notification | `notifications/tools/list_changed` | → info event + stale marker |
| S→C | Notification | `notifications/resources/list_changed` | → info event + stale marker |
| S→C | Notification | `notifications/prompts/list_changed` | → info event + stale marker |
| S→C | Notification | `notifications/resources/updated` | → info event |
| S→C | Request | `elicitation/create` | → interactive terminal prompt |
| S→C | Request | `sampling/createMessage` | → human-in-the-loop terminal |
| S→C | Request | unknown | → JSON-RPC -32601 error |

## Architecture

### ServerMessageHandler trait

```
trait ServerMessageHandler: Send + Sync {
    fn handle_notification(&self, method: &str, params: Option<&Value>);
    fn handle_request(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse>;
}
```

### OperationMessageHandler struct

Built per-`perform()` call, holding:
- `app_id: String`
- `events: EventBroker`
- `inventory_stale_path: Option<PathBuf>` (for list_changed invalidation)

### Event delivery sinks

All runtime events route through `EventBroker → [sinks]`:
- **StderrEventSink** — human-readable lines on stderr
- **MemoryEventSink** — captured for JSON output rendering
- **HttpWebhookSink** — POST JSON to HTTP endpoint
- **UnixSocketSink** — NDJSON to Unix domain socket
- **SseServerSink** — text/event-stream to connected clients
- **CommandExecSink** (new) — run a shell command with event interpolation

### CommandExecSink

Runs a shell command template for each event, with environment variables:
- `MCP_EVENT_TYPE` — event type (info, progress, server_log, etc.)
- `MCP_EVENT_JSON` — full JSON-serialized event
- `MCP_EVENT_APP_ID` — app_id field
- `MCP_EVENT_MESSAGE` — human-readable message line

Config:
```yaml
events:
  command: "notify-send 'mcp2cli' '$MCP_EVENT_MESSAGE'"
```

Or with direct interpolation:
```yaml
events:
  command: "curl -s -X POST http://hooks/mcp -d '${MCP_EVENT_JSON}'"
```

## Implementation Phases

### Phase 1 — Notification routing
1. Add `RuntimeEvent::ServerLog` variant
2. Create `ServerMessageHandler` trait + `OperationMessageHandler`
3. Update stdio read loop to dispatch notifications
4. Move elicitation into handler's `handle_request()`

### Phase 2 — Sampling
5. Add `sampling/createMessage` terminal handler
6. Advertise `sampling` capability

### Phase 3 — HTTP SSE parity
7. Refactor SSE parser for interleaved events
8. Add response-back POST for HTTP server→client requests

### Phase 4 — Polish
9. `CommandExecSink` event delivery
10. `OperationContext` refactor of `McpClient::perform`
11. Inventory stale marker + auto-refresh

## Capability Advertisement

```rust
ClientCapabilities {
    elicitation: Some(CapabilityMarker {}),
    sampling: Some(CapabilityMarker {}),
    ..Default::default()
}
```

## CLI User Experience

### Progress during tool calls
```
$ work analyze-dataset --dataset q4-2025
[work] analyze-1 1/5 Loading dataset...
[work] analyze-1 5/5 Complete
```

### Server log messages
```
$ work deploy --env staging
[work] server info (deploy): Image pulled: myapp:2.1.0
```

### Sampling request
```
$ work generate-code --spec api-spec.yaml
--- sampling request ---
The server requests a model response.
Model hint: claude-3-5-sonnet
Messages:
  [user] Generate a REST controller
Your response (or 'decline' to reject):
> ...
--- end sampling ---
```

### Command execution sink
```yaml
events:
  command: "logger -t mcp2cli '${MCP_EVENT_MESSAGE}'"
```
