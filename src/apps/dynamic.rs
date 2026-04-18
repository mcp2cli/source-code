//! Dynamic CLI surface: generates clap commands from a CommandManifest and
//! routes parsed commands back to MCP operations.
//!
//! This module replaces the static `BridgeCli` parser when a discovery cache
//! is available, turning server capabilities into a domain-native CLI.

use std::ffi::OsString;

use anyhow::{Result, anyhow};
use clap::Arg;
use serde_json::{Map, Value, json};

use crate::{
    apps::{
        AppContext,
        manifest::{
            CommandKind, CommandManifest, FlagSpec, FlagType, ManifestCommand, ManifestEntry,
        },
    },
    mcp::model::{DiscoveryCategory, McpOperation, McpOperationResult},
    output::{CommandOutput, ExecutionReport, OutputFormat},
    runtime::RuntimeEvent,
};

// Runtime-owned command names that always exist (reserved).
const RUNTIME_COMMANDS: &[&str] = &[
    "auth",
    "jobs",
    "doctor",
    "inspect",
    "ls",
    "ping",
    "log",
    "complete",
    "subscribe",
    "unsubscribe",
];

// ---------------------------------------------------------------------------
// Dynamic CLI builder
// ---------------------------------------------------------------------------

/// Build a clap::Command from a manifest + runtime commands.
pub fn build_dynamic_cli(
    invoked_as: &str,
    manifest: &CommandManifest,
    server_display: &str,
) -> clap::Command {
    let mut app = clap::Command::new(invoked_as.to_owned())
        .version(env!("CARGO_PKG_VERSION"))
        .about(format!("{} — powered by mcp2cli", server_display))
        .disable_help_subcommand(true)
        .arg_required_else_help(true)
        .subcommand_required(true)
        .arg(
            Arg::new("json")
                .long("json")
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help("Output in JSON format"),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .global(true)
                .value_parser(["human", "json", "ndjson"])
                .help("Output format"),
        )
        .arg(
            Arg::new("non-interactive")
                .long("non-interactive")
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help("Fail instead of prompting (CI mode)"),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .global(true)
                .value_parser(clap::value_parser!(u64))
                .value_name("SECONDS")
                .help("Operation timeout in seconds (0 = no timeout)"),
        );

    // Add manifest-derived commands
    for (name, entry) in &manifest.commands {
        if RUNTIME_COMMANDS.contains(&name.as_str()) {
            // Skip — runtime commands take precedence and are added below
            continue;
        }
        match entry {
            ManifestEntry::Command(cmd) => {
                app = app.subcommand(build_leaf_command(name, cmd));
            }
            ManifestEntry::Group { summary, children } => {
                let mut group = clap::Command::new(name.clone())
                    .about(summary.clone())
                    .subcommand_required(true)
                    .arg_required_else_help(true);
                for (child_name, child_cmd) in children {
                    group = group.subcommand(build_leaf_command(child_name, child_cmd));
                }
                app = app.subcommand(group);
            }
        }
    }

    // Runtime-owned commands
    app = app
        .subcommand(
            clap::Command::new("auth")
                .about("Authentication management")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(clap::Command::new("login").about("Authenticate with the server"))
                .subcommand(clap::Command::new("logout").about("Clear stored credentials"))
                .subcommand(clap::Command::new("status").about("Show current auth state")),
        )
        .subcommand(
            clap::Command::new("jobs")
                .about("Background job management")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(clap::Command::new("list").about("List background jobs"))
                .subcommand(
                    clap::Command::new("show")
                        .about("Show job details")
                        .arg(Arg::new("job_id").help("Job ID"))
                        .arg(
                            Arg::new("latest")
                                .long("latest")
                                .action(clap::ArgAction::SetTrue)
                                .help("Show latest job"),
                        ),
                )
                .subcommand(
                    clap::Command::new("wait")
                        .about("Wait for job completion")
                        .arg(Arg::new("job_id").help("Job ID"))
                        .arg(
                            Arg::new("latest")
                                .long("latest")
                                .action(clap::ArgAction::SetTrue)
                                .help("Wait for latest job"),
                        ),
                )
                .subcommand(
                    clap::Command::new("cancel")
                        .about("Cancel a running job")
                        .arg(Arg::new("job_id").help("Job ID"))
                        .arg(
                            Arg::new("latest")
                                .long("latest")
                                .action(clap::ArgAction::SetTrue)
                                .help("Cancel latest job"),
                        ),
                )
                .subcommand(
                    clap::Command::new("watch")
                        .about("Watch job progress")
                        .arg(Arg::new("job_id").help("Job ID"))
                        .arg(
                            Arg::new("latest")
                                .long("latest")
                                .action(clap::ArgAction::SetTrue)
                                .help("Watch latest job"),
                        ),
                ),
        )
        .subcommand(clap::Command::new("doctor").about("Runtime health diagnostics"))
        .subcommand(clap::Command::new("inspect").about("Full server metadata and capabilities"))
        .subcommand(
            clap::Command::new("ls")
                .about("List all capabilities")
                .arg(
                    Arg::new("tools")
                        .long("tools")
                        .action(clap::ArgAction::SetTrue)
                        .help("Show only tools"),
                )
                .arg(
                    Arg::new("resources")
                        .long("resources")
                        .action(clap::ArgAction::SetTrue)
                        .help("Show only resources"),
                )
                .arg(
                    Arg::new("prompts")
                        .long("prompts")
                        .action(clap::ArgAction::SetTrue)
                        .help("Show only prompts"),
                )
                .arg(
                    Arg::new("filter")
                        .long("filter")
                        .value_name("PATTERN")
                        .help("Filter by name substring"),
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .action(clap::ArgAction::SetTrue)
                        .help("Show all items without pagination"),
                ),
        )
        .subcommand(clap::Command::new("ping").about("Check if the server is alive"))
        .subcommand(
            clap::Command::new("log")
                .about("Set server logging level")
                .arg(
                    Arg::new("level")
                        .required(true)
                        .value_parser([
                            "debug",
                            "info",
                            "notice",
                            "warning",
                            "error",
                            "critical",
                            "alert",
                            "emergency",
                        ])
                        .help("Logging level to set"),
                ),
        )
        .subcommand(
            clap::Command::new("complete")
                .about("Request tab-completions from the server")
                .arg(
                    Arg::new("ref_kind")
                        .required(true)
                        .value_parser(["prompt", "resource"])
                        .help("What to complete: 'prompt' or 'resource'"),
                )
                .arg(
                    Arg::new("name")
                        .required(true)
                        .help("Prompt or resource name"),
                )
                .arg(
                    Arg::new("argument")
                        .required(true)
                        .help("Argument to complete"),
                )
                .arg(
                    Arg::new("value")
                        .default_value("")
                        .help("Partial value to complete"),
                ),
        )
        .subcommand(
            clap::Command::new("subscribe")
                .about("Subscribe to resource change notifications")
                .arg(
                    Arg::new("uri")
                        .required(true)
                        .help("Resource URI to subscribe to"),
                ),
        )
        .subcommand(
            clap::Command::new("unsubscribe")
                .about("Unsubscribe from resource change notifications")
                .arg(
                    Arg::new("uri")
                        .required(true)
                        .help("Resource URI to unsubscribe from"),
                ),
        );

    // Legacy hidden aliases for backward compatibility
    app = app
        .subcommand(
            clap::Command::new("tool")
                .hide(true)
                .subcommand_required(true)
                .subcommand(
                    clap::Command::new("list")
                        .arg(Arg::new("filter").long("filter"))
                        .arg(
                            Arg::new("limit")
                                .long("limit")
                                .value_parser(clap::value_parser!(u32)),
                        )
                        .arg(Arg::new("cursor").long("cursor"))
                        .arg(Arg::new("all").long("all").action(clap::ArgAction::SetTrue)),
                )
                .subcommand(
                    clap::Command::new("call")
                        .arg(Arg::new("name").required(true))
                        .arg(Arg::new("arg").long("arg").action(clap::ArgAction::Append))
                        .arg(
                            Arg::new("arg-json")
                                .long("arg-json")
                                .action(clap::ArgAction::Append),
                        )
                        .arg(Arg::new("args-json").long("args-json"))
                        .arg(Arg::new("args-file").long("args-file"))
                        .arg(
                            Arg::new("background")
                                .long("background")
                                .action(clap::ArgAction::SetTrue),
                        ),
                ),
        )
        .subcommand(
            clap::Command::new("resource")
                .hide(true)
                .subcommand_required(true)
                .subcommand(
                    clap::Command::new("list")
                        .arg(Arg::new("filter").long("filter"))
                        .arg(
                            Arg::new("limit")
                                .long("limit")
                                .value_parser(clap::value_parser!(u32)),
                        )
                        .arg(Arg::new("cursor").long("cursor"))
                        .arg(Arg::new("all").long("all").action(clap::ArgAction::SetTrue)),
                )
                .subcommand(clap::Command::new("read").arg(Arg::new("uri").required(true))),
        )
        .subcommand(
            clap::Command::new("prompt")
                .hide(true)
                .subcommand_required(true)
                .subcommand(
                    clap::Command::new("list")
                        .arg(Arg::new("filter").long("filter"))
                        .arg(
                            Arg::new("limit")
                                .long("limit")
                                .value_parser(clap::value_parser!(u32)),
                        )
                        .arg(Arg::new("cursor").long("cursor"))
                        .arg(Arg::new("all").long("all").action(clap::ArgAction::SetTrue)),
                )
                .subcommand(
                    clap::Command::new("run")
                        .arg(Arg::new("name").required(true))
                        .arg(Arg::new("arg").long("arg").action(clap::ArgAction::Append))
                        .arg(
                            Arg::new("arg-json")
                                .long("arg-json")
                                .action(clap::ArgAction::Append),
                        )
                        .arg(Arg::new("args-json").long("args-json"))
                        .arg(Arg::new("args-file").long("args-file")),
                ),
        );

    app
}

