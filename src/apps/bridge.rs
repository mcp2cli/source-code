//! The static `BridgeCli` — a fixed `clap` tree that works with any
//! MCP server regardless of whether a discovery cache exists.
//!
//! Compared to [`crate::apps::dynamic`], which generates a bespoke
//! CLI from cached capabilities, the bridge surface is
//! protocol-shaped: commands name MCP primitives (`invoke`, `get`,
//! `prompt`, `subscribe`, `complete`, `log`) rather than server
//! tools. The bridge stays useful in four situations:
//!
//! 1. **First run** — there's no discovery cache yet; the dynamic CLI
//!    can't build a tree. `ls` populates the cache via `tools/list` /
//!    `resources/list` / `prompts/list`.
//! 2. **Protocol-shaped use** — `mcp2cli invoke <tool> --arg k=v` is
//!    a deterministic back-door that bypasses the generated flag
//!    schema. Handy for tools whose JSON Schema is odd enough that
//!    the dynamic CLI would get confused.
//! 3. **Lifecycle and introspection commands** — `doctor`, `inspect`,
//!    `ping`, `auth login/logout/status`, `jobs show/wait/cancel/watch`,
//!    and `log level` never belong in the dynamic surface; they're
//!    client operations, not server tools.
//! 4. **Scripting** — CI and shell pipelines want stable command
//!    shapes. The bridge's `invoke`/`get`/`prompt` commands never
//!    change based on which server is on the other end.
//!
//! The bridge composes with [`crate::apps::AppContext::perform`] for
//! every MCP call, so timeouts, events, and telemetry behave
//! identically to the dynamic path.

use std::{ffi::OsString, path::Path};

use anyhow::{Result, anyhow};
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, error::ErrorKind};
use serde_json::{Map, Number, Value, json};

use crate::{
    apps::{AppContext, default_job_detail_lines, default_job_overview_lines},
    mcp::model::{DiscoveryCategory, McpOperation, McpOperationResult, TaskState, TransportKind},
    output::{CommandOutput, ExecutionReport, OutputFormat},
    runtime::{
        AuthSessionRecord, AuthSessionState, DiscoveryInventoryView, JobStatus,
        NegotiatedCapabilityView, RuntimeEvent, StoredToken,
    },
};

#[derive(Debug, Parser)]
#[command(
    about = "MCP bridge CLI — domain-shaped commands for MCP servers",
    disable_help_subcommand = true,
    arg_required_else_help = true,
    subcommand_required = true
)]
struct BridgeCli {
    #[arg(long, global = true)]
    _config: Option<std::path::PathBuf>,
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true, value_enum)]
    output: Option<OutputFormat>,
    /// Fail instead of prompting for interactive input (CI mode)
    #[arg(long, global = true)]
    non_interactive: bool,
    /// Provide elicitation answers as a JSON object (CI mode)
    #[arg(long, global = true, value_name = "JSON")]
    input_json: Option<String>,
    /// Operation timeout in seconds (0 = no timeout, overrides config)
    #[arg(long, global = true, value_name = "SECONDS")]
    timeout: Option<u64>,
    #[command(subcommand)]
    command: BridgeCommand,
}

impl BridgeCli {
    fn effective_output(&self, default_format: OutputFormat) -> OutputFormat {
        if self.json {
            OutputFormat::Json
        } else {
            self.output.unwrap_or(default_format)
        }
    }
}

// ---------------------------------------------------------------------------
// Primary domain-shaped command surface
// ---------------------------------------------------------------------------

#[derive(Debug, Subcommand)]
enum BridgeCommand {
    /// Manage and call server tools (actions / verbs)
    Tool(ToolArgs),
    /// Manage and read server resources (nouns / collections)
    Resource(ResourceArgs),
    /// List and run server prompts (guided flows / recipes)
    Prompt(PromptCmdArgs),
    /// Authentication management
    Auth(AuthArgs),
    /// Background job management
    Jobs(JobsArgs),
    /// Runtime health diagnostics
    Doctor,
    /// Inspect server capabilities, metadata, and mapped commands
    Inspect,

    // --- backward-compatible aliases (hidden from primary help) -----------
    /// Discover server capabilities [alias: tool list / resource list / prompt list]
    #[command(hide = true)]
    Discover(DiscoverArgs),
    /// Invoke a tool by capability name [alias: tool call]
    #[command(hide = true)]
    Invoke(InvokeArgs),
    /// Read a resource by URI [alias: resource read]
    #[command(hide = true)]
    Read(ReadArgs),
    /// List capabilities [alias: tool list / resource list / prompt list]
    #[command(hide = true)]
    List(LegacyListArgs),
}

// ---------------------------------------------------------------------------
// tool subcommand
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
struct ToolArgs {
    #[command(subcommand)]
    command: ToolCommand,
}

#[derive(Debug, Subcommand)]
enum ToolCommand {
    /// List available tools from the server
    List(ToolListArgs),
    /// Call a tool by name
    Call(ToolCallArgs),
}

#[derive(Debug, Args)]
struct ToolListArgs {
    /// Filter tools by name/description substring
    #[arg(long)]
    filter: Option<String>,
    #[arg(long)]
    limit: Option<u32>,
    #[arg(long)]
    cursor: Option<String>,
    #[arg(long)]
    all: bool,
}

#[derive(Debug, Args)]
struct ToolCallArgs {
    /// Tool name to call
    name: String,
    #[arg(long = "arg", value_name = "KEY=VALUE")]
    args: Vec<String>,
    #[arg(long = "arg-json", value_name = "KEY=JSON")]
    json_args: Vec<String>,
    #[arg(long = "args-json", value_name = "JSON_OBJECT")]
    args_json: Option<String>,
    #[arg(long = "args-file", value_name = "PATH")]
    args_file: Option<std::path::PathBuf>,
    /// Run as background job
    #[arg(long)]
    background: bool,
}

// ---------------------------------------------------------------------------
// resource subcommand
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
struct ResourceArgs {
    #[command(subcommand)]
    command: ResourceCommand,
}

#[derive(Debug, Subcommand)]
enum ResourceCommand {
    /// List available resources from the server
    List(ResourceListArgs),
    /// Read a resource by URI
    Read(ResourceReadArgs),
}

#[derive(Debug, Args)]
struct ResourceListArgs {
    /// Filter resources by name/URI substring
    #[arg(long)]
    filter: Option<String>,
    #[arg(long)]
    limit: Option<u32>,
    #[arg(long)]
    cursor: Option<String>,
    #[arg(long)]
    all: bool,
}

#[derive(Debug, Args)]
struct ResourceReadArgs {
    /// Resource URI to read
    uri: String,
}

// ---------------------------------------------------------------------------
// prompt subcommand
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
struct PromptCmdArgs {
    #[command(subcommand)]
    command: PromptCommand,
}

#[derive(Debug, Subcommand)]
enum PromptCommand {
    /// List available prompts from the server
    List(PromptListArgs),
    /// Run a prompt by name
    Run(PromptRunArgs),
}

#[derive(Debug, Args)]
struct PromptListArgs {
    /// Filter prompts by name/description substring
    #[arg(long)]
    filter: Option<String>,
    #[arg(long)]
    limit: Option<u32>,
    #[arg(long)]
    cursor: Option<String>,
    #[arg(long)]
    all: bool,
}

#[derive(Debug, Args)]
struct PromptRunArgs {
    /// Prompt name to run
    name: String,
    #[arg(long = "arg", value_name = "KEY=VALUE")]
    args: Vec<String>,
    #[arg(long = "arg-json", value_name = "KEY=JSON")]
    json_args: Vec<String>,
    #[arg(long = "args-json", value_name = "JSON_OBJECT")]
    args_json: Option<String>,
    #[arg(long = "args-file", value_name = "PATH")]
    args_file: Option<std::path::PathBuf>,
}

// ---------------------------------------------------------------------------
// auth subcommand
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommand,
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    /// Authenticate with the server
    Login,
    /// Clear stored credentials
    Logout,
    /// Show current authentication state
    Status,
}

// ---------------------------------------------------------------------------
// jobs subcommand
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
struct JobsArgs {
    #[command(subcommand)]
    command: JobsCommand,
}

#[derive(Debug, Subcommand)]
enum JobsCommand {
    /// List background jobs
    List,
    /// Show job details
    Show(JobSelectorArgs),
    /// Wait for job completion
    Wait(JobSelectorArgs),
    /// Cancel a running job
    Cancel(JobSelectorArgs),
    /// Watch job progress
    Watch(JobSelectorArgs),
}

#[derive(Debug, Args, Clone, PartialEq, Eq)]
struct JobSelectorArgs {
    job_id: Option<String>,
    #[arg(long, conflicts_with = "job_id")]
    latest: bool,
    #[arg(long)]
    command: Option<String>,
}

// ---------------------------------------------------------------------------
// backward-compatible legacy args (hidden commands)
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
struct DiscoverArgs {
    #[command(subcommand)]
    command: DiscoverCommand,
}

#[derive(Debug, Subcommand)]
enum DiscoverCommand {
    Capabilities,
    Resources,
    Prompts,
}

#[derive(Debug, Args)]
struct InvokeArgs {
    #[arg(long)]
    capability: String,
    #[arg(long = "arg", value_name = "KEY=VALUE")]
    args: Vec<String>,
    #[arg(long = "arg-json", value_name = "KEY=JSON")]
    json_args: Vec<String>,
    #[arg(long = "args-json", value_name = "JSON_OBJECT")]
    args_json: Option<String>,
    #[arg(long = "args-file", value_name = "PATH")]
    args_file: Option<std::path::PathBuf>,
    #[arg(long)]
    background: bool,
}

#[derive(Debug, Args)]
struct ReadArgs {
    #[arg(long)]
    uri: String,
}

