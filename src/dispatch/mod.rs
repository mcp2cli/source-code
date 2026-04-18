use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Captured CLI invocation with the raw argv and the resolved binary stem name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub argv: Vec<OsString>,
    pub invoked_as: String,
}

impl Invocation {
    pub fn capture(argv: Vec<OsString>) -> Self {
        let invoked_as = argv
            .first()
            .and_then(|value| Path::new(value).file_stem())
            .and_then(|value| value.to_str())
            .unwrap_or("mcp2cli")
            .to_owned();
        Self { argv, invoked_as }
    }
}

/// Routing decision: either an app-config-driven bridge command or a host-level administrative command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchTarget {
    AppConfig {
        config_name: String,
        invoked_as: String,
        forwarded_argv: Vec<OsString>,
    },
    Host {
        invoked_as: String,
        argv: Vec<OsString>,
    },
    /// Ad-hoc connection: no config file, just a URL or stdio command.
    AdHoc {
        invoked_as: String,
        forwarded_argv: Vec<OsString>,
        transport: AdHocTransport,
    },
    /// MCP shim dispatch. Invoked when argv[0] matches
    /// `mcp-<server>-<tool>`. The shim reads
    /// `$MCP_CACHE_DIR/<server>.json` (default `/run/mcp/`) for the
    /// tool schema + VSOCK port and dials the MCP bridge.
    McpShim {
        server: String,
        tool: String,
        invoked_as: String,
        /// argv stripped of `argv[0]`; passed to the eventual MCP tool.
        forwarded_argv: Vec<OsString>,
    },
}

/// Parse a shim filename `mcp-<server>-<tool>` into its two parts.
///
/// The parse rule is **first dash after the `mcp-` prefix** splits
/// server from tool. Tools may themselves contain dashes
/// (`mcp-demo-read-file` → `("demo", "read-file")`), but server names
/// may not.
///
/// Returns `None` if `name` does not start with `mcp-` or doesn't have
/// a server+tool separator.
pub fn parse_mcp_shim_name(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix("mcp-")?;
    let (server, tool) = rest.split_once('-')?;
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server.to_string(), tool.to_string()))
}

/// Ad-hoc transport specification from CLI flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdHocTransport {
    Url(String),
    Stdio {
        command: String,
        env: Vec<(String, String)>,
    },
}

pub const HOST_BINARY_NAME: &str = "mcp2cli";

pub fn is_host_command(value: &str) -> bool {
    matches!(value, "config" | "link" | "use" | "daemon")
}

/// Resolve an invocation into a dispatch target based on the binary name, argv tokens, and flag layout.
pub fn resolve_invocation(invocation: &Invocation) -> DispatchTarget {
    // Check for ad-hoc flags first (--url or --stdio)
    if let Some(adhoc) = extract_adhoc_transport(&invocation.argv) {
        let forwarded_argv = strip_adhoc_flags(&invocation.argv);
        return DispatchTarget::AdHoc {
            invoked_as: invocation.invoked_as.clone(),
            forwarded_argv,
            transport: adhoc,
        };
    }

    // If argv[0] is an `mcp-<server>-<tool>` symlink, route to
    // the MCP shim before falling back to the app-config path. This
    // takes precedence over config-named symlinks so an operator
    // can't accidentally shadow the shim namespace with a config file.
    if let Some((server, tool)) = parse_mcp_shim_name(&invocation.invoked_as) {
        let forwarded_argv = invocation.argv.iter().skip(1).cloned().collect();
        return DispatchTarget::McpShim {
            server,
            tool,
            invoked_as: invocation.invoked_as.clone(),
            forwarded_argv,
        };
    }

    if invocation.invoked_as != HOST_BINARY_NAME {
        return DispatchTarget::AppConfig {
            config_name: invocation.invoked_as.clone(),
            invoked_as: invocation.invoked_as.clone(),
            forwarded_argv: invocation.argv.clone(),
        };
    }

    if let Some((index, selector)) = find_selector(invocation.argv.as_slice())
        && !is_host_command(selector)
    {
        let mut forwarded_argv = Vec::with_capacity(invocation.argv.len());
        forwarded_argv.push(OsString::from(selector));
        forwarded_argv.extend(invocation.argv.iter().skip(1).take(index - 1).cloned());
        forwarded_argv.extend(invocation.argv.iter().skip(index + 1).cloned());

        return DispatchTarget::AppConfig {
            config_name: selector.to_owned(),
            invoked_as: selector.to_owned(),
            forwarded_argv,
        };
    }

    DispatchTarget::Host {
        invoked_as: invocation.invoked_as.clone(),
        argv: invocation.argv.clone(),
    }
}

