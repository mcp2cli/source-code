//! `mcp2cli package` — turn a config into a CLI someone else can install.
//!
//! An mcp2cli alias is personal: it needs mcp2cli installed, a named config in
//! the user's config directory, a symlink, and a first discovery before its
//! commands exist. A *package* is the same binding made distributable. It is a
//! small directory holding
//!
//! - the config, with a [`crate::config::BrandingConfig`] section so the runtime
//!   presents the publisher's identity instead of mcp2cli's;
//! - an [`InventorySnapshot`], so commands and `--help` exist on first run,
//!   offline, and before login;
//! - a launcher that starts the mcp2cli binary with `MCP2CLI_INVOKED_AS` and
//!   `MCP2CLI_CONFIG` pointing at the two files above.
//!
//! The launcher is the only target-specific part: [`PackageTarget::Npm`] emits
//! an npm package whose `bin` resolves the binary through the `mcp2cli` npm
//! package; [`PackageTarget::Shell`] emits a POSIX script for anything that
//! installs files and already has `mcp2cli` on `PATH` (Homebrew, a tarball, a
//! container image).

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde_json::Value;

use crate::{
    config::{
        BUILTIN_COMMANDS, BrandingConfig, ResolvedAppConfig, RuntimeLayout, StdioServerConfig,
        validate_config_name,
    },
    mcp::{
        client::{build_client, perform_with_timeout},
        model::{DiscoveryCategory, McpOperation, McpOperationResult, TransportKind},
    },
    runtime::{EventBroker, InventorySnapshot, SNAPSHOT_SCHEMA_VERSION, StateStore},
};

/// File name of the snapshot inside a package, and the value of
/// `discovery.snapshot` in the packaged config.
pub const SNAPSHOT_FILE_NAME: &str = "inventory.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageTarget {
    Npm,
    Shell,
}

impl PackageTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Shell => "shell",
        }
    }
}

pub struct PackageInitOptions {
    /// Command name of the published CLI.
    pub name: String,
    /// Published package name (npm); defaults to `name`.
    pub package_name: Option<String>,
    pub version: String,
    pub about: Option<String>,
    /// Built-in commands to expose; `None` picks a default from the transport.
    pub builtin_commands: Option<Vec<String>>,
    pub target: PackageTarget,
    pub out_dir: PathBuf,
    pub force: bool,
}

pub struct PackageInitReport {
    pub out_dir: PathBuf,
    pub files: Vec<PathBuf>,
    pub launcher: PathBuf,
    pub builtin_commands: Vec<String>,
    /// Parts of the source config that were left out or will not travel, for
    /// the author to resolve before publishing.
    pub warnings: Vec<String>,
}

/// Capture what `source`'s server offers right now. Falls back to the inventory
/// already cached for that config when the server cannot be reached, so a
/// package can still be refreshed offline from the author's last session.
pub async fn capture_snapshot(
    layout: &RuntimeLayout,
    source: &ResolvedAppConfig,
) -> Result<InventorySnapshot> {
    let layout = layout.for_config(&source.config, &source.name);
    match discover_snapshot(&layout, source).await {
        Ok(snapshot) => Ok(snapshot),
        Err(error) => {
            let cached = StateStore::load(layout.state_file_path(&source.name))
                .await
                .ok();
            let cached = match &cached {
                Some(store) => store.discovery_inventory_view(&source.name).await,
                None => None,
            };
            match cached {
                Some(inventory) => Ok(InventorySnapshot::from_inventory(&inventory)),
                None => Err(error.context(format!(
                    "could not discover the capabilities of config '{}', and none are cached",
                    source.name
                ))),
            }
        }
    }
}

