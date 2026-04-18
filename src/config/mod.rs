//! Configuration schema and active-config selection.
//!
//! Each named MCP server binding lives as a YAML file at
//! `$XDG_CONFIG_HOME/mcp2cli/configs/<name>.yaml` ([`AppConfig`]).
//! The active selection (which config `mcp2cli ls/invoke/...`
//! operates on by default) is persisted at
//! `$XDG_CONFIG_HOME/mcp2cli/active.yaml`.
//!
//! # Loading pipeline
//!
//! [`load_active_config`] runs this pipeline:
//!
//! 1. Figure out **which** config to load — either an explicit
//!    `--config <path>` / `MCP2CLI_CONFIG` env, the CLI-invocation
//!    alias, or the saved active selection.
//! 2. Parse the YAML with `figment`, layering:
//!    - Built-in defaults.
//!    - The YAML file itself.
//!    - Environment overrides prefixed `MCP2CLI_` (e.g.
//!      `MCP2CLI_SERVER__ENDPOINT=https://prod.api/mcp`).
//! 3. Validate the result into a [`ResolvedAppConfig`] — which also
//!    includes the runtime layout
//!    ([`RuntimeLayout`]: directories where state, caches, and
//!    tokens live).
//!
//! # Config shape
//!
//! The YAML tree maps to [`AppConfig`]:
//!
//! - `app.profile` — which profile to apply (overlays rename/hide
//!   commands). See [`crate::apps::manifest`] for how profiles are
//!   consumed.
//! - `server.transport` — `stdio` or `streamable_http`.
//! - `server.stdio` — command, args, env, cwd when transport is stdio.
//! - `server.http` — endpoint URL, auth, headers when transport is
//!   streamable HTTP.
//! - `defaults` — timeouts, output format, log levels.
//! - `events` — which [`crate::runtime::EventSink`]s to install (stderr,
//!   HTTP webhook, Unix socket, SSE server, command exec).
//! - `roots` — client-advertised root URIs for servers that query them
//!   via `roots/list`.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use directories::ProjectDirs;
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Yaml},
};
use serde::{Deserialize, Serialize};

use crate::{mcp::model::TransportKind, output::OutputFormat};

/// Prefix for environment-variable overrides consumed by `figment`.
///
/// `MCP2CLI_SERVER__ENDPOINT=https://...` maps to `server.endpoint`
/// in the config tree (double-underscore is the nesting separator).
const ENV_PREFIX: &str = "MCP2CLI_";

/// Root configuration model for a named MCP server binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub app: AppBindingConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub defaults: DefaultsConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub plugins: PluginConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub events: EventConfig,
    #[serde(default)]
    pub telemetry: crate::telemetry::TelemetryConfig,
    /// Optional profile overlay to customize the dynamic CLI surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<crate::apps::manifest::ProfileOverlay>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            app: AppBindingConfig::default(),
            server: ServerConfig::default(),
            defaults: DefaultsConfig::default(),
            logging: LoggingConfig::default(),
            plugins: PluginConfig::default(),
            auth: AuthConfig::default(),
            events: EventConfig::default(),
            telemetry: crate::telemetry::TelemetryConfig::default(),
            profile: None,
        }
    }
}

impl AppConfig {
    pub fn load_named(
        name: &str,
        path: Option<&Path>,
        layout: &RuntimeLayout,
    ) -> Result<ResolvedAppConfig> {
        validate_config_name(name)?;

        let config_path = if let Some(path) = path {
            if !path.exists() {
                return Err(anyhow!("config file not found: {}", path.display()));
            }
            path.to_path_buf()
        } else {
            let candidate = layout.named_config_path(name);
            if !candidate.exists() {
                return Err(anyhow!(
                    "config '{}' not found at {}",
                    name,
                    candidate.display()
                ));
            }
            candidate
        };

        let mut figment = Figment::from(Serialized::defaults(AppConfig::default()));
        figment = figment.merge(Yaml::file(&config_path));
        figment = figment.merge(Env::prefixed(ENV_PREFIX).split("__"));

        let mut config: AppConfig = figment
            .extract()
            .context("failed to load application config")?;
        config.apply_runtime_defaults(layout, name);
        config.validate()?;

        Ok(ResolvedAppConfig {
            name: name.to_owned(),
            path: config_path,
            config,
        })
    }

