//! Runtime host: executes a resolved dispatch target.
//!
//! [`RuntimeHost`] is the command executor — it receives a
//! [`crate::dispatch::DispatchTarget`] (the decision made by
//! [`crate::app::build`]) and runs it. There are four shapes of
//! execution:
//!
//! - **`DispatchTarget::AppConfig`** (the common path) — resolve the
//!   named config, check whether a discovery cache is populated, and
//!   pick between the dynamic CLI
//!   ([`crate::apps::dynamic::build_dynamic_cli`]) and the static
//!   bridge ([`crate::apps::bridge::BridgeCli`]). Either way, the
//!   selected command lowers to an
//!   [`crate::mcp::model::McpOperation`] and flows through
//!   [`crate::apps::AppContext::perform`].
//! - **`DispatchTarget::Host`** — the host CLI lives in
//!   [`crate::cli`]. These are the mcp2cli-administrative commands:
//!   `config` (create/show/list/delete configs), `use` (switch active
//!   config), `link` (install `name` symlink aliases), `daemon`
//!   (start/stop/status the connection-reuse daemon), and `man` (emit
//!   or install the man page).
//! - **`DispatchTarget::AdHoc`** — no config on disk. The user
//!   passed `--url <URL>` or `--stdio <CMD>`; we build an ephemeral
//!   [`crate::config::ResolvedAppConfig`] in-memory and reuse the
//!   `AppConfig` code path.
//! - **`DispatchTarget::McpShim`** — the `mcp-<server>-<tool>` symlink
//!   form. [`run_mcp_shim`] reads the tool cache, builds a JSON-RPC
//!   `tools/call` from argv, and dials the MCP bridge over
//!   AF_VSOCK or AF_UNIX via [`crate::mcp::vsock_shim`].
//!
//! [`RuntimeServices`] is the shared bag of handles —
//! [`crate::runtime::StateStore`], [`crate::runtime::TokenStore`],
//! [`crate::runtime::EventBroker`], the selected
//! [`crate::mcp::client::McpClient`], and the
//! [`crate::telemetry::TelemetryRecorder`]. All execution paths
//! receive a cloned `RuntimeServices`, which keeps the hot path
//! allocation-light and tests easy to wire up.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use clap::error::ErrorKind;

use crate::{
    apps::{AppContext, bridge, manifest::CommandManifest},
    cli::{
        ConfigCommand, DaemonCommand, HostCommand, LinkCommand, ManCommand, UseArgs,
        config_show_output, configs_list_output, link_create_output, man_install_output,
        parse_host_cli, use_clear_output, use_config_output, use_status_output,
    },
    config::{
        ConfigCreateOptions, ResolvedAppConfig, RuntimeLayout, active_config_load_status,
        clear_active_config, list_named_configs, validate_config_name, write_active_config,
        write_named_config,
    },
    dispatch::{DispatchTarget, HOST_BINARY_NAME, is_host_command},
    man,
    mcp::client::McpClient,
    output::{CommandOutput, OutputFormat, detect_output_format, render},
    runtime::{EventBroker, MemoryEventSink, StderrEventSink},
};

use super::state::StateStore;
use super::token_store::TokenStore;

#[derive(Clone)]
pub struct RuntimeServices {
    pub state_store: Arc<StateStore>,
    pub token_store: Arc<TokenStore>,
    pub event_broker: EventBroker,
    pub mcp_client: Arc<dyn McpClient>,
}

pub struct RuntimeHost {
    layout: RuntimeLayout,
    selected_config: Option<Arc<ResolvedAppConfig>>,
    state_store: Option<Arc<StateStore>>,
    token_store: Option<Arc<TokenStore>>,
    mcp_client: Arc<dyn McpClient>,
}

impl RuntimeHost {
    pub fn new(
        layout: RuntimeLayout,
        selected_config: Option<Arc<ResolvedAppConfig>>,
        state_store: Option<Arc<StateStore>>,
        token_store: Option<Arc<TokenStore>>,
        mcp_client: Arc<dyn McpClient>,
    ) -> Self {
        Self {
            layout,
            selected_config,
            state_store,
            token_store,
            mcp_client,
        }
    }

