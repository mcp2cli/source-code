#!/usr/bin/env sh
# mcp2cli installer — curl -fsSL https://mcp2cli.dev/install.sh | sh
#
# Downloads a pre-built mcp2cli binary from the matching GitHub
# Release and drops it into ~/.local/bin (override with
# $MCP2CLI_BIN_DIR or $PREFIX). Verifies the archive against the
# release's SHA256SUMS file before extracting. Sends no telemetry.
#
# Optional environment overrides:
#   MCP2CLI_VERSION    Pin a specific release tag (e.g. v0.2.0).
#                      Default: latest.
#   MCP2CLI_BIN_DIR    Where to install the binary.
#                      Default: ~/.local/bin.
#   MCP2CLI_TARGET     Override auto-detected target triple.

set -eu

REPO="mcp2cli/source-code"
BIN="mcp2cli"

# -------- colour helpers --------
if [ -t 2 ]; then
    BOLD="$(printf '\033[1m')"
    DIM="$(printf '\033[2m')"
    GREEN="$(printf '\033[32m')"
    RED="$(printf '\033[31m')"
    YELLOW="$(printf '\033[33m')"
    RESET="$(printf '\033[0m')"
else
    BOLD="" DIM="" GREEN="" RED="" YELLOW="" RESET=""
fi

info()  { printf '%s%s%s %s\n' "$BOLD" "mcp2cli" "$RESET" "$1" >&2; }
warn()  { printf '%s%swarn%s    %s\n' "$BOLD" "$YELLOW" "$RESET" "$1" >&2; }
err()   { printf '%s%serror%s   %s\n' "$BOLD" "$RED" "$RESET" "$1" >&2; }
ok()    { printf '%s%sok%s      %s\n' "$BOLD" "$GREEN" "$RESET" "$1" >&2; }

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        err "required command not found: $1"
        exit 1
    fi
}

# -------- target detection --------
# Maps `uname -s` + `uname -m` to the target triples produced by
# release-binaries.yml. Anything unsupported errors out loudly rather
# than silently picking something wrong.
detect_target() {
    if [ -n "${MCP2CLI_TARGET:-}" ]; then
        printf '%s' "$MCP2CLI_TARGET"
        return
    fi
    uname_s="$(uname -s 2>/dev/null || echo unknown)"
    uname_m="$(uname -m 2>/dev/null || echo unknown)"
    case "$uname_s" in
        Linux)
            # Glibc unless we detect a musl-based userspace.
            libc="gnu"
            if command -v ldd >/dev/null 2>&1; then
                if ldd --version 2>&1 | grep -qi musl; then
                    libc="musl"
                fi
            fi
            if [ -f /etc/alpine-release ]; then
                libc="musl"
            fi
            case "$uname_m" in
                x86_64|amd64)  printf 'x86_64-unknown-linux-%s' "$libc" ;;
                aarch64|arm64) printf 'aarch64-unknown-linux-%s' "$libc" ;;
                *) err "unsupported Linux arch: $uname_m"; exit 1 ;;
            esac
            ;;
        Darwin)
            case "$uname_m" in
                x86_64)        printf 'x86_64-apple-darwin' ;;
                arm64|aarch64) printf 'aarch64-apple-darwin' ;;
                *) err "unsupported macOS arch: $uname_m"; exit 1 ;;
            esac
            ;;
        *)
            err "unsupported OS: $uname_s (Windows is not yet shipped; track https://github.com/${REPO}/issues)"
            exit 1
            ;;
    esac
}

# -------- release tag resolution --------
resolve_tag() {
    if [ -n "${MCP2CLI_VERSION:-}" ]; then
        printf '%s' "$MCP2CLI_VERSION"
        return
    fi
    # GitHub's /releases/latest redirects to the stable tag. We follow
    # the Location header instead of hitting the API to avoid rate limits.
    url="https://github.com/${REPO}/releases/latest"
    tag="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$url" | sed 's|.*/tag/||')"
    if [ -z "$tag" ] || [ "$tag" = "$url" ]; then
        err "could not resolve latest release tag from ${url}"
        exit 1
    fi
    printf '%s' "$tag"
}

