# Installation Guide — mcp2cli

mcp2cli is a single Rust binary. Once installed it requires no runtime
dependencies, no configuration, and no server to start exploring.

---

## Requirements

| Requirement | Minimum version |
|-------------|----------------|
| Rust toolchain | 1.85 (edition 2024) |
| Operating system | Linux, macOS |
| Architecture | x86_64, aarch64 |

For integration tests a Node.js runtime (≥18) is recommended to run the
`@modelcontextprotocol/server-everything` reference server.

---

## Install from Source (recommended)

```bash
# 1. Clone the repository
git clone https://github.com/mcp2cli/source-code.git
cd mcp2cli

# 2. Build and install
cargo install --path .

# The binary is placed at ~/.cargo/bin/mcp2cli
# Ensure ~/.cargo/bin is on your PATH:
export PATH="$HOME/.cargo/bin:$PATH"
```

### Verify

```bash
mcp2cli --version
```

---

## Install Man Page

After installing the binary, install the man page so that `man mcp2cli` works:

```bash
mcp2cli man install
```

This writes `~/.local/share/man/man1/mcp2cli.1`. Modern `man-db` (Linux) and
`man` (macOS) search `~/.local/share/man` without extra configuration.

To install to a system-wide location:

```bash
mcp2cli man install --dir /usr/local/share/man/man1
```

Verify with:

```bash
man mcp2cli
```

---

## First Run — Ad-hoc Mode (zero config)

No configuration is required to connect to a server and explore it:

```bash
# HTTP server already running at localhost:3001
mcp2cli --url http://127.0.0.1:3001/mcp ls
mcp2cli --url http://127.0.0.1:3001/mcp echo --message hello

# Stdio subprocess server
mcp2cli --stdio "npx @modelcontextprotocol/server-everything" ls
```

---

## Persistent Setup — Named Config + Alias

For day-to-day use, create a named config and a symlink alias:

```bash
# 1. Create a named config
mcp2cli config init --name work \
  --transport streamable_http \
  --endpoint http://127.0.0.1:3001/mcp

# 2. Create a symlink alias + man page
mcp2cli link create --name work

# 3. Put the link directory on your PATH (once)
export PATH="$HOME/.local/bin:$PATH"

# 4. Use the alias as a standalone application
work ls
work echo --message hello
work doctor
man work      # alias man page
```

### Multiple servers

```bash
mcp2cli config init --name dev \
  --transport stdio \
  --stdio-command ./dev-server

mcp2cli config init --name prod \
  --transport streamable_http \
  --endpoint https://prod.api/mcp

mcp2cli link create --name dev
mcp2cli link create --name prod

dev ls
prod doctor
```

---

## PATH Configuration

### Linux (bash / zsh)

Add to your shell profile (`~/.bashrc`, `~/.zshrc`, or `~/.profile`):

```bash
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
```

### macOS

Same as Linux. If using Homebrew's shell, the above export usually belongs in
`~/.zprofile` for login shells.

### Custom link directory

If you prefer symlinks in a different directory:

```bash
mcp2cli link create --name work --dir /usr/local/bin
```

---

## Directory Layout

All mcp2cli data lives under the XDG base directories:

| Path | Purpose |
|------|---------|
| `~/.config/mcp2cli/configs/` | Named configuration files (`<name>.yaml`) |
| `~/.local/share/mcp2cli/host/` | Active-config pointer |
| `~/.local/share/mcp2cli/instances/<name>/` | Per-config state, tokens, job records |
| `~/.local/share/mcp2cli/telemetry.ndjson` | Local telemetry event log |
| `~/.local/bin/` | Symlink aliases (default) |
| `~/.local/share/man/man1/` | Generated man pages (default) |

Override any directory with environment variables:

```bash
export MCP2CLI_CONFIG_DIR=/custom/config
export MCP2CLI_DATA_DIR=/custom/data
export MCP2CLI_BIN_DIR=/custom/bin
```

---

## Updating

```bash
cd path/to/mcp2cli-repo
git pull
cargo install --path . --force

# Refresh man pages after update
mcp2cli man install
```

---

## Uninstalling

```bash
# Remove binary
cargo uninstall mcp2cli

# Remove all data (configs, state, tokens, man pages)
rm -rf ~/.config/mcp2cli
rm -rf ~/.local/share/mcp2cli
rm -f  ~/.local/share/man/man1/mcp2cli.1

# Remove symlink aliases (if in default directory)
ls ~/.local/bin/ | grep -v mcp2cli   # inspect first
# then remove the specific symlinks you created
```

---

## Telemetry

mcp2cli collects anonymous usage telemetry by default (command category,
transport type, feature flags, outcome, duration — never endpoints, tool names,
or argument values). Opt out with any of:

```bash
export MCP2CLI_TELEMETRY=off       # environment variable
export DO_NOT_TRACK=1              # standard opt-out signal
mcp2cli --no-telemetry ls          # per-invocation flag
```

Or set in your config file:

```yaml
telemetry:
  enabled: false
```

---

## Troubleshooting

### `mcp2cli: command not found`

Ensure `~/.cargo/bin` is on your `PATH`. Open a new shell after editing your
profile, or run `source ~/.bashrc`.

### `man mcp2cli` shows nothing

Run `mcp2cli man install` to install the man page, then verify that
`~/.local/share/man` is in your `MANPATH`. On most Linux systems it is
included by default. On macOS you may need:

```bash
export MANPATH="$HOME/.local/share/man:$(manpath)"
```

### `no named config 'work' found`

You attempted `mcp2cli link create --name work` before creating the config.
Create it first:

```bash
mcp2cli config init --name work --transport streamable_http --endpoint http://...
mcp2cli link create --name work
```

Or use `--force` to create the symlink without a config (useful when the config
will be created later):

```bash
mcp2cli link create --name work --force
```

### Connection errors

```bash
# Check server health first
mcp2cli --url http://127.0.0.1:3001/mcp doctor

# Increase timeout
mcp2cli --timeout 30 ls

# Enable debug logging
RUST_LOG=mcp2cli=debug mcp2cli ls
```