    pub async fn run(&self, target: DispatchTarget) -> Result<()> {
        match target {
            DispatchTarget::AppConfig {
                config_name,
                invoked_as,
                forwarded_argv,
            } => {
                self.run_app(&config_name, &invoked_as, &forwarded_argv)
                    .await
            }
            DispatchTarget::Host { invoked_as, argv } => self.run_host(&invoked_as, &argv).await,
            DispatchTarget::AdHoc { .. } => {
                // AdHoc is resolved to AppConfig during build(); should never appear here
                Err(anyhow!("unexpected AdHoc dispatch target at runtime"))
            }
            DispatchTarget::McpShim {
                server,
                tool,
                invoked_as,
                forwarded_argv,
            } => run_mcp_shim(&server, &tool, &invoked_as, &forwarded_argv),
        }
    }

    async fn run_app(
        &self,
        config_name: &str,
        invoked_as: &str,
        argv: &[std::ffi::OsString],
    ) -> Result<()> {
        let selected_config = self
            .selected_config
            .as_ref()
            .ok_or_else(|| anyhow!("application config was not loaded for '{}'", config_name))?;
        if selected_config.name != config_name {
            return Err(anyhow!(
                "loaded config '{}' does not match requested config '{}'",
                selected_config.name,
                config_name
            ));
        }

        let default_output = selected_config.config.defaults.output;
        let output_format = bridge::peek_output_format(argv, default_output);
        let memory_sink = Arc::new(MemoryEventSink::default());
        let broker = self.event_broker(output_format, memory_sink.clone());
        let context = AppContext {
            invoked_as: invoked_as.to_owned(),
            config_name: selected_config.name.clone(),
            config: Arc::new(selected_config.config.clone()),
            services: RuntimeServices {
                state_store: Arc::clone(self.state_store.as_ref().ok_or_else(|| {
                    anyhow!("state store was not initialized for '{}'", config_name)
                })?),
                token_store: Arc::clone(self.token_store.as_ref().ok_or_else(|| {
                    anyhow!("token store was not initialized for '{}'", config_name)
                })?),
                event_broker: broker,
                mcp_client: Arc::clone(&self.mcp_client),
            },
            timeout_override: None,
        };
        let report = bridge::execute(argv, context).await?;
        if let Some(session) = self.mcp_client.negotiated_session().await {
            self.state_store
                .as_ref()
                .ok_or_else(|| anyhow!("state store was not initialized for '{}'", config_name))?
                .upsert_negotiated_capability_view(
                    &selected_config.name,
                    &selected_config.config.app.profile,
                    &session,
                )
                .await?;
        }
        render(report.output_format, &report.output, &memory_sink.events())
    }