#[derive(Debug, Args)]
struct LegacyListArgs {
    #[arg(long)]
    capability: String,
    #[arg(long)]
    limit: Option<u32>,
    #[arg(long)]
    cursor: Option<String>,
    #[arg(long)]
    page_size: Option<u32>,
    #[arg(long)]
    all: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BridgeDomainCommand {
    Discover {
        category: DiscoveryCategory,
    },
    Invoke {
        capability: String,
        arguments: Value,
        background: bool,
    },
    Read {
        uri: String,
    },
    List {
        capability: String,
        limit: Option<u32>,
        cursor: Option<String>,
        page_size: Option<u32>,
        all: bool,
    },
    Prompt {
        name: String,
        arguments: Value,
    },
    AuthLogin,
    AuthLogout,
    AuthStatus,
    JobsList,
    JobsShow {
        selector: JobSelector,
    },
    JobsWait {
        selector: JobSelector,
    },
    JobsCancel {
        selector: JobSelector,
    },
    JobsWatch {
        selector: JobSelector,
    },
    Doctor,
    Inspect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct JobSelector {
    job_id: Option<String>,
    latest: bool,
    command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedListSelector {
    category: DiscoveryCategory,
    filter: Option<String>,
}

#[derive(Debug, Clone)]
struct DiscoveryItemsSnapshot {
    items: Vec<Value>,
    cached: bool,
    live_error: Option<String>,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CapabilityGroup {
    Tools,
    Resources,
    Prompts,
}

// ---------------------------------------------------------------------------
// Public bridge entry points — called directly by RuntimeHost
// ---------------------------------------------------------------------------

/// Returns true if the given token is a valid bridge root command.
/// With dynamic CLI, any token from the manifest could be valid too,
/// but this only checks the static/runtime commands for `mcp2cli` dispatch.
pub fn supports_root_command(token: &str) -> bool {
    matches!(
        token,
        "tool" | "resource" | "prompt" | "auth" | "jobs" | "doctor" | "inspect" | "ls"
        // legacy aliases
        | "discover" | "invoke" | "read" | "list"
    )
}

fn bridge_command(invoked_as: &str) -> clap::Command {
    let mut command = BridgeCli::command();
    command = command
        .name("mcp2cli")
        .bin_name(invoked_as)
        .version(env!("CARGO_PKG_VERSION"))
        .after_help(if invoked_as == crate::dispatch::HOST_BINARY_NAME {
            "Examples:\n  mcp2cli tool list\n  mcp2cli tool call echo --arg message=hello\n  mcp2cli resource list\n  mcp2cli resource read demo://resource/readme.md\n  mcp2cli prompt list\n  mcp2cli prompt run simple-prompt\n  mcp2cli tool call tasks.run --args-file ./payload.json --background\n  mcp2cli auth status\n  mcp2cli jobs list\n  mcp2cli inspect\n\nHost commands:\n  mcp2cli config list\n  mcp2cli use work"
        } else {
            "Examples:\n  work tool list\n  work tool call echo --arg message=hello\n  work resource list\n  work resource read demo://resource/readme.md\n  work prompt list\n  work prompt run simple-prompt\n  work tool call tasks.run --args-file ./payload.json --background\n  work auth status\n  work jobs list\n  work inspect"
        });
    command
}

fn parse_bridge_cli(
    argv: &[OsString],
    invoked_as: &str,
) -> std::result::Result<BridgeCli, clap::Error> {
    let mut command = bridge_command(invoked_as);
    let matches = command.try_get_matches_from_mut(argv.to_vec())?;
    BridgeCli::from_arg_matches(&matches)
}

fn version_report(output_format: OutputFormat, context: &AppContext) -> ExecutionReport {
    ExecutionReport {
        output_format,
        output: CommandOutput::new(
            &context.config_name,
            "version",
            format!("{} {}", context.invoked_as, env!("CARGO_PKG_VERSION")),
            vec![
                format!("{} {}", context.invoked_as, env!("CARGO_PKG_VERSION")),
                format!("config: {}", context.config_name),
                format!("server: {}", context.config.server.display_name),
                format!("transport: {}", context.config.server.transport.as_str()),
            ],
            json!({
                "runtime_version": env!("CARGO_PKG_VERSION"),
                "invoked_as": context.invoked_as,
                "config_name": context.config_name,
                "server": context.config.server,
            }),
        ),
    }
}

fn help_report(
    output_format: OutputFormat,
    context: &AppContext,
    error: clap::Error,
) -> ExecutionReport {
    let help_text = error.to_string();
    let lines = help_text.lines().map(ToOwned::to_owned).collect::<Vec<_>>();

    ExecutionReport {
        output_format,
        output: CommandOutput::new(
            &context.config_name,
            "help",
            format!("showing help for {}", context.invoked_as),
            lines,
            json!({
                "invoked_as": context.invoked_as,
                "config_name": context.config_name,
                "help": help_text,
            }),
        ),
    }
}

async fn execute_domain_command(
    command: BridgeDomainCommand,
    output_format: OutputFormat,
    context: AppContext,
) -> Result<ExecutionReport> {
    enforce_cached_capability_support(&context, &command).await?;
    let output = match command {
        BridgeDomainCommand::Discover { category } => execute_discover(&context, category).await?,
        BridgeDomainCommand::Invoke {
            capability,
            arguments,
            background,
        } => execute_invoke(&context, capability, arguments, background).await?,
        BridgeDomainCommand::Read { uri } => execute_read(&context, uri).await?,
        BridgeDomainCommand::List {
            capability,
            limit,
            cursor,
            page_size,
            all,
        } => {
            let effective_limit = if all { None } else { page_size.or(limit) };
            list_command_output(&context, &capability, effective_limit, cursor).await?
        }
        BridgeDomainCommand::Prompt { name, arguments } => {
            execute_prompt(&context, name, arguments).await?
        }
        BridgeDomainCommand::AuthLogin => auth_login(&context).await?,
        BridgeDomainCommand::AuthLogout => auth_logout(&context).await?,
        BridgeDomainCommand::AuthStatus => auth_status(&context).await?,
        BridgeDomainCommand::JobsList => execute_jobs_list(&context).await?,
        BridgeDomainCommand::JobsShow { selector } => execute_jobs_show(&context, selector).await?,
        BridgeDomainCommand::JobsWait { selector } => execute_jobs_wait(&context, selector).await?,
        BridgeDomainCommand::JobsCancel { selector } => {
            execute_jobs_cancel(&context, selector).await?
        }
        BridgeDomainCommand::JobsWatch { selector } => {
            execute_jobs_watch(&context, selector).await?
        }
        BridgeDomainCommand::Doctor => execute_doctor(&context).await?,
        BridgeDomainCommand::Inspect => execute_inspect(&context).await?,
    };

    Ok(ExecutionReport {
        output_format,
        output,
    })
}

/// Main entry point: parse CLI, resolve domain command, execute.
pub async fn execute(argv: &[OsString], context: AppContext) -> Result<ExecutionReport> {
    let output_format = crate::output::detect_output_format(argv, context.config.defaults.output);
    if requests_version(argv) {
        return Ok(version_report(output_format, &context));
    }

    // Try dynamic surface first if we have cached inventory
    if let Some(inventory) = context
        .services
        .state_store
        .discovery_inventory_view(&context.config_name)
        .await
    {
        let mut manifest = super::manifest::CommandManifest::from_inventory(&inventory);
        let server_display = context.config.server.display_name.as_str();

        // Apply profile overlay if configured
        if let Some(profile) = &context.config.profile {
            manifest.apply_profile(profile);
        }

        match super::dynamic::parse_dynamic(argv, &context.invoked_as, &manifest, server_display) {
            Ok(super::dynamic::DynamicParseResult {
                command: super::dynamic::DynamicCommand::LegacyBridge,
                ..
            }) => {
                // Fall through to static bridge parser below
            }
            Ok(super::dynamic::DynamicParseResult {
                command: cmd,
                output_format: dyn_format,
                timeout,
            }) => {
                let mut context = context.clone();
                if let Some(t) = timeout {
                    context.timeout_override = Some(t);
                }
                match super::dynamic::execute_dynamic(cmd, dyn_format, &context).await {
                    Ok(report) => return Ok(report),
                    Err(e) if e.to_string() == "__delegate_to_bridge__" => {
                        // Runtime commands (auth, jobs, doctor, inspect) —
                        // fall through to static bridge
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(_) => {
                // Dynamic parse failed — fall through to static bridge
            }
        }
    }

    // Static bridge parser (always available, backward-compatible)
    let cli = match parse_bridge_cli(argv, &context.invoked_as) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) =>
        {
            return Ok(help_report(output_format, &context, error));
        }
        Err(error) => return Err(error.into()),
    };
    let output_format = cli.effective_output(context.config.defaults.output);
    let mut context = context;
    if let Some(timeout) = cli.timeout {
        context.timeout_override = Some(timeout);
    }
    let domain_command = map_command(&cli.command)?;
    execute_domain_command(domain_command, output_format, context).await
}

/// Peek at the output format from argv without full parsing.
pub fn peek_output_format(argv: &[OsString], default_format: OutputFormat) -> OutputFormat {
    crate::output::detect_output_format(argv, default_format)
}

fn requests_version(argv: &[OsString]) -> bool {
    argv.iter()
        .skip(1)
        .filter_map(|value| value.to_str())
        .any(|token| matches!(token, "-V" | "--version"))
}

async fn command_output_for_action(
    command: &str,
    result: McpOperationResult,
    context: &AppContext,
) -> Result<CommandOutput> {
    match result {
        McpOperationResult::Action { message, data } => {
            let descriptor = context
                .services
                .state_store
                .discovery_inventory_view(&context.config_name)
                .await
                .and_then(|inventory| {
                    data.get("capability")
                        .and_then(Value::as_str)
                        .and_then(|capability| {
                            lookup_inventory_item(inventory.tools.as_deref(), capability, |item| {
                                item.get("id").and_then(Value::as_str)
                            })
                        })
                });
            let mut lines = cached_descriptor_lines(descriptor.as_ref());
            lines.push(
                data.get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("operation completed")
                    .to_owned(),
            );

            Ok(CommandOutput::new(
                &context.config_name,
                command,
                message,
                lines,
                cached_descriptor_data(data, descriptor.as_ref()),
            ))
        }
        McpOperationResult::TaskAccepted {
            message,
            remote_task_id,
            detail,
        } => {
            let job = context
                .services
                .state_store
                .create_job(
                    &context.config_name,
                    &context.config_name,
                    command,
                    detail
                        .get("summary")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    remote_task_id,
                )
                .await?;
            context.services.event_broker.emit(RuntimeEvent::JobUpdate {
                app_id: context.config_name.clone(),
                job_id: job.job_id.clone(),
                status: JobStatus::Queued.as_str().to_owned(),
                message: format!("created background job for {}", command),
            });
            Ok(CommandOutput::new(
                &context.config_name,
                command,
                message,
                vec![
                    format!("job: {}", job.job_id),
                    format!("status: {}", job.status.as_str()),
                ],
                json!({ "job": job, "detail": detail }),
            ))
        }
        other => Err(anyhow!(
            "unexpected MCP response for {}: {}",
            command,
            serde_json::to_string(&other)?
        )),
    }
}

fn map_command(command: &BridgeCommand) -> Result<BridgeDomainCommand> {
    match command {
        // -- primary domain-shaped commands --
        BridgeCommand::Tool(args) => match &args.command {
            ToolCommand::List(list_args) => Ok(BridgeDomainCommand::List {
                capability: match &list_args.filter {
                    Some(f) => format!("tools.{}", f),
                    None => "tools".to_owned(),
                },
                limit: list_args.limit,
                cursor: list_args.cursor.clone(),
                page_size: None,
                all: list_args.all,
            }),
            ToolCommand::Call(call_args) => Ok(BridgeDomainCommand::Invoke {
                capability: call_args.name.clone(),
                arguments: build_structured_arguments(
                    &call_args.args,
                    &call_args.json_args,
                    call_args.args_json.as_deref(),
                    call_args.args_file.as_deref(),
                )?,
                background: call_args.background,
            }),
        },
        BridgeCommand::Resource(args) => match &args.command {
            ResourceCommand::List(list_args) => Ok(BridgeDomainCommand::List {
                capability: match &list_args.filter {
                    Some(f) => format!("resources.{}", f),
                    None => "resources".to_owned(),
                },
                limit: list_args.limit,
                cursor: list_args.cursor.clone(),
                page_size: None,
                all: list_args.all,
            }),
            ResourceCommand::Read(read_args) => Ok(BridgeDomainCommand::Read {
                uri: read_args.uri.clone(),
            }),
        },
        BridgeCommand::Prompt(args) => match &args.command {
            PromptCommand::List(list_args) => Ok(BridgeDomainCommand::List {
                capability: match &list_args.filter {
                    Some(f) => format!("prompts.{}", f),
                    None => "prompts".to_owned(),
                },
                limit: list_args.limit,
                cursor: list_args.cursor.clone(),
                page_size: None,
                all: list_args.all,
            }),
            PromptCommand::Run(run_args) => Ok(BridgeDomainCommand::Prompt {
                name: run_args.name.clone(),
                arguments: build_structured_arguments(
                    &run_args.args,
                    &run_args.json_args,
                    run_args.args_json.as_deref(),
                    run_args.args_file.as_deref(),
                )?,
            }),
        },
        BridgeCommand::Auth(args) => Ok(match args.command {
            AuthCommand::Login => BridgeDomainCommand::AuthLogin,
            AuthCommand::Logout => BridgeDomainCommand::AuthLogout,
            AuthCommand::Status => BridgeDomainCommand::AuthStatus,
        }),
        BridgeCommand::Jobs(args) => Ok(match &args.command {
            JobsCommand::List => BridgeDomainCommand::JobsList,
            JobsCommand::Show(job) => BridgeDomainCommand::JobsShow {
                selector: job_selector(job),
            },
            JobsCommand::Wait(job) => BridgeDomainCommand::JobsWait {
                selector: job_selector(job),
            },
            JobsCommand::Cancel(job) => BridgeDomainCommand::JobsCancel {
                selector: job_selector(job),
            },
            JobsCommand::Watch(job) => BridgeDomainCommand::JobsWatch {
                selector: job_selector(job),
            },
        }),
        BridgeCommand::Doctor => Ok(BridgeDomainCommand::Doctor),
        BridgeCommand::Inspect => Ok(BridgeDomainCommand::Inspect),

        // -- backward-compatible legacy aliases --
        BridgeCommand::Discover(args) => Ok(BridgeDomainCommand::Discover {
            category: match args.command {
                DiscoverCommand::Capabilities => DiscoveryCategory::Capabilities,
                DiscoverCommand::Resources => DiscoveryCategory::Resources,
                DiscoverCommand::Prompts => DiscoveryCategory::Prompts,
            },
        }),
        BridgeCommand::Invoke(args) => Ok(BridgeDomainCommand::Invoke {
            capability: args.capability.clone(),
            arguments: build_invoke_arguments(args)?,
            background: args.background,
        }),
        BridgeCommand::Read(args) => Ok(BridgeDomainCommand::Read {
            uri: args.uri.clone(),
        }),
        BridgeCommand::List(args) => Ok(BridgeDomainCommand::List {
            capability: args.capability.clone(),
            cursor: args.cursor.clone(),
            page_size: args.page_size,
            all: args.all,
            limit: args.limit,
        }),
    }
}

fn is_demo_config(config: &crate::config::AppConfig) -> bool {
    matches!(config.server.transport, TransportKind::StreamableHttp)
        && config
            .server
            .endpoint
            .as_deref()
            .and_then(|value| url::Url::parse(value).ok())
            .and_then(|url| url.host_str().map(str::to_owned))
            .map(|host| host.eq_ignore_ascii_case("demo.invalid"))
            .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Domain command implementations (extracted from execute_domain_command)
// ---------------------------------------------------------------------------

async fn execute_discover(
    context: &AppContext,
    category: DiscoveryCategory,
) -> Result<CommandOutput> {
    let result = context
        .perform(McpOperation::Discover {
            category: category.clone(),
        })
        .await;
    match result {
        Ok(McpOperationResult::Discovery {
            message,
            category,
            items,
        }) => {
            context
                .services
                .state_store
                .upsert_discovery_inventory(
                    &context.config_name,
                    &context.config_name,
                    category.clone(),
                    items.clone(),
                )
                .await?;
            Ok(CommandOutput::new(
                &context.config_name,
                "discover",
                message,
                discovery_output_lines(&category, &items),
                json!({
                    "category": category,
                    "items": items,
                }),
            ))
        }
        Ok(other) => Err(anyhow!(
            "unexpected MCP response for discover: {}",
            serde_json::to_string(&other)?
        )),
        Err(error) => {
            let Some(output) =
                cached_discovery_output_from_state(context, &category, &error.to_string()).await
            else {
                return Err(error);
            };
            context.services.event_broker.emit(RuntimeEvent::Info {
                app_id: context.config_name.clone(),
                message: format!(
                    "live discover {} failed; returned cached inventory instead",
                    category.as_str()
                ),
            });
            Ok(output)
        }
    }
}

async fn execute_invoke(
    context: &AppContext,
    capability: String,
    arguments: Value,
    background: bool,
) -> Result<CommandOutput> {
    context.services.event_broker.emit(RuntimeEvent::Info {
        app_id: context.config_name.clone(),
        message: format!("invoking capability {}", capability),
    });
    let result = context
        .perform(McpOperation::InvokeAction {
            capability,
            arguments,
            background,
        })
        .await?;
    command_output_for_action("invoke", result, context).await
}

async fn execute_read(context: &AppContext, uri: String) -> Result<CommandOutput> {
    let result = context
        .perform(McpOperation::ReadResource { uri: uri.clone() })
        .await?;
    match result {
        McpOperationResult::Resource {
            message,
            uri,
            mime_type,
            text,
            data,
        } => {
            let descriptor = context
                .services
                .state_store
                .discovery_inventory_view(&context.config_name)
                .await
                .and_then(|inventory| {
                    lookup_inventory_item(inventory.resources.as_deref(), &uri, |item| {
                        item.get("uri").and_then(Value::as_str)
                    })
                });
            Ok(CommandOutput::new(
                &context.config_name,
                "read",
                message,
                resource_output_lines(
                    &uri,
                    mime_type.as_deref(),
                    text.as_deref(),
                    &data,
                    descriptor.as_ref(),
                ),
                cached_descriptor_data(
                    json!({
                        "uri": uri,
                        "mime_type": mime_type,
                        "text": text,
                        "data": data,
                    }),
                    descriptor.as_ref(),
                ),
            ))
        }
        other => Err(anyhow!(
            "unexpected MCP response for read: {}",
            serde_json::to_string(&other)?
        )),
    }
}

async fn execute_prompt(
    context: &AppContext,
    name: String,
    arguments: Value,
) -> Result<CommandOutput> {
    let result = context
        .perform(McpOperation::RunPrompt {
            name: name.clone(),
            arguments,
        })
        .await?;
    match result {
        McpOperationResult::Prompt {
            message,
            name,
            output,
            data,
        } => {
            let descriptor = context
                .services
                .state_store
                .discovery_inventory_view(&context.config_name)
                .await
                .and_then(|inventory| {
                    lookup_inventory_item(inventory.prompts.as_deref(), &name, |item| {
                        item.get("name").and_then(Value::as_str)
                    })
                });
            Ok(CommandOutput::new(
                &context.config_name,
                "prompt",
                message,
                prompt_output_lines(&name, &output, &data, descriptor.as_ref()),
                cached_descriptor_data(data, descriptor.as_ref()),
            ))
        }
        other => Err(anyhow!(
            "unexpected MCP response for prompt: {}",
            serde_json::to_string(&other)?
        )),
    }
}

async fn execute_jobs_list(context: &AppContext) -> Result<CommandOutput> {
    let jobs = context
        .services
        .state_store
        .jobs_for_config(&context.config_name)
        .await;
    let lines = if jobs.is_empty() {
        vec!["no jobs recorded".to_owned()]
    } else {
        jobs.iter()
            .map(|job| format!("{}  {}  {}", job.job_id, job.status.as_str(), job.command))
            .collect()
    };
    Ok(CommandOutput::new(
        &context.config_name,
        "jobs list",
        format!("listed {} jobs", jobs.len()),
        lines,
        json!({ "items": jobs }),
    ))
}

async fn execute_jobs_show(context: &AppContext, selector: JobSelector) -> Result<CommandOutput> {
    let mut job = resolve_job_selector(context, &selector).await?;
    if let Some(remote_task_id) = job.remote_task_id.clone() {
        if let Some(updated) = sync_job_with_remote(context, &job, "status").await? {
            job = updated;
        } else {
            job = context
                .services
                .state_store
                .update_job_status(
                    &context.config_name,
                    &job.job_id,
                    JobStatus::Failed,
                    Some(format!("remote task '{}' no longer exists", remote_task_id)),
                    None,
                    Some(format!("remote task '{}' no longer exists", remote_task_id)),
                )
                .await?;
        }
    }
    Ok(CommandOutput::new(
        &context.config_name,
        "jobs show",
        format!("showing job '{}'", job.job_id),
        default_job_detail_lines(&job),
        json!({ "job": job }),
    ))
}

async fn execute_jobs_wait(context: &AppContext, selector: JobSelector) -> Result<CommandOutput> {
    let job = resolve_job_selector(context, &selector).await?;
    if matches!(
        job.status,
        JobStatus::Completed | JobStatus::Canceled | JobStatus::Failed
    ) {
        return Ok(CommandOutput::new(
            &context.config_name,
            "jobs wait",
            format!("job '{}' is already {}", job.job_id, job.status.as_str()),
            default_job_overview_lines(&job),
            json!({ "job": job }),
        ));
    }
    if let Some(updated) = sync_job_with_remote(context, &job, "wait").await? {
        return Ok(CommandOutput::new(
            &context.config_name,
            "jobs wait",
            format!("job '{}' is {}", updated.job_id, updated.status.as_str()),
            default_job_overview_lines(&updated),
            json!({ "job": updated }),
        ));
    }
    let failed = context
        .services
        .state_store
        .update_job_status(
            &context.config_name,
            &job.job_id,
            JobStatus::Failed,
            Some("remote task disappeared while waiting".to_owned()),
            None,
            Some("remote task disappeared while waiting".to_owned()),
        )
        .await?;
    Ok(CommandOutput::new(
        &context.config_name,
        "jobs wait",
        format!("job '{}' failed", failed.job_id),
        default_job_overview_lines(&failed),
        json!({ "job": failed }),
    ))
}

async fn execute_jobs_cancel(context: &AppContext, selector: JobSelector) -> Result<CommandOutput> {
    let job = resolve_job_selector(context, &selector).await?;
    if matches!(job.status, JobStatus::Completed | JobStatus::Canceled) {
        return Ok(CommandOutput::new(
            &context.config_name,
            "jobs cancel",
            format!("job '{}' is already {}", job.job_id, job.status.as_str()),
            default_job_overview_lines(&job),
            json!({ "job": job }),
        ));
    }
    if let Some(updated) = sync_job_with_remote(context, &job, "cancel").await? {
        return Ok(CommandOutput::new(
            &context.config_name,
            "jobs cancel",
            format!("job '{}' is {}", updated.job_id, updated.status.as_str()),
            default_job_overview_lines(&updated),
            json!({ "job": updated }),
        ));
    }
    let canceled = context
        .services
        .state_store
        .update_job_status(
            &context.config_name,
            &job.job_id,
            JobStatus::Canceled,
            Some("job canceled locally".to_owned()),
            None,
            Some("job canceled locally".to_owned()),
        )
        .await?;
    Ok(CommandOutput::new(
        &context.config_name,
        "jobs cancel",
        format!("job '{}' is canceled", canceled.job_id),
        default_job_overview_lines(&canceled),
        json!({ "job": canceled }),
    ))
}

async fn execute_jobs_watch(context: &AppContext, selector: JobSelector) -> Result<CommandOutput> {
    let mut job = resolve_job_selector(context, &selector).await?;
    context.services.event_broker.emit(RuntimeEvent::JobUpdate {
        app_id: context.config_name.clone(),
        job_id: job.job_id.clone(),
        status: job.status.as_str().to_owned(),
        message: "watch started".to_owned(),
    });
    if matches!(job.status, JobStatus::Queued | JobStatus::Running)
        && let Some(updated) = sync_job_with_remote(context, &job, "wait").await?
    {
        job = updated;
    }
    context.services.event_broker.emit(RuntimeEvent::JobUpdate {
        app_id: context.config_name.clone(),
        job_id: job.job_id.clone(),
        status: job.status.as_str().to_owned(),
        message: "watch completed".to_owned(),
    });
    Ok(CommandOutput::new(
        &context.config_name,
        "jobs watch",
        format!("job '{}' is {}", job.job_id, job.status.as_str()),
        default_job_overview_lines(&job),
        json!({ "job": job }),
    ))
}

async fn execute_doctor(context: &AppContext) -> Result<CommandOutput> {
    let metadata = context
        .services
        .mcp_client
        .metadata(&context.config_name)
        .await?;
    let auth = context
        .services
        .state_store
        .auth_session(&context.config_name)
        .await;
    let negotiated = context
        .services
        .state_store
        .negotiated_capability_view(&context.config_name)
        .await;
    let inventory = context
        .services
        .state_store
        .discovery_inventory_view(&context.config_name)
        .await;

    let auth_state = auth
        .as_ref()
        .map(|record| record.state.as_str().to_owned())
        .unwrap_or_else(|| "unauthenticated".to_owned());
    let negotiated_summary = negotiated.as_ref().map(|view| {
        format!(
            "protocol {} with {} capability groups cached",
            view.protocol_version,
            negotiated_capability_group_count(&view.server_capabilities)
        )
    });
    let inventory_summary = inventory.as_ref().map(|view| {
        format!(
            "{} tools, {} resources, {} prompts cached",
            view.tools.as_ref().map(Vec::len).unwrap_or(0),
            view.resources.as_ref().map(Vec::len).unwrap_or(0),
            view.prompts.as_ref().map(Vec::len).unwrap_or(0)
        )
    });
    let server_name = negotiated
        .as_ref()
        .and_then(|view| view.server_info.as_ref().map(|value| value.name.as_str()))
        .unwrap_or(metadata.server_name.as_str());
    let server_version = negotiated
        .as_ref()
        .and_then(|view| {
            view.server_info
                .as_ref()
                .map(|value| value.version.as_str())
        })
        .unwrap_or(metadata.server_version.as_str());

    Ok(CommandOutput::new(
        &context.config_name,
        "doctor",
        "bridge runtime health summary".to_owned(),
        [
            vec![
                format!("config: {}", context.config_name),
                format!("transport: {}", metadata.transport.as_str()),
                format!("server: {} {}", server_name, server_version),
                format!("auth: {}", auth_state),
            ],
            negotiated_summary
                .map(|value| vec![format!("negotiated: {}", value)])
                .unwrap_or_default(),
            inventory_summary
                .map(|value| vec![format!("inventory: {}", value)])
                .unwrap_or_default(),
        ]
        .concat(),
        json!({
            "runtime_version": env!("CARGO_PKG_VERSION"),
            "server": metadata,
            "auth": auth,
            "negotiated": negotiated,
            "inventory": inventory,
        }),
    ))
}

/// Inspect server capabilities, mapped command tree, and operational metadata.
async fn execute_inspect(context: &AppContext) -> Result<CommandOutput> {
    let metadata = context
        .services
        .mcp_client
        .metadata(&context.config_name)
        .await?;
    let negotiated = context
        .services
        .state_store
        .negotiated_capability_view(&context.config_name)
        .await;
    let inventory = context
        .services
        .state_store
        .discovery_inventory_view(&context.config_name)
        .await;
    let auth = context
        .services
        .state_store
        .auth_session(&context.config_name)
        .await;

    let mut lines = vec![
        format!("config: {}", context.config_name),
        format!("invoked_as: {}", context.invoked_as),
        format!("transport: {}", metadata.transport.as_str()),
        format!(
            "server: {} {}",
            metadata.server_name, metadata.server_version
        ),
    ];

    if let Some(view) = &negotiated {
        lines.push(format!("protocol: {}", view.protocol_version));
        let caps = &view.server_capabilities;
        let mut supported = Vec::new();
        if caps.tools.is_some() {
            supported.push("tools");
        }
        if caps.resources.is_some() {
            supported.push("resources");
        }
        if caps.prompts.is_some() {
            supported.push("prompts");
        }
        if caps.logging.is_some() {
            supported.push("logging");
        }
        if caps.completions.is_some() {
            supported.push("completions");
        }
        lines.push(format!("server_capabilities: {}", supported.join(", ")));
    }

    lines.push(format!(
        "auth: {}",
        auth.as_ref()
            .map(|record| record.state.as_str().to_owned())
            .unwrap_or_else(|| "unauthenticated".to_owned())
    ));

    if let Some(inv) = &inventory {
        lines.push(String::new());
        lines.push("--- mapped actions (tools) ---".to_owned());
        if let Some(tools) = &inv.tools {
            for item in tools {
                let id = item
                    .get("name")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                let desc = item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                lines.push(format!("  {} — {}", id, desc));
            }
        }
        lines.push("--- collections (resources) ---".to_owned());
        if let Some(resources) = &inv.resources {
            for item in resources {
                let uri = item.get("uri").and_then(Value::as_str).unwrap_or("?");
                let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                lines.push(format!("  {} — {}", uri, name));
            }
        }
        lines.push("--- guided flows (prompts) ---".to_owned());
        if let Some(prompts) = &inv.prompts {
            for item in prompts {
                let name = item
                    .get("name")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                let desc = item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                lines.push(format!("  {} — {}", name, desc));
            }
        }
    } else {
        lines.push(String::new());
        lines.push(
            "no cached inventory — run 'tool list', 'resource list', and 'prompt list' first"
                .to_owned(),
        );
    }

    Ok(CommandOutput::new(
        &context.config_name,
        "inspect",
        "server capability inspection".to_owned(),
        lines,
        json!({
            "config_name": context.config_name,
            "invoked_as": context.invoked_as,
            "server": metadata,
            "negotiated": negotiated,
            "inventory": inventory,
            "auth": auth,
        }),
    ))
}

async fn auth_login(context: &AppContext) -> Result<CommandOutput> {
    if is_demo_config(&context.config) {
        return auth_login_demo(context).await;
    }

    // For real servers, prompt for a bearer token on stdin.
    context
        .services
        .event_broker
        .emit(RuntimeEvent::AuthPrompt {
            app_id: context.config_name.clone(),
            message: "enter bearer token for authentication".to_owned(),
        });

    let token = read_bearer_token_from_stdin()?;
    if token.is_empty() {
        return Err(anyhow!(
            "auth login requires a non-empty bearer token; pipe one via stdin or enter it interactively"
        ));
    }

    let stored = StoredToken {
        bearer_token: token,
        account: None,
        updated_at: chrono::Utc::now(),
    };
    context
        .services
        .token_store
        .put(&context.config_name, stored.clone())
        .await?;
    let record = AuthSessionRecord {
        config_name: context.config_name.clone(),
        app_id: context.config_name.clone(),
        state: AuthSessionState::Authenticated,
        account: None,
        server: Some(context.config.server.display_name.clone()),
        updated_at: stored.updated_at,
    };
    context
        .services
        .state_store
        .upsert_auth_session(record)
        .await?;

    Ok(CommandOutput::new(
        &context.config_name,
        "auth login",
        "bearer token stored".to_owned(),
        vec![
            "status: authenticated".to_owned(),
            format!("server: {}", context.config.server.display_name),
        ],
        json!({
            "status": "authenticated",
            "server": context.config.server.display_name,
        }),
    ))
}

async fn auth_login_demo(context: &AppContext) -> Result<CommandOutput> {
    context
        .services
        .event_broker
        .emit(RuntimeEvent::AuthPrompt {
            app_id: context.config_name.clone(),
            message: "open browser to complete login".to_owned(),
        });
    let account = "demo.user@example.com".to_owned();
    let record = AuthSessionRecord {
        config_name: context.config_name.clone(),
        app_id: context.config_name.clone(),
        state: AuthSessionState::Authenticated,
        account: Some(account.clone()),
        server: Some(context.config.server.display_name.clone()),
        updated_at: chrono::Utc::now(),
    };
    context
        .services
        .state_store
        .upsert_auth_session(record)
        .await?;
    Ok(CommandOutput::new(
        &context.config_name,
        "auth login",
        "browser login completed".to_owned(),
        vec![
            "status: authenticated".to_owned(),
            format!("account: {}", account),
        ],
        json!({
            "status": "authenticated",
            "account": account,
            "details": { "browser_url": "https://demo.invalid/auth" },
        }),
    ))
}

async fn auth_logout(context: &AppContext) -> Result<CommandOutput> {
    if is_demo_config(&context.config) {
        return auth_logout_demo(context).await;
    }

    context
        .services
        .token_store
        .remove(&context.config_name)
        .await?;
    let record = AuthSessionRecord {
        config_name: context.config_name.clone(),
        app_id: context.config_name.clone(),
        state: AuthSessionState::LoggedOut,
        account: None,
        server: Some(context.config.server.display_name.clone()),
        updated_at: chrono::Utc::now(),
    };
    context
        .services
        .state_store
        .upsert_auth_session(record)
        .await?;

    Ok(CommandOutput::new(
        &context.config_name,
        "auth logout",
        "bearer token cleared".to_owned(),
        vec!["status: logged_out".to_owned()],
        json!({ "status": "logged_out" }),
    ))
}

async fn auth_logout_demo(context: &AppContext) -> Result<CommandOutput> {
    let record = AuthSessionRecord {
        config_name: context.config_name.clone(),
        app_id: context.config_name.clone(),
        state: AuthSessionState::LoggedOut,
        account: None,
        server: Some(context.config.server.display_name.clone()),
        updated_at: chrono::Utc::now(),
    };
    context
        .services
        .state_store
        .upsert_auth_session(record)
        .await?;
    Ok(CommandOutput::new(
        &context.config_name,
        "auth logout",
        "remote auth session cleared".to_owned(),
        vec!["status: logged_out".to_owned()],
        json!({ "status": "logged_out" }),
    ))
}

async fn auth_status(context: &AppContext) -> Result<CommandOutput> {
    if is_demo_config(&context.config) {
        return auth_status_demo(context).await;
    }

    let token = context
        .services
        .token_store
        .get(&context.config_name)
        .await?;
    let local_auth = context
        .services
        .state_store
        .auth_session(&context.config_name)
        .await;
    let state = if token.is_some() {
        AuthSessionState::Authenticated
    } else {
        AuthSessionState::LoggedOut
    };
    let account = local_auth.as_ref().and_then(|r| r.account.clone());
    let updated_at = token
        .as_ref()
        .map(|t| t.updated_at)
        .or_else(|| local_auth.as_ref().map(|r| r.updated_at))
        .unwrap_or_else(chrono::Utc::now);

    let record = AuthSessionRecord {
        config_name: context.config_name.clone(),
        app_id: context.config_name.clone(),
        state: state.clone(),
        account: account.clone(),
        server: Some(context.config.server.display_name.clone()),
        updated_at,
    };
    context
        .services
        .state_store
        .upsert_auth_session(record.clone())
        .await?;

    Ok(CommandOutput::new(
        &context.config_name,
        "auth status",
        format!("bridge auth is {}", state.as_str()),
        vec![
            format!("state: {}", state.as_str()),
            format!(
                "account: {}",
                account.unwrap_or_else(|| "unknown".to_owned())
            ),
            format!("server: {}", context.config.server.display_name),
            format!("updated: {}", updated_at.to_rfc3339()),
        ],
        json!({
            "state": state,
            "account": record.account,
            "server": context.config.server.display_name,
            "updated_at": updated_at,
        }),
    ))
}

async fn auth_status_demo(context: &AppContext) -> Result<CommandOutput> {
    let local_auth = context
        .services
        .state_store
        .auth_session(&context.config_name)
        .await;
    let metadata = context
        .services
        .mcp_client
        .metadata(&context.config_name)
        .await?;
    let state = local_auth
        .as_ref()
        .map(|r| r.state.clone())
        .unwrap_or(AuthSessionState::LoggedOut);
    let account = local_auth.as_ref().and_then(|r| r.account.clone());
    let reconciled = AuthSessionRecord {
        config_name: context.config_name.clone(),
        app_id: context.config_name.clone(),
        state: state.clone(),
        account: account.clone(),
        server: Some(context.config.server.display_name.clone()),
        updated_at: chrono::Utc::now(),
    };
    context
        .services
        .state_store
        .upsert_auth_session(reconciled.clone())
        .await?;
    Ok(CommandOutput::new(
        &context.config_name,
        "auth status",
        format!("bridge auth is {}", reconciled.state.as_str()),
        vec![
            format!("state: {}", reconciled.state.as_str()),
            format!(
                "account: {}",
                reconciled
                    .account
                    .clone()
                    .unwrap_or_else(|| "unknown".to_owned())
            ),
            format!("server: {}", context.config.server.display_name),
            format!("updated: {}", reconciled.updated_at.to_rfc3339()),
        ],
        json!({
            "state": reconciled.state,
            "account": reconciled.account,
            "server": context.config.server.display_name,
            "updated_at": reconciled.updated_at,
            "metadata": metadata,
        }),
    ))
}

fn read_bearer_token_from_stdin() -> Result<String> {
    use std::io::{self, BufRead};
    let stdin = io::stdin();
    let mut line = String::new();
    stdin
        .lock()
        .read_line(&mut line)
        .map_err(|error| anyhow!("failed to read bearer token from stdin: {}", error))?;
    Ok(line.trim().to_owned())
}

fn job_status_from_task_state(status: &TaskState) -> JobStatus {
    match status {
        TaskState::Queued => JobStatus::Queued,
        TaskState::Running => JobStatus::Running,
        TaskState::Completed => JobStatus::Completed,
        TaskState::Canceled => JobStatus::Canceled,
        TaskState::Failed => JobStatus::Failed,
    }
}

async fn sync_job_with_remote(
    context: &AppContext,
    job: &crate::runtime::JobRecord,
    action: &str,
) -> Result<Option<crate::runtime::JobRecord>> {
    let Some(remote_task_id) = job.remote_task_id.clone() else {
        return Ok(None);
    };

    let operation = match action {
        "status" => McpOperation::TaskGet {
            task_id: remote_task_id,
        },
        "wait" => McpOperation::TaskResult {
            task_id: remote_task_id,
        },
        "cancel" => McpOperation::TaskCancel {
            task_id: remote_task_id,
        },
        other => return Err(anyhow!("unknown job sync action: {}", other)),
    };

    let result = context.perform(operation).await?;

    match result {
        McpOperationResult::Task {
            status,
            message,
            data,
            result,
            failure_reason,
            ..
        } => {
            let updated = context
                .services
                .state_store
                .update_job_status(
                    &context.config_name,
                    &job.job_id,
                    job_status_from_task_state(&status),
                    Some(
                        data.get("summary")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                            .unwrap_or(message),
                    ),
                    result,
                    failure_reason,
                )
                .await?;
            Ok(Some(updated))
        }
        other => Err(anyhow!(
            "unexpected MCP response for remote job sync: {}",
            serde_json::to_string(&other)?
        )),
    }
}

fn job_selector(args: &JobSelectorArgs) -> JobSelector {
    JobSelector {
        job_id: args.job_id.clone(),
        latest: args.latest,
        command: args.command.clone(),
    }
}

async fn resolve_job_selector(
    context: &AppContext,
    selector: &JobSelector,
) -> Result<crate::runtime::JobRecord> {
    if let Some(job_id) = &selector.job_id {
        return context
            .services
            .state_store
            .job_for_config(&context.config_name, job_id)
            .await
            .ok_or_else(|| anyhow!("job '{}' was not found", job_id));
    }

    let selected = context
        .services
        .state_store
        .latest_job_for_config(&context.config_name, selector.command.as_deref())
        .await;

    match selected {
        Some(job) => Ok(job),
        None if selector.command.is_some() => Err(anyhow!(
            "no jobs found for command '{}'",
            selector.command.as_deref().unwrap_or_default()
        )),
        None => Err(anyhow!(
            "no jobs found; run a background command first or pass an explicit job id"
        )),
    }
}

fn build_invoke_arguments(args: &InvokeArgs) -> Result<Value> {
    build_structured_arguments(
        &args.args,
        &args.json_args,
        args.args_json.as_deref(),
        args.args_file.as_deref(),
    )
}

fn build_structured_arguments(
    args: &[String],
    json_args: &[String],
    args_json: Option<&str>,
    args_file: Option<&Path>,
) -> Result<Value> {
    let mut object = match args_file {
        Some(path) => parse_json_object_file(path, "--args-file")?,
        None => Map::new(),
    };

    if let Some(raw) = args_json {
        merge_json_object(&mut object, parse_json_object(raw, "--args-json")?);
    }

    for value in args {
        let (key, raw) = split_assignment(value, "--arg")?;
        insert_nested_value(&mut object, key, parse_scalar_value(raw))?;
    }

    for value in json_args {
        let (key, raw) = split_assignment(value, "--arg-json")?;
        let parsed = serde_json::from_str(raw)
            .map_err(|error| anyhow!("invalid JSON value for --arg-json '{}': {}", value, error))?;
        insert_nested_value(&mut object, key, parsed)?;
    }

    Ok(Value::Object(object))
}

fn pretty_json_lines(value: &Value) -> Vec<String> {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| "<invalid-json>".to_owned())
        .lines()
        .map(ToOwned::to_owned)
        .collect()
}

fn resource_output_lines(
    uri: &str,
    mime_type: Option<&str>,
    text: Option<&str>,
    data: &Value,
    descriptor: Option<&Value>,
) -> Vec<String> {
    let mut lines = vec![format!("uri: {}", uri)];
    lines.extend(cached_descriptor_lines(descriptor));
    lines.push(format!("mime_type: {}", mime_type.unwrap_or("(unknown)")));

    if let Some(text) = text {
        let text_lines = text.lines().collect::<Vec<_>>();
        if text_lines.len() <= 1 {
            lines.push(format!("content: {}", text));
        } else {
            lines.push("content:".to_owned());
            lines.extend(text_lines.into_iter().map(|line| format!("  {}", line)));
        }
    } else {
        lines.push("data:".to_owned());
        lines.extend(
            pretty_json_lines(data)
                .into_iter()
                .map(|line| format!("  {}", line)),
        );
    }

    lines
}

fn prompt_output_lines(
    name: &str,
    output: &str,
    data: &Value,
    descriptor: Option<&Value>,
) -> Vec<String> {
    let mut lines = vec![format!("prompt: {}", name)];
    lines.extend(cached_descriptor_lines(descriptor));
    lines.push("output:".to_owned());
    lines.extend(output.lines().map(|line| format!("  {}", line)));

    if let Some(arguments) = data.get("arguments")
        && arguments
            .as_object()
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    {
        lines.push("arguments:".to_owned());
        lines.extend(
            pretty_json_lines(arguments)
                .into_iter()
                .map(|line| format!("  {}", line)),
        );
    }

    lines
}

fn lookup_inventory_item<F>(
    items: Option<&[Value]>,
    identifier: &str,
    extract_identifier: F,
) -> Option<Value>
where
    F: Fn(&Value) -> Option<&str>,
{
    items?
        .iter()
        .find(|item| extract_identifier(item) == Some(identifier))
        .cloned()
}

fn cached_descriptor_lines(descriptor: Option<&Value>) -> Vec<String> {
    let Some(descriptor) = descriptor else {
        return Vec::new();
    };

    let mut lines = Vec::new();
    if let Some(title) = descriptor.get("title").and_then(Value::as_str)
        && !title.trim().is_empty()
    {
        lines.push(format!("title: {}", title));
    }
    if let Some(name) = descriptor.get("name").and_then(Value::as_str)
        && !name.trim().is_empty()
    {
        lines.push(format!("name: {}", name));
    }
    if let Some(description) = descriptor.get("description").and_then(Value::as_str)
        && !description.trim().is_empty()
    {
        lines.push(format!("description: {}", description));
    }
    lines
}

fn cached_descriptor_data(mut data: Value, descriptor: Option<&Value>) -> Value {
    if let Some(descriptor) = descriptor
        && let Some(object) = data.as_object_mut()
    {
        object.insert("cached_descriptor".to_owned(), descriptor.clone());
    }
    data
}

fn discovery_output_lines(category: &DiscoveryCategory, items: &[Value]) -> Vec<String> {
    if items.is_empty() {
        return vec![format!("no {} discovered", category.as_str())];
    }

    match category {
        DiscoveryCategory::Capabilities => items
            .iter()
            .map(|item| {
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("(unknown)");
                let kind = item
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let description = item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("(no description)");
                format!("{}  {}  {}", id, kind, description)
            })
            .collect(),
        DiscoveryCategory::Resources => items
            .iter()
            .map(|item| {
                let uri = item
                    .get("uri")
                    .and_then(Value::as_str)
                    .unwrap_or("(unknown)");
                let mime_type = item
                    .get("mime_type")
                    .and_then(Value::as_str)
                    .unwrap_or("(unknown)");
                let description = item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("(no description)");
                format!("{}  {}  {}", uri, mime_type, description)
            })
            .collect(),
        DiscoveryCategory::Prompts => items
            .iter()
            .map(|item| {
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("(unknown)");
                let description = item
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("(no description)");
                format!("{}  {}", name, description)
            })
            .collect(),
    }
}

async fn list_command_output(
    context: &AppContext,
    selector: &str,
    limit: Option<u32>,
    cursor: Option<String>,
) -> Result<CommandOutput> {
    let parsed = parse_list_selector(selector)?;
    let snapshot = fetch_discovery_items_snapshot(context, &parsed.category).await?;
    let filtered = filter_list_items(&parsed.category, &snapshot.items, parsed.filter.as_deref());
    let (items, next_cursor) = paginate_list_items(filtered, limit, cursor.as_deref())?;

    let mut lines = stale_discovery_prefix_lines(&snapshot);
    if items.is_empty() {
        lines.push(format!(
            "no {} matched '{}'",
            parsed.category.as_str(),
            selector
        ));
    } else {
        lines.extend(discovery_output_lines(&parsed.category, &items));
    }

    let summary = if snapshot.cached {
        format!(
            "listed {} cached {} for '{}'",
            items.len(),
            parsed.category.as_str(),
            selector
        )
    } else {
        format!(
            "listed {} {} for '{}'",
            items.len(),
            parsed.category.as_str(),
            selector
        )
    };

    Ok(CommandOutput::new(
        &context.config_name,
        "list",
        summary,
        lines,
        json!({
            "category": parsed.category,
            "selector": selector,
            "filter": parsed.filter,
            "items": items,
            "next_cursor": next_cursor,
            "cached": snapshot.cached,
            "live_error": snapshot.live_error,
            "updated_at": snapshot.updated_at,
        }),
    ))
}

async fn fetch_discovery_items_snapshot(
    context: &AppContext,
    category: &DiscoveryCategory,
) -> Result<DiscoveryItemsSnapshot> {
    let result = context
        .perform(McpOperation::Discover {
            category: category.clone(),
        })
        .await;

    match result {
        Ok(McpOperationResult::Discovery {
            category: response_category,
            items,
            ..
        }) => {
            context
                .services
                .state_store
                .upsert_discovery_inventory(
                    &context.config_name,
                    &context.config_name,
                    response_category,
                    items.clone(),
                )
                .await?;
            Ok(DiscoveryItemsSnapshot {
                items,
                cached: false,
                live_error: None,
                updated_at: None,
            })
        }
        Ok(other) => Err(anyhow!(
            "unexpected MCP response for discover: {}",
            serde_json::to_string(&other)?
        )),
        Err(error) => {
            let Some(inventory) = context
                .services
                .state_store
                .discovery_inventory_view(&context.config_name)
                .await
            else {
                return Err(error);
            };
            let Some(items) = cached_discovery_items(&inventory, category) else {
                return Err(error);
            };
            Ok(DiscoveryItemsSnapshot {
                items,
                cached: true,
                live_error: Some(error.to_string()),
                updated_at: Some(inventory.updated_at),
            })
        }
    }
}

fn parse_list_selector(selector: &str) -> Result<ParsedListSelector> {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("list selector must not be empty"));
    }

    let (prefix, remainder) = trimmed
        .split_once('.')
        .map(|(prefix, remainder)| (prefix, Some(remainder)))
        .unwrap_or((trimmed, None));

    let category = match prefix {
        "capabilities" | "capability" | "tools" | "tool" => DiscoveryCategory::Capabilities,
        "resources" | "resource" => DiscoveryCategory::Resources,
        "prompts" | "prompt" => DiscoveryCategory::Prompts,
        _ => {
            return Err(anyhow!(
                "invalid list selector '{}'; expected tools, resources, prompts, or a prefixed selector like resources.files",
                selector
            ));
        }
    };

    let filter = remainder
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    Ok(ParsedListSelector { category, filter })
}

fn filter_list_items(
    category: &DiscoveryCategory,
    items: &[Value],
    filter: Option<&str>,
) -> Vec<Value> {
    let Some(filter) = filter else {
        return items.to_vec();
    };
    let filter_terms = selector_filter_terms(filter);

    items
        .iter()
        .filter(|item| item_matches_filter(category, item, &filter_terms))
        .cloned()
        .collect()
}

fn selector_filter_terms(filter: &str) -> Vec<String> {
    let mut terms = vec![filter.to_ascii_lowercase()];
    if filter.contains('.') {
        terms.push(filter.replace('.', "/").to_ascii_lowercase());
        terms.push(filter.replace('.', "-").to_ascii_lowercase());
        terms.extend(
            filter
                .split('.')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_ascii_lowercase()),
        );
    }
    terms.sort();
    terms.dedup();
    terms
}

fn item_matches_filter(
    category: &DiscoveryCategory,
    item: &Value,
    filter_terms: &[String],
) -> bool {
    let haystacks = list_item_search_fields(category, item);
    filter_terms.iter().any(|term| {
        haystacks
            .iter()
            .any(|value| value.to_ascii_lowercase().contains(term))
    })
}

fn list_item_search_fields(category: &DiscoveryCategory, item: &Value) -> Vec<String> {
    match category {
        DiscoveryCategory::Capabilities => ["id", "title", "description", "kind"]
            .into_iter()
            .filter_map(|key| item.get(key).and_then(Value::as_str).map(ToOwned::to_owned))
            .collect(),
        DiscoveryCategory::Resources => ["uri", "name", "description", "mime_type"]
            .into_iter()
            .filter_map(|key| item.get(key).and_then(Value::as_str).map(ToOwned::to_owned))
            .collect(),
        DiscoveryCategory::Prompts => ["name", "title", "description"]
            .into_iter()
            .filter_map(|key| item.get(key).and_then(Value::as_str).map(ToOwned::to_owned))
            .collect(),
    }
}

fn paginate_list_items(
    items: Vec<Value>,
    limit: Option<u32>,
    cursor: Option<&str>,
) -> Result<(Vec<Value>, Option<String>)> {
    let start = match cursor {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| anyhow!("invalid list cursor '{}'; expected a numeric offset", raw))?,
        None => 0,
    };

    if start > items.len() {
        return Err(anyhow!(
            "list cursor '{}' is past the available inventory",
            cursor.unwrap_or_default()
        ));
    }

    let remaining = items.into_iter().skip(start).collect::<Vec<_>>();
    let Some(limit) = limit else {
        return Ok((remaining, None));
    };
    let end = remaining.len().min(limit as usize);
    let next_cursor = (end < remaining.len()).then(|| (start + end).to_string());
    Ok((
        remaining.into_iter().take(limit as usize).collect(),
        next_cursor,
    ))
}

fn stale_discovery_prefix_lines(snapshot: &DiscoveryItemsSnapshot) -> Vec<String> {
    if !snapshot.cached {
        return Vec::new();
    }

    let mut lines = Vec::new();
    if let Some(updated_at) = snapshot.updated_at {
        lines.push(format!(
            "source: cached inventory from {}",
            updated_at.to_rfc3339()
        ));
    }
    if let Some(live_error) = &snapshot.live_error {
        lines.push(format!("live_error: {}", live_error));
    }
    lines
}

async fn cached_discovery_output_from_state(
    context: &AppContext,
    category: &DiscoveryCategory,
    live_error: &str,
) -> Option<CommandOutput> {
    let inventory = context
        .services
        .state_store
        .discovery_inventory_view(&context.config_name)
        .await?;
    cached_discovery_output_from_inventory(&context.config_name, category, &inventory, live_error)
}

fn cached_discovery_output_from_inventory(
    config_name: &str,
    category: &DiscoveryCategory,
    inventory: &DiscoveryInventoryView,
    live_error: &str,
) -> Option<CommandOutput> {
    let items = cached_discovery_items(inventory, category)?;
    let mut lines = vec![
        format!(
            "source: cached inventory from {}",
            inventory.updated_at.to_rfc3339()
        ),
        format!("live_error: {}", live_error),
    ];
    lines.extend(discovery_output_lines(category, &items));

    Some(CommandOutput::new(
        config_name,
        "discover",
        format!(
            "returned {} cached {} because live discovery failed",
            items.len(),
            category.as_str()
        ),
        lines,
        json!({
            "category": category,
            "items": items,
            "cached": true,
            "live_error": live_error,
            "updated_at": inventory.updated_at,
        }),
    ))
}

fn cached_discovery_items(
    inventory: &DiscoveryInventoryView,
    category: &DiscoveryCategory,
) -> Option<Vec<Value>> {
    match category {
        DiscoveryCategory::Capabilities => inventory.tools.clone(),
        DiscoveryCategory::Resources => inventory.resources.clone(),
        DiscoveryCategory::Prompts => inventory.prompts.clone(),
    }
}

async fn enforce_cached_capability_support(
    context: &AppContext,
    command: &BridgeDomainCommand,
) -> Result<()> {
    let Some(view) = context
        .services
        .state_store
        .negotiated_capability_view(&context.config_name)
        .await
    else {
        return Ok(());
    };

    validate_cached_capability_support(&view, command)?;

    let inventory = context
        .services
        .state_store
        .discovery_inventory_view(&context.config_name)
        .await;
    validate_cached_inventory_support(inventory.as_ref(), command)
}

fn validate_cached_capability_support(
    view: &NegotiatedCapabilityView,
    command: &BridgeDomainCommand,
) -> Result<()> {
    let Some(group) = required_capability_group(command) else {
        return Ok(());
    };
    if cached_capability_supported(view, group) {
        return Ok(());
    }

    Err(anyhow!(
        "cached negotiated capability view from {} shows server '{}' does not advertise {} support required for '{}'; refresh the cache with '{}' after reconnecting if server capabilities changed",
        view.updated_at.to_rfc3339(),
        view.server_info
            .as_ref()
            .map(|value| value.name.as_str())
            .unwrap_or("(unknown)"),
        capability_group_name(group),
        command_summary(command),
        discovery_refresh_command(group),
    ))
}

fn required_capability_group(command: &BridgeDomainCommand) -> Option<CapabilityGroup> {
    match command {
        BridgeDomainCommand::Discover { category } => match category {
            DiscoveryCategory::Capabilities => Some(CapabilityGroup::Tools),
            DiscoveryCategory::Resources => Some(CapabilityGroup::Resources),
            DiscoveryCategory::Prompts => Some(CapabilityGroup::Prompts),
        },
        BridgeDomainCommand::Invoke { .. } => Some(CapabilityGroup::Tools),
        BridgeDomainCommand::Read { .. } => Some(CapabilityGroup::Resources),
        BridgeDomainCommand::List { capability, .. } => {
            parse_list_selector(capability)
                .ok()
                .map(|parsed| match parsed.category {
                    DiscoveryCategory::Capabilities => CapabilityGroup::Tools,
                    DiscoveryCategory::Resources => CapabilityGroup::Resources,
                    DiscoveryCategory::Prompts => CapabilityGroup::Prompts,
                })
        }
        BridgeDomainCommand::Prompt { .. } => Some(CapabilityGroup::Prompts),
        _ => None,
    }
}

fn cached_capability_supported(view: &NegotiatedCapabilityView, group: CapabilityGroup) -> bool {
    match group {
        CapabilityGroup::Tools => view.server_capabilities.tools.is_some(),
        CapabilityGroup::Resources => view.server_capabilities.resources.is_some(),
        CapabilityGroup::Prompts => view.server_capabilities.prompts.is_some(),
    }
}

fn capability_group_name(group: CapabilityGroup) -> &'static str {
    match group {
        CapabilityGroup::Tools => "tool",
        CapabilityGroup::Resources => "resource",
        CapabilityGroup::Prompts => "prompt",
    }
}

