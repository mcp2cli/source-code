//! Host CLI — the `mcp2cli ...` subcommand tree.
//!
//! These are administrative commands that don't target a specific
//! MCP server. They manage the mcp2cli installation itself: configs,
//! symlink aliases, the connection-reuse daemon, and the man page.
//!
//! The tree parses through `clap` and is rooted at [`HostCli`].
//! [`parse_host_cli`] is the single entry point used by
//! [`crate::runtime::RuntimeHost`] — it returns a typed
//! [`HostCommand`] that the host then executes.
//!
//! Commands at a glance:
//!
//! - `mcp2cli config create | show | list | delete` — YAML config
//!   management. Configs live under
//!   `$XDG_CONFIG_HOME/mcp2cli/configs/<name>.yaml`.
//! - `mcp2cli use <name> | clear` — switch the active config. The
//!   active selection is recorded so subsequent `mcp2cli ls`/`invoke`
//!   calls don't need to specify a config each time.
//! - `mcp2cli link create <name>` — install a symlink named `<name>`
//!   alongside the mcp2cli binary so `<name> ls`, `<name> invoke` etc.
//!   dispatch to that config automatically.
//! - `mcp2cli daemon start | stop | status` — control the long-lived
//!   daemon that holds warm MCP connections between invocations.
//! - `mcp2cli man install | show` — emit or install the `mcp2cli(1)`
//!   man page. See [`crate::man`] for the generated nroff source.
//! - `mcp2cli package init | snapshot` — scaffold a config as a CLI
//!   published under its own name. See [`crate::package`].

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
    /// Package a config as a CLI published under its own name
    Package(PackageArgs),
}

#[derive(Debug, Args)]
pub struct PackageArgs {
    #[command(subcommand)]
    pub command: PackageCommand,
}

#[derive(Debug, Subcommand)]
pub enum PackageCommand {
    /// Scaffold a publishable package: config, inventory snapshot, launcher
    Init(PackageInitArgs),
    /// Refresh a package's inventory snapshot from the live server
    Snapshot(PackageSnapshotArgs),
}

#[derive(Debug, Args)]
pub struct PackageSourceArgs {
    /// Named config to package (default: the CLI name)
    #[arg(long, conflicts_with = "config")]
    pub from: Option<String>,
    /// Config file to package, instead of a named config
    #[arg(long)]
    pub config: Option<std::path::PathBuf>,
}

