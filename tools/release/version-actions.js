// Nx 22 versionActions for mcp2cli.
//
// Nx bumps `package.json` via the default @nx/js actions; this wrapper
// hooks `afterAllProjectsVersioned` (which runs AFTER every project has
// been versioned but BEFORE Nx commits) to fan the new version out to
// Cargo.toml, Cargo.lock, and docs/files/llms-full.txt. The returned
// `changedFiles` get staged with the release commit — so the tag points
// at a tree where all version strings are consistent.
//
// Why this instead of `postVersionCommand`: that key existed in Nx < 22
// but was removed. `afterAllProjectsVersioned` is its Nx 22 replacement.

const { spawnSync } = require('node:child_process');
const { readFileSync, writeFileSync } = require('node:fs');
const { resolve } = require('node:path');

const jsRelease = require('@nx/js/src/release/version-actions');

const defaultExport = jsRelease.default;
const jsAfterAllProjectsVersioned = jsRelease.afterAllProjectsVersioned;

function read(cwd, path) {
  return readFileSync(resolve(cwd, path), 'utf8');
}

function write(cwd, path, content) {
  writeFileSync(resolve(cwd, path), content);
}

function replaceOnce(content, pattern, replacement, file) {
  if (!pattern.test(content)) {
    throw new Error(`pattern ${pattern} did not match anything in ${file}`);
  }
  return content.replace(pattern, replacement);
}

async function afterAllProjectsVersioned(cwd, opts) {
  console.log(`[mcp2cli-sync] afterAllProjectsVersioned invoked (dryRun=${!!opts.dryRun})`);

  // Run @nx/js's default hook first — that's what updates the lockfile
  // (pnpm-lock.yaml / package-lock.json) for the version bump.
  const jsResult = await jsAfterAllProjectsVersioned(cwd, opts);

  const pkg = JSON.parse(read(cwd, 'package.json'));
  const version = pkg.version;
  if (!/^\d+\.\d+\.\d+(?:-[\w.]+)?$/.test(version)) {
    throw new Error(
      `refusing to propagate non-semver version "${version}" from package.json`,
    );
  }

  const changedFiles = [...jsResult.changedFiles];
  const log = (msg) => console.log(`  [mcp2cli-sync] ${msg}`);

  if (opts.dryRun) {
    log(`would propagate ${version} to Cargo.toml / Cargo.lock / docs/files/llms-full.txt`);
    return {
      changedFiles: [...changedFiles, 'Cargo.toml', 'Cargo.lock', 'docs/files/llms-full.txt'],
      deletedFiles: jsResult.deletedFiles,
    };
  }

  // Cargo.toml — first `version = "..."` under [package].
  {
    const file = 'Cargo.toml';
    const content = read(cwd, file);
    write(
      cwd,
      file,
      replaceOnce(content, /^version\s*=\s*"[^"]+"/m, `version = "${version}"`, file),
    );
    changedFiles.push(file);
    log(`Cargo.toml → ${version}`);
  }

  // Cargo.lock — regenerate via cargo so we don't hand-edit it.
  {
    const res = spawnSync('cargo', ['check', '--offline', '--quiet'], {
      cwd,
      stdio: 'inherit',
    });
    if (res.status === 0) {
      changedFiles.push('Cargo.lock');
      log(`Cargo.lock refreshed via cargo check`);
    } else {
      console.warn(
        `  [mcp2cli-sync] cargo check failed (status ${res.status}); Cargo.lock may need a manual refresh`,
      );
    }
  }

  // docs/files/llms-full.txt — `- Version: X.Y.Z` line.
  {
    const file = 'docs/files/llms-full.txt';
    const content = read(cwd, file);
    write(
      cwd,
      file,
      replaceOnce(content, /^- Version:\s*\S+$/m, `- Version: ${version}`, file),
    );
    changedFiles.push(file);
    log(`llms-full.txt → ${version}`);
  }

  return {
    changedFiles,
    deletedFiles: jsResult.deletedFiles,
  };
}

module.exports = {
  // Default export is the per-project VersionActions class. We don't
  // need to customise anything there — the JS actions already handle
  // reading/writing package.json correctly. Reuse them verbatim.
  default: defaultExport,
  afterAllProjectsVersioned,
};
