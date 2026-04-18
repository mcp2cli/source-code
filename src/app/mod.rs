use std::{ffi::OsString, path::PathBuf, sync::Arc};

use anyhow::Result;
use tracing::info;

use crate::{
    apps::bridge,
    config::{
        AppConfig, LoggingConfig, ResolvedAppConfig, RuntimeLayout, ServerConfig,
        StdioServerConfig, load_active_config,
    },
    dispatch::{
        AdHocTransport, DispatchTarget, HOST_BINARY_NAME, Invocation, config_path_from_argv,
        is_host_command, resolve_invocation,
    },
    mcp::client::{McpClient, build_client},
    mcp::model::TransportKind,
    observability::{self, ObservabilityHandle},
    runtime::{RuntimeHost, StateStore, TokenStore},
    telemetry::TelemetryRecorder,
};

#[derive(Clone)]
pub struct AppState {
    pub dispatch_target: DispatchTarget,
    pub invocation: Invocation,
    pub runtime: Arc<RuntimeHost>,
    pub observability: Arc<ObservabilityHandle>,
    pub telemetry: Option<Arc<TelemetryRecorder>>,
}

pub async fn build(argv: Vec<OsString>, config_path: Option<PathBuf>) -> Result<AppState> {
    let invocation = Invocation::capture(argv);
    let layout = RuntimeLayout::discover();
    let config_path = config_path_from_argv(&invocation.argv).or(config_path);

    // Check for --no-telemetry flag early (before full parse)
    if invocation.argv.iter().any(|a| a == "--no-telemetry") {
        TelemetryRecorder::disable_globally();
    }

    let initial_target = resolve_invocation(&invocation);
    let (dispatch_target, selected_config) =
        resolve_runtime_selection(initial_target, &invocation, config_path.as_deref(), &layout)?;

    let fallback_logging = LoggingConfig::default();
    let observability = observability::init(
        selected_config
            .as_ref()
            .map(|config| &config.config.logging)
            .unwrap_or(&fallback_logging),
    )?;

    let state_store = match &selected_config {
        Some(config) => Some(Arc::new(
            StateStore::load(layout.state_file_path(&config.name)).await?,
        )),
        None => None,
    };
    let token_store = selected_config.as_ref().map(|config| {
        Arc::new(TokenStore::new(
            config
                .config
                .auth
                .token_store_file
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| layout.token_store_path(&config.name)),
        ))
    });
    let mcp_client: Arc<dyn McpClient> =
        Arc::from(build_client(&layout, selected_config.as_deref()).await?);
    let runtime = RuntimeHost::new(
        layout.clone(),
        selected_config.clone(),
        state_store,
        token_store,
        mcp_client,
    );

    // Initialize telemetry recorder (if enabled)
    let telemetry_config = selected_config
        .as_ref()
        .map(|c| c.config.telemetry.clone())
        .unwrap_or_default();
    let telemetry = TelemetryRecorder::new(&telemetry_config, &layout.data_root).map(Arc::new);
    if let Some(ref t) = telemetry {
        t.record_first_run();
    }

    info!(
        invoked_as = %invocation.invoked_as,
        config_name = %selected_config.as_ref().map(|config| config.name.as_str()).unwrap_or("<host>"),
        "application bootstrap complete"
    );

    Ok(AppState {
        dispatch_target,
        invocation,
        runtime: Arc::new(runtime),
        observability: Arc::new(observability),
        telemetry,
    })
}

pub async fn run(state: AppState) -> Result<()> {
    let timer = crate::telemetry::start_timer();
    let result = state.runtime.run(state.dispatch_target.clone()).await;

    // Record telemetry event
    if let Some(ref recorder) = state.telemetry {
        let category = dispatch_target_category(&state.dispatch_target);
        let transport = dispatch_target_transport(&state.dispatch_target);
        let outcome = if result.is_ok() { "success" } else { "error" };
        let ad_hoc = matches!(state.dispatch_target, DispatchTarget::AdHoc { .. });
        // These flags require deeper argv inspection but we keep it minimal
        let has_json = state.invocation.argv.iter().any(|a| a == "--json");
        let has_bg = state.invocation.argv.iter().any(|a| a == "--background");
        let has_timeout = state
            .invocation
            .argv
            .iter()
            .any(|a| a.to_str().is_some_and(|s| s.starts_with("--timeout")));

        recorder.record_command(
            category,
            transport,
            has_json,
            has_bg,
            has_timeout,
            false, // profile detection would require config access
            false, // daemon detection would require runtime check
            ad_hoc,
            outcome,
            timer.elapsed(),
        );
    }

    result
}

