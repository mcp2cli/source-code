#!/usr/bin/env node
/**
 * Post-version sync for mcp2cli releases.
 *
 * `nx release version` writes the new version into the root
 * package.json (the manifest Nx recognises out of the box). This
 * script runs right after and fans that version out to every other
 * place the repo embeds it:
 *
 *   - Cargo.toml               [package] version
 *   - Cargo.lock               the "mcp2cli" entry's version field
 *   - docs/files/llms-full.txt the `- Version: X.Y.Z` line
 *
 * Called by the `release:postversion` Nx target (wired into the
 * release workflow so it fires between `nx release version` and
 * `nx release changelog`).
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');

function read(path) {
  return readFileSync(resolve(repoRoot, path), 'utf8');
}

function write(path, content) {
  writeFileSync(resolve(repoRoot, path), content);
}

function replaceOnce(content, pattern, replacement, file) {
  if (!pattern.test(content)) {
    throw new Error(`pattern ${pattern} did not match anything in ${file}`);
  }
  return content.replace(pattern, replacement);
}

// 1. Read the new version from package.json — Nx has already written it.
const pkg = JSON.parse(read('package.json'));
const version = pkg.version;
if (!/^\d+\.\d+\.\d+(?:-[\w.]+)?$/.test(version)) {
  throw new Error(
    `refusing to propagate non-semver version "${version}" from package.json`,
  );
}
console.log(`[post-version] propagating ${version}`);

// 2. Cargo.toml — first `version = "..."` line inside [package].
{
  const cargo = read('Cargo.toml');
  const updated = replaceOnce(
    cargo,
    /^version\s*=\s*"[^"]+"/m,
    `version = "${version}"`,
    'Cargo.toml',
  );
  write('Cargo.toml', updated);
  console.log('  - Cargo.toml');
}

// 3. Cargo.lock — regenerate via cargo so we don't hand-edit the
//    lockfile. `cargo check --offline` walks the manifest and writes
//    the new version into every [[package]] entry that references us.
try {
  execSync('cargo check --offline --quiet', {
    cwd: repoRoot,
    stdio: 'inherit',
  });
  console.log('  - Cargo.lock (via cargo check --offline)');
} catch (err) {
  // Not fatal — the release workflow runs `cargo check` separately
  // before tagging anyway, so a hiccup here is recoverable.
  console.warn(
    `  ! cargo check failed; Cargo.lock may need a manual refresh: ${err.message}`,
  );
}

// 4. docs/files/llms-full.txt — `- Version: X.Y.Z` line.
{
  const path = 'docs/files/llms-full.txt';
  const content = read(path);
  const updated = replaceOnce(
    content,
    /^- Version:\s*.+$/m,
    `- Version: ${version}`,
    path,
  );
  write(path, updated);
  console.log(`  - ${path}`);
}

console.log(`[post-version] done`);
