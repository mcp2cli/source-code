# AGENTS.md

Working rules for AI agents and contributors editing this repository.

This file is optimised for implementation agents.
For human-oriented onboarding see `README.md`.
For full API and CLI detail see `docs/usage-guide.md` and `docs/reference/`.

---

## What mcp2cli Is

`mcp2cli` is a **discovery-driven CLI bridge** for MCP (Model Context Protocol) servers.
It maps every MCP server capability to a native terminal command:

- Tools → typed subcommands with `--flags` derived from JSON Schema
- Resources → `get <URI>` (concrete) or parameterised subcommands (templates)
- Prompts → subcommands with typed flags from argument metadata

One binary. Unlimited servers. Zero protocol knowledge required at the terminal.

---

## Repository Location

```
.                     ← this project
  src/
    app/               entry-point: build() + run() → AppState
    apps/
      bridge.rs        runtime bridge dispatcher (BridgeApp)
      dynamic.rs       clap CLI tree builder from CommandManifest
      manifest.rs      DiscoveryInventoryView → CommandManifest
    cli/               host-mode clap structs + output helpers
    config/            AppConfig, RuntimeLayout, named-config I/O
    dispatch/          Invocation capture + target resolution
    lib.rs             public module tree
    main.rs            #[tokio::main] entry
    man.rs             man page generator (alias + host)
    mcp/
      client.rs        McpClient trait + transport implementations
      model.rs         TransportKind, protocol types
      protocol.rs      session bootstrap helpers
      handler.rs       server→client notification handling
    observability/     tracing subscriber setup
    output/            CommandOutput, OutputFormat, render()
    runtime/
      host.rs          RuntimeHost: run_host() + run_app()
      state.rs         StateStore (JSON persistence, discovery inventory)
      token_store.rs   TokenStore
      sinks.rs         EventSink implementations
      daemon/          background daemon for warm-session mode
    telemetry.rs       anonymous usage telemetry
  tests/
    integration.rs     integration test suite (requires binary)
  docs/                documentation
  config.example.yaml  annotated full config template
  Cargo.toml
```

---

## Architecture Invariants

1. **Dispatch happens at the edge, before any app logic.**
   `dispatch::resolve_invocation` is the single place that reads `argv[0]` and decides
   whether this is a host-mode invocation or a named-config bridge invocation.
   Do not replicate this logic elsewhere.

2. **`CommandManifest` is the single source of truth for the CLI surface.**
   Everything from flag names to nested subcommand groups flows through
   `apps/manifest.rs → ManifestEntry → ManifestCommand → FlagSpec`.
   The dynamic clap tree in `apps/dynamic.rs` is built from, and only from, the manifest.

3. **`StateStore` is the only place that writes runtime state.**
   Auth sessions, discovery inventory, background jobs, and negotiated capability
   views are all stored in `~/.local/share/mcp2cli/instances/<name>/state.json`.
   Never write state to the config directory or anywhere else.

4. **`RuntimeLayout` is the only source of directory paths.**
   All paths (config, data, links, man pages) must be derived from `RuntimeLayout`
   methods. Do not construct paths by hand from `$HOME` or string concatenation.

5. **Output always goes through `render()`.**
   The `CommandOutput` + `render()` pattern applies to every command.
   Do not print to stdout directly.

6. **Man page generation is best-effort.**
   `install_man_page` and the host auto-install in `link create` must never fail
   the primary operation. Wrap man-page errors and surface them as output annotations.

---

## Transport Model

| Kind | Config key | Description |
|------|-----------|-------------|
| `streamable_http` | `server.transport` | JSON-RPC over HTTP, SSE streaming |
| `stdio` | `server.transport` | Spawn subprocess, communicate on stdin/stdout |
| Demo (file-backed) | endpoint = `*.invalid` | Offline demo backend, no server needed |

Ad-hoc invocations (`--url`, `--stdio`) bypass named configs entirely and are
resolved in `dispatch/mod.rs` under `AdHocTransport`.

---

## Key Data Flow