async fn discover_snapshot(
    layout: &RuntimeLayout,
    source: &ResolvedAppConfig,
) -> Result<InventorySnapshot> {
    let client = build_client(layout, Some(source)).await?;
    let events = EventBroker::new(Vec::new());
    let timeout = source.config.defaults.timeout_seconds;

    let mut snapshot = InventorySnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        generated_at: chrono::Utc::now(),
        generator: Some(format!("mcp2cli {}", env!("CARGO_PKG_VERSION"))),
        tools: Vec::new(),
        resources: Vec::new(),
        resource_templates: Vec::new(),
        prompts: Vec::new(),
    };
    let mut first_error = None;
    let mut discovered = false;
    for category in [
        DiscoveryCategory::Capabilities,
        DiscoveryCategory::Resources,
        DiscoveryCategory::Prompts,
    ] {
        let operation = McpOperation::Discover {
            category: category.clone(),
        };
        match perform_with_timeout(
            client.as_ref(),
            &source.name,
            operation,
            &events,
            None,
            timeout,
        )
        .await
        {
            Ok(McpOperationResult::Discovery { items, .. }) => {
                discovered = true;
                match category {
                    DiscoveryCategory::Capabilities => snapshot.tools = items,
                    DiscoveryCategory::Prompts => snapshot.prompts = items,
                    DiscoveryCategory::Resources => {
                        let (templates, resources) = items.into_iter().partition(is_template);
                        snapshot.resource_templates = templates;
                        snapshot.resources = resources;
                    }
                }
            }
            Ok(other) => {
                first_error.get_or_insert_with(|| anyhow!("unexpected MCP response: {:?}", other));
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }

    match first_error {
        Some(error) if !discovered => Err(error),
        _ => Ok(snapshot),
    }
}

fn is_template(item: &Value) -> bool {
    item.get("kind").and_then(Value::as_str) == Some("resource_template")
        || item.get("uriTemplate").is_some()
}

/// Write the package directory. `snapshot` is `None` when the author opted out;
/// the packaged CLI then discovers its commands on first run instead.
pub fn init_package(
    options: &PackageInitOptions,
    source: &ResolvedAppConfig,
    snapshot: Option<&InventorySnapshot>,
) -> Result<PackageInitReport> {
    validate_config_name(&options.name)?;
    if options.name.starts_with("mcp-") || options.name == crate::dispatch::HOST_BINARY_NAME {
        return Err(anyhow!(
            "'{}' cannot be used as a CLI name: 'mcp2cli' and the 'mcp-' prefix are reserved",
            options.name
        ));
    }
    let builtin_commands = match &options.builtin_commands {
        Some(commands) => {
            for command in commands {
                if !BUILTIN_COMMANDS.contains(&command.as_str()) {
                    return Err(anyhow!(
                        "unknown built-in command '{}' (expected one of: {})",
                        command,
                        BUILTIN_COMMANDS.join(", ")
                    ));
                }
            }
            commands.clone()
        }
        // Login is the one built-in most published CLIs need, and it only
        // means something for a remote server.
        None => match source.config.server.transport {
            TransportKind::StreamableHttp => vec!["auth".to_owned()],
            TransportKind::Stdio => Vec::new(),
        },
    };

    let out_dir = &options.out_dir;
    if out_dir.exists() && !options.force {
        let occupied = fs::read_dir(out_dir)
            .with_context(|| format!("failed to read {}", out_dir.display()))?
            .next()
            .is_some();
        if occupied {
            return Err(anyhow!(
                "{} already exists and is not empty; pass --force to overwrite its files",
                out_dir.display()
            ));
        }
    }

    let config_file = format!("{}.yaml", options.name);
    let about = options
        .about
        .clone()
        .unwrap_or_else(|| source.config.server.display_name.clone());
    let mut files = vec![(
        PathBuf::from(&config_file),
        packaged_config_yaml(
            options,
            source,
            &about,
            &builtin_commands,
            snapshot.is_some(),
        )?,
    )];
    if let Some(snapshot) = snapshot {
        let mut json = serde_json::to_string_pretty(snapshot)
            .context("failed to serialize inventory snapshot")?;
        json.push('\n');
        files.push((PathBuf::from(SNAPSHOT_FILE_NAME), json));
    }

    let launcher = match options.target {
        PackageTarget::Npm => {
            let launcher = PathBuf::from("bin").join(format!("{}.js", options.name));
            files.push((launcher.clone(), npm_launcher(&options.name, &config_file)));
            files.push((
                PathBuf::from("package.json"),
                npm_package_json(options, &about, &config_file, snapshot.is_some())?,
            ));
            launcher
        }
        PackageTarget::Shell => {
            let launcher = PathBuf::from("bin").join(&options.name);
            files.push((
                launcher.clone(),
                shell_launcher(&options.name, &config_file),
            ));
            launcher
        }
    };
    files.push((
        PathBuf::from("README.md"),
        package_readme(options, source, &about, snapshot.is_some()),
    ));

    let mut written = Vec::new();
    for (relative, content) in files {
        let path = out_dir.join(&relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
        written.push(path);
    }
    let launcher = out_dir.join(launcher);
    make_executable(&launcher)?;

    Ok(PackageInitReport {
        out_dir: out_dir.clone(),
        files: written,
        launcher,
        builtin_commands,
        warnings: portability_warnings(source, &config_file),
    })
}

/// What in the source config is tied to the author's machine. A package is
/// published, so a secret or a home-directory path in it is a leak as well as a
/// bug — none of these are copied, and each is reported instead.
fn portability_warnings(source: &ResolvedAppConfig, config_file: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    if source.config.server.transport != TransportKind::Stdio {
        return warnings;
    }
    let stdio = &source.config.server.stdio;
    if !stdio.env.is_empty() {
        let names = stdio.env.keys().cloned().collect::<Vec<_>>().join(", ");
        warnings.push(format!(
            "server.stdio.env was not copied, because it may hold secrets ({names}). \
             Users must provide these themselves; add only non-secret values to {config_file}."
        ));
    }
    if let Some(cwd) = &stdio.cwd {
        warnings.push(format!(
            "server.stdio.cwd ({cwd}) was not copied: it is a path on this machine."
        ));
    }
    let local_paths = stdio
        .command
        .iter()
        .chain(&stdio.args)
        .filter(|value| Path::new(value).is_absolute())
        .cloned()
        .collect::<Vec<_>>();
    if !local_paths.is_empty() {
        warnings.push(format!(
            "server.stdio refers to absolute paths that will not exist on a user's machine: {}. \
             Point {config_file} at something installable instead, e.g. `npx -y <your-server>`.",
            local_paths.join(", ")
        ));
    }
    warnings
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("failed to mark {} executable", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// The packaged config: only what describes the server and the CLI. Anything
/// resolved against the author's machine (token store path, logging outputs,
/// event sinks) is left to the runtime's defaults on the user's machine.
///
/// Typed rather than assembled as a JSON value so the file reads in a
/// deliberate order: the server first, then how the CLI presents itself.
#[derive(Serialize)]
struct PackagedConfig<'a> {
    schema_version: u32,
    server: PackagedServer<'a>,
    defaults: PackagedDefaults,
    events: PackagedEvents,
    #[serde(skip_serializing_if = "Option::is_none")]
    discovery: Option<PackagedDiscovery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<&'a crate::apps::manifest::ProfileOverlay>,
    branding: BrandingConfig,
}

#[derive(Serialize)]
struct PackagedServer<'a> {
    display_name: &'a str,
    transport: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdio: Option<PackagedStdio<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    protocol_version: Option<&'a str>,
}

// No `env` and no `cwd`: the first is where API keys live, the second is a path
// on the author's machine. `portability_warnings` tells the author about both.
#[derive(Serialize)]
struct PackagedStdio<'a> {
    command: Option<&'a str>,
    args: &'a [String],
}

#[derive(Serialize)]
struct PackagedDefaults {
    timeout_seconds: u64,
}

#[derive(Serialize)]
struct PackagedEvents {
    enable_stdio_events: bool,
}

#[derive(Serialize)]
struct PackagedDiscovery {
    snapshot: &'static str,
}

fn packaged_config_yaml(
    options: &PackageInitOptions,
    source: &ResolvedAppConfig,
    about: &str,
    builtin_commands: &[String],
    has_snapshot: bool,
) -> Result<String> {
    let server = &source.config.server;
    let stdio: &StdioServerConfig = &server.stdio;
    let config = PackagedConfig {
        schema_version: source.config.schema_version,
        server: PackagedServer {
            display_name: &server.display_name,
            transport: server.transport.as_str(),
            endpoint: match server.transport {
                TransportKind::StreamableHttp => server.endpoint.as_deref(),
                TransportKind::Stdio => None,
            },
            stdio: match server.transport {
                TransportKind::Stdio => Some(PackagedStdio {
                    command: stdio.command.as_deref(),
                    args: &stdio.args,
                }),
                TransportKind::StreamableHttp => None,
            },
            protocol_version: server.protocol_version.as_deref(),
        },
        defaults: PackagedDefaults {
            timeout_seconds: source.config.defaults.timeout_seconds,
        },
        // Progress lines such as "[name] invoking capability …" are mcp2cli's
        // voice, not the published CLI's.
        events: PackagedEvents {
            enable_stdio_events: false,
        },
        discovery: has_snapshot.then_some(PackagedDiscovery {
            snapshot: SNAPSHOT_FILE_NAME,
        }),
        profile: source.config.profile.as_ref(),
        branding: BrandingConfig {
            name: Some(options.name.clone()),
            about: Some(about.to_owned()),
            // The npm launcher passes the package.json version at run time, so
            // the two cannot drift; a shell package has nowhere else to keep it.
            version: (options.target == PackageTarget::Shell).then(|| options.version.clone()),
            after_help: None,
            builtin_commands: Some(builtin_commands.to_vec()),
            attribution: false,
        },
    };

    let yaml = serde_yaml::to_string(&config).context("failed to serialize packaged config")?;
    Ok(format!(
        "# Configuration of the `{name}` CLI — generated by `mcp2cli package init`.\n\
         # Reference: https://mcp2cli.dev/docs/features/branded-cli\n\
         {yaml}",
        name = options.name,
    ))
}

#[derive(Serialize)]
struct NpmPackage<'a> {
    name: &'a str,
    version: &'a str,
    description: &'a str,
    bin: BTreeMap<&'a str, String>,
    files: Vec<&'a str>,
    scripts: BTreeMap<&'static str, String>,
    dependencies: BTreeMap<&'static str, String>,
    engines: BTreeMap<&'static str, &'static str>,
}