/// Map dispatch target to telemetry command category.
fn dispatch_target_category(target: &DispatchTarget) -> &str {
    match target {
        DispatchTarget::AppConfig { forwarded_argv, .. } => {
            // Peek at first non-flag arg to determine category
            let first_cmd = forwarded_argv
                .iter()
                .filter_map(|a| a.to_str())
                .find(|s| !s.starts_with('-'));
            match first_cmd {
                Some("ls") => "discover",
                Some("ping") => "ping",
                Some("doctor") => "doctor",
                Some("inspect") => "inspect",
                Some("auth") => "auth",
                Some("jobs") => "jobs",
                Some("log") => "log",
                Some("complete") => "complete",
                Some("subscribe") => "subscribe",
                Some("unsubscribe") => "subscribe",
                Some("get") => "resource_read",
                Some(_) => "command", // tool/prompt/template — we don't log the name
                None => "unknown",
            }
        }
        DispatchTarget::Host { argv, .. } => {
            let first_cmd = argv
                .iter()
                .filter_map(|a| a.to_str())
                .find(|s| !s.starts_with('-'));
            match first_cmd {
                Some("config") => "config",
                Some("link") => "link",
                Some("use") => "use",
                Some("daemon") => "daemon",
                _ => "host",
            }
        }
        DispatchTarget::AdHoc { .. } => "ad_hoc",
        DispatchTarget::McpShim { .. } => "mcp_shim",
    }
}

/// Map dispatch target to transport name for telemetry.
fn dispatch_target_transport(target: &DispatchTarget) -> &str {
    match target {
        DispatchTarget::Host { .. } => "none",
        DispatchTarget::AdHoc { .. } => "ad_hoc",
        DispatchTarget::AppConfig { .. } => "configured",
        DispatchTarget::McpShim { .. } => "mcp_shim",
    }
}

fn missing_active_config_error(config_name: &str, source: &anyhow::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "no active config selected for direct mcp2cli bridge mode, and config '{}' was not found\n\nuse one of:\n  mcp2cli use <name>\n  mcp2cli <name> <command>\n  <name> <command>\n\noriginal lookup error: {}",
        config_name,
        source
    )
}

fn stale_active_config_error(config_name: &str, source: &anyhow::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "active config '{}' is selected but could not be loaded\n\nuse one of:\n  mcp2cli use --show\n  mcp2cli use --clear\n  mcp2cli config init --name {} --app bridge\n\nload error: {}",
        config_name,
        config_name,
        source,
    )
}

fn ambiguous_direct_command_error(
    token: &str,
    active_config_name: &str,
    active_app_id: &str,
) -> anyhow::Error {
    anyhow::anyhow!(
        "config '{}' was not found, and '{}' is not a top-level command for the active app '{}' from config '{}'\n\nif you meant direct bridge mode, try a valid {} command such as:\n  mcp2cli invoke --capability tools.echo --arg message=hello\n  mcp2cli auth status\n\nif you meant explicit config selection, use:\n  mcp2cli <config-name> <command>\n  <config-name> <command>",
        token,
        token,
        active_app_id,
        active_config_name,
        active_app_id,
    )
}

