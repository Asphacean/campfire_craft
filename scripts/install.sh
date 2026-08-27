#!/usr/bin/env bash
# Idempotent RLCraft server install: verify the pinned pack, unpack it, run the
# Forge 1.12.2-14.23.5.2860 installer, accept the EULA, and render server.properties.
# Safe to re-run (D-03) — every step no-ops if its output already exists.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"

FORGE_VERSION="1.12.2-14.23.5.2860"
FORGE_INSTALLER_URL="https://maven.minecraftforge.net/net/minecraftforge/forge/${FORGE_VERSION}/forge-${FORGE_VERSION}-installer.jar"
DOWNLOADS_DIR="$ROOT_DIR/downloads"
SERVER_DIR="$ROOT_DIR/server"
INSTALLER_JAR="$DOWNLOADS_DIR/forge-${FORGE_VERSION}-installer.jar"

CONFIG_ONLY=false
if [[ "${1:-}" == "--config-only" ]]; then
  CONFIG_ONLY=true
fi

# Mirrors preflight.sh's quoting-safe helper (01-01 Rule 1 deviation fix) — an
# unquoted value with spaces (e.g. PACK_ZIP's filename) breaks every later
# `. ./server.env` sourcing.
set_env_var() {
  local key="$1" val="$2"
  local escaped="${val//\"/\\\"}"
  if grep -q "^${key}=" "$ROOT_DIR/server.env" 2>/dev/null; then
    sed -i "s|^${key}=.*|${key}=\"${escaped}\"|" "$ROOT_DIR/server.env"
  else
    echo "${key}=\"${escaped}\"" >> "$ROOT_DIR/server.env"
  fi
}

render_config() {
  echo "== Rendering server/server.properties from template =="
  local whitelist_flag enforce_flag
  # D-09 override (see 01-01-SUMMARY.md): operator declined a whitelist for
  # Phase 1. WHITELIST_ENABLED=false renders white-list=false /
  # enforce-whitelist=false instead of D-09's true default.
  if [[ "${WHITELIST_ENABLED:-true}" == "true" ]]; then
    whitelist_flag=true
    enforce_flag=true
  else
    whitelist_flag=false
    enforce_flag=false
  fi
  mkdir -p "$SERVER_DIR"
  WHITELIST_FLAG="$whitelist_flag" ENFORCE_WHITELIST_FLAG="$enforce_flag" \
    VIEW_DISTANCE="$VIEW_DISTANCE" MAX_PLAYERS="$MAX_PLAYERS" \
    RCON_PASSWORD="$RCON_PASSWORD" SERVER_NAME="$SERVER_NAME" \
    envsubst '$VIEW_DISTANCE $MAX_PLAYERS $RCON_PASSWORD $SERVER_NAME $WHITELIST_FLAG $ENFORCE_WHITELIST_FLAG' \
    < "$ROOT_DIR/server/server.properties.template" > "$SERVER_DIR/server.properties"
  echo "  rendered: white-list=$whitelist_flag enforce-whitelist=$enforce_flag view-distance=$VIEW_DISTANCE max-players=$MAX_PLAYERS motd=$SERVER_NAME"
}

if $CONFIG_ONLY; then
  render_config
  echo "--config-only: done."
  exit 0
fi

echo "== Step 1: verify pack integrity =="
: "${PACK_ZIP:?PACK_ZIP not set in server.env}"
: "${PACK_SHA256:?PACK_SHA256 not set in server.env}"
: "${JAVA8_BIN:?JAVA8_BIN not set in server.env}"
if [[ ! -f "$PACK_ZIP" ]]; then
  echo "ERROR: PACK_ZIP not found: $PACK_ZIP" >&2
  exit 1
fi
ACTUAL_SHA256=$(sha256sum "$PACK_ZIP" | awk '{print $1}')
if [[ "$ACTUAL_SHA256" != "$PACK_SHA256" ]]; then
  echo "ERROR: pack sha256 mismatch. expected $PACK_SHA256 got $ACTUAL_SHA256" >&2
  exit 1
fi
echo "  pack hash verified: $ACTUAL_SHA256"

