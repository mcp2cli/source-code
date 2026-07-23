# CLI Reference

Complete reference for every mcp2cli command, flag, and option.

---

## Invocation Modes

```text
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
| `--timeout <SECONDS>` | | Operation timeout in seconds (`0` = no timeout; overrides config) |
| `--non-interactive` | | Fail instead of prompting for input (CI mode) |
| `--input-json <JSON>` | | Supply elicitation answers (and `auth login` credentials) as a JSON object so prompts never block (CI mode) |
| `--no-telemetry` | | Disable anonymous local usage telemetry for this invocation |
| `--url <URL>` | | Ad-hoc HTTP MCP server URL (no config needed) |
| `--stdio <COMMAND>` | | Ad-hoc stdio MCP server command (no config needed) |
| `--env <KEY=VALUE>` | | Environment variable for `--stdio` server (repeatable) |

### CI / non-interactive mode

`--non-interactive`, `--input-json`, and `--timeout` are global and apply to every
command surface (the dynamic per-server CLI and the static bridge). They make
mcp2cli safe to run unattended:

- `--non-interactive` turns any prompt (elicitation, `auth login` token entry)
  into an immediate error instead of blocking on a TTY.
- `--input-json '<OBJECT>'` pre-supplies answers. For elicitation, the keys are
  the requested field names. For `auth login`, pass
  `--input-json '{"bearer_token": "<token>", "account": "<optional>"}'`.

```bash
email --non-interactive search --query "from:boss"
email --input-json '{"bearer_token": "tok_123"}' auth login
email --no-telemetry --timeout 30 send --to user@example.com --subject "Hi" --body "..."
```

---

## Host Commands

These manage configs and aliases — no server connection required.

### `config init`

Create a new named config.

```bash
mcp2cli config init --name <NAME> --app bridge \
  [--transport <stdio|streamable_http>] \
  [--endpoint <URL>] \
  [--stdio-command <CMD>] \
  [--stdio-arg <ARG>]... \
  [--protocol-version <auto|2026-07-28|2025-11-25>] \
  [--force]
```

| Flag | Required | Description |
|------|----------|-------------|
| `--name <NAME>` | ✅ | Config name (alphanumeric, hyphens) |
| `--app <PROFILE>` | | Application profile (default: `bridge`) |
| `--transport <TYPE>` | | `stdio` or `streamable_http` (default: `streamable_http`) |
| `--endpoint <URL>` | for HTTP | Server endpoint URL |
| `--stdio-command <CMD>` | for stdio | Subprocess command |
| `--stdio-arg <ARG>` | | Subprocess argument (repeat for each arg) |
| `--protocol-version <V>` | | MCP revision: `auto` (default), `2026-07-28`, `2025-11-25` |
| `--force` | | Overwrite an existing config of the same name |

```bash
# HTTP email server
mcp2cli config init --name email --app bridge \
  --transport streamable_http --endpoint https://mcp.example.com/email

# Local stdio reference server (the everything server speaks echo/add, not email)
mcp2cli config init --name local --app bridge \
  --transport stdio --stdio-command npx \
  --stdio-arg @modelcontextprotocol/server-everything
```

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
mcp2cli use email          # Set active config
mcp2cli use --show         # Show current active config
mcp2cli use --clear        # Clear active config
```

### `link create`

Create a symlink alias to mcp2cli. By default this also generates and installs a
man page for the alias.

```bash
mcp2cli link create --name email \
  [--dir <PATH>] [--force] [--man-dir <PATH>] [--no-man]
```

| Flag | Required | Description |
|------|----------|-------------|
| `--name <NAME>` | ✅ | Alias name (also the symlink filename) |
| `--dir <PATH>` | | Directory for the symlink (default: next to binary) |
| `--force` | | Replace an existing symlink of the same name |
| `--man-dir <PATH>` | | Man page install directory (default: `~/.local/share/man/man1`) |
| `--no-man` | | Skip man page generation and installation |

Reserved names: `mcp2cli`, `config`, `link`, `use`, `daemon`.

