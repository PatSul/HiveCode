#!/bin/bash
# Hive installer — https://hivecode.app
# Usage: curl -fsSL https://hivecode.app/install.sh | bash
set -euo pipefail

REPO="PatSul/Hive"
INSTALL_DIR="/usr/local/bin"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${CYAN}==>${NC} $1"; }
ok()    { echo -e "${GREEN}==>${NC} $1"; }
warn()  { echo -e "${YELLOW}==>${NC} $1"; }
err()   { echo -e "${RED}Error:${NC} $1" >&2; exit 1; }

# ── Detect platform ────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64) ASSET="hive-macos-arm64.dmg" ;;
      *)     err "Unsupported macOS architecture: $ARCH (Apple Silicon required)" ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64) ASSET="hive-linux-x64.tar.gz" ;;
      *)      err "Unsupported Linux architecture: $ARCH" ;;
    esac
    ;;
  *)
    err "Unsupported OS: $OS. Use the Windows installer from https://github.com/$REPO/releases"
    ;;
esac

info "Detected $OS $ARCH"

# ── Get latest release URL ─────────────────────────────────────────
info "Fetching latest release..."
RELEASE_URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep "browser_download_url.*$ASSET" \
  | cut -d '"' -f 4)

if [ -z "$RELEASE_URL" ]; then
  err "Could not find $ASSET in the latest release. Check https://github.com/$REPO/releases"
fi

VERSION=$(echo "$RELEASE_URL" | grep -oP 'v[\d.]+' | head -1)
info "Latest version: $VERSION"

# ── Download ───────────────────────────────────────────────────────
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

info "Downloading $ASSET..."
curl -fsSL "$RELEASE_URL" -o "$TMPDIR/$ASSET"

# ── Install ────────────────────────────────────────────────────────
case "$OS" in
  Darwin)
    info "Mounting DMG..."
    MOUNT_POINT=$(hdiutil attach "$TMPDIR/$ASSET" -nobrowse -readonly | tail -1 | awk '{print $NF}')

    if [ -d "/Applications/Hive.app" ]; then
      warn "Removing previous installation..."
      rm -rf "/Applications/Hive.app"
    fi

    info "Installing Hive.app to /Applications..."
    cp -R "$MOUNT_POINT/Hive.app" "/Applications/"

    hdiutil detach "$MOUNT_POINT" -quiet

    ok "Hive installed to /Applications/Hive.app"
    echo ""
    info "Launch Hive from your Applications folder or Spotlight."
    ;;

  Linux)
    info "Extracting..."
    tar xzf "$TMPDIR/$ASSET" -C "$TMPDIR"

    if [ -w "$INSTALL_DIR" ]; then
      cp "$TMPDIR/hive" "$INSTALL_DIR/hive"
    else
      info "Installing to $INSTALL_DIR (requires sudo)..."
      sudo cp "$TMPDIR/hive" "$INSTALL_DIR/hive"
    fi
    chmod +x "$INSTALL_DIR/hive"

    ok "Hive installed to $INSTALL_DIR/hive"
    echo ""
    info "Run 'hive' to start."
    ;;
esac

echo ""
echo -e "${GREEN}  ╔══════════════════════════════════╗${NC}"
echo -e "${GREEN}  ║${NC}   🐝 Hive installed successfully  ${GREEN}║${NC}"
echo -e "${GREEN}  ╚══════════════════════════════════╝${NC}"
echo ""