    pub fn scaffold(
        app_profile: &str,
        transport: TransportKind,
        endpoint: Option<String>,
        stdio_command: Option<String>,
        stdio_args: Vec<String>,
    ) -> Self {
        let mut config = AppConfig::default();
        config.app.profile = app_profile.to_owned();
        config.server.display_name = format!("{} MCP Server", app_profile);
        config.server.transport = transport.clone();
        config.server.endpoint = match transport {
            TransportKind::Stdio => None,
            TransportKind::StreamableHttp => {
                endpoint.or_else(|| Some("https://demo.invalid/mcp".to_owned()))
            }
        };
        if matches!(transport, TransportKind::Stdio) {
            config.server.stdio.command = stdio_command;
            config.server.stdio.args = stdio_args;
        }
        config
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version == 0 {
            return Err(anyhow!("schema_version must be greater than 0"));
        }
        self.app.validate()?;
        self.server.validate()?;
        self.logging.validate()?;
        Ok(())
    }

    fn apply_runtime_defaults(&mut self, layout: &RuntimeLayout, config_name: &str) {
        if self.auth.token_store_file.is_none() {
            self.auth.token_store_file = Some(
                layout
                    .token_store_path(config_name)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
}

/// Default output format and behavior preferences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DefaultsConfig {
    #[serde(default)]
    pub output: OutputFormat,
    /// Operation timeout in seconds (0 = no timeout). Default: 120.
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            output: OutputFormat::Human,
            timeout_seconds: default_timeout_seconds(),
        }
    }
}

fn default_timeout_seconds() -> u64 {
    120
}

/// Application adapter binding — which built-in profile to use (e.g. `bridge`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppBindingConfig {
    #[serde(default = "default_app_profile")]
    pub profile: String,
}

impl AppBindingConfig {
    fn validate(&self) -> Result<()> {
        if self.profile.trim().is_empty() {
            return Err(anyhow!("app.profile must not be empty"));
        }
        Ok(())
    }
}

impl Default for AppBindingConfig {
    fn default() -> Self {
        Self {
            profile: default_app_profile(),
        }
    }
}

/// MCP server connection settings — transport kind, endpoint, and stdio subprocess config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerConfig {
    #[serde(default = "default_server_display_name")]
    pub display_name: String,
    #[serde(default = "default_transport_kind")]
    pub transport: TransportKind,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub stdio: StdioServerConfig,
}

impl ServerConfig {
    fn validate(&self) -> Result<()> {
        if self.display_name.trim().is_empty() {
            return Err(anyhow!("server.display_name must not be empty"));
        }
        match self.transport {
            TransportKind::StreamableHttp
                if self
                    .endpoint
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty() =>
            {
                return Err(anyhow!(
                    "server.endpoint must be set for streamable_http transport"
                ));
            }
            TransportKind::Stdio => self.stdio.validate()?,
            _ => {}
        }
        Ok(())
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            display_name: default_server_display_name(),
            transport: default_transport_kind(),
            endpoint: Some("https://demo.invalid/mcp".to_owned()),
            stdio: StdioServerConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StdioServerConfig {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl StdioServerConfig {
    pub fn validate(&self) -> Result<()> {
        if self
            .command
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            return Err(anyhow!(
                "server.stdio.command must be set for stdio transport"
            ));
        }
        for value in &self.args {
            if value.trim().is_empty() {
                return Err(anyhow!("server.stdio.args entries must not be empty"));
            }
        }
        if let Some(cwd) = &self.cwd
            && cwd.trim().is_empty()
        {
            return Err(anyhow!("server.stdio.cwd must not be empty when set"));
        }
        for (key, value) in &self.env {
            if key.trim().is_empty() {
                return Err(anyhow!("server.stdio.env keys must not be empty"));
            }
            if value.trim().is_empty() {
                return Err(anyhow!("server.stdio.env values must not be empty"));
            }
        }
        Ok(())
    }
}

/// Legacy plugin config — kept for backward-compatible deserialization of existing config files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PluginConfig {
    #[serde(default)]
    pub search_dirs: Vec<String>,
}

/// Authentication configuration — optional browser-open command for OAuth flows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub browser_open_command: Option<String>,
    #[serde(default)]
    pub token_store_file: Option<String>,
}