```bash
# Install an `email` alias bound to the active/email config
mcp2cli link create --name email
# email ls, email send, email auth login ... now dispatch to that config
```

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
<alias> ls [--tools] [--resources] [--prompts] [--filter <PATTERN>] [--all]
```

| Flag | Description |
|------|-------------|
| `--tools` | Show only tools |
| `--resources` | Show only resources |
| `--prompts` | Show only prompts |
| `--filter <PATTERN>` | Filter results by name substring |
| `--all` | List every item without pagination |

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

Server tools become commands with flags from JSON Schema. With an `email` alias
bound to an email MCP server, its tools are exposed directly (the alias already
namespaces them, so there is no `email.` prefix on the command):

```bash
<alias> <tool-name> [--flag <value>]...
```

Examples against the `email` server:

```bash
email send --to user@example.com --subject "Hi" --body "..."   # String flags
email search --query "from:boss"                               # String flag
email draft create --subject "New draft"                       # Dotted tool (draft.create)
email reply --thread-id 123 --body "Thanks"                    # String flags
email labels add --thread-id 123 --label important             # Dotted tool (labels.add)
```

Flag types follow the tool's JSON Schema — booleans become bare switches, arrays
and objects take JSON values:

```bash
local echo --message hello             # String flag
local add --a 5 --b 3                  # Integer flags
<alias> deploy --tags '["a","b"]'      # Array flag (JSON)
<alias> process --include-metadata     # Boolean flag
<alias> build --config '{"opt": true}' # JSON flag
```

> The `local` alias above points at a stdio
> `@modelcontextprotocol/server-everything` config — a real reference server
> whose tools are `echo`/`add`/etc., not email tools.

### Static Bridge Fallback

```bash
<alias> tool list [--filter <PATTERN>] [--all]
<alias> tool call <TOOL_NAME> [--arg <KEY=VALUE>]... [--args-file <PATH>] [--args-json <JSON>] [--background]
```

---

## Resource Commands

### Read a Resource

```bash
<alias> get <URI>
<alias> <resource-verb> <URI>        # If profile.resource_verb is set
```

Examples against the `email` server:

```bash
email get mail://inbox
email get mail://thread/123
```

### Resource Templates (auto-generated)

```bash
<alias> <template-name> [--param <value>]...
<alias> <template-name> <positional>          # Single-param templates
```

### Static Bridge Fallback

```bash
<alias> resource list [--filter <PATTERN>] [--all]
<alias> resource read <URI>
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
<alias> prompt list [--filter <PATTERN>] [--all]
<alias> prompt run <PROMPT_NAME> [--arg <KEY=VALUE>]...
```

---

## Auth Commands

```bash
email auth login           # Start browser OAuth, or store a supplied bearer token
email auth logout          # Clear stored credentials
email auth status          # Show current auth state
```

For streamable-HTTP configs, `auth login` starts OAuth authorization-code + PKCE
when no token is supplied. Existing bearer-token workflows still work via piped
stdin (`echo "$TOKEN" | email auth login`) or
`--input-json '{"bearer_token": "<token>"}'`. With `--non-interactive` and no
token available it fails fast.

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
email send --args-file ./payload.json

# From JSON string
email send --args-json '{"to": "user@example.com"}'

# From flags
email send --to user@example.com

# Combined (merged, flags override)
email send --args-file base.json --args-json '{"subject": "Hi"}' --to final@example.com
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
| `SSL_CERT_FILE` | Extra CA certificate(s) (PEM) to trust for every outbound HTTPS connection — MCP transport, telemetry, and OAuth — on top of the bundled roots. Same convention as curl/Python/Go. See [Transports](../features/transports.md#custom-ca-certificates-ssl_cert_file). |

Pattern: `MCP2CLI_` prefix + config path with `__` as separator.

---

## See Also

- [Configuration Reference](config-reference.md) — full YAML schema
- [Getting Started](../getting-started.md) — quick start
- [Discovery-Driven CLI](../features/discovery-driven-cli.md) — how dynamic commands work
