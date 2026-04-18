//! Handles server→client messages (notifications and requests) received during
//! MCP operations. This module provides a unified dispatch layer that both the
//! stdio and streamable HTTP transports call into.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value, json};

use crate::mcp::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use crate::runtime::{EventBroker, RuntimeEvent};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Handles server→client messages received during a pending operation.
pub trait ServerMessageHandler: Send + Sync {
    /// Called when a server→client notification arrives (has `method`, no `id`).
    fn handle_notification(&self, method: &str, params: Option<&Value>);

    /// Called when a server→client request arrives (has `method` and `id`).
    /// Must return a JSON-RPC response.
    fn handle_request(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse>;
}

// ---------------------------------------------------------------------------
// Concrete handler built per operation
// ---------------------------------------------------------------------------

/// Context for handling server messages during a single `perform()` call.
pub struct OperationMessageHandler {
    pub app_id: String,
    pub events: EventBroker,
    /// Path to a marker file; if set, `list_changed` notifications write this file
    /// to signal the discovery cache should be refreshed.
    pub inventory_stale_path: Option<PathBuf>,
    /// Configured roots to return for `roots/list` requests.
    pub roots: Vec<RootEntry>,
}

/// A root entry that the client exposes to the server via `roots/list`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RootEntry {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ServerMessageHandler for OperationMessageHandler {
    fn handle_notification(&self, method: &str, params: Option<&Value>) {
        match method {
            "notifications/progress" => self.handle_progress(params),
            "notifications/message" => self.handle_server_log(params),
            "notifications/tools/list_changed"
            | "notifications/resources/list_changed"
            | "notifications/prompts/list_changed" => self.handle_list_changed(method),
            "notifications/resources/updated" => self.handle_resource_updated(params),
            "notifications/elicitation/complete" => {
                tracing::debug!("elicitation completed");
                self.events.emit(RuntimeEvent::Info {
                    app_id: self.app_id.clone(),
                    message: "elicitation completed".to_owned(),
                });
            }
            "notifications/cancelled" => {
                let request_id = params
                    .and_then(|p| p.get("requestId"))
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "(unknown)".to_owned());
                let reason = params
                    .and_then(|p| p.get("reason"))
                    .and_then(Value::as_str)
                    .unwrap_or("no reason");
                tracing::debug!("server cancelled request {}: {}", request_id, reason);
                self.events.emit(RuntimeEvent::Info {
                    app_id: self.app_id.clone(),
                    message: format!("server cancelled request {}: {}", request_id, reason),
                });
            }
            "notifications/tasks/status" => self.handle_task_status(params),
            _ => {
                tracing::debug!("ignoring unknown server notification: {}", method);
            }
        }
    }

    fn handle_request(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        match request.method.as_str() {
            "elicitation/create" => handle_elicitation_request(request),
            "sampling/createMessage" => handle_sampling_request(request, &self.app_id),
            "roots/list" => self.handle_roots_list(request),
            _ => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_owned(),
                id: request.id.clone(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("method not found: {}", request.method),
                    data: None,
                }),
            }),
        }
    }
}

impl OperationMessageHandler {
    fn handle_progress(&self, params: Option<&Value>) {
        let Some(params) = params else { return };
        let token = params
            .get("progressToken")
            .and_then(|v| {
                v.as_str()
                    .map(ToOwned::to_owned)
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
            })
            .unwrap_or_else(|| "progress".to_owned());
        let progress = params.get("progress").and_then(Value::as_u64).unwrap_or(0);
        let total = params.get("total").and_then(Value::as_u64);
        let message = params
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();

        self.events.emit(RuntimeEvent::Progress {
            app_id: self.app_id.clone(),
            operation: token,
            current: progress,
            total,
            message,
        });
    }