echo "== Step 2: unpack pack contents (idempotent, never overwrite) =="
mkdir -p "$SERVER_DIR"
UNIQUE_TOP_COUNT=$(unzip -Z1 "$PACK_ZIP" | sed -E 's#^([^/]+)/.*#\1#' | sort -u | wc -l)
if [[ "$UNIQUE_TOP_COUNT" -eq 1 ]]; then
  echo "  archive has a single wrapping top-level directory — stripping it"
  TMP_EXTRACT=$(mktemp -d)
  unzip -q "$PACK_ZIP" -d "$TMP_EXTRACT"
  WRAP_DIR=$(find "$TMP_EXTRACT" -mindepth 1 -maxdepth 1 -type d | head -1)
  rsync -a --ignore-existing "$WRAP_DIR"/ "$SERVER_DIR"/
  rm -rf "$TMP_EXTRACT"
else
  set +e
  unzip -n -q "$PACK_ZIP" -d "$SERVER_DIR"
  UNZIP_RC=$?
  set -e
  # unzip -n exits 1 (warning) when files are skipped because they already
  # exist — expected and harmless on a re-run. Only >1 is a real failure.
  if [[ "$UNZIP_RC" -gt 1 ]]; then
    echo "ERROR: unzip failed with exit code $UNZIP_RC" >&2
    exit 1
  fi
fi
echo "  unpacked into $SERVER_DIR"

# Discover the launchable jar the Forge installer produces. Prefer a
# "-universal.jar" name (older installer convention); this installer version
# (2860) actually emits a plain "forge-<version>.jar" with no suffix, so fall
# back to any forge-*.jar that isn't the installer/sources jar. Used both for
# the "already installed, skip" check below and to persist SERVER_JAR.
discover_server_jar() {
  local jar
  jar=$(find "$SERVER_DIR" -maxdepth 1 -type f -name "forge-*-universal.jar" 2>/dev/null | head -1)
  if [[ -z "$jar" ]]; then
    jar=$(find "$SERVER_DIR" -maxdepth 1 -type f -name "forge-*.jar" ! -name "*-installer.jar" ! -name "*-sources.jar" 2>/dev/null | head -1)
  fi
  echo "$jar"
}

echo "== Step 3: Forge ${FORGE_VERSION} installer =="
EXISTING_JAR=$(discover_server_jar)
if [[ -n "$EXISTING_JAR" && -d "$SERVER_DIR/libraries" && -n "$(find "$SERVER_DIR/libraries" -type f -print -quit)" ]]; then
  echo "  already installed ($(basename "$EXISTING_JAR")), skipping installer"
else
  mkdir -p "$DOWNLOADS_DIR"
  if [[ ! -f "$INSTALLER_JAR" ]]; then
    echo "  downloading Forge installer..."
    curl -fL -o "$INSTALLER_JAR" "$FORGE_INSTALLER_URL"
  fi
  echo "  running Forge installer (--installServer)..."
  ( cd "$SERVER_DIR" && "$JAVA8_BIN" -jar "$INSTALLER_JAR" --installServer )
fi

echo "== Step 4: discover server jar =="
SERVER_JAR_PATH=$(discover_server_jar)
if [[ -z "$SERVER_JAR_PATH" ]]; then
  echo "ERROR: could not discover the launchable server jar after the Forge installer ran" >&2
  exit 1
fi
SERVER_JAR_NAME="$(basename "$SERVER_JAR_PATH")"
set_env_var SERVER_JAR "$SERVER_JAR_NAME"
# shellcheck disable=SC2034  # re-source so the rest of this run sees the persisted value
SERVER_JAR="$SERVER_JAR_NAME"
echo "  SERVER_JAR=$SERVER_JAR_NAME"

echo "== Step 5: EULA =="
{
  echo "#RLCraft server — EULA accepted by the operator running scripts/install.sh."
  echo "#By running this server you agree to the Mojang EULA: https://aka.ms/MinecraftEULA"
  echo "eula=true"
} > "$SERVER_DIR/eula.txt"

render_config

echo "== install.sh summary =="
echo "  pack hash verified:  $ACTUAL_SHA256"
echo "  server jar:          $SERVER_JAR_NAME"
echo "  config rendered:     server/server.properties"
echo "  eula accepted:       server/eula.txt"