/// Event delivery configuration — controls which runtime event sinks are active.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventConfig {
    #[serde(default = "default_enable_stdio_events")]
    pub enable_stdio_events: bool,
    #[serde(default)]
    pub local_socket_path: Option<String>,
    #[serde(default)]
    pub http_endpoint: Option<String>,
    #[serde(default)]
    pub sse_endpoint: Option<String>,
    /// Shell command template executed for each event.
    /// Environment variables: MCP_EVENT_TYPE, MCP_EVENT_JSON, MCP_EVENT_APP_ID, MCP_EVENT_MESSAGE
    #[serde(default)]
    pub command: Option<String>,
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            enable_stdio_events: default_enable_stdio_events(),
            local_socket_path: None,
            http_endpoint: None,
            sse_endpoint: None,
            command: None,
        }
    }
}

/// Logging/tracing configuration — level, format, and output targets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub format: LogFormat,
    #[serde(default = "default_log_outputs")]
    pub outputs: Vec<LogOutput>,
}

impl LoggingConfig {
    pub fn validate(&self) -> Result<()> {
        if self.level.trim().is_empty() {
            return Err(anyhow!("logging.level must not be empty"));
        }
        if self.outputs.is_empty() {
            return Err(anyhow!("logging.outputs must not be empty"));
        }
        for output in &self.outputs {
            if let LogOutput::File { path } = output
                && path.trim().is_empty()
            {
                return Err(anyhow!("logging.outputs file paths must not be empty"));
            }
        }
        Ok(())
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: LogFormat::Pretty,
            outputs: default_log_outputs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    #[default]
    Pretty,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LogOutput {
    Stdout,
    Stderr,
    File { path: String },
}

fn default_schema_version() -> u32 {
    1
}

fn default_app_profile() -> String {
    "bridge".to_owned()
}

fn default_server_display_name() -> String {
    "Demo MCP Bridge Server".to_owned()
}

fn default_transport_kind() -> TransportKind {
    TransportKind::StreamableHttp
}

fn default_enable_stdio_events() -> bool {
    true
}

fn default_log_level() -> String {
    "warn".to_owned()
}

fn default_log_outputs() -> Vec<LogOutput> {
    vec![LogOutput::Stderr]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedAppConfig {
    pub name: String,
    pub path: PathBuf,
    pub config: AppConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamedConfigSummary {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveConfigSelection {
    pub config_name: String,
}

#[derive(Debug, Clone)]
pub struct ConfigCreateOptions {
    pub name: String,
    pub app_profile: String,
    pub transport: TransportKind,
    pub endpoint: Option<String>,
    pub stdio_command: Option<String>,
    pub stdio_args: Vec<String>,
    pub force: bool,
}

/// Platform-resolved directory layout for config, data, and link storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLayout {
    pub config_root: PathBuf,
    pub data_root: PathBuf,
    pub link_root: PathBuf,
}

impl RuntimeLayout {
    pub fn discover() -> Self {
        let project_dirs = ProjectDirs::from("org", "tsok", "mcp2cli");
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));

        let config_root = std::env::var_os("MCP2CLI_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                project_dirs
                    .as_ref()
                    .map(|dirs| dirs.config_dir().to_path_buf())
            })
            .unwrap_or_else(|| home.join(".config/mcp2cli"));
        let data_root = std::env::var_os("MCP2CLI_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                project_dirs
                    .as_ref()
                    .map(|dirs| dirs.data_dir().to_path_buf())
            })
            .unwrap_or_else(|| home.join(".local/share/mcp2cli"));
        let link_root = std::env::var_os("MCP2CLI_BIN_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/bin"));

