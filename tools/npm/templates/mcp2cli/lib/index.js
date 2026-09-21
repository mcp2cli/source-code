"use strict";

// The `mcp2cli` npm package: locates the native mcp2cli binary that npm
// installed for this platform and runs it.
//
// The binary ships in one small package per platform (`@mcp2cli/linux-x64`, …),
// each marked with `os`/`cpu` and listed here as an optional dependency, so npm
// installs exactly the one that matches and skips the rest. Nothing is
// downloaded at install time and there is no postinstall script.
//
// A CLI published on top of mcp2cli depends on this package and calls `run()`
// from its own `bin` script — see https://mcp2cli.dev/docs/features/branded-cli.

const { spawn } = require("node:child_process");

const PLATFORM_PACKAGES = {
  "linux-x64": "@mcp2cli/linux-x64",
  "linux-arm64": "@mcp2cli/linux-arm64",
  "darwin-x64": "@mcp2cli/darwin-x64",
  "darwin-arm64": "@mcp2cli/darwin-arm64",
};

/**
 * Absolute path of the mcp2cli binary for this platform.
 * `MCP2CLI_BINARY` overrides the lookup (a locally built or system binary).
 */
function binaryPath() {
  if (process.env.MCP2CLI_BINARY) {
    return process.env.MCP2CLI_BINARY;
  }
  const platform = `${process.platform}-${process.arch}`;
  const packageName = PLATFORM_PACKAGES[platform];
  if (!packageName) {
    throw new Error(
      `mcp2cli has no prebuilt binary for ${platform}. ` +
        `Supported: ${Object.keys(PLATFORM_PACKAGES).join(", ")}. ` +
        `Build it from source (cargo install mcp2cli) and set MCP2CLI_BINARY to its path.`
    );
  }
  try {
    return require.resolve(`${packageName}/bin/mcp2cli`);
  } catch {
    throw new Error(
      `The ${packageName} package is not installed. It is an optional dependency of ` +
        `mcp2cli, so this usually means the install ran with --no-optional / --omit=optional. ` +
        `Reinstall without that flag, or set MCP2CLI_BINARY to an mcp2cli binary.`
    );
  }
}

/**
 * Run mcp2cli and exit with its status.
 *
 * With `name` and `config`, the runtime acts as the CLI `name` described by the
 * config file — this is how a published CLI starts. With neither, it is plain
 * `mcp2cli`.
 *
 * @param {object}   [options]
 * @param {string}   [options.name]     Command name the CLI presents itself as.
 * @param {string}   [options.config]   Path to the CLI's config file.
 * @param {string}   [options.version]  Version `--version` reports (the package's).
 * @param {string[]} [options.args]     Arguments; defaults to the process's own.
 * @param {object}   [options.env]      Extra environment for the runtime.
 */
function run(options = {}) {
  const env = { ...process.env, ...options.env };
  if (options.name) env.MCP2CLI_INVOKED_AS = options.name;
  if (options.config) env.MCP2CLI_CONFIG = options.config;
  if (options.version) env.MCP2CLI_BRANDING__VERSION = options.version;

  let binary;
  try {
    binary = binaryPath();
  } catch (error) {
    console.error(`error: ${error.message}`);
    process.exit(1);
  }

  const child = spawn(binary, options.args ?? process.argv.slice(2), {
    stdio: "inherit",
    env,
  });

  // The terminal already delivers Ctrl-C to the whole foreground group; these
  // cover signals sent to this process alone (a supervisor, `kill <pid>`).
  const forwarded = ["SIGINT", "SIGTERM", "SIGHUP"];
  const forward = (signal) => child.kill(signal);
  for (const signal of forwarded) process.on(signal, forward);

  child.on("error", (error) => {
    console.error(`error: could not start ${binary}: ${error.message}`);
    process.exit(1);
  });
  child.on("exit", (code, signal) => {
    for (const forwardedSignal of forwarded) process.removeListener(forwardedSignal, forward);
    if (signal) {
      // Die the way the runtime died, so a caller sees the same status.
      process.kill(process.pid, signal);
    } else {
      process.exit(code ?? 1);
    }
  });
}

module.exports = { binaryPath, run, PLATFORM_PACKAGES };
