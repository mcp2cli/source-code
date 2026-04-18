//! Persistent state store.
//!
//! [`StateStore`] owns everything mcp2cli persists between runs:
//!
//! - **Discovery inventory** — the last-seen list of tools, resources,
//!   resource templates, and prompts for each named config. Powers
//!   the dynamic CLI without re-querying the server on every
//!   invocation; invalidated by
//!   `notifications/tools/list_changed` / `.../resources/list_changed`
//!   / `.../prompts/list_changed`.
//! - **Negotiated capability snapshot** — what the server advertised
//!   during the last `initialize`. Used by `mcp2cli doctor` and by
//!   dispatch logic that needs to skip unsupported operations early.
//! - **Auth session records** — long-lived credentials (e.g. OAuth
//!   authorization-code flows) tied to a config. Bearer tokens
//!   themselves live in [`crate::runtime::TokenStore`]; this store
//!   only tracks session metadata (state, last refresh, scopes).
//! - **Job records** — for `--background` invocations,
//!   [`JobRecord`]s persist job id, server-side task id, status,
//!   start/update timestamps, and final result until the user
//!   acknowledges with `jobs clear` or TTL expires. Enables
//!   `jobs show/wait/cancel/watch` across invocations.
//!
//! Storage is simple JSON files under the runtime data dir
//! ([`crate::config::RuntimeLayout`]). Writes are serialised through
//! an in-process `Mutex` so concurrent commands within a single
//! process don't race; cross-process writes rely on file locking
//! implicit in atomic rename.

use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{fs, sync::Mutex};
use uuid::Uuid;

use crate::mcp::model::DiscoveryCategory;
use crate::mcp::protocol::{McpClientSession, PeerInfo, ServerCapabilities};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthSessionState {
    Authenticated,
    LoggedOut,
}