fn discovery_refresh_command(group: CapabilityGroup) -> &'static str {
    match group {
        CapabilityGroup::Tools => "discover capabilities",
        CapabilityGroup::Resources => "discover resources",
        CapabilityGroup::Prompts => "discover prompts",
    }
}

fn command_summary(command: &BridgeDomainCommand) -> &'static str {
    match command {
        BridgeDomainCommand::Discover { category } => match category {
            DiscoveryCategory::Capabilities => "discover capabilities",
            DiscoveryCategory::Resources => "discover resources",
            DiscoveryCategory::Prompts => "discover prompts",
        },
        BridgeDomainCommand::Invoke { .. } => "invoke",
        BridgeDomainCommand::Read { .. } => "read",
        BridgeDomainCommand::List { .. } => "list",
        BridgeDomainCommand::Prompt { .. } => "prompt",
        BridgeDomainCommand::AuthLogin => "auth login",
        BridgeDomainCommand::AuthLogout => "auth logout",
        BridgeDomainCommand::AuthStatus => "auth status",
        BridgeDomainCommand::JobsList => "jobs list",
        BridgeDomainCommand::JobsShow { .. } => "jobs show",
        BridgeDomainCommand::JobsWait { .. } => "jobs wait",
        BridgeDomainCommand::JobsCancel { .. } => "jobs cancel",
        BridgeDomainCommand::JobsWatch { .. } => "jobs watch",
        BridgeDomainCommand::Doctor => "doctor",
        BridgeDomainCommand::Inspect => "inspect",
    }
}

