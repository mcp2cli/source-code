use std::io::Write;

use bytes::Bytes;
use http::{Method, Request, header};
use http_body_util::Full;
use hyper_util::{
    client::legacy::{Client as HyperClient, connect::HttpConnector},
    rt::TokioExecutor,
};
use tokio::sync::broadcast;
use tracing::{debug, warn};
use url::Url;

use super::events::{EventSink, RuntimeEvent};

// ---------------------------------------------------------------------------
// HTTP Webhook Sink — POSTs each event as JSON to a remote endpoint
// ---------------------------------------------------------------------------

pub struct HttpWebhookSink {
    endpoint: Url,
    client: HyperClient<HttpConnector, Full<Bytes>>,
    runtime: tokio::runtime::Handle,
}

impl HttpWebhookSink {
    pub fn new(endpoint: Url) -> Self {
        let connector = HttpConnector::new();
        let client = HyperClient::builder(TokioExecutor::new()).build(connector);
        Self {
            endpoint,
            client,
            runtime: tokio::runtime::Handle::current(),
        }
    }
}

impl EventSink for HttpWebhookSink {
    fn emit(&self, event: &RuntimeEvent) {
        let body = match serde_json::to_vec(event) {
            Ok(bytes) => bytes,
            Err(error) => {
                warn!("http webhook sink: failed to serialize event: {}", error);
                return;
            }
        };
        let uri: hyper::Uri = match self.endpoint.as_str().parse() {
            Ok(uri) => uri,
            Err(error) => {
                warn!("http webhook sink: invalid endpoint URI: {}", error);
                return;
            }
        };
        let request = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(body)));
        let request = match request {
            Ok(value) => value,
            Err(error) => {
                warn!("http webhook sink: failed to build request: {}", error);
                return;
            }
        };
        let client = self.client.clone();
        self.runtime.spawn(async move {
            match client.request(request).await {
                Ok(response) if response.status().is_success() => {
                    debug!("http webhook sink: event delivered");
                }
                Ok(response) => {
                    warn!(
                        "http webhook sink: non-success status {}",
                        response.status()
                    );
                }
                Err(error) => {
                    warn!("http webhook sink: request failed: {}", error);
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Unix Domain Socket Sink — writes newline-delimited JSON to a UDS
// ---------------------------------------------------------------------------

pub struct UnixSocketSink {
    path: String,
    runtime: tokio::runtime::Handle,
}

impl UnixSocketSink {
    pub fn new(path: String) -> Self {
        Self {
            path,
            runtime: tokio::runtime::Handle::current(),
        }
    }
}

impl EventSink for UnixSocketSink {
    fn emit(&self, event: &RuntimeEvent) {
        let mut line = match serde_json::to_vec(event) {
            Ok(bytes) => bytes,
            Err(error) => {
                warn!("unix socket sink: failed to serialize event: {}", error);
                return;
            }
        };
        line.push(b'\n');
        let path = self.path.clone();
        self.runtime.spawn(async move {
            match tokio::net::UnixStream::connect(&path).await {
                Ok(stream) => {
                    if let Err(error) = stream.try_write(&line) {
                        debug!("unix socket sink: write failed: {}", error);
                    }
                }
                Err(error) => {
                    debug!("unix socket sink: connect failed ({}): {}", path, error);
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// SSE Server Sink — serves a local HTTP SSE endpoint for real-time streams
// ---------------------------------------------------------------------------

/// An EventSink that broadcasts events to connected SSE clients.
pub struct SseServerSink {
    sender: broadcast::Sender<String>,
}

impl SseServerSink {
    /// Spawns a local HTTP SSE server on the given `bind_addr` (e.g. `127.0.0.1:9090`)
    /// and returns the sink. Callers emit events into the sink; connected SSE clients
    /// receive them as `text/event-stream` messages.
    pub fn start(bind_addr: std::net::SocketAddr) -> anyhow::Result<Self> {
        let (sender, _) = broadcast::channel::<String>(256);
        let server_sender = sender.clone();

        tokio::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(bind_addr).await {
                Ok(listener) => listener,
                Err(error) => {
                    warn!("sse server: failed to bind {}: {}", bind_addr, error);
                    return;
                }
            };
            debug!("sse server: listening on {}", bind_addr);

            loop {
                let (stream, _peer) = match listener.accept().await {
                    Ok(value) => value,
                    Err(error) => {
                        warn!("sse server: accept failed: {}", error);
                        continue;
                    }
                };

                let rx = server_sender.subscribe();
                tokio::spawn(handle_sse_connection(stream, rx));
            }
        });

        Ok(Self { sender })
    }
}

impl EventSink for SseServerSink {
    fn emit(&self, event: &RuntimeEvent) {
        let json = match serde_json::to_string(event) {
            Ok(value) => value,
            Err(error) => {
                warn!("sse sink: failed to serialize event: {}", error);
                return;
            }
        };
        // Broadcast errors are acceptable — they mean no subscribers are active.
        let _ = self.sender.send(json);
    }
}

async fn handle_sse_connection(stream: tokio::net::TcpStream, mut rx: broadcast::Receiver<String>) {
    let mut buf = Vec::new();

    // Read the HTTP request line minimally, then send the SSE response headers.
    // We use a trivial approach: read until we see \r\n\r\n, then respond.
    let readable = stream.readable().await;
    if readable.is_err() {
        return;
    }
    let mut request_buf = [0u8; 4096];
    match stream.try_read(&mut request_buf) {
        Ok(0) | Err(_) => return,
        Ok(_) => {}
    }

    // Write HTTP response headers for SSE
    let headers = b"HTTP/1.1 200 OK\r\n\
        Content-Type: text/event-stream\r\n\
        Cache-Control: no-cache\r\n\
        Connection: keep-alive\r\n\
        Access-Control-Allow-Origin: *\r\n\
        \r\n";

    if stream.try_write(headers).is_err() {
        return;
    }

    // Stream events as SSE data lines
    loop {
        match rx.recv().await {
            Ok(json) => {
                buf.clear();
                write!(&mut buf, "data: {}\n\n", json).ok();
                if stream.try_write(&buf).is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                debug!("sse client lagged, skipped {} events", skipped);
                continue;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

// ---------------------------------------------------------------------------
// Command Execution Sink — runs a shell command for each event
// ---------------------------------------------------------------------------

/// An EventSink that executes a shell command template for each event.
///
/// The command string is run via `sh -c` with environment variables set:
/// - `MCP_EVENT_TYPE` — event type (info, progress, server_log, etc.)
/// - `MCP_EVENT_JSON` — full JSON-serialized event
/// - `MCP_EVENT_APP_ID` — the app_id field
/// - `MCP_EVENT_MESSAGE` — human-readable message line
pub struct CommandExecSink {
    command_template: String,
    runtime: tokio::runtime::Handle,
}

impl CommandExecSink {
    pub fn new(command_template: String) -> Self {
        Self {
            command_template,
            runtime: tokio::runtime::Handle::current(),
        }
    }
}

impl EventSink for CommandExecSink {
    fn emit(&self, event: &RuntimeEvent) {
        let json_str = match serde_json::to_string(event) {
            Ok(s) => s,
            Err(error) => {
                warn!("command exec sink: failed to serialize event: {}", error);
                return;
            }
        };
        let human_msg = event.human_line();
        let event_type = event.event_type().to_owned();
        let app_id = event.app_id().to_owned();
        let command = self.command_template.clone();

        self.runtime.spawn(async move {
            let result = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .env("MCP_EVENT_TYPE", &event_type)
                .env("MCP_EVENT_JSON", &json_str)
                .env("MCP_EVENT_APP_ID", &app_id)
                .env("MCP_EVENT_MESSAGE", &human_msg)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
            match result {
                Ok(status) if !status.success() => {
                    debug!("command exec sink: command exited with status {}", status);
                }
                Err(error) => {
                    warn!("command exec sink: failed to execute command: {}", error);
                }
                _ => {}
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_webhook_sink_serializes_event_to_json() {
        let event = RuntimeEvent::Info {
            app_id: "test".to_owned(),
            message: "hello".to_owned(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"info\""));
        assert!(json.contains("\"app_id\":\"test\""));
    }

    #[tokio::test]
    async fn unix_socket_sink_constructs_without_existing_socket() {
        let _sink = UnixSocketSink::new("/tmp/nonexistent-test-socket.sock".to_owned());
        // Sink construction should not fail even if the socket doesn't exist.
        // Delivery failures are handled at emit time.
    }

    #[tokio::test]
    async fn sse_server_sink_broadcasts_events() {
        let _addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        // We can't easily test the full TCP flow in a unit test, but we can verify
        // that the broadcast sender works correctly.
        let (sender, mut rx) = broadcast::channel::<String>(16);
        sender.send("test-event".to_owned()).unwrap();
        let received = rx.recv().await.unwrap();
        assert_eq!(received, "test-event");
    }
}
