//! MCP transport clients and the top-level `McpClient` trait.
//!
//! # The trait
//!
//! [`McpClient`] is the boundary between protocol-aware code
//! ([`crate::mcp::protocol`]) and wire transport. Implementations:
//!
//! - [`StdioMcpClient`] — spawns a subprocess and exchanges
//!   newline-delimited JSON-RPC on its stdio. Used for servers
//!   packaged as local binaries (`npx
//!   @modelcontextprotocol/server-everything`, Python packages, etc.).
//! - [`StreamableHttpMcpClient`] — JSON-RPC POSTs over HTTP with
//!   optional Server-Sent Events on the response body for
//!   server→client notifications and mid-request streaming. Handles
//!   the MCP session lifecycle (`mcp-session-id` header) and
//!   bearer-token auth.
//! - [`DaemonMcpClient`] — IPC proxy to a local `mcp2cli daemon`
//!   holding warm connections. Transparent to callers: when a daemon
//!   is running and healthy, `build_client` routes through it.
//! - Demo/file-backed client — used by `mcp2cli --url
//!   demo.invalid/mcp` for offline-friendly onboarding and tests.
//!
//! # Responsibilities per implementation
//!
//! Each client runs its own `initialize` handshake via
//! [`crate::mcp::protocol::ProtocolEngine`], pumps the request queue,
//! routes server-initiated messages through a
//! [`crate::mcp::handler::ServerMessageHandler`], and surfaces
//! progress/log events via the [`crate::runtime::EventBroker`].
//!
//! # The `perform_with_timeout` helper
//!
//! The crate's *single* execution helper is [`perform_with_timeout`].
//! It takes an `McpClient`, an [`crate::mcp::model::McpOperation`], an
//! event broker, an optional inventory stale path (for invalidating
//! the discovery cache on list-change notifications), and a timeout.
//! The helper dispatches the operation on the client, wraps it in a
//! `tokio::time::timeout`, emits a canonical
//! [`crate::runtime::RuntimeEvent::Info`] if the call times out, and
//! returns the [`crate::mcp::model::McpOperationResult`] or the
//! underlying transport error.

use std::{collections::BTreeMap, path::PathBuf, process::Stdio};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use bytes::Bytes;
use http::{Method, Request, StatusCode, header};
use http_body_util::{BodyExt, Full};
use hyper::Uri;
use hyper_rustls::HttpsConnector;
use hyper_util::{
    client::legacy::{Client as HyperClient, connect::HttpConnector},
    rt::TokioExecutor,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::time::{Duration, sleep};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};
use url::Url;
use uuid::Uuid;

use crate::{
    config::{ResolvedAppConfig, RuntimeLayout, StdioServerConfig},
    mcp::handler::{OperationMessageHandler, ServerMessageHandler},
    mcp::model::{
        ConnectionMetadata, DiscoveryCategory, McpOperation, McpOperationResult, TaskState,
        TransportKind,
    },
    mcp::protocol::{
        DEFAULT_MCP_PROTOCOL_VERSION, InitializeResult, JsonRpcError, JsonRpcId,
        JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, MCP_PROTOCOL_VERSION_LEGACY,
        META_SUBSCRIPTION_ID, McpClientSession, ModernResultKind, ProbeOutcome, ProtocolEngine,
        ProtocolEra, VersionPolicy, attach_input_responses, classify_probe_response,
        is_modern_protocol_error, modern_offline_result, modern_result_kind, parse_input_required,
        parse_modern_task, select_modern_version,
    },
    runtime::{EventBroker, RuntimeEvent, StateStore, TokenStore},
};

/// How long the stdio `server/discover` era probe may wait before the
/// client assumes a legacy (initialize-handshake) server that will never
/// answer an unknown pre-`initialize` method.
const STDIO_PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// MRTR guard: maximum `input_required` → retry round trips for one
/// logical operation before giving up.
const MAX_MRTR_ROUNDS: usize = 8;

/// Tasks-extension polling bounds (`pollIntervalMs` is clamped into
/// this range; the outer operation timeout still governs the total wait).
const DEFAULT_TASK_POLL_MS: u64 = 1_000;
const MIN_TASK_POLL_MS: u64 = 250;
const MAX_TASK_POLL_MS: u64 = 30_000;
const MAX_TASK_POLLS: usize = 100_000;

#[async_trait]
pub trait McpClient: Send + Sync {
    async fn metadata(&self, app_id: &str) -> Result<ConnectionMetadata>;

    async fn negotiated_session(&self) -> Option<McpClientSession>;

    async fn perform(
        &self,
        app_id: &str,
        operation: McpOperation,
        events: &EventBroker,
        inventory_stale_path: Option<&PathBuf>,
    ) -> Result<McpOperationResult>;

    /// Send a cancellation notification for the given request ID.
    /// Returns Ok(()) if sent, or Err if the transport can't send it.
    async fn cancel_request(&self, request_id: u64, reason: Option<&str>) -> Result<()> {
        let _ = (request_id, reason);
        Ok(()) // default no-op for transports that don't support it
    }

    /// Whether this invocation was routed through a running `mcp2cli
    /// daemon` (a warm, already-negotiated connection) rather than
    /// connecting fresh. Used only for the coarse `daemon_active`
    /// telemetry dimension — never anything identifying the daemon.
    fn is_daemon(&self) -> bool {
        false
    }
}

