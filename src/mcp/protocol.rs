//! MCP JSON-RPC protocol engine.
//!
//! This is the deepest protocol-aware module in the crate. It owns:
//!
//! - **JSON-RPC 2.0 framing** — [`JsonRpcRequest`], [`JsonRpcResponse`],
//!   [`JsonRpcError`], [`JsonRpcId`].
//! - **The protocol era model** — mcp2cli speaks two MCP revisions:
//!   - **Legacy** (`2025-11-25`): session-oriented; every connection
//!     starts with the `initialize` / `notifications/initialized`
//!     handshake and (on HTTP) an `Mcp-Session-Id` header.
//!   - **Modern** (`2026-07-28`): stateless; there is no handshake.
//!     Every request carries its protocol version, client identity and
//!     client capabilities in `_meta`
//!     (`io.modelcontextprotocol/protocolVersion`, `…/clientInfo`,
//!     `…/clientCapabilities`), and servers implement `server/discover`
//!     ([SEP-2575](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2575)).
//!
//!   [`VersionPolicy`] selects the era: `auto` (default) probes with
//!   `server/discover` and falls back to the legacy handshake exactly as
//!   the 2026-07-28 backward-compatibility rules describe; pinning a
//!   version skips the probe.
//! - **Operation → request mapping** — `prepare_operation` turns a
//!   transport-neutral [`McpOperation`] into a
//!   [`PreparedProtocolRequest`] ready for the wire, choosing
//!   era-appropriate method names (`ping` vs `server/discover`,
//!   `resources/subscribe` vs `subscriptions/listen`, `tasks/result`
//!   vs polled `tasks/get`, …).
//! - **Progress-token injection** — operations that support
//!   long-running progress have a unique progress token attached in
//!   `_meta.progressToken`; the matching `notifications/progress`
//!   stream is correlated back by [`crate::mcp::handler`].
//! - **Background-job augmentation** — legacy servers get `_meta.task`
//!   on `tools/call` (MCP 2025-11-25 experimental tasks); modern
//!   servers are offered the `io.modelcontextprotocol/tasks` extension
//!   via `clientCapabilities.extensions` and decide per request whether
//!   to return a task handle ([SEP-2663](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2663)).
//! - **Multi Round-Trip Requests (MRTR)** — helpers for recognising
//!   `resultType: "input_required"` results, collecting the requested
//!   `inputResponses`, and rebuilding the retry request with an echoed
//!   `requestState` ([SEP-2322](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2322)).
//!
//! Response decoding lives alongside the request builders — each
//! operation has a corresponding `decode_*` helper that turns the
//! raw [`serde_json::Value`] back into an
//! [`crate::mcp::model::McpOperationResult`].

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};

use crate::mcp::model::{DiscoveryCategory, McpOperation, McpOperationResult};

/// Protocol version used for the legacy `initialize` handshake.
pub const DEFAULT_MCP_PROTOCOL_VERSION: &str = "2025-11-25";
/// Latest session-oriented (handshake) protocol revision.
pub const MCP_PROTOCOL_VERSION_LEGACY: &str = "2025-11-25";
/// Stateless per-request-metadata protocol revision (2026-07-28 RC).
pub const MCP_PROTOCOL_VERSION_MODERN: &str = "2026-07-28";
/// Modern protocol versions this client can speak, in preference order.
pub const SUPPORTED_MODERN_PROTOCOL_VERSIONS: &[&str] = &[MCP_PROTOCOL_VERSION_MODERN];

/// `_meta` key carrying the protocol version on every modern request.
pub const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
/// `_meta` key carrying the client identity on every modern request.
pub const META_CLIENT_INFO: &str = "io.modelcontextprotocol/clientInfo";
/// `_meta` key carrying the client capabilities on every modern request.
pub const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
/// `_meta` key requesting server log messages for a modern request.
pub const META_LOG_LEVEL: &str = "io.modelcontextprotocol/logLevel";
/// `_meta` key correlating notifications with a `subscriptions/listen` stream.
pub const META_SUBSCRIPTION_ID: &str = "io.modelcontextprotocol/subscriptionId";
/// Extension identifier for the MCP Tasks extension.
pub const TASKS_EXTENSION_ID: &str = "io.modelcontextprotocol/tasks";

/// MCP 2026-07-28 protocol error: HTTP headers do not match the body.
pub const ERROR_HEADER_MISMATCH: i64 = -32020;
/// MCP 2026-07-28 protocol error: a required client capability is missing.
pub const ERROR_MISSING_REQUIRED_CLIENT_CAPABILITY: i64 = -32021;
/// MCP 2026-07-28 protocol error: the requested protocol version is unsupported.
pub const ERROR_UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;
/// Pre-RC draft code for `UnsupportedProtocolVersionError`, accepted defensively.
const ERROR_UNSUPPORTED_PROTOCOL_VERSION_PRE_RC: i64 = -32004;