/// Extract a `--config <path>` or `--config=<path>` value from the raw argv.
pub fn config_path_from_argv(argv: &[OsString]) -> Option<PathBuf> {
    let mut iter = argv.iter().peekable();
    while let Some(value) = iter.next() {
        if let Some(as_str) = value.to_str() {
            if let Some(path) = as_str.strip_prefix("--config=") {
                return Some(PathBuf::from(path));
            }
            if as_str == "--config" {
                return iter.next().map(PathBuf::from);
            }
        }
    }
    None
}

fn find_selector(argv: &[OsString]) -> Option<(usize, &str)> {
    let mut index = 1;
    while let Some(value) = argv.get(index) {
        let token = value.to_str()?;
        if matches!(token, "-h" | "--help" | "-V" | "--version") {
            return None;
        }
        if token.strip_prefix("--config=").is_some() {
            index += 1;
            continue;
        }
        if token.strip_prefix("--output=").is_some() {
            index += 1;
            continue;
        }
        if matches!(token, "--json") {
            index += 1;
            continue;
        }
        if matches!(token, "--config" | "--output") {
            index += 2;
            continue;
        }
        if token.starts_with('-') {
            return None;
        }
        return Some((index, token));
    }
    None
}

/// Extract an ad-hoc transport from `--url <URL>` or `--stdio <COMMAND>` flags.
fn extract_adhoc_transport(argv: &[OsString]) -> Option<AdHocTransport> {
    let mut iter = argv.iter().peekable();
    let mut url = None;
    let mut stdio_cmd = None;
    let mut env_pairs = Vec::new();

    while let Some(value) = iter.next() {
        let Some(token) = value.to_str() else {
            continue;
        };
        match token {
            "--url" => {
                url = iter.next().and_then(|v| v.to_str().map(str::to_owned));
            }
            _ if token.starts_with("--url=") => {
                url = token.strip_prefix("--url=").map(str::to_owned);
            }
            "--stdio" => {
                stdio_cmd = iter.next().and_then(|v| v.to_str().map(str::to_owned));
            }
            _ if token.starts_with("--stdio=") => {
                stdio_cmd = token.strip_prefix("--stdio=").map(str::to_owned);
            }
            "--env" => {
                if let Some(pair) = iter.next().and_then(|v| v.to_str())
                    && let Some((k, v)) = pair.split_once('=')
                {
                    env_pairs.push((k.to_owned(), v.to_owned()));
                }
            }
            _ if token.starts_with("--env=") => {
                if let Some(pair) = token.strip_prefix("--env=")
                    && let Some((k, v)) = pair.split_once('=')
                {
                    env_pairs.push((k.to_owned(), v.to_owned()));
                }
            }
            _ => {}
        }
    }

    if let Some(url) = url {
        Some(AdHocTransport::Url(url))
    } else {
        stdio_cmd.map(|command| AdHocTransport::Stdio {
            command,
            env: env_pairs,
        })
    }
}

