//! Transport-neutral MCP operation and result types.
//!
//! These enums are the common vocabulary shared by every layer above
//! the wire. [`crate::apps`] lowers user-facing commands to an
//! [`McpOperation`]; [`crate::mcp::protocol`] turns it into JSON-RPC;
//! [`crate::mcp::client`] sends it; decoded responses come back as
//! an [`McpOperationResult`] for rendering.
//!
//! Keeping protocol-method strings out of the rest of the crate means
//! a version bump in the MCP spec is a change inside the `mcp`
//! module and nowhere else.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wire transport the client configuration targets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    Stdio,
    StreamableHttp,
}

impl TransportKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::StreamableHttp => "streamable_http",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Running,
    Completed,
    Canceled,
    Failed,
}

impl TaskState {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryCategory {
    Capabilities,
    Resources,
    Prompts,
}

impl DiscoveryCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Capabilities => "capabilities",
            Self::Resources => "resources",
            Self::Prompts => "prompts",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpOperation {
    InvokeAction {
        capability: String,
        arguments: Value,
        background: bool,
    },
    ReadResource {
        uri: String,
    },
    Discover {
        category: DiscoveryCategory,
    },
    RunPrompt {
        name: String,
        arguments: Value,
    },
    /// MCP ping — server liveness check.
    Ping,
    /// Set logging level on the server.
    SetLoggingLevel {
        level: String,
    },
    /// Request tab-completion from the server.
    Complete {
        /// "ref/prompt" or "ref/resource"
        ref_kind: String,
        /// The prompt/resource name to complete
        ref_name: String,
        /// The argument name being completed
        argument_name: String,
        /// Current partial value
        argument_value: String,
        /// Previously-resolved argument values for context (MCP 2025-11-25).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<serde_json::Map<String, Value>>,
    },
    /// Subscribe to resource change notifications.
    SubscribeResource {
        uri: String,
    },
    /// Unsubscribe from resource change notifications.
    UnsubscribeResource {
        uri: String,
    },
    /// MCP tasks/get — retrieve task status.
    TaskGet {
        task_id: String,
    },
    /// MCP tasks/result — retrieve completed task result (may block).
    TaskResult {
        task_id: String,
    },
    /// MCP tasks/cancel — cancel a running task.
    TaskCancel {
        task_id: String,
    },
    /// List resource templates separately.
    DiscoverResourceTemplates,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpOperationResult {
    Action {
        message: String,
        data: Value,
    },
    Resource {
        message: String,
        uri: String,
        mime_type: Option<String>,
        text: Option<String>,
        data: Value,
    },
    Discovery {
        message: String,
        category: DiscoveryCategory,
        items: Vec<Value>,
    },
    Prompt {
        message: String,
        name: String,
        output: String,
        data: Value,
    },
    TaskAccepted {
        message: String,
        remote_task_id: Option<String>,
        detail: Value,
    },
    Task {
        status: TaskState,
        message: String,
        remote_task_id: String,
        data: Value,
        result: Option<Value>,
        failure_reason: Option<String>,
    },
    /// Server responded to ping.
    Pong {
        message: String,
    },
    /// Logging level set confirmation.
    LoggingLevelSet {
        message: String,
        level: String,
    },
    /// Tab-completion result.
    Completion {
        message: String,
        values: Vec<String>,
        has_more: bool,
        total: Option<u64>,
    },
    /// Resource subscription confirmed.
    Subscribed {
        message: String,
        uri: String,
    },
    /// Resource unsubscription confirmed.
    Unsubscribed {
        message: String,
        uri: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionMetadata {
    pub app_id: String,
    pub server_name: String,
    pub server_version: String,
    pub transport: TransportKind,
}
