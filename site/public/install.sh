#!/usr/bin/env sh
# mcp2cli installer — curl -fsSL https://mcp2cli.dev/install.sh | sh
#
# Installs mcp2cli from source using cargo. mcp2cli does not yet ship
# pre-built binaries; until it does, this script is the one-command
# way to get the latest main on your PATH.
#
# The installer intentionally sends no telemetry and writes no
# correlation IDs — whatever the CLI does later with its own opt-out
# telemetry is a separate story.

set -eu

REPO="https://github.com/mcp2cli/source-code"
BRANCH="main"
BIN="mcp2cli"

# ANSI colours — only if stderr is a TTY, so `| less` / logs stay clean.
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
        return 1
    fi
}

print_rustup_hint() {
    cat >&2 <<EOF

${BOLD}mcp2cli installs from source with cargo.${RESET}
Install the Rust toolchain first (one-liner, official installer):

    ${DIM}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${RESET}

Then re-run this script. See https://rustup.rs for details.
EOF
}

main() {
    info "installing from ${DIM}${REPO}${RESET} (branch ${BRANCH})"

    if ! need_cmd cargo; then
        print_rustup_hint
        exit 1
    fi

    # cargo installs the binary under $CARGO_HOME/bin (defaults to
    # ~/.cargo/bin). Warn if that's not on PATH so the user can't
    # end up with a silently-unfindable install.
    CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"
    case ":${PATH:-}:" in
        *":${CARGO_BIN}:"*) ;;
        *) warn "${CARGO_BIN} is not on your PATH — add it to your shell profile after install" ;;
    esac

    info "running: cargo install --git ${REPO} --branch ${BRANCH} --locked"
    cargo install --git "$REPO" --branch "$BRANCH" --locked

    if command -v "$BIN" >/dev/null 2>&1; then
        ok  "installed: $(command -v "$BIN")"
        info "next: ${DIM}mcp2cli --help${RESET}  ·  https://mcp2cli.dev"
    else
        warn "cargo finished but '${BIN}' is not on PATH yet"
        warn "open a new shell, or add ${CARGO_BIN} to PATH"
    fi
}

main "$@"
