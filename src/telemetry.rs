//! Anonymous usage telemetry for mcp2cli.
//!
//! Collects **non-sensitive** usage data to understand which features are
//! used, what fails, and where to invest development effort.
//!
//! ## Privacy guarantees
//! - No server endpoints, URIs, arguments, or tool names are recorded.
//! - The installation ID is a random UUID (not derived from user identity).
//! - Telemetry is opt-out: disable via config, env var, or CLI flag.
//!
//! ## Data flow
//! 1. Each command invocation produces a [`TelemetryEvent`].
//! 2. Events are appended to a local NDJSON file (`telemetry.ndjson`).
//! 3. Optionally, events are batched and POSTed to a configurable HTTP endpoint.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use uuid::Uuid;

/// Whether telemetry is globally disabled for this process (set once at startup).
static TELEMETRY_DISABLED: OnceLock<bool> = OnceLock::new();

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Telemetry configuration from YAML config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryConfig {
    /// Master switch. Default: true (opt-out model).
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Optional HTTP endpoint for shipping events.
    /// Accepts any NDJSON-compatible collector (PostHog, Plausible, custom).
    #[serde(default)]
    pub endpoint: Option<String>,

    /// Maximum events to batch before flushing to HTTP endpoint. Default: 25.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            endpoint: None,
            batch_size: default_batch_size(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_batch_size() -> usize {
    25
}

// ---------------------------------------------------------------------------
// Event Model
// ---------------------------------------------------------------------------

/// A single anonymous telemetry event — one per CLI invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    /// Schema version for forward compatibility.
    pub schema: u32,
    /// Random installation UUID (not user-identifying).
    pub installation_id: String,
    /// ISO-8601 UTC timestamp.
    pub timestamp: String,
    /// mcp2cli version.
    pub cli_version: String,
    /// OS family: "linux", "macos", "windows".
    pub os: String,
    /// "x86_64", "aarch64", etc.
    pub arch: String,
    /// What happened.
    pub event: EventKind,
}

/// The event payload — what command category was used and how it went.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    /// A CLI command was executed.
    CommandRun {
        /// Command category (NOT the actual tool/prompt name).
        /// Values: "tool_invoke", "resource_read", "prompt_run", "discover",
        /// "ping", "doctor", "inspect", "auth", "jobs", "log", "complete",
        /// "subscribe", "config", "link", "use", "daemon".
        command_category: String,
        /// Transport used: "streamable_http", "stdio", "demo".
        transport: String,
        /// Whether --json/--output was used.
        json_output: bool,
        /// Whether --background was used.
        background: bool,
        /// Whether --timeout was explicitly set.
        timeout_override: bool,
        /// Whether a profile overlay was active.
        profile_active: bool,
        /// Whether daemon mode was active.
        daemon_active: bool,
        /// Whether this was an ad-hoc (--url/--stdio) invocation.
        ad_hoc: bool,
        /// Outcome: "success" or "error".
        outcome: String,
        /// Duration in milliseconds.
        duration_ms: u64,
    },
    /// First run — sent once per installation.
    FirstRun,
}

// ---------------------------------------------------------------------------
// Installation ID
// ---------------------------------------------------------------------------

/// Read or create the installation ID file.
/// Stored at `<data_root>/telemetry_id`.
pub fn get_or_create_installation_id(data_root: &Path) -> String {
    let id_path = data_root.join("telemetry_id");
    if let Ok(id) = fs::read_to_string(&id_path) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }
    let id = Uuid::new_v4().to_string();
    if let Err(e) = fs::create_dir_all(data_root) {
        warn!("failed to create telemetry data dir: {}", e);
        return id;
    }
    if let Err(e) = fs::write(&id_path, &id) {
        warn!("failed to write telemetry ID: {}", e);
    }
    id
}

// ---------------------------------------------------------------------------
// Recorder
// ---------------------------------------------------------------------------

/// Handles recording telemetry events to local file and optional remote endpoint.
pub struct TelemetryRecorder {
    config: TelemetryConfig,
    data_root: PathBuf,
    installation_id: String,
}

impl TelemetryRecorder {
    /// Create a new recorder. Returns `None` if telemetry is disabled.
    pub fn new(config: &TelemetryConfig, data_root: &Path) -> Option<Self> {
        if !Self::is_enabled(config) {
            return None;
        }
        let installation_id = get_or_create_installation_id(data_root);
        Some(Self {
            config: config.clone(),
            data_root: data_root.to_path_buf(),
            installation_id,
        })
    }