fn resolve_runtime_selection(
    dispatch_target: DispatchTarget,
    invocation: &Invocation,
    config_path: Option<&std::path::Path>,
    layout: &RuntimeLayout,
) -> Result<(DispatchTarget, Option<Arc<ResolvedAppConfig>>)> {
    match dispatch_target {
        DispatchTarget::AdHoc {
            invoked_as,
            forwarded_argv,
            transport,
        } => {
            let (config, config_name) = build_adhoc_config(&transport);
            let resolved = Arc::new(ResolvedAppConfig {
                name: config_name.clone(),
                path: PathBuf::from("<ad-hoc>"),
                config,
            });
            Ok((
                DispatchTarget::AppConfig {
                    config_name,
                    invoked_as,
                    forwarded_argv,
                },
                Some(resolved),
            ))
        }
        DispatchTarget::Host { invoked_as, argv } => {
            if invocation.invoked_as == HOST_BINARY_NAME
                && config_path.is_none()
                && should_route_host_to_active(invocation.argv.as_slice())
                && let Some(selection) = load_active_config(layout)?
            {
                let config = Arc::new(
                    AppConfig::load_named(&selection.config_name, None, layout).map_err(
                        |error| stale_active_config_error(&selection.config_name, &error),
                    )?,
                );
                return Ok((
                    DispatchTarget::AppConfig {
                        config_name: selection.config_name,
                        invoked_as: invocation.invoked_as.clone(),
                        forwarded_argv: invocation.argv.clone(),
                    },
                    Some(config),
                ));
            }

            Ok((DispatchTarget::Host { invoked_as, argv }, None))
        }
        DispatchTarget::AppConfig {
            config_name,
            invoked_as,
            forwarded_argv,
        } => match AppConfig::load_named(&config_name, config_path, layout) {
            Ok(config) => Ok((
                DispatchTarget::AppConfig {
                    config_name,
                    invoked_as,
                    forwarded_argv,
                },
                Some(Arc::new(config)),
            )),
            Err(error) => {
                if invocation.invoked_as != HOST_BINARY_NAME || config_path.is_some() {
                    return Err(error);
                }

                let Some(selection) = load_active_config(layout)? else {
                    return Err(missing_active_config_error(&config_name, &error));
                };

                if selection.config_name == config_name {
                    return Err(stale_active_config_error(&selection.config_name, &error));
                }

                let config = Arc::new(
                    AppConfig::load_named(&selection.config_name, None, layout).map_err(
                        |active_error| {
                            stale_active_config_error(&selection.config_name, &active_error)
                        },
                    )?,
                );
                if !bridge::supports_root_command(&config_name) {
                    return Err(ambiguous_direct_command_error(
                        &config_name,
                        &selection.config_name,
                        &config.config.app.profile,
                    ));
                }
                Ok((
                    DispatchTarget::AppConfig {
                        config_name: selection.config_name,
                        invoked_as: invocation.invoked_as.clone(),
                        forwarded_argv: invocation.argv.clone(),
                    },
                    Some(config),
                ))
            }
        },
        DispatchTarget::McpShim { .. } => {
            // Shims have no config file — the tool cache at
            // `/run/mcp/<server>.json` is what they consult. Pass
            // through unchanged with no config attached.
            Ok((dispatch_target, None))
        }
    }
}

fn should_route_host_to_active(argv: &[OsString]) -> bool {
    let mut index = 1;
    while let Some(value) = argv.get(index) {
        let Some(token) = value.to_str() else {
            return false;
        };

        if let Some(_path) = token.strip_prefix("--config=") {
            index += 1;
            continue;
        }
        if let Some(_output) = token.strip_prefix("--output=") {
            index += 1;
            continue;
        }
        if token == "--json" {
            index += 1;
            continue;
        }
        if matches!(token, "--config" | "--output") {
            index += 2;
            continue;
        }
        if matches!(token, "-h" | "--help" | "-V" | "--version") {
            return true;
        }
        if is_host_command(token) {
            return false;
        }
        if token.starts_with('-') {
            return false;
        }
        return false;
    }

    true
}