        Self {
            config_root,
            data_root,
            link_root,
        }
    }

    pub fn configs_dir(&self) -> PathBuf {
        self.config_root.join("configs")
    }

    pub fn named_config_path(&self, name: &str) -> PathBuf {
        self.configs_dir().join(format!("{}.yaml", name))
    }

    pub fn state_file_path(&self, name: &str) -> PathBuf {
        self.data_root
            .join("instances")
            .join(name)
            .join("state.json")
    }

    pub fn token_store_path(&self, name: &str) -> PathBuf {
        self.data_root
            .join("instances")
            .join(name)
            .join("tokens.json")
    }

    pub fn active_config_path(&self) -> PathBuf {
        self.data_root.join("host").join("active-config.json")
    }

    pub fn demo_remote_state_path(&self) -> PathBuf {
        self.data_root.join("demo-remote-state.json")
    }

    pub fn link_dir(&self) -> PathBuf {
        self.link_root.clone()
    }

    /// Default directory for user-level man pages (`man1` section).
    ///
    /// Follows the XDG base-directory convention: the `man/man1` subdirectory
    /// under the XDG data home (typically `~/.local/share/man/man1`).
    /// Modern `man-db` (Linux) and `man` (macOS) search this path without
    /// extra `MANPATH` configuration.
    pub fn man_dir(&self) -> PathBuf {
        // data_root is e.g. ~/.local/share/mcp2cli
        // parent()    is     ~/.local/share
        self.data_root
            .parent()
            .map(|base| base.join("man/man1"))
            .unwrap_or_else(|| PathBuf::from(".local/share/man/man1"))
    }
}

pub fn list_named_configs(layout: &RuntimeLayout) -> Result<Vec<NamedConfigSummary>> {
    let configs_dir = layout.configs_dir();
    if !configs_dir.exists() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&configs_dir)
        .with_context(|| format!("failed to read config directory: {}", configs_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", configs_dir.display()))?;
        let path = entry.path();
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if extension != "yaml" && extension != "yml" {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        items.push(NamedConfigSummary {
            name: name.to_owned(),
            path,
        });
    }
    items.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(items)
}

pub fn write_named_config(
    layout: &RuntimeLayout,
    options: &ConfigCreateOptions,
) -> Result<ResolvedAppConfig> {
    validate_config_name(&options.name)?;

    let path = layout.named_config_path(&options.name);
    if path.exists() && !options.force {
        return Err(anyhow!(
            "config '{}' already exists at {}",
            options.name,
            path.display()
        ));
    }

    let mut config = AppConfig::scaffold(
        &options.app_profile,
        options.transport.clone(),
        options.endpoint.clone(),
        options.stdio_command.clone(),
        options.stdio_args.clone(),
    );
    config.apply_runtime_defaults(layout, &options.name);
    config.validate()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory: {}", parent.display()))?;
    }

    let yaml = serde_yaml::to_string(&config).context("failed to serialize config as yaml")?;
    fs::write(&path, yaml)
        .with_context(|| format!("failed to write config file: {}", path.display()))?;

    Ok(ResolvedAppConfig {
        name: options.name.clone(),
        path,
        config,
    })
}

pub fn load_active_config(layout: &RuntimeLayout) -> Result<Option<ActiveConfigSelection>> {
    let path = layout.active_config_path();
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(&path)
        .with_context(|| format!("failed to read active config file: {}", path.display()))?;
    let selection: ActiveConfigSelection = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse active config file: {}", path.display()))?;
    validate_config_name(&selection.config_name)?;
    Ok(Some(selection))
}

pub fn write_active_config(layout: &RuntimeLayout, name: &str) -> Result<ActiveConfigSelection> {
    validate_config_name(name)?;

    let selection = ActiveConfigSelection {
        config_name: name.to_owned(),
    };
    let path = layout.active_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create active config directory: {}",
                parent.display()
            )
        })?;
    }

    let bytes = serde_json::to_vec_pretty(&selection)
        .context("failed to serialize active config selection")?;
    fs::write(&path, bytes)
        .with_context(|| format!("failed to write active config file: {}", path.display()))?;

    Ok(selection)
}

pub fn clear_active_config(layout: &RuntimeLayout) -> Result<Option<ActiveConfigSelection>> {
    let path = layout.active_config_path();
    let selection = load_active_config(layout)?;
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove active config file: {}", path.display()))?;
    }
    Ok(selection)
}

pub fn active_config_load_status(
    layout: &RuntimeLayout,
) -> Result<Option<(ActiveConfigSelection, Result<ResolvedAppConfig>)>> {
    let Some(selection) = load_active_config(layout)? else {
        return Ok(None);
    };
    let loaded = AppConfig::load_named(&selection.config_name, None, layout);
    Ok(Some((selection, loaded)))
}