/// Perform an MCP operation with an optional timeout.
/// When `timeout_seconds` is 0, the operation runs without a deadline.
pub async fn perform_with_timeout(
    client: &dyn McpClient,
    app_id: &str,
    operation: McpOperation,
    events: &EventBroker,
    inventory_stale_path: Option<&std::path::PathBuf>,
    timeout_seconds: u64,
) -> Result<McpOperationResult> {
    if timeout_seconds == 0 {
        return client
            .perform(app_id, operation, events, inventory_stale_path)
            .await;
    }
    let timeout_duration = Duration::from_secs(timeout_seconds);
    match tokio::time::timeout(
        timeout_duration,
        client.perform(app_id, operation, events, inventory_stale_path),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(anyhow!(
            "operation timed out after {} seconds (configure with defaults.timeout_seconds or --timeout)",
            timeout_seconds
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientMode {
    Demo,
    Stdio,
    StreamableHttp,
}

pub async fn build_client(
    layout: &RuntimeLayout,
    config: Option<&ResolvedAppConfig>,
) -> Result<Box<dyn McpClient>> {
    // If a daemon is running for this config, use the daemon client
    if let Some(config) = config
        && let Ok(Some(info)) = crate::runtime::daemon::daemon_status(layout, &config.name)
    {
        let socket = std::path::PathBuf::from(&info.socket_path);
        if socket.exists() {
            tracing::info!(config = %config.name, pid = info.pid, "using running daemon");
            return Ok(Box::new(DaemonMcpClient {
                config_name: config.name.clone(),
                socket_path: socket,
            }));
        }
    }

    match select_client_mode(config) {
        ClientMode::Demo => Ok(Box::new(
            DemoMcpClient::load(layout.demo_remote_state_path()).await?,
        )),
        ClientMode::Stdio => {
            let config = config.ok_or_else(|| anyhow!("missing config for stdio MCP client"))?;
            let policy = VersionPolicy::parse(config.config.server.protocol_version.as_deref())?;
            let log_level = load_server_log_level(layout, config).await;
            Ok(Box::new(StdioMcpClient::new(
                config.name.clone(),
                config.config.server.stdio.clone(),
                policy,
                log_level,
            )?))
        }
        ClientMode::StreamableHttp => {
            let config =
                config.ok_or_else(|| anyhow!("missing config for streamable HTTP MCP client"))?;
            let bearer_token = load_bearer_token(layout, config).await;
            let policy = VersionPolicy::parse(config.config.server.protocol_version.as_deref())?;
            let log_level = load_server_log_level(layout, config).await;
            Ok(Box::new(StreamableHttpMcpClient::new(
                config.name.clone(),
                config.config.server.endpoint.clone().ok_or_else(|| {
                    anyhow!("server.endpoint must be set for streamable HTTP transport")
                })?,
                bearer_token,
                policy,
                log_level,
            )?))
        }
    }
}

/// Read the persisted server log level (set via `log <LEVEL>`) so MCP
/// 2026-07-28 clients can inject it per-request via
/// `_meta[io.modelcontextprotocol/logLevel]`. Best effort — a missing or
/// unreadable state file simply means no log level is requested.
async fn load_server_log_level(
    layout: &RuntimeLayout,
    config: &ResolvedAppConfig,
) -> Option<String> {
    let state_path = layout.state_file_path(&config.name);
    if !state_path.exists() {
        return None;
    }
    let store = StateStore::load(state_path).await.ok()?;
    store.server_log_level(&config.name).await
}

/// Read the stored bearer token for a config from the same token-store file the
/// `auth login` command writes to. Returns `None` when no token is stored (or the
/// store can't be read) so unauthenticated servers keep working unchanged.
async fn load_bearer_token(layout: &RuntimeLayout, config: &ResolvedAppConfig) -> Option<String> {
    let token_path = config
        .config
        .auth
        .token_store_file
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| layout.token_store_path(&config.name));
    if !token_path.exists() {
        return None;
    }
    let store = TokenStore::new(token_path);
    match store.get(&config.name).await {
        Ok(token) => token.map(|stored| stored.bearer_token),
        Err(error) => {
            tracing::warn!(
                config = %config.name,
                %error,
                "failed to read token store for bearer auth; proceeding unauthenticated"
            );
            None
        }
    }
}

fn select_client_mode(config: Option<&ResolvedAppConfig>) -> ClientMode {
    let Some(config) = config else {
        return ClientMode::Demo;
    };

    match config.config.server.transport {
        TransportKind::Stdio => ClientMode::Stdio,
        TransportKind::StreamableHttp => {
            if is_demo_endpoint(config.config.server.endpoint.as_deref()) {
                ClientMode::Demo
            } else {
                ClientMode::StreamableHttp
            }
        }
    }
}

fn is_demo_endpoint(endpoint: Option<&str>) -> bool {
    endpoint
        .and_then(|value| Url::parse(value).ok())
        .and_then(|url| url.host_str().map(str::to_owned))
        .map(|host| host.eq_ignore_ascii_case("demo.invalid"))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DemoTaskState {
    app_id: String,
    capability: String,
    status: TaskState,
    summary: String,
    arguments: serde_json::Value,
    result: Option<serde_json::Value>,
    failure_reason: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DemoClientState {
    tasks: BTreeMap<String, DemoTaskState>,
}

pub struct DemoMcpClient {
    path: PathBuf,
    state: Mutex<DemoClientState>,
}

#[derive(Debug)]
struct StdioProcess {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

#[derive(Debug)]
pub struct StdioMcpClient {
    config_name: String,
    stdio: StdioServerConfig,
    protocol: ProtocolEngine,
    session: Mutex<McpClientSession>,
    next_request_id: Mutex<u64>,
    process: Mutex<Option<StdioProcess>>,
}

#[derive(Debug)]
pub struct StreamableHttpMcpClient {
    config_name: String,
    endpoint: Url,
    endpoint_uri: Uri,
    client: HyperClient<HttpsConnector<HttpConnector>, Full<Bytes>>,
    protocol: ProtocolEngine,
    session: Mutex<McpClientSession>,
    next_request_id: Mutex<u64>,
    /// Bearer token injected as `Authorization: Bearer <token>` on every
    /// request when present (populated from the token store at build time).
    bearer_token: Option<String>,
}

impl StdioMcpClient {
    pub fn new(
        config_name: String,
        stdio: StdioServerConfig,
        policy: VersionPolicy,
        log_level: Option<String>,
    ) -> Result<Self> {
        stdio.validate()?;
        let protocol = ProtocolEngine::new(
            DEFAULT_MCP_PROTOCOL_VERSION,
            "mcp2cli",
            env!("CARGO_PKG_VERSION"),
        )
        .with_policy(policy)
        .with_log_level(log_level);
        let session = Mutex::new(protocol.initial_session());

        Ok(Self {
            config_name,
            stdio,
            protocol,
            session,
            next_request_id: Mutex::new(1),
            process: Mutex::new(None),
        })
    }

    fn command_display(&self) -> String {
        let args = if self.stdio.args.is_empty() {
            String::new()
        } else {
            format!(" {}", self.stdio.args.join(" "))
        };
        format!(
            "{}{}",
            self.stdio.command.as_deref().unwrap_or("(unknown)"),
            args
        )
    }

    async fn ensure_process(&self) -> Result<()> {
        let mut process = self.process.lock().await;
        if process.is_some() {
            return Ok(());
        }

        let mut command = Command::new(
            self.stdio
                .command
                .as_deref()
                .ok_or_else(|| anyhow!("stdio command missing"))?,
        );
        command.args(&self.stdio.args);
        if let Some(cwd) = &self.stdio.cwd {
            command.current_dir(cwd);
        }
        if !self.stdio.env.is_empty() {
            command.envs(self.stdio.env.clone());
        }
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());

        let mut child = command
            .spawn()
            .map_err(|error| anyhow!("failed to spawn stdio MCP server: {}", error))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("stdio MCP child did not expose stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("stdio MCP child did not expose stdout"))?;
        *process = Some(StdioProcess {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout),
        });
        Ok(())
    }

    async fn send_jsonrpc_request(
        &self,
        request: &JsonRpcRequest,
        handler: Option<&OperationMessageHandler>,
    ) -> Result<JsonRpcResponse> {
        self.ensure_process().await?;
        let mut process = self.process.lock().await;
        let process = process
            .as_mut()
            .ok_or_else(|| anyhow!("stdio MCP process was not available"))?;
        let payload = serde_json::to_string(request)
            .map_err(|error| anyhow!("failed to serialize stdio JSON-RPC request: {}", error))?;
        process
            .stdin
            .write_all(payload.as_bytes())
            .await
            .map_err(|error| anyhow!("failed to write stdio JSON-RPC request: {}", error))?;
        process
            .stdin
            .write_all(b"\n")
            .await
            .map_err(|error| anyhow!("failed to terminate stdio JSON-RPC request: {}", error))?;
        process
            .stdin
            .flush()
            .await
            .map_err(|error| anyhow!("failed to flush stdio JSON-RPC request: {}", error))?;

        let expected_id = request.id.clone();
        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read =
                process.stdout.read_line(&mut line).await.map_err(|error| {
                    anyhow!("failed to read stdio JSON-RPC response: {}", error)
                })?;
            if bytes_read == 0 {
                return Err(anyhow!(
                    "stdio MCP server ended before returning a JSON-RPC response"
                ));
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let value: Value = match serde_json::from_str(trimmed) {
                Ok(value) => value,
                Err(_) => continue,
            };

            let has_method = value.get("method").is_some();
            let has_id = value.get("id").is_some();

            // Server→client notification: has "method" but no "id"
            if has_method && !has_id {
                if let Some(handler) = handler {
                    let method = value["method"].as_str().unwrap_or("");
                    handler.handle_notification(method, value.get("params"));
                }
                continue;
            }

            // Server→client request: has both "method" and "id"
            if has_method
                && has_id
                && let Ok(server_request) = serde_json::from_value::<JsonRpcRequest>(value.clone())
            {
                let response = if let Some(handler) = handler {
                    handler.handle_request(&server_request)?
                } else {
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_owned(),
                        id: server_request.id.clone(),
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32601,
                            message: format!("method not found: {}", server_request.method),
                            data: None,
                        }),
                    }
                };
                let response_payload = serde_json::to_string(&response).map_err(|error| {
                    anyhow!("failed to serialize server→client response: {}", error)
                })?;
                process
                    .stdin
                    .write_all(response_payload.as_bytes())
                    .await
                    .map_err(|error| {
                        anyhow!("failed to write server→client response: {}", error)
                    })?;
                process.stdin.write_all(b"\n").await.map_err(|error| {
                    anyhow!("failed to terminate server→client response: {}", error)
                })?;
                process.stdin.flush().await.map_err(|error| {
                    anyhow!("failed to flush server→client response: {}", error)
                })?;
                continue;
            }

            if has_id {
                let response: JsonRpcResponse = serde_json::from_value(value).map_err(|error| {
                    anyhow!("failed to decode stdio JSON-RPC response: {}", error)
                })?;
                if response.id == expected_id {
                    return Ok(response);
                }
            }
        }
    }

    async fn send_notification(&self, notification: &JsonRpcNotification) -> Result<()> {
        self.ensure_process().await?;
        let mut process = self.process.lock().await;
        let process = process
            .as_mut()
            .ok_or_else(|| anyhow!("stdio MCP process was not available"))?;
        let payload = serde_json::to_string(notification).map_err(|error| {
            anyhow!("failed to serialize stdio JSON-RPC notification: {}", error)
        })?;
        process
            .stdin
            .write_all(payload.as_bytes())
            .await
            .map_err(|error| anyhow!("failed to write stdio JSON-RPC notification: {}", error))?;
        process.stdin.write_all(b"\n").await.map_err(|error| {
            anyhow!("failed to terminate stdio JSON-RPC notification: {}", error)
        })?;
        process
            .stdin
            .flush()
            .await
            .map_err(|error| anyhow!("failed to flush stdio JSON-RPC notification: {}", error))?;
        Ok(())
    }

    async fn allocate_request_id(&self) -> u64 {
        let mut next_request_id = self.next_request_id.lock().await;
        let value = *next_request_id;
        *next_request_id += 2;
        value
    }

    /// Determine the protocol era before the first real operation.
    ///
    /// Per the MCP 2026-07-28 stdio backward-compatibility rules the client
    /// probes with `server/discover`: a `DiscoverResult` (or a recognised
    /// modern error) marks the server modern; any other error — or silence
    /// within [`STDIO_PROBE_TIMEOUT`] — marks it legacy, and the legacy
    /// `initialize` handshake rides along with the first operation as
    /// before. Pinning `server.protocol_version` skips the probe (legacy)
    /// or forbids the fallback (modern).
    async fn ensure_ready(
        &self,
        app_id: &str,
        events: &EventBroker,
        handler: &OperationMessageHandler,
    ) -> Result<()> {
        {
            let session = self.session.lock().await;
            if session.initialized {
                return Ok(());
            }
        }
        let policy = self.protocol.policy();
        if !policy.probe_first() {
            return Ok(());
        }

        let request_id = self.allocate_request_id().await;
        let probe = self.protocol.probe_request(request_id, None);
        let outcome = match tokio::time::timeout(
            STDIO_PROBE_TIMEOUT,
            self.send_jsonrpc_request(&probe, Some(handler)),
        )
        .await
        {
            Ok(Ok(response)) => classify_probe_response(&response),
            Ok(Err(error)) => {
                if policy.allows_legacy_fallback() {
                    ProbeOutcome::Legacy {
                        detail: format!("server/discover probe failed: {}", error),
                    }
                } else {
                    return Err(error);
                }
            }
            Err(_) => ProbeOutcome::Legacy {
                detail: format!(
                    "server/discover probe timed out after {}s",
                    STDIO_PROBE_TIMEOUT.as_secs()
                ),
            },
        };

        match outcome {
            ProbeOutcome::Modern {
                negotiated_version,
                discover,
            } => {
                {
                    let mut session = self.session.lock().await;
                    self.protocol.complete_discover(
                        &mut session,
                        negotiated_version.clone(),
                        discover,
                    );
                }
                events.emit(RuntimeEvent::Info {
                    app_id: app_id.to_owned(),
                    message: format!(
                        "negotiated MCP {} (stateless) via server/discover",
                        negotiated_version
                    ),
                });
                Ok(())
            }
            ProbeOutcome::ModernUnsupported { supported } => {
                if let Some(version) = select_modern_version(&supported) {
                    let request_id = self.allocate_request_id().await;
                    let probe = self.protocol.probe_request(request_id, Some(&version));
                    let response = self.send_jsonrpc_request(&probe, Some(handler)).await?;
                    if let ProbeOutcome::Modern {
                        negotiated_version,
                        discover,
                    } = classify_probe_response(&response)
                    {
                        let mut session = self.session.lock().await;
                        self.protocol
                            .complete_discover(&mut session, negotiated_version, discover);
                        return Ok(());
                    }
                    return Err(anyhow!(
                        "server advertised MCP {} but rejected the retried server/discover",
                        version
                    ));
                }
                if policy.allows_legacy_fallback()
                    && supported
                        .iter()
                        .any(|version| version == MCP_PROTOCOL_VERSION_LEGACY)
                {
                    events.emit(RuntimeEvent::Info {
                        app_id: app_id.to_owned(),
                        message: format!(
                            "server offers MCP {} only; using the initialize handshake",
                            MCP_PROTOCOL_VERSION_LEGACY
                        ),
                    });
                    return Ok(());
                }
                Err(anyhow!(
                    "no mutually supported MCP protocol version (server supports: {})",
                    supported.join(", ")
                ))
            }
            ProbeOutcome::Legacy { detail } => {
                if policy.allows_legacy_fallback() {
                    tracing::debug!(%detail, "treating server as legacy MCP");
                    events.emit(RuntimeEvent::Info {
                        app_id: app_id.to_owned(),
                        message: format!(
                            "server is not MCP 2026-07-28 ({}); falling back to the {} initialize handshake",
                            detail, MCP_PROTOCOL_VERSION_LEGACY
                        ),
                    });
                    Ok(())
                } else {
                    Err(anyhow!(
                        "server did not answer server/discover as an MCP 2026-07-28 server ({}); server.protocol_version pins 2026-07-28",
                        detail
                    ))
                }
            }
        }
    }

    /// One-shot modern subscription check: open a `subscriptions/listen`
    /// stream, wait for `notifications/subscriptions/acknowledged`, then
    /// cancel the stream (`notifications/cancelled` on stdio). MCP
    /// 2026-07-28 subscriptions last only while a listener holds the
    /// stream open, so a one-shot CLI reports the acknowledgment and
    /// releases the stream.
    async fn modern_subscribe(
        &self,
        uri: &str,
        session: &McpClientSession,
        handler: &OperationMessageHandler,
    ) -> Result<McpOperationResult> {
        let request_id = self.allocate_request_id().await;
        let prepared = self.protocol.prepare_operation(
            session,
            request_id,
            &McpOperation::SubscribeResource {
                uri: uri.to_owned(),
            },
        )?;
        self.ensure_process().await?;
        let mut process = self.process.lock().await;
        let process = process
            .as_mut()
            .ok_or_else(|| anyhow!("stdio MCP process was not available"))?;
        let payload = serde_json::to_string(&prepared.request)
            .map_err(|error| anyhow!("failed to serialize subscriptions/listen: {}", error))?;
        process
            .stdin
            .write_all(format!("{}\n", payload).as_bytes())
            .await
            .map_err(|error| anyhow!("failed to write subscriptions/listen: {}", error))?;
        process
            .stdin
            .flush()
            .await
            .map_err(|error| anyhow!("failed to flush subscriptions/listen: {}", error))?;

        let expected_id = JsonRpcId::Number(request_id);
        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = process
                .stdout
                .read_line(&mut line)
                .await
                .map_err(|error| anyhow!("failed to read listen stream: {}", error))?;
            if bytes_read == 0 {
                return Err(anyhow!(
                    "stdio MCP server ended before acknowledging the subscription"
                ));
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            let has_method = value.get("method").is_some();
            let has_id = value.get("id").is_some();

            if has_method && !has_id {
                let method = value["method"].as_str().unwrap_or("");
                handler.handle_notification(method, value.get("params"));
                let matches_subscription = value
                    .get("params")
                    .and_then(|params| params.get("_meta"))
                    .and_then(|meta| meta.get(META_SUBSCRIPTION_ID))
                    .and_then(Value::as_u64)
                    == Some(request_id);
                if method == "notifications/subscriptions/acknowledged" && matches_subscription {
                    let cancel = JsonRpcNotification::new(
                        "notifications/cancelled",
                        Some(json!({
                            "requestId": request_id,
                            "reason": "one-shot subscribe verification complete",
                        })),
                    );
                    let cancel_payload = serde_json::to_string(&cancel)
                        .map_err(|error| anyhow!("failed to serialize cancellation: {}", error))?;
                    process
                        .stdin
                        .write_all(format!("{}\n", cancel_payload).as_bytes())
                        .await
                        .map_err(|error| anyhow!("failed to write cancellation: {}", error))?;
                    process
                        .stdin
                        .flush()
                        .await
                        .map_err(|error| anyhow!("failed to flush cancellation: {}", error))?;
                    return Ok(McpOperationResult::Subscribed {
                        message: format!(
                            "server acknowledged resource subscription for '{}' (MCP 2026-07-28: subscriptions last only while a subscriptions/listen stream stays open; this check released the stream)",
                            uri
                        ),
                        uri: uri.to_owned(),
                    });
                }
                continue;
            }

            if !has_method
                && has_id
                && let Ok(response) = serde_json::from_value::<JsonRpcResponse>(value.clone())
                && response.id == expected_id
            {
                if let Some(error) = response.error {
                    return Err(anyhow!(
                        "subscriptions/listen failed: json-rpc error {}: {}",
                        error.code,
                        error.message
                    ));
                }
                return Ok(McpOperationResult::Subscribed {
                    message: format!(
                        "subscriptions/listen for '{}' was closed gracefully by the server",
                        uri
                    ),
                    uri: uri.to_owned(),
                });
            }
        }
    }
}

struct StdioModernSender<'a> {
    client: &'a StdioMcpClient,
    handler: &'a OperationMessageHandler,
}

#[async_trait]
impl ModernSender for StdioModernSender<'_> {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        self.client
            .send_jsonrpc_request(request, Some(self.handler))
            .await
    }

    async fn allocate_request_id(&self) -> u64 {
        self.client.allocate_request_id().await
    }
}

impl StreamableHttpMcpClient {
    pub fn new(
        config_name: String,
        endpoint: String,
        bearer_token: Option<String>,
        policy: VersionPolicy,
        log_level: Option<String>,
    ) -> Result<Self> {
        let endpoint = Url::parse(&endpoint).map_err(|error| {
            anyhow!("invalid streamable HTTP endpoint '{}': {}", endpoint, error)
        })?;
        if !matches!(endpoint.scheme(), "http" | "https") {
            return Err(anyhow!(
                "streamable HTTP endpoint must use http or https; got '{}'",
                endpoint.scheme()
            ));
        }
        let endpoint_uri = endpoint.as_str().parse::<Uri>().map_err(|error| {
            anyhow!(
                "invalid streamable HTTP endpoint URI '{}': {}",
                endpoint,
                error
            )
        })?;
        // Accept both plain http (local/dev servers) and https (anything real,
        // and required whenever a bearer token is sent). Trust starts from the
        // bundled webpki roots and layers in SSL_CERT_FILE when set (see
        // crate::tls), so the binary needs no system trust store by default
        // but still works behind a corporate TLS-inspection proxy.
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(crate::tls::client_config())
            .https_or_http()
            .enable_http1()
            .build();
        let client = HyperClient::builder(TokioExecutor::new()).build(connector);
        let protocol = ProtocolEngine::new(
            DEFAULT_MCP_PROTOCOL_VERSION,
            "mcp2cli",
            env!("CARGO_PKG_VERSION"),
        )
        .with_policy(policy)
        .with_log_level(log_level);
        let session = Mutex::new(protocol.initial_session());

        Ok(Self {
            config_name,
            endpoint,
            endpoint_uri,
            client,
            protocol,
            session,
            next_request_id: Mutex::new(1),
            bearer_token,
        })
    }

    fn endpoint_display(&self) -> String {
        self.endpoint.as_str().to_owned()
    }

    async fn send_jsonrpc_request(
        &self,
        request: &JsonRpcRequest,
        session: &McpClientSession,
        handler: Option<&OperationMessageHandler>,
    ) -> Result<HttpJsonRpcResponse> {
        let bytes = serde_json::to_vec(request)
            .map_err(|error| anyhow!("failed to serialize JSON-RPC request: {}", error))?;
        let http_response = self
            .send_http_message(&bytes, &WireHeaders::legacy(session, session.initialized))
            .await?;
        let session_id = http_response.session_id.clone();
        let protocol_version = http_response.protocol_version.clone();
        let response = http_response.into_jsonrpc_response(handler)?;

        Ok(HttpJsonRpcResponse {
            response,
            session_id,
            protocol_version,
        })
    }

    /// Send a modern (MCP 2026-07-28) request: no session header, required
    /// `MCP-Protocol-Version` / `Mcp-Method` / `Mcp-Name` request-metadata
    /// headers, plus any `Mcp-Param-*` headers mirrored from
    /// `x-mcp-header`-annotated tool parameters. HTTP 4xx responses that
    /// carry a JSON-RPC error body surface as JSON-RPC errors so callers
    /// can react to protocol errors like `UnsupportedProtocolVersionError`.
    async fn send_modern_jsonrpc(
        &self,
        request: &JsonRpcRequest,
        protocol_version: &str,
        param_headers: &[(String, String)],
        handler: Option<&OperationMessageHandler>,
    ) -> Result<JsonRpcResponse> {
        let bytes = serde_json::to_vec(request)
            .map_err(|error| anyhow!("failed to serialize JSON-RPC request: {}", error))?;
        let headers = WireHeaders::modern(protocol_version, request, param_headers);
        let raw = self.send_http_message(&bytes, &headers).await?;
        modern_http_into_jsonrpc(raw, handler)
    }

    async fn send_notification(
        &self,
        notification: &JsonRpcNotification,
        session: &McpClientSession,
    ) -> Result<()> {
        let bytes = serde_json::to_vec(notification)
            .map_err(|error| anyhow!("failed to serialize JSON-RPC notification: {}", error))?;
        let response = self
            .send_http_message(&bytes, &WireHeaders::legacy(session, session.initialized))
            .await?;
        if !matches!(
            response.status,
            StatusCode::ACCEPTED | StatusCode::OK | StatusCode::NO_CONTENT
        ) {
            return Err(anyhow!(
                "unexpected HTTP status {} for MCP notification",
                response.status
            ));
        }
        Ok(())
    }