fn validate_cached_inventory_support(
    inventory: Option<&DiscoveryInventoryView>,
    command: &BridgeDomainCommand,
) -> Result<()> {
    let Some(inventory) = inventory else {
        return Ok(());
    };

    match command {
        BridgeDomainCommand::Invoke { capability, .. } => validate_cached_identifier_membership(
            inventory.tools.as_deref(),
            capability,
            "tool",
            "discover capabilities",
            |item| item.get("id").and_then(Value::as_str),
        ),
        BridgeDomainCommand::Read { uri } => validate_cached_identifier_membership(
            inventory.resources.as_deref(),
            uri,
            "resource",
            "discover resources",
            |item| item.get("uri").and_then(Value::as_str),
        ),
        BridgeDomainCommand::Prompt { name, .. } => validate_cached_identifier_membership(
            inventory.prompts.as_deref(),
            name,
            "prompt",
            "discover prompts",
            |item| item.get("name").and_then(Value::as_str),
        ),
        _ => Ok(()),
    }
}

fn validate_cached_identifier_membership<F>(
    items: Option<&[Value]>,
    identifier: &str,
    item_kind: &str,
    refresh_command: &str,
    extract_identifier: F,
) -> Result<()>
where
    F: Fn(&Value) -> Option<&str>,
{
    let Some(items) = items else {
        return Ok(());
    };

    if items
        .iter()
        .any(|item| extract_identifier(item) == Some(identifier))
    {
        return Ok(());
    }

    let suggested = suggested_identifiers(items, identifier, &extract_identifier);
    let suggested_text = if suggested.is_empty() {
        String::new()
    } else {
        format!(" Known cached {}s: {}.", item_kind, suggested.join(", "))
    };

    Err(anyhow!(
        "cached discovery inventory does not contain {} '{}'; refresh the cache with '{}' if server inventory changed.{}",
        item_kind,
        identifier,
        refresh_command,
        suggested_text,
    ))
}

