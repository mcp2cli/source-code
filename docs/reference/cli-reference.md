# CLI Reference

Complete reference for every mcp2cli command, flag, and option.

---

## Invocation Modes

```
mcp2cli [--json] [--output <FORMAT>] [--timeout <SECS>] <command>
mcp2cli [--json] [--output <FORMAT>] [--timeout <SECS>] <config-name> <command>
mcp2cli --url <URL> [--json] [--timeout <SECS>] <command>
mcp2cli --stdio <COMMAND> [--env KEY=VAL]... [--json] [--timeout <SECS>] <command>
<alias> [--json] [--output <FORMAT>] [--timeout <SECS>] <command>
```

---

## Global Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--json` | | Output JSON instead of human-readable text |
| `--output <FORMAT>` | `-o` | Output format: `human`, `json`, `ndjson` |
| `--timeout <SECS>` | | Request timeout in seconds (0 = no timeout) |
| `--url <URL>` | | Ad-hoc HTTP MCP server URL (no config needed) |
| `--stdio <COMMAND>` | | Ad-hoc stdio MCP server command (no config needed) |
| `--env <KEY=VALUE>` | | Environment variable for `--stdio` server (repeatable) |

---

## Host Commands

These manage configs and aliases — no server connection required.

### `config init`

Create a new named config.

```bash
mcp2cli config init --name <NAME> --app bridge \
  --transport <stdio|streamable_http> \
  [--endpoint <URL>] \
  [--stdio-command <CMD>] \
  [--stdio-args <ARG>...]
```

| Flag | Required | Description |
|------|----------|-------------|
| `--name <NAME>` | ✅ | Config name (alphanumeric, hyphens) |
| `--app <PROFILE>` | ✅ | Application profile (`bridge`) |
| `--transport <TYPE>` | ✅ | `stdio` or `streamable_http` |
| `--endpoint <URL>` | for HTTP | Server endpoint URL |
| `--stdio-command <CMD>` | for stdio | Subprocess command |
| `--stdio-args <ARG>...` | | Subprocess arguments |

### `config list`

List all named configs.

```bash
mcp2cli config list
```

### `config show`

Display a named config.

```bash
mcp2cli config show --name <NAME>
```

### `use`

Manage the active config.

```bash
mcp2cli use <NAME>         # Set active config
mcp2cli use --show         # Show current active config
mcp2cli use --clear        # Clear active config
```

### `link create`

Create a symlink alias to mcp2cli.

```bash
mcp2cli link create --name <NAME> [--dir <PATH>]
```

| Flag | Required | Description |
|------|----------|-------------|
| `--name <NAME>` | ✅ | Alias name (also the symlink filename) |
| `--dir <PATH>` | | Directory for the symlink (default: next to binary) |

Reserved names: `mcp2cli`, `config`, `link`, `use`, `daemon`.

### `daemon`

Manage background daemon processes.

```bash
mcp2cli daemon start <CONFIG_NAME>    # Start daemon for config
mcp2cli daemon stop <CONFIG_NAME>     # Stop running daemon
mcp2cli daemon status [CONFIG_NAME]   # Check daemon status
```

---

## Discovery Commands

### `ls`

List server capabilities.

```bash
<alias> ls [--tools] [--resources] [--prompts] [--filter <PATTERN>]
```

| Flag | Description |
|------|-------------|
| `--tools` | Show only tools |
| `--resources` | Show only resources |
| `--prompts` | Show only prompts |
| `--filter <PATTERN>` | Filter results by name substring |

### `inspect`

Dump full server capabilities, metadata, and negotiated protocol info.

```bash
<alias> inspect
```

### `doctor`

Run runtime health diagnostics.

```bash
<alias> doctor
```

Checks: transport connection, server info, auth state, cached capabilities.

### `ping`

Server liveness check with latency measurement.

```bash
<alias> ping
```

---

## Tool Commands

### Dynamic (auto-generated)

Server tools become commands with flags from JSON Schema:

```bash
<alias> <tool-name> [--flag <value>]...
```

Examples:

