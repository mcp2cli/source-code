//! Daemon mode: keeps MCP connections warm between CLI invocations.
//!
//! `mcp2cli daemon start <name>` spawns a background process that initializes
//! the MCP transport (stdio subprocess or HTTP connection) and holds it open.
//! Subsequent CLI invocations for the same config detect the running daemon
//! via a PID file and connect through a local Unix socket, avoiding the
//! overhead of process startup and MCP initialization for every command.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::config::RuntimeLayout;

/// Metadata written to the daemon PID file.
#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub config_name: String,
    pub socket_path: String,
    pub started_at: String,
}

impl RuntimeLayout {
    /// Path to the daemon PID file for a named config.
    pub fn daemon_pid_path(&self, config_name: &str) -> PathBuf {
        self.data_root
            .join("instances")
            .join(config_name)
            .join("daemon.json")
    }

    /// Path to the Unix socket for a daemon.
    pub fn daemon_socket_path(&self, config_name: &str) -> PathBuf {
        self.data_root
            .join("instances")
            .join(config_name)
            .join("daemon.sock")
    }
}

/// Check if a daemon is running for the given config.
pub fn daemon_status(layout: &RuntimeLayout, config_name: &str) -> Result<Option<DaemonInfo>> {
    let pid_path = layout.daemon_pid_path(config_name);
    if !pid_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&pid_path)
        .map_err(|e| anyhow!("failed to read daemon PID file: {}", e))?;
    let info: DaemonInfo = serde_json::from_str(&content)
        .map_err(|e| anyhow!("failed to parse daemon PID file: {}", e))?;

    // Verify the process is still alive
    if is_process_alive(info.pid) {
        Ok(Some(info))
    } else {
        // Stale PID file — clean up
        let _ = std::fs::remove_file(&pid_path);
        let socket_path = layout.daemon_socket_path(config_name);
        let _ = std::fs::remove_file(&socket_path);
        Ok(None)
    }
}

/// Stop a running daemon by sending SIGTERM.
pub fn stop_daemon(layout: &RuntimeLayout, config_name: &str) -> Result<bool> {
    let info = match daemon_status(layout, config_name)? {
        Some(info) => info,
        None => return Ok(false),
    };

    // Send SIGTERM
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(info.pid as libc::pid_t, libc::SIGTERM);
        }
    }

    // Clean up PID and socket files
    let _ = std::fs::remove_file(layout.daemon_pid_path(config_name));
    let _ = std::fs::remove_file(layout.daemon_socket_path(config_name));
    Ok(true)
}

/// Write the daemon info file.
fn write_daemon_info(path: &Path, info: &DaemonInfo) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(info)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Start the daemon main loop. This function runs forever until signaled.
pub async fn run_daemon(
    layout: &RuntimeLayout,
    config_name: &str,
    mcp_client: std::sync::Arc<dyn crate::mcp::client::McpClient>,
    event_broker: crate::runtime::EventBroker,
) -> Result<()> {
    let socket_path = layout.daemon_socket_path(config_name);
    let pid_path = layout.daemon_pid_path(config_name);

    // Remove stale socket
    let _ = std::fs::remove_file(&socket_path);

    // Ensure the directory exists
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Bind the Unix socket
    let listener = UnixListener::bind(&socket_path).map_err(|e| {
        anyhow!(
            "failed to bind daemon socket at {}: {}",
            socket_path.display(),
            e
        )
    })?;

    // Write PID file
    let info = DaemonInfo {
        pid: std::process::id(),
        config_name: config_name.to_owned(),
        socket_path: socket_path.to_string_lossy().into_owned(),
        started_at: chrono::Utc::now().to_rfc3339(),
    };
    write_daemon_info(&pid_path, &info)?;

    // Initialize the MCP connection by performing a ping
    let _ = mcp_client
        .perform(
            config_name,
            crate::mcp::model::McpOperation::Ping,
            &event_broker,
            None,
        )
        .await;

    tracing::info!(config = config_name, socket = %socket_path.display(), "daemon started");

    // Set up graceful shutdown on SIGTERM/SIGINT
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _)) => {
                        let client = mcp_client.clone();
                        let broker = event_broker.clone();
                        let name = config_name.to_owned();
                        tokio::spawn(async move {
                            if let Err(e) = handle_daemon_client(stream, &name, &*client, &broker).await {
                                tracing::warn!(error = %e, "daemon client connection error");
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "daemon accept error");
                    }
                }
            }
            _ = &mut shutdown => {
                tracing::info!("daemon shutting down");
                break;
            }
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&pid_path);
    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

/// Handle a single client connection to the daemon.
/// Protocol: client sends a JSON-encoded McpOperation, daemon returns a JSON-encoded result.
async fn handle_daemon_client(
    stream: UnixStream,
    config_name: &str,
    client: &dyn crate::mcp::client::McpClient,
    event_broker: &crate::runtime::EventBroker,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            break; // Client disconnected
        }

        let operation: crate::mcp::model::McpOperation = match serde_json::from_str(line.trim()) {
            Ok(op) => op,
            Err(e) => {
                let err_response =
                    serde_json::json!({ "error": format!("invalid operation: {}", e) });
                writer
                    .write_all(serde_json::to_string(&err_response)?.as_bytes())
                    .await?;
                writer.write_all(b"\n").await?;
                continue;
            }
        };

        let result = client
            .perform(config_name, operation, event_broker, None)
            .await;

        let response = match result {
            Ok(result) => serde_json::to_string(&result)?,
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        };
        writer.write_all(response.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}

/// Connect to a running daemon and send an operation.
pub async fn daemon_perform(
    socket_path: &Path,
    operation: &crate::mcp::model::McpOperation,
) -> Result<crate::mcp::model::McpOperationResult> {
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(|e| anyhow!("failed to connect to daemon: {}", e))?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Send the operation
    let request = serde_json::to_string(operation)?;
    writer.write_all(request.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    // Read the response
    let mut response_line = String::new();
    reader.read_line(&mut response_line).await?;

    // Check for error envelope
    let value: serde_json::Value = serde_json::from_str(response_line.trim())?;
    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
        return Err(anyhow!("daemon error: {}", error));
    }

    serde_json::from_value(value).map_err(|e| anyhow!("failed to parse daemon response: {}", e))
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // kill(pid, 0) checks if the process exists
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
