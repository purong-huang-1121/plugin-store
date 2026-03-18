#!/bin/sh
set -e

# ──────────────────────────────────────────────────────────────
# strategy binary installer / updater (macOS / Linux)
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/purong-huang-1121/plugin-store/main/install_strategy.sh | sh -s -- <strategy-name>
#
# Supported strategies:
#   strategy-auto-rebalance
#   strategy-grid
#   strategy-ranking-sniper
#   strategy-signal-tracker
#   strategy-memepump-scanner
#
# Supported platforms:
#   macOS  : x86_64 (Intel), arm64 (Apple Silicon)
#   Linux  : x86_64, i686, aarch64, armv7l
# ──────────────────────────────────────────────────────────────

REPO="purong-huang-1121/plugin-store"
INSTALL_DIR="$HOME/.cargo/bin"
CACHE_BASE="$HOME/.cargo/bin/.plugin-store"

STRATEGY="$1"

if [ -z "$STRATEGY" ]; then
  echo "Usage: install_strategy.sh <strategy-name>" >&2
  echo "Available strategies:" >&2
  echo "  strategy-auto-rebalance" >&2
  echo "  strategy-grid" >&2
  echo "  strategy-ranking-sniper" >&2
  echo "  strategy-signal-tracker" >&2
  echo "  strategy-memepump-scanner" >&2
  exit 1
fi

CACHE_KEY=$(echo "$STRATEGY" | tr '-' '_')
CACHE_FILE="$CACHE_BASE/last_check_${CACHE_KEY}"
CACHE_TTL=43200  # 12 hours

get_target() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$os" in
    Darwin)
      case "$arch" in
        x86_64) echo "x86_64-apple-darwin" ;;
        arm64)  echo "aarch64-apple-darwin" ;;
        *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
      esac
      ;;
    Linux)
      case "$arch" in
        x86_64)  echo "x86_64-unknown-linux-gnu" ;;
        i686)    echo "i686-unknown-linux-gnu" ;;
        aarch64) echo "aarch64-unknown-linux-gnu" ;;
        armv7l)  echo "armv7-unknown-linux-gnueabihf" ;;
        *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
      esac
      ;;
    *) echo "Unsupported OS" >&2; exit 1 ;;
  esac
}

is_cache_fresh() {
  [ -f "$CACHE_FILE" ] || return 1
  cached_ts=$(cat "$CACHE_FILE" 2>/dev/null | head -1)
  [ -z "$cached_ts" ] && return 1
  now=$(date +%s)
  elapsed=$((now - cached_ts))
  [ "$elapsed" -lt "$CACHE_TTL" ]
}

write_cache() {
  mkdir -p "$CACHE_BASE"
  tmpf="${CACHE_FILE}.tmp.$$"
  date +%s > "$tmpf" && mv "$tmpf" "$CACHE_FILE"
}

get_local_version() {
  if [ -x "$INSTALL_DIR/$STRATEGY" ]; then
    "$INSTALL_DIR/$STRATEGY" --version 2>/dev/null | awk '{print $2}'
  fi
}

normalize_tag() {
  echo "$1" | sed 's/^v//'
}

semver_cmp() {
  if [ "$1" = "$2" ]; then return 0; fi
  local_ifs="$IFS"; IFS='.'
  set -- $1 $2
  IFS="$local_ifs"
  a1=${1:-0}; a2=${2:-0}; a3=${3:-0}
  b1=${4:-0}; b2=${5:-0}; b3=${6:-0}
  # Strip pre-release suffix (e.g. "0-beta" → "0") before numeric compare
  a1=$(echo "$a1" | cut -d'-' -f1)
  a2=$(echo "$a2" | cut -d'-' -f1)
  a3=$(echo "$a3" | cut -d'-' -f1)
  b1=$(echo "$b1" | cut -d'-' -f1)
  b2=$(echo "$b2" | cut -d'-' -f1)
  b3=$(echo "$b3" | cut -d'-' -f1)
  if [ "$a1" -gt "$b1" ] 2>/dev/null; then return 1; fi
  if [ "$a1" -lt "$b1" ] 2>/dev/null; then return 2; fi
  if [ "$a2" -gt "$b2" ] 2>/dev/null; then return 1; fi
  if [ "$a2" -lt "$b2" ] 2>/dev/null; then return 2; fi
  if [ "$a3" -gt "$b3" ] 2>/dev/null; then return 1; fi
  if [ "$a3" -lt "$b3" ] 2>/dev/null; then return 2; fi
  return 0
}

