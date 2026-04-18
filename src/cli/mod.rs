use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use serde_json::json;

use crate::{
    config::{ActiveConfigSelection, NamedConfigSummary, ResolvedAppConfig},
    mcp::model::TransportKind,
    output::{CommandOutput, OutputFormat},
};

#[derive(Debug, Parser)]
#[command(
    about = "Generic bridge runtime for MCP servers",
    disable_help_subcommand = true,
    arg_required_else_help = true,
    subcommand_required = true
)]
pub struct HostCli {
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true, value_enum)]
    pub output: Option<OutputFormat>,
    #[command(subcommand)]
    pub command: HostCommand,
}

impl HostCli {
    pub fn effective_output(&self, default_format: OutputFormat) -> OutputFormat {
        if self.json {
            OutputFormat::Json
        } else {
            self.output.unwrap_or(default_format)
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum HostCommand {
    Config(ConfigArgs),
    Link(LinkArgs),
    Use(UseArgs),
    /// Manage the background daemon that keeps MCP connections warm
    Daemon(DaemonArgs),
    /// Install man pages for mcp2cli and its aliases
    Man(ManArgs),
}

#[derive(Debug, Args)]
pub struct ManArgs {
    #[command(subcommand)]
    pub command: ManCommand,
}

#[derive(Debug, Subcommand)]
pub enum ManCommand {
    /// Install (or refresh) the mcp2cli(1) man page for the host binary
    Install(ManInstallArgs),
}

#[derive(Debug, Args)]
pub struct ManInstallArgs {
    /// Target man1 directory (default: ~/.local/share/man/man1)
    #[arg(long)]
    pub dir: Option<std::path::PathBuf>,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    List,
    Show(ConfigNameArgs),
    Init(ConfigInitArgs),
}

#[derive(Debug, Args)]
pub struct ConfigNameArgs {
    #[arg(long)]
    pub name: String,
}

#[derive(Debug, Args)]
pub struct ConfigInitArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long, default_value = "bridge")]
    pub app: String,
    #[arg(long, value_enum, default_value_t = ConfigTransportArg::StreamableHttp)]
    pub transport: ConfigTransportArg,
    #[arg(long)]
    pub endpoint: Option<String>,
    #[arg(long = "stdio-command")]
    pub stdio_command: Option<String>,
    #[arg(long = "stdio-arg")]
    pub stdio_args: Vec<String>,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ConfigTransportArg {
    Stdio,
    StreamableHttp,
}

impl From<ConfigTransportArg> for TransportKind {
    fn from(value: ConfigTransportArg) -> Self {
        match value {
            ConfigTransportArg::Stdio => TransportKind::Stdio,
            ConfigTransportArg::StreamableHttp => TransportKind::StreamableHttp,
        }
    }
}

#[derive(Debug, Args)]
pub struct LinkArgs {
    #[command(subcommand)]
    pub command: LinkCommand,
}

#[derive(Debug, Subcommand)]
pub enum LinkCommand {
    Create(LinkCreateArgs),
}

#[derive(Debug, Args)]
pub struct LinkCreateArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub dir: Option<std::path::PathBuf>,
    #[arg(long)]
    pub force: bool,
    /// Directory where the man page will be installed (default: ~/.local/share/man/man1).
    /// Pass an explicit path to install into a custom prefix, e.g. /usr/local/share/man/man1.
    #[arg(long = "man-dir")]
    pub man_dir: Option<std::path::PathBuf>,
    /// Skip man page generation and installation.
    #[arg(long = "no-man", default_value_t = false)]
    pub no_man: bool,
}

#[derive(Debug, Args)]
pub struct UseArgs {
    #[arg(long, conflicts_with_all = ["clear", "name"])]
    pub show: bool,
    #[arg(long, conflicts_with_all = ["show", "name"])]
    pub clear: bool,
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct DaemonArgs {
    #[command(subcommand)]
    pub command: DaemonCommand,
}

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Start the daemon for a named config (backgrounds automatically)
    Start {
        /// Config name to keep warm
        name: String,
    },
    /// Stop a running daemon
    Stop {
        /// Config name to stop
        name: String,
    },
    /// Show status of running daemons
    Status {
        /// Config name to check (omit for all)
        name: Option<String>,
    },
}

pub fn parse_host_cli(
    argv: &[std::ffi::OsString],
    invoked_as: &str,
) -> std::result::Result<HostCli, clap::Error> {
    let mut command = HostCli::command();
    command = command
        .name("mcp2cli")
        .bin_name(invoked_as)
        .version(env!("CARGO_PKG_VERSION"))
        .after_help(
            "Examples:\n  mcp2cli config init --name work --app bridge --endpoint https://demo.invalid/mcp\n  mcp2cli config init --name local --app bridge --transport stdio --stdio-command npx --stdio-arg @modelcontextprotocol/server-everything\n  mcp2cli use work\n  mcp2cli use --show\n  mcp2cli use --clear\n  mcp2cli tool call echo --arg message=hello\n  mcp2cli jobs list\n  mcp2cli work tool call echo --arg message=hello",
        );
    let matches = command.try_get_matches_from_mut(argv.to_vec())?;
    HostCli::from_arg_matches(&matches)
}

