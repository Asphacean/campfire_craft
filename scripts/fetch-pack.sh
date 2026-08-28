#!/usr/bin/env bash
# scripts/fetch-pack.sh
#
# Acquires the RLCraft Server Pack 1.12.2 - Release v2.9.3.zip (CurseForge
# project 285109, file 4612990, ~318.9 MB) via three paths, stopping at the
# first success (Pitfall 1 risk spike):
#   1. CurseForge API (requires CF_API_KEY in server.env)
#   2. Unauthenticated CDN URL guessed from the file ID (LOW confidence)
#   3. An operator-staged zip named by PACK_ZIP in server.env
#
# Any file obtained is gated before being trusted: size >= 300000000 bytes,
# `file` reports a Zip archive, `unzip -t` reports no errors. PACK_SHA256 is
# then pinned (trust-on-first-use) or verified against the existing pin.
#
# Exit codes:
#   0 = verified zip in hand (PACK_ZIP + PACK_SHA256 written to server.env)
#   3 = every automated path refused; operator must supply CF_API_KEY or
#       stage the zip and set PACK_ZIP
#   4 = a file was obtained but failed integrity/hash checks

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
ENV_FILE="$REPO_ROOT/server.env"
DOWNLOAD_DIR="$REPO_ROOT/downloads"
mkdir -p "$DOWNLOAD_DIR"

PROJECT_ID=285109
FILE_ID=4612990
FILENAME="RLCraft Server Pack 1.12.2 - Release v2.9.3.zip"
MIN_SIZE=300000000

log() { echo "[fetch-pack] $*"; }

set_env_var() {
  # Quotes the value — PACK_ZIP paths can contain spaces, and an unquoted
  # value breaks `. ./server.env` sourcing in every script that reads it.
  local key="$1" val="$2"
  local escaped="${val//\"/\\\"}"
  if grep -q "^${key}=" "$ENV_FILE"; then
    sed -i "s|^${key}=.*|${key}=\"${escaped}\"|" "$ENV_FILE"
  else
    printf '%s="%s"\n' "$key" "$escaped" >>"$ENV_FILE"
  fi
}

# shellcheck disable=SC1090
. "$ENV_FILE"

DEST="$DOWNLOAD_DIR/$FILENAME"

# ---------------------------------------------------------------------------
# If an already-verified, pinned zip exists, short-circuit (idempotent —
# re-running does not re-download).
# ---------------------------------------------------------------------------
if [ -n "${PACK_ZIP:-}" ] && [ -f "$PACK_ZIP" ] && [ -n "${PACK_SHA256:-}" ]; then
  EXISTING_SHA="$(sha256sum "$PACK_ZIP" | awk '{print $1}')"
  if [ "$EXISTING_SHA" = "$PACK_SHA256" ]; then
    log "Pinned pack zip already verified at $PACK_ZIP (sha256 matches server.env) — skipping re-download"
    exit 0
  else
    log "FATAL: existing PACK_ZIP ($PACK_ZIP) sha256 ($EXISTING_SHA) does NOT match pinned PACK_SHA256 ($PACK_SHA256)"
    exit 4
  fi
fi

# ---------------------------------------------------------------------------
# Path 1: CurseForge API (requires CF_API_KEY)
# ---------------------------------------------------------------------------
OBTAINED=""

if [ -n "${CF_API_KEY:-}" ]; then
  log "Attempting Path 1: CurseForge API (project $PROJECT_ID, file $FILE_ID)"
  API_RESPONSE="$(curl -fsSL -H "x-api-key: ${CF_API_KEY}" \
    "https://api.curseforge.com/v1/mods/${PROJECT_ID}/files/${FILE_ID}/download-url" 2>&1 || true)"
  DOWNLOAD_URL="$(echo "$API_RESPONSE" | jq -r '.data // empty' 2>/dev/null || true)"
  if [ -n "$DOWNLOAD_URL" ] && [ "$DOWNLOAD_URL" != "null" ]; then
    log "API returned a download URL — fetching"
    if curl -fsSL "$DOWNLOAD_URL" -o "$DEST"; then
      OBTAINED="path1-api"
    else
      log "Path 1: download from API-provided URL failed"
    fi
  else
    log "Path 1 refused: CurseForge API did not return a usable download URL. Response: $(echo "$API_RESPONSE" | head -c 300)"
  fi
