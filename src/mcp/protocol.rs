//! MCP JSON-RPC protocol engine.
//!
//! This is the deepest protocol-aware module in the crate. It owns:
//!
//! - **JSON-RPC 2.0 framing** — [`JsonRpcRequest`], [`JsonRpcResponse`],
//!   [`JsonRpcError`], [`JsonRpcId`].
//! - **The `initialize` state machine** — [`ProtocolEngine`] tracks
//!   protocol version, client/server capabilities, and whether the
//!   `initialized` notification has been sent. Transports call
//!   `initialize` / `complete_initialize` once per session before any
//!   other request flows.
//! - **Operation → request mapping** — `prepare_request` turns a
//!   transport-neutral [`McpOperation`] into a
//!   [`PreparedProtocolRequest`] ready for the wire. This is where
//!   method names get picked (`tools/call`, `resources/read`,
//!   `prompts/get`, `completion/complete`, `logging/setLevel`,
//!   `resources/subscribe`, `resources/unsubscribe`, `tasks/get`,
//!   `tasks/cancel`, `tasks/result`, `ping`, and the various `*/list`
//!   discovery methods).
//! - **Progress-token injection** — operations that support
//!   long-running progress (`tools/call`, `resources/read`,
//!   `prompts/get`, and background tasks) have a unique progress token
//!   attached in `_meta.progressToken`. The matching
//!   `notifications/progress` stream is correlated back to the
//!   operation by [`crate::mcp::handler`].
//! - **Background-job augmentation** — when a caller passes
//!   `background=true`, the engine sets `_meta.task` on `tools/call`
//!   so the server creates a task and returns a `task_id` the client
//!   can later poll with `tasks/get` / `tasks/result` and cancel with
//!   `tasks/cancel` (MCP 2025-11-25 tasks extension).
//!
//! Response decoding lives alongside the request builders — each
//! operation has a corresponding `decode_*` helper that turns the
//! raw [`serde_json::Value`] back into an
//! [`crate::mcp::model::McpOperationResult`].

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};

use crate::mcp::model::{DiscoveryCategory, McpOperation};