/// Build a clap subcommand for a single leaf manifest command.
fn build_leaf_command(name: &str, cmd: &ManifestCommand) -> clap::Command {
    let mut sub = clap::Command::new(name.to_owned()).about(cmd.summary.clone());

    // Add positional argument if specified
    if let Some(pos) = &cmd.positional {
        let mut arg = Arg::new(&pos.name).required(pos.required);
        if let Some(help) = &pos.help {
            arg = arg.help(help.clone());
        }
        sub = sub.arg(arg);
    }

    // Add typed flags from schema
    for (flag_name, spec) in &cmd.flags {
        // Skip flags that are covered by a positional with the same name
        if cmd
            .positional
            .as_ref()
            .map(|p| p.name == *flag_name)
            .unwrap_or(false)
        {
            continue;
        }
        sub = sub.arg(build_flag_arg(flag_name, spec));
    }

    // Add generic fallback flags (always available)
    sub = sub
        .arg(
            Arg::new("arg")
                .long("arg")
                .value_name("KEY=VALUE")
                .action(clap::ArgAction::Append)
                .help("Additional argument (key=value)"),
        )
        .arg(
            Arg::new("arg-json")
                .long("arg-json")
                .value_name("KEY=JSON")
                .action(clap::ArgAction::Append)
                .help("Additional argument with JSON value"),
        )
        .arg(
            Arg::new("args-json")
                .long("args-json")
                .value_name("JSON_OBJECT")
                .help("Arguments as JSON object string"),
        )
        .arg(
            Arg::new("args-file")
                .long("args-file")
                .value_name("PATH")
                .help("Arguments from JSON file"),
        );

    // Add --background for tools
    if cmd.supports_background {
        sub = sub.arg(
            Arg::new("background")
                .long("background")
                .action(clap::ArgAction::SetTrue)
                .help("Run as background job"),
        );
    }

    sub
}

