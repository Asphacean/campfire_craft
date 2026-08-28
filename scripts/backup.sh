#!/usr/bin/env bash
# Six-hourly consistent world backup (D-10). RCON save-off/save-all before a
# --zstd tar of the whole world/ tree, save-on always restored via an EXIT
# trap, then rotation to BACKUP_KEEP newest world-*.tar.zst archives.
#
# Forge 1.12.2 nests the Nether/End as world/DIM-1 and world/DIM1 *inside*
# world/ (RESEARCH.md Pitfall 4) — there is no sibling world_nether/
# world_the_end here. A single `-C server world` tar already covers
# level.dat, region/, playerdata/, data/, stats/, advancements/ and both
# dimension subfolders together; do not add separate archive members for
# them.
#
# BACKUP_DIR / BACKUP_KEEP are read from server.env but stay overridable by
# the caller's environment (e.g. `BACKUP_KEEP=1 bash scripts/backup.sh`) so
# rotation can be exercised without editing the file.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

_ENV_BACKUP_DIR="${BACKUP_DIR:-}"
_ENV_BACKUP_KEEP="${BACKUP_KEEP:-}"
# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"
BACKUP_DIR="${_ENV_BACKUP_DIR:-$BACKUP_DIR}"
BACKUP_KEEP="${_ENV_BACKUP_KEEP:-$BACKUP_KEEP}"

: "${BACKUP_DIR:?BACKUP_DIR not set in server.env}"
: "${BACKUP_KEEP:?BACKUP_KEEP not set in server.env}"
: "${RCON_HOST:?RCON_HOST not set in server.env}"
: "${RCON_PORT:?RCON_PORT not set in server.env}"
: "${RCON_PASSWORD:?RCON_PASSWORD not set in server.env}"

rcon() {
  # CR-01: never pass RCON_PASSWORD as a CLI flag (visible via ps/proc to any
  # local user) — rcon-cli reads RCON_HOST/RCON_PORT/RCON_PASSWORD from the
  # environment.
  RCON_HOST="$RCON_HOST" RCON_PORT="$RCON_PORT" RCON_PASSWORD="$RCON_PASSWORD" \
    rcon-cli "$@"
}

# Step 1: archive directory, operator-only (Information Disclosure mitigation
# — T-03-04 — world data carries player nicknames and positions).
mkdir -p -m 700 "$BACKUP_DIR"
chmod 700 "$BACKUP_DIR"
LOG_FILE="$BACKUP_DIR/backup.log"

ARCHIVE_PATH=""
START_TS=$(date +%s)

# Step 2: EXIT trap that always issues RCON save-on, so an abort anywhere
# below (including under `set -e`) cannot leave world saving disabled — the
# single worst failure mode of this script (T-03-01).
on_exit() {
  local ec=$?
  set +e
  if rcon save-on >/dev/null 2>&1; then
    local elapsed=$(( $(date +%s) - START_TS ))
    local size=0
    [[ -n "$ARCHIVE_PATH" && -f "$ARCHIVE_PATH" ]] && size=$(stat -c %s "$ARCHIVE_PATH")
    printf '%s archive=%s size=%sB elapsed=%ss save-on ok\n' \
      "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "${ARCHIVE_PATH:-none}" "$size" "$elapsed" \
      >> "$LOG_FILE" 2>/dev/null
  else
    printf '%s ERROR save-on failed after exit code %s\n' \
      "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$ec" >> "$LOG_FILE" 2>/dev/null
  fi
  exit "$ec"
}
trap on_exit EXIT

# Step 3: pause saving, force a flush, let it land on disk.
rcon save-off >/dev/null || { echo "FATAL: RCON unreachable (save-off)" >&2; exit 1; }
rcon save-all >/dev/null || { echo "FATAL: RCON unreachable (save-all)" >&2; exit 1; }
sleep 5

# Test hook (not a general feature): proves the trap path fires and world
# saving is restored even when the run aborts mid-backup, per this plan's own
# acceptance criteria. No-op unless the caller explicitly sets the var.
if [[ -n "${BACKUP_TEST_FAIL_AFTER_SAVEOFF:-}" ]]; then
  echo "TEST: forced failure after save-off/save-all (trap path exercise)" >&2
  exit 1
fi

# Step 4: one tar of the whole world/ tree, relative paths only.
TS="$(date -u +%Y%m%d-%H%M%S)"
ARCHIVE_PATH="$BACKUP_DIR/world-$TS.tar.zst"
if ! tar --zstd -cf "$ARCHIVE_PATH" -C "$ROOT_DIR/server" world; then
  echo "FATAL: tar failed" >&2
  rm -f "$ARCHIVE_PATH"
  ARCHIVE_PATH=""
  exit 1
fi

SIZE=$(stat -c %s "$ARCHIVE_PATH")
if [[ "$SIZE" -lt 1048576 ]]; then
  echo "FATAL: archive smaller than 1MB ($SIZE bytes) — refusing, treating as a bad backup" >&2
  exit 1
fi

# Step 6: rotate to BACKUP_KEEP newest world-*.tar.zst. pre-restore-* archives
# (written by restore.sh) are never matched by this glob and are never
# rotated away (T-03-07).
mapfile -t ARCHIVES < <(ls -t "$BACKUP_DIR"/world-*.tar.zst 2>/dev/null)
if [[ "${#ARCHIVES[@]}" -gt "$BACKUP_KEEP" ]]; then
  for old in "${ARCHIVES[@]:$BACKUP_KEEP}"; do
    rm -f -- "$old"
  done
fi

echo "Backup OK: $ARCHIVE_PATH ($SIZE bytes)"