fn suggested_identifiers<F>(
    items: &[Value],
    identifier: &str,
    extract_identifier: &F,
) -> Vec<String>
where
    F: Fn(&Value) -> Option<&str>,
{
    let needle = identifier.to_ascii_lowercase();

    let mut likely = items
        .iter()
        .filter_map(extract_identifier)
        .filter(|candidate| {
            let candidate_lower = candidate.to_ascii_lowercase();
            candidate_lower.starts_with(&needle)
                || candidate_lower.contains(&needle)
                || needle.starts_with(&candidate_lower)
        })
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    likely.sort();
    likely.dedup();
    if !likely.is_empty() {
        likely.truncate(3);
        return likely;
    }

    let mut fallback = items
        .iter()
        .filter_map(extract_identifier)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    fallback.sort();
    fallback.dedup();
    fallback.truncate(3);
    fallback
}

fn negotiated_capability_group_count(
    capabilities: &crate::mcp::protocol::ServerCapabilities,
) -> usize {
    [
        capabilities.logging.is_some(),
        capabilities.completions.is_some(),
        capabilities.prompts.is_some(),
        capabilities.resources.is_some(),
        capabilities.tools.is_some(),
        !capabilities.experimental.is_empty(),
    ]
    .into_iter()
    .filter(|value| *value)
    .count()
}

