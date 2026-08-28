#!/usr/bin/env bash
# scripts/publish-pack.sh
#
# The single DIST-02 operator command. From a changed mod or config to new
# hashes live on the file server, with no other step:
#   1. Refuse to run if PACK_DIR's filesystem is below the MIN_FREE_MB floor.
#   2. Acquire the RLCraft 2.9.3 CLIENT base zip (CurseForge project 285109,
#      file 4612979) — a different artifact from Phase 1's SERVER pack zip.
#      Sha-pinned trust-on-first-use, same gate style as scripts/fetch-pack.sh.
#   3. Fetch every {projectID, fileID} the client zip's own manifest.json
#      references, routed by extension into pack/mods/ or pack/resourcepacks/.
#   4. Extract the client zip's overrides/ tree (config, extra mods, scripts,
#      resources, structures, resourcepacks) minus the server-only/options
#      files the locked manifest schema never manages.
#   5. Overlay server/config/ (source of truth) and the campfire-auth jar.
#   6. Regenerate pack/manifest.json atomically (scripts/gen-manifest.py).
#
# --skip-fetch skips step 2-4 entirely (no CurseForge network calls) and
# re-hashes/re-publishes from the pack tree already on disk — the fast path
# for "the operator only edited a config".
#
# Exit codes:
#   0 = pack published successfully
#   1 = usage error
#   2 = disk space floor not met (nothing downloaded)
#   3 = client base zip failed acquisition/integrity/hash gate
#   4 = one or more CurseForge files were refused (see the printed list) —
#       the run stops here; an incomplete pack is never published as if
#       complete
#   5 = manifest generation failed

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
ENV_FILE="$REPO_ROOT/server.env"
DOWNLOAD_DIR="$REPO_ROOT/downloads"
WORK_DIR="$DOWNLOAD_DIR/client-work"

CLIENT_PROJECT_ID=285109
CLIENT_FILE_ID=4612979
CLIENT_ZIP_MIN_SIZE=40000000

log() { echo "[publish-pack] $*"; }

usage() {
  cat <<'EOF'
Usage: scripts/publish-pack.sh [--skip-fetch] [--help]

The single DIST-02 operator command. Acquires the RLCraft 2.9.3 client base
from CurseForge, fetches every mod/resourcepack its manifest references,
overlays server/config/ and the campfire-auth jar, and regenerates
pack/manifest.json atomically — one re-runnable step.

  --skip-fetch   Skip CurseForge entirely; re-hash and re-publish from the
                 pack tree already on disk (fast path for a config-only
                 change).
  --help         Show this message and exit.
EOF
}

SKIP_FETCH=0
for arg in "$@"; do
  case "$arg" in
    --skip-fetch) SKIP_FETCH=1 ;;
    --help) usage; exit 0 ;;
    *) echo "Unknown argument: $arg" >&2; usage >&2; exit 1 ;;
  esac
done

set_env_var() {
  # Quotes the value — paths can contain spaces, and an unquoted value
  # breaks `. ./server.env` sourcing in every script that reads it.
  local key="$1" val="$2"
  local escaped="${val//\"/\\\"}"
  if grep -q "^${key}=" "$ENV_FILE"; then
    sed -i "s|^${key}=.*|${key}=\"${escaped}\"|" "$ENV_FILE"
  else
    printf '%s="%s"\n' "$key" "$escaped" >>"$ENV_FILE"
  fi
}

# Capture a caller-supplied MIN_FREE_MB before sourcing server.env, so the
# environment always wins over the file's own default.
_CALLER_MIN_FREE_MB="${MIN_FREE_MB:-}"

# shellcheck disable=SC1090
. "$ENV_FILE"

if [ -n "$_CALLER_MIN_FREE_MB" ]; then
  MIN_FREE_MB="$_CALLER_MIN_FREE_MB"
fi
MIN_FREE_MB="${MIN_FREE_MB:-5000}"
PACK_DIR="${PACK_DIR:?PACK_DIR must be set in server.env}"

# ---------------------------------------------------------------------------
# Resolve + sanitise a filename from a redirect's final effective URL.
# A filename arriving from a third party (CurseForge) must never be able to
# steer a write outside its target directory (T-03-02-01).
# ---------------------------------------------------------------------------
resolve_and_sanitize_filename() {
  local final_url="$1"
  python3 - "$final_url" <<'PYEOF'
import sys, urllib.parse, os
url = sys.argv[1]
path = url.split('?', 1)[0]
raw = os.path.basename(path)
name = urllib.parse.unquote(raw)
if not name or name in ('.', '..'):
    sys.exit(1)
if '/' in name or '\\' in name:
    sys.exit(1)
if '..' in name:
    sys.exit(1)
if name.startswith('.'):
    sys.exit(1)
if any(ord(c) < 0x20 or ord(c) == 0x7f for c in name):
    sys.exit(1)
print(name)
PYEOF
}