/// Build a single clap Arg from a FlagSpec.
fn build_flag_arg(name: &str, spec: &FlagSpec) -> Arg {
    let mut arg = Arg::new(name.to_owned()).long(name.to_owned());
    if spec.required {
        arg = arg.required(true);
    }

    if spec.flag_type == FlagType::Boolean {
        arg = arg.action(clap::ArgAction::SetTrue);
    } else {
        let vn = spec.flag_type.value_name();
        if !vn.is_empty() {
            arg = arg.value_name(vn);
        }
    }

    if let Some(help) = &spec.help {
        arg = arg.help(help.to_owned());
    }

    if let Some(default) = &spec.default {
        let default_str = match default {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        arg = arg.default_value(default_str);
    }

    if let Some(values) = &spec.enum_values {
        arg = arg.value_parser(values.clone());
    }

    arg
}

// ---------------------------------------------------------------------------
// Parsed dynamic command → domain routing
// ---------------------------------------------------------------------------

/// A parsed dynamic command ready for MCP dispatch.
#[derive(Debug, Clone)]
pub enum DynamicCommand {
    /// A manifest-backed command (tool, resource, prompt, template).
    Manifest {
        cmd: ManifestCommand,
        arguments: Value,
        background: bool,
    },
    /// `get <URI>` — resource read.
    ResourceGet {
        uri: String,
    },
    /// `ls` — list capabilities.
    Ls {
        tools: bool,
        resources: bool,
        prompts: bool,
        filter: Option<String>,
    },
    /// Runtime: auth login/logout/status.
    AuthLogin,
    AuthLogout,
    AuthStatus,
    /// Runtime: job management.
    JobsList,
    JobsShow {
        job_id: Option<String>,
        latest: bool,
    },
    JobsWait {
        job_id: Option<String>,
        latest: bool,
    },
    JobsCancel {
        job_id: Option<String>,
        latest: bool,
    },
    JobsWatch {
        job_id: Option<String>,
        latest: bool,
    },
    /// Runtime: doctor/inspect.
    Doctor,
    Inspect,
    /// Ping server liveness.
    Ping,
    /// Set logging level.
    Log {
        level: String,
    },
    /// Request completions.
    Complete {
        ref_kind: String,
        name: String,
        argument: String,
        value: String,
    },
    /// Subscribe to resource change notifications.
    Subscribe {
        uri: String,
    },
    /// Unsubscribe from resource change notifications.
    Unsubscribe {
        uri: String,
    },
    /// Legacy pass-through to old bridge.
    LegacyBridge,
}

/// Parsed result from the dynamic CLI surface.
pub struct DynamicParseResult {
    pub command: DynamicCommand,
    pub output_format: OutputFormat,
    pub timeout: Option<u64>,
}

/// Parse argv against the dynamic CLI, returning a DynamicCommand.
pub fn parse_dynamic(
    argv: &[OsString],
    invoked_as: &str,
    manifest: &CommandManifest,
    server_display: &str,
) -> Result<DynamicParseResult> {
    let app = build_dynamic_cli(invoked_as, manifest, server_display);
    let matches = app.try_get_matches_from(argv.to_vec()).map_err(|e| {
        if matches!(
            e.kind(),
            clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                | clap::error::ErrorKind::DisplayVersion
        ) {
            anyhow!("{}", e)
        } else {
            // Try fuzzy suggestion for unknown subcommands
            let err_msg = e.to_string();
            if let Some(attempted) = extract_attempted_subcommand(&err_msg) {
                let suggestions = super::manifest::fuzzy_suggest(&attempted, manifest);
                if !suggestions.is_empty() {
                    let hint = suggestions.join(", ");
                    return anyhow!("{}\n\nDid you mean: {}?", err_msg.trim(), hint);
                }
            }
            e.into()
        }
    })?;

    let output_format = if matches.get_flag("json") {
        OutputFormat::Json
    } else {
        match matches.get_one::<String>("output").map(String::as_str) {
            Some("json") => OutputFormat::Json,
            Some("ndjson") => OutputFormat::Ndjson,
            _ => OutputFormat::Human,
        }
    };
    let timeout = matches.get_one::<u64>("timeout").copied();

    let (sub_name, sub_matches) = matches
        .subcommand()
        .ok_or_else(|| anyhow!("no subcommand"))?;

    let cmd = match sub_name {
        // Runtime commands
        "auth" => {
            let (auth_sub, _) = sub_matches
                .subcommand()
                .ok_or_else(|| anyhow!("auth requires a subcommand"))?;
            match auth_sub {
                "login" => DynamicCommand::AuthLogin,
                "logout" => DynamicCommand::AuthLogout,
                "status" => DynamicCommand::AuthStatus,
                other => return Err(anyhow!("unknown auth subcommand: {}", other)),
            }
        }
        "jobs" => {
            let (jobs_sub, jobs_matches) = sub_matches
                .subcommand()
                .ok_or_else(|| anyhow!("jobs requires a subcommand"))?;
            let job_id = jobs_matches.get_one::<String>("job_id").cloned();
            let latest = jobs_matches.get_flag("latest");
            match jobs_sub {
                "list" => DynamicCommand::JobsList,
                "show" => DynamicCommand::JobsShow { job_id, latest },
                "wait" => DynamicCommand::JobsWait { job_id, latest },
                "cancel" => DynamicCommand::JobsCancel { job_id, latest },
                "watch" => DynamicCommand::JobsWatch { job_id, latest },
                other => return Err(anyhow!("unknown jobs subcommand: {}", other)),
            }
        }
        "doctor" => DynamicCommand::Doctor,
        "inspect" => DynamicCommand::Inspect,
        "ping" => DynamicCommand::Ping,
        "log" => {
            let level = sub_matches
                .get_one::<String>("level")
                .ok_or_else(|| anyhow!("log requires a level"))?
                .clone();
            DynamicCommand::Log { level }
        }
        "complete" => {
            let ref_kind = sub_matches
                .get_one::<String>("ref_kind")
                .ok_or_else(|| anyhow!("complete requires ref_kind"))?
                .clone();
            let name = sub_matches
                .get_one::<String>("name")
                .ok_or_else(|| anyhow!("complete requires name"))?
                .clone();
            let argument = sub_matches
                .get_one::<String>("argument")
                .ok_or_else(|| anyhow!("complete requires argument"))?
                .clone();
            let value = sub_matches
                .get_one::<String>("value")
                .cloned()
                .unwrap_or_default();
            DynamicCommand::Complete {
                ref_kind: format!("ref/{}", ref_kind),
                name,
                argument,
                value,
            }
        }
        "ls" => DynamicCommand::Ls {
            tools: sub_matches.get_flag("tools"),
            resources: sub_matches.get_flag("resources"),
            prompts: sub_matches.get_flag("prompts"),
            filter: sub_matches.get_one::<String>("filter").cloned(),
        },
        "subscribe" => {
            let uri = sub_matches
                .get_one::<String>("uri")
                .ok_or_else(|| anyhow!("subscribe requires a URI"))?
                .clone();
            DynamicCommand::Subscribe { uri }
        }
        "unsubscribe" => {
            let uri = sub_matches
                .get_one::<String>("uri")
                .ok_or_else(|| anyhow!("unsubscribe requires a URI"))?
                .clone();
            DynamicCommand::Unsubscribe { uri }
        }

        // Legacy hidden aliases — pass through to old bridge
        "tool" | "resource" | "prompt" => {
            return Ok(DynamicParseResult {
                command: DynamicCommand::LegacyBridge,
                output_format,
                timeout,
            });
        }

        // Manifest-derived commands
        _ => resolve_manifest_command(sub_name, sub_matches, manifest)?,
    };

    Ok(DynamicParseResult {
        command: cmd,
        output_format,
        timeout,
    })
}

/// Extract the subcommand name from a clap error message like
/// `error: unrecognized subcommand 'ecoh'`.
fn extract_attempted_subcommand(err_msg: &str) -> Option<String> {
    // Clap uses: "unrecognized subcommand 'name'" or similar
    if let Some(start) = err_msg.find('\'') {
        let rest = &err_msg[start + 1..];
        if let Some(end) = rest.find('\'') {
            return Some(rest[..end].to_owned());
        }
    }
    None
}

/// Resolve a subcommand match against the manifest.
fn resolve_manifest_command(
    name: &str,
    matches: &clap::ArgMatches,
    manifest: &CommandManifest,
) -> Result<DynamicCommand> {
    let entry = manifest
        .commands
        .get(name)
        .ok_or_else(|| anyhow!("unknown command: {}", name))?;

    match entry {
        ManifestEntry::Command(cmd) => {
            // Special case: "get" command → ResourceGet
            if cmd.kind == CommandKind::Resource && cmd.origin_name == "get" {
                let uri = matches
                    .get_one::<String>("uri")
                    .ok_or_else(|| anyhow!("get requires a URI argument"))?
                    .clone();
                return Ok(DynamicCommand::ResourceGet { uri });
            }

            let arguments = extract_arguments_from_matches(matches, cmd)?;
            let background = matches
                .try_get_one::<bool>("background")
                .ok()
                .flatten()
                .copied()
                .unwrap_or(false);

            Ok(DynamicCommand::Manifest {
                cmd: cmd.clone(),
                arguments,
                background,
            })
        }
        ManifestEntry::Group { children, .. } => {
            let (child_name, child_matches) = matches
                .subcommand()
                .ok_or_else(|| anyhow!("{} requires a subcommand", name))?;
            let child_cmd = children
                .get(child_name)
                .ok_or_else(|| anyhow!("unknown subcommand: {} {}", name, child_name))?;

            let arguments = extract_arguments_from_matches(child_matches, child_cmd)?;
            let background = child_matches
                .try_get_one::<bool>("background")
                .ok()
                .flatten()
                .copied()
                .unwrap_or(false);

            Ok(DynamicCommand::Manifest {
                cmd: child_cmd.clone(),
                arguments,
                background,
            })
        }
    }
}

/// Extract arguments from clap matches into a JSON object, using the manifest
/// command's flag/positional info to map back to original MCP argument names.
fn extract_arguments_from_matches(
    matches: &clap::ArgMatches,
    cmd: &ManifestCommand,
) -> Result<Value> {
    let mut object = Map::new();

    // Process args-file first (base layer)
    if let Some(path) = matches.get_one::<String>("args-file") {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("failed to read --args-file '{}': {}", path, e))?;
        let parsed: Value = serde_json::from_str(&content)
            .map_err(|e| anyhow!("invalid JSON in --args-file '{}': {}", path, e))?;
        if let Some(obj) = parsed.as_object() {
            for (k, v) in obj {
                object.insert(k.clone(), v.clone());
            }
        }
    }

    // Process args-json overlay
    if let Some(raw) = matches.get_one::<String>("args-json") {
        let parsed: Value =
            serde_json::from_str(raw).map_err(|e| anyhow!("invalid --args-json: {}", e))?;
        if let Some(obj) = parsed.as_object() {
            for (k, v) in obj {
                object.insert(k.clone(), v.clone());
            }
        }
    }

    // Process positional argument
    if let Some(pos) = &cmd.positional
        && let Some(value) = matches.get_one::<String>(&pos.name)
    {
        // For resource templates, the positional maps to the template param
        object.insert(pos.name.clone(), Value::String(value.clone()));
    }

    // Process typed flags (from schema)
    for (flag_name, spec) in &cmd.flags {
        // Skip if already covered by positional
        if cmd
            .positional
            .as_ref()
            .map(|p| p.name == *flag_name)
            .unwrap_or(false)
        {
            continue;
        }
        // Map flag value back to original property name
        let original_name = flag_name_to_property(flag_name);
        match spec.flag_type {
            FlagType::Boolean => {
                if matches.get_flag(flag_name) {
                    object.insert(original_name, Value::Bool(true));
                }
            }
            FlagType::Integer => {
                if let Some(val) = matches.get_one::<String>(flag_name) {
                    let parsed: i64 = val
                        .parse()
                        .map_err(|_| anyhow!("--{} requires an integer value", flag_name))?;
                    object.insert(original_name, json!(parsed));
                }
            }
            FlagType::Number => {
                if let Some(val) = matches.get_one::<String>(flag_name) {
                    let parsed: f64 = val
                        .parse()
                        .map_err(|_| anyhow!("--{} requires a numeric value", flag_name))?;
                    object.insert(original_name, json!(parsed));
                }
            }
            FlagType::Array => {
                if let Some(val) = matches.get_one::<String>(flag_name) {
                    let items: Vec<Value> = val
                        .split(',')
                        .map(|s| Value::String(s.trim().to_owned()))
                        .collect();
                    object.insert(original_name, Value::Array(items));
                }
            }
            FlagType::Json => {
                if let Some(val) = matches.get_one::<String>(flag_name) {
                    let parsed: Value = serde_json::from_str(val)
                        .map_err(|e| anyhow!("--{} requires valid JSON: {}", flag_name, e))?;
                    object.insert(original_name, parsed);
                }
            }
            FlagType::String => {
                if let Some(val) = matches.get_one::<String>(flag_name) {
                    object.insert(original_name, Value::String(val.clone()));
                }
            }
        }
    }

    // Process generic --arg fallbacks
    if let Some(args) = matches.get_many::<String>("arg") {
        for arg in args {
            if let Some((key, value)) = arg.split_once('=') {
                object.insert(key.to_owned(), Value::String(value.to_owned()));
            }
        }
    }

    // Process generic --arg-json fallbacks
    if let Some(args) = matches.get_many::<String>("arg-json") {
        for arg in args {
            if let Some((key, raw)) = arg.split_once('=') {
                let parsed: Value = serde_json::from_str(raw)
                    .map_err(|e| anyhow!("invalid JSON in --arg-json '{}': {}", arg, e))?;
                object.insert(key.to_owned(), parsed);
            }
        }
    }

    Ok(Value::Object(object))
}