/// Build an ephemeral AppConfig from ad-hoc transport flags.
fn build_adhoc_config(transport: &AdHocTransport) -> (AppConfig, String) {
    match transport {
        AdHocTransport::Url(url) => {
            let name = url::Url::parse(url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned))
                .unwrap_or_else(|| "adhoc".to_owned());
            let config = AppConfig {
                server: ServerConfig {
                    display_name: format!("Ad-hoc HTTP: {}", url),
                    transport: TransportKind::StreamableHttp,
                    endpoint: Some(url.clone()),
                    stdio: StdioServerConfig::default(),
                },
                ..AppConfig::default()
            };
            (config, name)
        }
        AdHocTransport::Stdio { command, env } => {
            // Split "python server.py --flag" into command + args
            let parts: Vec<String> = command.split_whitespace().map(str::to_owned).collect();
            let (cmd, args) = match parts.split_first() {
                Some((c, a)) => (c.clone(), a.to_vec()),
                None => (command.clone(), vec![]),
            };
            let name = std::path::Path::new(&cmd)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("adhoc")
                .to_owned();
            let config = AppConfig {
                server: ServerConfig {
                    display_name: format!("Ad-hoc stdio: {}", command),
                    transport: TransportKind::Stdio,
                    endpoint: None,
                    stdio: StdioServerConfig {
                        command: Some(cmd),
                        args,
                        cwd: None,
                        env: env.iter().cloned().collect(),
                    },
                },
                ..AppConfig::default()
            };
            (config, name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigCreateOptions, write_active_config, write_named_config};

    fn test_tempdir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("mcp2cli-app-tests.")
            .tempdir()
            .expect("tempdir should exist")
    }

    #[test]
    fn host_help_uses_active_config_when_selected() {
        let root = test_tempdir();
        let layout = RuntimeLayout {
            config_root: root.path().join("config"),
            data_root: root.path().join("data"),
            link_root: root.path().join("bin"),
        };
        write_named_config(
            &layout,
            &ConfigCreateOptions {
                name: "work".to_owned(),
                app_profile: "bridge".to_owned(),
                transport: crate::mcp::model::TransportKind::StreamableHttp,
                endpoint: Some("https://example.com/mcp".to_owned()),
                stdio_command: None,
                stdio_args: Vec::new(),
                force: false,
            },
        )
        .expect("config should be written");
        write_active_config(&layout, "work").expect("active config should be written");

        let invocation =
            Invocation::capture(vec![OsString::from("mcp2cli"), OsString::from("--help")]);

        let (target, selected) = resolve_runtime_selection(
            DispatchTarget::Host {
                invoked_as: "mcp2cli".to_owned(),
                argv: invocation.argv.clone(),
            },
            &invocation,
            None,
            &layout,
        )
        .expect("runtime selection should succeed");

        assert_eq!(
            target,
            DispatchTarget::AppConfig {
                config_name: "work".to_owned(),
                invoked_as: "mcp2cli".to_owned(),
                forwarded_argv: invocation.argv.clone(),
            }
        );
        assert_eq!(selected.expect("selected config should exist").name, "work");
    }

    #[test]
    fn host_command_stays_in_host_mode_with_active_config() {
        let root = test_tempdir();
        let layout = RuntimeLayout {
            config_root: root.path().join("config"),
            data_root: root.path().join("data"),
            link_root: root.path().join("bin"),
        };
        write_named_config(
            &layout,
            &ConfigCreateOptions {
                name: "work".to_owned(),
                app_profile: "bridge".to_owned(),
                transport: crate::mcp::model::TransportKind::StreamableHttp,
                endpoint: Some("https://example.com/mcp".to_owned()),
                stdio_command: None,
                stdio_args: Vec::new(),
                force: false,
            },
        )
        .expect("config should be written");
        write_active_config(&layout, "work").expect("active config should be written");

        let invocation = Invocation::capture(vec![
            OsString::from("mcp2cli"),
            OsString::from("config"),
            OsString::from("list"),
        ]);

        let (target, selected) = resolve_runtime_selection(
            DispatchTarget::Host {
                invoked_as: "mcp2cli".to_owned(),
                argv: invocation.argv.clone(),
            },
            &invocation,
            None,
            &layout,
        )
        .expect("runtime selection should succeed");

        assert_eq!(
            target,
            DispatchTarget::Host {
                invoked_as: "mcp2cli".to_owned(),
                argv: invocation.argv.clone(),
            }
        );
        assert!(selected.is_none());
    }

    #[test]
    fn direct_bridge_mode_without_active_config_returns_guidance() {
        let root = test_tempdir();
        let layout = RuntimeLayout {
            config_root: root.path().join("config"),
            data_root: root.path().join("data"),
            link_root: root.path().join("bin"),
        };
        let invocation = Invocation::capture(vec![
            OsString::from("mcp2cli"),
            OsString::from("invoke"),
            OsString::from("--capability"),
            OsString::from("tools.echo"),
        ]);

        let error = resolve_runtime_selection(
            DispatchTarget::AppConfig {
                config_name: "invoke".to_owned(),
                invoked_as: "invoke".to_owned(),
                forwarded_argv: vec![
                    OsString::from("invoke"),
                    OsString::from("--capability"),
                    OsString::from("tools.echo"),
                ],
            },
            &invocation,
            None,
            &layout,
        )
        .expect_err("selection should fail without active config");

        let message = error.to_string();
        assert!(message.contains("no active config selected"));
        assert!(message.contains("mcp2cli use <name>"));
    }

    #[test]
    fn ambiguous_direct_command_returns_guidance() {
        let root = test_tempdir();
        let layout = RuntimeLayout {
            config_root: root.path().join("config"),
            data_root: root.path().join("data"),
            link_root: root.path().join("bin"),
        };
        write_named_config(
            &layout,
            &ConfigCreateOptions {
                name: "work".to_owned(),
                app_profile: "bridge".to_owned(),
                transport: crate::mcp::model::TransportKind::StreamableHttp,
                endpoint: Some("https://example.com/mcp".to_owned()),
                stdio_command: None,
                stdio_args: Vec::new(),
                force: false,
            },
        )
        .expect("config should be written");
        write_active_config(&layout, "work").expect("active config should be written");

        let invocation = Invocation::capture(vec![
            OsString::from("mcp2cli"),
            OsString::from("not-a-command"),
            OsString::from("value"),
        ]);

        let error = resolve_runtime_selection(
            DispatchTarget::AppConfig {
                config_name: "not-a-command".to_owned(),
                invoked_as: "not-a-command".to_owned(),
                forwarded_argv: vec![OsString::from("not-a-command"), OsString::from("value")],
            },
            &invocation,
            None,
            &layout,
        )
        .expect_err("selection should fail for an ambiguous token");

        let message = error.to_string();
        assert!(message.contains("not a top-level command"));
    }
}
