//! Per-app CLI surfaces and shared app-context plumbing.
//!
//! Once [`crate::dispatch`] decides an invocation targets a specific
//! app (i.e. a named MCP server binding), this module owns the parse →
//! plan → execute path:
//!
//! - [`bridge`] — the **static** `BridgeCli`. A fixed `clap` tree
//!   (`ls`, `invoke`, `get`, `prompt`, `ping`, `doctor`, `auth`,
//!   `jobs`, `log`, `complete`, `subscribe`, `unsubscribe`, etc.).
//!   Used as a fallback when no discovery cache exists yet, and as
//!   the backing for protocol-shaped commands.
//! - [`dynamic`] — the **dynamic** CLI. When a discovery cache is
//!   available, [`dynamic::build_dynamic_cli`] reads a
//!   [`manifest::CommandManifest`] and materialises a server-specific
//!   `clap::Command` tree (one subcommand per tool, resource, or
//!   prompt, with flags derived from JSON Schema). Dotted tool names
//!   like `email.send` nest automatically.
//! - [`manifest`] — the model that powers `dynamic`. Transforms raw
//!   discovery items + an optional profile overlay (rename / hide /
//!   group / alias) into a flat list of commands with typed flag
//!   specs.
//!
//! [`AppContext`] threads config, runtime services, and timeout
//! overrides through each command path and exposes a single
//! `perform(operation)` → [`McpOperationResult`] helper.

pub mod bridge;
pub mod dynamic;
pub mod manifest;

use std::sync::Arc;

use serde_json::Value;

use crate::{
    config::AppConfig,
    mcp::{
        client::perform_with_timeout,
        model::{McpOperation, McpOperationResult},
    },
    runtime::{JobRecord, RuntimeServices},
};

/// Runtime context passed to the bridge for executing commands.
#[derive(Clone)]
pub struct AppContext {
    pub invoked_as: String,
    pub config_name: String,
    pub config: Arc<AppConfig>,
    pub services: RuntimeServices,
    /// Per-invocation timeout override (from --timeout flag). 0 = use config default.
    pub timeout_override: Option<u64>,
}

impl AppContext {
    /// Execute an MCP operation with the configured timeout.
    pub async fn perform(&self, operation: McpOperation) -> anyhow::Result<McpOperationResult> {
        let timeout = self
            .timeout_override
            .unwrap_or(self.config.defaults.timeout_seconds);
        perform_with_timeout(
            self.services.mcp_client.as_ref(),
            &self.config_name,
            operation,
            &self.services.event_broker,
            None,
            timeout,
        )
        .await
    }

    /// Execute an MCP operation with the configured timeout and an inventory stale path.
    pub async fn perform_with_stale_path(
        &self,
        operation: McpOperation,
        inventory_stale_path: &std::path::PathBuf,
    ) -> anyhow::Result<McpOperationResult> {
        let timeout = self
            .timeout_override
            .unwrap_or(self.config.defaults.timeout_seconds);
        perform_with_timeout(
            self.services.mcp_client.as_ref(),
            &self.config_name,
            operation,
            &self.services.event_broker,
            Some(inventory_stale_path),
            timeout,
        )
        .await
    }
}

pub fn default_job_overview_lines(job: &JobRecord) -> Vec<String> {
    let mut lines = vec![
        format!("job: {}", job.job_id),
        format!("status: {}", job.status.as_str()),
    ];
    if let Some(detail) = &job.detail {
        lines.push(format!("detail: {}", detail));
    }
    if let Some(failure_reason) = &job.failure_reason {
        lines.push(format!("failure: {}", failure_reason));
    }
    lines
}

pub fn default_job_detail_lines(job: &JobRecord) -> Vec<String> {
    let mut lines = vec![
        format!("job: {}", job.job_id),
        format!("command: {}", job.command),
        format!("status: {}", job.status.as_str()),
        format!(
            "remote task: {}",
            job.remote_task_id
                .clone()
                .unwrap_or_else(|| "(none)".to_owned())
        ),
        format!(
            "detail: {}",
            job.detail.clone().unwrap_or_else(|| "(none)".to_owned())
        ),
        format!(
            "failure: {}",
            job.failure_reason
                .clone()
                .unwrap_or_else(|| "(none)".to_owned())
        ),
    ];
    lines.extend(render_job_result_lines(job.result.as_ref()));
    lines.push(format!("created: {}", job.created_at.to_rfc3339()));
    lines.push(format!("updated: {}", job.updated_at.to_rfc3339()));
    lines
}

pub fn render_job_result_lines(result: Option<&Value>) -> Vec<String> {
    let Some(result) = result else {
        return vec!["result: (none)".to_owned()];
    };

    if let Some(object) = result.as_object() {
        let mut lines = Vec::new();
        if object.is_empty() {
            lines.push("result: {}".to_owned());
            return lines;
        }
        for (key, value) in object {
            lines.push(format!("result.{}: {}", key, compact_json(value)));
        }
        return lines;
    }

    vec![format!("result: {}", compact_json(result))]
}

pub fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<invalid-json>".to_owned())
}