    async fn run_host(&self, invoked_as: &str, argv: &[std::ffi::OsString]) -> Result<()> {
        let detected_output = detect_output_format(argv, OutputFormat::Human);
        let host_cli = match parse_host_cli(argv, invoked_as) {
            Ok(cli) => cli,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::DisplayHelp
                        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                        | ErrorKind::DisplayVersion
                ) =>
            {
                let output = self.host_display_output(invoked_as, error);
                return render(detected_output, &output, &[]);
            }
            Err(error) => return Err(error.into()),
        };
        let output_format = host_cli.effective_output(OutputFormat::Human);
        let output = match host_cli.command {
            HostCommand::Config(args) => match args.command {
                ConfigCommand::List => configs_list_output(&list_named_configs(&self.layout)?),
                ConfigCommand::Show(args) => {
                    let config =
                        crate::config::AppConfig::load_named(&args.name, None, &self.layout)?;
                    config_show_output(&config)
                }
                ConfigCommand::Init(args) => {
                    self.ensure_host_config_name(&args.name)?;
                    let created = write_named_config(
                        &self.layout,
                        &ConfigCreateOptions {
                            name: args.name,
                            app_profile: args.app,
                            transport: args.transport.into(),
                            endpoint: args.endpoint,
                            stdio_command: args.stdio_command,
                            stdio_args: args.stdio_args,
                            force: args.force,
                        },
                    )?;
                    config_show_output(&created)
                }
            },
            HostCommand::Link(args) => match args.command {
                LinkCommand::Create(args) => {
                    self.ensure_host_config_name(&args.name)?;
                    if !args.force {
                        let config_path = self.layout.named_config_path(&args.name);
                        if !config_path.exists() {
                            return Err(anyhow!(
                                "no named config '{}' found at {}; create it first with `mcp2cli config init --name {}`, or pass --force to skip this check",
                                args.name,
                                config_path.display(),
                                args.name,
                            ));
                        }
                    }
                    let (link_path, target_path) =
                        self.create_self_link(&args.name, args.dir.as_deref(), args.force)?;

                    // --- Man page generation (best-effort) ---
                    let man_result = if args.no_man {
                        None // user opted out
                    } else {
                        Some(
                            self.install_man_page(&args.name, args.man_dir.as_deref())
                                .await
                                .map_err(|e| e.to_string()),
                        )
                    };

                    link_create_output(&args.name, &link_path, &target_path, man_result)
                }
            },
            HostCommand::Use(args) => self.use_named_config(args)?,
            HostCommand::Man(args) => match args.command {
                ManCommand::Install(args) => {
                    let man_dir = args
                        .dir
                        .as_deref()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| self.layout.man_dir());
                    let content = man::generate_host();
                    let page_path = man::install("mcp2cli", &content, &man_dir)?;
                    man_install_output(&page_path)
                }
            },
            HostCommand::Daemon(args) => {
                return self.handle_daemon(args, output_format).await;
            }
        };
        render(output_format, &output, &[])
    }

    fn host_display_output(&self, invoked_as: &str, error: clap::Error) -> CommandOutput {
        let command = if error.kind() == ErrorKind::DisplayVersion {
            "version"
        } else {
            "help"
        };
        let text = error.to_string();
        CommandOutput::new(
            HOST_BINARY_NAME,
            command,
            format!("showing {} {}", invoked_as, command),
            text.lines().map(ToOwned::to_owned).collect(),
            serde_json::json!({
                "invoked_as": invoked_as,
                "command": command,
                "text": text,
            }),
        )
    }

    fn use_named_config(&self, args: UseArgs) -> Result<crate::output::CommandOutput> {
        if args.clear {
            let cleared = clear_active_config(&self.layout)?;
            return Ok(use_clear_output(cleared.as_ref()));
        }

        if args.show || args.name.is_none() {
            let Some((selection, loaded)) = active_config_load_status(&self.layout)? else {
                return Ok(use_status_output(None, None, None));
            };
            return Ok(match loaded {
                Ok(config) => use_status_output(Some(&selection), Some(&config), None),
                Err(error) => use_status_output(Some(&selection), None, Some(&error.to_string())),
            });
        }

        let name = args.name.expect("validated use args should include a name");
        self.ensure_host_config_name(&name)?;
        let config = crate::config::AppConfig::load_named(&name, None, &self.layout)?;
        let selection = write_active_config(&self.layout, &name)?;
        Ok(use_config_output(&selection, &config))
    }

    fn event_broker(
        &self,
        output_format: OutputFormat,
        memory_sink: Arc<MemoryEventSink>,
    ) -> EventBroker {
        let mut sinks: Vec<Arc<dyn super::EventSink>> = vec![memory_sink];
        if output_format == OutputFormat::Human
            && self
                .selected_config
                .as_ref()
                .map(|config| config.config.events.enable_stdio_events)
                .unwrap_or(false)
        {
            sinks.push(Arc::new(StderrEventSink));
        }

        if let Some(config) = self.selected_config.as_ref() {
            let events = &config.config.events;

            if let Some(endpoint) = &events.http_endpoint {
                if let Ok(url) = url::Url::parse(endpoint) {
                    sinks.push(Arc::new(super::HttpWebhookSink::new(url)));
                } else {
                    tracing::warn!("events.http_endpoint is not a valid URL: {}", endpoint);
                }
            }

            if let Some(path) = &events.local_socket_path {
                sinks.push(Arc::new(super::UnixSocketSink::new(path.clone())));
            }

            if let Some(endpoint) = &events.sse_endpoint {
                if let Ok(addr) = endpoint.parse::<std::net::SocketAddr>() {
                    match super::SseServerSink::start(addr) {
                        Ok(sink) => sinks.push(Arc::new(sink)),
                        Err(error) => {
                            tracing::warn!("failed to start SSE server on {}: {}", addr, error)
                        }
                    }
                } else {
                    tracing::warn!(
                        "events.sse_endpoint is not a valid socket address: {}",
                        endpoint
                    );
                }
            }

            if let Some(command) = &events.command {
                sinks.push(Arc::new(super::CommandExecSink::new(command.clone())));
            }
        }

        EventBroker::new(sinks)
    }

    fn ensure_host_config_name(&self, name: &str) -> Result<()> {
        validate_config_name(name)?;
        if name == HOST_BINARY_NAME || is_host_command(name) {
            return Err(anyhow!(
                "'{}' is reserved for host commands and cannot be used as a config or link name",
                name
            ));
        }
        Ok(())
    }

    /// Generate and install a man page for the given alias name.
    ///
    /// Attempts to load the named config and the cached discovery inventory.
    /// If either is unavailable the page is still generated without the MCP
    /// commands section. Errors are returned but the caller treats them as
    /// non-fatal: a failure to install a man page must not fail `link create`.
    async fn install_man_page(&self, name: &str, override_dir: Option<&Path>) -> Result<PathBuf> {
        // Load the named config (needed for transport / display_name metadata).
        let config = match crate::config::AppConfig::load_named(name, None, &self.layout) {
            Ok(resolved) => resolved.config,
            Err(e) => {
                return Err(anyhow!(
                    "could not load config '{}' for man page generation: {}",
                    name,
                    e
                ));
            }
        };

        // Attempt to read the cached discovery inventory from the state store.
        let state_path = self.layout.state_file_path(name);
        let manifest: Option<CommandManifest> = if state_path.exists() {
            match crate::runtime::StateStore::load(state_path).await {
                Ok(store) => match store.discovery_inventory_view(name).await {
                    Some(inventory) => {
                        let profile = config.profile.as_ref();
                        let mut manifest = CommandManifest::from_inventory(&inventory);
                        if let Some(p) = profile {
                            manifest.apply_profile(p);
                        }
                        Some(manifest)
                    }
                    None => None,
                },
                Err(e) => {
                    tracing::debug!("could not load state store for man page: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let man_dir = override_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.layout.man_dir());

        let ctx = man::ManPageContext {
            name,
            config: &config,
            manifest: manifest.as_ref(),
        };
        let content = man::generate(&ctx);
        let alias_page = man::install(name, &content, &man_dir)?;

        // Opportunistically install the mcp2cli(1) host page alongside the
        // alias page.  This is best-effort: a failure here does not fail the
        // link creation.
        let host_content = man::generate_host();
        let _ = man::install("mcp2cli", &host_content, &man_dir);

        Ok(alias_page)
    }

    fn create_self_link(
        &self,
        name: &str,
        dir: Option<&Path>,
        force: bool,
    ) -> Result<(PathBuf, PathBuf)> {
        let target_path = std::env::current_exe().context("failed to locate current executable")?;
        let link_dir = dir
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.layout.link_dir());
        fs::create_dir_all(&link_dir)
            .with_context(|| format!("failed to create link directory: {}", link_dir.display()))?;

        let link_path = link_dir.join(name);
        if fs::symlink_metadata(&link_path).is_ok() {
            if !force {
                return Err(anyhow!("link already exists: {}", link_path.display()));
            }
            fs::remove_file(&link_path).with_context(|| {
                format!("failed to remove existing link: {}", link_path.display())
            })?;
        }

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target_path, &link_path).with_context(|| {
                format!(
                    "failed to create symlink from {} to {}",
                    link_path.display(),
                    target_path.display()
                )
            })?;
        }

        #[cfg(not(unix))]
        {
            return Err(anyhow!(
                "link creation is currently only implemented on unix"
            ));
        }

        Ok((link_path, target_path))
    }

    async fn handle_daemon(
        &self,
        args: crate::cli::DaemonArgs,
        output_format: OutputFormat,
    ) -> Result<()> {
        use super::daemon;

        match args.command {
            DaemonCommand::Start { name } => {
                // Check if already running
                if let Some(info) = daemon::daemon_status(&self.layout, &name)? {
                    let output = CommandOutput::new(
                        HOST_BINARY_NAME,
                        "daemon start",
                        format!(
                            "daemon for '{}' is already running (pid {})",
                            name, info.pid
                        ),
                        vec![
                            format!("pid: {}", info.pid),
                            format!("socket: {}", info.socket_path),
                        ],
                        serde_json::json!({ "config": name, "pid": info.pid, "status": "already_running" }),
                    );
                    return render(output_format, &output, &[]);
                }

                // Load config to validate it exists
                let resolved = crate::config::AppConfig::load_named(&name, None, &self.layout)?;

                // Build client for this config
                let client =
                    crate::mcp::client::build_client(&self.layout, Some(&resolved)).await?;
                let client: Arc<dyn crate::mcp::client::McpClient> = Arc::from(client);

                // Fork by re-executing ourselves with a special env var
                // For simplicity in the first implementation, we run daemonized in foreground
                // if MCP2CLI_DAEMON_FOREGROUND is set, otherwise we provide instructions.
                if std::env::var("MCP2CLI_DAEMON_FOREGROUND").is_ok() {
                    let _memory_sink = Arc::new(MemoryEventSink::default());
                    let broker = EventBroker::new(vec![Arc::new(StderrEventSink)]);
                    daemon::run_daemon(&self.layout, &name, client, broker).await?;
                    return Ok(());
                }

                // Spawn a background child process
                let exe = std::env::current_exe()
                    .map_err(|e| anyhow!("failed to locate current executable: {}", e))?;
                let child = tokio::process::Command::new(&exe)
                    .arg("daemon")
                    .arg("start")
                    .arg(&name)
                    .env("MCP2CLI_DAEMON_FOREGROUND", "1")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .map_err(|e| anyhow!("failed to spawn daemon process: {}", e))?;

                let pid = child.id().unwrap_or(0);
                let output = CommandOutput::new(
                    HOST_BINARY_NAME,
                    "daemon start",
                    format!("daemon started for '{}' (pid {})", name, pid),
                    vec![
                        format!("config: {}", name),
                        format!("pid: {}", pid),
                        format!(
                            "socket: {}",
                            self.layout.daemon_socket_path(&name).display()
                        ),
                    ],
                    serde_json::json!({ "config": name, "pid": pid, "status": "started" }),
                );
                render(output_format, &output, &[])
            }

            DaemonCommand::Stop { name } => {
                let stopped = daemon::stop_daemon(&self.layout, &name)?;
                let output = if stopped {
                    CommandOutput::new(
                        HOST_BINARY_NAME,
                        "daemon stop",
                        format!("daemon for '{}' stopped", name),
                        vec![format!("config: {}", name), "status: stopped".to_owned()],
                        serde_json::json!({ "config": name, "status": "stopped" }),
                    )
                } else {
                    CommandOutput::new(
                        HOST_BINARY_NAME,
                        "daemon stop",
                        format!("no daemon running for '{}'", name),
                        vec![
                            format!("config: {}", name),
                            "status: not_running".to_owned(),
                        ],
                        serde_json::json!({ "config": name, "status": "not_running" }),
                    )
                };
                render(output_format, &output, &[])
            }

            DaemonCommand::Status { name } => {
                let configs = if let Some(name) = &name {
                    vec![name.clone()]
                } else {
                    crate::config::list_named_configs(&self.layout)?
                        .iter()
                        .map(|c| c.name.clone())
                        .collect()
                };

                let mut lines = Vec::new();
                let mut json_entries = Vec::new();
                for config_name in &configs {
                    match daemon::daemon_status(&self.layout, config_name)? {
                        Some(info) => {
                            lines.push(format!(
                                "{}: running (pid {}, socket {})",
                                config_name, info.pid, info.socket_path
                            ));
                            json_entries.push(serde_json::json!({
                                "config": config_name,
                                "status": "running",
                                "pid": info.pid,
                                "socket": info.socket_path,
                                "started_at": info.started_at,
                            }));
                        }
                        None => {
                            lines.push(format!("{}: not running", config_name));
                            json_entries.push(serde_json::json!({
                                "config": config_name,
                                "status": "not_running",
                            }));
                        }
                    }
                }

                let summary = if json_entries.iter().any(|e| e["status"] == "running") {
                    "daemon status"
                } else {
                    "no daemons running"
                };

                let output = CommandOutput::new(
                    HOST_BINARY_NAME,
                    "daemon status",
                    summary.to_owned(),
                    lines,
                    serde_json::json!({ "daemons": json_entries }),
                );
                render(output_format, &output, &[])
            }
        }
    }
}

