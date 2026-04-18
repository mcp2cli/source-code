use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argv = std::env::args_os().collect::<Vec<_>>();
    let config_path = std::env::var_os("MCP2CLI_CONFIG").map(PathBuf::from);
    let state = mcp2cli::app::build(argv, config_path).await?;
    mcp2cli::app::run(state).await
}
