//! Runtime event bus — the in-process observability channel.
//!
//! [`RuntimeEvent`] is the single event type emitted by any layer
//! (protocol, transport, runtime, bridge) that has something worth
//! reporting: progress ticks, server logs, list-changed signals, tool
//! displays for sampling, elicitation prompts, resource updates, and
//! cancellations.
//!
//! [`EventBroker`] fans a single event out to every registered
//! [`EventSink`] without blocking the producer. Sinks today include:
//!
//! - [`StderrEventSink`] — human-readable status on `stderr`.
//! - [`MemoryEventSink`] — capture into a `Vec` for tests.
//! - [`crate::runtime::sinks::HttpWebhookSink`] — POST NDJSON events
//!   to a webhook.
//! - [`crate::runtime::sinks::UnixSocketSink`] — stream NDJSON over a
//!   Unix domain socket (e.g. for a local watcher/ sidecar).
//! - [`crate::runtime::sinks::SseServerSink`] — expose an HTTP Server-
//!   Sent Events endpoint so a UI can tail a live session.
//! - [`crate::runtime::sinks::CommandExecSink`] — spawn a user command
//!   with the serialised event on stdin.

use std::sync::{Arc, Mutex};

use serde::Serialize;

/// Structured event emitted during command execution.
///
/// The `#[serde(tag = "type")]` representation serialises each variant
/// with a top-level `"type"` discriminator. JSON sinks and the
/// `--json` output mode read this directly without schema drift.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeEvent {
    Info {
        app_id: String,
        message: String,
    },
    Progress {
        app_id: String,
        operation: String,
        current: u64,
        total: Option<u64>,
        message: String,
    },
    JobUpdate {
        app_id: String,
        job_id: String,
        status: String,
        message: String,
    },
    AuthPrompt {
        app_id: String,
        message: String,
    },
    /// Server-sent log message (`notifications/message`).
    ServerLog {
        app_id: String,
        level: String,
        logger: String,
        message: String,
    },
    /// Server notified that its capability list changed.
    ListChanged {
        app_id: String,
        kind: String,
        message: String,
    },
}

impl RuntimeEvent {
    pub fn human_line(&self) -> String {
        match self {
            Self::Info { app_id, message } => format!("[{}] {}", app_id, message),
            Self::Progress {
                app_id,
                operation,
                current,
                total,
                message,
            } => match total {
                Some(total) => format!(
                    "[{}] {} {}/{} {}",
                    app_id, operation, current, total, message
                ),
                None => format!("[{}] {} {} {}", app_id, operation, current, message),
            },
            Self::JobUpdate {
                app_id,
                job_id,
                status,
                message,
            } => format!("[{}] job {} {} {}", app_id, job_id, status, message),
            Self::AuthPrompt { app_id, message } => format!("[{}] auth {}", app_id, message),
            Self::ServerLog {
                app_id,
                level,
                logger,
                message,
            } => {
                if logger.is_empty() {
                    format!("[{}] server {}: {}", app_id, level, message)
                } else {
                    format!("[{}] server {} ({}): {}", app_id, level, logger, message)
                }
            }
            Self::ListChanged {
                app_id,
                kind: _,
                message,
            } => format!("[{}] {}", app_id, message),
        }
    }

    /// Returns the event type tag as a string.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Info { .. } => "info",
            Self::Progress { .. } => "progress",
            Self::JobUpdate { .. } => "job_update",
            Self::AuthPrompt { .. } => "auth_prompt",
            Self::ServerLog { .. } => "server_log",
            Self::ListChanged { .. } => "list_changed",
        }
    }

    /// Returns the app_id of this event.
    pub fn app_id(&self) -> &str {
        match self {
            Self::Info { app_id, .. }
            | Self::Progress { app_id, .. }
            | Self::JobUpdate { app_id, .. }
            | Self::AuthPrompt { app_id, .. }
            | Self::ServerLog { app_id, .. }
            | Self::ListChanged { app_id, .. } => app_id,
        }
    }
}

/// Trait for runtime event delivery targets (stderr, memory, HTTP webhook, SSE, etc.).
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &RuntimeEvent);
}

#[derive(Default)]
pub struct MemoryEventSink {
    events: Mutex<Vec<RuntimeEvent>>,
}

impl MemoryEventSink {
    pub fn events(&self) -> Vec<RuntimeEvent> {
        self.events
            .lock()
            .expect("event sink lock poisoned")
            .clone()
    }
}

impl EventSink for MemoryEventSink {
    fn emit(&self, event: &RuntimeEvent) {
        self.events
            .lock()
            .expect("event sink lock poisoned")
            .push(event.clone());
    }
}

pub struct StderrEventSink;

impl EventSink for StderrEventSink {
    fn emit(&self, event: &RuntimeEvent) {
        eprintln!("{}", event.human_line());
    }
}

/// Fan-out event broker that delivers each event to all registered sinks.
#[derive(Clone, Default)]
pub struct EventBroker {
    sinks: Arc<Vec<Arc<dyn EventSink>>>,
}

impl EventBroker {
    pub fn new(sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self {
            sinks: Arc::new(sinks),
        }
    }

    pub fn emit(&self, event: RuntimeEvent) {
        for sink in self.sinks.iter() {
            sink.emit(&event);
        }
    }
}
