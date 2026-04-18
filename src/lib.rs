//! `mcp2cli` — turn any MCP server into a native command-line application.
//!
//! mcp2cli is a client for the [Model Context Protocol]. It speaks MCP
//! over stdio or streamable HTTP, introspects any server's tools,
//! resources, resource templates, and prompts, and renders them as a
//! typed `clap` CLI — required fields become required flags, enums
//! become value-parsed options, JSON Schema types drive argument
//! parsing.
//!
//! # Crate layout
//!
//! - [`app`] — process-level bootstrap: parse `argv`, load config, wire
//!   up runtime services, resolve the [`dispatch`] target, run it.
//! - [`dispatch`] — routing: decide whether an invocation is a host
//!   command (`mcp2cli config ...`), a named-config app (invoked via
//!   symlink alias), an ad-hoc `--url`/`--stdio` connection, or an
//!   `mcp-<server>-<tool>` shim.
//! - [`cli`] — the host CLI (`mcp2cli config`, `link`, `use`,
//!   `daemon`, `man`) parsed with `clap`.
//! - [`apps`] — per-app CLI surfaces:
//!   - [`apps::bridge`] — the static `BridgeCli` (fallback when no
//!     discovery cache exists).
//!   - [`apps::dynamic`] — the dynamic CLI generator that materialises
//!     clap commands from a cached `CommandManifest`.
//!   - [`apps::manifest`] — the cache model: discovery items + profile
//!     overlay → flat command tree with typed flag specs.
//! - [`mcp`] — the protocol layer:
//!   - [`mcp::protocol`] — JSON-RPC framing, `initialize` state
//!     machine, `McpOperation` → request mapping.
//!   - [`mcp::client`] — transport abstraction (stdio, streamable HTTP,
//!     daemon IPC, VSOCK/Unix shim) behind the `McpClient` trait.
//!   - [`mcp::handler`] — server→client message dispatch
//!     (notifications/progress, elicitation, sampling, roots, logging).
//!   - [`mcp::model`] — shared operation and result types.
//!   - [`mcp::vsock_shim`] — AF_VSOCK / AF_UNIX dial-and-pipe for the
//!     `mcp-<server>-<tool>` shim.
//! - [`runtime`] — state, events, daemon, token store. The [`runtime::host`]
//!   submodule owns command execution for each dispatch target.
//! - [`config`] — YAML config schema, figment loading, profile overlays.
//! - [`output`] — structured `CommandOutput` + JSON/NDJSON/human
//!   formatters.
//! - [`observability`] — tracing subscriber wiring.
//! - [`telemetry`] — opt-in local telemetry sink.
//! - [`man`] — generated `mcp2cli(1)` man page source.
//!
//! # Request lifecycle (tldr)
//!
//! 1. `main` calls [`app::build`] which captures `argv`, loads active
//!    config, and returns an [`app::AppState`].
//! 2. [`app::run`] inspects the [`dispatch::DispatchTarget`] and hands
//!    control to the [`runtime::RuntimeHost`].
//! 3. For a config-bound command, the host constructs an
//!    [`apps::AppContext`], chooses the dynamic or static CLI, parses
//!    the command, and translates it into an [`mcp::model::McpOperation`].
//! 4. The operation flows through [`mcp::protocol::ProtocolEngine`]
//!    (which attaches a progress token and marshals `tools/call`, etc.)
//!    to an [`mcp::client::McpClient`] implementation.
//! 5. Responses come back to the operation; server-initiated
//!    notifications and requests are delivered to the active
//!    [`mcp::handler::ServerMessageHandler`] (which surfaces them as
//!    [`runtime::RuntimeEvent`]s).
//!
//! [Model Context Protocol]: https://modelcontextprotocol.io

pub mod app;
pub mod apps;
pub mod cli;
pub mod config;
pub mod dispatch;
pub mod man;
pub mod mcp;
pub mod observability;
pub mod output;
pub mod runtime;
pub mod telemetry;