# -------- main --------
main() {
    need_cmd curl
    need_cmd tar
    need_cmd uname

    TARGET="$(detect_target)"
    TAG="$(resolve_tag)"
    VERSION="${TAG#v}"
    ARCHIVE="mcp2cli-${VERSION}-${TARGET}.tar.xz"
    BASE="https://github.com/${REPO}/releases/download/${TAG}"

    info "installing ${BOLD}${TAG}${RESET} for ${DIM}${TARGET}${RESET}"

    # Try the target; musl builds may not exist for every ARM variant
    # yet, so fall through to gnu if the 404 says so.
    if ! curl -fsI "${BASE}/${ARCHIVE}" >/dev/null 2>&1; then
        if printf %s "$TARGET" | grep -q 'musl$'; then
            fallback="$(printf %s "$TARGET" | sed 's/musl$/gnu/')"
            warn "no musl build for this release; falling back to ${fallback}"
            TARGET="$fallback"
            ARCHIVE="mcp2cli-${VERSION}-${TARGET}.tar.xz"
        else
            err "no release asset at ${BASE}/${ARCHIVE}"
            exit 1
        fi
    fi

    tmpdir="$(mktemp -d 2>/dev/null || mktemp -d -t mcp2cli)"
    trap 'rm -rf "$tmpdir"' EXIT

    info "downloading ${ARCHIVE}"
    curl -fsSL "${BASE}/${ARCHIVE}"        -o "${tmpdir}/${ARCHIVE}"
    curl -fsSL "${BASE}/SHA256SUMS"         -o "${tmpdir}/SHA256SUMS" || {
        warn "SHA256SUMS not found; trying per-archive sidecar"
        curl -fsSL "${BASE}/${ARCHIVE}.sha256" -o "${tmpdir}/SHA256SUMS"
    }

    # Verify the archive checksum against SHA256SUMS.
    expected="$(grep -E "(^| )${ARCHIVE}\$" "${tmpdir}/SHA256SUMS" | awk '{print $1}' | head -n1)"
    if [ -z "$expected" ]; then
        err "no checksum entry for ${ARCHIVE} in SHA256SUMS"
        exit 1
    fi
    if command -v sha256sum >/dev/null 2>&1; then
        actual="$(sha256sum "${tmpdir}/${ARCHIVE}" | awk '{print $1}')"
    else
        actual="$(shasum -a 256 "${tmpdir}/${ARCHIVE}" | awk '{print $1}')"
    fi
    if [ "$expected" != "$actual" ]; then
        err "checksum mismatch: expected ${expected}, got ${actual}"
        exit 1
    fi
    ok "checksum verified"

    info "extracting archive"
    tar -xJf "${tmpdir}/${ARCHIVE}" -C "${tmpdir}"
    src="${tmpdir}/mcp2cli-${VERSION}-${TARGET}/mcp2cli"
    if [ ! -x "$src" ]; then
        err "extracted binary not found at ${src}"
        exit 1
    fi

    BIN_DIR="${MCP2CLI_BIN_DIR:-${PREFIX:-$HOME/.local}/bin}"
    mkdir -p "$BIN_DIR"
    install -m 0755 "$src" "${BIN_DIR}/${BIN}"

    # macOS Gatekeeper puts a `com.apple.quarantine` xattr on files
    # that arrive via curl/Safari/Chrome. Stripping it avoids the
    # "Apple could not verify … is free of malware" dialog on first
    # launch. Silent no-op on Linux where xattr isn't installed.
    if command -v xattr >/dev/null 2>&1; then
        xattr -d com.apple.quarantine "${BIN_DIR}/${BIN}" 2>/dev/null || true
    fi

    ok  "installed: ${BIN_DIR}/${BIN}"

    case ":${PATH:-}:" in
        *":${BIN_DIR}:"*) ;;
        *) warn "${BIN_DIR} is not on your PATH — add it to your shell profile" ;;
    esac

    info "next: ${DIM}mcp2cli --help${RESET}  ·  https://mcp2cli.dev"
}

main "$@"
