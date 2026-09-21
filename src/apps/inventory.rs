//! Where the dynamic CLI's command list comes from.
//!
//! The dynamic surface is built from a discovery inventory. Three sources can
//! supply one, and [`resolve`] reconciles them before every parse:
//!
//! 1. the per-user **cache** in the state store, written by every discovery;
//! 2. a **snapshot** file shipped next to the config (`discovery.snapshot`),
//!    which seeds the cache whenever it is the newer of the two;
//! 3. a **live discovery**, run when there is still nothing to parse against
//!    (`discovery.auto`) or the cache has outlived `discovery.ttl_seconds`.
//!
//! The point of all three is that the first command a user ever runs works:
//! without them, a fresh machine has an empty cache, every mapped command is an
//! "unrecognized subcommand", and `--help` shows the generic MCP surface.

use std::{path::Path, time::Duration};

use anyhow::{Result, anyhow};
use chrono::Utc;

use crate::{
    apps::AppContext,
    config::BUILTIN_COMMANDS,
    mcp::model::{DiscoveryCategory, McpOperation, McpOperationResult},
    runtime::{DiscoveryInventoryView, InventorySnapshot},
};

// Hidden pre-manifest aliases the static bridge still answers to. They are part
// of mcp2cli's own surface, not of a CLI that restricts its built-ins.
const LEGACY_COMMANDS: &[&str] = &["invoke", "read", "list", "discover"];

// An on-demand discovery is a detour on the way to what the user asked for, so
// it gets a budget of its own instead of the operation timeout (120 s by
// default, per request, times three categories). `--help` gets the shorter one:
// a help screen that is late is worse than one that lists fewer commands.
const HELP_DISCOVERY_BUDGET: Duration = Duration::from_secs(10);
const COMMAND_DISCOVERY_BUDGET: Duration = Duration::from_secs(30);

/// The inventory to build the CLI from, plus why the command list is missing
/// when a live discovery was attempted and failed.
pub struct InventoryResolution {
    pub inventory: Option<DiscoveryInventoryView>,
    pub discovery_error: Option<anyhow::Error>,
}

impl InventoryResolution {
    /// Whether the server's tools are known. Discovery caches each category it
    /// gets an answer for, so an inventory can exist without them.
    pub fn has_tools(&self) -> bool {
        self.inventory
            .as_ref()
            .is_some_and(|inventory| inventory.tools.is_some())
    }
}

/// Reconcile cache, snapshot and live discovery for this invocation.
/// `requested` is the top-level command the user typed; `None` means the
/// top-level help.
pub async fn resolve(context: &AppContext, requested: Option<&str>) -> InventoryResolution {
    let store = &context.services.state_store;
    let discovery = &context.config.discovery;
    let mut inventory = store.discovery_inventory_view(&context.config_name).await;

    if let Some(path) = &discovery.snapshot {
        match InventorySnapshot::load(Path::new(path)) {
            Ok(snapshot) => {
                // Each snapshot seeds the cache once. Comparing timestamps alone
                // would let a snapshot stamped by a faster clock than the
                // user's overwrite every live discovery, forever.
                let already_seeded = store.seeded_snapshot(&context.config_name).await
                    == Some(snapshot.generated_at);
                let newer = inventory
                    .as_ref()
                    .is_none_or(|cached| snapshot.generated_at > cached.updated_at);
                if newer && !already_seeded {
                    let seeded =
                        snapshot.into_inventory(&context.config_name, &context.config.app.profile);
                    // The store updates its in-memory view first, so a read-only
                    // data directory costs persistence, not the command list.
                    if let Err(error) = store.seed_discovery_inventory(seeded.clone()).await {
                        tracing::debug!("could not persist inventory snapshot: {:#}", error);
                    }
                    inventory = Some(seeded);
                }
            }
            // A packaging defect the user can do nothing about; discovery below
            // covers for it.
            Err(error) => tracing::debug!("ignoring inventory snapshot: {:#}", error),
        }
    }

    // Built-ins such as `auth login` must keep working when the server cannot be
    // asked for its commands — that is often exactly why they are being run.
    if requested.is_some_and(|name| is_manifest_independent(context, name)) {
        return InventoryResolution {
            inventory,
            discovery_error: None,
        };
    }

    // Tools are what commands are made of. An inventory without them is what a
    // discovery leaves behind when `tools/list` failed but another category
    // answered; treating it as complete would strand the CLI without commands.
    let missing = inventory
        .as_ref()
        .is_none_or(|cached| cached.tools.is_none());
    // A refresh is worth a round-trip on the way to running a command, never on
    // the way to a help screen that the cache can already answer.
    let expired = requested.is_some()
        && match (&inventory, discovery.ttl_seconds) {
            (Some(cached), Some(ttl)) => {
                let age = Utc::now().signed_duration_since(cached.updated_at);
                age.num_seconds() > i64::try_from(ttl).unwrap_or(i64::MAX)
            }
            _ => false,
        };

    let mut discovery_error = None;
    if (missing && discovery.auto) || expired {
        let budget = discovery_budget(context, requested);
        let outcome = match tokio::time::timeout(budget, discover_all(context)).await {
            Ok(outcome) => outcome,
            Err(_) => Err(anyhow!(
                "the server did not answer within {} seconds",
                budget.as_secs()
            )),
        };
        // Categories are cached as they answer, so re-read either way.
        inventory = store.discovery_inventory_view(&context.config_name).await;
        match outcome {
            Ok(()) => {}
            // A failed refresh leaves a usable list behind; say nothing.
            Err(error) if !missing => tracing::debug!("inventory refresh failed: {:#}", error),
            Err(error) => discovery_error = Some(error),
        }
    }

    InventoryResolution {
        inventory,
        discovery_error,
    }
}

