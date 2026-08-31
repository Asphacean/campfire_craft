#!/usr/bin/env bash
# scripts/publish-launcher.sh
#
# The single LNCH-08 operator command. From one or more built launcher
# artifacts to a signed, live update feed, with no other step:
#   1. Refuse to run if launcher-dist's filesystem is below the MIN_FREE_MB
#      floor (the same floor scripts/publish-pack.sh already enforces —
#      one disk, one shared risk).
#   2. Determine every artifact's platform key from its filename, refusing
#      the whole run before anything is copied if any one of them can't be
#      determined (an unrecognized artifact is refused by name, never
#      guessed).
#   3. Copy each artifact into launcher-dist/ and sign it with the
#      operator's own minisign key (~/.tauri/campfire.key by default,
#      LAUNCHER_SIGNING_KEY_PATH to override) — the D-20/T-04-04-03
#      checkpoint's "pi-only" choice keeps the key on this same host, so
#      signing happens here rather than taking a pre-signed file.
#   4. Write launcher-dist/latest.json atomically (temp file + same-
#      directory rename) describing every platform this run was given.
#   5. Set world-readable permissions for Caddy's file_server and print the
#      curl command that confirms the feed is live.
#
# Signatures are cached by the artifact's own sha256 (downloads/launcher-
# sig-cache/<sha256>.sig): minisign embeds a signing timestamp, so signing
# the same bytes twice produces two different signature strings even
# though nothing about the artifact changed. Without the cache, re-running
# this script for the same version would silently churn every platform's
# `signature` field on every run — the cache is what makes "publish the
# same version and artifacts twice" produce a byte-identical feed (aside
# from pub_date), rather than only an equivalent one.
#
# Exit codes:
#   0 = feed published successfully
#   1 = usage error
#   2 = disk space floor not met (nothing copied)
#   3 = an artifact's platform could not be determined from its filename
#   4 = signing failed
#   5 = feed write failed

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
ENV_FILE="$REPO_ROOT/server.env"
DOWNLOAD_DIR="$REPO_ROOT/downloads"
SIG_CACHE_DIR="$DOWNLOAD_DIR/launcher-sig-cache"
LAUNCHER_DIST_DIR="$REPO_ROOT/launcher-dist"

log() { echo "[publish-launcher] $*" >&2; }

usage() {
  cat <<'EOF'
Usage: scripts/publish-launcher.sh --version <X.Y.Z> [--notes <text>] <artifact> [<artifact> ...]
       scripts/publish-launcher.sh --help

The single LNCH-08 operator command: copies each built launcher artifact
into launcher-dist/, signs it with the operator's own minisign key, and
writes launcher-dist/latest.json atomically. Platform is determined from
each artifact's own filename (Tauri's own naming convention):

  <name>_<version>_x64-setup.exe        -> windows-x86_64
  <name>_<version>_x64_en-US.msi        -> windows-x86_64
  <name>_<version>_x64.app.tar.gz       -> darwin-x86_64 (retired 2026-08-31 — Intel leg removed from CI; mapping kept for old feeds)
  <name>_<version>_aarch64.app.tar.gz   -> darwin-aarch64

  --version <X.Y.Z>   The version this feed advertises (semantic, three
                       dot-separated integers — the same scheme
                       campfire_launcher_core::update compares against).
  --notes <text>       Optional release notes carried in the feed. Empty
                       string if omitted.
  --help               Show this message and exit.

Environment (read from server.env unless overridden):
  LAUNCHER_SIGNING_KEY_PATH       Private key path (default ~/.tauri/campfire.key)
  LAUNCHER_SIGNING_KEY_PASSWORD   Private key password
  MIN_FREE_MB                     Disk-space floor under launcher-dist/'s filesystem
EOF
}

VERSION=""
NOTES=""
ARTIFACTS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:-}"; shift 2 ;;
    --notes) NOTES="${2:-}"; shift 2 ;;
    --help) usage; exit 0 ;;
    -*) echo "Unknown argument: $1" >&2; usage >&2; exit 1 ;;
    *) ARTIFACTS+=("$1"); shift ;;
  esac
done

if [ -z "$VERSION" ]; then
  echo "FATAL: --version is required" >&2
  usage >&2
  exit 1
fi
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "FATAL: --version '$VERSION' is not major.minor.patch (the scheme campfire_launcher_core::update compares against)" >&2
  exit 1