# ---------------------------------------------------------------------------
# Pre-flight: refuse to run if free space under PACK_DIR's filesystem is
# below MIN_FREE_MB. A publish that fills the disk would take the game
# server's world down with it — this check comes first, before anything is
# downloaded.
# ---------------------------------------------------------------------------
check_disk_space() {
  mkdir -p "$PACK_DIR"
  local avail_mb
  avail_mb="$(df --output=avail -m "$PACK_DIR" | tail -1 | tr -d ' ')"
  if [ "$avail_mb" -lt "$MIN_FREE_MB" ]; then
    local shortfall=$((MIN_FREE_MB - avail_mb))
    log "FATAL: only ${avail_mb} MB free under $PACK_DIR's filesystem, need at least ${MIN_FREE_MB} MB (shortfall: ${shortfall} MB). Nothing was downloaded."
    exit 2
  fi
  log "Disk space OK: ${avail_mb} MB free under $PACK_DIR (floor: ${MIN_FREE_MB} MB)"
}

# ---------------------------------------------------------------------------
# Acquire the client base zip — a different artifact from Phase 1's server
# pack. Sha-pinned trust-on-first-use, same integrity gate style as
# scripts/fetch-pack.sh (min size, `file` type, `unzip -t`).
# ---------------------------------------------------------------------------
acquire_client_zip() {
  mkdir -p "$DOWNLOAD_DIR"
  local url="https://www.curseforge.com/api/v1/mods/${CLIENT_PROJECT_ID}/files/${CLIENT_FILE_ID}/download"

  if [ -n "${CLIENT_PACK_ZIP:-}" ] && [ -f "$CLIENT_PACK_ZIP" ] && [ -n "${CLIENT_PACK_SHA256:-}" ]; then
    local existing_sha
    existing_sha="$(sha256sum "$CLIENT_PACK_ZIP" | awk '{print $1}')"
    if [ "$existing_sha" = "$CLIENT_PACK_SHA256" ]; then
      log "Client base zip already verified at $CLIENT_PACK_ZIP (sha256 matches) — skipping re-download"
      return 0
    else
      log "FATAL: existing CLIENT_PACK_ZIP ($CLIENT_PACK_ZIP) sha256 ($existing_sha) does NOT match pinned CLIENT_PACK_SHA256 ($CLIENT_PACK_SHA256)"
      exit 3
    fi
  fi

  log "Fetching client base zip (CurseForge project $CLIENT_PROJECT_ID, file $CLIENT_FILE_ID)"
  local resolve_out resolve_code final_url
  resolve_out="$(curl -sSL -o /dev/null -w '%{http_code} %{url_effective}' --max-time 30 "$url" 2>/dev/null)"
  resolve_code="${resolve_out%% *}"
  final_url="${resolve_out#* }"
  if [ "$resolve_code" != "200" ]; then
    log "FATAL: could not resolve the client zip's download URL (HTTP $resolve_code)"
    exit 3
  fi

  local filename
  if ! filename="$(resolve_and_sanitize_filename "$final_url")"; then
    log "FATAL: client zip filename failed sanitisation ($final_url)"
    exit 3
  fi

  local dest="$DOWNLOAD_DIR/$filename"
  if ! curl -sSL -o "$dest" --max-time 180 "$url"; then
    log "FATAL: client zip download failed"
    rm -f "$dest"
    exit 3
  fi

  local size
  size="$(stat -c %s "$dest" 2>/dev/null || echo 0)"
  if [ "$size" -lt "$CLIENT_ZIP_MIN_SIZE" ]; then
    log "FATAL: $dest is $size bytes, below the $CLIENT_ZIP_MIN_SIZE minimum (likely an HTML error page, not the pack)"
    rm -f "$dest"
    exit 3
  fi
  if ! file "$dest" | grep -qi 'Zip archive'; then
    log "FATAL: $dest is not reported as a Zip archive by 'file' — $(file "$dest")"
    rm -f "$dest"
    exit 3
  fi
  local unzip_log
  unzip_log="$(mktemp /tmp/publish-pack-unzip-test.XXXXXX.log)"
  if ! unzip -t "$dest" >"$unzip_log" 2>&1; then
    log "FATAL: unzip -t reported errors for $dest"
    tail -20 "$unzip_log"
    rm -f "$unzip_log" "$dest"
    exit 3
  fi
  rm -f "$unzip_log"

  local computed_sha
  computed_sha="$(sha256sum "$dest" | awk '{print $1}')"
  if [ -z "${CLIENT_PACK_SHA256:-}" ]; then
    log "CLIENT_PACK_SHA256 empty — pinning this download as trust-on-first-use: $computed_sha"
    set_env_var CLIENT_PACK_SHA256 "$computed_sha"
  elif [ "$computed_sha" != "$CLIENT_PACK_SHA256" ]; then
    log "FATAL: computed sha256 ($computed_sha) does not match pinned CLIENT_PACK_SHA256 ($CLIENT_PACK_SHA256)"
    exit 3
  else
    log "sha256 matches existing pin"
  fi

  local abs_dest
  abs_dest="$(readlink -f "$dest")"
  set_env_var CLIENT_PACK_ZIP "$abs_dest"
  CLIENT_PACK_ZIP="$abs_dest"
  log "Verified client base zip: $abs_dest ($size bytes, sha256 $computed_sha)"
}