    fn handle_server_log(&self, params: Option<&Value>) {
        let Some(params) = params else { return };
        let level = params
            .get("level")
            .and_then(Value::as_str)
            .unwrap_or("info")
            .to_owned();
        let logger = params
            .get("logger")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let data = params.get("data");
        let message = match data {
            Some(Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => String::new(),
        };

        self.events.emit(RuntimeEvent::ServerLog {
            app_id: self.app_id.clone(),
            level,
            logger,
            message,
        });
    }

    fn handle_list_changed(&self, method: &str) {
        let kind = if method.contains("tools") {
            "tools"
        } else if method.contains("resources") {
            "resources"
        } else {
            "prompts"
        };

        self.events.emit(RuntimeEvent::ListChanged {
            app_id: self.app_id.clone(),
            kind: kind.to_owned(),
            message: format!("server {} have changed; run 'ls' to refresh", kind),
        });

        // Write stale marker so the next invocation forces re-discovery.
        if let Some(path) = &self.inventory_stale_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, b"stale");
        }
    }

    fn handle_resource_updated(&self, params: Option<&Value>) {
        let uri = params
            .and_then(|p| p.get("uri"))
            .and_then(Value::as_str)
            .unwrap_or("(unknown)");
        self.events.emit(RuntimeEvent::Info {
            app_id: self.app_id.clone(),
            message: format!("resource updated: {}", uri),
        });
    }

    fn handle_task_status(&self, params: Option<&Value>) {
        let Some(params) = params else { return };
        let task_id = params
            .get("taskId")
            .and_then(Value::as_str)
            .unwrap_or("(unknown)");
        let status = params
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let message = params.get("message").and_then(Value::as_str).unwrap_or("");
        self.events.emit(RuntimeEvent::Info {
            app_id: self.app_id.clone(),
            message: format!("task {} status: {} {}", task_id, status, message),
        });
    }

    /// Handle `roots/list` server→client request.
    fn handle_roots_list(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        let roots_json: Vec<Value> = self
            .roots
            .iter()
            .map(|r| {
                let mut entry = json!({ "uri": r.uri });
                if let Some(name) = &r.name {
                    entry["name"] = json!(name);
                }
                entry
            })
            .collect();
        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: request.id.clone(),
            result: Some(json!({ "roots": roots_json })),
            error: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Elicitation handler (`elicitation/create`)
// ---------------------------------------------------------------------------

/// Handles the `elicitation/create` server→client request by prompting the user
/// on the terminal (stderr for prompts, stdin for values).
///
/// Supports two modes per MCP 2025-11-25:
/// - `"form"` (default): structured field-by-field prompting.
/// - `"url"`: direct the user to an external URL for out-of-band interaction.
fn handle_elicitation_request(request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
    let params = request
        .params
        .as_ref()
        .ok_or_else(|| anyhow!("elicitation/create request is missing params"))?;

    let mode = params.get("mode").and_then(Value::as_str).unwrap_or("form");

    match mode {
        "form" | "" => handle_form_elicitation(request, params),
        "url" => handle_url_elicitation(request, params),
        other => Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: request.id.clone(),
            result: None,
            error: Some(JsonRpcError {
                code: -32602,
                message: format!("unsupported elicitation mode: {}", other),
                data: None,
            }),
        }),
    }
}

/// Form-mode elicitation: collect structured fields from the user via stdin/stderr.
fn handle_form_elicitation(request: &JsonRpcRequest, params: &Value) -> Result<JsonRpcResponse> {
    let message = params
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("The server is requesting input:");
    let schema = params.get("requestedSchema");

    eprintln!("--- elicitation request ---");
    eprintln!("{}", message);

    let mut content = serde_json::Map::new();

    if let Some(schema) = schema
        && let Some(properties) = schema.get("properties").and_then(Value::as_object)
    {
        let required: Vec<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();

        let stdin = io::stdin();
        let mut stdin_lock = stdin.lock();

        for (key, prop) in properties {
            let title = prop.get("title").and_then(Value::as_str).unwrap_or(key);
            let description = prop.get("description").and_then(Value::as_str);
            let default_value = prop.get("default");
            let is_required = required.contains(&key.as_str());
            let prop_type = prop.get("type").and_then(Value::as_str).unwrap_or("string");

            eprint!("  {}", title);
            if let Some(desc) = description {
                eprint!(" ({})", desc);
            }
            if is_required {
                eprint!(" [required]");
            }
            if let Some(default) = default_value {
                eprint!(" [default: {}]", format_default(default));
            }
            if let Some(options) = prop.get("enum").and_then(Value::as_array) {
                let labels: Vec<String> = options.iter().map(format_default).collect();
                eprint!(" [options: {}]", labels.join(", "));
            }
            eprint!(": ");
            io::stderr().flush().ok();

            let mut line = String::new();
            stdin_lock
                .read_line(&mut line)
                .map_err(|error| anyhow!("failed to read elicitation input: {}", error))?;
            let raw = line.trim();

            if raw.is_empty() {
                if let Some(default) = default_value {
                    content.insert(key.clone(), default.clone());
                }
                continue;
            }

            let parsed = coerce_elicitation_value(raw, prop_type, prop);
            content.insert(key.clone(), parsed);
        }
    }

    eprintln!("--- end elicitation ---");

    Ok(JsonRpcResponse {
        jsonrpc: "2.0".to_owned(),
        id: request.id.clone(),
        result: Some(json!({
            "action": "accept",
            "content": content,
        })),
        error: None,
    })
}