/// Returns true when the error code identifies a *modern* (2026-07-28+)
/// server. Used during era detection: a recognised modern error means the
/// peer speaks the stateless protocol and the client must not fall back to
/// the legacy `initialize` handshake.
pub fn is_modern_protocol_error(code: i64) -> bool {
    matches!(
        code,
        ERROR_HEADER_MISMATCH
            | ERROR_MISSING_REQUIRED_CLIENT_CAPABILITY
            | ERROR_UNSUPPORTED_PROTOCOL_VERSION
            | ERROR_UNSUPPORTED_PROTOCOL_VERSION_PRE_RC
    )
}

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
    /// Optional protocol extensions (MCP 2026-07-28), keyed by extension
    /// identifier (e.g. `io.modelcontextprotocol/tasks`).
    #[serde(skip_serializing_if = "Map::is_empty", default)]
    pub extensions: Map<String, Value>,
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
    /// Optional protocol extensions (MCP 2026-07-28), keyed by extension
    /// identifier (e.g. `io.modelcontextprotocol/tasks`).
    #[serde(skip_serializing_if = "Map::is_empty", default)]
    pub extensions: Map<String, Value>,
    #[serde(skip_serializing_if = "Map::is_empty", default)]
    pub experimental: Map<String, Value>,
}

impl ServerCapabilities {
    /// Check if the server supports task-augmented tool calls
    /// (MCP 2025-11-25 experimental core tasks).
    pub fn supports_tool_tasks(&self) -> bool {
        self.tasks
            .as_ref()
            .and_then(|t| t.get("requests"))
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.get("call"))
            .is_some()
    }

    /// Check if the server advertises the MCP Tasks extension
    /// (`io.modelcontextprotocol/tasks`, MCP 2026-07-28).
    pub fn supports_tasks_extension(&self) -> bool {
        self.extensions.contains_key(TASKS_EXTENSION_ID)
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

/// Result of the modern `server/discover` request (MCP 2026-07-28).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverResult {
    pub supported_versions: Vec<String>,
    #[serde(default)]
    pub capabilities: ServerCapabilities,
    pub server_info: PeerInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// Which shape of the protocol the negotiated session speaks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolEra {
    /// `2025-11-25` and earlier: `initialize` handshake, sessions.
    #[default]
    Legacy,
    /// `2026-07-28` and later: stateless, per-request `_meta`.
    Modern,
}

impl ProtocolEra {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Modern => "modern",
        }
    }
}

/// How the client chooses a protocol era for a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VersionPolicy {
    /// Probe with `server/discover`; fall back to the legacy handshake
    /// when the server does not answer with a recognised modern
    /// response (the 2026-07-28 backward-compatibility algorithm).
    #[default]
    Auto,
    /// Speak `2025-11-25` (initialize handshake) without probing.
    PinLegacy,
    /// Speak `2026-07-28` (stateless); never fall back to the handshake.
    PinModern,
}

impl VersionPolicy {
    /// Parse the `server.protocol_version` config value.
    pub fn parse(raw: Option<&str>) -> Result<Self> {
        match raw.map(str::trim) {
            None | Some("") | Some("auto") => Ok(Self::Auto),
            Some(MCP_PROTOCOL_VERSION_LEGACY) => Ok(Self::PinLegacy),
            Some(MCP_PROTOCOL_VERSION_MODERN) => Ok(Self::PinModern),
            Some(other) => Err(anyhow!(
                "unsupported server.protocol_version '{}'; expected 'auto', '{}' or '{}'",
                other,
                MCP_PROTOCOL_VERSION_LEGACY,
                MCP_PROTOCOL_VERSION_MODERN,
            )),
        }
    }

    /// Whether the bootstrap should start with a `server/discover` probe.
    pub fn probe_first(&self) -> bool {
        !matches!(self, Self::PinLegacy)
    }