```bash
work echo --message hello              # String flag
work add --a 5 --b 3                   # Integer flags
work deploy --tags '["a","b"]'         # Array flag (JSON)
work process --include-metadata        # Boolean flag
work build --config '{"opt": true}'    # JSON flag
```

### Static Bridge Fallback

```bash
<alias> tool list
<alias> tool call --name <TOOL_NAME> [--arg <KEY=VALUE>]... [--args-file <PATH>] [--args-json <JSON>] [--background]
```

---

## Resource Commands

### Read a Resource

```bash
<alias> get <URI>
<alias> <resource-verb> <URI>        # If profile.resource_verb is set
```

### Resource Templates (auto-generated)

```bash
<alias> <template-name> [--param <value>]...
<alias> <template-name> <positional>          # Single-param templates
```

### Static Bridge Fallback

```bash
<alias> resource list
<alias> resource read --uri <URI>
```

### Subscriptions

```bash
<alias> subscribe <URI>
<alias> unsubscribe <URI>
```

---

## Prompt Commands

### Dynamic (auto-generated)

```bash
<alias> <prompt-name> [--arg <value>]...
```

### Static Bridge Fallback

```bash
<alias> prompt list
<alias> prompt run --name <PROMPT_NAME> [--arg <KEY=VALUE>]...
```

---

## Auth Commands

```bash
<alias> auth login         # Interactive token prompt
<alias> auth logout        # Clear stored credentials
<alias> auth status        # Show current auth state
```

---

## Job Commands

```bash
<alias> jobs list                          # List all background jobs
<alias> jobs show <JOB_ID>                 # Show job details
<alias> jobs show --latest                 # Show most recent job
<alias> jobs wait <JOB_ID>                 # Block until job completes
<alias> jobs wait --latest                 # Wait for most recent job
<alias> jobs cancel <JOB_ID>              # Cancel a running job
<alias> jobs cancel --latest               # Cancel most recent job
<alias> jobs watch <JOB_ID>               # Stream job progress events
<alias> jobs watch --latest                # Watch most recent job
```

---

## Logging Command

```bash
<alias> log <LEVEL>
```

| Level | Description |
|-------|-------------|
| `trace` | Most verbose |
| `debug` | Debug messages |
| `info` | Informational |
| `warn` | Warnings |
| `error` | Errors only |

---

## Completion Command

```bash
<alias> complete <REF_TYPE> <REF_NAME> <ARG_NAME> [CURRENT_VALUE]
```

Requests tab-completion values from the server.

| Argument | Description |
|----------|-------------|
| `<REF_TYPE>` | Reference type: `ref/prompt`, `ref/resource` |
| `<REF_NAME>` | Name of the prompt or resource |
| `<ARG_NAME>` | Argument name to complete |
| `[CURRENT_VALUE]` | Current partial input for filtering |

---

## Argument Input Methods

Multiple sources for tool arguments, merged in order (later wins):

```bash
# From file
<alias> tool-name --args-file ./payload.json

# From JSON string
<alias> tool-name --args-json '{"key": "value"}'

# From flags
<alias> tool-name --key value

# Combined (merged, flags override)
<alias> tool-name --args-file base.json --args-json '{"override": true}' --key final-value
```

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `MCP2CLI_CONFIG_DIR` | Override config directory |
| `MCP2CLI_DATA_DIR` | Override data directory |
| `MCP2CLI_LOGGING__LEVEL` | Override log level |
| `MCP2CLI_LOGGING__FORMAT` | Override log format |
| `MCP2CLI_SERVER__ENDPOINT` | Override server endpoint |
| `MCP2CLI_SERVER__TRANSPORT` | Override transport type |
| `MCP2CLI_DEFAULTS__OUTPUT` | Override default output format |
| `MCP2CLI_DEFAULTS__TIMEOUT_SECONDS` | Override default timeout |

Pattern: `MCP2CLI_` prefix + config path with `__` as separator.

---

## See Also

- [Configuration Reference](config-reference.md) — full YAML schema
- [Getting Started](../getting-started.md) — quick start
- [Discovery-Driven CLI](../features/discovery-driven-cli.md) — how dynamic commands work