/// Convert a kebab-case flag name back to the original property name.
/// This is a best-effort reverse of `to_flag_name`.
fn flag_name_to_property(flag: &str) -> String {
    // We keep kebab-case as the property name — MCP properties use various
    // conventions and the server should accept the original property name.
    // The flag name IS the property name (with - instead of _ or .).
    // For tools with inputSchema, the original name is preserved in the schema.
    flag.to_owned()
}

// ---------------------------------------------------------------------------
// Execution: DynamicCommand → MCP operation → result
// ---------------------------------------------------------------------------

/// Execute a dynamic command against the MCP server.
pub async fn execute_dynamic(
    cmd: DynamicCommand,
    output_format: OutputFormat,
    context: &AppContext,
) -> Result<ExecutionReport> {
    let output = match cmd {
        DynamicCommand::Manifest {
            cmd,
            arguments,
            background,
        } => execute_manifest_command(&cmd, arguments, background, context).await?,

        DynamicCommand::ResourceGet { uri } => execute_resource_get(&uri, context).await?,

        DynamicCommand::Ls {
            tools,
            resources,
            prompts,
            filter,
        } => execute_ls(tools, resources, prompts, filter, context).await?,

        DynamicCommand::Ping => execute_ping(context).await?,

        DynamicCommand::Log { level } => execute_log(&level, context).await?,

        DynamicCommand::Complete {
            ref_kind,
            name,
            argument,
            value,
        } => execute_complete(&ref_kind, &name, &argument, &value, context).await?,

        DynamicCommand::Subscribe { uri } => execute_subscribe(&uri, context).await?,
        DynamicCommand::Unsubscribe { uri } => execute_unsubscribe(&uri, context).await?,

        DynamicCommand::JobsList => execute_jobs_list(context).await?,
        DynamicCommand::JobsShow { job_id, latest } => {
            execute_jobs_show(job_id, latest, context).await?
        }
        DynamicCommand::JobsWait { job_id, latest } => {
            execute_jobs_wait(job_id, latest, context).await?
        }
        DynamicCommand::JobsCancel { job_id, latest } => {
            execute_jobs_cancel(job_id, latest, context).await?
        }
        DynamicCommand::JobsWatch { job_id, latest } => {
            execute_jobs_watch(job_id, latest, context).await?
        }

        DynamicCommand::AuthLogin
        | DynamicCommand::AuthLogout
        | DynamicCommand::AuthStatus
        | DynamicCommand::Doctor
        | DynamicCommand::Inspect => {
            // These are handled by delegating to the existing bridge
            return Err(anyhow!("__delegate_to_bridge__"));
        }

        DynamicCommand::LegacyBridge => {
            return Err(anyhow!("__delegate_to_bridge__"));
        }
    };

    Ok(ExecutionReport {
        output_format,
        output,
    })
}