// ---------------------------------------------------------------------------
// Sampling handler (`sampling/createMessage`)
// ---------------------------------------------------------------------------

/// Handles `sampling/createMessage` by showing the request to the user
/// and collecting a text response from the terminal (human-in-the-loop).
fn handle_sampling_request(request: &JsonRpcRequest, _app_id: &str) -> Result<JsonRpcResponse> {
    let params = request
        .params
        .as_ref()
        .ok_or_else(|| anyhow!("sampling/createMessage request is missing params"))?;

    let messages = params.get("messages").and_then(Value::as_array);
    let system_prompt = params.get("systemPrompt").and_then(Value::as_str);
    let max_tokens = params.get("maxTokens").and_then(Value::as_u64);
    let model_hints = params
        .get("modelPreferences")
        .and_then(|p| p.get("hints"))
        .and_then(Value::as_array);

    eprintln!("--- sampling request ---");
    eprintln!("The server requests a model response.");

    if let Some(hints) = model_hints {
        let names: Vec<&str> = hints
            .iter()
            .filter_map(|h| h.get("name").and_then(Value::as_str))
            .collect();
        if !names.is_empty() {
            eprintln!("Model hint: {}", names.join(", "));
        }
    }
    if let Some(prompt) = system_prompt {
        eprintln!("System: {}", prompt);
    }
    if let Some(tokens) = max_tokens {
        eprintln!("Max tokens: {}", tokens);
    }

    if let Some(msgs) = messages {
        eprintln!();
        eprintln!("Messages:");
        for msg in msgs {
            let role = msg.get("role").and_then(Value::as_str).unwrap_or("unknown");
            let content = msg.get("content");
            let text = match content {
                Some(Value::Object(obj)) => obj
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("(non-text content)"),
                Some(Value::String(s)) => s.as_str(),
                _ => "(no content)",
            };
            eprintln!("  [{}] {}", role, text);
        }
    }

    // Display available tools if the server provides them (SEP-1577)
    if let Some(tools) = params.get("tools").and_then(Value::as_array)
        && !tools.is_empty()
    {
        eprintln!();
        eprintln!("Available tools:");
        for tool in tools {
            let name = tool.get("name").and_then(Value::as_str).unwrap_or("?");
            let desc = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("");
            eprintln!("  {} — {}", name, desc);
        }
    }
    if let Some(choice) = params.get("toolChoice") {
        let choice_str = serde_json::to_string(choice).unwrap_or_else(|_| "(invalid)".to_owned());
        eprintln!("Tool choice: {}", choice_str);
    }

    eprintln!();
    eprint!("Your response (or 'decline' to reject): ");
    io::stderr().flush().ok();

    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let mut response_text = String::new();
    stdin_lock
        .read_line(&mut response_text)
        .map_err(|error| anyhow!("failed to read sampling response: {}", error))?;
    let response_text = response_text.trim();

    eprintln!("--- end sampling ---");

    if response_text.eq_ignore_ascii_case("decline") || response_text.is_empty() {
        return Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: request.id.clone(),
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: "user declined sampling request".to_owned(),
                data: None,
            }),
        });
    }

    Ok(JsonRpcResponse {
        jsonrpc: "2.0".to_owned(),
        id: request.id.clone(),
        result: Some(json!({
            "model": "human-in-the-loop",
            "role": "assistant",
            "content": {
                "type": "text",
                "text": response_text,
            }
        })),
        error: None,
    })
}