// ---------------- MCP shim runtime ----------------

/// Handle a `DispatchTarget::McpShim` invocation.
///
/// The shim dispatches when `mcp2cli` is invoked as a
/// `mcp-<server>-<tool>` symlink and reads a per-server tool-cache
/// JSON at `$MCP_CACHE_DIR/<server>.json` (default `/run/mcp/`). With
/// `--help`, `-h`, or `--describe`, the shim prints the tool's
/// recorded description + server metadata. Any other invocation dials
/// the MCP bridge (AF_UNIX in dev/CI via `MCP_SHIM_UNIX_DIR`, or
/// AF_VSOCK in production via `MCP_HOST_CID`) and pipes NDJSON.
///
/// The cache directory is overridable via the `MCP_CACHE_DIR`
/// environment variable so unit tests don't need a real `/run/mcp`.
fn run_mcp_shim(
    server: &str,
    tool: &str,
    invoked_as: &str,
    forwarded_argv: &[std::ffi::OsString],
) -> anyhow::Result<()> {
    use std::ffi::OsStr;
    use std::path::PathBuf;

    let wants_help = forwarded_argv.iter().any(|a| {
        a == OsStr::new("--help") || a == OsStr::new("-h") || a == OsStr::new("--describe")
    });

    let cache_root = std::env::var_os("MCP_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/run/mcp"));
    let cache_path = cache_root.join(format!("{server}.json"));

    if wants_help {
        match std::fs::read(&cache_path) {
            Ok(bytes) => match serde_json::from_slice::<McpShimToolCache>(&bytes) {
                Ok(cache) => {
                    print_shim_help(invoked_as, server, tool, &cache);
                    return Ok(());
                }
                Err(err) => {
                    eprintln!(
                        "mcp shim: cache at {} is malformed: {err}",
                        cache_path.display()
                    );
                    return Err(anyhow!("malformed tool cache"));
                }
            },
            Err(err) => {
                eprintln!(
                    "mcp shim: no tool cache at {} ({}); ensure the MCP layout has been materialised (set MCP_CACHE_DIR or write <server>.json)",
                    cache_path.display(),
                    err
                );
                return Err(anyhow!("tool cache missing"));
            }
        }
    }

    // Dial the MCP bridge and pipe NDJSON. The tool cache is
    // authoritative for the VSOCK port; the dial target (Unix for
    // dev/CI or AF_VSOCK in prod) comes from environment variables
    // — `MCP_SHIM_UNIX_DIR` wins, then `MCP_HOST_CID`.
    let cache_bytes = std::fs::read(&cache_path).map_err(|err| {
        eprintln!(
            "mcp shim: no tool cache at {} ({}); ensure the MCP layout has been materialised (set MCP_CACHE_DIR or write <server>.json)",
            cache_path.display(),
            err
        );
        anyhow!("tool cache missing")
    })?;
    let cache: McpShimToolCache = serde_json::from_slice(&cache_bytes).map_err(|err| {
        eprintln!(
            "mcp shim: cache at {} is malformed: {err}",
            cache_path.display()
        );
        anyhow!("malformed tool cache")
    })?;

    // Allow-list enforcement client-side. The host proxy is still
    // the authority; failing fast here saves a round-trip and yields
    // a clearer error.
    if !cache.allowed_tools.is_empty() && !cache.allowed_tools.iter().any(|a| a == tool) {
        eprintln!(
            "mcp shim: tool `{tool}` is not in server `{server}`'s allowed_tools; call refused locally"
        );
        return Err(anyhow!("tool not allowed"));
    }

    // Argv marshalling: by default, build a JSON-RPC `tools/call`
    // request from argv (`key=value` pairs and `--json '{…}'`). The
    // raw-NDJSON escape hatch is `--ndjson` — useful for testing or
    // when the caller has a hand-built request body.
    let raw_pipe = forwarded_argv.iter().any(|a| a == OsStr::new("--ndjson"));

    match crate::mcp::vsock_shim::target_from_env(server, cache.vsock_port) {
        Some(target) => {
            tracing::debug!(
                server = %server,
                tool = %tool,
                raw_pipe,
                ?target,
                "mcp shim dialing bridge"
            );
            let stream = crate::mcp::vsock_shim::dial(&target)?;
            if raw_pipe {
                crate::mcp::vsock_shim::pipe_ndjson(std::io::stdin(), std::io::stdout(), stream)
            } else {
                let arguments = parse_tool_args(forwarded_argv)?;
                let request = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": { "name": tool, "arguments": arguments },
                });
                let response = crate::mcp::vsock_shim::single_shot(&request, stream)?;
                let pretty = serde_json::to_string_pretty(&response)
                    .unwrap_or_else(|_| response.to_string());
                println!("{pretty}");
                if let Some(err) = response.get("error") {
                    return Err(anyhow!("mcp tool error: {err}"));
                }
                Ok(())
            }
        }
        None => {
            eprintln!(
                "mcp shim: `{invoked_as}` → server=`{server}` tool=`{tool}`: \
                 no dial target configured. Set MCP_SHIM_UNIX_DIR=<dir> (dev: \
                 expects <dir>/{server}.sock) or MCP_HOST_CID=<cid> (AF_VSOCK). \
                 Run with --help for offline tool details."
            );
            Err(anyhow!("mcp shim: no dial target"))
        }
    }
}