fn npm_package_json(
    options: &PackageInitOptions,
    about: &str,
    config_file: &str,
    has_snapshot: bool,
) -> Result<String> {
    let mut files = vec!["bin", config_file];
    if has_snapshot {
        files.push(SNAPSHOT_FILE_NAME);
    }
    let package = NpmPackage {
        name: options.package_name.as_deref().unwrap_or(&options.name),
        version: &options.version,
        description: about,
        bin: BTreeMap::from([(options.name.as_str(), format!("bin/{}.js", options.name))]),
        files,
        scripts: BTreeMap::from([(
            "snapshot",
            format!(
                "mcp2cli package snapshot --config {} --out {}",
                config_file, SNAPSHOT_FILE_NAME
            ),
        )]),
        dependencies: BTreeMap::from([("mcp2cli", format!("^{}", env!("CARGO_PKG_VERSION")))]),
        engines: BTreeMap::from([("node", ">=18")]),
    };
    let mut json =
        serde_json::to_string_pretty(&package).context("failed to serialize package.json")?;
    json.push('\n');
    Ok(json)
}

fn npm_launcher(name: &str, config_file: &str) -> String {
    format!(
        r#"#!/usr/bin/env node
"use strict";

// Launcher for the `{name}` CLI. The `mcp2cli` package resolves the native
// runtime for this platform; this file only tells it who it is running as.
const path = require("node:path");
const {{ run }} = require("mcp2cli");
const pkg = require("../package.json");

run({{
  name: "{name}",
  config: path.join(__dirname, "..", "{config_file}"),
  version: pkg.version,
}});
"#
    )
}