// ---------------------------------------------------------------------------
// URL-mode elicitation handler
// ---------------------------------------------------------------------------

/// URL-mode elicitation: direct the user to an external URL.
///
/// Per MCP 2025-11-25 spec:
/// - MUST show the full URL to the user.
/// - MUST NOT automatically pre-fetch the URL.
/// - MUST NOT open the URL without explicit consent.
fn handle_url_elicitation(request: &JsonRpcRequest, params: &Value) -> Result<JsonRpcResponse> {
    let message = params
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("The server is requesting you to visit a URL:");
    let url = params
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("url mode elicitation missing 'url' field"))?;

    eprintln!("--- url elicitation request ---");
    eprintln!("{}", message);
    eprintln!();
    eprintln!("The server asks you to open this URL:");
    eprintln!("  {}", url);
    eprintln!();
    eprint!("Open in browser? [Y/n/cancel]: ");
    io::stderr().flush().ok();

    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let mut line = String::new();
    stdin_lock
        .read_line(&mut line)
        .map_err(|error| anyhow!("failed to read elicitation input: {}", error))?;
    let choice = line.trim().to_ascii_lowercase();

    let action = match choice.as_str() {
        "" | "y" | "yes" => {
            // Attempt to open browser — best-effort, platform-dependent
            let open_result = open_url_in_browser(url);
            if let Err(e) = open_result {
                eprintln!(
                    "Could not open browser: {}. Please open the URL manually.",
                    e
                );
            } else {
                eprintln!("Browser launched.");
            }
            eprintln!();
            eprint!("Press Enter after completing the interaction... ");
            io::stderr().flush().ok();
            let mut wait = String::new();
            stdin_lock.read_line(&mut wait).ok();
            "accept"
        }
        "n" | "no" => "decline",
        _ => "cancel",
    };

    eprintln!("--- end url elicitation ---");

    Ok(JsonRpcResponse {
        jsonrpc: "2.0".to_owned(),
        id: request.id.clone(),
        result: Some(json!({
            "action": action,
        })),
        error: None,
    })
}

