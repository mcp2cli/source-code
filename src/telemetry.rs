//! Anonymous usage telemetry for mcp2cli.
//!
//! Collects **non-sensitive** usage data to understand which features are
//! used, what fails, and where to invest development effort.
//!
//! ## Privacy guarantees
//! - No server endpoints, URIs, arguments, or tool names are recorded.
//! - The installation ID is a random UUID (not derived from user identity).
//! - No resource attribute identifies the machine or user (no hostname,
//!   username, or process id) — only the coarse platform family already
//!   in [`TelemetryEvent::os`]/[`TelemetryEvent::arch`].
//! - Telemetry is opt-out: disable via config, env var, or CLI flag.
//!
//! ## Data flow
//! 1. Each command invocation produces a [`TelemetryEvent`].
//! 2. Events are appended to a local NDJSON file (`telemetry.ndjson`).
//! 3. Optionally, events are converted to an OTLP/HTTP JSON `resourceSpans`
//!    batch (see [`to_otlp_payload`]) and POSTed to a configurable
//!    endpoint, tagged with `service.namespace` so the shared backend
//!    files them under the right project.
//! 4. [`TelemetryRecorder::flush`] gives that POST a short bounded window
//!    to finish before the process exits, since a detached shipping
//!    thread would otherwise be killed mid-flight by a short-lived CLI
//!    invocation — see its doc comment for why this matters.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    thread::JoinHandle,
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

    /// OTLP/HTTP endpoint events are shipped to as `resourceSpans`
    /// batches. Defaults to the first-party `telemetry.mcp2cli.dev`
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

/// Default collector URL — the tsok observability stack's dedicated
/// mcp2cli ingest endpoint (Grafana + Tempo/Loki/Prometheus behind
/// `telemetry.mcp2cli.dev`). Speaks standard OTLP/HTTP JSON, so any
/// OpenTelemetry Collector can receive us natively. Data is filed under
/// this project by the `service.namespace` resource attribute (see
/// [`to_otlp_payload`]) — the endpoint itself does not scope the data.
/// Sending is opt-out via the usual mechanisms: `telemetry.enabled:
/// false` in config, `MCP2CLI_TELEMETRY=off`, `DO_NOT_TRACK=1`, or
/// `--no-telemetry`.
pub const DEFAULT_TELEMETRY_ENDPOINT: &str = "https://telemetry.mcp2cli.dev/v1/traces";

/// OTel resource attribute that files this app's telemetry under the
/// `mcp2cli` project on the shared backend. MUST be a resource
/// attribute (not a span attribute) or the collector's dashboards won't
/// group the data — see [`to_otlp_payload`].
const OTEL_SERVICE_NAMESPACE: &str = "mcp2cli";

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
        /// Negotiated MCP protocol era, when a session was negotiated:
        /// "legacy" (2025-11-25 initialize handshake) or "modern"
        /// (2026-07-28 stateless). `None` for host commands, ad-hoc
        /// connections that never reached a server, and daemon-routed
        /// calls.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protocol_era: Option<String>,
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

/// How long [`TelemetryRecorder::flush`] waits for in-flight shipping
/// threads before letting the process exit anyway. Detached
/// `std::thread::spawn` threads are killed outright when the process
/// exits, so without a bounded wait here a short-lived CLI invocation
/// almost never gives its own shipping attempt enough time to complete
/// a network round trip — events would accumulate locally but rarely
/// actually ship. Short enough that a dead or slow collector can't
/// meaningfully delay the shell prompt returning; the user's command
/// output is already printed by this point regardless.
const SHIP_FLUSH_TIMEOUT: Duration = Duration::from_millis(250);