impl AuthSessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Authenticated => "authenticated",
            Self::LoggedOut => "logged_out",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthSessionRecord {
    pub config_name: String,
    pub app_id: String,
    pub state: AuthSessionState,
    pub account: Option<String>,
    pub server: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Canceled,
    Failed,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Canceled => "canceled",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobRecord {
    pub job_id: String,
    pub config_name: String,
    pub app_id: String,
    pub command: String,
    pub status: JobStatus,
    pub detail: Option<String>,
    pub remote_task_id: Option<String>,
    pub result: Option<Value>,
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NegotiatedCapabilityView {
    pub config_name: String,
    pub app_id: String,
    pub protocol_version: String,
    pub session_id: Option<String>,
    pub server_info: Option<PeerInfo>,
    pub server_capabilities: ServerCapabilities,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryInventoryView {
    pub config_name: String,
    pub app_id: String,
    pub tools: Option<Vec<Value>>,
    pub resources: Option<Vec<Value>>,
    pub resource_templates: Option<Vec<Value>>,
    pub prompts: Option<Vec<Value>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct PersistedState {
    #[serde(default)]
    auth_sessions: BTreeMap<String, AuthSessionRecord>,
    #[serde(default)]
    negotiated_capabilities: BTreeMap<String, NegotiatedCapabilityView>,
    #[serde(default)]
    discovery_inventory: BTreeMap<String, DiscoveryInventoryView>,
    #[serde(default)]
    jobs: Vec<JobRecord>,
}

pub struct StateStore {
    path: PathBuf,
    state: Mutex<PersistedState>,
}

impl StateStore {
    pub async fn load(path: PathBuf) -> Result<Self> {
        let state = if path.exists() {
            let bytes = fs::read(&path)
                .await
                .with_context(|| format!("failed to read state file: {}", path.display()))?;
            serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse state file: {}", path.display()))?
        } else {
            PersistedState::default()
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    pub async fn auth_session(&self, config_name: &str) -> Option<AuthSessionRecord> {
        self.state
            .lock()
            .await
            .auth_sessions
            .get(config_name)
            .cloned()
    }

    pub async fn upsert_auth_session(&self, record: AuthSessionRecord) -> Result<()> {
        let bytes = {
            let mut state = self.state.lock().await;
            state
                .auth_sessions
                .insert(record.config_name.clone(), record);
            serde_json::to_vec_pretty(&*state).context("failed to serialize auth state")?
        };
        self.persist_bytes(bytes).await
    }

    pub async fn negotiated_capability_view(
        &self,
        config_name: &str,
    ) -> Option<NegotiatedCapabilityView> {
        self.state
            .lock()
            .await
            .negotiated_capabilities
            .get(config_name)
            .cloned()
    }

    pub async fn upsert_negotiated_capability_view(
        &self,
        config_name: &str,
        app_id: &str,
        session: &McpClientSession,
    ) -> Result<()> {
        let Some(server_capabilities) = session.server_capabilities.clone() else {
            return Ok(());
        };

        let record = NegotiatedCapabilityView {
            config_name: config_name.to_owned(),
            app_id: app_id.to_owned(),
            protocol_version: session.protocol_version.clone(),
            session_id: session.session_id.clone(),
            server_info: session.server_info.clone(),
            server_capabilities,
            updated_at: Utc::now(),
        };
        let bytes = {
            let mut state = self.state.lock().await;
            state
                .negotiated_capabilities
                .insert(config_name.to_owned(), record);
            serde_json::to_vec_pretty(&*state)
                .context("failed to serialize negotiated capability state")?
        };
        self.persist_bytes(bytes).await
    }

    pub async fn discovery_inventory_view(
        &self,
        config_name: &str,
    ) -> Option<DiscoveryInventoryView> {
        self.state
            .lock()
            .await
            .discovery_inventory
            .get(config_name)
            .cloned()
    }

    pub async fn upsert_discovery_inventory(
        &self,
        config_name: &str,
        app_id: &str,
        category: DiscoveryCategory,
        items: Vec<Value>,
    ) -> Result<()> {
        let bytes = {
            let mut state = self.state.lock().await;
            let entry = state
                .discovery_inventory
                .entry(config_name.to_owned())
                .or_insert_with(|| DiscoveryInventoryView {
                    config_name: config_name.to_owned(),
                    app_id: app_id.to_owned(),
                    tools: None,
                    resources: None,
                    resource_templates: None,
                    prompts: None,
                    updated_at: Utc::now(),
                });
            entry.app_id = app_id.to_owned();
            entry.updated_at = Utc::now();
            match category {
                DiscoveryCategory::Capabilities => entry.tools = Some(items),
                DiscoveryCategory::Resources => {
                    // Separate concrete resources from resource templates
                    let mut concrete = Vec::new();
                    let mut templates = Vec::new();
                    for item in items {
                        if item.get("kind").and_then(Value::as_str) == Some("resource_template")
                            || item.get("uriTemplate").is_some()
                        {
                            templates.push(item);
                        } else {
                            concrete.push(item);
                        }
                    }
                    entry.resources = Some(concrete);
                    if !templates.is_empty() {
                        entry.resource_templates = Some(templates);
                    }
                }
                DiscoveryCategory::Prompts => entry.prompts = Some(items),
            }

            serde_json::to_vec_pretty(&*state)
                .context("failed to serialize discovery inventory state")?
        };
        self.persist_bytes(bytes).await
    }

    pub async fn create_job(
        &self,
        config_name: &str,
        app_id: &str,
        command: &str,
        detail: Option<String>,
        remote_task_id: Option<String>,
    ) -> Result<JobRecord> {
        let job = JobRecord {
            job_id: Uuid::new_v4().to_string(),
            config_name: config_name.to_owned(),
            app_id: app_id.to_owned(),
            command: command.to_owned(),
            status: JobStatus::Queued,
            detail,
            remote_task_id,
            result: None,
            failure_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let bytes = {
            let mut state = self.state.lock().await;
            state.jobs.push(job.clone());
            serde_json::to_vec_pretty(&*state).context("failed to serialize job state")?
        };
        self.persist_bytes(bytes).await?;
        Ok(job)
    }

    pub async fn jobs_for_config(&self, config_name: &str) -> Vec<JobRecord> {
        let mut jobs = self
            .state
            .lock()
            .await
            .jobs
            .iter()
            .filter(|job| job.config_name == config_name)
            .cloned()
            .collect::<Vec<_>>();
        jobs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        jobs
    }

    pub async fn job_for_config(&self, config_name: &str, job_id: &str) -> Option<JobRecord> {
        self.state
            .lock()
            .await
            .jobs
            .iter()
            .find(|job| job.config_name == config_name && job.job_id == job_id)
            .cloned()
    }

    pub async fn latest_job_for_config(
        &self,
        config_name: &str,
        command: Option<&str>,
    ) -> Option<JobRecord> {
        self.jobs_for_config(config_name)
            .await
            .into_iter()
            .find(|job| command.map(|value| job.command == value).unwrap_or(true))
    }

    pub async fn update_job_status(
        &self,
        config_name: &str,
        job_id: &str,
        status: JobStatus,
        detail: Option<String>,
        result: Option<Value>,
        failure_reason: Option<String>,
    ) -> Result<JobRecord> {
        let updated = {
            let mut state = self.state.lock().await;
            let job = state
                .jobs
                .iter_mut()
                .find(|job| job.config_name == config_name && job.job_id == job_id)
                .with_context(|| {
                    format!("job '{}' not found for config '{}'", job_id, config_name)
                })?;

            job.status = status;
            if detail.is_some() {
                job.detail = detail;
            }
            if result.is_some() {
                job.result = result;
            }
            job.failure_reason = failure_reason;
            job.updated_at = Utc::now();
            let snapshot = job.clone();
            let bytes =
                serde_json::to_vec_pretty(&*state).context("failed to serialize job state")?;
            (snapshot, bytes)
        };

        self.persist_bytes(updated.1).await?;
        Ok(updated.0)
    }

    async fn persist_bytes(&self, bytes: Vec<u8>) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!("failed to create state directory: {}", parent.display())
            })?;
        }
        fs::write(&self.path, bytes)
            .await
            .with_context(|| format!("failed to write state file: {}", self.path.display()))
    }
}
