# Release process

Maintainer-facing. End-users should follow the [install
guide](files/install.md) instead.

## Shape

One command kicks off a release: the **Release** workflow
(`.github/workflows/release.yml`) dispatches to the org's reusable
`nx-cd.yml`, which runs `nx release` inside the repo with the
right preid for the branch. `nx release`:

1. Inspects conventional-commit messages since the last matching
   tag and picks a semver bump (major / minor / patch / prerelease).
2. Writes the new version into `package.json`.
3. Runs the `release:postversion` Nx target, which propagates the
   version into `Cargo.toml`, `Cargo.lock`, and
   `docs/files/llms-full.txt` (see `tools/release/post-version.mjs`).
4. Generates `CHANGELOG.md` from the commit log.
5. Commits the bump (`chore(release): vX.Y.Z`), tags it `vX.Y.Z`,
   pushes, and creates a GitHub Release.

Creating the GitHub Release fires the **Release binaries**
workflow (`.github/workflows/release-binaries.yml`), which builds
six target triples in parallel, tars them, sha256-sums them, and
uploads the artifacts plus an aggregated `SHA256SUMS` file to the
release.

`site/public/install.sh` discovers the newest tag via
`https://github.com/mcp2cli/source-code/releases/latest` and
pulls the matching archive.

## Branch → channel

| Branch    | preid   | dist_tag | GitHub prerelease |
|-----------|---------|----------|-------------------|
| `main`    | *(none)* | `latest` | false             |
| `next`    | `beta`   | `beta`   | true              |
| `develop` | `alpha`  | `alpha`  | true              |

Push-to-`main` triggers a stable release; push-to-`develop` or
`next` cuts an alpha/beta prerelease so those channels stay in
sync with in-flight work. Manual dispatch of the **Release**
workflow lets you force a dry-run or a first-release regardless of
recent commits.

## First release

The first ever tag needs `first_release: true` on the manual
dispatch, otherwise `nx release` tries to read conventional
commits starting from the previous tag (which doesn't exist) and
bails. Go to **Actions → Release → Run workflow**, tick
*first_release*, leave *dry_run* off, and submit.

## Dry-run

Before any tag lands, run:

```bash
pnpm nx release --dry-run
```

locally. It prints the version it would pick, the changelog it
would write, and the commits / tags it would push. Nothing
touches the remote. CI equivalent: dispatch **Release** with
*dry_run: true*.

## Build targets

`release-binaries.yml` currently ships:

- `x86_64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-gnu`
- `aarch64-unknown-linux-musl`
- `x86_64-apple-darwin` (macOS 13)
- `aarch64-apple-darwin` (macOS 14)

Windows is not yet supported — `daemon` mode depends on Unix
sockets (`src/runtime/daemon.rs`) and symlink install uses
`#[cfg(unix)]` blocks. Add a Windows target only after those
codepaths grow fallbacks.

Cross-compilation for Linux targets uses
[`cross`](https://github.com/cross-rs/cross) inside a container;
macOS targets build natively on the matching runner.

## Re-driving a failed matrix job

If one target flakes (transient runner issue, registry hiccup),
re-run just that job:

**Actions → Release binaries → failing run → Re-run failed jobs**.

The `concurrency` guard on the workflow is scoped to the release
tag, so a re-run for the same tag will serialize behind any
in-flight attempt and won't double-upload assets thanks to
`gh release upload --clobber`.

## Yanking a release

1. `git tag -d vX.Y.Z && git push origin :vX.Y.Z` — delete the tag.
2. Delete the GitHub Release via the UI (tick *Delete the git
   tag when you delete the release*).
3. If the buggy version has been pulled by users, push a patch:
   commit `fix: ...` and let the next release cut a `X.Y.Z+1`.
   Don't try to reuse a yanked tag.

## Secrets the Release workflow expects

Provided by the mcp2cli org at the repository or org level:

| Name | Type | Purpose |
|---|---|---|
| `OPERATOR_GITHUB_APPLICATION_ID` | variable | GitHub App id that bypasses branch protection on `main` to push the release commit + tag |
| `OPERATOR_GITHUB_APP_PRIVATE_KEY` | secret | Private key for the app above |
| `NX_NO_CLOUD` | secret/variable | Set so Nx runs fully locally in CI — no Nx Cloud today |

`GITHUB_TOKEN` is always available to workflows; no additional PAT
is required for asset uploads.
