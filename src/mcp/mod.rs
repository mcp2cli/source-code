//! MCP protocol layer.
//!
//! This module owns everything wire-format-related:
//!
//! - [`model`] — transport-neutral operation and result types
//!   (`McpOperation`, `McpOperationResult`, `DiscoveryCategory`,
//!   `TaskState`). These are what the rest of the crate traffics in.
//! - [`protocol`] — JSON-RPC framing, the `initialize` → server-caps
//!   state machine, and the mapping from [`model::McpOperation`] to
//!   MCP request/response shapes (`tools/call`, `resources/read`,
//!   `prompts/get`, `completion/complete`, `logging/setLevel`,
//!   `resources/subscribe`, `tasks/*`, `ping`). Also owns progress-token
//!   injection for operations that support streaming progress.
//! - [`client`] — the [`client::McpClient`] trait and its four
//!   implementations: `StdioMcpClient`, `StreamableHttpMcpClient`,
//!   `DaemonMcpClient` (IPC to a local reuse daemon), and a demo
//!   backend. This is the transport boundary.
//! - [`handler`] — server→client message dispatch. MCP is bidirectional;
//!   servers can send notifications (`notifications/progress`,
//!   `notifications/resources/updated`, `notifications/message`,
//!   `notifications/cancelled`) and requests
//!   (`elicitation/create`, `sampling/createMessage`, `roots/list`).
//!   The [`handler::ServerMessageHandler`] trait is what transports
//!   call into.
//! - [`vsock_shim`] — the AF_VSOCK / AF_UNIX dial-and-pipe backend used
//!   when `mcp2cli` is invoked as an `mcp-<server>-<tool>` symlink.

pub mod client;
pub mod handler;
pub mod model;
pub mod protocol;
pub mod vsock_shim;