/// Build the `arguments` object for an MCP `tools/call` from a shim
/// invocation's argv. Three accepted shapes:
///
/// - `key=value` pairs (the common case, e.g. `path=/workspace/x.txt`).
///   Each value is parsed as JSON if it looks like a JSON literal
///   (starts with `{`, `[`, `"`, `-`, a digit, or is `true`/`false`/
///   `null`); otherwise it stays a string. Repeated keys overwrite.
/// - `--json '{"…":…}'` — a single JSON object literal merged into the
///   arguments. `--json @file.json` reads from disk.
/// - `--ndjson` is consumed by the caller (raw-pipe escape hatch) and
///   never reaches here.
///
/// Bare positional args (no `=`, not `--json`) are rejected with a
/// clear error so we never silently drop input.
pub(crate) fn parse_tool_args(argv: &[std::ffi::OsString]) -> anyhow::Result<serde_json::Value> {
    use serde_json::{Map, Value};
    let mut map: Map<String, Value> = Map::new();
    let mut iter = argv.iter().peekable();
    while let Some(raw) = iter.next() {
        let s = raw.to_str().ok_or_else(|| {
            anyhow!("non-UTF-8 argv element rejected; use --ndjson for raw bytes")
        })?;
        // Skip help-style flags handled earlier.
        if matches!(s, "--help" | "-h" | "--describe") {
            continue;
        }
        if s == "--json" {
            let v = iter
                .next()
                .ok_or_else(|| anyhow!("--json requires a value"))?;
            let body = read_json_arg(v)?;
            merge_object(&mut map, body)?;
            continue;
        }
        if let Some(rest) = s.strip_prefix("--json=") {
            let body = read_json_arg(std::ffi::OsStr::new(rest))?;
            merge_object(&mut map, body)?;
            continue;
        }
        if let Some((k, v)) = s.split_once('=') {
            if k.is_empty() {
                return Err(anyhow!("empty key in `{s}`"));
            }
            map.insert(k.to_string(), parse_value(v));
            continue;
        }
        return Err(anyhow!(
            "bare positional arg `{s}` rejected — use key=value, --json '{{…}}', or --ndjson"
        ));
    }
    Ok(Value::Object(map))
}

