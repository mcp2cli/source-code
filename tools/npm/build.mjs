#!/usr/bin/env node
// Assemble the npm distribution of mcp2cli from release archives.
//
//   node tools/npm/build.mjs --version 0.2.0 --archives ./artifacts --out ./npm-dist
//
// `--archives` is a directory of `mcp2cli-<version>-<target>.tar.xz` files as
// uploaded by the release-binaries workflow. For a local smoke test, pass an
// already-built binary instead:
//
//   node tools/npm/build.mjs --version 0.0.0-dev --out /tmp/npm-dist \
//        --binary linux-x64=target/release/mcp2cli
//
// Output: one directory per package under --out, each ready for `npm publish`.
// Platform packages come first in the printed publish order: the `mcp2cli`
// package lists them as optional dependencies at this exact version.

import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, "..", "..");

// Linux ships the musl builds: statically linked, so one binary runs on every
// distribution. The glibc builds require whatever glibc the CI runner had.
const PLATFORMS = [
  { key: "linux-x64", os: "linux", cpu: "x64", target: "x86_64-unknown-linux-musl" },
  { key: "linux-arm64", os: "linux", cpu: "arm64", target: "aarch64-unknown-linux-musl" },
  { key: "darwin-x64", os: "darwin", cpu: "x64", target: "x86_64-apple-darwin" },
  { key: "darwin-arm64", os: "darwin", cpu: "arm64", target: "aarch64-apple-darwin" },
];

const REPOSITORY = { type: "git", url: "git+https://github.com/mcp2cli/source-code.git" };
const HOMEPAGE = "https://mcp2cli.dev";

function parseArgs(argv) {
  const args = { binaries: {} };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    const value = argv[i + 1];
    if (flag === "--version") args.version = value;
    else if (flag === "--archives") args.archives = resolve(value);
    else if (flag === "--out") args.out = resolve(value);
    else if (flag === "--binary") {
      const [key, path] = value.split("=");
      args.binaries[key] = resolve(path);
    } else throw new Error(`unknown argument: ${flag}`);
    i += 1;
  }
  if (!args.version || !args.out) {
    throw new Error("usage: build.mjs --version <v> --out <dir> (--archives <dir> | --binary <platform>=<path>...)");
  }
  return args;
}

/** Path of the binary for `platform`, extracting its release archive if needed. */
function locateBinary(platform, args, scratch) {
  if (args.binaries[platform.key]) return args.binaries[platform.key];
  if (!args.archives) return null;
  const stem = `mcp2cli-${args.version}-${platform.target}`;
  const archive = join(args.archives, `${stem}.tar.xz`);
  if (!existsSync(archive)) return null;
  execFileSync("tar", ["-xJf", archive, "-C", scratch]);
  return join(scratch, stem, "mcp2cli");
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function buildPlatformPackage(platform, binary, args) {
  const name = `@mcp2cli/${platform.key}`;
  const dir = join(args.out, platform.key);
  mkdirSync(join(dir, "bin"), { recursive: true });
  copyFileSync(binary, join(dir, "bin", "mcp2cli"));
  chmodSync(join(dir, "bin", "mcp2cli"), 0o755);
  copyFileSync(join(REPO, "LICENSE"), join(dir, "LICENSE"));
  writeFileSync(
    join(dir, "README.md"),
    `# ${name}\n\nThe mcp2cli binary for ${platform.os} ${platform.cpu} (\`${platform.target}\`).\n\n` +
      `Install [\`mcp2cli\`](https://www.npmjs.com/package/mcp2cli) instead — it depends on the right\n` +
      `binary package for your platform.\n`
  );
  writeJson(join(dir, "package.json"), {
    name,
    version: args.version,
    description: `mcp2cli binary for ${platform.os} ${platform.cpu}`,
    license: "Apache-2.0",
    homepage: HOMEPAGE,
    repository: REPOSITORY,
    os: [platform.os],
    cpu: [platform.cpu],
    files: ["bin", "LICENSE", "README.md"],
    // No "bin" entry: the launcher in the `mcp2cli` package is the command.
    // This package is only a carrier for the file.
    publishConfig: { access: "public" },
  });
  return { name, dir };
}

function buildMainPackage(platforms, args) {
  const dir = join(args.out, "mcp2cli");
  rmSync(dir, { recursive: true, force: true });
  cpSync(join(HERE, "templates", "mcp2cli"), dir, { recursive: true });
  copyFileSync(join(REPO, "LICENSE"), join(dir, "LICENSE"));
  writeJson(join(dir, "package.json"), {
    name: "mcp2cli",
    version: args.version,
    description: "Turn any MCP server into a native command-line application",
    keywords: ["mcp", "model-context-protocol", "cli", "ai", "agents"],
    license: "Apache-2.0",
    homepage: HOMEPAGE,
    repository: REPOSITORY,
    bin: { mcp2cli: "bin/mcp2cli.js" },
    main: "lib/index.js",
    types: "lib/index.d.ts",
    files: ["bin", "lib", "LICENSE", "README.md"],
    engines: { node: ">=18" },
    optionalDependencies: Object.fromEntries(platforms.map(({ name }) => [name, args.version])),
    publishConfig: { access: "public" },
  });
  return { name: "mcp2cli", dir };
}

const args = parseArgs(process.argv.slice(2));
const license = readFileSync(join(REPO, "LICENSE"), "utf8");
if (!license) throw new Error("LICENSE is empty");

mkdirSync(args.out, { recursive: true });
const scratch = mkdtempSync(join(tmpdir(), "mcp2cli-npm-"));
try {
  const built = [];
  const missing = [];
  for (const platform of PLATFORMS) {
    const binary = locateBinary(platform, args, scratch);
    if (binary && existsSync(binary)) built.push(buildPlatformPackage(platform, binary, args));
    else missing.push(platform.key);
  }
  if (built.length === 0) throw new Error("no binaries found — nothing to package");
  // A release must carry every platform; a local smoke test names the ones it has.
  if (missing.length > 0 && args.archives) {
    throw new Error(`release archives missing for: ${missing.join(", ")}`);
  }
  const main = buildMainPackage(built, args);
  for (const { name, dir } of [...built, main]) console.log(`${name}\t${dir}`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