fn shell_launcher(name: &str, config_file: &str) -> String {
    format!(
        r#"#!/bin/sh
# Launcher for the `{name}` CLI. Requires `mcp2cli` on PATH.
set -e

self="$0"
while [ -L "$self" ]; do
  target="$(readlink "$self")"
  case "$target" in
    /*) self="$target" ;;
    *) self="$(dirname "$self")/$target" ;;
  esac
done
root="$(cd "$(dirname "$self")/.." && pwd)"

MCP2CLI_INVOKED_AS="{name}" MCP2CLI_CONFIG="$root/{config_file}" exec mcp2cli "$@"
"#
    )
}

fn package_readme(
    options: &PackageInitOptions,
    source: &ResolvedAppConfig,
    about: &str,
    has_snapshot: bool,
) -> String {
    let name = &options.name;
    let package_name = options.package_name.as_deref().unwrap_or(name);
    let (install, try_it, publish) = match options.target {
        PackageTarget::Npm => (
            format!("```sh\nnpm install -g {package_name}\n# or, without installing:\nnpx {package_name} --help\n```"),
            format!("```sh\nnpm install\nnode bin/{name}.js --help\n```"),
            "```sh\nnpm publish --access public\n```".to_owned(),
        ),
        PackageTarget::Shell => (
            format!("Install [mcp2cli](https://mcp2cli.dev), then put `bin/{name}` on your `PATH`\n(keep it next to `{name}.yaml`; a symlink to it works too)."),
            format!("```sh\n./bin/{name} --help\n```"),
            "Ship this directory as-is — a tarball, a Homebrew formula, or a layer in a\ncontainer image — alongside the `mcp2cli` binary.".to_owned(),
        ),
    };
    let login = if source.config.server.transport == TransportKind::StreamableHttp {
        format!(
            "\n## Signing in\n\n```sh\n{name} auth login     # opens the browser, or prompts for a token\n{name} auth status\n{name} auth logout\n```\n\nCredentials are stored per user, in `{name}`'s own data directory.\n"
        )
    } else {
        String::new()
    };
    let snapshot = if has_snapshot {
        format!(
            "\n## Keeping the command list current\n\n`{SNAPSHOT_FILE_NAME}` is a snapshot of what the server offered when this package was\nbuilt. It is what makes `{name} --help` instant, offline, and available before\nlogin. Refresh it whenever the server's tools change, then publish a new version:\n\n```sh\nmcp2cli package snapshot --config {name}.yaml --out {SNAPSHOT_FILE_NAME}\n```\n"
        )
    } else {
        String::new()
    };

    format!(
        "# {name}\n\n{about}\n\n## Install\n\n{install}\n\n## Usage\n\n```sh\n{name} --help\n{name} <command> --help\n```\n{login}{snapshot}\n## Developing this package\n\nTry it from this directory:\n\n{try_it}\n\n`{name}.yaml` holds the server binding and everything about how the CLI presents\nitself (`branding`, `profile`). See\n<https://mcp2cli.dev/docs/features/branded-cli> for every option.\n\n## Publishing\n\n{publish}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use serde_json::json;

    fn source(transport: TransportKind) -> ResolvedAppConfig {
        let mut config = AppConfig::scaffold(
            "bridge",
            transport,
            Some("https://mcp.example.com/email".to_owned()),
            Some("npx".to_owned()),
            vec!["@acme/email-mcp".to_owned()],
        );
        config.server.display_name = "Acme Mail".to_owned();
        // Resolved on the author's machine; must never reach the package.
        config.auth.token_store_file = Some("/home/author/.local/share/tokens.json".to_owned());
        ResolvedAppConfig {
            name: "email".to_owned(),
            path: PathBuf::from("/home/author/.config/mcp2cli/configs/email.yaml"),
            config,
        }
    }

    fn options(target: PackageTarget, out_dir: &Path) -> PackageInitOptions {
        PackageInitOptions {
            name: "email".to_owned(),
            package_name: Some("@acme/email-cli".to_owned()),
            version: "1.4.0".to_owned(),
            about: None,
            builtin_commands: None,
            target,
            out_dir: out_dir.to_path_buf(),
            force: false,
        }
    }

    fn empty_snapshot() -> InventorySnapshot {
        InventorySnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            generated_at: chrono::Utc::now(),
            generator: None,
            tools: vec![json!({"id": "send", "kind": "tool"})],
            resources: Vec::new(),
            resource_templates: Vec::new(),
            prompts: Vec::new(),
        }
    }

    #[test]
    fn npm_package_loads_as_a_branded_config_and_carries_nothing_from_the_author() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let out = dir.path().join("email-cli");
        let report = init_package(
            &options(PackageTarget::Npm, &out),
            &source(TransportKind::StreamableHttp),
            Some(&empty_snapshot()),
        )
        .expect("package should be written");

        assert_eq!(report.builtin_commands, vec!["auth".to_owned()]);
        let yaml = fs::read_to_string(out.join("email.yaml")).expect("config should exist");
        assert!(!yaml.contains("/home/author"), "{yaml}");

        // The generated file is a valid config for the runtime that will load it.
        let layout = RuntimeLayout {
            config_root: dir.path().join("config"),
            data_root: dir.path().join("data"),
            link_root: dir.path().join("bin"),
        };
        let loaded = AppConfig::load_named("email", Some(&out.join("email.yaml")), &layout)
            .expect("generated config should load");
        let branding = loaded.config.branding.expect("package must be branded");
        assert_eq!(branding.name.as_deref(), Some("email"));
        assert_eq!(branding.about.as_deref(), Some("Acme Mail"));
        assert!(!loaded.config.telemetry.enabled);
        assert!(!loaded.config.events.enable_stdio_events);
        assert_eq!(
            loaded.config.discovery.snapshot.as_deref(),
            out.join(SNAPSHOT_FILE_NAME).to_str()
        );

        let package: Value = serde_json::from_str(
            &fs::read_to_string(out.join("package.json")).expect("package.json should exist"),
        )
        .expect("package.json should be JSON");
        assert_eq!(package["name"], "@acme/email-cli");
        assert_eq!(package["bin"]["email"], "bin/email.js");
        assert!(package["dependencies"]["mcp2cli"].is_string());
        assert!(out.join("bin/email.js").exists());
    }

    #[test]
    fn shell_package_for_a_stdio_server_exposes_no_builtins_by_default() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let out = dir.path().join("email-cli");
        let report = init_package(
            &options(PackageTarget::Shell, &out),
            &source(TransportKind::Stdio),
            None,
        )
        .expect("package should be written");

        assert!(report.builtin_commands.is_empty());
        assert!(out.join("bin/email").exists());
        assert!(!out.join("package.json").exists());
        assert!(!out.join(SNAPSHOT_FILE_NAME).exists());
        let yaml = fs::read_to_string(out.join("email.yaml")).expect("config should exist");
        assert!(yaml.contains("version: 1.4.0"), "{yaml}");
        assert!(yaml.contains("@acme/email-mcp"), "{yaml}");
        assert!(!yaml.contains("snapshot"), "{yaml}");
    }

    #[test]
    fn secrets_and_local_paths_in_a_stdio_config_are_reported_not_shipped() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let out = dir.path().join("email-cli");
        let mut source = source(TransportKind::Stdio);
        source.config.server.stdio.command = Some("/home/author/bin/email-mcp".to_owned());
        source.config.server.stdio.cwd = Some("/home/author/work".to_owned());
        source
            .config
            .server
            .stdio
            .env
            .insert("EMAIL_API_KEY".to_owned(), "sk-live-SECRET".to_owned());

        let report = init_package(&options(PackageTarget::Npm, &out), &source, None)
            .expect("package should be written");

        for file in &report.files {
            let content = fs::read_to_string(file).expect("generated file should be readable");
            assert!(
                !content.contains("sk-live-SECRET"),
                "{} leaks the key",
                file.display()
            );
            assert!(
                !content.contains("/home/author/work"),
                "{} leaks cwd",
                file.display()
            );
        }
        let warnings = report.warnings.join("\n");
        assert!(warnings.contains("EMAIL_API_KEY"), "{warnings}");
        assert!(warnings.contains("server.stdio.cwd"), "{warnings}");
        assert!(
            warnings.contains("/home/author/bin/email-mcp"),
            "{warnings}"
        );
    }

    #[test]
    fn refuses_to_overwrite_a_populated_directory_without_force() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let out = dir.path().join("email-cli");
        fs::create_dir_all(&out).expect("dir should be created");
        fs::write(out.join("keep.txt"), "mine").expect("file should be written");

        let error = init_package(
            &options(PackageTarget::Npm, &out),
            &source(TransportKind::StreamableHttp),
            None,
        )
        .err()
        .expect("populated directory must be refused");
        assert!(error.to_string().contains("--force"), "{error}");
    }

    #[test]
    fn rejects_reserved_cli_names() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let mut options = options(PackageTarget::Npm, &dir.path().join("out"));
        options.name = "mcp-email-send".to_owned();
        let error = init_package(&options, &source(TransportKind::StreamableHttp), None)
            .err()
            .expect("shim-shaped names must be rejected");
        assert!(error.to_string().contains("reserved"), "{error}");
    }
}
