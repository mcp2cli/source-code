# mcp2cli

Turn any [MCP](https://modelcontextprotocol.io) server into a native command-line
application. This package installs the prebuilt `mcp2cli` binary for your
platform — no Rust toolchain, no download at install time.

```sh
npm install -g mcp2cli
mcp2cli --stdio "npx -y @modelcontextprotocol/server-everything" echo --message hi

# or without installing
npx mcp2cli --help
```

Documentation: <https://mcp2cli.dev>

## Publishing your own CLI on top of it

A CLI built with `mcp2cli package init` depends on this package and starts the
runtime from its own `bin` script:

```js
#!/usr/bin/env node
const path = require("node:path");
const { run } = require("mcp2cli");

run({
  name: "email",
  config: path.join(__dirname, "..", "email.yaml"),
  version: require("../package.json").version,
});
```

`run()` resolves the binary, starts it as the CLI `name` described by `config`,
inherits stdio, forwards signals, and exits with its status. `binaryPath()`
returns the binary's location if you need to spawn it yourself. Guide:
<https://mcp2cli.dev/docs/articles/ship-mcp-server-as-cli>

## Platforms

| Platform | Package |
|---|---|
| Linux x64 | `@mcp2cli/linux-x64` |
| Linux arm64 | `@mcp2cli/linux-arm64` |
| macOS x64 | `@mcp2cli/darwin-x64` |
| macOS arm64 | `@mcp2cli/darwin-arm64` |

The Linux binaries are statically linked (musl), so they run on any
distribution regardless of its glibc version, including Alpine. On another
platform, build from source (`cargo install mcp2cli`) and set `MCP2CLI_BINARY`
to the binary's path.