extract_client_zip() {
  rm -rf "$WORK_DIR"
  mkdir -p "$WORK_DIR"
  unzip -q "$CLIENT_PACK_ZIP" -d "$WORK_DIR"
}

# ---------------------------------------------------------------------------
# Fetch every {projectID, fileID} the client zip's own manifest.json
# references. Routed by extension: .jar -> mods/, .zip -> resourcepacks/.
# Skip a file already present with non-zero size (resumable). Every failure
# is logged and collected, never aborted on the first one (D-10); only after
# every entry has been attempted does a non-empty failure list stop the run.
# ---------------------------------------------------------------------------
fetch_cf_files() {
  local cf_manifest="$WORK_DIR/manifest.json"
  [ -f "$cf_manifest" ] || { log "FATAL: $cf_manifest not found after unzip"; exit 3; }

  mkdir -p "$PACK_DIR/mods" "$PACK_DIR/resourcepacks"

  local total jar_count=0 zip_count=0 skip_count=0 fail_count=0 i=0
  total="$(jq '.files | length' "$cf_manifest")"
  log "Fetching $total CurseForge entries referenced by the client manifest..."

  local failures=()
  while IFS=$'\t' read -r project_id file_id; do
    i=$((i+1))
    local url="https://www.curseforge.com/api/v1/mods/${project_id}/files/${file_id}/download"
    local resolve_out resolve_code final_url
    resolve_out="$(curl -sSL -o /dev/null -w '%{http_code} %{url_effective}' --max-time 30 "$url" 2>/dev/null)"
    resolve_code="${resolve_out%% *}"
    final_url="${resolve_out#* }"
    if [ "$resolve_code" != "200" ]; then
      log "REFUSED [$i/$total] project=$project_id file=$file_id: could not resolve filename (HTTP $resolve_code)"
      failures+=("project=$project_id file=$file_id reason=resolve-http-$resolve_code")
      fail_count=$((fail_count+1))
      sleep 0.2
      continue
    fi

    local filename
    if ! filename="$(resolve_and_sanitize_filename "$final_url")"; then
      log "REFUSED [$i/$total] project=$project_id file=$file_id: unsafe filename from $final_url"
      failures+=("project=$project_id file=$file_id reason=unsafe-filename")
      fail_count=$((fail_count+1))
      sleep 0.2
      continue
    fi

    local ext="${filename##*.}"
    local destdir=""
    case "$ext" in
      jar) destdir="mods" ;;
      zip) destdir="resourcepacks" ;;
      *)
        log "REFUSED [$i/$total] project=$project_id file=$file_id: unexpected extension '.$ext' ($filename)"
        failures+=("project=$project_id file=$file_id reason=unexpected-extension-$ext")
        fail_count=$((fail_count+1))
        sleep 0.2
        continue
        ;;
    esac

    local destpath="$PACK_DIR/$destdir/$filename"
    if [ -s "$destpath" ]; then
      skip_count=$((skip_count+1))
      if [ "$destdir" = "mods" ]; then jar_count=$((jar_count+1)); else zip_count=$((zip_count+1)); fi
      sleep 0.05
      continue
    fi

    local tmp="${destpath}.part"
    local http_code
    http_code="$(curl -sSL -o "$tmp" -w '%{http_code}' --max-time 60 "$url" 2>/dev/null)"
    if [ "$http_code" != "200" ] || [ ! -s "$tmp" ]; then
      log "REFUSED [$i/$total] project=$project_id file=$file_id: HTTP $http_code"
      failures+=("project=$project_id file=$file_id reason=download-http-$http_code")
      fail_count=$((fail_count+1))
      rm -f "$tmp"
      sleep 0.3
      continue
    fi
    mv "$tmp" "$destpath"
    if [ "$destdir" = "mods" ]; then jar_count=$((jar_count+1)); else zip_count=$((zip_count+1)); fi
    if [ $((i % 25)) -eq 0 ]; then log "...$i/$total processed"; fi
    sleep 0.3
  done < <(jq -r '.files[] | "\(.projectID)\t\(.fileID)"' "$cf_manifest")

  log "Fetch phase done: $jar_count jars, $zip_count resourcepacks present ($skip_count already-present, skipped); $fail_count refused"

  if [ "$fail_count" -gt 0 ]; then
    log "REFUSED FILES (operator must resolve manually, D-10):"
    local f
    for f in "${failures[@]}"; do log "  - $f"; done
    exit 4
  fi
}

