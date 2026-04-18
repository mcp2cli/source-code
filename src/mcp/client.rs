use std::{collections::BTreeMap, path::PathBuf, process::Stdio};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use bytes::Bytes;
use http::{Method, Request, StatusCode, header};
use http_body_util::{BodyExt, Full};
use hyper::Uri;
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
        DEFAULT_MCP_PROTOCOL_VERSION, InitializeResult, JsonRpcError, JsonRpcNotification,
        JsonRpcRequest, JsonRpcResponse, McpClientSession, ProtocolEngine,
    },
    runtime::{EventBroker, RuntimeEvent},
};

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
            Ok(Box::new(StdioMcpClient::new(
                config.name.clone(),
                config.config.server.stdio.clone(),
            )?))
        }
        ClientMode::StreamableHttp => {
            let config =
                config.ok_or_else(|| anyhow!("missing config for streamable HTTP MCP client"))?;
            Ok(Box::new(StreamableHttpMcpClient::new(
                config.name.clone(),
                config.config.server.endpoint.clone().ok_or_else(|| {
                    anyhow!("server.endpoint must be set for streamable HTTP transport")
                })?,
            )?))
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
    client: HyperClient<HttpConnector, Full<Bytes>>,
    protocol: ProtocolEngine,
    session: Mutex<McpClientSession>,
    next_request_id: Mutex<u64>,
}

impl StdioMcpClient {
    pub fn new(config_name: String, stdio: StdioServerConfig) -> Result<Self> {
        stdio.validate()?;
        let protocol = ProtocolEngine::new(
            DEFAULT_MCP_PROTOCOL_VERSION,
            "mcp2cli",
            env!("CARGO_PKG_VERSION"),
        );
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
}

impl StreamableHttpMcpClient {
    pub fn new(config_name: String, endpoint: String) -> Result<Self> {
        let endpoint = Url::parse(&endpoint).map_err(|error| {
            anyhow!("invalid streamable HTTP endpoint '{}': {}", endpoint, error)
        })?;
        if endpoint.scheme() != "http" {
            return Err(anyhow!(
                "streamable HTTP client currently supports plain http endpoints only; got '{}'",
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
        let connector = HttpConnector::new();
        let client = HyperClient::builder(TokioExecutor::new()).build(connector);
        let protocol = ProtocolEngine::new(
            DEFAULT_MCP_PROTOCOL_VERSION,
            "mcp2cli",
            env!("CARGO_PKG_VERSION"),
        );
        let session = Mutex::new(protocol.initial_session());

        Ok(Self {
            config_name,
            endpoint,
            endpoint_uri,
            client,
            protocol,
            session,
            next_request_id: Mutex::new(1),
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
            .send_http_message(&bytes, session, session.initialized)
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

    async fn send_notification(
        &self,
        notification: &JsonRpcNotification,
        session: &McpClientSession,
    ) -> Result<()> {
        let bytes = serde_json::to_vec(notification)
            .map_err(|error| anyhow!("failed to serialize JSON-RPC notification: {}", error))?;
        let response = self
            .send_http_message(&bytes, session, session.initialized)
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
        session: &McpClientSession,
        include_protocol_version: bool,
    ) -> Result<HttpTransportResponse> {
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri(self.endpoint_uri.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream");

        if include_protocol_version {
            builder = builder.header("mcp-protocol-version", session.protocol_version.as_str());
        }
        if let Some(session_id) = &session.session_id {
            builder = builder.header("mcp-session-id", session_id.as_str());
        }

        let request = builder
            .body(Full::new(Bytes::copy_from_slice(body)))
            .map_err(|error| anyhow!("failed to build HTTP MCP request: {}", error))?;
        let response = self
            .client
            .request(request)
            .await
            .map_err(|error| anyhow!("streamable HTTP request failed: {}", error))?;
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
        let request_id = {
            let mut next_request_id = self.next_request_id.lock().await;
            let value = *next_request_id;
            *next_request_id += 2;
            value
        };
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

        let handler = OperationMessageHandler {
            app_id: app_id.to_owned(),
            events: events.clone(),
            inventory_stale_path: inventory_stale_path.cloned(),
            roots: Vec::new(),
        };

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
        let request_id = {
            let mut next_request_id = self.next_request_id.lock().await;
            let value = *next_request_id;
            *next_request_id += 2;
            value
        };
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

        let handler = OperationMessageHandler {
            app_id: app_id.to_owned(),
            events: events.clone(),
            inventory_stale_path: inventory_stale_path.cloned(),
            roots: Vec::new(),
        };

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
        let error = StreamableHttpMcpClient::new("work".to_owned(), "not a url".to_owned())
            .expect_err("invalid endpoint should fail");
        assert!(
            error
                .to_string()
                .contains("invalid streamable HTTP endpoint")
        );
    }

    #[test]
    fn streamable_http_client_rejects_https_for_now() {
        let error =
            StreamableHttpMcpClient::new("work".to_owned(), "https://example.com/mcp".to_owned())
                .expect_err("https should fail for now");
        assert!(error.to_string().contains("plain http endpoints only"));
    }

    #[test]
    fn stdio_client_rejects_missing_command() {
        let error = StdioMcpClient::new("work".to_owned(), StdioServerConfig::default())
            .expect_err("missing stdio command should fail");
        assert!(
            error
                .to_string()
                .contains("server.stdio.command must be set")
        );
    }

    #[tokio::test]
    async fn streamable_http_client_prepares_protocol_bootstrap_for_discovery() {
        let client =
            StreamableHttpMcpClient::new("work".to_owned(), "http://example.com/mcp".to_owned())
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
}