    async fn send_http_message(
        &self,
        body: &[u8],
        wire_headers: &WireHeaders,
    ) -> Result<HttpTransportResponse> {
        let response = self.dispatch_http_request(body, wire_headers).await?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .into_body()
            .collect()
            .await
            .map_err(|error| anyhow!("failed to read streamable HTTP response body: {}", error))?
            .to_bytes();

        Ok(HttpTransportResponse {
            status,
            content_type: headers
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned),
            session_id: headers
                .get("mcp-session-id")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned),
            protocol_version: headers
                .get("mcp-protocol-version")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned),
            body,
        })
    }

    /// Issue the POST and return the raw hyper response (body unread).
    /// Callers either collect the body ([`Self::send_http_message`]) or
    /// stream it incrementally (`subscriptions/listen`).
    async fn dispatch_http_request(
        &self,
        body: &[u8],
        wire_headers: &WireHeaders,
    ) -> Result<hyper::Response<hyper::body::Incoming>> {
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri(self.endpoint_uri.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream");

        if let Some(version) = &wire_headers.protocol_version {
            builder = builder.header("mcp-protocol-version", version.as_str());
        }
        if let Some(session_id) = &wire_headers.session_id {
            builder = builder.header("mcp-session-id", session_id.as_str());
        }
        if let Some(method) = &wire_headers.mcp_method {
            builder = builder.header("mcp-method", method.as_str());
        }
        if let Some(name) = &wire_headers.mcp_name {
            builder = builder.header("mcp-name", name.as_str());
        }
        for (name, value) in &wire_headers.params {
            builder = builder.header(format!("mcp-param-{}", name), value.as_str());
        }
        if let Some(token) = &self.bearer_token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {}", token));
        }

        let request = builder
            .body(Full::new(Bytes::copy_from_slice(body)))
            .map_err(|error| anyhow!("failed to build HTTP MCP request: {}", error))?;
        self.client
            .request(request)
            .await
            .map_err(|error| anyhow!("streamable HTTP request failed: {}", error))
    }

    async fn allocate_request_id(&self) -> u64 {
        let mut next_request_id = self.next_request_id.lock().await;
        let value = *next_request_id;
        *next_request_id += 2;
        value
    }

    /// Determine the protocol era before the first real operation.
    ///
    /// Per the MCP 2026-07-28 Streamable HTTP backward-compatibility rules
    /// the client attempts a modern request first (`server/discover`, which
    /// modern servers MUST implement) and inspects failure bodies: a
    /// recognised modern JSON-RPC error keeps the client modern; an HTTP
    /// error without one falls back to the legacy `initialize` handshake.
    /// Network failures propagate — a dead endpoint is dead in either era.
    async fn ensure_ready(&self, app_id: &str, events: &EventBroker) -> Result<()> {
        {
            let session = self.session.lock().await;
            if session.initialized {
                return Ok(());
            }
        }
        let policy = self.protocol.policy();
        if !policy.probe_first() {
            return Ok(());
        }

        let request_id = self.allocate_request_id().await;
        let probe = self.protocol.probe_request(request_id, None);
        let outcome = self
            .send_probe(&probe, crate::mcp::protocol::MCP_PROTOCOL_VERSION_MODERN)
            .await?;

        match outcome {
            ProbeOutcome::Modern {
                negotiated_version,
                discover,
            } => {
                {
                    let mut session = self.session.lock().await;
                    self.protocol.complete_discover(
                        &mut session,
                        negotiated_version.clone(),
                        discover,
                    );
                }
                events.emit(RuntimeEvent::Info {
                    app_id: app_id.to_owned(),
                    message: format!(
                        "negotiated MCP {} (stateless) via server/discover",
                        negotiated_version
                    ),
                });
                Ok(())
            }
            ProbeOutcome::ModernUnsupported { supported } => {
                if let Some(version) = select_modern_version(&supported) {
                    let request_id = self.allocate_request_id().await;
                    let probe = self.protocol.probe_request(request_id, Some(&version));
                    if let ProbeOutcome::Modern {
                        negotiated_version,
                        discover,
                    } = self.send_probe(&probe, &version).await?
                    {
                        let mut session = self.session.lock().await;
                        self.protocol
                            .complete_discover(&mut session, negotiated_version, discover);
                        return Ok(());
                    }
                    return Err(anyhow!(
                        "server advertised MCP {} but rejected the retried server/discover",
                        version
                    ));
                }
                if policy.allows_legacy_fallback()
                    && supported
                        .iter()
                        .any(|version| version == MCP_PROTOCOL_VERSION_LEGACY)
                {
                    events.emit(RuntimeEvent::Info {
                        app_id: app_id.to_owned(),
                        message: format!(
                            "server offers MCP {} only; using the initialize handshake",
                            MCP_PROTOCOL_VERSION_LEGACY
                        ),
                    });
                    return Ok(());
                }
                Err(anyhow!(
                    "no mutually supported MCP protocol version (server supports: {})",
                    supported.join(", ")
                ))
            }
            ProbeOutcome::Legacy { detail } => {
                if policy.allows_legacy_fallback() {
                    tracing::debug!(%detail, "treating server as legacy MCP");
                    events.emit(RuntimeEvent::Info {
                        app_id: app_id.to_owned(),
                        message: format!(
                            "server is not MCP 2026-07-28 ({}); falling back to the {} initialize handshake",
                            detail, MCP_PROTOCOL_VERSION_LEGACY
                        ),
                    });
                    Ok(())
                } else {
                    Err(anyhow!(
                        "server did not answer server/discover as an MCP 2026-07-28 server ({}); server.protocol_version pins 2026-07-28",
                        detail
                    ))
                }
            }
        }
    }

    async fn send_probe(&self, probe: &JsonRpcRequest, version: &str) -> Result<ProbeOutcome> {
        let bytes = serde_json::to_vec(probe)
            .map_err(|error| anyhow!("failed to serialize server/discover probe: {}", error))?;
        let headers = WireHeaders::modern(version, probe, &[]);
        let raw = self.send_http_message(&bytes, &headers).await?;

        if raw.status == StatusCode::OK {
            let response = raw.into_jsonrpc_response(None)?;
            return Ok(classify_probe_response(&response));
        }
        let status = raw.status;
        if let Ok(response) = serde_json::from_slice::<JsonRpcResponse>(&raw.body)
            && let Some(error) = &response.error
        {
            if is_modern_protocol_error(error.code) {
                return Ok(classify_probe_response(&response));
            }
            return Ok(ProbeOutcome::Legacy {
                detail: format!(
                    "HTTP {} with JSON-RPC error {}: {}",
                    status, error.code, error.message
                ),
            });
        }
        Ok(ProbeOutcome::Legacy {
            detail: format!("HTTP {} without a modern JSON-RPC error body", status),
        })
    }

    /// One-shot modern subscription check over Streamable HTTP: POST
    /// `subscriptions/listen`, read the SSE response stream incrementally
    /// until `notifications/subscriptions/acknowledged` arrives, then drop
    /// the stream — closing the response stream is the MCP 2026-07-28
    /// cancellation signal.
    async fn modern_subscribe(
        &self,
        uri: &str,
        session: &McpClientSession,
        handler: &OperationMessageHandler,
    ) -> Result<McpOperationResult> {
        let request_id = self.allocate_request_id().await;
        let prepared = self.protocol.prepare_operation(
            session,
            request_id,
            &McpOperation::SubscribeResource {
                uri: uri.to_owned(),
            },
        )?;
        let bytes = serde_json::to_vec(&prepared.request)
            .map_err(|error| anyhow!("failed to serialize subscriptions/listen: {}", error))?;
        let headers = WireHeaders::modern(&session.protocol_version, &prepared.request, &[]);
        let response = self.dispatch_http_request(&bytes, &headers).await?;
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);

        if status != StatusCode::OK
            || !content_type
                .as_deref()
                .unwrap_or_default()
                .starts_with("text/event-stream")
        {
            // Collected-body path: an error, or an immediate JSON close.
            let body = response
                .into_body()
                .collect()
                .await
                .map_err(|error| anyhow!("failed to read listen response body: {}", error))?
                .to_bytes();
            let raw = HttpTransportResponse {
                status,
                content_type,
                session_id: None,
                protocol_version: None,
                body,
            };
            let jsonrpc = modern_http_into_jsonrpc(raw, Some(handler))?;
            if let Some(error) = jsonrpc.error {
                return Err(anyhow!(
                    "subscriptions/listen failed: json-rpc error {}: {}",
                    error.code,
                    error.message
                ));
            }
            return Ok(McpOperationResult::Subscribed {
                message: format!(
                    "subscriptions/listen for '{}' was closed gracefully by the server",
                    uri
                ),
                uri: uri.to_owned(),
            });
        }

        let mut body = response.into_body();
        let mut buffer = String::new();
        loop {
            let Some(frame) = body.frame().await else {
                return Err(anyhow!(
                    "listen stream ended before the server acknowledged the subscription"
                ));
            };
            let frame =
                frame.map_err(|error| anyhow!("failed to read listen stream: {}", error))?;
            if let Some(data) = frame.data_ref() {
                buffer.push_str(&String::from_utf8_lossy(data).replace("\r\n", "\n"));
            }
            while let Some(boundary) = buffer.find("\n\n") {
                let event = buffer[..boundary].to_owned();
                buffer.replace_range(..boundary + 2, "");
                let payload = event
                    .lines()
                    .filter_map(|line| line.strip_prefix("data:"))
                    .map(str::trim_start)
                    .collect::<Vec<_>>()
                    .join("\n");
                if payload.is_empty() {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(&payload) else {
                    continue;
                };
                let has_method = value.get("method").is_some();
                let has_id = value.get("id").is_some();
                if has_method && !has_id {
                    let method = value["method"].as_str().unwrap_or("");
                    handler.handle_notification(method, value.get("params"));
                    if method == "notifications/subscriptions/acknowledged" {
                        // Dropping `body` closes the response stream, which
                        // the server MUST treat as cancellation.
                        return Ok(McpOperationResult::Subscribed {
                            message: format!(
                                "server acknowledged resource subscription for '{}' (MCP 2026-07-28: subscriptions last only while a subscriptions/listen stream stays open; this check released the stream)",
                                uri
                            ),
                            uri: uri.to_owned(),
                        });
                    }
                    continue;
                }
                if !has_method
                    && has_id
                    && let Ok(response) = serde_json::from_value::<JsonRpcResponse>(value)
                {
                    if let Some(error) = response.error {
                        return Err(anyhow!(
                            "subscriptions/listen failed: json-rpc error {}: {}",
                            error.code,
                            error.message
                        ));
                    }
                    return Ok(McpOperationResult::Subscribed {
                        message: format!(
                            "subscriptions/listen for '{}' was closed gracefully by the server",
                            uri
                        ),
                        uri: uri.to_owned(),
                    });
                }
            }
        }
    }
}

struct HttpModernSender<'a> {
    client: &'a StreamableHttpMcpClient,
    handler: &'a OperationMessageHandler,
    protocol_version: String,
    param_headers: Vec<(String, String)>,
}

#[async_trait]
impl ModernSender for HttpModernSender<'_> {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        self.client
            .send_modern_jsonrpc(
                request,
                &self.protocol_version,
                &self.param_headers,
                Some(self.handler),
            )
            .await
    }

    async fn allocate_request_id(&self) -> u64 {
        self.client.allocate_request_id().await
    }
}

/// HTTP headers attached to a Streamable HTTP POST. Legacy requests carry
/// the session id (and, once negotiated, the protocol version); modern
/// requests carry the MCP 2026-07-28 request-metadata headers instead.
#[derive(Debug, Clone, Default)]
struct WireHeaders {
    protocol_version: Option<String>,
    session_id: Option<String>,
    mcp_method: Option<String>,
    mcp_name: Option<String>,
    /// (`x-mcp-header` name, encoded value) pairs → `Mcp-Param-{name}`.
    params: Vec<(String, String)>,
}

impl WireHeaders {
    fn legacy(session: &McpClientSession, include_protocol_version: bool) -> Self {
        Self {
            protocol_version: include_protocol_version.then(|| session.protocol_version.clone()),
            session_id: session.session_id.clone(),
            ..Self::default()
        }
    }

    fn modern(
        protocol_version: &str,
        request: &JsonRpcRequest,
        param_headers: &[(String, String)],
    ) -> Self {
        let params = if request.method == "tools/call" {
            param_headers.to_vec()
        } else {
            Vec::new()
        };
        Self {
            protocol_version: Some(protocol_version.to_owned()),
            session_id: None,
            mcp_method: Some(request.method.clone()),
            mcp_name: mcp_name_for_request(request).map(|name| encode_header_value(&name)),
            params,
        }
    }
}

/// Convert a raw HTTP transport response into a JSON-RPC response under
/// modern semantics: modern servers return protocol errors (unsupported
/// version, header mismatch, unknown method) as HTTP 4xx **with** a
/// JSON-RPC error body, which must reach the caller as a JSON-RPC error
/// rather than an opaque transport failure.
fn modern_http_into_jsonrpc(
    raw: HttpTransportResponse,
    handler: Option<&OperationMessageHandler>,
) -> Result<JsonRpcResponse> {
    if raw.status == StatusCode::OK {
        return raw.into_jsonrpc_response(handler);
    }
    if let Ok(response) = serde_json::from_slice::<JsonRpcResponse>(&raw.body)
        && response.error.is_some()
    {
        return Ok(response);
    }
    let body = String::from_utf8_lossy(&raw.body);
    Err(anyhow!(
        "unexpected HTTP status {} from streamable MCP endpoint: {}",
        raw.status,
        body.trim()
    ))
}

/// Source of the `Mcp-Name` header per MCP 2026-07-28: `params.name` for
/// `tools/call` / `prompts/get`, `params.uri` for `resources/read`.
fn mcp_name_for_request(request: &JsonRpcRequest) -> Option<String> {
    let params = request.params.as_ref()?;
    let field = match request.method.as_str() {
        "tools/call" | "prompts/get" => "name",
        "resources/read" => "uri",
        _ => return None,
    };
    params
        .get(field)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

/// Encode a header value per the MCP 2026-07-28 value-encoding rules:
/// plain visible-ASCII values pass through; anything else (non-ASCII,
/// control characters, leading/trailing whitespace, or a literal value
/// matching the sentinel pattern) is carried as `=?base64?<data>?=`.
fn encode_header_value(raw: &str) -> String {
    let plain_safe = !raw.is_empty()
        && raw
            .bytes()
            .all(|byte| (0x21..=0x7E).contains(&byte) || byte == b' ')
        && !raw.starts_with(' ')
        && !raw.ends_with(' ')
        && !(raw.starts_with("=?base64?") && raw.ends_with("?="));
    if plain_safe {
        return raw.to_owned();
    }
    format!("=?base64?{}?=", base64_standard(raw.as_bytes()))
}

/// Minimal RFC 4648 standard-alphabet base64 encoder (with padding), kept
/// local to avoid a dependency for one header-encoding rule.
fn base64_standard(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        output.push(ALPHABET[(triple >> 18) as usize & 0x3F] as char);
        output.push(ALPHABET[(triple >> 12) as usize & 0x3F] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 0x3F] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 0x3F] as char
        } else {
            '='
        });
    }
    output
}

