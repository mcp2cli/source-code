//! Binary entry point.
//!
//! Two-phase startup keeps `main` small and testable:
//!
//! 1. [`mcp2cli::app::build`] captures `argv` + the optional
//!    `MCP2CLI_CONFIG` override, loads the active config, and wires
//!    runtime services (state store, event broker, MCP client, telemetry).
//! 2. [`mcp2cli::app::run`] drives the dispatcher and returns the
//!    command's exit status via `anyhow::Result`.
//!
//! Everything interesting lives in the library crate; see the
//! top-level [`mcp2cli`] docs for the request lifecycle.

use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argv = std::env::args_os().collect::<Vec<_>>();
    let config_path = std::env::var_os("MCP2CLI_CONFIG").map(PathBuf::from);
    let state = mcp2cli::app::build(argv, config_path).await?;
    mcp2cli::app::run(state).await
}
