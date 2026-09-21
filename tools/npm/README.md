# npm distribution

`build.mjs` assembles the npm packages for a release from the archives the
`release-binaries` workflow uploads. The `publish-npm` job in that workflow runs
it and publishes the result; nothing here is a workspace package.

| Package | Contents |
|---------|----------|
| `mcp2cli` | The launcher (`bin/mcp2cli.js`) and the `run()` / `binaryPath()` API in `lib/`. No binary |
| `@mcp2cli/linux-x64`, `@mcp2cli/linux-arm64` | The static musl binary |
| `@mcp2cli/darwin-x64`, `@mcp2cli/darwin-arm64` | The native macOS binary |

`mcp2cli` lists the four platform packages as `optionalDependencies` pinned to
its own version; their `os`/`cpu` fields make npm install exactly one. There is
no postinstall script and no download at install time.

Linux ships the musl builds on purpose: the glibc builds require whatever glibc
the CI runner had, which excludes older distributions.

CLIs scaffolded by `mcp2cli package init` depend on the `mcp2cli` package and
call `run()` from their own `bin` script, so its API in
`templates/mcp2cli/lib/index.js` is public — keep `index.d.ts` in step.

## Local smoke test

```sh
cargo build --release
node tools/npm/build.mjs --version 0.0.0-dev --out /tmp/npm-dist \
  --binary linux-x64=target/release/mcp2cli
(cd /tmp/npm-dist/linux-x64 && npm pack) && (cd /tmp/npm-dist/mcp2cli && npm pack)
```

Install both tarballs into a scratch project and run `npx mcp2cli --version`.

## Publishing

Requires an npm organization named `mcp2cli` (for the `@mcp2cli/*` scope) and an
automation token in the `NPM_TOKEN` repository secret. Without the secret the
job reports a notice and skips; the rest of the release is unaffected.