# ---------------------------------------------------------------------------
# Extract the client zip's overrides/ tree. files[] is not the whole pack —
# overrides/ carries a mod jar bundled directly (antiquecities) plus five
# other content directories (Pitfall 3). Excludes server-only/options files
# the locked manifest schema never manages (D-08, Pitfall 4).
# ---------------------------------------------------------------------------
extract_overrides() {
  local overrides_dir="$WORK_DIR/overrides"
  [ -d "$overrides_dir" ] || { log "FATAL: $overrides_dir not found in the client zip"; exit 3; }
  # overrides/options.txt and overrides/optionsof.txt are deliberately
  # dropped: the locked manifest schema has no seed-once-never-overwrite
  # flag, so there is no safe way to deliver them without clobbering a
  # returning player's tuned settings on every publish. Closing that gap is
  # Phase 4's problem (RESEARCH.md Pitfall 4), not this manifest's.
  rsync -a \
    --exclude '/server.properties' \
    --exclude '/options.txt' \
    --exclude '/optionsof.txt' \
    --exclude '/*.txt' \
    --exclude 'servers.dat' \
    "$overrides_dir/" "$PACK_DIR/"
}

overlay_own_content() {
  rsync -a --delete "$REPO_ROOT/server/config/" "$PACK_DIR/config/"
  rm -f "$PACK_DIR/mods/campfire-auth-"*.jar
  cp "$REPO_ROOT"/server/mods/campfire-auth-*.jar "$PACK_DIR/mods/"
}

finish_tree() {
  chmod -R a+rX "$PACK_DIR"
  local mod_count rp_count cfg_count tree_size
  mod_count="$(find "$PACK_DIR/mods" -maxdepth 1 -name '*.jar' 2>/dev/null | wc -l)"
  rp_count="$(find "$PACK_DIR/resourcepacks" -maxdepth 1 -name '*.zip' 2>/dev/null | wc -l)"
  cfg_count="$(find "$PACK_DIR/config" -type f 2>/dev/null | wc -l)"
  tree_size="$(du -sh "$PACK_DIR" 2>/dev/null | cut -f1)"
  log "Pack tree: $mod_count mods, $rp_count resourcepacks, $cfg_count config files, $tree_size total"
}

publish_manifest() {
  log "Generating manifest..."
  local rc=0
  python3 "$REPO_ROOT/scripts/gen-manifest.py" "$PACK_DIR" || rc=$?
  if [ "$rc" -ne 0 ]; then
    log "FATAL: manifest generation failed (exit $rc)"
    exit 5
  fi
}

main() {
  check_disk_space

  if [ "$SKIP_FETCH" -eq 0 ]; then
    acquire_client_zip
    extract_client_zip
    fetch_cf_files
    extract_overrides
  else
    log "--skip-fetch: working from the pack tree already on disk, no CurseForge requests"
  fi

  overlay_own_content
  finish_tree
  publish_manifest
  log "Confirm the new manifest is live: curl -s --cacert ca/campfire-ca.pem https://mc.campfire.pub:${HTTPS_PORT:-8444}/manifest.json | jq '.pack_version, (.files|length), (.delete|length)'"
}

main