```
argv
  → dispatch::resolve_invocation()
      → DispatchTarget::Host   → RuntimeHost::run_host()  → HostCommand handlers
      → DispatchTarget::AppConfig → RuntimeHost::run_app() → bridge::execute()
          → apps/dynamic.rs    build clap tree from CommandManifest
          → clap parse
          → BridgeDomainCommand
          → McpClient calls
          → CommandOutput
          → render()
```

---

## MCP Protocol Coverage

Full MCP 2025-11-25 compliance. Key protocol operations in `mcp/client.rs`:

- `initialize` / `initialized` — session negotiation
- `tools/list`, `resources/list`, `prompts/list` — discovery
- `tools/call` — tool invocation
- `resources/read` — resource reading
- `prompts/get` — prompt execution
- `ping`, `logging/setLevel`, `completion/complete`
- `resources/subscribe`, `resources/unsubscribe`
- Server→client: `elicitation/create`, `sampling/createMessage`, progress, logs, list-changed

---

## Adding a New Host Command

1. Add a variant to `HostCommand` in `src/cli/mod.rs`.
2. Define its `Args` struct in the same file with `#[derive(Debug, Args)]`.
3. Add a `CommandOutput`-returning output helper function to `src/cli/mod.rs`.
4. Handle the variant in `RuntimeHost::run_host()` in `src/runtime/host.rs`.
5. Add unit tests in the affected modules.

---

## Adding a New Bridge (Runtime) Command

1. Add a variant to `BridgeCommand` and `BridgeDomainCommand` in `src/apps/bridge.rs`.
2. Add the clap subcommand to the dynamic CLI builder in `src/apps/dynamic.rs`.
3. Implement the domain handler in `bridge.rs`.
4. Add tests.

---

## Config Anatomy

All config fields are in `src/config/mod.rs` as serde structs.
Key sections:

| Struct | Purpose |
|--------|---------|
| `AppConfig` | Root; wraps all sub-configs |
| `AppBindingConfig` | `app.profile` — which built-in profile |
| `ServerConfig` | transport, endpoint, stdio subprocess, roots |
| `DefaultsConfig` | output format, timeout |
| `AuthConfig` | token store path, browser open command |
| `EventConfig` | 5 event sink types |
| `ProfileOverlay` | rename/hide/group/alias CLI surface |

Named configs live at `~/.config/mcp2cli/configs/<name>.yaml`.
The active-config pointer lives at `~/.local/share/mcp2cli/host/active-config.json`.

---

## Man Page System

`src/man.rs` exposes three public items:

- `generate(ctx: &ManPageContext)` — produce troff content for an alias app
- `generate_host()` — produce troff content for `mcp2cli` itself
- `install(name, content, man_dir)` — write `<man_dir>/<name>.1`

`RuntimeLayout::man_dir()` returns `~/.local/share/man/man1` by default.

Man pages are auto-generated on `mcp2cli link create` (alias + host) and can
be explicitly refreshed with `mcp2cli man install`.

---

## Test Strategy

- **Unit tests** — in each `mod tests {}` block; run with `cargo test --lib`.
- **Integration tests** — `tests/integration.rs`; spawn the real binary; require a live
  stdio MCP server for transport tests. Host and link tests work without a server.
- **Demo mode** — use `--endpoint https://demo.invalid/mcp` for offline tests in CI.

Do not mock `McpClient` for integration tests — use the demo backend or real server.
Do mock it in unit tests via the `TestMcpClient` in `mcp/client.rs`.

---

## Reserved Names

The following cannot be used as config or alias names:

`mcp2cli`, `config`, `link`, `use`, `daemon`, `man`

All are enforced in `RuntimeHost::ensure_host_config_name()`.

---

## Telemetry

Telemetry is anonymous, non-sensitive, and opt-out. It records command category,
transport type, feature flags, outcome, and duration. No tool names, endpoints,
argument values, or user identifiers are ever collected.

Disable with `MCP2CLI_TELEMETRY=off`, `DO_NOT_TRACK=1`, or `--no-telemetry`.