/// How long an on-demand discovery may take. An explicit `--timeout` is the
/// user's own answer to that; otherwise the budget, or the configured operation
/// timeout when that is shorter.
fn discovery_budget(context: &AppContext, requested: Option<&str>) -> Duration {
    if let Some(seconds) = context.timeout_override.filter(|seconds| *seconds > 0) {
        return Duration::from_secs(seconds);
    }
    let budget = match requested {
        Some(_) => COMMAND_DISCOVERY_BUDGET,
        None => HELP_DISCOVERY_BUDGET,
    };
    match context.config.defaults.timeout_seconds {
        0 => budget,
        seconds => budget.min(Duration::from_secs(seconds)),
    }
}

/// Whether `name` runs without the server's command list: an enabled built-in,
/// or — on mcp2cli's own unrestricted surface — a legacy alias.
fn is_manifest_independent(context: &AppContext, name: &str) -> bool {
    match &context.config.branding {
        Some(branding) if branding.builtin_commands.is_some() => {
            BUILTIN_COMMANDS.contains(&name) && branding.allows_builtin(name)
        }
        _ => BUILTIN_COMMANDS.contains(&name) || LEGACY_COMMANDS.contains(&name),
    }
}

/// Discover tools, resources and prompts, caching each category as it answers.
///
/// A category the server does not advertise is cached as empty — that *is* the
/// answer. A category it does advertise but failed to list is an error, and
/// stays uncached so the next run asks again.
pub async fn discover_all(context: &AppContext) -> Result<()> {
    let store = &context.services.state_store;
    let mut first_error = None;
    for category in [
        DiscoveryCategory::Capabilities,
        DiscoveryCategory::Resources,
        DiscoveryCategory::Prompts,
    ] {
        let outcome = context
            .perform(McpOperation::Discover {
                category: category.clone(),
            })
            .await;
        let items = match outcome {
            Ok(McpOperationResult::Discovery { items, .. }) => items,
            Ok(other) => {
                first_error.get_or_insert_with(|| {
                    anyhow!("unexpected MCP response to discovery: {:?}", other)
                });
                continue;
            }
            Err(error) => {
                if server_advertises(context, &category).await == Some(false) {
                    Vec::new()
                } else {
                    first_error.get_or_insert(error);
                    continue;
                }
            }
        };
        store
            .upsert_discovery_inventory(
                &context.config_name,
                &context.config.app.profile,
                category,
                items,
            )
            .await?;
    }

    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Whether the negotiated server capabilities include `category`. `None` when
/// nothing was negotiated (the server was never reached, or this client does
/// not expose its session).
async fn server_advertises(context: &AppContext, category: &DiscoveryCategory) -> Option<bool> {
    let session = context.services.mcp_client.negotiated_session().await?;
    let capabilities = session.server_capabilities?;
    Some(match category {
        DiscoveryCategory::Capabilities => capabilities.tools.is_some(),
        DiscoveryCategory::Resources => capabilities.resources.is_some(),
        DiscoveryCategory::Prompts => capabilities.prompts.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::{
        config::AppConfig,
        mcp::{
            client::McpClient,
            model::{ConnectionMetadata, TransportKind},
            protocol::{ListCapability, McpClientSession, ServerCapabilities},
        },
        runtime::{EventBroker, RuntimeServices, StateStore, TokenStore},
    };

    /// A server that lists tools and resources, advertises no prompts, and
    /// fails `tools/list` for as long as `tools_down` is set.
    struct ScriptedServer {
        tools_down: Mutex<bool>,
        tool_listings: Mutex<usize>,
        hang: bool,
    }

    #[async_trait]
    impl McpClient for ScriptedServer {
        async fn metadata(&self, app_id: &str) -> Result<ConnectionMetadata> {
            Ok(ConnectionMetadata {
                app_id: app_id.to_owned(),
                server_name: "scripted".to_owned(),
                server_version: "1".to_owned(),
                transport: TransportKind::Stdio,
            })
        }

        async fn negotiated_session(&self) -> Option<McpClientSession> {
            let mut session = McpClientSession::new("2025-11-25");
            session.server_capabilities = Some(ServerCapabilities {
                tools: Some(ListCapability::default()),
                resources: Some(Default::default()),
                ..ServerCapabilities::default()
            });
            Some(session)
        }

        async fn perform(
            &self,
            _app_id: &str,
            operation: McpOperation,
            _events: &EventBroker,
            _inventory_stale_path: Option<&PathBuf>,
        ) -> Result<McpOperationResult> {
            if self.hang {
                std::future::pending::<()>().await;
            }
            let McpOperation::Discover { category } = operation else {
                return Err(anyhow!("unexpected operation"));
            };
            let items = match category {
                DiscoveryCategory::Capabilities => {
                    *self.tool_listings.lock().unwrap() += 1;
                    if *self.tools_down.lock().unwrap() {
                        return Err(anyhow!("tools/list: upstream unavailable"));
                    }
                    vec![json!({ "id": "send", "kind": "tool" })]
                }
                DiscoveryCategory::Resources => vec![json!({ "uri": "mail://inbox" })],
                DiscoveryCategory::Prompts => return Err(anyhow!("Method not found")),
            };
            Ok(McpOperationResult::Discovery {
                message: "discovered".to_owned(),
                category,
                items,
            })
        }
    }

    async fn context(
        dir: &std::path::Path,
        server: Arc<ScriptedServer>,
        configure: impl FnOnce(&mut AppConfig),
    ) -> AppContext {
        let mut config = AppConfig::default();
        configure(&mut config);
        AppContext {
            invoked_as: "email".to_owned(),
            config_name: "email".to_owned(),
            config: Arc::new(config),
            services: RuntimeServices {
                state_store: Arc::new(
                    StateStore::load(dir.join("state.json"))
                        .await
                        .expect("state store should load"),
                ),
                token_store: Arc::new(TokenStore::new(dir.join("tokens.json"))),
                event_broker: EventBroker::new(Vec::new()),
                mcp_client: server,
            },
            timeout_override: None,
            non_interactive: false,
            input_json: None,
        }
    }

    fn server(tools_down: bool) -> Arc<ScriptedServer> {
        Arc::new(ScriptedServer {
            tools_down: Mutex::new(tools_down),
            tool_listings: Mutex::new(0),
            hang: false,
        })
    }

    #[tokio::test]
    async fn a_failed_tool_listing_is_retried_instead_of_cached_as_no_tools() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let server = server(true);
        let context = context(dir.path(), server.clone(), |_| {}).await;

        // Resources answered, tools did not: that is a failure, reported as one…
        let first = resolve(&context, Some("send")).await;
        assert!(!first.has_tools());
        let error = first.discovery_error.expect("the failure must surface");
        assert!(
            error.to_string().contains("upstream unavailable"),
            "{error}"
        );
        // …and what did answer is kept.
        let cached = first.inventory.expect("resources should be cached");
        assert_eq!(cached.resources.map(|items| items.len()), Some(1));

        // Once the server recovers, the next run asks again and gets its tools.
        *server.tools_down.lock().unwrap() = false;
        let second = resolve(&context, Some("send")).await;
        assert!(second.has_tools());
        assert!(second.discovery_error.is_none());
        assert_eq!(*server.tool_listings.lock().unwrap(), 2);

        // A category the server does not advertise is an answer, not a failure.
        let inventory = second.inventory.expect("inventory should exist");
        assert_eq!(inventory.prompts, Some(Vec::new()));

        // Complete now: no further round-trips.
        resolve(&context, Some("send")).await;
        assert_eq!(*server.tool_listings.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn builtins_and_disabled_auto_discovery_never_ask_the_server() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let server = server(false);

        let context_auto = context(dir.path(), server.clone(), |_| {}).await;
        resolve(&context_auto, Some("auth")).await;
        assert_eq!(*server.tool_listings.lock().unwrap(), 0);

        let context_manual = context(dir.path(), server.clone(), |config| {
            config.discovery.auto = false;
        })
        .await;
        let resolution = resolve(&context_manual, Some("send")).await;
        assert!(resolution.inventory.is_none());
        assert_eq!(*server.tool_listings.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn a_silent_server_costs_the_budget_not_the_operation_timeout() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let silent = Arc::new(ScriptedServer {
            tools_down: Mutex::new(false),
            tool_listings: Mutex::new(0),
            hang: true,
        });
        // The operation timeout is off entirely; only the budget can end this.
        let mut context = context(dir.path(), silent, |config| {
            config.defaults.timeout_seconds = 0;
        })
        .await;
        assert_eq!(discovery_budget(&context, None), HELP_DISCOVERY_BUDGET);
        assert_eq!(
            discovery_budget(&context, Some("send")),
            COMMAND_DISCOVERY_BUDGET
        );

        // `--timeout` is the user's own answer to how long is too long.
        context.timeout_override = Some(1);
        let started = std::time::Instant::now();
        let resolution = resolve(&context, Some("send")).await;
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        let error = resolution
            .discovery_error
            .expect("the wait must be reported");
        assert!(
            error
                .to_string()
                .contains("did not answer within 1 seconds"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn the_discovery_budget_never_exceeds_a_shorter_operation_timeout() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let context = context(dir.path(), server(false), |config| {
            config.defaults.timeout_seconds = 4;
        })
        .await;
        assert_eq!(discovery_budget(&context, None), Duration::from_secs(4));
        assert_eq!(
            discovery_budget(&context, Some("send")),
            Duration::from_secs(4)
        );
    }

    #[tokio::test]
    async fn an_expired_list_is_refreshed_for_a_command_but_never_for_help() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let server = server(false);
        let context = context(dir.path(), server.clone(), |config| {
            config.discovery.ttl_seconds = Some(60);
        })
        .await;
        let mut stale = InventorySnapshot {
            schema_version: crate::runtime::SNAPSHOT_SCHEMA_VERSION,
            generated_at: Utc::now() - chrono::Duration::days(30),
            generator: None,
            tools: vec![json!({ "id": "old-send", "kind": "tool" })],
            resources: Vec::new(),
            resource_templates: Vec::new(),
            prompts: Vec::new(),
        }
        .into_inventory("email", "bridge");
        stale.config_name = "email".to_owned();
        context
            .services
            .state_store
            .seed_discovery_inventory(stale)
            .await
            .expect("cache should be seeded");

        // Help is answered from what is there — no round-trip, no complaint.
        let help = resolve(&context, None).await;
        assert!(help.has_tools() && help.discovery_error.is_none());
        assert_eq!(*server.tool_listings.lock().unwrap(), 0);

        // A command is worth the refresh.
        let run = resolve(&context, Some("send")).await;
        assert_eq!(*server.tool_listings.lock().unwrap(), 1);
        let tools = run.inventory.and_then(|inventory| inventory.tools).unwrap();
        assert_eq!(tools[0]["id"], "send");
    }

    #[tokio::test]
    async fn a_snapshot_seeds_the_cache_once_even_if_its_clock_runs_ahead() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let snapshot_path = dir.path().join("inventory.json");
        // Stamped by an author whose clock is a day ahead of this machine's.
        InventorySnapshot {
            schema_version: crate::runtime::SNAPSHOT_SCHEMA_VERSION,
            generated_at: Utc::now() + chrono::Duration::days(1),
            generator: None,
            tools: vec![json!({ "id": "snapshot-tool", "kind": "tool" })],
            resources: Vec::new(),
            resource_templates: Vec::new(),
            prompts: Vec::new(),
        }
        .write(&snapshot_path)
        .expect("snapshot should be written");

        let server = server(false);
        let context = context(dir.path(), server, |config| {
            config.discovery.snapshot = Some(snapshot_path.to_string_lossy().into_owned());
        })
        .await;

        let seeded = resolve(&context, None).await;
        let tools = seeded
            .inventory
            .and_then(|inventory| inventory.tools)
            .unwrap();
        assert_eq!(tools[0]["id"], "snapshot-tool");

        // A live discovery then replaces it, and must stay replaced although the
        // snapshot's timestamp is still the later of the two.
        discover_all(&context)
            .await
            .expect("discovery should succeed");
        let after = resolve(&context, None).await;
        let tools = after
            .inventory
            .and_then(|inventory| inventory.tools)
            .unwrap();
        assert_eq!(tools[0]["id"], "send");
    }
}