/// Execute a manifest-backed command (tool call, prompt run, or template read).
async fn execute_manifest_command(
    cmd: &ManifestCommand,
    arguments: Value,
    background: bool,
    context: &AppContext,
) -> Result<CommandOutput> {
    match cmd.kind {
        CommandKind::Tool => {
            context.services.event_broker.emit(RuntimeEvent::Info {
                app_id: context.config_name.clone(),
                message: format!("executing tool '{}'", cmd.origin_name),
            });
            let result = context
                .perform(McpOperation::InvokeAction {
                    capability: cmd.origin_name.clone(),
                    arguments,
                    background,
                })
                .await?;
            format_action_result(&cmd.origin_name, result, context).await
        }

        CommandKind::Prompt => {
            context.services.event_broker.emit(RuntimeEvent::Info {
                app_id: context.config_name.clone(),
                message: format!("executing prompt '{}'", cmd.origin_name),
            });
            let result = context
                .perform(McpOperation::RunPrompt {
                    name: cmd.origin_name.clone(),
                    arguments,
                })
                .await?;
            format_prompt_result(&cmd.origin_name, result)
        }

        CommandKind::ResourceTemplate => {
            // Materialize the URI template with the provided arguments
            let uri = materialize_uri_template(&cmd.origin_name, &arguments);
            execute_resource_get(&uri, context).await
        }

        CommandKind::Resource => {
            // Direct resource read (shouldn't normally happen — "get" is special-cased)
            let uri = arguments
                .get("uri")
                .and_then(Value::as_str)
                .unwrap_or(&cmd.origin_name);
            execute_resource_get(uri, context).await
        }
    }
}

/// Execute resource read by URI.
async fn execute_resource_get(uri: &str, context: &AppContext) -> Result<CommandOutput> {
    context.services.event_broker.emit(RuntimeEvent::Info {
        app_id: context.config_name.clone(),
        message: format!("reading resource '{}'", uri),
    });
    let result = context
        .perform(McpOperation::ReadResource {
            uri: uri.to_owned(),
        })
        .await?;
    match result {
        McpOperationResult::Resource {
            message,
            uri,
            mime_type,
            text,
            data,
        } => {
            let mut lines = vec![format!("uri: {}", uri)];
            if let Some(mt) = &mime_type {
                lines.push(format!("type: {}", mt));
            }
            if let Some(text) = &text {
                let text_lines: Vec<&str> = text.lines().collect();
                if text_lines.len() <= 1 {
                    lines.push(text.clone());
                } else {
                    for line in text_lines {
                        lines.push(line.to_owned());
                    }
                }
            } else {
                lines.push(serde_json::to_string_pretty(&data).unwrap_or_default());
            }
            Ok(CommandOutput::new(
                &context.config_name,
                "get",
                message,
                lines,
                json!({
                    "uri": uri,
                    "mime_type": mime_type,
                    "text": text,
                    "data": data,
                }),
            ))
        }
        other => Err(anyhow!(
            "unexpected response for resource read: {}",
            serde_json::to_string(&other)?
        )),
    }
}

