#!/bin/sh
# cc-token-usage installer — downloads a prebuilt binary from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/LokiQ0713/cc-token-usage/master/install.sh | sh
#
# Environment variables:
#   CC_TOKEN_USAGE_INSTALL_DIR  — install directory (default: ~/.local/bin)
#   VERSION                     — specific version to install (default: latest)

set -eu

REPO="LokiQ0713/cc-token-usage"
BINARY="cc-token-usage"
INSTALL_DIR="${CC_TOKEN_USAGE_INSTALL_DIR:-$HOME/.local/bin}"

# ── Helpers ──────────────────────────────────────────────────────────────────

info()  { printf '  \033[1;34m%s\033[0m %s\n' "$1" "$2"; }
error() { printf '  \033[1;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        error "need '$1' (command not found)"
    fi
}

# ── Platform detection ───────────────────────────────────────────────────────

detect_platform() {
    OS=$(uname -s)
    ARCH=$(uname -m)

    case "$OS" in
        Darwin) OS_TRIPLE="apple-darwin" ;;
        Linux)  OS_TRIPLE="unknown-linux-musl" ;;
        *)      error "unsupported OS: $OS" ;;
    esac

    case "$ARCH" in
        x86_64|amd64)  ARCH_TRIPLE="x86_64" ;;
        aarch64|arm64) ARCH_TRIPLE="aarch64" ;;
        *)             error "unsupported architecture: $ARCH" ;;
    esac

    TARGET="${ARCH_TRIPLE}-${OS_TRIPLE}"
}

# ── Fetch utility (curl preferred, wget fallback) ───────────────────────────

fetch() {
    URL="$1"
    if command -v curl >/dev/null 2>&1; then
        if [ -n "${GITHUB_TOKEN:-}" ]; then
            curl -fsSL -H "Authorization: Bearer $GITHUB_TOKEN" "$URL"
        else
            curl -fsSL "$URL"
        fi
    elif command -v wget >/dev/null 2>&1; then
        if [ -n "${GITHUB_TOKEN:-}" ]; then
            wget -qO- --header="Authorization: Bearer $GITHUB_TOKEN" "$URL"
        else
            wget -qO- "$URL"
        fi
    else
        error "neither curl nor wget found — cannot download"
    fi
}

# ── Get latest version from GitHub API ───────────────────────────────────────

get_latest_version() {
    # Primary: follow GitHub's redirect (no API quota needed)
    # https://github.com/REPO/releases/latest redirects to /releases/tag/vX.Y.Z
    if command -v curl >/dev/null 2>&1; then
        VERSION_URL=$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" 2>/dev/null)
        if [ -n "$VERSION_URL" ]; then
            echo "$VERSION_URL" | sed -E 's|.*/v?||'
            return
        fi
    fi

    # Fallback: GitHub API (may be rate-limited for unauthenticated requests)
    fetch "https://api.github.com/repos/$REPO/releases/latest" |
        grep '"tag_name"' |
        sed -E 's/.*"v?([^"]+)".*/\1/'
}

# ── Main ─────────────────────────────────────────────────────────────────────

main() {
    detect_platform

    VERSION="${VERSION:-$(get_latest_version)}"
    VERSION="${VERSION#v}"  # strip leading 'v' if present

    if [ -z "$VERSION" ]; then
        error "could not determine latest version — GitHub API may be rate-limited"
    fi

    ASSET="${BINARY}-${TARGET}.tar.gz"
    URL="https://github.com/$REPO/releases/download/v${VERSION}/${ASSET}"

    info "Platform" "$TARGET"
    info "Version" "v$VERSION"
    info "Install" "$INSTALL_DIR/$BINARY"
    echo ""

    # Create temp directory with cleanup trap
    TMPDIR=$(mktemp -d)
    trap 'rm -rf "$TMPDIR"' EXIT

    # Download
    info "Downloading" "$ASSET"
    fetch "$URL" > "$TMPDIR/$ASSET" || error "download failed — check that v$VERSION exists for $TARGET"

    # Extract
    tar xzf "$TMPDIR/$ASSET" -C "$TMPDIR"

    # Install
    mkdir -p "$INSTALL_DIR"
    mv "$TMPDIR/$BINARY" "$INSTALL_DIR/$BINARY"
    chmod 755 "$INSTALL_DIR/$BINARY"

    # Verify
    if "$INSTALL_DIR/$BINARY" --version >/dev/null 2>&1; then
        echo ""
        info "Installed" "$("$INSTALL_DIR/$BINARY" --version)"
    else
        error "installation verification failed"
    fi

    # PATH check
    case ":$PATH:" in
        *":$INSTALL_DIR:"*)
            ;;
        *)
            echo ""
            printf '  \033[1;33mwarning:\033[0m %s is not in your PATH\n' "$INSTALL_DIR"
            echo ""
            echo "  Add this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
            echo ""
            echo "    export PATH=\"$INSTALL_DIR:\$PATH\""
            echo ""
            ;;
    esac
}

main "$@"