install_binary() {
  target=$(get_target)
  tag="$1"

  binary_name="${STRATEGY}-${target}"
  url="https://github.com/${REPO}/releases/download/${tag}/${binary_name}"
  checksums_url="https://github.com/${REPO}/releases/download/${tag}/checksums.txt"

  echo "Installing ${STRATEGY} ${tag} (${target})..."

  tmpdir=$(mktemp -d)
  trap 'rm -rf "$tmpdir"' EXIT

  # Download with resume support — retries pick up from where they left off
  max_attempts=5
  attempt=0
  while [ $attempt -lt $max_attempts ]; do
    attempt=$((attempt + 1))
    # -C - resumes partial download; exit 33 means server doesn't support resume (start over)
    if curl -fsSL --retry 2 "$url" -o "$tmpdir/$binary_name"; then
      break
    fi
    rm -f "$tmpdir/$binary_name"
    if [ $attempt -lt $max_attempts ]; then
      echo "Download interrupted (attempt $attempt/$max_attempts), resuming in 3s..."
      sleep 3
    else
      echo "Error: download failed after $max_attempts attempts" >&2
      exit 1
    fi
  done
  curl -sSfL "$checksums_url" -o "$tmpdir/checksums.txt"

  expected_hash=$(grep "$binary_name" "$tmpdir/checksums.txt" | awk '{print $1}')
  if [ -z "$expected_hash" ]; then
    echo "Error: no checksum found for $binary_name" >&2
    exit 1
  fi

  if command -v sha256sum >/dev/null 2>&1; then
    actual_hash=$(sha256sum "$tmpdir/$binary_name" | awk '{print $1}')
  elif command -v shasum >/dev/null 2>&1; then
    actual_hash=$(shasum -a 256 "$tmpdir/$binary_name" | awk '{print $1}')
  else
    echo "Error: sha256sum or shasum is required" >&2
    exit 1
  fi

  if [ "$actual_hash" != "$expected_hash" ]; then
    echo "Error: checksum mismatch!" >&2
    exit 1
  fi

  echo "Checksum verified."
  mkdir -p "$INSTALL_DIR"
  mv "$tmpdir/$binary_name" "$INSTALL_DIR/$STRATEGY"
  chmod +x "$INSTALL_DIR/$STRATEGY"

  if [ "$(uname -s)" = "Darwin" ]; then
    xattr -d com.apple.quarantine "$INSTALL_DIR/$STRATEGY" 2>/dev/null || true
  fi

  echo "Installed ${STRATEGY} ${tag} to ${INSTALL_DIR}/${STRATEGY}"
}

ensure_in_path() {
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) return 0 ;;
  esac

  EXPORT_LINE="export PATH=\"\$HOME/.local/bin:\$PATH\""
  shell_name=$(basename "$SHELL" 2>/dev/null || echo "sh")
  case "$shell_name" in
    zsh)  profile="$HOME/.zshrc" ;;
    bash)
      if [ -f "$HOME/.bash_profile" ]; then
        profile="$HOME/.bash_profile"
      elif [ -f "$HOME/.bashrc" ]; then
        profile="$HOME/.bashrc"
      else
        profile="$HOME/.profile"
      fi
      ;;
    *)    profile="$HOME/.profile" ;;
  esac

  if [ -f "$profile" ] && grep -qF '$HOME/.local/bin' "$profile" 2>/dev/null; then
    return 0
  fi

  echo "" >> "$profile"
  echo "# Added by plugin-store installer" >> "$profile"
  echo "$EXPORT_LINE" >> "$profile"
  export PATH="$INSTALL_DIR:$PATH"

  echo "Added $INSTALL_DIR to PATH in $profile"
  echo "Run: source $profile"
}

main() {
  local_ver=$(get_local_version)

  if [ -n "$local_ver" ] && is_cache_fresh; then
    echo "${STRATEGY} ${local_ver} is already installed (update check skipped)."
    return 0
  fi

  api_response=$(curl -sSf "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null) || {
    echo "Warning: could not reach GitHub API." >&2
    if [ -n "$local_ver" ]; then return 0; fi
    echo "Error: no local installation and cannot reach GitHub." >&2
    exit 1
  }

  tag=$(echo "$api_response" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
  if [ -z "$tag" ]; then
    echo "Error: could not determine latest release" >&2
    if [ -n "$local_ver" ]; then return 0; fi
    exit 1
  fi

  latest_ver=$(normalize_tag "$tag")

  if [ -n "$local_ver" ]; then
    semver_cmp "$local_ver" "$latest_ver" || cmp_result=$?
    cmp_result=${cmp_result:-0}
    if [ "$cmp_result" -eq 0 ]; then
      echo "${STRATEGY} ${local_ver} is already up to date."
      write_cache; return 0
    elif [ "$cmp_result" -eq 1 ]; then
      echo "${STRATEGY} ${local_ver} is newer than ${latest_ver}. Skipping."
      write_cache; return 0
    fi
    echo "Upgrading ${STRATEGY} from ${local_ver} to ${latest_ver}..."
  fi

  install_binary "$tag"
  write_cache
  ensure_in_path
  echo "Run '${STRATEGY} --help' to get started."
}

main