/// Handles recording telemetry events to local file and optional remote endpoint.
pub struct TelemetryRecorder {
    config: TelemetryConfig,
    data_root: PathBuf,
    installation_id: String,
    /// Handles of detached shipping threads spawned by [`Self::persist`]
    /// that haven't been waited on yet. Drained by [`Self::flush`].
    pending_ships: Mutex<Vec<JoinHandle<()>>>,
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
            pending_ships: Mutex::new(Vec::new()),
        })
    }

    /// Give any in-flight background shipping threads a short, bounded
    /// window ([`SHIP_FLUSH_TIMEOUT`]) to finish. Call once, right
    /// before the process would otherwise exit. A thread that hasn't
    /// finished within the window is simply left running — it dies with
    /// the process exactly as it would have without this call — and its
    /// events remain on local disk (only a confirmed 2xx truncates them)
    /// for the next invocation to retry.
    pub fn flush(&self) {
        let handles = {
            let mut pending = self
                .pending_ships
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::take(&mut *pending)
        };
        let deadline = Instant::now() + SHIP_FLUSH_TIMEOUT;
        for handle in handles {
            while !handle.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
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
        protocol_era: Option<&str>,
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
            protocol_era: protocol_era.map(str::to_owned),
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

        // Runs on a detached std::thread so the CLI's actual work never
        // waits on network I/O — [`Self::flush`] later gives this thread
        // a short bounded window to finish before the process exits,
        // rather than the process racing ahead and simply killing it.
        // The 2s/5s timeouts here are an independent upper bound in case
        // the thread outlives that flush window (best-effort
        // continuation); they don't block the CLI itself.
        let handle = std::thread::spawn(move || {
            let user_agent = format!("mcp2cli/{cli_version}");
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(2))
                .timeout(Duration::from_secs(5))
                .user_agent(&user_agent)
                .tls_config(std::sync::Arc::new(crate::tls::client_config()))
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
        if let Ok(mut pending) = self.pending_ships.lock() {
            pending.push(handle);
        }
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
    // Resource attributes are common to all spans in this batch — they
    // describe the sending service, not any individual event.
    //
    // `service.namespace` is the field that actually files this data
    // under the mcp2cli project on the shared backend; the endpoint URL
    // itself carries no project identity. Deliberately absent: any
    // resource attribute that would identify the machine or user
    // (hostname, process id, username, detailed OS version banner) —
    // `mcp2cli.os`/`mcp2cli.arch` stay at the coarse platform-family
    // granularity already used locally (see `TelemetryEvent::os`/`arch`).
    let first = events.first();
    let resource_attributes = serde_json::json!([
        str_attr("service.name", "mcp2cli-cli"),
        str_attr("service.namespace", OTEL_SERVICE_NAMESPACE),
        str_attr(
            "service.version",
            first
                .map(|e| e.cli_version.as_str())
                .unwrap_or(env!("CARGO_PKG_VERSION"))
        ),
        str_attr(
            "mcp2cli.os",
            first.map(|e| e.os.as_str()).unwrap_or(std::env::consts::OS)
        ),
        str_attr(
            "mcp2cli.arch",
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
            protocol_era,
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
            if let Some(era) = protocol_era {
                attributes.push(str_attr("mcp2cli.protocol_era", era));
            }
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
    use std::thread;
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
            Some("modern"),
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
                protocol_era,
                ..
            } => {
                assert_eq!(command_category, "tool_invoke");
                assert_eq!(outcome, "success");
                assert_eq!(*duration_ms, 150);
                assert_eq!(protocol_era.as_deref(), Some("modern"));
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
                protocol_era: Some("legacy".to_string()),
                outcome: "success".to_string(),
                duration_ms: 42,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: TelemetryEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.schema, 1);
        assert_eq!(parsed.installation_id, "test-id");
    }

    fn sample_event(protocol_era: Option<&str>) -> TelemetryEvent {
        TelemetryEvent {
            schema: 1,
            installation_id: "test-id".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            cli_version: "9.9.9".to_string(),
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
                protocol_era: protocol_era.map(str::to_owned),
                outcome: "success".to_string(),
                duration_ms: 42,
            },
        }
    }

    fn resource_attributes(payload: &serde_json::Value) -> &Vec<serde_json::Value> {
        payload["resourceSpans"][0]["resource"]["attributes"]
            .as_array()
            .expect("resource attributes should be an array")
    }

    fn attr_str_value<'a>(attributes: &'a [serde_json::Value], key: &str) -> Option<&'a str> {
        attributes
            .iter()
            .find(|attribute| attribute["key"] == key)?
            .get("value")?
            .get("stringValue")?
            .as_str()
    }

    #[test]
    fn otlp_payload_files_data_under_the_mcp2cli_project() {
        // service.namespace is the field that actually routes this data
        // to the mcp2cli project on the shared backend — the endpoint
        // URL itself carries no project identity, so this attribute
        // being present and correct is the single most important
        // correctness property of the payload.
        let payload = to_otlp_payload(&[sample_event(Some("modern"))]);
        let attributes = resource_attributes(&payload);
        assert_eq!(
            attr_str_value(attributes, "service.namespace"),
            Some("mcp2cli")
        );
        assert_eq!(
            attr_str_value(attributes, "service.name"),
            Some("mcp2cli-cli")
        );
        assert_eq!(attr_str_value(attributes, "service.version"), Some("9.9.9"));
    }

    #[test]
    fn otlp_payload_resource_attributes_carry_no_identifying_data() {
        // Explicit negative check: no hostname, username, process id, or
        // any other machine/user-identifying resource attribute — only
        // the coarse platform family the local schema already exposes.
        let payload = to_otlp_payload(&[sample_event(None)]);
        let attributes = resource_attributes(&payload);
        let keys: Vec<&str> = attributes
            .iter()
            .filter_map(|attribute| attribute["key"].as_str())
            .collect();
        for forbidden in [
            "host.name",
            "host.id",
            "process.pid",
            "user.name",
            "os.description",
        ] {
            assert!(
                !keys.contains(&forbidden),
                "resource attributes must not include '{forbidden}': {keys:?}"
            );
        }
    }

    #[test]
    fn otlp_span_includes_protocol_era_only_when_negotiated() {
        let with_era = event_to_span(&sample_event(Some("legacy")));
        let span_attrs = with_era["attributes"].as_array().unwrap();
        assert_eq!(
            attr_str_value(span_attrs, "mcp2cli.protocol_era"),
            Some("legacy")
        );

        let without_era = event_to_span(&sample_event(None));
        let span_attrs = without_era["attributes"].as_array().unwrap();
        assert!(attr_str_value(span_attrs, "mcp2cli.protocol_era").is_none());
    }

    #[test]
    fn default_endpoint_targets_the_dedicated_telemetry_host() {
        assert_eq!(
            DEFAULT_TELEMETRY_ENDPOINT,
            "https://telemetry.mcp2cli.dev/v1/traces"
        );
    }

    /// Accept one HTTP POST on a localhost listener and reply 200 OK.
    /// Stands in for the collector so shipping can be exercised without
    /// a real network call.
    fn accept_one_and_reply_ok(listener: std::net::TcpListener) {
        use std::io::Read;
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0_u8; 8192];
        // Best-effort drain — we don't need the body, just to let the
        // client finish writing before we reply.
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.read(&mut buf);
        let _ =
            stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
    }

    #[test]
    fn flush_waits_for_shipping_to_actually_complete() {
        // Regression test: a detached std::thread spawned to ship events
        // is killed outright when the process exits, so without a
        // bounded wait a short-lived CLI invocation almost never gives
        // it time to finish. This proves record_command + flush() leaves
        // the local file empty against a real (if local) HTTP round
        // trip, not just that the code compiles.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || accept_one_and_reply_ok(listener));

        let tmp = TempDir::new().unwrap();
        let config = TelemetryConfig {
            endpoint: Some(format!("http://{addr}/v1/traces")),
            ..TelemetryConfig::default()
        };
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
            None,
            "success",
            Duration::from_millis(10),
        );
        recorder.flush();

        server.join().unwrap();
        let content = fs::read_to_string(tmp.path().join("telemetry.ndjson")).unwrap_or_default();
        assert!(
            content.is_empty(),
            "expected the shipped event to be truncated from disk, got: {content:?}"
        );
    }

    #[test]
    fn flush_returns_promptly_when_the_collector_never_responds() {
        // The other half of the contract: flush() must not hang forever
        // waiting on a dead/slow collector — it bounds the wait and lets
        // the process exit anyway, leaving the event safely on disk.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        // Accept the connection but never reply — simulates a collector
        // that's up but hung.
        let _server = thread::spawn(move || {
            let _ = listener.accept();
            thread::sleep(Duration::from_secs(10));
        });

        let tmp = TempDir::new().unwrap();
        let config = TelemetryConfig {
            endpoint: Some(format!("http://{addr}/v1/traces")),
            ..TelemetryConfig::default()
        };
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
            None,
            "success",
            Duration::from_millis(10),
        );

        let started = Instant::now();
        recorder.flush();
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "flush() should bound its wait, took {:?}",
            started.elapsed()
        );

        // The event is still there — it was never shipped — ready for
        // the next invocation to retry.
        let content = fs::read_to_string(tmp.path().join("telemetry.ndjson")).unwrap();
        assert!(!content.is_empty());
    }
}