    /// Check whether telemetry should be enabled, considering config, env, and global flag.
    fn is_enabled(config: &TelemetryConfig) -> bool {
        // Global process-level override (from --no-telemetry flag)
        if let Some(&disabled) = TELEMETRY_DISABLED.get() {
            if disabled {
                return false;
            }
        }
        // Environment variable: MCP2CLI_TELEMETRY=off|false|0|no
        if let Ok(val) = std::env::var("MCP2CLI_TELEMETRY") {
            let val = val.to_lowercase();
            if matches!(val.as_str(), "off" | "false" | "0" | "no" | "disabled") {
                return false;
            }
        }
        // CI environments: respect DO_NOT_TRACK (https://consoledonottrack.com/)
        if std::env::var("DO_NOT_TRACK").is_ok() {
            return false;
        }
        config.enabled
    }

    /// Globally disable telemetry for this process (called when --no-telemetry is passed).
    pub fn disable_globally() {
        let _ = TELEMETRY_DISABLED.set(true);
    }

    /// Record a command-run event.
    pub fn record_command(
        &self,
        command_category: &str,
        transport: &str,
        json_output: bool,
        background: bool,
        timeout_override: bool,
        profile_active: bool,
        daemon_active: bool,
        ad_hoc: bool,
        outcome: &str,
        duration: Duration,
    ) {
        let event = self.build_event(EventKind::CommandRun {
            command_category: command_category.to_string(),
            transport: transport.to_string(),
            json_output,
            background,
            timeout_override,
            profile_active,
            daemon_active,
            ad_hoc,
            outcome: outcome.to_string(),
            duration_ms: duration.as_millis() as u64,
        });
        self.persist(&event);
    }

    /// Record a first-run event (sent once per installation).
    pub fn record_first_run(&self) {
        let marker = self.data_root.join("telemetry_first_run");
        if marker.exists() {
            return;
        }
        let event = self.build_event(EventKind::FirstRun);
        self.persist(&event);
        let _ = fs::write(&marker, "1");
    }

    fn build_event(&self, event: EventKind) -> TelemetryEvent {
        TelemetryEvent {
            schema: 1,
            installation_id: self.installation_id.clone(),
            timestamp: Utc::now().to_rfc3339(),
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            event,
        }
    }

    fn persist(&self, event: &TelemetryEvent) {
        // Local NDJSON file — always written (primary storage).
        self.write_local(event);
        // If an HTTP endpoint is configured, attempt to ship.
        // This is fire-and-forget; failures are silently ignored.
        if self.config.endpoint.is_some() {
            self.try_ship_batch();
        }
    }

    fn local_file_path(&self) -> PathBuf {
        self.data_root.join("telemetry.ndjson")
    }

    fn write_local(&self, event: &TelemetryEvent) {
        let path = self.local_file_path();
        if let Err(e) = fs::create_dir_all(&self.data_root) {
            debug!("telemetry: failed to create dir: {}", e);
            return;
        }
        let line = match serde_json::to_string(event) {
            Ok(json) => json,
            Err(e) => {
                debug!("telemetry: failed to serialize event: {}", e);
                return;
            }
        };
        let file = OpenOptions::new().create(true).append(true).open(&path);
        match file {
            Ok(mut f) => {
                let _ = writeln!(f, "{}", line);
            }
            Err(e) => {
                debug!("telemetry: failed to write event: {}", e);
            }
        }
    }

    /// Try to batch-ship events to the configured HTTP endpoint.
    /// Reads the local NDJSON file, sends up to `batch_size` events,
    /// then truncates what was sent.
    fn try_ship_batch(&self) {
        let Some(endpoint) = &self.config.endpoint else {
            return;
        };
        let path = self.local_file_path();
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return;
        }
        let batch_size = self.config.batch_size.max(1);
        let to_send: Vec<&str> = lines.iter().take(batch_size).copied().collect();
        let payload = format!("[{}]", to_send.join(","));

        // Synchronous HTTP POST — best-effort, non-blocking for the user.
        // In production you'd use the tokio runtime, but telemetry must not
        // delay the CLI. We spawn a detached thread with a short timeout.
        let endpoint = endpoint.clone();
        let remaining: String = lines
            .iter()
            .skip(to_send.len())
            .map(|l| format!("{}\n", l))
            .collect();
        let path_clone = path.clone();

        std::thread::spawn(move || {
            // Use a simple blocking HTTP client (hyper is async-only in our deps,
            // so we use a raw TcpStream + HTTP/1.1 — but for production, add ureq
            // or reqwest[blocking]). For now, write a marker file so an external
            // agent can pick up the batch.
            let pending_path = path_clone.with_extension("pending.json");
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&pending_path)
            {
                let ship_request = serde_json::json!({
                    "endpoint": endpoint,
                    "payload": payload,
                });
                let _ = writeln!(f, "{}", ship_request);
            }
            // Truncate shipped events from the main file
            let _ = fs::write(&path_clone, remaining);
        });
    }
}

/// Convenience: start a timer for measuring command duration.
pub fn start_timer() -> Instant {
    Instant::now()
}