/// An `x-mcp-header` annotation discovered in a tool `inputSchema`
/// (SEP-2243): `name` becomes the `Mcp-Param-{name}` header, `path` is
/// the `properties`-only chain to the annotated parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HeaderAnnotation {
    name: String,
    path: Vec<String>,
    prop_type: String,
}

/// Collect and validate every `x-mcp-header` annotation in a tool's
/// `inputSchema`. Returns an error when any annotation violates the
/// SEP-2243 constraints — per spec such a tool definition is invalid and
/// MUST be excluded from `tools/list` by Streamable HTTP clients.
fn collect_header_annotations(input_schema: &Value) -> Result<Vec<HeaderAnnotation>> {
    fn walk(schema: &Value, path: &mut Vec<String>, out: &mut Vec<HeaderAnnotation>) -> Result<()> {
        // Annotations are only valid on chains made purely of `properties`
        // keys; anything below `items`, composition keywords, `$ref`, or
        // conditionals is deliberately not traversed, so an annotation
        // there is simply unreachable — but the spec makes a *present*
        // annotation on the current node invalid outside a property.
        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            return Ok(());
        };
        for (key, prop) in properties {
            path.push(key.clone());
            if let Some(header) = prop.get("x-mcp-header") {
                let name = header
                    .as_str()
                    .ok_or_else(|| anyhow!("x-mcp-header on '{}' must be a string", key))?;
                validate_header_name(name, key)?;
                let prop_type = prop
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if !matches!(prop_type.as_str(), "string" | "integer" | "boolean") {
                    return Err(anyhow!(
                        "x-mcp-header '{}' on '{}' targets type '{}'; only string, integer and boolean are permitted",
                        name,
                        key,
                        prop_type
                    ));
                }
                out.push(HeaderAnnotation {
                    name: name.to_owned(),
                    path: path.clone(),
                    prop_type,
                });
            }
            walk(prop, path, out)?;
            path.pop();
        }
        Ok(())
    }

    let mut annotations = Vec::new();
    walk(input_schema, &mut Vec::new(), &mut annotations)?;

    // Names must be case-insensitively unique within the schema.
    for (index, annotation) in annotations.iter().enumerate() {
        if annotations[..index]
            .iter()
            .any(|prior| prior.name.eq_ignore_ascii_case(&annotation.name))
        {
            return Err(anyhow!(
                "duplicate x-mcp-header name '{}' (names are case-insensitively unique)",
                annotation.name
            ));
        }
    }
    Ok(annotations)
}

fn validate_header_name(name: &str, property: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("x-mcp-header on '{}' must not be empty", property));
    }
    let is_tchar = |c: char| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c);
    if !name.chars().all(is_tchar) {
        return Err(anyhow!(
            "x-mcp-header '{}' on '{}' is not a valid HTTP field-name token",
            name,
            property
        ));
    }
    Ok(())
}

/// Build `Mcp-Param-*` header pairs for a `tools/call` from the tool's
/// cached `inputSchema` and the call arguments. Missing or `null` values
/// omit the header, matching the SEP-2243 client behavior table.
fn extract_param_headers(
    input_schema: Option<&Value>,
    arguments: &Value,
) -> Result<Vec<(String, String)>> {
    let Some(schema) = input_schema else {
        return Ok(Vec::new());
    };
    let annotations = collect_header_annotations(schema)?;
    let mut headers = Vec::new();
    for annotation in annotations {
        let mut cursor = arguments;
        let mut present = true;
        for key in &annotation.path {
            match cursor.get(key) {
                Some(next) => cursor = next,
                None => {
                    present = false;
                    break;
                }
            }
        }
        if !present || cursor.is_null() {
            continue;
        }
        let rendered = match annotation.prop_type.as_str() {
            "string" => cursor.as_str().map(ToOwned::to_owned),
            "integer" => cursor.as_i64().and_then(|value| {
                // JavaScript safe-integer bound required by the spec.
                (value.abs() <= 9_007_199_254_740_991).then(|| value.to_string())
            }),
            "boolean" => cursor.as_bool().map(|value| value.to_string()),
            _ => None,
        };
        if let Some(rendered) = rendered {
            headers.push((annotation.name.clone(), encode_header_value(&rendered)));
        }
    }
    Ok(headers)
}

/// Drop tools whose `x-mcp-header` annotations are invalid from a
/// discovery result, as MCP 2026-07-28 requires of Streamable HTTP
/// clients, logging a warning for each rejected tool.
fn reject_invalid_header_tools(items: &mut Vec<Value>, events: &EventBroker, app_id: &str) {
    items.retain(|item| {
        if item.get("kind").and_then(Value::as_str) != Some("tool") {
            return true;
        }
        let Some(schema) = item.get("inputSchema") else {
            return true;
        };
        match collect_header_annotations(schema) {
            Ok(_) => true,
            Err(error) => {
                let tool = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("(unknown)");
                tracing::warn!(tool, %error, "rejecting tool with invalid x-mcp-header annotation");
                events.emit(RuntimeEvent::Info {
                    app_id: app_id.to_owned(),
                    message: format!(
                        "rejected tool '{}' from discovery: invalid x-mcp-header annotation ({})",
                        tool, error
                    ),
                });
                false
            }
        }
    });
}

// ---------------------------------------------------------------------------
// MCP 2026-07-28 operation driver (shared by stdio and streamable HTTP)
// ---------------------------------------------------------------------------

/// Transport adapter used by [`drive_modern_operation`]: sends one modern
/// JSON-RPC request and allocates request IDs from the client's counter.
#[async_trait]
trait ModernSender: Send + Sync {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse>;
    async fn allocate_request_id(&self) -> u64;
}

/// Execute one modern operation end-to-end: send the prepared request,
/// resolve MRTR `input_required` interim results by fulfilling the
/// embedded input requests and retrying with a fresh request ID, and
/// resolve tasks-extension `task` results by polling `tasks/get`
/// (answering `input_required` task states via `tasks/update`).
async fn drive_modern_operation(
    protocol: &ProtocolEngine,
    session: &McpClientSession,
    operation: &McpOperation,
    mut request: JsonRpcRequest,
    sender: &dyn ModernSender,
    handler: &OperationMessageHandler,
) -> Result<McpOperationResult> {
    let mut response = sender.send(&request).await?;
    let mut mrtr_rounds = 0usize;
    loop {
        if response.error.is_some() {
            return decode_modern_response(protocol, session, operation, sender, handler, response)
                .await;
        }
        let result = response.result.clone().unwrap_or_else(|| json!({}));
        match modern_result_kind(&result) {
            ModernResultKind::Complete => {
                return decode_modern_response(
                    protocol, session, operation, sender, handler, response,
                )
                .await;
            }
            ModernResultKind::InputRequired => {
                mrtr_rounds += 1;
                if mrtr_rounds > MAX_MRTR_ROUNDS {
                    return Err(anyhow!(
                        "server kept requesting additional input after {} round trips",
                        MAX_MRTR_ROUNDS
                    ));
                }
                let interim = parse_input_required(&result)?;
                let input_responses = handler.fulfill_input_requests(&interim.input_requests)?;
                let new_id = sender.allocate_request_id().await;
                attach_input_responses(
                    &mut request,
                    new_id,
                    input_responses,
                    interim.request_state.as_deref(),
                );
                response = sender.send(&request).await?;
            }
            ModernResultKind::Task => {
                let task = crate::mcp::protocol::parse_modern_task(&result)?;
                if let McpOperation::InvokeAction {
                    capability,
                    arguments,
                    background: true,
                    ..
                } = operation
                {
                    return Ok(McpOperationResult::TaskAccepted {
                        message: format!(
                            "{} accepted as background task ({})",
                            capability, task.task_id
                        ),
                        remote_task_id: Some(task.task_id.clone()),
                        detail: json!({
                            "capability": capability,
                            "arguments": arguments,
                            "task": task.raw,
                        }),
                    });
                }
                let final_task = poll_modern_task(protocol, session, task, sender, handler).await?;
                return finish_modern_task(operation, final_task);
            }
        }
    }
}

/// Poll a tasks-extension task to a terminal state, answering
/// `input_required` states via `tasks/update` and honouring the server's
/// suggested `pollIntervalMs` (clamped to sane bounds).
async fn poll_modern_task(
    protocol: &ProtocolEngine,
    session: &McpClientSession,
    mut task: crate::mcp::protocol::ModernTask,
    sender: &dyn ModernSender,
    handler: &OperationMessageHandler,
) -> Result<crate::mcp::protocol::ModernTask> {
    let mut polls = 0usize;
    loop {
        if task.is_terminal() {
            return Ok(task);
        }
        if task.status == "input_required" && !task.input_requests.is_empty() {
            let input_responses = handler.fulfill_input_requests(&task.input_requests)?;
            let id = sender.allocate_request_id().await;
            let mut update = JsonRpcRequest::new(
                JsonRpcId::Number(id),
                "tasks/update",
                Some(json!({
                    "taskId": task.task_id,
                    "inputResponses": input_responses,
                })),
            );
            protocol.inject_modern_meta(&mut update, session);
            let response = sender.send(&update).await?;
            if let Some(error) = response.error {
                return Err(anyhow!(
                    "tasks/update for '{}' failed: {}",
                    task.task_id,
                    error.message
                ));
            }
        } else {
            let interval = task
                .poll_interval_ms
                .unwrap_or(DEFAULT_TASK_POLL_MS)
                .clamp(MIN_TASK_POLL_MS, MAX_TASK_POLL_MS);
            sleep(Duration::from_millis(interval)).await;
        }

        polls += 1;
        if polls > MAX_TASK_POLLS {
            return Err(anyhow!(
                "task '{}' did not reach a terminal state after {} polls",
                task.task_id,
                MAX_TASK_POLLS
            ));
        }
        let id = sender.allocate_request_id().await;
        let mut get = JsonRpcRequest::new(
            JsonRpcId::Number(id),
            "tasks/get",
            Some(json!({ "taskId": task.task_id })),
        );
        protocol.inject_modern_meta(&mut get, session);
        let response = sender.send(&get).await?;
        if let Some(error) = response.error {
            return Err(anyhow!(
                "tasks/get for '{}' failed: {}",
                task.task_id,
                error.message
            ));
        }
        let result = response
            .result
            .ok_or_else(|| anyhow!("tasks/get did not return a result"))?;
        task = crate::mcp::protocol::parse_modern_task(&result)?;
    }
}

/// Turn a terminal tasks-extension task into the operation's result: a
/// completed task's `result` field carries exactly what the original
/// request would have returned synchronously.
fn finish_modern_task(
    operation: &McpOperation,
    task: crate::mcp::protocol::ModernTask,
) -> Result<McpOperationResult> {
    match task.status.as_str() {
        "completed" => {
            let inner = task.result.clone().unwrap_or_else(|| json!({}));
            let synthetic = JsonRpcResponse {
                jsonrpc: "2.0".to_owned(),
                id: JsonRpcId::String(format!("task-{}", task.task_id)),
                result: Some(inner),
                error: None,
            };
            map_streamable_http_response(operation, synthetic)
        }
        "failed" => {
            let detail = task
                .error
                .as_ref()
                .and_then(|error| error.get("message").and_then(Value::as_str))
                .or(task.status_message.as_deref())
                .unwrap_or("task failed");
            Err(anyhow!("task '{}' failed: {}", task.task_id, detail))
        }
        other => Err(anyhow!("task '{}' ended as '{}'", task.task_id, other)),
    }
}