fn merge_json_object(target: &mut Map<String, Value>, source: Map<String, Value>) {
    for (key, value) in source {
        match (target.get_mut(&key), value) {
            (Some(Value::Object(existing)), Value::Object(incoming)) => {
                merge_json_object(existing, incoming);
            }
            (_, replacement) => {
                target.insert(key, replacement);
            }
        }
    }
}

fn split_assignment<'a>(value: &'a str, flag: &str) -> Result<(&'a str, &'a str)> {
    let (key, raw) = value
        .split_once('=')
        .ok_or_else(|| anyhow!("invalid {} '{}'; expected KEY=VALUE", flag, value))?;
    if key.trim().is_empty() {
        return Err(anyhow!("argument keys must not be empty"));
    }
    Ok((key, raw))
}

fn parse_json_object(raw: &str, flag: &str) -> Result<Map<String, Value>> {
    let parsed: Value = serde_json::from_str(raw)
        .map_err(|error| anyhow!("invalid JSON object for {}: {}", flag, error))?;
    let Value::Object(object) = parsed else {
        return Err(anyhow!("{} expects a JSON object", flag));
    };
    Ok(object)
}

fn parse_json_object_file(path: &Path, flag: &str) -> Result<Map<String, Value>> {
    let raw = std::fs::read_to_string(path).map_err(|error| {
        anyhow!(
            "failed to read JSON object for {} '{}': {}",
            flag,
            path.display(),
            error
        )
    })?;
    parse_json_object(&raw, flag)
}