fn read_json_arg(s: &std::ffi::OsStr) -> anyhow::Result<serde_json::Value> {
    let s = s
        .to_str()
        .ok_or_else(|| anyhow!("non-UTF-8 --json value"))?;
    let bytes = if let Some(path) = s.strip_prefix('@') {
        std::fs::read(path).map_err(|e| anyhow!("--json @{path}: {e}"))?
    } else {
        s.as_bytes().to_vec()
    };
    serde_json::from_slice(&bytes).map_err(|e| anyhow!("--json parse: {e}"))
}

fn merge_object(
    into: &mut serde_json::Map<String, serde_json::Value>,
    body: serde_json::Value,
) -> anyhow::Result<()> {
    match body {
        serde_json::Value::Object(o) => {
            for (k, v) in o {
                into.insert(k, v);
            }
            Ok(())
        }
        other => Err(anyhow!(
            "--json must be an object, got {}",
            other_kind(&other)
        )),
    }
}

fn other_kind(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Parse a value: if it parses as JSON, use that; else keep as string.
/// Conservatively, only attempt JSON parse when the first char hints
/// at a JSON literal — avoids surprising the user when their string
/// happens to contain a colon or a brace.
fn parse_value(s: &str) -> serde_json::Value {
    let looks_like_json = matches!(
        s.chars().next(),
        Some('{') | Some('[') | Some('"') | Some('-') | Some('0'..='9')
    ) || matches!(s, "true" | "false" | "null");
    if looks_like_json && let Ok(v) = serde_json::from_str(s) {
        return v;
    }
    serde_json::Value::String(s.to_string())
}

#[derive(Debug, serde::Deserialize)]
struct McpShimToolCache {
    #[allow(dead_code)]
    name: String,
    vsock_port: u32,
    #[serde(default)]
    tools: Vec<McpShimToolEntry>,
    #[serde(default)]
    allowed_tools: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct McpShimToolEntry {
    name: String,
    #[serde(default)]
    description: String,
}

fn print_shim_help(invoked_as: &str, server: &str, tool: &str, cache: &McpShimToolCache) {
    println!("Usage: {invoked_as} [args...]");
    println!();
    println!("Server:   {server}  (vsock port {})", cache.vsock_port);
    println!("Tool:     {tool}");
    let declared = cache.tools.iter().find(|t| t.name == tool);
    match declared {
        Some(entry) if !entry.description.is_empty() => {
            println!("Describe: {}", entry.description);
        }
        Some(_) => println!("Describe: (no description in cache)"),
        None => println!(
            "Describe: (tool not listed in cache — shim may be stale; re-materialise the tool cache)"
        ),
    }
    if !cache.allowed_tools.is_empty() && !cache.allowed_tools.iter().any(|a| a == tool) {
        println!();
        println!(
            "Note: this tool is NOT in the server's allowed_tools list; calls will be refused."
        );
    }
}

#[cfg(test)]
mod shim_tests {
    use super::parse_tool_args;
    use std::ffi::OsString;

    fn argv(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(|s| OsString::from(*s)).collect()
    }

    #[test]
    fn empty_argv_yields_empty_object() {
        let v = parse_tool_args(&[]).unwrap();
        assert_eq!(v, serde_json::json!({}));
    }

    #[test]
    fn key_value_pairs_become_object_keys() {
        let v = parse_tool_args(&argv(&["path=foo.txt", "n=3"])).unwrap();
        assert_eq!(v, serde_json::json!({ "path": "foo.txt", "n": 3 }));
    }

    #[test]
    fn json_literal_values_are_parsed() {
        let v = parse_tool_args(&argv(&[
            "list=[1,2,3]",
            "obj={\"k\":1}",
            "flag=true",
            "miss=null",
        ]))
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "list": [1, 2, 3],
                "obj": {"k": 1},
                "flag": true,
                "miss": null,
            })
        );
    }

    #[test]
    fn ambiguous_strings_stay_strings() {
        // Doesn't start with a JSON-shaped char → keep verbatim.
        let v = parse_tool_args(&argv(&["msg=hello world", "tag=alpha"])).unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "msg": "hello world", "tag": "alpha" })
        );
    }

    #[test]
    fn json_flag_merges_into_arguments() {
        let v = parse_tool_args(&argv(&["a=1", "--json", "{\"b\":2}", "c=3"])).unwrap();
        assert_eq!(v, serde_json::json!({ "a": 1, "b": 2, "c": 3 }));
    }

    #[test]
    fn json_eq_form_is_supported() {
        let v = parse_tool_args(&argv(&["--json={\"x\":42}"])).unwrap();
        assert_eq!(v, serde_json::json!({ "x": 42 }));
    }

    #[test]
    fn json_at_path_reads_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("body.json");
        std::fs::write(&path, br#"{"k":"v"}"#).unwrap();
        let arg = format!("@{}", path.display());
        let v = parse_tool_args(&argv(&["--json", &arg])).unwrap();
        assert_eq!(v, serde_json::json!({ "k": "v" }));
    }

    #[test]
    fn json_must_be_object() {
        let err = parse_tool_args(&argv(&["--json", "[1,2]"])).unwrap_err();
        assert!(format!("{err}").contains("--json must be an object"));
    }

    #[test]
    fn bare_positional_arg_is_rejected() {
        let err = parse_tool_args(&argv(&["this-is-not-a-pair"])).unwrap_err();
        assert!(format!("{err}").contains("bare positional"));
    }

    #[test]
    fn empty_key_is_rejected() {
        let err = parse_tool_args(&argv(&["=value"])).unwrap_err();
        assert!(format!("{err}").contains("empty key"));
    }

    #[test]
    fn help_flags_are_silently_dropped() {
        // The shim's help branch consumes these, but parse_tool_args
        // is also called when the help branch already exited; for
        // safety it ignores them rather than treating as positional.
        let v = parse_tool_args(&argv(&["--help", "path=x", "-h"])).unwrap();
        assert_eq!(v, serde_json::json!({ "path": "x" }));
    }

    #[test]
    fn later_keys_overwrite_earlier_ones() {
        let v = parse_tool_args(&argv(&["k=1", "k=2"])).unwrap();
        assert_eq!(v, serde_json::json!({ "k": 2 }));
    }

    #[test]
    fn json_flag_overrides_key_value_with_same_name() {
        let v = parse_tool_args(&argv(&["k=1", "--json", "{\"k\":99}"])).unwrap();
        assert_eq!(v, serde_json::json!({ "k": 99 }));
    }
}