/// Decode a modern complete/error response into an operation result.
/// Most operations share the legacy decoders (extra fields such as
/// `resultType`, `ttlMs` and `cacheScope` are ignored); the exceptions
/// are the era-specific method mappings.
async fn decode_modern_response(
    protocol: &ProtocolEngine,
    session: &McpClientSession,
    operation: &McpOperation,
    sender: &dyn ModernSender,
    handler: &OperationMessageHandler,
    response: JsonRpcResponse,
) -> Result<McpOperationResult> {
    if let Some(error) = &response.error {
        let hint = if error.code == crate::mcp::protocol::ERROR_UNSUPPORTED_PROTOCOL_VERSION {
            let supported = error
                .data
                .as_ref()
                .and_then(|data| data.get("supported"))
                .map(|value| format!("; server supports {}", value))
                .unwrap_or_default();
            format!(" (unsupported protocol version{})", supported)
        } else {
            String::new()
        };
        return Err(anyhow!(
            "json-rpc error {}: {}{}",
            error.code,
            error.message,
            hint
        ));
    }

    match operation {
        // Modern liveness: `ping` was removed, `server/discover` answered.
        McpOperation::Ping => {
            let server = response
                .result
                .as_ref()
                .and_then(|result| result.get("serverInfo"))
                .and_then(|info| info.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("server");
            Ok(McpOperationResult::Pong {
                message: format!("{} is alive (answered server/discover)", server),
            })
        }
        McpOperation::TaskGet { task_id } | McpOperation::TaskResult { task_id } => {
            let result = response
                .result
                .ok_or_else(|| anyhow!("tasks/get did not return a result"))?;
            let mut task = parse_modern_task(&result)?;
            // Fulfil pending input requests so `jobs show/wait` keep a
            // paused task moving, then re-read the state once.
            if task.status == "input_required" && !task.input_requests.is_empty() {
                let input_responses = handler.fulfill_input_requests(&task.input_requests)?;
                let id = sender.allocate_request_id().await;
                let mut update = JsonRpcRequest::new(
                    JsonRpcId::Number(id),
                    "tasks/update",
                    Some(json!({ "taskId": task.task_id, "inputResponses": input_responses })),
                );
                protocol.inject_modern_meta(&mut update, session);
                sender.send(&update).await?;
                let id = sender.allocate_request_id().await;
                let mut get = JsonRpcRequest::new(
                    JsonRpcId::Number(id),
                    "tasks/get",
                    Some(json!({ "taskId": task.task_id })),
                );
                protocol.inject_modern_meta(&mut get, session);
                let refreshed = sender.send(&get).await?;
                if let Some(result) = refreshed.result {
                    task = parse_modern_task(&result)?;
                }
            }
            let failure_reason = task
                .error
                .as_ref()
                .and_then(|error| error.get("message").and_then(Value::as_str))
                .map(ToOwned::to_owned);
            Ok(McpOperationResult::Task {
                status: parse_task_state(&task.status),
                message: format!("task {} is {}", task_id, task.status),
                remote_task_id: task.task_id.clone(),
                data: task.raw.clone(),
                result: task.result.clone(),
                failure_reason,
            })
        }
        _ => map_streamable_http_response(operation, response),
    }
}

struct HttpJsonRpcResponse {
    response: JsonRpcResponse,
    session_id: Option<String>,
    protocol_version: Option<String>,
}

struct HttpTransportResponse {
    status: StatusCode,
    content_type: Option<String>,
    session_id: Option<String>,
    protocol_version: Option<String>,
    body: Bytes,
}

impl HttpTransportResponse {
    fn into_jsonrpc_response(
        self,
        handler: Option<&OperationMessageHandler>,
    ) -> Result<JsonRpcResponse> {
        if self.status != StatusCode::OK {
            let body = String::from_utf8_lossy(&self.body);
            return Err(anyhow!(
                "unexpected HTTP status {} from streamable MCP endpoint: {}",
                self.status,
                body.trim()
            ));
        }

        let body = String::from_utf8(self.body.to_vec()).map_err(|error| {
            anyhow!(
                "streamable HTTP response body was not valid UTF-8: {}",
                error
            )
        })?;
        match self.content_type.as_deref() {
            Some(content_type) if content_type.starts_with("application/json") => {
                serde_json::from_str(&body)
                    .map_err(|error| anyhow!("failed to decode JSON-RPC response body: {}", error))
            }
            Some(content_type) if content_type.starts_with("text/event-stream") => {
                parse_sse_jsonrpc_response(&body, handler)
            }
            Some(content_type) => Err(anyhow!(
                "unsupported streamable HTTP response content type '{}'",
                content_type
            )),
            None => Err(anyhow!(
                "streamable HTTP response was missing a content type"
            )),
        }
    }
}

impl DemoMcpClient {
    pub async fn load(path: PathBuf) -> Result<Self> {
        let state = if path.exists() {
            let bytes = fs::read(&path).await.map_err(|error| {
                anyhow!(
                    "failed to read demo remote state '{}': {}",
                    path.display(),
                    error
                )
            })?;
            serde_json::from_slice(&bytes).map_err(|error| {
                anyhow!(
                    "failed to parse demo remote state '{}': {}",
                    path.display(),
                    error
                )
            })?
        } else {
            DemoClientState::default()
        };

        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    async fn persist_state(&self, state: &DemoClientState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await.map_err(|error| {
                anyhow!(
                    "failed to create demo remote state directory '{}': {}",
                    parent.display(),
                    error
                )
            })?;
        }
        let bytes = serde_json::to_vec_pretty(state)
            .map_err(|error| anyhow!("failed to serialize demo remote state: {}", error))?;
        fs::write(&self.path, bytes).await.map_err(|error| {
            anyhow!(
                "failed to write demo remote state '{}': {}",
                self.path.display(),
                error
            )
        })
    }
}

#[async_trait]
impl McpClient for DemoMcpClient {
    async fn metadata(&self, app_id: &str) -> Result<ConnectionMetadata> {
        Ok(ConnectionMetadata {
            app_id: app_id.to_owned(),
            server_name: format!("{}-demo-server", app_id),
            server_version: "2026.03.25".to_owned(),
            transport: TransportKind::StreamableHttp,
        })
    }

    async fn negotiated_session(&self) -> Option<McpClientSession> {
        None
    }

    async fn perform(
        &self,
        app_id: &str,
        operation: McpOperation,
        events: &EventBroker,
        _inventory_stale_path: Option<&PathBuf>,
    ) -> Result<McpOperationResult> {
        match operation {
            McpOperation::InvokeAction {
                capability,
                arguments,
                background,
                ..
            } => {
                events.emit(RuntimeEvent::Progress {
                    app_id: app_id.to_owned(),
                    operation: capability.clone(),
                    current: 1,
                    total: Some(2),
                    message: "accepted by demo client".to_owned(),
                });
                sleep(Duration::from_millis(10)).await;
                events.emit(RuntimeEvent::Progress {
                    app_id: app_id.to_owned(),
                    operation: capability.clone(),
                    current: 2,
                    total: Some(2),
                    message: "demo execution finished".to_owned(),
                });

                let summary = match arguments.as_object() {
                    Some(arguments) if !arguments.is_empty() => {
                        format!(
                            "{} invoked with {} argument(s)",
                            capability,
                            arguments.len()
                        )
                    }
                    _ => format!("{} invoked", capability),
                };

                if background {
                    let remote_task_id =
                        format!("{}-{}", capability.replace('.', "-"), Uuid::new_v4());
                    let task = DemoTaskState {
                        app_id: app_id.to_owned(),
                        capability: capability.clone(),
                        status: TaskState::Queued,
                        summary: summary.clone(),
                        arguments: arguments.clone(),
                        result: None,
                        failure_reason: None,
                    };
                    {
                        let mut state = self.state.lock().await;
                        state.tasks.insert(remote_task_id.clone(), task);
                        self.persist_state(&state).await?;
                    }
                    return Ok(McpOperationResult::TaskAccepted {
                        message: format!("{} is running in the background", capability),
                        remote_task_id: Some(remote_task_id),
                        detail: json!({
                            "capability": capability,
                            "summary": summary,
                        }),
                    });
                }

                Ok(McpOperationResult::Action {
                    message: format!("{} completed", capability),
                    data: json!({
                        "capability": capability,
                        "summary": summary,
                        "arguments": arguments,
                    }),
                })
            }
            McpOperation::ReadResource { uri } => {
                let message = format!("read '{}' via demo resource", uri);
                events.emit(RuntimeEvent::Info {
                    app_id: app_id.to_owned(),
                    message: message.clone(),
                });

                let (mime_type, text, data) = if uri.ends_with(".json") {
                    (
                        Some("application/json".to_owned()),
                        None,
                        json!({
                            "uri": uri,
                            "kind": "demo_json_resource",
                            "items": [
                                { "id": 1, "label": "alpha" },
                                { "id": 2, "label": "beta" }
                            ]
                        }),
                    )
                } else {
                    let text = format!("demo resource content for {}", uri);
                    (
                        Some("text/plain".to_owned()),
                        Some(text.clone()),
                        json!({
                            "uri": uri,
                            "kind": "demo_text_resource",
                            "text": text,
                        }),
                    )
                };

                Ok(McpOperationResult::Resource {
                    message,
                    uri,
                    mime_type,
                    text,
                    data,
                })
            }
            McpOperation::Discover { category } => {
                let items = demo_discovery_items(&category);
                let message = format!(
                    "discovered {} {} via demo server",
                    items.len(),
                    category.as_str()
                );
                events.emit(RuntimeEvent::Info {
                    app_id: app_id.to_owned(),
                    message: message.clone(),
                });
                Ok(McpOperationResult::Discovery {
                    message,
                    category,
                    items,
                })
            }
            McpOperation::RunPrompt { name, arguments } => {
                let argument_count = arguments.as_object().map(|value| value.len()).unwrap_or(0);
                let output = if argument_count == 0 {
                    format!("demo prompt '{}' rendered with no arguments", name)
                } else {
                    format!(
                        "demo prompt '{}' rendered with {} argument(s)",
                        name, argument_count
                    )
                };
                events.emit(RuntimeEvent::Info {
                    app_id: app_id.to_owned(),
                    message: format!("executed prompt '{}' via demo client", name),
                });

                Ok(McpOperationResult::Prompt {
                    message: format!("{} completed", name),
                    name: name.clone(),
                    output: output.clone(),
                    data: json!({
                        "name": name,
                        "output": output,
                        "arguments": arguments,
                    }),
                })
            }
            McpOperation::Ping => Ok(McpOperationResult::Pong {
                message: "demo server is alive".to_owned(),
            }),
            McpOperation::SetLoggingLevel { level } => Ok(McpOperationResult::LoggingLevelSet {
                message: format!("demo logging level set to '{}'", level),
                level,
            }),
            McpOperation::Complete {
                argument_name,
                argument_value,
                ..
            } => {
                // Demo: return some fake completions
                let values = vec![
                    format!("{}alpha", argument_value),
                    format!("{}beta", argument_value),
                ];
                Ok(McpOperationResult::Completion {
                    message: format!("demo completions for '{}'", argument_name),
                    values,
                    has_more: false,
                    total: Some(2),
                })
            }
            McpOperation::DiscoverResourceTemplates => Ok(McpOperationResult::Discovery {
                message: "discovered 0 resource templates via demo server".to_owned(),
                category: DiscoveryCategory::Resources,
                items: vec![],
            }),
            McpOperation::SubscribeResource { uri } => Ok(McpOperationResult::Subscribed {
                message: format!("demo subscribed to '{}'", uri),
                uri,
            }),
            McpOperation::UnsubscribeResource { uri } => Ok(McpOperationResult::Unsubscribed {
                message: format!("demo unsubscribed from '{}'", uri),
                uri,
            }),
            McpOperation::TaskGet { task_id } => {
                let state = self.state.lock().await;
                let task = state
                    .tasks
                    .get(&task_id)
                    .cloned()
                    .ok_or_else(|| anyhow!("remote task '{}' was not found", task_id))?;
                Ok(task_result(
                    &task_id,
                    &task,
                    format!("{} status fetched", task.capability),
                ))
            }
            McpOperation::TaskResult { task_id } => {
                let task = {
                    let mut state = self.state.lock().await;
                    let task = state
                        .tasks
                        .get_mut(&task_id)
                        .ok_or_else(|| anyhow!("remote task '{}' was not found", task_id))?;
                    if matches!(task.status, TaskState::Queued | TaskState::Running) {
                        task.status = TaskState::Running;
                    }
                    let snapshot = task.clone();
                    self.persist_state(&state).await?;
                    snapshot
                };

                if task.status == TaskState::Running {
                    events.emit(RuntimeEvent::Progress {
                        app_id: task.app_id.clone(),
                        operation: task.capability.clone(),
                        current: 1,
                        total: Some(1),
                        message: "waiting for remote task completion".to_owned(),
                    });
                    sleep(Duration::from_millis(25)).await;
                    let updated = {
                        let mut state = self.state.lock().await;
                        let task = state
                            .tasks
                            .get_mut(&task_id)
                            .ok_or_else(|| anyhow!("remote task '{}' was not found", task_id))?;
                        if task.status == TaskState::Running {
                            if task.arguments.get("demo_fail") == Some(&json!(true)) {
                                task.status = TaskState::Failed;
                                task.result = None;
                                task.failure_reason = Some(
                                    "demo failure triggered by argument demo_fail=true".to_owned(),
                                );
                            } else {
                                task.status = TaskState::Completed;
                                task.failure_reason = None;
                                task.result = Some(json!({
                                    "capability": task.capability,
                                    "summary": task.summary,
                                    "arguments": task.arguments,
                                    "remote_task_id": task_id,
                                }));
                            }
                        }
                        let snapshot = task.clone();
                        self.persist_state(&state).await?;
                        snapshot
                    };
                    return Ok(task_result(
                        &task_id,
                        &updated,
                        format!("{} is {}", updated.capability, updated.status.as_str()),
                    ));
                }

                Ok(task_result(
                    &task_id,
                    &task,
                    format!("{} remains {}", task.capability, task.status.as_str()),
                ))
            }
            McpOperation::TaskCancel { task_id } => {
                let updated = {
                    let mut state = self.state.lock().await;
                    let task = state
                        .tasks
                        .get_mut(&task_id)
                        .ok_or_else(|| anyhow!("remote task '{}' was not found", task_id))?;
                    if matches!(task.status, TaskState::Queued | TaskState::Running) {
                        task.status = TaskState::Canceled;
                        task.result = None;
                        task.failure_reason = Some("task canceled by operator".to_owned());
                    }
                    let snapshot = task.clone();
                    self.persist_state(&state).await?;
                    snapshot
                };
                Ok(task_result(
                    &task_id,
                    &updated,
                    format!("{} is {}", updated.capability, updated.status.as_str()),
                ))
            }
        }
    }
}

#[async_trait]
impl McpClient for StdioMcpClient {
    async fn metadata(&self, app_id: &str) -> Result<ConnectionMetadata> {
        let session = self.session.lock().await;
        let server_name = session
            .server_info
            .as_ref()
            .map(|value| value.name.clone())
            .unwrap_or_else(|| self.command_display());
        let server_version = session
            .server_info
            .as_ref()
            .map(|value| value.version.clone())
            .unwrap_or_else(|| "unknown".to_owned());

        Ok(ConnectionMetadata {
            app_id: app_id.to_owned(),
            server_name,
            server_version,
            transport: TransportKind::Stdio,
        })
    }

    async fn negotiated_session(&self) -> Option<McpClientSession> {
        let session = self.session.lock().await;
        session.initialized.then(|| session.clone())
    }

    async fn perform(
        &self,
        app_id: &str,
        operation: McpOperation,
        events: &EventBroker,
        inventory_stale_path: Option<&PathBuf>,
    ) -> Result<McpOperationResult> {
        let handler = OperationMessageHandler {
            app_id: app_id.to_owned(),
            events: events.clone(),
            inventory_stale_path: inventory_stale_path.cloned(),
            roots: Vec::new(),
        };

        self.ensure_ready(app_id, events, &handler).await?;
        let session_snapshot = {
            let session = self.session.lock().await;
            session.clone()
        };

        if session_snapshot.era == ProtocolEra::Modern {
            if let Some(result) = modern_offline_result(&operation) {
                return Ok(result);
            }
            if let McpOperation::SubscribeResource { uri } = &operation {
                return self
                    .modern_subscribe(uri, &session_snapshot, &handler)
                    .await;
            }
            let request_id = self.allocate_request_id().await;
            let prepared =
                self.protocol
                    .prepare_operation(&session_snapshot, request_id, &operation)?;
            let sender = StdioModernSender {
                client: self,
                handler: &handler,
            };
            return drive_modern_operation(
                &self.protocol,
                &session_snapshot,
                &operation,
                prepared.request,
                &sender,
                &handler,
            )
            .await;
        }

        let request_id = self.allocate_request_id().await;
        let prepared = {
            let session = self.session.lock().await;
            self.protocol
                .prepare_operation(&session, request_id, &operation)?
        };
        events.emit(RuntimeEvent::Info {
            app_id: app_id.to_owned(),
            message: format!(
                "selected stdio client for '{}' with {} prepared message(s)",
                self.config_name,
                prepared.outbound_message_count(),
            ),
        });

        if let Some(initialize) = &prepared.initialize {
            let initialize_response = self.send_jsonrpc_request(initialize, None).await?;
            let initialize_result: InitializeResult = initialize_response.into_result()?;

            {
                let mut session = self.session.lock().await;
                self.protocol
                    .complete_initialize(&mut session, initialize_result, None);
            }

            if let Some(notification) = &prepared.initialized_notification {
                self.send_notification(notification).await?;
            }
        }

        let response = self
            .send_jsonrpc_request(&prepared.request, Some(&handler))
            .await?;
        map_streamable_http_response(&operation, response)
    }

    async fn cancel_request(&self, request_id: u64, reason: Option<&str>) -> Result<()> {
        let mut params = json!({ "requestId": request_id });
        if let Some(reason) = reason {
            params["reason"] = json!(reason);
        }
        let notification = JsonRpcNotification::new("notifications/cancelled", Some(params));
        self.send_notification(&notification).await
    }
}

#[async_trait]
impl McpClient for StreamableHttpMcpClient {
    async fn metadata(&self, app_id: &str) -> Result<ConnectionMetadata> {
        let session = self.session.lock().await;
        let server_name = session
            .server_info
            .as_ref()
            .map(|value| value.name.clone())
            .or_else(|| self.endpoint.host_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "remote-mcp-server".to_owned());
        let server_version = session
            .server_info
            .as_ref()
            .map(|value| value.version.clone())
            .unwrap_or_else(|| "unknown".to_owned());

        Ok(ConnectionMetadata {
            app_id: app_id.to_owned(),
            server_name,
            server_version,
            transport: TransportKind::StreamableHttp,
        })
    }

    async fn negotiated_session(&self) -> Option<McpClientSession> {
        let session = self.session.lock().await;
        session.initialized.then(|| session.clone())
    }

    async fn perform(
        &self,
        app_id: &str,
        operation: McpOperation,
        events: &EventBroker,
        inventory_stale_path: Option<&PathBuf>,
    ) -> Result<McpOperationResult> {
        let handler = OperationMessageHandler {
            app_id: app_id.to_owned(),
            events: events.clone(),
            inventory_stale_path: inventory_stale_path.cloned(),
            roots: Vec::new(),
        };

        self.ensure_ready(app_id, events).await?;
        let modern_session = {
            let session = self.session.lock().await;
            (session.era == ProtocolEra::Modern).then(|| session.clone())
        };

        if let Some(session_snapshot) = modern_session {
            if let Some(result) = modern_offline_result(&operation) {
                return Ok(result);
            }
            if let McpOperation::SubscribeResource { uri } = &operation {
                return self
                    .modern_subscribe(uri, &session_snapshot, &handler)
                    .await;
            }
            let param_headers = if let McpOperation::InvokeAction {
                arguments,
                input_schema,
                ..
            } = &operation
            {
                extract_param_headers(input_schema.as_ref(), arguments)?
            } else {
                Vec::new()
            };
            let request_id = self.allocate_request_id().await;
            let prepared =
                self.protocol
                    .prepare_operation(&session_snapshot, request_id, &operation)?;
            let sender = HttpModernSender {
                client: self,
                handler: &handler,
                protocol_version: session_snapshot.protocol_version.clone(),
                param_headers,
            };
            let mut result = drive_modern_operation(
                &self.protocol,
                &session_snapshot,
                &operation,
                prepared.request,
                &sender,
                &handler,
            )
            .await?;
            // MCP 2026-07-28 requires Streamable HTTP clients to exclude
            // tools whose x-mcp-header annotations are invalid.
            if matches!(
                &operation,
                McpOperation::Discover {
                    category: DiscoveryCategory::Capabilities
                }
            ) && let McpOperationResult::Discovery { items, .. } = &mut result
            {
                reject_invalid_header_tools(items, events, app_id);
            }
            return Ok(result);
        }

        let request_id = self.allocate_request_id().await;
        let prepared = {
            let session = self.session.lock().await;
            self.protocol
                .prepare_operation(&session, request_id, &operation)?
        };
        events.emit(RuntimeEvent::Info {
            app_id: app_id.to_owned(),
            message: format!(
                "selected streamable HTTP client for '{}' at '{}' with {} prepared message(s)",
                self.config_name,
                self.endpoint_display(),
                prepared.outbound_message_count(),
            ),
        });

        if let Some(initialize) = &prepared.initialize {
            let session_snapshot = {
                let session = self.session.lock().await;
                session.clone()
            };
            let initialize_response = self
                .send_jsonrpc_request(initialize, &session_snapshot, None)
                .await?;
            let initialize_result: InitializeResult = initialize_response.response.into_result()?;

            {
                let mut session = self.session.lock().await;
                self.protocol.complete_initialize(
                    &mut session,
                    initialize_result,
                    initialize_response.session_id.clone(),
                );
                if let Some(protocol_version) = initialize_response.protocol_version {
                    session.protocol_version = protocol_version;
                }
            }

            if let Some(notification) = &prepared.initialized_notification {
                let initialized_session = {
                    let session = self.session.lock().await;
                    session.clone()
                };
                self.send_notification(notification, &initialized_session)
                    .await?;
            }
        }

        let session_snapshot = {
            let session = self.session.lock().await;
            session.clone()
        };
        let response = self
            .send_jsonrpc_request(&prepared.request, &session_snapshot, Some(&handler))
            .await?;
        if let Some(protocol_version) = response.protocol_version {
            let mut session = self.session.lock().await;
            session.protocol_version = protocol_version;
        }
        map_streamable_http_response(&operation, response.response)
    }

    async fn cancel_request(&self, request_id: u64, reason: Option<&str>) -> Result<()> {
        let mut params = json!({ "requestId": request_id });
        if let Some(reason) = reason {
            params["reason"] = json!(reason);
        }
        let notification = JsonRpcNotification::new("notifications/cancelled", Some(params));
        let session_snapshot = {
            let session = self.session.lock().await;
            session.clone()
        };
        self.send_notification(&notification, &session_snapshot)
            .await
    }
}

fn map_streamable_http_response(
    operation: &McpOperation,
    response: JsonRpcResponse,
) -> Result<McpOperationResult> {
    match operation {
        McpOperation::Discover { category } => map_discovery_response(category, response),
        McpOperation::InvokeAction {
            capability,
            arguments,
            background,
            ..
        } => map_tool_call_response(capability, arguments, *background, response),
        McpOperation::ReadResource { uri } => map_resource_read_response(uri, response),
        McpOperation::RunPrompt { name, arguments } => {
            map_prompt_get_response(name, arguments, response)
        }
        McpOperation::Ping => {
            // Ping succeeds if we get any response (even empty result)
            let _result = response.result; // May be {} or null
            Ok(McpOperationResult::Pong {
                message: "server is alive".to_owned(),
            })
        }
        McpOperation::SetLoggingLevel { level } => {
            // logging/setLevel returns empty result on success
            if let Some(error) = response.error {
                return Err(anyhow!("logging/setLevel failed: {}", error.message));
            }
            Ok(McpOperationResult::LoggingLevelSet {
                message: format!("logging level set to '{}'", level),
                level: level.clone(),
            })
        }
        McpOperation::Complete { argument_name, .. } => {
            let result = response
                .result
                .ok_or_else(|| anyhow!("completion/complete did not return a result"))?;
            let completion = result.get("completion").unwrap_or(&result);
            let values: Vec<String> = completion
                .get("values")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let has_more = completion
                .get("hasMore")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let total = completion.get("total").and_then(Value::as_u64);
            Ok(McpOperationResult::Completion {
                message: format!("{} completions for '{}'", values.len(), argument_name),
                values,
                has_more,
                total,
            })
        }
        McpOperation::DiscoverResourceTemplates => {
            let result = response
                .result
                .ok_or_else(|| anyhow!("resources/templates/list did not return a result"))?;
            let items: Vec<Value> = result
                .get("resourceTemplates")
                .and_then(Value::as_array)
                .unwrap_or(&Vec::new())
                .iter()
                .map(|template| {
                    let mut item = json!({
                        "uriTemplate": template.get("uriTemplate").cloned().unwrap_or_else(|| json!("(unknown)")),
                        "name": template.get("name").cloned(),
                        "title": template.get("title").cloned(),
                        "mime_type": template.get("mimeType").cloned(),
                        "description": template.get("description").cloned().unwrap_or_else(|| json!("(no description)")),
                        "kind": "resource_template",
                    });
                    if let Some(icons) = template.get("icons") {
                        item["icons"] = icons.clone();
                    }
                    item
                })
                .collect();
            Ok(McpOperationResult::Discovery {
                message: format!("discovered {} resource templates", items.len()),
                category: DiscoveryCategory::Resources,
                items,
            })
        }
        McpOperation::SubscribeResource { uri } => {
            if let Some(error) = response.error {
                return Err(anyhow!("resources/subscribe failed: {}", error.message));
            }
            Ok(McpOperationResult::Subscribed {
                message: format!("subscribed to '{}'", uri),
                uri: uri.clone(),
            })
        }
        McpOperation::UnsubscribeResource { uri } => {
            if let Some(error) = response.error {
                return Err(anyhow!("resources/unsubscribe failed: {}", error.message));
            }
            Ok(McpOperationResult::Unsubscribed {
                message: format!("unsubscribed from '{}'", uri),
                uri: uri.clone(),
            })
        }
        McpOperation::TaskGet { task_id } => {
            let result = response
                .result
                .ok_or_else(|| anyhow!("tasks/get did not return a result"))?;
            let status = result
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            let failure_reason = if status == "failed" {
                result
                    .get("error")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| {
                        result
                            .get("message")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                    })
            } else {
                None
            };
            Ok(McpOperationResult::Task {
                status: parse_task_state(&status),
                message: format!("task {} is {}", task_id, status),
                remote_task_id: task_id.clone(),
                data: result,
                result: None,
                failure_reason,
            })
        }
        McpOperation::TaskResult { task_id } => {
            let result = response
                .result
                .ok_or_else(|| anyhow!("tasks/result did not return a result"))?;
            let status = result
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("completed")
                .to_owned();
            let task_result = result.get("result").cloned();
            let failure_reason = result
                .get("error")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            Ok(McpOperationResult::Task {
                status: parse_task_state(&status),
                message: format!("task {} result retrieved", task_id),
                remote_task_id: task_id.clone(),
                data: result,
                result: task_result,
                failure_reason,
            })
        }
        McpOperation::TaskCancel { task_id } => {
            if let Some(error) = response.error {
                return Err(anyhow!("tasks/cancel failed: {}", error.message));
            }
            Ok(McpOperationResult::Task {
                status: crate::mcp::model::TaskState::Canceled,
                message: format!("task {} cancelled", task_id),
                remote_task_id: task_id.clone(),
                data: response.result.unwrap_or(json!({})),
                result: None,
                failure_reason: None,
            })
        }
    }
}