#[derive(Debug, Args)]
pub struct PackageInitArgs {
    /// Command name of the published CLI
    #[arg(long)]
    pub name: String,
    #[command(flatten)]
    pub source: PackageSourceArgs,
    /// Directory to write the package to (default: ./<name>-cli)
    #[arg(long)]
    pub out: Option<std::path::PathBuf>,
    #[arg(long, value_enum, default_value_t = PackageTargetArg::Npm)]
    pub target: PackageTargetArg,
    /// Published package name, when it differs from the command name
    #[arg(long = "package-name")]
    pub package_name: Option<String>,
    /// Version of the published package
    #[arg(long = "package-version", default_value = "0.1.0")]
    pub package_version: String,
    /// One-line description shown at the top of --help
    #[arg(long)]
    pub about: Option<String>,
    /// Built-in command to expose (repeatable): auth, jobs, doctor, inspect, ls,
    /// ping, log, complete, subscribe, unsubscribe, tool, resource, prompt.
    /// Default: auth for HTTP servers, none for stdio servers.
    #[arg(long = "builtin", value_name = "COMMAND")]
    pub builtin: Vec<String>,
    /// Expose no built-in commands at all
    #[arg(long = "no-builtins", conflicts_with = "builtin")]
    pub no_builtins: bool,
    /// Do not bundle an inventory snapshot; discover on first run instead
    #[arg(long = "no-snapshot")]
    pub no_snapshot: bool,
    /// Overwrite files in a non-empty output directory
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct PackageSnapshotArgs {
    #[command(flatten)]
    pub source: PackageSourceArgs,
    /// Snapshot file to write
    #[arg(long, default_value = crate::package::SNAPSHOT_FILE_NAME)]
    pub out: std::path::PathBuf,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PackageTargetArg {
    /// npm package; `npx <name>` works, the runtime comes from the mcp2cli package
    Npm,
    /// POSIX launcher script for installs that already have mcp2cli on PATH
    Shell,
}

impl From<PackageTargetArg> for crate::package::PackageTarget {
    fn from(value: PackageTargetArg) -> Self {
        match value {
            PackageTargetArg::Npm => Self::Npm,
            PackageTargetArg::Shell => Self::Shell,
        }
    }
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
    /// MCP protocol version: auto (default), 2026-07-28, or 2025-11-25.
    #[arg(long = "protocol-version")]
    pub protocol_version: Option<String>,
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
            "Examples:\n  mcp2cli config init --name email --app bridge --endpoint https://mcp.example.com/email\n  mcp2cli config init --name local --app bridge --transport stdio --stdio-command npx --stdio-arg @modelcontextprotocol/server-everything\n  mcp2cli use email\n  mcp2cli use --show\n  mcp2cli use --clear\n  mcp2cli tool call send --arg to=user@example.com\n  mcp2cli jobs list\n  mcp2cli email tool call send --arg to=user@example.com",
        );
    let matches = command.try_get_matches_from_mut(argv.to_vec())?;
    HostCli::from_arg_matches(&matches)
}

pub fn package_init_output(
    name: &str,
    target: crate::package::PackageTarget,
    report: &crate::package::PackageInitReport,
    snapshot_items: Option<usize>,
    snapshot_warning: Option<&str>,
) -> CommandOutput {
    let mut lines = vec![
        format!("package: {}", report.out_dir.display()),
        format!("target: {}", target.as_str()),
        format!(
            "built-in commands: {}",
            if report.builtin_commands.is_empty() {
                "(none)".to_owned()
            } else {
                report.builtin_commands.join(", ")
            }
        ),
    ];
    match (snapshot_items, snapshot_warning) {
        (Some(items), _) => lines.push(format!("snapshot: {} capabilities", items)),
        (None, Some(warning)) => lines.push(format!("snapshot: (skipped — {})", warning)),
        (None, None) => lines.push("snapshot: (skipped — --no-snapshot)".to_owned()),
    }
    for warning in &report.warnings {
        lines.push(format!("warning: {}", warning));
    }
    lines.push("files:".to_owned());
    for file in &report.files {
        let shown = file.strip_prefix(&report.out_dir).unwrap_or(file);
        lines.push(format!("  {}", shown.display()));
    }
    lines.push(String::new());
    lines.push("next:".to_owned());
    match target {
        crate::package::PackageTarget::Npm => {
            lines.push(format!("  cd {} && npm install", report.out_dir.display()));
            lines.push(format!("  node bin/{}.js --help", name));
            lines.push("  npm publish --access public".to_owned());
        }
        crate::package::PackageTarget::Shell => {
            lines.push(format!("  {} --help", report.launcher.display()));
        }
    }

    CommandOutput::new(
        "mcp2cli",
        "package init",
        format!("created package '{}'", name),
        lines,
        json!({
            "name": name,
            "target": target.as_str(),
            "out_dir": report.out_dir,
            "files": report.files,
            "launcher": report.launcher,
            "builtin_commands": report.builtin_commands,
            "warnings": report.warnings,
            "snapshot_items": snapshot_items,
            "snapshot_warning": snapshot_warning,
        }),
    )
}

pub fn package_snapshot_output(
    path: &std::path::Path,
    snapshot: &crate::runtime::InventorySnapshot,
) -> CommandOutput {
    CommandOutput::new(
        "mcp2cli",
        "package snapshot",
        format!("wrote inventory snapshot to {}", path.display()),
        vec![
            format!("snapshot: {}", path.display()),
            format!("tools: {}", snapshot.tools.len()),
            format!("resources: {}", snapshot.resources.len()),
            format!("resource templates: {}", snapshot.resource_templates.len()),
            format!("prompts: {}", snapshot.prompts.len()),
        ],
        json!({
            "path": path,
            "generated_at": snapshot.generated_at,
            "tools": snapshot.tools.len(),
            "resources": snapshot.resources.len(),
            "resource_templates": snapshot.resource_templates.len(),
            "prompts": snapshot.prompts.len(),
        }),
    )
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