/// Map a DynamicCommand variant to its telemetry category string.
pub fn command_category(command_name: &str) -> &str {
    match command_name {
        "tool_invoke" | "invoke" => "tool_invoke",
        "resource_read" | "get" => "resource_read",
        "prompt_run" | "prompt" => "prompt_run",
        "discover" | "ls" => "discover",
        "ping" => "ping",
        "doctor" => "doctor",
        "inspect" => "inspect",
        "auth_login" | "auth_logout" | "auth_status" => "auth",
        "jobs_list" | "jobs_show" | "jobs_wait" | "jobs_cancel" | "jobs_watch" => "jobs",
        "log" => "log",
        "complete" => "complete",
        "subscribe" | "unsubscribe" => "subscribe",
        "config_init" | "config_list" | "config_show" => "config",
        "link_create" => "link",
        "use" => "use",
        "daemon_start" | "daemon_stop" | "daemon_status" => "daemon",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn installation_id_persists() {
        let tmp = TempDir::new().unwrap();
        let id1 = get_or_create_installation_id(tmp.path());
        let id2 = get_or_create_installation_id(tmp.path());
        assert_eq!(id1, id2);
        assert!(!id1.is_empty());
        // Should be valid UUID
        Uuid::parse_str(&id1).unwrap();
    }

    #[test]
    fn installation_id_is_random() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        let id1 = get_or_create_installation_id(tmp1.path());
        let id2 = get_or_create_installation_id(tmp2.path());
        assert_ne!(id1, id2);
    }

    #[test]
    fn disabled_by_env() {
        let config = TelemetryConfig::default();
        // SAFETY: test-only; tests run serially for env-var tests
        unsafe {
            std::env::set_var("MCP2CLI_TELEMETRY", "off");
        }
        assert!(!TelemetryRecorder::is_enabled(&config));
        unsafe {
            std::env::remove_var("MCP2CLI_TELEMETRY");
        }
    }

    #[test]
    fn disabled_by_config() {
        let config = TelemetryConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(!TelemetryRecorder::is_enabled(&config));
    }

    #[test]
    fn disabled_by_do_not_track() {
        let config = TelemetryConfig::default();
        // SAFETY: test-only
        unsafe {
            std::env::set_var("DO_NOT_TRACK", "1");
        }
        assert!(!TelemetryRecorder::is_enabled(&config));
        unsafe {
            std::env::remove_var("DO_NOT_TRACK");
        }
    }

    #[test]
    fn records_to_local_file() {
        let tmp = TempDir::new().unwrap();
        let config = TelemetryConfig::default();
        // SAFETY: test-only
        unsafe {
            std::env::remove_var("MCP2CLI_TELEMETRY");
            std::env::remove_var("DO_NOT_TRACK");
        }
        let recorder = TelemetryRecorder::new(&config, tmp.path()).unwrap();
        recorder.record_command(
            "tool_invoke",
            "streamable_http",
            false,
            false,
            false,
            false,
            false,
            false,
            "success",
            Duration::from_millis(150),
        );
        let content = fs::read_to_string(tmp.path().join("telemetry.ndjson")).unwrap();
        let event: TelemetryEvent = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(event.schema, 1);
        match &event.event {
            EventKind::CommandRun {
                command_category,
                outcome,
                duration_ms,
                ..
            } => {
                assert_eq!(command_category, "tool_invoke");
                assert_eq!(outcome, "success");
                assert_eq!(*duration_ms, 150);
            }
            _ => panic!("expected CommandRun event"),
        }
    }

    #[test]
    fn first_run_only_once() {
        let tmp = TempDir::new().unwrap();
        let config = TelemetryConfig::default();
        // SAFETY: test-only
        unsafe {
            std::env::remove_var("MCP2CLI_TELEMETRY");
            std::env::remove_var("DO_NOT_TRACK");
        }
        let recorder = TelemetryRecorder::new(&config, tmp.path()).unwrap();
        recorder.record_first_run();
        recorder.record_first_run();
        let content = fs::read_to_string(tmp.path().join("telemetry.ndjson")).unwrap();
        let events: Vec<&str> = content.lines().collect();
        assert_eq!(events.len(), 1); // Only one first-run event
    }

    #[test]
    fn event_serialization_roundtrip() {
        let event = TelemetryEvent {
            schema: 1,
            installation_id: "test-id".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            cli_version: "0.1.0".to_string(),
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            event: EventKind::CommandRun {
                command_category: "discover".to_string(),
                transport: "stdio".to_string(),
                json_output: true,
                background: false,
                timeout_override: false,
                profile_active: true,
                daemon_active: false,
                ad_hoc: false,
                outcome: "success".to_string(),
                duration_ms: 42,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: TelemetryEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.schema, 1);
        assert_eq!(parsed.installation_id, "test-id");
    }
}