    /// Whether falling back to the legacy handshake is permitted.
    pub fn allows_legacy_fallback(&self) -> bool {
        !matches!(self, Self::PinModern)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpClientSession {
    pub protocol_version: String,
    #[serde(default)]
    pub era: ProtocolEra,
    pub session_id: Option<String>,
    pub initialized: bool,
    pub server_capabilities: Option<ServerCapabilities>,
    pub server_info: Option<PeerInfo>,
}

impl McpClientSession {
    pub fn new(protocol_version: impl Into<String>) -> Self {
        Self {
            protocol_version: protocol_version.into(),
            era: ProtocolEra::Legacy,
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

/// Outcome of classifying a `server/discover` probe response.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeOutcome {
    /// The server is modern; a mutually supported version was selected.
    Modern {
        negotiated_version: String,
        discover: DiscoverResult,
    },
    /// The server is modern but none of our modern versions matched;
    /// `supported` lists the versions the server advertised.
    ModernUnsupported { supported: Vec<String> },
    /// The server did not answer with a recognised modern response —
    /// treat it as a legacy (initialize-handshake) server.
    Legacy { detail: String },
}

/// Classify a `server/discover` probe response per the MCP 2026-07-28
/// backward-compatibility rules: a `DiscoverResult` or a recognised modern
/// JSON-RPC error identifies a modern server; anything else identifies a
/// legacy server. The fallback is deliberately not keyed to one specific
/// error code — legacy servers answer unknown pre-`initialize` requests
/// with implementation-defined errors.
pub fn classify_probe_response(response: &JsonRpcResponse) -> ProbeOutcome {
    if let Some(error) = &response.error {
        if is_modern_protocol_error(error.code) {
            let supported = error
                .data
                .as_ref()
                .and_then(|data| data.get("supported"))
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            return ProbeOutcome::ModernUnsupported { supported };
        }
        return ProbeOutcome::Legacy {
            detail: format!(
                "server/discover returned non-modern error {}: {}",
                error.code, error.message
            ),
        };
    }

    let Some(result) = &response.result else {
        return ProbeOutcome::Legacy {
            detail: "server/discover response carried neither result nor error".to_owned(),
        };
    };

    match serde_json::from_value::<DiscoverResult>(result.clone()) {
        Ok(discover) => match select_modern_version(&discover.supported_versions) {
            Some(version) => ProbeOutcome::Modern {
                negotiated_version: version,
                discover,
            },
            None => ProbeOutcome::ModernUnsupported {
                supported: discover.supported_versions,
            },
        },
        Err(error) => ProbeOutcome::Legacy {
            detail: format!("server/discover result was not a DiscoverResult: {}", error),
        },
    }
}

/// Pick the first modern protocol version we support from a server's
/// advertised list.
pub fn select_modern_version(supported: &[String]) -> Option<String> {
    SUPPORTED_MODERN_PROTOCOL_VERSIONS
        .iter()
        .find(|candidate| supported.iter().any(|offered| offered == *candidate))
        .map(|candidate| (*candidate).to_owned())
}

#[derive(Debug, Clone)]
pub struct ProtocolEngine {
    protocol_version: String,
    modern_version: String,
    policy: VersionPolicy,
    client_info: PeerInfo,
    client_capabilities: ClientCapabilities,
    /// Log level injected as `_meta[io.modelcontextprotocol/logLevel]`
    /// on modern requests (replaces `logging/setLevel`).
    log_level: Option<String>,
}

impl ProtocolEngine {
    pub fn new(
        protocol_version: impl Into<String>,
        client_name: impl Into<String>,
        client_version: impl Into<String>,
    ) -> Self {
        Self {
            protocol_version: protocol_version.into(),
            modern_version: MCP_PROTOCOL_VERSION_MODERN.to_owned(),
            policy: VersionPolicy::Auto,
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
            log_level: None,
        }
    }

    pub fn with_policy(mut self, policy: VersionPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_log_level(mut self, log_level: Option<String>) -> Self {
        self.log_level = log_level;
        self
    }

    pub fn policy(&self) -> VersionPolicy {
        self.policy
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
        session.era = ProtocolEra::Legacy;
        session.session_id = session_id;
        session.server_capabilities = Some(result.capabilities);
        session.server_info = Some(result.server_info);
        session.initialized = true;
    }

    /// Build the modern `server/discover` probe request. `version`
    /// defaults to the preferred modern version; pass an explicit value
    /// when retrying after an `UnsupportedProtocolVersionError`.
    pub fn probe_request(&self, request_id: u64, version: Option<&str>) -> JsonRpcRequest {
        let version = version.unwrap_or(&self.modern_version);
        let mut request = JsonRpcRequest::new(
            JsonRpcId::Number(request_id),
            "server/discover",
            Some(json!({})),
        );
        self.inject_modern_meta_with_version(&mut request, version);
        request
    }

    /// Record a successful `server/discover` outcome on the session.
    pub fn complete_discover(
        &self,
        session: &mut McpClientSession,
        negotiated_version: String,
        discover: DiscoverResult,
    ) {
        session.protocol_version = negotiated_version;
        session.era = ProtocolEra::Modern;
        session.session_id = None;
        session.server_capabilities = Some(discover.capabilities);
        session.server_info = Some(discover.server_info);
        session.initialized = true;
    }

    /// Client capabilities advertised per-request in modern `_meta`.
    /// Includes the MCP Tasks extension so servers may return task
    /// handles for long-running requests.
    fn modern_client_capabilities(&self) -> ClientCapabilities {
        let mut capabilities = self.client_capabilities.clone();
        capabilities
            .extensions
            .insert(TASKS_EXTENSION_ID.to_owned(), json!({}));
        capabilities
    }

    /// Inject the required modern `_meta` fields into a request.
    pub fn inject_modern_meta(&self, request: &mut JsonRpcRequest, session: &McpClientSession) {
        let version = session.protocol_version.clone();
        self.inject_modern_meta_with_version(request, &version);
    }

    fn inject_modern_meta_with_version(&self, request: &mut JsonRpcRequest, version: &str) {
        let params = request.params.get_or_insert_with(|| json!({}));
        let Some(obj) = params.as_object_mut() else {
            return;
        };
        let meta = obj.entry("_meta").or_insert_with(|| json!({}));
        let Some(meta_obj) = meta.as_object_mut() else {
            return;
        };
        meta_obj.insert(META_PROTOCOL_VERSION.to_owned(), json!(version));
        meta_obj.insert(META_CLIENT_INFO.to_owned(), json!(self.client_info));
        meta_obj.insert(
            META_CLIENT_CAPABILITIES.to_owned(),
            json!(self.modern_client_capabilities()),
        );
        if let Some(level) = &self.log_level {
            meta_obj.insert(META_LOG_LEVEL.to_owned(), json!(level));
        }
    }

    pub fn prepare_operation(
        &self,
        session: &McpClientSession,
        request_id: u64,
        operation: &McpOperation,
    ) -> Result<PreparedProtocolRequest> {
        if session.era == ProtocolEra::Modern {
            let mut request = map_operation_to_modern_request(request_id, operation)?;
            self.inject_modern_meta(&mut request, session);
            inject_progress_token(&mut request);
            return Ok(PreparedProtocolRequest {
                initialize: None,
                initialized_notification: None,
                request,
            });
        }

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

/// Operations that resolve without a wire request on modern servers.
///
/// - `logging/setLevel` was removed in 2026-07-28; the level travels
///   per-request in `_meta[io.modelcontextprotocol/logLevel]` instead.
/// - `resources/unsubscribe` was removed; a subscription ends when its
///   `subscriptions/listen` stream closes, so there is nothing to send.
pub fn modern_offline_result(operation: &McpOperation) -> Option<McpOperationResult> {
    match operation {
        McpOperation::SetLoggingLevel { level } => Some(McpOperationResult::LoggingLevelSet {
            message: format!(
                "log level '{}' stored; MCP 2026-07-28 servers receive it per-request via _meta['{}']",
                level, META_LOG_LEVEL
            ),
            level: level.clone(),
        }),
        McpOperation::UnsubscribeResource { uri } => Some(McpOperationResult::Unsubscribed {
            message: format!(
                "'{}' has no active listen stream; MCP 2026-07-28 subscriptions end when their subscriptions/listen stream closes",
                uri
            ),
            uri: uri.clone(),
        }),
        _ => None,
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
            ..
        } => {
            let mut params = json!({
                "name": capability,
                "arguments": arguments,
            });
            // When background is true, request task augmentation so the
            // server returns a task ID immediately instead of blocking.
            if *background && let Some(params_obj) = params.as_object_mut() {
                let meta = params_obj.entry("_meta").or_insert_with(|| json!({}));
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
        } => Ok(JsonRpcRequest::new(
            id,
            "completion/complete",
            Some(completion_params(
                ref_kind,
                ref_name,
                argument_name,
                argument_value,
                context.as_ref(),
            )),
        )),
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

/// Map an operation to its MCP 2026-07-28 request.
///
/// Differences from the legacy mapping:
/// - `ping` was removed — `server/discover` doubles as the liveness probe.
/// - `resources/subscribe` was replaced by `subscriptions/listen`.
/// - `tasks/result` (blocking) was removed — the tasks extension exposes
///   the final result on `tasks/get` once the task is terminal.
/// - background `tools/call` no longer sets `_meta.task`; the client
///   advertises the tasks extension and the server decides per request.
///
/// `SetLoggingLevel` and `UnsubscribeResource` never reach this function —
/// they resolve locally via [`modern_offline_result`].
fn map_operation_to_modern_request(
    request_id: u64,
    operation: &McpOperation,
) -> Result<JsonRpcRequest> {
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
            ..
        } => Ok(JsonRpcRequest::new(
            id,
            "tools/call",
            Some(json!({
                "name": capability,
                "arguments": arguments,
            })),
        )),
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
        McpOperation::Ping => Ok(JsonRpcRequest::new(id, "server/discover", None)),
        McpOperation::Complete {
            ref_kind,
            ref_name,
            argument_name,
            argument_value,
            context,
        } => Ok(JsonRpcRequest::new(
            id,
            "completion/complete",
            Some(completion_params(
                ref_kind,
                ref_name,
                argument_name,
                argument_value,
                context.as_ref(),
            )),
        )),
        McpOperation::SubscribeResource { uri } => Ok(JsonRpcRequest::new(
            id,
            "subscriptions/listen",
            Some(json!({
                "notifications": {
                    "resourceSubscriptions": [uri],
                }
            })),
        )),
        McpOperation::TaskGet { task_id } | McpOperation::TaskResult { task_id } => Ok(
            JsonRpcRequest::new(id, "tasks/get", Some(json!({ "taskId": task_id }))),
        ),
        McpOperation::TaskCancel { task_id } => Ok(JsonRpcRequest::new(
            id,
            "tasks/cancel",
            Some(json!({ "taskId": task_id })),
        )),
        McpOperation::DiscoverResourceTemplates => {
            Ok(JsonRpcRequest::new(id, "resources/templates/list", None))
        }
        McpOperation::SetLoggingLevel { .. } | McpOperation::UnsubscribeResource { .. } => {
            Err(anyhow!(
                "operation resolves locally on MCP 2026-07-28 servers; use modern_offline_result"
            ))
        }
    }
}

fn completion_params(
    ref_kind: &str,
    ref_name: &str,
    argument_name: &str,
    argument_value: &str,
    context: Option<&Map<String, Value>>,
) -> Value {
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
    params
}

fn discover_method_name(category: &DiscoveryCategory) -> &'static str {
    match category {
        DiscoveryCategory::Capabilities => "tools/list",
        DiscoveryCategory::Resources => "resources/list",
        DiscoveryCategory::Prompts => "prompts/list",
    }
}

// ---------------------------------------------------------------------------
// MCP 2026-07-28 result classification (resultType, MRTR, tasks extension)
// ---------------------------------------------------------------------------

/// How a modern result should be consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModernResultKind {
    /// An ordinary result (`resultType: "complete"`, or absent — clients
    /// MUST treat results without the field as complete).
    Complete,
    /// A Multi Round-Trip Requests interim result
    /// (`resultType: "input_required"`).
    InputRequired,
    /// A tasks-extension handle (`resultType: "task"`).
    Task,
}

pub fn modern_result_kind(result: &Value) -> ModernResultKind {
    match result.get("resultType").and_then(Value::as_str) {
        Some("input_required") => ModernResultKind::InputRequired,
        Some("task") => ModernResultKind::Task,
        _ => ModernResultKind::Complete,
    }
}

/// Parsed `InputRequiredResult` (MRTR, SEP-2322).
#[derive(Debug, Clone, PartialEq)]
pub struct InputRequiredResult {
    /// Server-initiated requests keyed by server-assigned identifiers.
    pub input_requests: Map<String, Value>,
    /// Opaque server state that MUST be echoed verbatim on the retry.
    pub request_state: Option<String>,
}

pub fn parse_input_required(result: &Value) -> Result<InputRequiredResult> {
    let input_requests = result
        .get("inputRequests")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let request_state = result
        .get("requestState")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    if input_requests.is_empty() && request_state.is_none() {
        return Err(anyhow!(
            "input_required result carried neither inputRequests nor requestState"
        ));
    }
    Ok(InputRequiredResult {
        input_requests,
        request_state,
    })
}

/// Rebuild a request for an MRTR retry: fresh JSON-RPC id, the collected
/// `inputResponses`, and the echoed `requestState` (removed when the
/// server did not send one — clients MUST NOT invent it).
pub fn attach_input_responses(
    request: &mut JsonRpcRequest,
    new_request_id: u64,
    input_responses: Map<String, Value>,
    request_state: Option<&str>,
) {
    request.id = JsonRpcId::Number(new_request_id);
    let params = request.params.get_or_insert_with(|| json!({}));
    if let Some(obj) = params.as_object_mut() {
        if !input_responses.is_empty() {
            obj.insert("inputResponses".to_owned(), Value::Object(input_responses));
        }
        match request_state {
            Some(state) => {
                obj.insert("requestState".to_owned(), json!(state));
            }
            None => {
                obj.remove("requestState");
            }
        }
        // Refresh the progress token so it stays unique per request id.
        if let Some(meta) = obj.get_mut("_meta").and_then(Value::as_object_mut)
            && meta.contains_key("progressToken")
        {
            meta.insert(
                "progressToken".to_owned(),
                json!(format!("mcp2cli-{}", new_request_id)),
            );
        }
    }
}

/// A task handle or snapshot from the MCP Tasks extension
/// (`io.modelcontextprotocol/tasks`).
#[derive(Debug, Clone, PartialEq)]
pub struct ModernTask {
    pub task_id: String,
    pub status: String,
    pub status_message: Option<String>,
    pub poll_interval_ms: Option<u64>,
    pub ttl_ms: Option<u64>,
    /// Pending server-initiated requests when `status == "input_required"`.
    pub input_requests: Map<String, Value>,
    /// Final result when `status == "completed"`.
    pub result: Option<Value>,
    /// JSON-RPC error object when `status == "failed"`.
    pub error: Option<Value>,
    pub raw: Value,
}

impl ModernTask {
    pub fn is_terminal(&self) -> bool {
        matches!(self.status.as_str(), "completed" | "failed" | "cancelled")
    }
}

/// Parse a `CreateTaskResult` / `tasks/get` result. Tolerates both the
/// nested (`{"task": {...}}`) and flattened shapes seen across tasks
/// extension implementations.
pub fn parse_modern_task(result: &Value) -> Result<ModernTask> {
    let task = result.get("task").unwrap_or(result);
    let task_id = task
        .get("taskId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("task result did not contain a taskId"))?
        .to_owned();
    let status = task
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("working")
        .to_owned();
    Ok(ModernTask {
        task_id,
        status,
        status_message: task
            .get("statusMessage")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        poll_interval_ms: task.get("pollIntervalMs").and_then(Value::as_u64),
        ttl_ms: task.get("ttlMs").and_then(Value::as_u64),
        input_requests: task
            .get("inputRequests")
            .or_else(|| result.get("inputRequests"))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
        result: task.get("result").or_else(|| result.get("result")).cloned(),
        error: task.get("error").or_else(|| result.get("error")).cloned(),
        raw: result.clone(),
    })
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

    fn modern_session(engine: &ProtocolEngine) -> McpClientSession {
        let mut session = engine.initial_session();
        engine.complete_discover(
            &mut session,
            MCP_PROTOCOL_VERSION_MODERN.to_owned(),
            DiscoverResult {
                supported_versions: vec![MCP_PROTOCOL_VERSION_MODERN.to_owned()],
                capabilities: ServerCapabilities::default(),
                server_info: PeerInfo {
                    name: "modern-server".to_owned(),
                    version: "1.0.0".to_owned(),
                },
                instructions: None,
            },
        );
        session
    }

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
                    input_schema: None,
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
        assert_eq!(session.era, ProtocolEra::Legacy);
        assert_eq!(session.session_id.as_deref(), Some("session-123"));
        assert_eq!(
            session
                .server_info
                .as_ref()
                .map(|value| value.name.as_str()),
            Some("demo-server")
        );
    }

    // -- MCP 2026-07-28 -----------------------------------------------------

    #[test]
    fn probe_request_carries_modern_meta() {
        let engine = ProtocolEngine::new(DEFAULT_MCP_PROTOCOL_VERSION, "mcp2cli", "0.1.0");
        let probe = engine.probe_request(1, None);

        assert_eq!(probe.method, "server/discover");
        let meta = &probe.params.unwrap()["_meta"];
        assert_eq!(
            meta[META_PROTOCOL_VERSION],
            json!(MCP_PROTOCOL_VERSION_MODERN)
        );
        assert_eq!(meta[META_CLIENT_INFO]["name"], json!("mcp2cli"));
        assert!(
            meta[META_CLIENT_CAPABILITIES]["extensions"][TASKS_EXTENSION_ID].is_object(),
            "tasks extension should be advertised"
        );
    }

    #[test]
    fn modern_prepare_injects_required_meta_and_no_handshake() {
        let engine = ProtocolEngine::new(DEFAULT_MCP_PROTOCOL_VERSION, "mcp2cli", "0.1.0")
            .with_log_level(Some("debug".to_owned()));
        let session = modern_session(&engine);

        let prepared = engine
            .prepare_operation(
                &session,
                5,
                &McpOperation::InvokeAction {
                    capability: "send".to_owned(),
                    arguments: json!({ "to": "user@example.com" }),
                    background: false,
                    input_schema: None,
                },
            )
            .expect("plan should build");

        assert!(prepared.initialize.is_none());
        assert!(prepared.initialized_notification.is_none());
        assert_eq!(prepared.request.method, "tools/call");
        let meta = &prepared.request.params.unwrap()["_meta"];
        assert_eq!(
            meta[META_PROTOCOL_VERSION],
            json!(MCP_PROTOCOL_VERSION_MODERN)
        );
        assert_eq!(meta[META_CLIENT_INFO]["name"], json!("mcp2cli"));
        assert!(meta[META_CLIENT_CAPABILITIES].is_object());
        assert_eq!(meta[META_LOG_LEVEL], json!("debug"));
        assert!(meta["progressToken"].is_string());
    }

    #[test]
    fn modern_background_invoke_does_not_set_legacy_task_meta() {
        let engine = ProtocolEngine::new(DEFAULT_MCP_PROTOCOL_VERSION, "mcp2cli", "0.1.0");
        let session = modern_session(&engine);

        let prepared = engine
            .prepare_operation(
                &session,
                5,
                &McpOperation::InvokeAction {
                    capability: "send".to_owned(),
                    arguments: json!({}),
                    background: true,
                    input_schema: None,
                },
            )
            .expect("plan should build");

        let meta = &prepared.request.params.unwrap()["_meta"];
        assert!(
            meta.get("task").is_none(),
            "modern era must not use _meta.task; the tasks extension is server-directed"
        );
    }

    #[test]
    fn modern_ping_maps_to_server_discover() {
        let engine = ProtocolEngine::new(DEFAULT_MCP_PROTOCOL_VERSION, "mcp2cli", "0.1.0");
        let session = modern_session(&engine);
        let prepared = engine
            .prepare_operation(&session, 3, &McpOperation::Ping)
            .expect("plan should build");
        assert_eq!(prepared.request.method, "server/discover");
    }

    #[test]
    fn modern_subscribe_maps_to_subscriptions_listen() {
        let engine = ProtocolEngine::new(DEFAULT_MCP_PROTOCOL_VERSION, "mcp2cli", "0.1.0");
        let session = modern_session(&engine);
        let prepared = engine
            .prepare_operation(
                &session,
                3,
                &McpOperation::SubscribeResource {
                    uri: "mail://inbox".to_owned(),
                },
            )
            .expect("plan should build");
        assert_eq!(prepared.request.method, "subscriptions/listen");
        let params = prepared.request.params.unwrap();
        assert_eq!(
            params["notifications"]["resourceSubscriptions"],
            json!(["mail://inbox"])
        );
    }

    #[test]
    fn modern_task_result_polls_tasks_get() {
        let engine = ProtocolEngine::new(DEFAULT_MCP_PROTOCOL_VERSION, "mcp2cli", "0.1.0");
        let session = modern_session(&engine);
        let prepared = engine
            .prepare_operation(
                &session,
                3,
                &McpOperation::TaskResult {
                    task_id: "task-7".to_owned(),
                },
            )
            .expect("plan should build");
        assert_eq!(prepared.request.method, "tasks/get");
    }

    #[test]
    fn modern_offline_results_cover_removed_methods() {
        let logging = modern_offline_result(&McpOperation::SetLoggingLevel {
            level: "debug".to_owned(),
        });
        assert!(matches!(
            logging,
            Some(McpOperationResult::LoggingLevelSet { .. })
        ));

        let unsubscribe = modern_offline_result(&McpOperation::UnsubscribeResource {
            uri: "mail://inbox".to_owned(),
        });
        assert!(matches!(
            unsubscribe,
            Some(McpOperationResult::Unsubscribed { .. })
        ));

        assert!(modern_offline_result(&McpOperation::Ping).is_none());
    }

    #[test]
    fn classify_probe_recognizes_modern_server() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: JsonRpcId::Number(1),
            result: Some(json!({
                "resultType": "complete",
                "supportedVersions": ["2026-07-28", "2025-11-25"],
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "modern", "version": "1.0.0" },
                "ttlMs": 3_600_000,
                "cacheScope": "public"
            })),
            error: None,
        };

        let ProbeOutcome::Modern {
            negotiated_version,
            discover,
        } = classify_probe_response(&response)
        else {
            panic!("expected modern outcome");
        };
        assert_eq!(negotiated_version, MCP_PROTOCOL_VERSION_MODERN);
        assert_eq!(discover.server_info.name, "modern");
        assert!(discover.capabilities.tools.is_some());
    }

    #[test]
    fn classify_probe_recognizes_unsupported_version_error() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: JsonRpcId::Number(1),
            result: None,
            error: Some(JsonRpcError {
                code: ERROR_UNSUPPORTED_PROTOCOL_VERSION,
                message: "Unsupported protocol version".to_owned(),
                data: Some(json!({
                    "supported": ["2027-01-01"],
                    "requested": "2026-07-28"
                })),
            }),
        };

        let ProbeOutcome::ModernUnsupported { supported } = classify_probe_response(&response)
        else {
            panic!("expected modern-unsupported outcome");
        };
        assert_eq!(supported, vec!["2027-01-01".to_owned()]);
    }