/// Execute the `ls` command — unified listing of all capabilities.
async fn execute_ls(
    tools_only: bool,
    resources_only: bool,
    prompts_only: bool,
    filter: Option<String>,
    context: &AppContext,
) -> Result<CommandOutput> {
    // Determine which categories to refresh/show
    let show_all = !tools_only && !resources_only && !prompts_only;

    let mut lines = Vec::new();
    let mut json_data = json!({});

    // Refresh discovery from server if needed, and collect from cache
    if show_all || tools_only {
        let result = try_discover(context, DiscoveryCategory::Capabilities).await;
        if let Some(items) = result {
            let filtered = apply_filter(&items, &filter, |item| {
                item.get("id")
                    .or_else(|| item.get("name"))
                    .and_then(Value::as_str)
            });
            if !filtered.is_empty() {
                lines.push("TOOLS:".to_owned());
                for item in &filtered {
                    let name = item
                        .get("id")
                        .or_else(|| item.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("?");
                    let title = item.get("title").and_then(Value::as_str);
                    let desc = item
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if let Some(t) = title {
                        lines.push(format!("  {:<30} {} — {}", name, t, desc));
                    } else {
                        lines.push(format!("  {:<30} {}", name, desc));
                    }
                }
                lines.push(String::new());
            }
            json_data["tools"] = json!(filtered);
        }
    }

    if show_all || resources_only {
        let result = try_discover(context, DiscoveryCategory::Resources).await;
        if let Some(items) = result {
            let filtered = apply_filter(&items, &filter, |item| {
                item.get("uri")
                    .or_else(|| item.get("name"))
                    .and_then(Value::as_str)
            });
            if !filtered.is_empty() {
                lines.push("RESOURCES:".to_owned());
                for item in &filtered {
                    let uri = item.get("uri").and_then(Value::as_str).unwrap_or("?");
                    let title = item.get("title").and_then(Value::as_str);
                    let desc = item
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if let Some(t) = title {
                        lines.push(format!("  {:<30} {} — {}", uri, t, desc));
                    } else {
                        lines.push(format!("  {:<30} {}", uri, desc));
                    }
                }
                lines.push(String::new());
            }
            json_data["resources"] = json!(filtered);
        }
    }

    if show_all || prompts_only {
        let result = try_discover(context, DiscoveryCategory::Prompts).await;
        if let Some(items) = result {
            let filtered = apply_filter(&items, &filter, |item| {
                item.get("name").and_then(Value::as_str)
            });
            if !filtered.is_empty() {
                lines.push("PROMPTS:".to_owned());
                for item in &filtered {
                    let name = item.get("name").and_then(Value::as_str).unwrap_or("?");
                    let title = item.get("title").and_then(Value::as_str);
                    let desc = item
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if let Some(t) = title {
                        lines.push(format!("  {:<30} {} — {}", name, t, desc));
                    } else {
                        lines.push(format!("  {:<30} {}", name, desc));
                    }
                }
            }
            json_data["prompts"] = json!(filtered);
        }
    }

    if lines.is_empty() {
        lines.push("no capabilities discovered — try connecting to the server first".to_owned());
    }

    Ok(CommandOutput::new(
        &context.config_name,
        "ls",
        "capability listing".to_owned(),
        lines,
        json_data,
    ))
}

/// Ping the server to check liveness.
async fn execute_ping(context: &AppContext) -> Result<CommandOutput> {
    let start = std::time::Instant::now();
    let result = context.perform(McpOperation::Ping).await;
    let elapsed = start.elapsed();

    match result {
        Ok(McpOperationResult::Pong { message }) => Ok(CommandOutput::new(
            &context.config_name,
            "ping",
            message,
            vec![format!("pong ({}ms)", elapsed.as_millis())],
            json!({
                "alive": true,
                "latency_ms": elapsed.as_millis() as u64,
            }),
        )),
        Ok(_) => Ok(CommandOutput::new(
            &context.config_name,
            "ping",
            "server responded".to_owned(),
            vec![format!("pong ({}ms)", elapsed.as_millis())],
            json!({ "alive": true, "latency_ms": elapsed.as_millis() as u64 }),
        )),
        Err(e) => Ok(CommandOutput::new(
            &context.config_name,
            "ping",
            "server unreachable".to_owned(),
            vec![format!("error: {}", e)],
            json!({ "alive": false, "error": e.to_string() }),
        )),
    }
}

/// Set the server logging level.
async fn execute_log(level: &str, context: &AppContext) -> Result<CommandOutput> {
    let result = context
        .perform(McpOperation::SetLoggingLevel {
            level: level.to_owned(),
        })
        .await?;
    match result {
        McpOperationResult::LoggingLevelSet { message, level } => Ok(CommandOutput::new(
            &context.config_name,
            "log",
            message,
            vec![format!("level: {}", level)],
            json!({ "level": level }),
        )),
        other => Err(anyhow!(
            "unexpected response for logging/setLevel: {}",
            serde_json::to_string(&other)?
        )),
    }
}

/// Request tab-completions from the server.
async fn execute_complete(
    ref_kind: &str,
    name: &str,
    argument: &str,
    value: &str,
    context: &AppContext,
) -> Result<CommandOutput> {
    let result = context
        .perform(McpOperation::Complete {
            ref_kind: ref_kind.to_owned(),
            ref_name: name.to_owned(),
            argument_name: argument.to_owned(),
            argument_value: value.to_owned(),
            context: None,
        })
        .await?;
    match result {
        McpOperationResult::Completion {
            message,
            values,
            has_more,
            total,
        } => {
            let mut lines: Vec<String> = values.to_vec();
            if has_more {
                lines.push("(more results available)".to_owned());
            }
            Ok(CommandOutput::new(
                &context.config_name,
                "complete",
                message,
                lines,
                json!({
                    "values": values,
                    "has_more": has_more,
                    "total": total,
                }),
            ))
        }
        other => Err(anyhow!(
            "unexpected response for completion/complete: {}",
            serde_json::to_string(&other)?
        )),
    }
}

// ---------------------------------------------------------------------------
// Resource subscription commands (WI-09)
// ---------------------------------------------------------------------------

async fn execute_subscribe(uri: &str, context: &AppContext) -> Result<CommandOutput> {
    let result = context
        .perform(McpOperation::SubscribeResource {
            uri: uri.to_owned(),
        })
        .await?;
    match result {
        McpOperationResult::Subscribed { message, uri } => Ok(CommandOutput::new(
            &context.config_name,
            "subscribe",
            message,
            vec![format!("subscribed: {}", uri)],
            json!({ "uri": uri }),
        )),
        other => Err(anyhow!(
            "unexpected response for subscribe: {}",
            serde_json::to_string(&other)?
        )),
    }
}

async fn execute_unsubscribe(uri: &str, context: &AppContext) -> Result<CommandOutput> {
    let result = context
        .perform(McpOperation::UnsubscribeResource {
            uri: uri.to_owned(),
        })
        .await?;
    match result {
        McpOperationResult::Unsubscribed { message, uri } => Ok(CommandOutput::new(
            &context.config_name,
            "unsubscribe",
            message,
            vec![format!("unsubscribed: {}", uri)],
            json!({ "uri": uri }),
        )),
        other => Err(anyhow!(
            "unexpected response for unsubscribe: {}",
            serde_json::to_string(&other)?
        )),
    }
}

// ---------------------------------------------------------------------------
// Job/task management commands (WI-07)
// ---------------------------------------------------------------------------

async fn execute_jobs_list(context: &AppContext) -> Result<CommandOutput> {
    let jobs = context
        .services
        .state_store
        .jobs_for_config(&context.config_name)
        .await;
    if jobs.is_empty() {
        return Ok(CommandOutput::new(
            &context.config_name,
            "jobs list",
            "no jobs found".to_owned(),
            vec!["no background jobs".to_owned()],
            json!({ "jobs": [] }),
        ));
    }
    let mut lines = Vec::new();
    for job in &jobs {
        let status = job.status.as_str();
        let remote = job.remote_task_id.as_deref().unwrap_or("-");
        lines.push(format!(
            "  {:<36} {:<12} {:<12} {}",
            job.job_id, status, remote, job.command
        ));
    }
    Ok(CommandOutput::new(
        &context.config_name,
        "jobs list",
        format!("{} jobs", jobs.len()),
        lines,
        json!({ "jobs": jobs }),
    ))
}

async fn execute_jobs_show(
    job_id: Option<String>,
    latest: bool,
    context: &AppContext,
) -> Result<CommandOutput> {
    let job = resolve_job(job_id, latest, context).await?;
    // If the job has a remote_task_id, query the server for fresh status
    if let Some(remote_id) = &job.remote_task_id
        && let Ok(result) = context
            .perform(McpOperation::TaskGet {
                task_id: remote_id.clone(),
            })
            .await
        && let McpOperationResult::Task { status, data, .. } = &result
    {
        let mut lines = crate::apps::default_job_detail_lines(&job);
        lines.push(format!("remote status: {}", status.as_str()));
        return Ok(CommandOutput::new(
            &context.config_name,
            "jobs show",
            format!("job {}", job.job_id),
            lines,
            json!({ "job": job, "remote": data }),
        ));
    }
    let lines = crate::apps::default_job_detail_lines(&job);
    Ok(CommandOutput::new(
        &context.config_name,
        "jobs show",
        format!("job {}", job.job_id),
        lines,
        json!({ "job": job }),
    ))
}

async fn execute_jobs_wait(
    job_id: Option<String>,
    latest: bool,
    context: &AppContext,
) -> Result<CommandOutput> {
    let job = resolve_job(job_id, latest, context).await?;
    let Some(remote_id) = &job.remote_task_id else {
        return Ok(CommandOutput::new(
            &context.config_name,
            "jobs wait",
            format!("job {} has no remote task", job.job_id),
            vec![format!("status: {}", job.status.as_str())],
            json!({ "job": job }),
        ));
    };

    // Poll tasks/get until the task reaches a terminal state
    let poll_interval = std::time::Duration::from_secs(2);
    let max_polls = 300; // 10 min max
    for i in 0..max_polls {
        let result = context
            .perform(McpOperation::TaskGet {
                task_id: remote_id.clone(),
            })
            .await?;
        if let McpOperationResult::Task {
            status,
            remote_task_id: _,
            data,
            result: _task_result,
            failure_reason,
            ..
        } = &result
        {
            let is_terminal = matches!(
                status,
                crate::mcp::model::TaskState::Completed
                    | crate::mcp::model::TaskState::Canceled
                    | crate::mcp::model::TaskState::Failed
            );
            if is_terminal {
                // If completed, fetch the full result via tasks/result
                if matches!(status, crate::mcp::model::TaskState::Completed)
                    && let Ok(full_result) = context
                        .perform(McpOperation::TaskResult {
                            task_id: remote_id.clone(),
                        })
                        .await
                    && let McpOperationResult::Task {
                        data: full_data,
                        result: full_task_result,
                        ..
                    } = &full_result
                {
                    return Ok(CommandOutput::new(
                        &context.config_name,
                        "jobs wait",
                        format!("task {} completed", remote_id),
                        vec![
                            format!("status: {}", status.as_str()),
                            serde_json::to_string_pretty(
                                full_task_result.as_ref().unwrap_or(full_data),
                            )
                            .unwrap_or_default(),
                        ],
                        json!({ "job": job, "task": full_data, "result": full_task_result }),
                    ));
                }
                let mut lines = vec![format!("status: {}", status.as_str())];
                if let Some(reason) = failure_reason {
                    lines.push(format!("reason: {}", reason));
                }
                return Ok(CommandOutput::new(
                    &context.config_name,
                    "jobs wait",
                    format!("task {} {}", remote_id, status.as_str()),
                    lines,
                    json!({ "job": job, "task": data }),
                ));
            }
            if i % 5 == 0 {
                eprintln!(
                    "waiting for task {} (poll {}, status: {})...",
                    remote_id,
                    i + 1,
                    status.as_str()
                );
            }
        }
        tokio::time::sleep(poll_interval).await;
    }
    Err(anyhow!(
        "timed out waiting for task {} after {} polls",
        remote_id,
        max_polls
    ))
}

async fn execute_jobs_cancel(
    job_id: Option<String>,
    latest: bool,
    context: &AppContext,
) -> Result<CommandOutput> {
    let job = resolve_job(job_id, latest, context).await?;
    let Some(remote_id) = &job.remote_task_id else {
        return Err(anyhow!(
            "job {} has no remote task ID — cannot cancel",
            job.job_id
        ));
    };
    let result = context
        .perform(McpOperation::TaskCancel {
            task_id: remote_id.clone(),
        })
        .await?;
    match result {
        McpOperationResult::Task {
            status, message, ..
        } => Ok(CommandOutput::new(
            &context.config_name,
            "jobs cancel",
            message,
            vec![format!("status: {}", status.as_str())],
            json!({ "job_id": job.job_id, "status": status.as_str() }),
        )),
        other => Err(anyhow!(
            "unexpected response for tasks/cancel: {}",
            serde_json::to_string(&other)?
        )),
    }
}

async fn execute_jobs_watch(
    job_id: Option<String>,
    latest: bool,
    context: &AppContext,
) -> Result<CommandOutput> {
    // Watch is essentially the same as wait but with more verbose output
    let job = resolve_job(job_id, latest, context).await?;
    let Some(remote_id) = &job.remote_task_id else {
        return Ok(CommandOutput::new(
            &context.config_name,
            "jobs watch",
            format!("job {} has no remote task", job.job_id),
            vec![format!("status: {}", job.status.as_str())],
            json!({ "job": job }),
        ));
    };

    let poll_interval = std::time::Duration::from_secs(1);
    let max_polls = 600; // 10 min max
    let mut last_status = String::new();
    for _ in 0..max_polls {
        let result = context
            .perform(McpOperation::TaskGet {
                task_id: remote_id.clone(),
            })
            .await?;
        if let McpOperationResult::Task {
            status, message, ..
        } = &result
        {
            let status_str = status.as_str().to_owned();
            if status_str != last_status {
                eprintln!("[watch] task {} → {} {}", remote_id, status_str, message);
                last_status = status_str.clone();
            }
            let is_terminal = matches!(
                status,
                crate::mcp::model::TaskState::Completed
                    | crate::mcp::model::TaskState::Canceled
                    | crate::mcp::model::TaskState::Failed
            );
            if is_terminal {
                return Ok(CommandOutput::new(
                    &context.config_name,
                    "jobs watch",
                    format!("task {} {}", remote_id, status_str),
                    vec![format!("final status: {}", status_str)],
                    json!({ "job": job, "status": status_str }),
                ));
            }
        }
        tokio::time::sleep(poll_interval).await;
    }
    Err(anyhow!(
        "watch timed out for task {} after {} polls",
        remote_id,
        max_polls
    ))
}

/// Resolve a job by ID or --latest flag.
async fn resolve_job(
    job_id: Option<String>,
    latest: bool,
    context: &AppContext,
) -> Result<crate::runtime::JobRecord> {
    let jobs = context
        .services
        .state_store
        .jobs_for_config(&context.config_name)
        .await;
    if let Some(id) = job_id {
        jobs.iter()
            .find(|j| j.job_id == id)
            .cloned()
            .ok_or_else(|| anyhow!("job '{}' not found", id))
    } else if latest {
        jobs.into_iter()
            .next()
            .ok_or_else(|| anyhow!("no jobs found"))
    } else {
        Err(anyhow!("specify a job ID or use --latest"))
    }
}

/// Try to discover a category from the server, falling back to cache.
async fn try_discover(context: &AppContext, category: DiscoveryCategory) -> Option<Vec<Value>> {
    // Try live discovery first
    let result = context
        .perform(McpOperation::Discover {
            category: category.clone(),
        })
        .await;

    match result {
        Ok(McpOperationResult::Discovery {
            items,
            category: cat,
            ..
        }) => {
            // Cache the results
            let _ = context
                .services
                .state_store
                .upsert_discovery_inventory(
                    &context.config_name,
                    &context.config_name,
                    cat,
                    items.clone(),
                )
                .await;
            Some(items)
        }
        _ => {
            // Fall back to cache
            let inventory = context
                .services
                .state_store
                .discovery_inventory_view(&context.config_name)
                .await?;
            match category {
                DiscoveryCategory::Capabilities => inventory.tools,
                DiscoveryCategory::Resources => {
                    // Merge resources and templates
                    let mut all = inventory.resources.unwrap_or_default();
                    if let Some(templates) = inventory.resource_templates {
                        all.extend(templates);
                    }
                    Some(all)
                }
                DiscoveryCategory::Prompts => inventory.prompts,
            }
        }
    }
}

fn apply_filter<'a, F>(
    items: &'a [Value],
    filter: &Option<String>,
    extract_name: F,
) -> Vec<&'a Value>
where
    F: Fn(&Value) -> Option<&str>,
{
    match filter {
        Some(pattern) => items
            .iter()
            .filter(|item| {
                extract_name(item)
                    .map(|name| name.contains(pattern.as_str()))
                    .unwrap_or(false)
            })
            .collect(),
        None => items.iter().collect(),
    }
}