fn map_discovery_response(
    category: &DiscoveryCategory,
    response: JsonRpcResponse,
) -> Result<McpOperationResult> {
    if let Some(error) = response.error {
        return Err(anyhow!("discovery failed: {}", error.message));
    }
    let result = response
        .result
        .ok_or_else(|| anyhow!("json-rpc discovery response did not contain a result"))?;
    let items: Vec<Value> = match category {
        DiscoveryCategory::Capabilities => result
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("tools/list result did not contain tools"))?
            .iter()
            .map(|tool| {
                let mut item = json!({
                    "id": tool.get("name").and_then(Value::as_str).unwrap_or("(unknown)"),
                    "kind": "tool",
                    "description": tool
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("(no description)"),
                    "title": tool.get("title").cloned(),
                });
                // Preserve full inputSchema for dynamic CLI generation
                if let Some(schema) = tool.get("inputSchema") {
                    item["inputSchema"] = schema.clone();
                }
                // Preserve outputSchema for structured content validation/display
                if let Some(schema) = tool.get("outputSchema") {
                    item["outputSchema"] = schema.clone();
                }
                // Preserve annotations if present
                if let Some(annotations) = tool.get("annotations") {
                    item["annotations"] = annotations.clone();
                }
                // Preserve icons if present
                if let Some(icons) = tool.get("icons") {
                    item["icons"] = icons.clone();
                }
                // Preserve execution metadata (e.g. taskSupport)
                if let Some(execution) = tool.get("execution") {
                    item["execution"] = execution.clone();
                }
                item
            })
            .collect(),
        DiscoveryCategory::Resources => {
            let mut items = Vec::new();
            // Concrete resources
            if let Some(resources) = result.get("resources").and_then(Value::as_array) {
                for resource in resources {
                    let mut item = json!({
                        "uri": resource.get("uri").cloned().unwrap_or_else(|| json!("(unknown)")),
                        "name": resource.get("name").cloned(),
                        "title": resource.get("title").cloned(),
                        "mime_type": resource.get("mimeType").cloned(),
                        "description": resource
                            .get("description")
                            .cloned()
                            .unwrap_or_else(|| json!("(no description)")),
                        "kind": "resource",
                    });
                    if let Some(icons) = resource.get("icons") {
                        item["icons"] = icons.clone();
                    }
                    if let Some(annotations) = resource.get("annotations") {
                        item["annotations"] = annotations.clone();
                    }
                    items.push(item);
                }
            }
            // Resource templates (if embedded in same response)
            if let Some(templates) = result.get("resourceTemplates").and_then(Value::as_array) {
                for template in templates {
                    let mut item = json!({
                        "uriTemplate": template.get("uriTemplate").cloned().unwrap_or_else(|| json!("(unknown)")),
                        "name": template.get("name").cloned(),
                        "title": template.get("title").cloned(),
                        "mime_type": template.get("mimeType").cloned(),
                        "description": template
                            .get("description")
                            .cloned()
                            .unwrap_or_else(|| json!("(no description)")),
                        "kind": "resource_template",
                    });
                    if let Some(icons) = template.get("icons") {
                        item["icons"] = icons.clone();
                    }
                    items.push(item);
                }
            }
            if items.is_empty() {
                // Fallback: try old shape
                result
                    .get("resources")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow!("resources/list result did not contain resources"))?;
            }
            items
        }
        DiscoveryCategory::Prompts => result
            .get("prompts")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("prompts/list result did not contain prompts"))?
            .iter()
            .map(|prompt| {
                let mut item = json!({
                    "name": prompt.get("name").cloned().unwrap_or_else(|| json!("(unknown)")),
                    "title": prompt.get("title").cloned(),
                    "description": prompt
                        .get("description")
                        .cloned()
                        .unwrap_or_else(|| json!("(no description)")),
                    "arguments": prompt.get("arguments").cloned(),
                });
                if let Some(icons) = prompt.get("icons") {
                    item["icons"] = icons.clone();
                }
                item
            })
            .collect(),
    };

    Ok(McpOperationResult::Discovery {
        message: format!(
            "discovered {} {} via streamable HTTP",
            items.len(),
            category.as_str()
        ),
        category: category.clone(),
        items,
    })
}

fn map_tool_call_response(
    capability: &str,
    arguments: &Value,
    _background: bool,
    response: JsonRpcResponse,
) -> Result<McpOperationResult> {
    if let Some(error) = response.error {
        return Err(anyhow!("tools/call failed: {}", error.message));
    }
    let result = response
        .result
        .ok_or_else(|| anyhow!("json-rpc tools/call response did not contain a result"))?;

    // Check if the server returned a task-accepted response (task augmentation).
    // Per MCP 2025-11-25, when _meta.task is present in the result, the server
    // accepted the request as a background task.
    if let Some(meta) = result.get("_meta").and_then(|m| m.get("task")) {
        let task_id = meta
            .get("taskId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        return Ok(McpOperationResult::TaskAccepted {
            message: format!(
                "{} accepted as background task{}",
                capability,
                task_id
                    .as_ref()
                    .map(|id| format!(" ({})", id))
                    .unwrap_or_default()
            ),
            remote_task_id: task_id,
            detail: json!({
                "capability": capability,
                "arguments": arguments,
                "meta": meta,
            }),
        });
    }

    let summary = tool_call_summary(capability, &result);
    Ok(McpOperationResult::Action {
        message: format!("{} completed", capability),
        data: json!({
            "capability": capability,
            "summary": summary,
            "arguments": arguments,
            "result": result,
        }),
    })
}