    #[test]
    fn classify_probe_falls_back_to_legacy_on_method_not_found() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: JsonRpcId::Number(1),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: "method not found".to_owned(),
                data: None,
            }),
        };

        assert!(matches!(
            classify_probe_response(&response),
            ProbeOutcome::Legacy { .. }
        ));
    }

    #[test]
    fn version_policy_parses_config_values() {
        assert_eq!(VersionPolicy::parse(None).unwrap(), VersionPolicy::Auto);
        assert_eq!(
            VersionPolicy::parse(Some("auto")).unwrap(),
            VersionPolicy::Auto
        );
        assert_eq!(
            VersionPolicy::parse(Some("2025-11-25")).unwrap(),
            VersionPolicy::PinLegacy
        );
        assert_eq!(
            VersionPolicy::parse(Some("2026-07-28")).unwrap(),
            VersionPolicy::PinModern
        );
        assert!(VersionPolicy::parse(Some("2024-11-05")).is_err());
    }

    #[test]
    fn modern_result_kind_defaults_to_complete() {
        assert_eq!(
            modern_result_kind(&json!({ "content": [] })),
            ModernResultKind::Complete
        );
        assert_eq!(
            modern_result_kind(&json!({ "resultType": "complete" })),
            ModernResultKind::Complete
        );
        assert_eq!(
            modern_result_kind(&json!({ "resultType": "input_required" })),
            ModernResultKind::InputRequired
        );
        assert_eq!(
            modern_result_kind(&json!({ "resultType": "task" })),
            ModernResultKind::Task
        );
    }

    #[test]
    fn mrtr_retry_attaches_responses_and_echoes_state() {
        let mut request = JsonRpcRequest::new(
            JsonRpcId::Number(1),
            "tools/call",
            Some(json!({
                "name": "send",
                "arguments": { "to": "user@example.com" },
                "_meta": { "progressToken": "mcp2cli-1" }
            })),
        );

        let mut responses = Map::new();
        responses.insert(
            "github_login".to_owned(),
            json!({ "action": "accept", "content": { "name": "octocat" } }),
        );
        attach_input_responses(&mut request, 3, responses, Some("opaque-state"));

        assert_eq!(request.id, JsonRpcId::Number(3));
        let params = request.params.unwrap();
        assert_eq!(
            params["inputResponses"]["github_login"]["action"],
            json!("accept")
        );
        assert_eq!(params["requestState"], json!("opaque-state"));
        assert_eq!(params["_meta"]["progressToken"], json!("mcp2cli-3"));
        // original params are preserved
        assert_eq!(params["name"], json!("send"));
    }

    #[test]
    fn mrtr_retry_omits_request_state_when_server_sent_none() {
        let mut request = JsonRpcRequest::new(JsonRpcId::Number(1), "tools/call", Some(json!({})));
        attach_input_responses(&mut request, 2, Map::new(), None);
        let params = request.params.unwrap();
        assert!(params.get("requestState").is_none());
        assert!(params.get("inputResponses").is_none());
    }

    #[test]
    fn parse_modern_task_supports_nested_and_flat_shapes() {
        let nested = parse_modern_task(&json!({
            "resultType": "task",
            "task": {
                "taskId": "task-1",
                "status": "working",
                "pollIntervalMs": 500,
                "ttlMs": 60000
            }
        }))
        .expect("nested task should parse");
        assert_eq!(nested.task_id, "task-1");
        assert_eq!(nested.status, "working");
        assert_eq!(nested.poll_interval_ms, Some(500));
        assert!(!nested.is_terminal());

        let flat = parse_modern_task(&json!({
            "resultType": "task",
            "taskId": "task-2",
            "status": "completed",
            "result": { "content": [] }
        }))
        .expect("flat task should parse");
        assert_eq!(flat.task_id, "task-2");
        assert!(flat.is_terminal());
        assert!(flat.result.is_some());
    }

    #[test]
    fn parse_input_required_requires_some_payload() {
        assert!(parse_input_required(&json!({ "resultType": "input_required" })).is_err());
        let parsed = parse_input_required(&json!({
            "resultType": "input_required",
            "requestState": "blob"
        }))
        .expect("state-only result should parse");
        assert!(parsed.input_requests.is_empty());
        assert_eq!(parsed.request_state.as_deref(), Some("blob"));
    }
}