fi
if [ "${#ARTIFACTS[@]}" -eq 0 ]; then
  echo "FATAL: at least one built artifact is required" >&2
  usage >&2
  exit 1
fi

# Capture a caller-supplied MIN_FREE_MB before sourcing server.env, so the
# environment always wins over the file's own default — same convention
# scripts/publish-pack.sh uses, which is what lets a test force the floor
# above what's actually free without editing server.env.
_CALLER_MIN_FREE_MB="${MIN_FREE_MB:-}"

# shellcheck disable=SC1090
. "$ENV_FILE"

if [ -n "$_CALLER_MIN_FREE_MB" ]; then
  MIN_FREE_MB="$_CALLER_MIN_FREE_MB"
fi
MIN_FREE_MB="${MIN_FREE_MB:-5000}"
DOMAIN="${DOMAIN:?DOMAIN must be set in server.env}"
HTTPS_PORT="${HTTPS_PORT:-8444}"
LAUNCHER_SIGNING_KEY_PATH="${LAUNCHER_SIGNING_KEY_PATH:-$HOME/.tauri/campfire.key}"
LAUNCHER_SIGNING_KEY_PASSWORD="${LAUNCHER_SIGNING_KEY_PASSWORD:-}"

# ---------------------------------------------------------------------------
# Pre-flight: refuse to run if free space under launcher-dist's filesystem
# is below MIN_FREE_MB. Checked first, before any artifact is touched.
# ---------------------------------------------------------------------------
check_disk_space() {
  mkdir -p "$LAUNCHER_DIST_DIR"
  local avail_mb
  avail_mb="$(df --output=avail -m "$LAUNCHER_DIST_DIR" | tail -1 | tr -d ' ')"
  if [ "$avail_mb" -lt "$MIN_FREE_MB" ]; then
    local shortfall=$((MIN_FREE_MB - avail_mb))
    log "FATAL: only ${avail_mb} MB free under $LAUNCHER_DIST_DIR's filesystem, need at least ${MIN_FREE_MB} MB (shortfall: ${shortfall} MB). Nothing was copied."
    exit 2
  fi
  log "Disk space OK: ${avail_mb} MB free under $LAUNCHER_DIST_DIR (floor: ${MIN_FREE_MB} MB)"
}

# ---------------------------------------------------------------------------
# Platform key from filename, Tauri's own updater-artifact naming
# convention. Prints the key on stdout and returns 0, or returns 1 (caller
# decides how to report the refusal) with nothing printed.
# ---------------------------------------------------------------------------
detect_platform() {
  local filename="$1"
  case "$filename" in
    *_aarch64.app.tar.gz) echo "darwin-aarch64" ;;
    *_x64.app.tar.gz) echo "darwin-x86_64" ;;
    *_x64-setup.exe|*_x64_en-US.msi) echo "windows-x86_64" ;;
    *) return 1 ;;
  esac
}

# ---------------------------------------------------------------------------
# Every artifact's platform must resolve BEFORE anything is copied — a
# whole-run refusal, not a partial publish. Populates the parallel arrays
# PLATFORM_KEYS[]/PLATFORM_FILES[] (index-aligned) as a side effect.
# ---------------------------------------------------------------------------
PLATFORM_KEYS=()
PLATFORM_FILES=()
resolve_all_platforms() {
  local artifact filename platform
  declare -A _seen_platforms
  for artifact in "${ARTIFACTS[@]}"; do
    [ -f "$artifact" ] || { log "FATAL: artifact not found: $artifact"; exit 3; }
    filename="$(basename "$artifact")"
    if ! platform="$(detect_platform "$filename")"; then
      log "FATAL: could not determine platform from filename: $filename (expected Tauri's own *_x64-setup.exe / *_x64_en-US.msi / *_x64.app.tar.gz / *_aarch64.app.tar.gz naming)"
      exit 3
    fi
    if [ -n "${_seen_platforms[$platform]:-}" ]; then
      log "FATAL: platform $platform already resolved from ${_seen_platforms[$platform]}, refusing duplicate from $filename (a whole-run refusal, not a silent clobber -- e.g. both a .exe and a .msi for windows-x86_64 in one run)"
      exit 3
    fi
    _seen_platforms[$platform]="$filename"
    PLATFORM_KEYS+=("$platform")
    PLATFORM_FILES+=("$artifact")
    log "Resolved $filename -> $platform"
  done
}