/// Validate that a config name contains only safe filesystem characters.
pub fn validate_config_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(anyhow!("config name must not be empty"));
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(anyhow!(
            "config name '{}' may only contain ASCII letters, digits, '-', '_' or '.'",
            name
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tempdir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("mcp2cli-config-tests.")
            .tempdir()
            .expect("tempdir should exist")
    }

    #[test]
    fn writes_and_lists_named_configs() {
        let root = test_tempdir();
        let layout = RuntimeLayout {
            config_root: root.path().join("config"),
            data_root: root.path().join("data"),
            link_root: root.path().join("bin"),
        };

        let created = write_named_config(
            &layout,
            &ConfigCreateOptions {
                name: "work".to_owned(),
                app_profile: "bridge".to_owned(),
                transport: TransportKind::StreamableHttp,
                endpoint: Some("https://example.com/mcp".to_owned()),
                stdio_command: None,
                stdio_args: Vec::new(),
                force: false,
            },
        )
        .expect("config should be created");

        assert_eq!(created.name, "work");
        assert_eq!(created.config.app.profile, "bridge");

        let listed = list_named_configs(&layout).expect("configs should list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "work");
    }

    #[test]
    fn loads_named_config_with_runtime_defaults() {
        let root = test_tempdir();
        let layout = RuntimeLayout {
            config_root: root.path().join("config"),
            data_root: root.path().join("data"),
            link_root: root.path().join("bin"),
        };
        write_named_config(
            &layout,
            &ConfigCreateOptions {
                name: "work-http".to_owned(),
                app_profile: "bridge".to_owned(),
                transport: TransportKind::StreamableHttp,
                endpoint: Some("https://example.com/mcp".to_owned()),
                stdio_command: None,
                stdio_args: Vec::new(),
                force: false,
            },
        )
        .expect("config should be created");

        let loaded = AppConfig::load_named("work-http", None, &layout).expect("config should load");
        assert_eq!(
            loaded.config.auth.token_store_file.as_deref(),
            Some(
                layout
                    .token_store_path("work-http")
                    .to_string_lossy()
                    .as_ref()
            )
        );
        assert_eq!(loaded.config.plugins.search_dirs, Vec::<String>::new());
    }

    #[test]
    fn writes_stdio_config_with_command_and_args() {
        let root = test_tempdir();
        let layout = RuntimeLayout {
            config_root: root.path().join("config"),
            data_root: root.path().join("data"),
            link_root: root.path().join("bin"),
        };

        let created = write_named_config(
            &layout,
            &ConfigCreateOptions {
                name: "work-stdio".to_owned(),
                app_profile: "bridge".to_owned(),
                transport: TransportKind::Stdio,
                endpoint: None,
                stdio_command: Some("npx".to_owned()),
                stdio_args: vec!["@modelcontextprotocol/server-everything".to_owned()],
                force: false,
            },
        )
        .expect("stdio config should be created");

        assert_eq!(created.config.server.transport, TransportKind::Stdio);
        assert_eq!(created.config.server.stdio.command.as_deref(), Some("npx"));
        assert_eq!(
            created.config.server.stdio.args,
            vec!["@modelcontextprotocol/server-everything".to_owned()]
        );
    }

    #[test]
    fn writes_and_loads_active_config_selection() {
        let root = test_tempdir();
        let layout = RuntimeLayout {
            config_root: root.path().join("config"),
            data_root: root.path().join("data"),
            link_root: root.path().join("bin"),
        };

        write_active_config(&layout, "work").expect("active config should be written");

        let loaded = load_active_config(&layout)
            .expect("active config should load")
            .expect("active config should exist");
        assert_eq!(loaded.config_name, "work");
    }

    #[test]
    fn clears_active_config_selection() {
        let root = test_tempdir();
        let layout = RuntimeLayout {
            config_root: root.path().join("config"),
            data_root: root.path().join("data"),
            link_root: root.path().join("bin"),
        };

        write_active_config(&layout, "work").expect("active config should be written");
        let cleared = clear_active_config(&layout)
            .expect("active config should clear")
            .expect("active config should exist before clearing");

        assert_eq!(cleared.config_name, "work");
        assert!(
            load_active_config(&layout)
                .expect("active config load should succeed")
                .is_none()
        );
    }
}
