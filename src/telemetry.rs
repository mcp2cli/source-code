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

    /// HTTP endpoint for shipping events as JSON batches.
    /// Defaults to the first-party `otel.mcp2cli.dev/v1/traces`
    /// collector; can be overridden in user/app config or set to
    /// `null` to keep events purely local.
    #[serde(default = "default_endpoint")]
    pub endpoint: Option<String>,

    /// Maximum events to batch before flushing to HTTP endpoint. Default: 25.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            endpoint: default_endpoint(),
            batch_size: default_batch_size(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

/// Default collector URL. First-party, hosted on the mcp2cli.dev
/// zone; speaks the standard OTLP/HTTP trace-ingest protocol so any
/// OpenTelemetry Collector sitting behind it can receive us
/// natively. Sending is opt-out via the usual mechanisms:
/// `telemetry.enabled: false` in config, `MCP2CLI_TELEMETRY=off`,
/// `DO_NOT_TRACK=1`, or `--no-telemetry`.
pub const DEFAULT_TELEMETRY_ENDPOINT: &str = "https://otel.mcp2cli.dev/v1/traces";

fn default_endpoint() -> Option<String> {
    Some(DEFAULT_TELEMETRY_ENDPOINT.to_string())
}

fn default_batch_size() -> usize {
    25
}

// ---------------------------------------------------------------------------
// Event Model
// ---------------------------------------------------------------------------

/// A single anonymous telemetry event — one per CLI invocation.
///
/// The CLI's telemetry is deliberately disconnected from any website
/// or installer telemetry: nothing in this event links back to a
/// browser session or a specific curl install run. `installation_id`
/// is a random per-machine UUID that only ever leaves this process.
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

    /// Try to batch-ship events to the configured OTLP endpoint.
    /// Reads the local NDJSON file, converts up to `batch_size`
    /// events into a single OTLP/JSON `resourceSpans` payload, POSTs
    /// it, and — only on a 2xx response — truncates the shipped
    /// events from disk. Failures leave the file intact so the next
    /// invocation retries.
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

        // Parse each NDJSON line back into a TelemetryEvent; drop any
        // that fail (shouldn't happen — we wrote them — but a corrupt
        // line shouldn't block the whole batch).
        let events: Vec<TelemetryEvent> = to_send
            .iter()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        if events.is_empty() {
            return;
        }

        let payload = match serde_json::to_string(&to_otlp_payload(&events)) {
            Ok(p) => p,
            Err(e) => {
                debug!("telemetry: failed to build OTLP payload: {}", e);
                return;
            }
        };

        // Keep exactly the lines we didn't ship so we can rewrite the
        // file atomically if (and only if) the POST succeeds.
        let remaining: String = lines
            .iter()
            .skip(to_send.len())
            .map(|l| format!("{}\n", l))
            .collect();

        let endpoint = endpoint.clone();
        let path_clone = path.clone();
        let cli_version = env!("CARGO_PKG_VERSION").to_string();

        // Fire-and-forget on a detached std::thread so the CLI returns
        // to the user immediately. A short timeout means a dead
        // collector never blocks past a few seconds of background work.
        std::thread::spawn(move || {
            let user_agent = format!("mcp2cli/{cli_version}");
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(2))
                .timeout(Duration::from_secs(5))
                .user_agent(&user_agent)
                .build();
            let response = agent
                .post(&endpoint)
                .set("Content-Type", "application/json")
                .send_string(&payload);
            match response {
                Ok(r) if (200..300).contains(&r.status()) => {
                    // Only drop the shipped events from disk after a
                    // confirmed 2xx — otherwise we'd lose data on a
                    // transient collector failure.
                    if let Err(e) = fs::write(&path_clone, remaining) {
                        debug!("telemetry: failed to truncate after ship: {}", e);
                    }
                }
                Ok(r) => {
                    debug!("telemetry: collector returned HTTP {}", r.status());
                }
                Err(e) => {
                    debug!("telemetry: ship failed, keeping events local: {}", e);
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// OTLP/JSON payload construction
// ---------------------------------------------------------------------------

fn attr(key: &str, value: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "key": key, "value": value })
}
fn str_attr(key: &str, value: &str) -> serde_json::Value {
    attr(key, serde_json::json!({ "stringValue": value }))
}
fn bool_attr(key: &str, value: bool) -> serde_json::Value {
    attr(key, serde_json::json!({ "boolValue": value }))
}
fn int_attr(key: &str, value: u64) -> serde_json::Value {
    attr(key, serde_json::json!({ "intValue": value.to_string() }))
}

fn random_hex(bytes: usize) -> String {
    // UUIDv4 gives 16 cryptographically-random bytes; for span_id
    // (8 bytes) we just take the first half of another fresh UUID.
    let u = Uuid::new_v4();
    let slice = &u.as_bytes()[..bytes.min(16)];
    let mut out = String::with_capacity(slice.len() * 2);
    for b in slice {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

fn rfc3339_to_ns(ts: &str) -> u64 {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .and_then(|dt| dt.timestamp_nanos_opt())
        .unwrap_or(0) as u64
}

/// Build the OTLP/JSON `resourceSpans` envelope that wraps a batch
/// of parsed [`TelemetryEvent`]s. One resource block, one scope,
/// one span per event.
fn to_otlp_payload(events: &[TelemetryEvent]) -> serde_json::Value {
    // Resource attributes are common to all spans in this batch —
    // they describe the sending service, not any individual event.
    let first = events.first();
    let resource_attributes = serde_json::json!([
        str_attr("service.name", "mcp2cli-cli"),
        str_attr(
            "service.version",
            first
                .map(|e| e.cli_version.as_str())
                .unwrap_or(env!("CARGO_PKG_VERSION"))
        ),
        str_attr(
            "host.os",
            first.map(|e| e.os.as_str()).unwrap_or(std::env::consts::OS)
        ),
        str_attr(
            "host.arch",
            first
                .map(|e| e.arch.as_str())
                .unwrap_or(std::env::consts::ARCH)
        ),
    ]);

    let spans: Vec<serde_json::Value> = events.iter().map(event_to_span).collect();

    serde_json::json!({
        "resourceSpans": [{
            "resource": { "attributes": resource_attributes },
            "scopeSpans": [{
                "scope": { "name": "mcp2cli.telemetry", "version": "1" },
                "spans": spans,
            }],
        }]
    })
}

fn event_to_span(event: &TelemetryEvent) -> serde_json::Value {
    let ts_ns = rfc3339_to_ns(&event.timestamp);

    let mut attributes: Vec<serde_json::Value> =
        vec![str_attr("mcp2cli.installation_id", &event.installation_id)];

    let (name, status_code, dur_ns): (&str, u8, u64) = match &event.event {
        EventKind::CommandRun {
            command_category,
            transport,
            json_output,
            background,
            timeout_override,
            profile_active,
            daemon_active,
            ad_hoc,
            outcome,
            duration_ms,
        } => {
            attributes.push(str_attr("mcp2cli.command.category", command_category));
            attributes.push(str_attr("mcp2cli.transport", transport));
            attributes.push(str_attr("mcp2cli.outcome", outcome));
            attributes.push(bool_attr("mcp2cli.json_output", *json_output));
            attributes.push(bool_attr("mcp2cli.background", *background));
            attributes.push(bool_attr("mcp2cli.timeout_override", *timeout_override));
            attributes.push(bool_attr("mcp2cli.profile_active", *profile_active));
            attributes.push(bool_attr("mcp2cli.daemon_active", *daemon_active));
            attributes.push(bool_attr("mcp2cli.ad_hoc", *ad_hoc));
            attributes.push(int_attr("mcp2cli.duration_ms", *duration_ms));
            let status = if outcome == "success" { 1 } else { 2 };
            ("command_run", status, *duration_ms * 1_000_000)
        }
        EventKind::FirstRun => ("first_run", 1, 0),
    };

    serde_json::json!({
        "traceId": random_hex(16),
        "spanId": random_hex(8),
        "name": name,
        "kind": 1,
        "startTimeUnixNano": ts_ns.to_string(),
        "endTimeUnixNano": (ts_ns + dur_ns).to_string(),
        "attributes": attributes,
        "status": { "code": status_code },
    })
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