# ---------------------------------------------------------------------------
# Signs one artifact, reusing a cached signature for identical bytes
# (see the file header for why this is what makes re-publishing the same
# artifact idempotent). Prints the signature string on stdout.
# ---------------------------------------------------------------------------
sign_artifact() {
  local artifact="$1"
  local sha256 cached
  sha256="$(sha256sum "$artifact" | awk '{print $1}')"
  cached="$SIG_CACHE_DIR/$sha256.sig"
  if [ -f "$cached" ]; then
    log "Signature cache hit for $(basename "$artifact") (sha256 $sha256)"
    cat "$cached"
    return 0
  fi

  [ -f "$LAUNCHER_SIGNING_KEY_PATH" ] || {
    log "FATAL: signing key not found at $LAUNCHER_SIGNING_KEY_PATH"
    exit 4
  }
  mkdir -p "$SIG_CACHE_DIR"

  local sig_out
  sig_out="$(mktemp "$DOWNLOAD_DIR/.launcher-sig.XXXXXX")"
  if ! TAURI_SIGNING_PRIVATE_KEY_PATH="$LAUNCHER_SIGNING_KEY_PATH" \
       TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$LAUNCHER_SIGNING_KEY_PASSWORD" \
       ~/.cargo/bin/cargo tauri signer sign -f "$LAUNCHER_SIGNING_KEY_PATH" -p "$LAUNCHER_SIGNING_KEY_PASSWORD" "$artifact" \
       >"$sig_out" 2>&1; then
    log "FATAL: signing failed for $artifact:"
    cat "$sig_out" >&2
    rm -f "$sig_out"
    exit 4
  fi
  rm -f "$sig_out"

  local produced="${artifact}.sig"
  [ -f "$produced" ] || { log "FATAL: signer reported success but $produced was not written"; exit 4; }
  mv "$produced" "$cached"
  cat "$cached"
}

# ---------------------------------------------------------------------------
# Copies every resolved artifact into launcher-dist/, signs each, and
# writes latest.json atomically. Signing and copying both happen only
# after resolve_all_platforms has confirmed every artifact is nameable —
# no partial feed is ever assembled from a run that was going to fail.
# ---------------------------------------------------------------------------
publish() {
  local pub_date
  pub_date="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

  local platforms_json="{}"
  local i filename dest sig url platform
  for i in "${!PLATFORM_FILES[@]}"; do
    platform="${PLATFORM_KEYS[$i]}"
    filename="$(basename "${PLATFORM_FILES[$i]}")"
    dest="$LAUNCHER_DIST_DIR/$filename"
    cp -f "${PLATFORM_FILES[$i]}" "$dest"
    sig="$(sign_artifact "$dest")"
    # The updater PLUGIN downloads this URL with the OS trust store — it can
    # never accept our private CA, so packages must come from GitHub's
    # publicly-trusted TLS (v0.1.8 Mac UAT: InvalidCertificate(UnknownIssuer)).
    # Trust still rests on the minisign signature below, not the transport.
    url="https://github.com/Asphacean/campfire_craft/releases/download/v${VERSION}/${filename}"
    platforms_json="$(jq --arg k "$platform" --arg url "$url" --arg sig "$sig" \
      '.[$k] = {"url": $url, "signature": $sig}' <<<"$platforms_json")"
    log "Published $filename as $platform"
  done

  local feed_json
  feed_json="$(jq -n --arg version "$VERSION" --arg notes "$NOTES" --arg pub_date "$pub_date" \
    --argjson platforms "$platforms_json" \
    '{version: $version, notes: $notes, pub_date: $pub_date, platforms: $platforms}')"

  local tmp="$LAUNCHER_DIST_DIR/.latest.json.tmp.$$"
  echo "$feed_json" >"$tmp" || { log "FATAL: could not write $tmp"; exit 5; }
  mv "$tmp" "$LAUNCHER_DIST_DIR/latest.json" || { log "FATAL: atomic rename of latest.json failed"; exit 5; }
  chmod -R a+rX "$LAUNCHER_DIST_DIR"
  log "Feed written: $LAUNCHER_DIST_DIR/latest.json ($VERSION, ${#PLATFORM_FILES[@]} platform(s))"
}

main() {
  check_disk_space
  resolve_all_platforms
  publish
  log "Confirm the feed is live: curl -s --cacert ca/campfire-ca.pem https://${DOMAIN}:${HTTPS_PORT}/launcher/latest.json | jq '.version, (.platforms|keys)'"
}

main