pub const DEFAULT_MCP_PROTOCOL_VERSION: &str = "2025-11-25";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum JsonRpcId {
    Number(u64),
    String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: JsonRpcId, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn into_result<T: DeserializeOwned>(self) -> Result<T> {
        if let Some(error) = self.error {
            return Err(anyhow!("json-rpc error {}: {}", error.code, error.message));
        }

        let value = self
            .result
            .ok_or_else(|| anyhow!("json-rpc response did not contain a result"))?;
        serde_json::from_value(value)
            .map_err(|error| anyhow!("failed to decode json-rpc result: {}", error))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CapabilityMarker {}

/// Elicitation capability with supported modes (2025-11-25+).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ElicitationCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<CapabilityMarker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<CapabilityMarker>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<CapabilityMarker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<CapabilityMarker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<ElicitationCapability>,
    #[serde(skip_serializing_if = "Map::is_empty", default)]
    pub experimental: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListCapability {
    #[serde(default, skip_serializing_if = "is_false")]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResourceCapability {
    #[serde(default, skip_serializing_if = "is_false")]
    pub list_changed: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub subscribe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<CapabilityMarker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completions: Option<CapabilityMarker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<ListCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ListCapability>,
    /// Task support (MCP 2025-11-25 experimental).
    /// Flexible Value to handle nested structure without strict typing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks: Option<Value>,
    #[serde(skip_serializing_if = "Map::is_empty", default)]
    pub experimental: Map<String, Value>,
}

impl ServerCapabilities {
    /// Check if the server supports task-augmented tool calls.
    pub fn supports_tool_tasks(&self) -> bool {
        self.tasks
            .as_ref()
            .and_then(|t| t.get("requests"))
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.get("call"))
            .is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: PeerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: PeerInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpClientSession {
    pub protocol_version: String,
    pub session_id: Option<String>,
    pub initialized: bool,
    pub server_capabilities: Option<ServerCapabilities>,
    pub server_info: Option<PeerInfo>,
}

impl McpClientSession {
    pub fn new(protocol_version: impl Into<String>) -> Self {
        Self {
            protocol_version: protocol_version.into(),
            session_id: None,
            initialized: false,
            server_capabilities: None,
            server_info: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedProtocolRequest {
    pub initialize: Option<JsonRpcRequest>,
    pub initialized_notification: Option<JsonRpcNotification>,
    pub request: JsonRpcRequest,
}

impl PreparedProtocolRequest {
    pub fn outbound_message_count(&self) -> usize {
        usize::from(self.initialize.is_some())
            + usize::from(self.initialized_notification.is_some())
            + 1
    }
}

#[derive(Debug, Clone)]
pub struct ProtocolEngine {
    protocol_version: String,
    client_info: PeerInfo,
    client_capabilities: ClientCapabilities,
}

impl ProtocolEngine {
    pub fn new(
        protocol_version: impl Into<String>,
        client_name: impl Into<String>,
        client_version: impl Into<String>,
    ) -> Self {
        Self {
            protocol_version: protocol_version.into(),
            client_info: PeerInfo {
                name: client_name.into(),
                version: client_version.into(),
            },
            client_capabilities: ClientCapabilities {
                roots: Some(CapabilityMarker {}),
                elicitation: Some(ElicitationCapability {
                    form: Some(CapabilityMarker {}),
                    url: Some(CapabilityMarker {}),
                }),
                sampling: Some(CapabilityMarker {}),
                ..ClientCapabilities::default()
            },
        }
    }

    pub fn initial_session(&self) -> McpClientSession {
        McpClientSession::new(self.protocol_version.clone())
    }

    pub fn initialize_request(&self, request_id: u64) -> JsonRpcRequest {
        JsonRpcRequest::new(
            JsonRpcId::Number(request_id),
            "initialize",
            Some(json!(InitializeParams {
                protocol_version: self.protocol_version.clone(),
                capabilities: self.client_capabilities.clone(),
                client_info: self.client_info.clone(),
            })),
        )
    }

    pub fn initialized_notification(&self) -> JsonRpcNotification {
        JsonRpcNotification::new("notifications/initialized", None)
    }

    pub fn complete_initialize(
        &self,
        session: &mut McpClientSession,
        result: InitializeResult,
        session_id: Option<String>,
    ) {
        session.protocol_version = result.protocol_version;
        session.session_id = session_id;
        session.server_capabilities = Some(result.capabilities);
        session.server_info = Some(result.server_info);
        session.initialized = true;
    }

    pub fn prepare_operation(
        &self,
        session: &McpClientSession,
        request_id: u64,
        operation: &McpOperation,
    ) -> Result<PreparedProtocolRequest> {
        let actual_id = request_id + u64::from(!session.initialized);
        let mut request = map_operation_to_request(actual_id, operation)?;

        // Inject _meta.progressToken so the server can send targeted progress
        // notifications for this specific request.
        inject_progress_token(&mut request);

        if session.initialized {
            return Ok(PreparedProtocolRequest {
                initialize: None,
                initialized_notification: None,
                request,
            });
        }

        Ok(PreparedProtocolRequest {
            initialize: Some(self.initialize_request(request_id)),
            initialized_notification: Some(self.initialized_notification()),
            request,
        })
    }
}

/// Inject `_meta.progressToken` into the request params so the server
/// can send targeted `notifications/progress` for this request.
fn inject_progress_token(request: &mut JsonRpcRequest) {
    // Only inject for methods that perform work (not discovery/ping/etc.)
    let needs_progress = matches!(
        request.method.as_str(),
        "tools/call" | "prompts/get" | "resources/read" | "tasks/get" | "tasks/result"
    );
    if !needs_progress {
        return;
    }
    let token = match &request.id {
        JsonRpcId::Number(n) => format!("mcp2cli-{}", n),
        JsonRpcId::String(s) => format!("mcp2cli-{}", s),
    };
    let params = request.params.get_or_insert_with(|| json!({}));
    if let Some(obj) = params.as_object_mut() {
        let meta = obj.entry("_meta").or_insert_with(|| json!({}));
        if let Some(meta_obj) = meta.as_object_mut() {
            meta_obj.insert("progressToken".to_owned(), json!(token));
        }
    }
}

fn map_operation_to_request(request_id: u64, operation: &McpOperation) -> Result<JsonRpcRequest> {
    let id = JsonRpcId::Number(request_id);
    match operation {
        McpOperation::Discover { category } => Ok(JsonRpcRequest::new(
            id,
            discover_method_name(category),
            None,
        )),
        McpOperation::InvokeAction {
            capability,
            arguments,
            background,
        } => {
            let mut params = json!({
                "name": capability,
                "arguments": arguments,
            });
            // When background is true, request task augmentation so the
            // server returns a task ID immediately instead of blocking.
            if *background {
                let meta = params
                    .as_object_mut()
                    .unwrap()
                    .entry("_meta")
                    .or_insert_with(|| json!({}));
                if let Some(meta_obj) = meta.as_object_mut() {
                    meta_obj.insert("task".to_owned(), json!({}));
                }
            }
            Ok(JsonRpcRequest::new(id, "tools/call", Some(params)))
        }
        McpOperation::ReadResource { uri } => Ok(JsonRpcRequest::new(
            id,
            "resources/read",
            Some(json!({ "uri": uri })),
        )),
        McpOperation::RunPrompt { name, arguments } => {
            let prompt_arguments = flatten_prompt_arguments(arguments)?;
            let params = if prompt_arguments.is_empty() {
                json!({ "name": name })
            } else {
                json!({
                    "name": name,
                    "arguments": prompt_arguments,
                })
            };
            Ok(JsonRpcRequest::new(id, "prompts/get", Some(params)))
        }
        McpOperation::Ping => Ok(JsonRpcRequest::new(id, "ping", None)),
        McpOperation::SetLoggingLevel { level } => Ok(JsonRpcRequest::new(
            id,
            "logging/setLevel",
            Some(json!({ "level": level })),
        )),
        McpOperation::Complete {
            ref_kind,
            ref_name,
            argument_name,
            argument_value,
            context,
        } => {
            let mut params = json!({
                "ref": {
                    "type": ref_kind,
                    "name": ref_name,
                },
                "argument": {
                    "name": argument_name,
                    "value": argument_value,
                }
            });
            if let Some(ctx) = context
                && !ctx.is_empty()
            {
                params["context"] = Value::Object(ctx.clone());
            }
            Ok(JsonRpcRequest::new(id, "completion/complete", Some(params)))
        }
        McpOperation::SubscribeResource { uri } => Ok(JsonRpcRequest::new(
            id,
            "resources/subscribe",
            Some(json!({ "uri": uri })),
        )),
        McpOperation::UnsubscribeResource { uri } => Ok(JsonRpcRequest::new(
            id,
            "resources/unsubscribe",
            Some(json!({ "uri": uri })),
        )),
        McpOperation::TaskGet { task_id } => Ok(JsonRpcRequest::new(
            id,
            "tasks/get",
            Some(json!({ "taskId": task_id })),
        )),
        McpOperation::TaskResult { task_id } => Ok(JsonRpcRequest::new(
            id,
            "tasks/result",
            Some(json!({ "taskId": task_id })),
        )),
        McpOperation::TaskCancel { task_id } => Ok(JsonRpcRequest::new(
            id,
            "tasks/cancel",
            Some(json!({ "taskId": task_id })),
        )),
        McpOperation::DiscoverResourceTemplates => {
            Ok(JsonRpcRequest::new(id, "resources/templates/list", None))
        }
    }
}

fn discover_method_name(category: &DiscoveryCategory) -> &'static str {
    match category {
        DiscoveryCategory::Capabilities => "tools/list",
        DiscoveryCategory::Resources => "resources/list",
        DiscoveryCategory::Prompts => "prompts/list",
    }
}

fn flatten_prompt_arguments(arguments: &Value) -> Result<Map<String, Value>> {
    let Some(object) = arguments.as_object() else {
        return Err(anyhow!("prompt arguments must be a JSON object"));
    };

    let mut flattened = Map::new();
    for (key, value) in object {
        flatten_prompt_argument_value(&mut flattened, key, value)?;
    }
    Ok(flattened)
}

fn flatten_prompt_argument_value(
    output: &mut Map<String, Value>,
    prefix: &str,
    value: &Value,
) -> Result<()> {
    match value {
        Value::Object(object) => {
            if object.is_empty() {
                output.insert(prefix.to_owned(), Value::String("{}".to_owned()));
                return Ok(());
            }
            for (key, nested) in object {
                let next_prefix = format!("{}.{}", prefix, key);
                flatten_prompt_argument_value(output, &next_prefix, nested)?;
            }
            Ok(())
        }
        Value::String(raw) => {
            output.insert(prefix.to_owned(), Value::String(raw.clone()));
            Ok(())
        }
        Value::Null => {
            output.insert(prefix.to_owned(), Value::String("null".to_owned()));
            Ok(())
        }
        Value::Bool(raw) => {
            output.insert(prefix.to_owned(), Value::String(raw.to_string()));
            Ok(())
        }
        Value::Number(raw) => {
            output.insert(prefix.to_owned(), Value::String(raw.to_string()));
            Ok(())
        }
        Value::Array(_) => {
            output.insert(
                prefix.to_owned(),
                Value::String(serde_json::to_string(value)?),
            );
            Ok(())
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::model::McpOperation;

    #[test]
    fn prepare_discover_includes_initialize_when_session_is_fresh() {
        let engine = ProtocolEngine::new("test-version", "mcp2cli", "0.1.0");
        let session = engine.initial_session();

        let prepared = engine
            .prepare_operation(
                &session,
                1,
                &McpOperation::Discover {
                    category: DiscoveryCategory::Capabilities,
                },
            )
            .expect("plan should build");

        assert_eq!(
            prepared
                .initialize
                .as_ref()
                .map(|value| value.method.as_str()),
            Some("initialize")
        );
        assert_eq!(
            prepared
                .initialized_notification
                .as_ref()
                .map(|value| value.method.as_str()),
            Some("notifications/initialized")
        );
        assert_eq!(prepared.request.method, "tools/list");
        assert_eq!(prepared.outbound_message_count(), 3);
    }

    #[test]
    fn prepare_invoke_for_initialized_session_skips_bootstrap() {
        let engine = ProtocolEngine::new(DEFAULT_MCP_PROTOCOL_VERSION, "mcp2cli", "0.1.0");
        let mut session = engine.initial_session();
        session.initialized = true;

        let prepared = engine
            .prepare_operation(
                &session,
                7,
                &McpOperation::InvokeAction {
                    capability: "tools.echo".to_owned(),
                    arguments: json!({ "message": "hello" }),
                    background: false,
                },
            )
            .expect("plan should build");

        assert!(prepared.initialize.is_none());
        assert!(prepared.initialized_notification.is_none());
        assert_eq!(prepared.request.method, "tools/call");
        // tools/call params now include _meta.progressToken injected by the engine
        let params = prepared.request.params.unwrap();
        assert_eq!(params["name"], json!("tools.echo"));
        assert_eq!(params["arguments"], json!({ "message": "hello" }));
        assert!(params["_meta"]["progressToken"].is_string());
    }

    #[test]
    fn prompt_arguments_are_flattened_to_string_values() {
        let prepared = map_operation_to_request(
            9,
            &McpOperation::RunPrompt {
                name: "drafts.reply".to_owned(),
                arguments: json!({
                    "context": {
                        "thread_id": 123,
                        "labels": ["important"]
                    },
                    "tone": "formal"
                }),
            },
        )
        .expect("prompt should map");

        assert_eq!(prepared.method, "prompts/get");
        assert_eq!(
            prepared.params,
            Some(json!({
                "name": "drafts.reply",
                "arguments": {
                    "context.thread_id": "123",
                    "context.labels": "[\"important\"]",
                    "tone": "formal"
                }
            }))
        );
    }

    #[test]
    fn initialize_response_can_update_session_state() {
        let engine = ProtocolEngine::new(DEFAULT_MCP_PROTOCOL_VERSION, "mcp2cli", "0.1.0");
        let mut session = engine.initial_session();

        engine.complete_initialize(
            &mut session,
            InitializeResult {
                protocol_version: DEFAULT_MCP_PROTOCOL_VERSION.to_owned(),
                capabilities: ServerCapabilities {
                    tools: Some(ListCapability::default()),
                    ..ServerCapabilities::default()
                },
                server_info: PeerInfo {
                    name: "demo-server".to_owned(),
                    version: "1.0.0".to_owned(),
                },
                instructions: None,
            },
            Some("session-123".to_owned()),
        );

        assert!(session.initialized);
        assert_eq!(session.session_id.as_deref(), Some("session-123"));
        assert_eq!(
            session
                .server_info
                .as_ref()
                .map(|value| value.name.as_str()),
            Some("demo-server")
        );
    }
}