/// Best-effort platform browser opener.
fn open_url_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(target_os = "windows")]
    let cmd = "start";
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    return Err(anyhow!("unsupported platform for browser opening"));

    std::process::Command::new(cmd)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| anyhow!("failed to launch browser ({}): {}", cmd, e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn format_default(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// Coerce raw user input to the appropriate JSON type based on JSON Schema.
pub fn coerce_elicitation_value(raw: &str, prop_type: &str, prop: &Value) -> Value {
    match prop_type {
        "boolean" => match raw.to_ascii_lowercase().as_str() {
            "true" | "yes" | "y" | "1" => Value::Bool(true),
            _ => Value::Bool(false),
        },
        "integer" => raw
            .parse::<i64>()
            .map(|n| Value::Number(Number::from(n)))
            .unwrap_or_else(|_| Value::String(raw.to_owned())),
        "number" => raw
            .parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(raw.to_owned())),
        "array" => {
            let items: Vec<Value> = raw
                .split(',')
                .map(|s| {
                    let trimmed = s.trim();
                    if let Some(items_schema) = prop.get("items") {
                        if let Some(any_of) = items_schema.get("anyOf").and_then(Value::as_array)
                            && let Some(matched) = any_of.iter().find(|opt| {
                                opt.get("title")
                                    .and_then(Value::as_str)
                                    .map(|t| t.eq_ignore_ascii_case(trimmed))
                                    .unwrap_or(false)
                            })
                            && let Some(val) = matched.get("const")
                        {
                            return val.clone();
                        }
                        if let Some(enum_vals) = items_schema.get("enum").and_then(Value::as_array)
                            && let Some(matched) = enum_vals.iter().find(|v| {
                                v.as_str()
                                    .map(|s| s.eq_ignore_ascii_case(trimmed))
                                    .unwrap_or(false)
                            })
                        {
                            return matched.clone();
                        }
                    }
                    Value::String(trimmed.to_owned())
                })
                .collect();
            Value::Array(items)
        }
        _ => {
            // For enum strings with oneOf, try to match by title
            if let Some(one_of) = prop.get("oneOf").and_then(Value::as_array)
                && let Some(matched) = one_of.iter().find(|opt| {
                    opt.get("title")
                        .and_then(Value::as_str)
                        .map(|t| t.eq_ignore_ascii_case(raw))
                        .unwrap_or(false)
                })
                && let Some(val) = matched.get("const")
            {
                return val.clone();
            }
            Value::String(raw.to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::protocol::JsonRpcId;

    #[test]
    fn handle_unknown_request_returns_method_not_found() {
        let handler = OperationMessageHandler {
            app_id: "test".to_owned(),
            events: EventBroker::default(),
            inventory_stale_path: None,
            roots: Vec::new(),
        };
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_owned(),
            id: JsonRpcId::Number(1),
            method: "unknown/method".to_owned(),
            params: None,
        };
        let response = handler.handle_request(&request).unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[test]
    fn handle_progress_notification() {
        let memory = std::sync::Arc::new(crate::runtime::MemoryEventSink::default());
        let broker = EventBroker::new(vec![memory.clone()]);
        let handler = OperationMessageHandler {
            app_id: "test".to_owned(),
            events: broker,
            inventory_stale_path: None,
            roots: Vec::new(),
        };
        let params = json!({
            "progressToken": "tok-1",
            "progress": 5,
            "total": 10,
            "message": "Loading..."
        });
        handler.handle_notification("notifications/progress", Some(&params));
        let events = memory.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            RuntimeEvent::Progress {
                operation,
                current,
                total,
                message,
                ..
            } => {
                assert_eq!(operation, "tok-1");
                assert_eq!(*current, 5);
                assert_eq!(*total, Some(10));
                assert_eq!(message, "Loading...");
            }
            other => panic!("expected Progress, got {:?}", other),
        }
    }

    #[test]
    fn handle_server_log_notification() {
        let memory = std::sync::Arc::new(crate::runtime::MemoryEventSink::default());
        let broker = EventBroker::new(vec![memory.clone()]);
        let handler = OperationMessageHandler {
            app_id: "test".to_owned(),
            events: broker,
            inventory_stale_path: None,
            roots: Vec::new(),
        };
        let params = json!({
            "level": "info",
            "logger": "db",
            "data": "Connection pool created"
        });
        handler.handle_notification("notifications/message", Some(&params));
        let events = memory.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            RuntimeEvent::ServerLog {
                level,
                logger,
                message,
                ..
            } => {
                assert_eq!(level, "info");
                assert_eq!(logger, "db");
                assert_eq!(message, "Connection pool created");
            }
            other => panic!("expected ServerLog, got {:?}", other),
        }
    }

    #[test]
    fn handle_list_changed_writes_stale_marker() {
        let dir = tempfile::tempdir().unwrap();
        let stale_path = dir.path().join("inventory.stale");
        let handler = OperationMessageHandler {
            app_id: "test".to_owned(),
            events: EventBroker::default(),
            inventory_stale_path: Some(stale_path.clone()),
            roots: Vec::new(),
        };
        handler.handle_notification("notifications/tools/list_changed", None);
        assert!(stale_path.exists());
    }

    #[test]
    fn coerce_boolean_values() {
        let prop = json!({"type": "boolean"});
        assert_eq!(
            coerce_elicitation_value("yes", "boolean", &prop),
            Value::Bool(true)
        );
        assert_eq!(
            coerce_elicitation_value("no", "boolean", &prop),
            Value::Bool(false)
        );
    }

    #[test]
    fn coerce_integer_values() {
        let prop = json!({"type": "integer"});
        assert_eq!(
            coerce_elicitation_value("42", "integer", &prop),
            Value::Number(Number::from(42))
        );
        assert_eq!(
            coerce_elicitation_value("abc", "integer", &prop),
            Value::String("abc".to_owned())
        );
    }
}