pub fn configs_list_output(configs: &[NamedConfigSummary]) -> CommandOutput {
    let lines = if configs.is_empty() {
        vec!["no named configs found".to_owned()]
    } else {
        configs
            .iter()
            .map(|config| format!("{}  {}", config.name, config.path.display()))
            .collect()
    };
    CommandOutput::new(
        "mcp2cli",
        "config list",
        format!("listed {} configs", configs.len()),
        lines,
        json!({ "items": configs }),
    )
}

pub fn config_show_output(config: &ResolvedAppConfig) -> CommandOutput {
    let server_lines = match config.config.server.transport {
        TransportKind::StreamableHttp => vec![format!(
            "endpoint: {}",
            config
                .config
                .server
                .endpoint
                .clone()
                .unwrap_or_else(|| "(none)".to_owned())
        )],
        TransportKind::Stdio => vec![
            format!(
                "stdio command: {}",
                config
                    .config
                    .server
                    .stdio
                    .command
                    .clone()
                    .unwrap_or_else(|| "(none)".to_owned())
            ),
            format!(
                "stdio args: {}",
                if config.config.server.stdio.args.is_empty() {
                    "(none)".to_owned()
                } else {
                    config.config.server.stdio.args.join(" ")
                }
            ),
        ],
    };
    CommandOutput::new(
        "mcp2cli",
        "config show",
        format!("showing config '{}'", config.name),
        [
            vec![
                format!("name: {}", config.name),
                format!("app profile: {}", config.config.app.profile),
                format!("transport: {}", config.config.server.transport.as_str()),
                format!("file: {}", config.path.display()),
            ],
            server_lines,
        ]
        .concat(),
        json!({ "config": config }),
    )
}

pub fn link_create_output(
    name: &str,
    link_path: &std::path::Path,
    target_path: &std::path::Path,
    man_page_result: Option<Result<std::path::PathBuf, String>>,
) -> CommandOutput {
    let mut lines = vec![
        format!("name: {}", name),
        format!("link: {}", link_path.display()),
        format!("target: {}", target_path.display()),
    ];

    let (man_page_path, man_page_warning) = match man_page_result {
        Some(Ok(ref path)) => {
            lines.push(format!("man page: {}", path.display()));
            (Some(path.to_string_lossy().to_string()), None)
        }
        Some(Err(ref msg)) => {
            lines.push(format!("man page: (skipped — {})", msg));
            (None, Some(msg.clone()))
        }
        None => {
            lines.push("man page: (skipped — --no-man)".to_owned());
            (None, None)
        }
    };

    CommandOutput::new(
        "mcp2cli",
        "link create",
        format!("created link '{}'", name),
        lines,
        json!({
            "name": name,
            "link_path": link_path,
            "target_path": target_path,
            "man_page_path": man_page_path,
            "man_page_warning": man_page_warning,
        }),
    )
}

pub fn use_config_output(
    selection: &ActiveConfigSelection,
    config: &ResolvedAppConfig,
) -> CommandOutput {
    CommandOutput::new(
        "mcp2cli",
        "use",
        format!("active config set to '{}'", selection.config_name),
        vec![
            format!("name: {}", selection.config_name),
            format!("app profile: {}", config.config.app.profile),
            format!("transport: {}", config.config.server.transport.as_str()),
            format!("file: {}", config.path.display()),
            "next: run mcp2cli <bridge-command> directly".to_owned(),
        ],
        json!({
            "active": selection,
            "config": config,
        }),
    )
}

pub fn use_status_output(
    selection: Option<&ActiveConfigSelection>,
    config: Option<&ResolvedAppConfig>,
    load_error: Option<&str>,
) -> CommandOutput {
    match selection {
        Some(selection) => {
            let mut lines = vec![format!("name: {}", selection.config_name)];
            let mut data = json!({
                "active": selection,
            });

            if let Some(config) = config {
                lines.push(format!("app profile: {}", config.config.app.profile));
                lines.push(format!(
                    "transport: {}",
                    config.config.server.transport.as_str()
                ));
                lines.push(format!("file: {}", config.path.display()));
                data["config"] = json!(config);
            }
            if let Some(load_error) = load_error {
                lines.push(format!("status: stale ({})", load_error));
                data["load_error"] = json!(load_error);
            }

            CommandOutput::new(
                "mcp2cli",
                "use",
                format!("active config is '{}'", selection.config_name),
                lines,
                data,
            )
        }
        None => CommandOutput::new(
            "mcp2cli",
            "use",
            "no active config selected".to_owned(),
            vec!["next: run mcp2cli use <name>".to_owned()],
            json!({}),
        ),
    }
}

pub fn use_clear_output(selection: Option<&ActiveConfigSelection>) -> CommandOutput {
    let lines = selection
        .map(|selection| vec![format!("cleared: {}", selection.config_name)])
        .unwrap_or_else(|| vec!["active config was already clear".to_owned()]);
    CommandOutput::new(
        "mcp2cli",
        "use clear",
        "active config cleared".to_owned(),
        lines,
        json!({ "cleared": selection }),
    )
}

pub fn man_install_output(page_path: &std::path::Path) -> CommandOutput {
    CommandOutput::new(
        "mcp2cli",
        "man install",
        "installed mcp2cli man page".to_owned(),
        vec![format!("man page: {}", page_path.display())],
        json!({ "path": page_path }),
    )
}