/// Materialize a URI template by substituting parameters.
fn materialize_uri_template(template: &str, arguments: &Value) -> String {
    let mut result = template.to_owned();
    if let Some(obj) = arguments.as_object() {
        for (key, value) in obj {
            let placeholder = format!("{{{}}}", key);
            let replacement = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            result = result.replace(&placeholder, &replacement);
        }
    }
    result
}

/// Format a tool call result.
async fn format_action_result(
    name: &str,
    result: McpOperationResult,
    context: &AppContext,
) -> Result<CommandOutput> {
    match result {
        McpOperationResult::Action { message, data } => {
            let lines = build_action_display_lines(&data);
            Ok(CommandOutput::new(
                &context.config_name,
                name,
                message,
                lines,
                data,
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
                    name,
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
                status: crate::runtime::JobStatus::Queued.as_str().to_owned(),
                message: format!("background job created for {}", name),
            });
            Ok(CommandOutput::new(
                &context.config_name,
                name,
                message,
                vec![
                    format!("job: {}", job.job_id),
                    format!("status: {}", job.status.as_str()),
                ],
                json!({ "job": job }),
            ))
        }
        other => Err(anyhow!(
            "unexpected response for tool '{}': {}",
            name,
            serde_json::to_string(&other)?
        )),
    }
}