fn insert_nested_value(object: &mut Map<String, Value>, key: &str, value: Value) -> Result<()> {
    let segments = key.split('.').collect::<Vec<_>>();
    if segments.iter().any(|segment| segment.trim().is_empty()) {
        return Err(anyhow!("argument path '{}' contains an empty segment", key));
    }

    insert_nested_segments(object, &segments, value)
}

fn insert_nested_segments(
    object: &mut Map<String, Value>,
    segments: &[&str],
    value: Value,
) -> Result<()> {
    if segments.len() == 1 {
        object.insert(segments[0].to_owned(), value);
        return Ok(());
    }

    let head = segments[0];
    let entry = object
        .entry(head.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(child) = entry else {
        return Err(anyhow!(
            "cannot assign nested value into non-object argument path '{}'",
            head
        ));
    };

    insert_nested_segments(child, &segments[1..], value)
}

fn parse_scalar_value(value: &str) -> Value {
    if value.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if value.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if value.eq_ignore_ascii_case("null") {
        return Value::Null;
    }
    if let Ok(integer) = value.parse::<i64>() {
        return Value::Number(integer.into());
    }
    if let Ok(float) = value.parse::<f64>()
        && let Some(number) = Number::from_f64(float)
    {
        return Value::Number(number);
    }
    Value::String(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::protocol::{PeerInfo, ServerCapabilities};
    use chrono::Utc;

    fn negotiated_view(capabilities: ServerCapabilities) -> NegotiatedCapabilityView {
        NegotiatedCapabilityView {
            config_name: "work".to_owned(),
            app_id: "bridge".to_owned(),
            protocol_version: "2025-03-26".to_owned(),
            session_id: None,
            server_info: Some(PeerInfo {
                name: "cached-server".to_owned(),
                version: "1.0.0".to_owned(),
            }),
            server_capabilities: capabilities,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn builds_invoke_arguments_from_scalar_pairs() {
        let parsed = build_structured_arguments(
            &[
                "message=hello".to_owned(),
                "count=3".to_owned(),
                "enabled=true".to_owned(),
            ],
            &[],
            None,
            None,
        )
        .expect("arguments should parse");

        assert_eq!(
            parsed,
            json!({
                "message": "hello",
                "count": 3,
                "enabled": true,
            })
        );
    }

    #[test]
    fn builds_invoke_arguments_with_nested_and_json_merges() {
        let parsed = build_structured_arguments(
            &["request.mode=fast".to_owned()],
            &["request.tags=[\"alpha\",\"beta\"]".to_owned()],
            Some("{\"request\":{\"id\":1}}"),
            None,
        )
        .expect("arguments should parse");

        assert_eq!(
            parsed,
            json!({
                "request": {
                    "id": 1,
                    "mode": "fast",
                    "tags": ["alpha", "beta"]
                }
            })
        );
    }

    #[test]
    fn rejects_non_object_args_json() {
        let error = build_structured_arguments(&[], &[], Some("[1,2,3]"), None)
            .expect_err("non-object payload should fail");

        assert!(
            error
                .to_string()
                .contains("--args-json expects a JSON object")
        );
    }

    #[test]
    fn merges_args_file_with_other_argument_forms() {
        let temp = tempfile::NamedTempFile::new().expect("temp file should exist");
        std::fs::write(
            temp.path(),
            "{\"request\":{\"id\":1,\"priority\":\"low\"},\"trace\":false}",
        )
        .expect("temp file should be written");

        let parsed = build_structured_arguments(
            &["request.priority=high".to_owned()],
            &["request.tags=[\"alpha\"]".to_owned()],
            Some("{\"trace\":true}"),
            Some(temp.path()),
        )
        .expect("arguments should parse");

        assert_eq!(
            parsed,
            json!({
                "request": {
                    "id": 1,
                    "priority": "high",
                    "tags": ["alpha"]
                },
                "trace": true
            })
        );
    }

    #[test]
    fn maps_discover_command_to_domain_action() {
        let command = BridgeCommand::Discover(DiscoverArgs {
            command: DiscoverCommand::Capabilities,
        });

        assert_eq!(
            map_command(&command).expect("command should map"),
            BridgeDomainCommand::Discover {
                category: DiscoveryCategory::Capabilities,
            }
        );
    }

    #[test]
    fn maps_read_command_to_domain_action() {
        let command = BridgeCommand::Read(ReadArgs {
            uri: "resources/files/readme.txt".to_owned(),
        });

        assert_eq!(
            map_command(&command).expect("command should map"),
            BridgeDomainCommand::Read {
                uri: "resources/files/readme.txt".to_owned(),
            }
        );
    }

    #[test]
    fn maps_prompt_command_to_domain_action() {
        let command = BridgeCommand::Prompt(PromptCmdArgs {
            command: PromptCommand::Run(PromptRunArgs {
                name: "drafts.reply".to_owned(),
                args: vec!["context.thread_id=123".to_owned()],
                json_args: Vec::new(),
                args_json: None,
                args_file: None,
            }),
        });

        assert_eq!(
            map_command(&command).expect("command should map"),
            BridgeDomainCommand::Prompt {
                name: "drafts.reply".to_owned(),
                arguments: json!({
                    "context": {
                        "thread_id": 123
                    }
                }),
            }
        );
    }

    #[test]
    fn cached_capability_validation_allows_supported_tool_command() {
        let view = negotiated_view(ServerCapabilities {
            tools: Some(crate::mcp::protocol::ListCapability::default()),
            ..ServerCapabilities::default()
        });

        let result = validate_cached_capability_support(
            &view,
            &BridgeDomainCommand::Invoke {
                capability: "echo".to_owned(),
                arguments: json!({ "message": "hello" }),
                background: false,
            },
        );

        assert!(result.is_ok());
    }

    #[test]
    fn cached_capability_validation_rejects_missing_prompt_support() {
        let view = negotiated_view(ServerCapabilities::default());

        let error = validate_cached_capability_support(
            &view,
            &BridgeDomainCommand::Prompt {
                name: "simple-prompt".to_owned(),
                arguments: json!({}),
            },
        )
        .expect_err("prompt should be rejected when cache says prompts are unsupported");

        assert!(
            error
                .to_string()
                .contains("does not advertise prompt support")
        );
        assert!(error.to_string().contains("discover prompts"));
    }

    #[test]
    fn cached_inventory_validation_rejects_unknown_prompt_name() {
        let inventory = DiscoveryInventoryView {
            config_name: "work".to_owned(),
            app_id: "bridge".to_owned(),
            tools: None,
            resources: None,
            resource_templates: None,
            prompts: Some(vec![json!({ "name": "simple-prompt" })]),
            updated_at: Utc::now(),
        };

        let error = validate_cached_inventory_support(
            Some(&inventory),
            &BridgeDomainCommand::Prompt {
                name: "missing-prompt".to_owned(),
                arguments: json!({}),
            },
        )
        .expect_err("unknown cached prompt should be rejected");

        assert!(
            error
                .to_string()
                .contains("cached discovery inventory does not contain prompt 'missing-prompt'")
        );
        assert!(error.to_string().contains("discover prompts"));
        assert!(
            error
                .to_string()
                .contains("Known cached prompts: simple-prompt.")
        );
    }

    #[test]
    fn cached_inventory_validation_allows_known_resource_uri() {
        let inventory = DiscoveryInventoryView {
            config_name: "work".to_owned(),
            app_id: "bridge".to_owned(),
            tools: None,
            resources: Some(vec![json!({ "uri": "demo://resource/file.md" })]),
            resource_templates: None,
            prompts: None,
            updated_at: Utc::now(),
        };

        let result = validate_cached_inventory_support(
            Some(&inventory),
            &BridgeDomainCommand::Read {
                uri: "demo://resource/file.md".to_owned(),
            },
        );

        assert!(result.is_ok());
    }

    #[test]
    fn cached_descriptor_lines_include_title_and_description() {
        let lines = cached_descriptor_lines(Some(&json!({
            "title": "Echo Tool",
            "description": "Echoes back the input string"
        })));

        assert_eq!(
            lines,
            vec![
                "title: Echo Tool".to_owned(),
                "description: Echoes back the input string".to_owned(),
            ]
        );
    }

    #[test]
    fn prompt_output_lines_include_cached_descriptor_metadata() {
        let lines = prompt_output_lines(
            "simple-prompt",
            "hello world",
            &json!({}),
            Some(&json!({
                "title": "Simple Prompt",
                "description": "A prompt with no arguments"
            })),
        );

        assert_eq!(
            lines,
            vec![
                "prompt: simple-prompt".to_owned(),
                "title: Simple Prompt".to_owned(),
                "description: A prompt with no arguments".to_owned(),
                "output:".to_owned(),
                "  hello world".to_owned(),
            ]
        );
    }

    #[test]
    fn suggested_identifiers_prefers_partial_matches() {
        let suggestions = suggested_identifiers(
            &[
                json!({ "name": "simple-prompt" }),
                json!({ "name": "args-prompt" }),
                json!({ "name": "resource-prompt" }),
            ],
            "prompt",
            &|item| item.get("name").and_then(Value::as_str),
        );

        assert_eq!(
            suggestions,
            vec![
                "args-prompt".to_owned(),
                "resource-prompt".to_owned(),
                "simple-prompt".to_owned(),
            ]
        );
    }

    #[test]
    fn parses_list_selector_with_prefixed_filter() {
        let parsed = parse_list_selector("resources.files").expect("selector should parse");

        assert_eq!(
            parsed,
            ParsedListSelector {
                category: DiscoveryCategory::Resources,
                filter: Some("files".to_owned()),
            }
        );
    }

    #[test]
    fn rejects_invalid_list_selector_prefix() {
        let error = parse_list_selector("tasks.run").expect_err("selector should fail");

        assert!(
            error
                .to_string()
                .contains("invalid list selector 'tasks.run'")
        );
    }

    #[test]
    fn filter_list_items_matches_resource_selector_terms() {
        let filtered = filter_list_items(
            &DiscoveryCategory::Resources,
            &[
                json!({
                    "uri": "resources/files/readme.txt",
                    "name": "readme.txt",
                    "description": "Demo text resource"
                }),
                json!({
                    "uri": "demo://resource/static/document/architecture.md",
                    "name": "architecture.md",
                    "description": "Architecture document"
                }),
            ],
            Some("files"),
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].get("name"), Some(&json!("readme.txt")));
    }

    #[test]
    fn paginate_list_items_returns_next_cursor() {
        let (items, next_cursor) = paginate_list_items(
            vec![json!({"id": 1}), json!({"id": 2}), json!({"id": 3})],
            Some(2),
            None,
        )
        .expect("pagination should succeed");

        assert_eq!(items.len(), 2);
        assert_eq!(next_cursor.as_deref(), Some("2"));
    }

    #[test]
    fn cached_discovery_output_from_inventory_returns_stale_result() {
        let inventory = DiscoveryInventoryView {
            config_name: "work".to_owned(),
            app_id: "bridge".to_owned(),
            tools: None,
            resources: Some(vec![json!({
                "uri": "demo://resource/file.md",
                "mime_type": "text/markdown",
                "description": "Example resource"
            })]),
            resource_templates: None,
            prompts: None,
            updated_at: Utc::now(),
        };

        let output = cached_discovery_output_from_inventory(
            "work",
            &DiscoveryCategory::Resources,
            &inventory,
            "streamable HTTP request failed",
        )
        .expect("cached discovery output should be available");

        assert_eq!(
            output.summary,
            "returned 1 cached resources because live discovery failed"
        );
        assert!(
            output
                .lines
                .iter()
                .any(|line| line.contains("source: cached inventory from"))
        );
        assert!(
            output
                .lines
                .iter()
                .any(|line| line == "live_error: streamable HTTP request failed")
        );
        assert_eq!(output.data.get("cached"), Some(&json!(true)));
    }

    #[test]
    fn cached_discovery_output_from_inventory_returns_none_without_category_items() {
        let inventory = DiscoveryInventoryView {
            config_name: "work".to_owned(),
            app_id: "bridge".to_owned(),
            tools: None,
            resources: None,
            resource_templates: None,
            prompts: None,
            updated_at: Utc::now(),
        };

        let output = cached_discovery_output_from_inventory(
            "work",
            &DiscoveryCategory::Capabilities,
            &inventory,
            "transport failed",
        );

        assert!(output.is_none());
    }

    #[test]
    fn discovery_output_lines_reads_normalized_resource_mime_type() {
        let lines = discovery_output_lines(
            &DiscoveryCategory::Resources,
            &[json!({
                "uri": "demo://resource/file.md",
                "mime_type": "text/markdown",
                "description": "Example resource"
            })],
        );

        assert_eq!(
            lines,
            vec!["demo://resource/file.md  text/markdown  Example resource".to_owned()]
        );
    }

    #[test]
    fn renders_resource_data_as_pretty_json_lines() {
        let lines = resource_output_lines(
            "resources/files/catalog.json",
            Some("application/json"),
            None,
            &json!({ "items": [{ "id": 1 }] }),
            None,
        );

        assert!(lines.iter().any(|line| line == "data:"));
        assert!(lines.iter().any(|line| line.contains("\"items\"")));
    }

    #[test]
    fn maps_invoke_command_to_domain_action() {
        let command = BridgeCommand::Invoke(InvokeArgs {
            capability: "tools.echo".to_owned(),
            args: vec!["message=hello".to_owned()],
            json_args: Vec::new(),
            args_json: None,
            args_file: None,
            background: true,
        });

        assert_eq!(
            map_command(&command).expect("command should map"),
            BridgeDomainCommand::Invoke {
                capability: "tools.echo".to_owned(),
                arguments: json!({ "message": "hello" }),
                background: true,
            }
        );
    }

    #[test]
    fn parses_list_with_config_name_as_bin() {
        let argv = vec![
            OsString::from("work"),
            OsString::from("list"),
            OsString::from("--capability"),
            OsString::from("resources.files"),
        ];

        let cli = parse_bridge_cli(&argv, "work").expect("bridge cli should parse");

        match cli.command {
            BridgeCommand::List(_) => {}
            _ => panic!("expected list command"),
        }
    }

    #[test]
    fn detects_version_requests() {
        assert!(requests_version(&[
            OsString::from("mcp2cli"),
            OsString::from("--version")
        ]));
        assert!(requests_version(&[
            OsString::from("work"),
            OsString::from("-V")
        ]));
        assert!(!requests_version(&[
            OsString::from("mcp2cli"),
            OsString::from("invoke"),
            OsString::from("--capability"),
            OsString::from("tools.echo")
        ]));
    }

    #[test]
    fn maps_jobs_watch_command() {
        let command = BridgeCommand::Jobs(JobsArgs {
            command: JobsCommand::Watch(JobSelectorArgs {
                job_id: Some("job-789".to_owned()),
                latest: false,
                command: None,
            }),
        });

        assert_eq!(
            map_command(&command).expect("command should map"),
            BridgeDomainCommand::JobsWatch {
                selector: JobSelector {
                    job_id: Some("job-789".to_owned()),
                    latest: false,
                    command: None,
                },
            }
        );
    }
}
