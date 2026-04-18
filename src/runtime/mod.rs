//! Runtime services and command execution.
//!
//! Everything in this module exists to turn a resolved
//! [`dispatch::DispatchTarget`](crate::dispatch::DispatchTarget) into
//! side effects:
//!
//! - [`host`] — the execution engine. Given a dispatch target and an
//!   [`AppContext`](crate::apps::AppContext), it picks between the
//!   dynamic and static CLI, drives the MCP operation, renders output,
//!   and handles host-level subcommands (`config`, `link`, `use`,
//!   `daemon`, `man`). Also owns the `mcp-<server>-<tool>` shim runtime.
//! - [`state`] — the on-disk state store. Discovery inventory cache,
//!   negotiated capability snapshots, auth session records, job records
//!   for background operations. Backed by JSON files under the project
//!   data dir.
//! - [`events`] — the in-process event bus. [`RuntimeEvent`] carries
//!   progress, logs, list-changes, resource-updated, info messages, and
//!   tool display for sampling. [`EventBroker`] fans out to one or more
//!   [`EventSink`]s (stderr, HTTP webhook, Unix socket, SSE server,
//!   command exec).
//! - [`sinks`] — concrete `EventSink` implementations.
//! - [`daemon`] — long-running IPC daemon that holds warm MCP
//!   connections so repeated invocations don't pay the init cost.
//! - [`token_store`] — encrypted-at-rest token store for OAuth flows
//!   and bearer-token transports.

pub mod daemon;
mod events;
mod host;
mod sinks;
mod state;
mod token_store;

pub use events::{EventBroker, EventSink, MemoryEventSink, RuntimeEvent, StderrEventSink};
pub use host::{RuntimeHost, RuntimeServices};
pub use sinks::{CommandExecSink, HttpWebhookSink, SseServerSink, UnixSocketSink};
pub use state::{
    AuthSessionRecord, AuthSessionState, DiscoveryInventoryView, JobRecord, JobStatus,
    NegotiatedCapabilityView, StateStore,
};
pub use token_store::{StoredToken, TokenStore};
