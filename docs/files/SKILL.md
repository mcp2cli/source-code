---
name: mcp2cli
description: >
  Use mcp2cli to connect to, explore, and execute commands against any MCP
  (Model Context Protocol) server from the terminal. Covers discovery, tool
  invocation, resource reading, prompt execution, alias creation, config
  management, background jobs, authentication, and JSON output pipelines.
applyTo:
  - "src/**"
  - "**/*mcp2cli*"
---

# SKILL: mcp2cli

This skill enables AI agents and coding assistants to work effectively with
the `mcp2cli` CLI tool — both as users invoking it from the terminal and as
contributors editing its source code.

---

## When to Activate This Skill

Use this skill when the user:

- Asks how to run a command against an MCP server from the terminal
- Wants to create a named config, symlink alias, or set an active config
- Needs to parse or transform `mcp2cli --json` output with `jq`
- Is writing a shell script or CI pipeline that calls MCP tools
- Is implementing an AI agent loop that invokes MCP tools via `mcp2cli`
- Is editing source files under `src/`
- Asks about the MCP protocol, transports, or capability discovery
- Wants to install or regenerate man pages for `mcp2cli` or an alias

---

## Key Concepts

### Two modes

| Mode | When to use | How |
|------|-------------|-----|
| Ad-hoc | Exploration, one-off scripts, CI | `mcp2cli --url URL COMMAND` or `mcp2cli --stdio CMD COMMAND` |
| Named config + alias | Daily use, multiple servers | `mcp2cli config init`, `mcp2cli link create`, then use the alias directly |

### Dispatch routing

- `argv[0] == "mcp2cli"` → host mode (config/link/use/man/daemon commands)
- `argv[0] == "<alias>"` → bridge mode (MCP runtime commands for that named config)
- `mcp2cli <config-name> <command>` → bridge mode with explicit config name

### JSON output

Every command supports `--json`. The response envelope is:

```json
{
  "app_id": "<alias>",
  "command": "<command>",
  "summary": "<one-line>",
  "lines": ["..."],
  "data": { ... }
}
```

Pipe to `jq` for scripting: `email --json ls | jq '.data.items[].id'`

---

## Common Tasks

### Discover what a server offers

```bash
# Ad-hoc
mcp2cli --url http://localhost:3001/mcp ls
mcp2cli --url http://localhost:3001/mcp ls --tools
mcp2cli --url http://localhost:3001/mcp ls --resources

# Named alias
email ls
email ls --prompts
email ls --filter "draft"
```

### Call a tool

```bash
# Ad-hoc
mcp2cli --url http://localhost:3001/mcp echo --message hello

# Named alias
email send --to user@example.com --subject "Hi" --body "Hello"
email search --query "from:boss"
email reply --thread-id 123 --body "Thanks"
```

### Read a resource

```bash
email get mail://inbox
email get mail://thread/123
```

### Run a prompt

```bash
email simple-prompt
email complex-prompt --temperature 0.7 --style concise
```

### Health check

```bash
mcp2cli --url http://localhost:3001/mcp doctor
email doctor
email --json doctor | jq '.data.server'
```

### Background jobs

```bash
email search --query "from:boss" --background
email jobs list
email jobs watch --latest
email jobs cancel --latest
```

### Create a config and alias

```bash
mcp2cli config init --name email \
  --transport streamable_http \
  --endpoint https://mcp.example.com/email

mcp2cli link create --name email     # creates ~/.local/bin/email + man page

# PATH (one time)
export PATH="$HOME/.local/bin:$PATH"
```

### Install man pages

```bash
mcp2cli man install              # installs mcp2cli(1)
man mcp2cli                      # read it
man email                        # alias man page
```

---

## JSON Schema → CLI Flag Mapping

| JSON Schema type | CLI flag format | Example |
|-----------------|-----------------|---------|
| `string` | `--name TEXT` | `--message hello` |
| `integer` | `--count INT` | `--steps 5` |
| `number` | `--rate NUM` | `--temperature 0.7` |
| `boolean` | `--flag` (presence toggle) | `--include-image` |
| `enum` | `--kind A|B|C` | `--level error` |
| `array` | `--tags VAL,...` | `--labels bug,urgent` |
| complex / nested | `--config JSON` | `--config '{"key":"val"}'` |

---

## Source Code Entry Points

| File | Role |
|------|------|
| `src/app/mod.rs` | `build()` + `run()` — application entry |
| `src/dispatch/mod.rs` | `resolve_invocation()` — routing |
| `src/runtime/host.rs` | `run_host()` — all host commands |
| `src/apps/bridge.rs` | `execute()` — all MCP runtime commands |
| `src/apps/manifest.rs` | `CommandManifest` — capability → CLI mapping |
| `src/apps/dynamic.rs` | Dynamic clap tree builder |
| `src/config/mod.rs` | `AppConfig`, `RuntimeLayout` |
| `src/man.rs` | `generate()`, `generate_host()`, `install()` |
| `src/mcp/client.rs` | `McpClient` trait + transport implementations |
| `src/output/mod.rs` | `CommandOutput`, `render()` |

---

## Common Errors and Fixes

| Error | Cause | Fix |
|-------|-------|-----|
| `no named config 'X' found` | Config missing | Run `mcp2cli config init --name X ...` first |
| `link already exists` | Symlink exists | Add `--force` to overwrite |
| `'X' is reserved` | Name conflicts with host command | Choose a different name |
| `server.endpoint must be set` | HTTP transport + no endpoint | Add `--endpoint` to `config init` |
| Connection refused | Server not running | Start the server, then retry |

---

## Profile Overlay (customize the CLI surface)

Add a `profile:` block to the config YAML to rename, hide, group, or alias commands:

```yaml
profile:
  display_name: "Email CLI"
  aliases:
    long-tool-name: ltn
  hide:
    - debug-dump
  groups:
    mail:
      - send
      - reply
  flags:
    send:
      body: text
  resource_verb: fetch
```

---

## Environment Variables

| Variable | Effect |
|----------|--------|
| `MCP2CLI_CONFIG_DIR` | Override config directory |
| `MCP2CLI_DATA_DIR` | Override data/state directory |
| `MCP2CLI_BIN_DIR` | Override default symlink directory |
| `MCP2CLI_CONFIG` | Path to an explicit config file |
| `MCP2CLI_TELEMETRY` | Set `off` to disable telemetry |
| `DO_NOT_TRACK=1` | Standard opt-out signal |
| `RUST_LOG` | Tracing filter (e.g. `mcp2cli=debug`) |
