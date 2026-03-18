#!/bin/sh
set -e

# ──────────────────────────────────────────────────────────────
# plugin-store local installer
#
# Usage: put this script next to the plugin-store binary, then run:
#   sh install-local.sh
#
# What it does:
#   1. Copy the binary from the script's directory to ~/.cargo/bin/
#   2. Make it executable, remove macOS quarantine flag
#   3. Add ~/.cargo/bin to PATH if needed
# ──────────────────────────────────────────────────────────────

BINARY="plugin-store"
INSTALL_DIR="$HOME/.cargo/bin"

# Resolve script directory (where the binary should be)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

SRC="$SCRIPT_DIR/$BINARY"
if [ ! -f "$SRC" ]; then
  echo "Error: $BINARY not found in $SCRIPT_DIR" >&2
  echo "Place this script in the same directory as the $BINARY binary." >&2
  exit 1
fi

echo "Installing $BINARY from $SCRIPT_DIR ..."

mkdir -p "$INSTALL_DIR"
cp "$SRC" "$INSTALL_DIR/$BINARY"
chmod +x "$INSTALL_DIR/$BINARY"

# macOS: remove Gatekeeper quarantine flag
if [ "$(uname -s)" = "Darwin" ]; then
  xattr -d com.apple.quarantine "$INSTALL_DIR/$BINARY" 2>/dev/null || true
fi

# Verify
ver=$("$INSTALL_DIR/$BINARY" --version 2>/dev/null || echo "unknown")
echo "Installed: $ver → $INSTALL_DIR/$BINARY"

# Ensure ~/.cargo/bin is in PATH
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    shell_name=$(basename "$SHELL" 2>/dev/null || echo "sh")
    case "$shell_name" in
      zsh)  profile="$HOME/.zshrc" ;;
      bash)
        if [ -f "$HOME/.bash_profile" ]; then profile="$HOME/.bash_profile"
        elif [ -f "$HOME/.bashrc" ]; then profile="$HOME/.bashrc"
        else profile="$HOME/.profile"; fi ;;
      *)    profile="$HOME/.profile" ;;
    esac

    if [ -f "$profile" ] && grep -qF '$HOME/.cargo/bin' "$profile" 2>/dev/null; then
      : # already in profile
    else
      echo "" >> "$profile"
      echo "# Added by plugin-store installer" >> "$profile"
      echo "export PATH=\"\$HOME/.cargo/bin:\$PATH\"" >> "$profile"
      echo "Added $INSTALL_DIR to PATH in $profile"
    fi

    echo ""
    echo "To use now, run:  source $profile"
    echo "Or open a new terminal."
    ;;
esac

echo "Done. Run '$BINARY --help' to get started."