fn map_resource_read_response(uri: &str, response: JsonRpcResponse) -> Result<McpOperationResult> {
    if let Some(error) = response.error {
        return Err(anyhow!("resources/read failed: {}", error.message));
    }
    let result = response
        .result
        .ok_or_else(|| anyhow!("json-rpc resources/read response did not contain a result"))?;
    let contents = result
        .get("contents")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("resources/read result did not contain contents"))?;
    let first = contents
        .first()
        .ok_or_else(|| anyhow!("resources/read returned no contents"))?;

    let text = first
        .get("text")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let mime_type = first
        .get("mimeType")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let data = if let Some(blob) = first.get("blob") {
        json!({
            "uri": first.get("uri").cloned().unwrap_or_else(|| json!(uri)),
            "mimeType": mime_type,
            "blob": blob,
        })
    } else {
        first.clone()
    };

    Ok(McpOperationResult::Resource {
        message: format!("read '{}' via streamable HTTP", uri),
        uri: first
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or(uri)
            .to_owned(),
        mime_type,
        text,
        data,
    })
}

fn map_prompt_get_response(
    name: &str,
    arguments: &Value,
    response: JsonRpcResponse,
) -> Result<McpOperationResult> {
    if let Some(error) = response.error {
        return Err(anyhow!("prompts/get failed: {}", error.message));
    }
    let result = response
        .result
        .ok_or_else(|| anyhow!("json-rpc prompts/get response did not contain a result"))?;
    let output = prompt_output_from_result(&result);

    Ok(McpOperationResult::Prompt {
        message: format!("{} completed", name),
        name: name.to_owned(),
        output: output.clone(),
        data: json!({
            "name": name,
            "arguments": arguments,
            "output": output,
            "result": result,
        }),
    })
}

fn tool_call_summary(capability: &str, result: &Value) -> String {
    // Prefer structuredContent — render as pretty JSON
    if let Some(sc) = result.get("structuredContent") {
        return serde_json::to_string_pretty(sc)
            .unwrap_or_else(|_| format!("{} returned structured content", capability));
    }
    // Collect all content items
    let content = result
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut parts: Vec<String> = Vec::new();
    for item in &content {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("text");
        match item_type {
            "text" => {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    parts.push(text.to_owned());
                }
            }
            "resource_link" => {
                let uri = item.get("uri").and_then(Value::as_str).unwrap_or("?");
                let name = item.get("name").and_then(Value::as_str);
                let mime = item.get("mimeType").and_then(Value::as_str);
                let mut link = String::new();
                link.push_str("→ ");
                if let Some(n) = name {
                    link.push_str(n);
                    link.push_str(&format!(" ({})", uri));
                } else {
                    link.push_str(uri);
                }
                if let Some(m) = mime {
                    link.push_str(&format!(" [{}]", m));
                }
                parts.push(link);
            }
            "image" => {
                let mime = item
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .unwrap_or("image/*");
                let size = item
                    .get("data")
                    .and_then(Value::as_str)
                    .map(|d| d.len())
                    .unwrap_or(0);
                parts.push(format!("[image: {}, ~{} bytes base64]", mime, size));
            }
            "audio" => {
                let mime = item
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .unwrap_or("audio/*");
                let size = item
                    .get("data")
                    .and_then(Value::as_str)
                    .map(|d| d.len())
                    .unwrap_or(0);
                parts.push(format!("[audio: {}, ~{} bytes base64]", mime, size));
            }
            "resource" => {
                if let Some(res) = item.get("resource") {
                    let uri = res.get("uri").and_then(Value::as_str).unwrap_or("?");
                    let text_preview = res.get("text").and_then(Value::as_str);
                    if let Some(text) = text_preview {
                        let preview: String = text.chars().take(200).collect();
                        parts.push(format!("[resource: {}]\n{}", uri, preview));
                    } else {
                        parts.push(format!("[resource: {}]", uri));
                    }
                }
            }
            _ => {
                // Unknown content type — show as JSON
                parts.push(serde_json::to_string(item).unwrap_or_default());
            }
        }
    }
    if !parts.is_empty() {
        return parts.join("\n");
    }
    format!("{} completed", capability)
}

fn prompt_output_from_result(result: &Value) -> String {
    let Some(messages) = result.get("messages").and_then(Value::as_array) else {
        return serde_json::to_string_pretty(result)
            .unwrap_or_else(|_| "<invalid-json>".to_owned());
    };

    let mut parts = Vec::new();
    for message in messages {
        collect_prompt_text_blocks(message.get("content"), &mut parts);
    }

    if parts.is_empty() {
        serde_json::to_string_pretty(result).unwrap_or_else(|_| "<invalid-json>".to_owned())
    } else {
        parts.join("\n")
    }
}

fn collect_prompt_text_blocks(content: Option<&Value>, output: &mut Vec<String>) {
    let Some(content) = content else {
        return;
    };

    match content {
        Value::Array(items) => {
            for item in items {
                collect_prompt_text_blocks(Some(item), output);
            }
        }
        Value::Object(object) => {
            if let Some(text) = object.get("text").and_then(Value::as_str) {
                output.push(text.to_owned());
            }
        }
        _ => {}
    }
}

fn parse_sse_jsonrpc_response(
    body: &str,
    handler: Option<&OperationMessageHandler>,
) -> Result<JsonRpcResponse> {
    for event in body.split("\n\n") {
        let payload = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>();
        if payload.is_empty() {
            continue;
        }

        let joined = payload.join("\n");
        let value: Value = match serde_json::from_str(&joined) {
            Ok(value) => value,
            Err(_) => continue,
        };

        let has_method = value.get("method").is_some();
        let has_id = value.get("id").is_some();

        // Server→client notification (method, no id)
        if has_method && !has_id {
            if let Some(handler) = handler {
                let method = value["method"].as_str().unwrap_or("");
                handler.handle_notification(method, value.get("params"));
            }
            continue;
        }

        // Server→client request (method + id) — handle and log result
        // (HTTP SSE doesn't support sending responses back inline, but we
        // still dispatch the handler for notifications/logging purposes)
        if has_method && has_id {
            if let Some(handler) = handler
                && let Ok(request) = serde_json::from_value::<JsonRpcRequest>(value.clone())
            {
                // Note: for full HTTP server→client request support, the response
                // would need to be POSTed back. For now, we handle it locally.
                let _ = handler.handle_request(&request);
            }
            continue;
        }

        // JSON-RPC response
        if has_id && (value.get("result").is_some() || value.get("error").is_some()) {
            return serde_json::from_value(value).map_err(|error| {
                anyhow!("failed to decode JSON-RPC response from SSE: {}", error)
            });
        }
    }

    Err(anyhow!(
        "SSE response did not contain a JSON-RPC response event"
    ))
}

fn parse_task_state(status: &str) -> crate::mcp::model::TaskState {
    match status {
        "queued" => crate::mcp::model::TaskState::Queued,
        "running" | "working" => crate::mcp::model::TaskState::Running,
        "input_required" => crate::mcp::model::TaskState::InputRequired,
        "completed" => crate::mcp::model::TaskState::Completed,
        "canceled" | "cancelled" => crate::mcp::model::TaskState::Canceled,
        "failed" => crate::mcp::model::TaskState::Failed,
        _ => crate::mcp::model::TaskState::Running, // default to running for unknown
    }
}