/// Strip --url, --stdio, and --env flags from argv so remaining args can be re-parsed as commands.
fn strip_adhoc_flags(argv: &[OsString]) -> Vec<OsString> {
    let mut result = Vec::new();
    let mut iter = argv.iter().peekable();
    while let Some(value) = iter.next() {
        let token = value.to_str().unwrap_or("");
        // Skip --url/--stdio/--env and their values
        if matches!(token, "--url" | "--stdio" | "--env") {
            iter.next(); // skip the value
            continue;
        }
        if token.starts_with("--url=")
            || token.starts_with("--stdio=")
            || token.starts_with("--env=")
        {
            continue;
        }
        result.push(value.clone());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_link_name_to_named_config() {
        let invocation = Invocation::capture(vec![
            OsString::from("work"),
            OsString::from("invoke"),
            OsString::from("--capability"),
            OsString::from("tools.echo"),
        ]);

        let target = resolve_invocation(&invocation);

        assert_eq!(
            target,
            DispatchTarget::AppConfig {
                config_name: "work".to_owned(),
                invoked_as: "work".to_owned(),
                forwarded_argv: vec![
                    OsString::from("work"),
                    OsString::from("invoke"),
                    OsString::from("--capability"),
                    OsString::from("tools.echo"),
                ],
            }
        );
    }

    #[test]
    fn resolves_direct_host_invocation_with_selector() {
        let invocation = Invocation::capture(vec![
            OsString::from("mcp2cli"),
            OsString::from("work"),
            OsString::from("invoke"),
            OsString::from("--capability"),
            OsString::from("tools.echo"),
            OsString::from("--arg"),
            OsString::from("message=Hello"),
        ]);

        let target = resolve_invocation(&invocation);

        assert_eq!(
            target,
            DispatchTarget::AppConfig {
                config_name: "work".to_owned(),
                invoked_as: "work".to_owned(),
                forwarded_argv: vec![
                    OsString::from("work"),
                    OsString::from("invoke"),
                    OsString::from("--capability"),
                    OsString::from("tools.echo"),
                    OsString::from("--arg"),
                    OsString::from("message=Hello"),
                ],
            }
        );
    }

    #[test]
    fn keeps_reserved_host_command_in_host_mode() {
        let invocation = Invocation::capture(vec![
            OsString::from("mcp2cli"),
            OsString::from("config"),
            OsString::from("list"),
        ]);

        let target = resolve_invocation(&invocation);

        assert_eq!(
            target,
            DispatchTarget::Host {
                invoked_as: "mcp2cli".to_owned(),
                argv: vec![
                    OsString::from("mcp2cli"),
                    OsString::from("config"),
                    OsString::from("list"),
                ],
            }
        );
    }

    #[test]
    fn keeps_use_in_host_mode() {
        let invocation = Invocation::capture(vec![
            OsString::from("mcp2cli"),
            OsString::from("use"),
            OsString::from("work"),
        ]);

        let target = resolve_invocation(&invocation);

        assert_eq!(
            target,
            DispatchTarget::Host {
                invoked_as: "mcp2cli".to_owned(),
                argv: vec![
                    OsString::from("mcp2cli"),
                    OsString::from("use"),
                    OsString::from("work"),
                ],
            }
        );
    }

    #[test]
    fn resolves_url_adhoc_to_adhoc_dispatch() {
        let invocation = Invocation::capture(vec![
            OsString::from("mcp2cli"),
            OsString::from("--url"),
            OsString::from("https://mcp.example.com/mcp"),
            OsString::from("discover"),
        ]);

        let target = resolve_invocation(&invocation);

        match target {
            DispatchTarget::AdHoc {
                transport,
                forwarded_argv,
                ..
            } => {
                assert_eq!(
                    transport,
                    AdHocTransport::Url("https://mcp.example.com/mcp".to_owned())
                );
                // --url and its value should be stripped from forwarded_argv
                assert!(!forwarded_argv.iter().any(|v| v == "--url"));
                assert!(forwarded_argv.iter().any(|v| v == "discover"));
            }
            other => panic!("expected AdHoc, got {:?}", other),
        }
    }

    #[test]
    fn resolves_stdio_adhoc_to_adhoc_dispatch() {
        let invocation = Invocation::capture(vec![
            OsString::from("mcp2cli"),
            OsString::from("--stdio"),
            OsString::from("npx @modelcontextprotocol/server"),
            OsString::from("--env"),
            OsString::from("API_KEY=test"),
            OsString::from("discover"),
        ]);

        let target = resolve_invocation(&invocation);

        match target {
            DispatchTarget::AdHoc {
                transport,
                forwarded_argv,
                ..
            } => {
                assert_eq!(
                    transport,
                    AdHocTransport::Stdio {
                        command: "npx @modelcontextprotocol/server".to_owned(),
                        env: vec![("API_KEY".to_owned(), "test".to_owned())],
                    }
                );
                assert!(forwarded_argv.iter().any(|v| v == "discover"));
                assert!(!forwarded_argv.iter().any(|v| v == "--stdio"));
                assert!(!forwarded_argv.iter().any(|v| v == "--env"));
            }
            other => panic!("expected AdHoc, got {:?}", other),
        }
    }

    #[test]
    fn daemon_is_host_command() {
        assert!(is_host_command("daemon"));
    }

    // ---------------- shim parser tests ----------------

    #[test]
    fn parse_mcp_shim_splits_on_first_dash_after_prefix() {
        assert_eq!(
            parse_mcp_shim_name("mcp-demo-read-file"),
            Some(("demo".into(), "read-file".into()))
        );
        assert_eq!(
            parse_mcp_shim_name("mcp-git-log"),
            Some(("git".into(), "log".into()))
        );
        assert_eq!(
            parse_mcp_shim_name("mcp-foo-do-thing-now"),
            Some(("foo".into(), "do-thing-now".into()))
        );
    }

    #[test]
    fn parse_mcp_shim_rejects_non_mcp_names() {
        assert_eq!(parse_mcp_shim_name("mcp2cli"), None);
        assert_eq!(parse_mcp_shim_name("work"), None);
        assert_eq!(parse_mcp_shim_name("amcp-demo-read"), None);
    }

    #[test]
    fn parse_mcp_shim_rejects_malformed_shapes() {
        // No tool segment.
        assert_eq!(parse_mcp_shim_name("mcp-demo"), None);
        // Empty server.
        assert_eq!(parse_mcp_shim_name("mcp--tool"), None);
        // Bare prefix.
        assert_eq!(parse_mcp_shim_name("mcp-"), None);
    }

    // ---------------- shim resolver tests ----------------

    #[test]
    fn resolves_mcp_shim_invocation() {
        let invocation = Invocation::capture(vec![
            OsString::from("mcp-demo-read-file"),
            OsString::from("/workspace/README.md"),
        ]);
        let target = resolve_invocation(&invocation);
        assert_eq!(
            target,
            DispatchTarget::McpShim {
                server: "demo".to_owned(),
                tool: "read-file".to_owned(),
                invoked_as: "mcp-demo-read-file".to_owned(),
                forwarded_argv: vec![OsString::from("/workspace/README.md")],
            }
        );
    }

    #[test]
    fn resolves_mcp_shim_with_multi_segment_tool() {
        let invocation = Invocation::capture(vec![
            OsString::from("mcp-foo-do-thing-now"),
            OsString::from("--flag"),
        ]);
        let target = resolve_invocation(&invocation);
        match target {
            DispatchTarget::McpShim {
                server,
                tool,
                forwarded_argv,
                ..
            } => {
                assert_eq!(server, "foo");
                assert_eq!(tool, "do-thing-now");
                assert_eq!(forwarded_argv, vec![OsString::from("--flag")]);
            }
            other => panic!("expected McpShim, got {other:?}"),
        }
    }

    #[test]
    fn mcp_shim_takes_precedence_over_config_name() {
        // A hypothetical config named `mcp-demo-read-file` would be
        // shadowed by the shim routing. This is deliberate: the shim
        // namespace is reserved.
        let invocation = Invocation::capture(vec![
            OsString::from("mcp-demo-read-file"),
            OsString::from("arg1"),
        ]);
        let target = resolve_invocation(&invocation);
        assert!(matches!(target, DispatchTarget::McpShim { .. }));
    }

    #[test]
    fn non_shim_symlink_still_routes_to_app_config() {
        // Config-named symlink without the `mcp-` prefix keeps its
        // existing behavior.
        let invocation =
            Invocation::capture(vec![OsString::from("work"), OsString::from("invoke")]);
        let target = resolve_invocation(&invocation);
        assert!(matches!(target, DispatchTarget::AppConfig { .. }));
    }

    #[test]
    fn mcp_prefix_alone_is_not_a_shim() {
        // "mcp" without a trailing `-server-tool` is just a config
        // name — the parser rejects it.
        let invocation = Invocation::capture(vec![OsString::from("mcp"), OsString::from("foo")]);
        let target = resolve_invocation(&invocation);
        assert!(matches!(target, DispatchTarget::AppConfig { .. }));
    }
}