/// Build human-readable display lines from a tool call result.
///
/// Prefers `structuredContent` (pretty JSON) when present, then falls back to
/// the `content` array with type-aware rendering for text, resource_link,
/// image, audio, and resource content items.
fn build_action_display_lines(data: &Value) -> Vec<String> {
    let result = match data.get("result") {
        Some(r) => r,
        None => {
            // Fall back to the summary field
            let summary = data
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("completed");
            return vec![summary.to_owned()];
        }
    };

    // Prefer structuredContent — render as pretty JSON
    if let Some(sc) = result.get("structuredContent") {
        return serde_json::to_string_pretty(sc)
            .unwrap_or_else(|_| "structured content".to_owned())
            .lines()
            .map(ToOwned::to_owned)
            .collect();
    }

    // Render the content array with type-aware formatting
    let content = match result.get("content").and_then(Value::as_array) {
        Some(c) => c,
        None => {
            let summary = data
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("completed");
            return vec![summary.to_owned()];
        }
    };

    let mut lines = Vec::new();
    for item in content {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("text");
        match item_type {
            "text" => {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    for line in text.lines() {
                        lines.push(line.to_owned());
                    }
                }
            }
            "resource_link" => {
                let uri = item.get("uri").and_then(Value::as_str).unwrap_or("?");
                let name = item.get("name").and_then(Value::as_str);
                let mime = item.get("mimeType").and_then(Value::as_str);
                let mut link = String::from("→ ");
                if let Some(n) = name {
                    link.push_str(n);
                    link.push_str(&format!(" ({})", uri));
                } else {
                    link.push_str(uri);
                }
                if let Some(m) = mime {
                    link.push_str(&format!(" [{}]", m));
                }
                lines.push(link);
            }
            "image" => {
                let mime = item
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .unwrap_or("image/*");
                let size = item
                    .get("data")
                    .and_then(Value::as_str)
                    .map(|d| d.len())
                    .unwrap_or(0);
                lines.push(format!("[image: {}, ~{} bytes base64]", mime, size));
            }
            "audio" => {
                let mime = item
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .unwrap_or("audio/*");
                let size = item
                    .get("data")
                    .and_then(Value::as_str)
                    .map(|d| d.len())
                    .unwrap_or(0);
                lines.push(format!("[audio: {}, ~{} bytes base64]", mime, size));
            }
            "resource" => {
                if let Some(res) = item.get("resource") {
                    let uri = res.get("uri").and_then(Value::as_str).unwrap_or("?");
                    let text_preview = res.get("text").and_then(Value::as_str);
                    if let Some(text) = text_preview {
                        lines.push(format!("[resource: {}]", uri));
                        for l in text.lines().take(20) {
                            lines.push(format!("  {}", l));
                        }
                    } else {
                        lines.push(format!("[resource: {}]", uri));
                    }
                }
            }
            _ => {
                // Unknown content type — render as JSON
                lines.push(serde_json::to_string(item).unwrap_or_default());
            }
        }
    }

    if lines.is_empty() {
        let summary = data
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or("completed");
        lines.push(summary.to_owned());
    }
    lines
}

/// Format a prompt result.
fn format_prompt_result(name: &str, result: McpOperationResult) -> Result<CommandOutput> {
    match result {
        McpOperationResult::Prompt {
            message,
            name: prompt_name,
            output,
            data,
        } => {
            let mut lines = Vec::new();
            for line in output.lines() {
                lines.push(line.to_owned());
            }
            Ok(CommandOutput::new(&prompt_name, name, message, lines, data))
        }
        other => Err(anyhow!(
            "unexpected response for prompt '{}': {}",
            name,
            serde_json::to_string(&other)?
        )),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::manifest::CommandManifest;
    use serde_json::json;

    fn test_manifest() -> CommandManifest {
        use crate::apps::manifest::*;
        use indexmap::IndexMap;

        let mut commands = IndexMap::new();
        commands.insert(
            "echo".to_owned(),
            ManifestEntry::Command(ManifestCommand {
                kind: CommandKind::Tool,
                origin_name: "echo".to_owned(),
                summary: "Echoes back the input".to_owned(),
                flags: IndexMap::from([(
                    "message".to_owned(),
                    FlagSpec {
                        flag_type: FlagType::String,
                        required: true,
                        default: None,
                        help: Some("Message to echo".to_owned()),
                        enum_values: None,
                    },
                )]),
                positional: None,
                supports_background: true,
            }),
        );
        commands.insert(
            "get".to_owned(),
            ManifestEntry::Command(ManifestCommand {
                kind: CommandKind::Resource,
                origin_name: "get".to_owned(),
                summary: "Fetch a resource".to_owned(),
                flags: IndexMap::new(),
                positional: Some(PositionalSpec {
                    name: "uri".to_owned(),
                    help: Some("URI to fetch".to_owned()),
                    required: true,
                }),
                supports_background: false,
            }),
        );

        CommandManifest {
            commands,
            server_name: Some("Test Server".to_owned()),
        }
    }

    #[test]
    fn builds_dynamic_cli_with_commands() {
        let manifest = test_manifest();
        let app = build_dynamic_cli("work", &manifest, "Test Server");
        // Should parse echo --message hello
        let matches = app
            .try_get_matches_from(vec!["work", "echo", "--message", "hello"])
            .expect("should parse");
        let (sub, sub_matches) = matches.subcommand().unwrap();
        assert_eq!(sub, "echo");
        assert_eq!(
            sub_matches.get_one::<String>("message").map(String::as_str),
            Some("hello")
        );
    }

    #[test]
    fn dynamic_cli_includes_runtime_commands() {
        let manifest = test_manifest();
        let app = build_dynamic_cli("work", &manifest, "Test Server");
        // Should parse auth login
        let matches = app
            .try_get_matches_from(vec!["work", "auth", "login"])
            .expect("should parse");
        let (sub, _) = matches.subcommand().unwrap();
        assert_eq!(sub, "auth");
    }

    #[test]
    fn parses_resource_get() {
        let manifest = test_manifest();
        let app = build_dynamic_cli("work", &manifest, "Test Server");
        let matches = app
            .try_get_matches_from(vec!["work", "get", "demo://resource/readme.md"])
            .expect("should parse");
        let (sub, sub_matches) = matches.subcommand().unwrap();
        assert_eq!(sub, "get");
        assert_eq!(
            sub_matches.get_one::<String>("uri").map(String::as_str),
            Some("demo://resource/readme.md")
        );
    }

    #[test]
    fn materialize_replaces_params() {
        let result = materialize_uri_template(
            "mail://search?q={query}&folder={folder}",
            &json!({"query": "invoice", "folder": "inbox"}),
        );
        assert_eq!(result, "mail://search?q=invoice&folder=inbox");
    }
}