fn demo_discovery_items(category: &DiscoveryCategory) -> Vec<serde_json::Value> {
    match category {
        DiscoveryCategory::Capabilities => vec![
            json!({
                "id": "tools.echo",
                "kind": "tool",
                "description": "Echo-style action for request/response validation",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "message": { "type": "string", "description": "Message to echo" }
                    },
                    "required": ["message"]
                }
            }),
            json!({
                "id": "tasks.run",
                "kind": "tool",
                "description": "Task-oriented execution surface with optional background support",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "request.mode": { "type": "string", "description": "Execution mode" },
                        "request.id": { "type": "integer", "description": "Request identifier" }
                    }
                }
            }),
        ],
        DiscoveryCategory::Resources => vec![
            json!({
                "uri": "resources/files/readme.txt",
                "mime_type": "text/plain",
                "description": "Demo text resource",
                "kind": "resource"
            }),
            json!({
                "uri": "resources/files/catalog.json",
                "mime_type": "application/json",
                "description": "Demo JSON resource",
                "kind": "resource"
            }),
        ],
        DiscoveryCategory::Prompts => vec![
            json!({
                "name": "drafts.reply",
                "description": "Draft a reply using a thread context",
                "arguments": [
                    { "name": "context.thread_id", "required": true, "description": "Thread to reply to" }
                ]
            }),
            json!({
                "name": "summaries.daily",
                "description": "Generate a daily summary prompt",
                "arguments": [
                    { "name": "context.date", "required": false, "description": "Date for summary" }
                ]
            }),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AppBindingConfig, AppConfig, DefaultsConfig, EventConfig, LoggingConfig, PluginConfig,
        ResolvedAppConfig, RuntimeLayout, ServerConfig,
    };

    fn test_tempdir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("mcp2cli-client-tests.")
            .tempdir()
            .expect("tempdir should exist")
    }

    fn resolved_config(endpoint: &str) -> ResolvedAppConfig {
        ResolvedAppConfig {
            name: "work".to_owned(),
            path: PathBuf::from("/tmp/work.yaml"),
            config: AppConfig {
                schema_version: 1,
                app: AppBindingConfig {
                    profile: "bridge".to_owned(),
                },
                server: ServerConfig {
                    display_name: "Test Server".to_owned(),
                    transport: TransportKind::StreamableHttp,
                    endpoint: Some(endpoint.to_owned()),
                    stdio: StdioServerConfig::default(),
                    protocol_version: None,
                },
                defaults: DefaultsConfig::default(),
                logging: LoggingConfig::default(),
                plugins: PluginConfig::default(),
                auth: crate::config::AuthConfig::default(),
                events: EventConfig::default(),
                telemetry: crate::telemetry::TelemetryConfig::default(),
                profile: None,
            },
        }
    }

    #[test]
    fn selects_demo_mode_for_demo_endpoint() {
        let config = resolved_config("https://demo.invalid/mcp");
        assert_eq!(select_client_mode(Some(&config)), ClientMode::Demo);
    }

    #[test]
    fn selects_streamable_http_mode_for_real_endpoint() {
        let config = resolved_config("https://example.com/mcp");
        assert_eq!(
            select_client_mode(Some(&config)),
            ClientMode::StreamableHttp
        );
    }

    #[test]
    fn selects_stdio_mode_for_stdio_transport() {
        let mut config = resolved_config("https://demo.invalid/mcp");
        config.config.server.transport = TransportKind::Stdio;
        config.config.server.endpoint = None;
        config.config.server.stdio.command = Some("npx".to_owned());
        config.config.server.stdio.args =
            vec!["@modelcontextprotocol/server-everything".to_owned()];

        assert_eq!(select_client_mode(Some(&config)), ClientMode::Stdio);
    }

    #[tokio::test]
    async fn builds_demo_client_without_selected_config() {
        let temp = test_tempdir();
        let layout = RuntimeLayout {
            config_root: temp.path().join("config"),
            data_root: temp.path().join("data"),
            link_root: temp.path().join("bin"),
        };

        let client = build_client(&layout, None)
            .await
            .expect("client should build");
        let metadata = client
            .metadata("bridge")
            .await
            .expect("metadata should be available");

        assert_eq!(metadata.transport, TransportKind::StreamableHttp);
        assert_eq!(metadata.server_name, "bridge-demo-server");
    }

    #[test]
    fn streamable_http_client_rejects_invalid_endpoint() {
        let error = StreamableHttpMcpClient::new(
            "work".to_owned(),
            "not a url".to_owned(),
            None,
            VersionPolicy::Auto,
            None,
        )
        .expect_err("invalid endpoint should fail");
        assert!(
            error
                .to_string()
                .contains("invalid streamable HTTP endpoint")
        );
    }

    #[test]
    fn streamable_http_client_accepts_https() {
        StreamableHttpMcpClient::new(
            "work".to_owned(),
            "https://example.com/mcp".to_owned(),
            Some("tok-abc".to_owned()),
            VersionPolicy::Auto,
            None,
        )
        .expect("https endpoints should build");
    }

    #[test]
    fn streamable_http_client_rejects_non_http_scheme() {
        let error = StreamableHttpMcpClient::new(
            "work".to_owned(),
            "ftp://example.com/mcp".to_owned(),
            None,
            VersionPolicy::Auto,
            None,
        )
        .expect_err("non http(s) scheme should fail");
        assert!(
            error.to_string().contains("must use http or https"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn stdio_client_rejects_missing_command() {
        let error = StdioMcpClient::new(
            "work".to_owned(),
            StdioServerConfig::default(),
            VersionPolicy::Auto,
            None,
        )
        .expect_err("missing stdio command should fail");
        assert!(
            error
                .to_string()
                .contains("server.stdio.command must be set")
        );
    }

    /// Accept one HTTP request on a localhost port, capture the raw request
    /// head (request line + headers), reply with a minimal 200, and return
    /// the captured bytes. Used to assert transport headers without standing
    /// up a full MCP server.
    async fn capture_one_http_request(listener: tokio::net::TcpListener) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut socket, _) = listener.accept().await.expect("accept connection");
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        // Read until we've seen the end of the headers; the canned request
        // bodies in these tests are tiny so a single small read suffices, but
        // loop to be robust against TCP fragmentation.
        loop {
            let n = socket.read(&mut chunk).await.expect("read request");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let _ = socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}",
            )
            .await;
        let _ = socket.flush().await;
        String::from_utf8_lossy(&buf).into_owned()
    }

    #[tokio::test]
    async fn streamable_http_client_sends_bearer_authorization_header() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("local addr");
        let server = tokio::spawn(capture_one_http_request(listener));

        let client = StreamableHttpMcpClient::new(
            "work".to_owned(),
            format!("http://{addr}/mcp"),
            Some("tok-abc".to_owned()),
            VersionPolicy::Auto,
            None,
        )
        .expect("client should build");
        let session = client.session.lock().await.clone();
        client
            .send_http_message(b"{}", &WireHeaders::legacy(&session, false))
            .await
            .expect("request should round-trip");

        let request = server.await.expect("server task");
        let lower = request.to_ascii_lowercase();
        assert!(
            lower.contains("authorization: bearer tok-abc"),
            "expected bearer header in request, got:\n{request}"
        );
    }

    #[tokio::test]
    async fn streamable_http_client_omits_authorization_when_unauthenticated() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("local addr");
        let server = tokio::spawn(capture_one_http_request(listener));

        let client = StreamableHttpMcpClient::new(
            "work".to_owned(),
            format!("http://{addr}/mcp"),
            None,
            VersionPolicy::Auto,
            None,
        )
        .expect("client should build");
        let session = client.session.lock().await.clone();
        client
            .send_http_message(b"{}", &WireHeaders::legacy(&session, false))
            .await
            .expect("request should round-trip");

        let request = server.await.expect("server task");
        assert!(
            !request.to_ascii_lowercase().contains("authorization:"),
            "did not expect an authorization header, got:\n{request}"
        );
    }

    #[tokio::test]
    async fn streamable_http_client_prepares_protocol_bootstrap_for_discovery() {
        let client = StreamableHttpMcpClient::new(
            "work".to_owned(),
            "http://example.com/mcp".to_owned(),
            None,
            VersionPolicy::Auto,
            None,
        )
        .expect("client should build");

        let request_id = {
            let mut next_request_id = client.next_request_id.lock().await;
            let value = *next_request_id;
            *next_request_id += 2;
            value
        };
        let session = client.session.lock().await;
        let prepared = client
            .protocol
            .prepare_operation(
                &session,
                request_id,
                &McpOperation::Discover {
                    category: DiscoveryCategory::Resources,
                },
            )
            .expect("request should prepare");

        assert_eq!(
            prepared
                .initialize
                .as_ref()
                .map(|value| value.method.as_str()),
            Some("initialize")
        );
        assert_eq!(prepared.request.method, "resources/list");
    }

    #[test]
    fn parses_jsonrpc_response_from_sse_body() {
        let parsed = parse_sse_jsonrpc_response(
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[]}}\n\n",
            None,
        )
        .expect("sse response should parse");

        assert_eq!(parsed.id, crate::mcp::protocol::JsonRpcId::Number(2));
        assert_eq!(parsed.result, Some(json!({ "tools": [] })));
    }

    #[test]
    fn prompt_output_supports_object_shaped_content_blocks() {
        let output = prompt_output_from_result(&json!({
            "messages": [
                {
                    "role": "user",
                    "content": {
                        "type": "text",
                        "text": "This is a simple prompt without arguments."
                    }
                }
            ]
        }));

        assert_eq!(output, "This is a simple prompt without arguments.");
    }

    #[test]
    fn discovery_response_normalizes_resource_keys() {
        let result = map_discovery_response(
            &DiscoveryCategory::Resources,
            JsonRpcResponse {
                jsonrpc: "2.0".to_owned(),
                id: crate::mcp::protocol::JsonRpcId::Number(1),
                result: Some(json!({
                    "resources": [
                        {
                            "uri": "demo://resource/file.md",
                            "name": "file.md",
                            "mimeType": "text/markdown",
                            "description": "Example resource"
                        }
                    ]
                })),
                error: None,
            },
        )
        .expect("resource discovery should map");

        let McpOperationResult::Discovery { items, .. } = result else {
            panic!("expected discovery result");
        };

        assert_eq!(
            items,
            vec![json!({
                "uri": "demo://resource/file.md",
                "name": "file.md",
                "title": null,
                "mime_type": "text/markdown",
                "description": "Example resource",
                "kind": "resource"
            })]
        );
    }

    #[test]
    fn unknown_server_request_returns_method_not_found() {
        use crate::mcp::handler::{OperationMessageHandler, ServerMessageHandler};
        use crate::runtime::EventBroker;

        let handler = OperationMessageHandler {
            app_id: "test".to_owned(),
            events: EventBroker::default(),
            inventory_stale_path: None,
            roots: Vec::new(),
        };
        let request = JsonRpcRequest::new(
            crate::mcp::protocol::JsonRpcId::Number(99),
            "unknown/method",
            None,
        );

        let response = handler
            .handle_request(&request)
            .expect("handler should return a response");

        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, -32601);
    }

    #[test]
    fn coerce_boolean_values() {
        use crate::mcp::handler::coerce_elicitation_value;
        let prop = json!({"type": "boolean"});
        assert_eq!(
            coerce_elicitation_value("true", "boolean", &prop),
            Value::Bool(true)
        );
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
        use crate::mcp::handler::coerce_elicitation_value;
        let prop = json!({"type": "integer"});
        assert_eq!(coerce_elicitation_value("42", "integer", &prop), json!(42));
        assert_eq!(
            coerce_elicitation_value("abc", "integer", &prop),
            json!("abc")
        );
    }

    #[test]
    #[allow(clippy::approx_constant)] // `3.14` is intentional test input, not π.
    fn coerce_number_values() {
        use crate::mcp::handler::coerce_elicitation_value;
        let prop = json!({"type": "number"});
        assert_eq!(
            coerce_elicitation_value("3.14", "number", &prop),
            json!(3.14)
        );
    }

    #[test]
    fn coerce_array_splits_comma_separated() {
        use crate::mcp::handler::coerce_elicitation_value;
        let prop = json!({"type": "array", "items": {"type": "string"}});
        assert_eq!(
            coerce_elicitation_value("Guitar, Piano", "array", &prop),
            json!(["Guitar", "Piano"])
        );
    }

    // -- MCP 2026-07-28 transport helpers ------------------------------------

    #[test]
    fn header_values_pass_plain_ascii_and_base64_encode_the_rest() {
        assert_eq!(encode_header_value("us-west1"), "us-west1");
        assert_eq!(encode_header_value("get_weather"), "get_weather");
        // Non-ASCII → base64 sentinel (example from the spec).
        assert_eq!(
            encode_header_value("Hello, 世界"),
            "=?base64?SGVsbG8sIOS4lueVjA==?="
        );
        // Leading/trailing whitespace → encoded.
        assert_eq!(encode_header_value(" padded "), "=?base64?IHBhZGRlZCA=?=");
        // Embedded newline → encoded.
        assert_eq!(
            encode_header_value("line1\nline2"),
            "=?base64?bGluZTEKbGluZTI=?="
        );
        // A literal sentinel-shaped value must itself be encoded.
        assert_eq!(
            encode_header_value("=?base64?literal?="),
            "=?base64?PT9iYXNlNjQ/bGl0ZXJhbD89?="
        );
    }

    #[test]
    fn mcp_name_is_derived_from_method_specific_fields() {
        let tool = JsonRpcRequest::new(
            crate::mcp::protocol::JsonRpcId::Number(1),
            "tools/call",
            Some(json!({ "name": "get_weather" })),
        );
        assert_eq!(mcp_name_for_request(&tool).as_deref(), Some("get_weather"));

        let read = JsonRpcRequest::new(
            crate::mcp::protocol::JsonRpcId::Number(2),
            "resources/read",
            Some(json!({ "uri": "file:///config.json" })),
        );
        assert_eq!(
            mcp_name_for_request(&read).as_deref(),
            Some("file:///config.json")
        );

        let list = JsonRpcRequest::new(
            crate::mcp::protocol::JsonRpcId::Number(3),
            "tools/list",
            None,
        );
        assert_eq!(mcp_name_for_request(&list), None);
    }

    #[test]
    fn param_headers_extract_annotated_primitives() {
        let schema = json!({
            "type": "object",
            "properties": {
                "region": { "type": "string", "x-mcp-header": "Region" },
                "attempts": { "type": "integer", "x-mcp-header": "Attempts" },
                "dry_run": { "type": "boolean", "x-mcp-header": "Dry-Run" },
                "query": { "type": "string" },
                "nested": {
                    "type": "object",
                    "properties": {
                        "tenant": { "type": "string", "x-mcp-header": "Tenant" }
                    }
                }
            }
        });
        let arguments = json!({
            "region": "us-west1",
            "attempts": 3,
            "dry_run": true,
            "query": "SELECT 1",
            "nested": { "tenant": "acme" }
        });

        let mut headers =
            extract_param_headers(Some(&schema), &arguments).expect("annotations should be valid");
        headers.sort();
        assert_eq!(
            headers,
            vec![
                ("Attempts".to_owned(), "3".to_owned()),
                ("Dry-Run".to_owned(), "true".to_owned()),
                ("Region".to_owned(), "us-west1".to_owned()),
                ("Tenant".to_owned(), "acme".to_owned()),
            ]
        );
    }

    #[test]
    fn param_headers_omit_missing_and_null_values() {
        let schema = json!({
            "type": "object",
            "properties": {
                "region": { "type": "string", "x-mcp-header": "Region" },
                "zone": { "type": "string", "x-mcp-header": "Zone" }
            }
        });
        let headers =
            extract_param_headers(Some(&schema), &json!({ "zone": null })).expect("valid");
        assert!(headers.is_empty());
    }

    #[test]
    fn param_headers_reject_invalid_annotations() {
        // number type is explicitly forbidden
        let number = json!({
            "type": "object",
            "properties": { "rate": { "type": "number", "x-mcp-header": "Rate" } }
        });
        assert!(extract_param_headers(Some(&number), &json!({})).is_err());

        // duplicate names differing only by case
        let duplicate = json!({
            "type": "object",
            "properties": {
                "a": { "type": "string", "x-mcp-header": "Region" },
                "b": { "type": "string", "x-mcp-header": "region" }
            }
        });
        assert!(extract_param_headers(Some(&duplicate), &json!({})).is_err());

        // non-tchar characters in the header name
        let bad_name = json!({
            "type": "object",
            "properties": { "a": { "type": "string", "x-mcp-header": "Bad Name" } }
        });
        assert!(extract_param_headers(Some(&bad_name), &json!({})).is_err());
    }

    #[test]
    fn invalid_header_tools_are_rejected_from_discovery() {
        let mut items = vec![
            json!({
                "id": "good",
                "kind": "tool",
                "inputSchema": {
                    "type": "object",
                    "properties": { "region": { "type": "string", "x-mcp-header": "Region" } }
                }
            }),
            json!({
                "id": "bad",
                "kind": "tool",
                "inputSchema": {
                    "type": "object",
                    "properties": { "rate": { "type": "number", "x-mcp-header": "Rate" } }
                }
            }),
        ];
        reject_invalid_header_tools(&mut items, &EventBroker::default(), "test");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["id"], json!("good"));
    }

    #[test]
    fn modern_wire_headers_carry_request_metadata_without_session() {
        let request = JsonRpcRequest::new(
            crate::mcp::protocol::JsonRpcId::Number(1),
            "tools/call",
            Some(json!({ "name": "send" })),
        );
        let headers = WireHeaders::modern(
            "2026-07-28",
            &request,
            &[("Region".to_owned(), "us-west1".to_owned())],
        );
        assert_eq!(headers.protocol_version.as_deref(), Some("2026-07-28"));
        assert_eq!(headers.mcp_method.as_deref(), Some("tools/call"));
        assert_eq!(headers.mcp_name.as_deref(), Some("send"));
        assert!(headers.session_id.is_none());
        assert_eq!(headers.params.len(), 1);

        // Mcp-Param-* headers only apply to tools/call requests.
        let get = JsonRpcRequest::new(
            crate::mcp::protocol::JsonRpcId::Number(2),
            "tasks/get",
            Some(json!({ "taskId": "t1" })),
        );
        let headers = WireHeaders::modern(
            "2026-07-28",
            &get,
            &[("Region".to_owned(), "us-west1".to_owned())],
        );
        assert!(headers.params.is_empty());
    }

    #[test]
    fn modern_http_surfaces_jsonrpc_error_bodies_on_4xx() {
        let raw = HttpTransportResponse {
            status: StatusCode::BAD_REQUEST,
            content_type: Some("application/json".to_owned()),
            session_id: None,
            protocol_version: None,
            body: Bytes::from(
                serde_json::to_vec(&json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "error": {
                        "code": -32022,
                        "message": "Unsupported protocol version",
                        "data": { "supported": ["2026-07-28"] }
                    }
                }))
                .unwrap(),
            ),
        };
        let response = modern_http_into_jsonrpc(raw, None).expect("error body should decode");
        assert_eq!(
            response.error.as_ref().map(|error| error.code),
            Some(-32022)
        );

        let opaque = HttpTransportResponse {
            status: StatusCode::BAD_REQUEST,
            content_type: Some("text/plain".to_owned()),
            session_id: None,
            protocol_version: None,
            body: Bytes::from_static(b"Bad Request: no session"),
        };
        assert!(modern_http_into_jsonrpc(opaque, None).is_err());
    }

    #[tokio::test]
    async fn modern_http_request_sends_required_metadata_headers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("local addr");
        let server = tokio::spawn(capture_one_http_request(listener));

        let client = StreamableHttpMcpClient::new(
            "email".to_owned(),
            format!("http://{addr}/mcp"),
            None,
            VersionPolicy::Auto,
            None,
        )
        .expect("client should build");
        let request = JsonRpcRequest::new(
            crate::mcp::protocol::JsonRpcId::Number(1),
            "tools/call",
            Some(json!({ "name": "send", "arguments": {} })),
        );
        // The canned reply body (`{}`) is not a JSON-RPC response; this
        // test only asserts on the request headers the client sent.
        let _ = client
            .send_modern_jsonrpc(
                &request,
                "2026-07-28",
                &[("Region".to_owned(), "us-west1".to_owned())],
                None,
            )
            .await;

        let captured = server.await.expect("server task");
        let lower = captured.to_ascii_lowercase();
        assert!(
            lower.contains("mcp-protocol-version: 2026-07-28"),
            "{captured}"
        );
        assert!(lower.contains("mcp-method: tools/call"), "{captured}");
        assert!(lower.contains("mcp-name: send"), "{captured}");
        assert!(lower.contains("mcp-param-region: us-west1"), "{captured}");
        assert!(
            !lower.contains("mcp-session-id"),
            "modern requests must not carry a session header: {captured}"
        );
    }
}

fn task_result(remote_task_id: &str, task: &DemoTaskState, message: String) -> McpOperationResult {
    McpOperationResult::Task {
        status: task.status.clone(),
        message,
        remote_task_id: remote_task_id.to_owned(),
        data: json!({
            "capability": task.capability,
            "summary": task.summary,
            "arguments": task.arguments,
        }),
        result: task.result.clone(),
        failure_reason: task.failure_reason.clone(),
    }
}

// ---------------------------------------------------------------------------
// Daemon MCP client — delegates operations to a running daemon via Unix socket
// ---------------------------------------------------------------------------

struct DaemonMcpClient {
    config_name: String,
    socket_path: std::path::PathBuf,
}

#[async_trait]
impl McpClient for DaemonMcpClient {
    async fn metadata(&self, app_id: &str) -> Result<ConnectionMetadata> {
        Ok(ConnectionMetadata {
            app_id: app_id.to_owned(),
            server_name: format!("daemon:{}", self.config_name),
            server_version: "daemon".to_owned(),
            transport: TransportKind::Stdio, // proxied
        })
    }

    async fn negotiated_session(&self) -> Option<McpClientSession> {
        None
    }

    async fn perform(
        &self,
        _app_id: &str,
        operation: McpOperation,
        _events: &EventBroker,
        _inventory_stale_path: Option<&std::path::PathBuf>,
    ) -> Result<McpOperationResult> {
        crate::runtime::daemon::daemon_perform(&self.socket_path, &operation).await
    }

    fn is_daemon(&self) -> bool {
        true
    }
}