else
  log "Path 1 skipped: CF_API_KEY not set in server.env"
fi

# ---------------------------------------------------------------------------
# Path 2: unauthenticated CDN URL guessed from the file ID (LOW confidence)
# ---------------------------------------------------------------------------
if [ -z "$OBTAINED" ]; then
  log "Attempting Path 2: unauthenticated CDN URL (LOW confidence per RESEARCH.md)"
  ENCODED_NAME="$(python3 -c "import urllib.parse,sys; print(urllib.parse.quote(sys.argv[1]))" "$FILENAME" 2>/dev/null \
    || printf '%s' "$FILENAME" | sed 's/ /%20/g')"
  CDN_URL="https://mediafilez.forgecdn.net/files/4612/990/${ENCODED_NAME}"
  if curl -fsSL "$CDN_URL" -o "$DEST"; then
    OBTAINED="path2-cdn"
  else
    log "Path 2 refused: unauthenticated CDN URL did not resolve to the zip"
    rm -f "$DEST"
  fi
fi

# ---------------------------------------------------------------------------
# Path 3: operator-staged zip named by PACK_ZIP
# ---------------------------------------------------------------------------
if [ -z "$OBTAINED" ] && [ -n "${PACK_ZIP:-}" ] && [ -r "${PACK_ZIP:-/nonexistent}" ]; then
  log "Attempting Path 3: operator-staged file at PACK_ZIP=$PACK_ZIP"
  DEST="$PACK_ZIP"
  OBTAINED="path3-staged"
fi

if [ -z "$OBTAINED" ]; then
  log "REFUSED: all automated paths failed and no staged PACK_ZIP was found."
  log "Operator action required — either:"
  log "  (a) set CF_API_KEY in server.env (request a free key at console.curseforge.com), or"
  log "  (b) download '$FILENAME' manually and set PACK_ZIP in server.env to its path"
  exit 3
fi

# ---------------------------------------------------------------------------
# Integrity gate
# ---------------------------------------------------------------------------
log "Obtained via $OBTAINED — running integrity gate on $DEST"

SIZE="$(stat -c %s "$DEST" 2>/dev/null || echo 0)"
if [ "$SIZE" -lt "$MIN_SIZE" ]; then
  log "FATAL: $DEST is $SIZE bytes, below the $MIN_SIZE minimum (likely an HTML error page, not the pack)"
  [ "$OBTAINED" != "path3-staged" ] && rm -f "$DEST"
  exit 4
fi

if ! file "$DEST" | grep -qi 'Zip archive'; then
  log "FATAL: $DEST is not reported as a Zip archive by 'file' — $(file "$DEST")"
  [ "$OBTAINED" != "path3-staged" ] && rm -f "$DEST"
  exit 4
fi

# WR-04: mktemp instead of a fixed, guessable /tmp path (symlink/TOCTOU risk
# in shared /tmp, even though this one is only read back by this script).
UNZIP_TEST_LOG="$(mktemp /tmp/fetch-pack-unzip-test.XXXXXX.log)"
if ! unzip -t "$DEST" >"$UNZIP_TEST_LOG" 2>&1; then
  log "FATAL: unzip -t reported errors for $DEST"
  tail -20 "$UNZIP_TEST_LOG"
  rm -f "$UNZIP_TEST_LOG"
  [ "$OBTAINED" != "path3-staged" ] && rm -f "$DEST"
  exit 4
fi
rm -f "$UNZIP_TEST_LOG"

COMPUTED_SHA="$(sha256sum "$DEST" | awk '{print $1}')"

if [ -z "${PACK_SHA256:-}" ]; then
  log "PACK_SHA256 empty — pinning this download as trust-on-first-use: $COMPUTED_SHA"
  set_env_var PACK_SHA256 "$COMPUTED_SHA"
else
  if [ "$COMPUTED_SHA" != "$PACK_SHA256" ]; then
    log "FATAL: computed sha256 ($COMPUTED_SHA) does not match pinned PACK_SHA256 ($PACK_SHA256)"
    exit 4
  fi
  log "sha256 matches existing pin"
fi

ABS_DEST="$(readlink -f "$DEST")"
set_env_var PACK_ZIP "$ABS_DEST"

log "Verified pack zip: $ABS_DEST ($SIZE bytes, sha256 $COMPUTED_SHA)"
exit 0
