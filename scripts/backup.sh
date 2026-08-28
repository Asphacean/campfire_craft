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

# WR-01: mutual exclusion with restore.sh (and overlapping backup.sh runs) —
# same lock file path as restore.sh.
exec 9>"$ROOT_DIR/.backup.lock"
flock -n 9 || { echo "FATAL: another backup/restore run is in progress" >&2; exit 1; }

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
AUTH_SNAPSHOT_DIR=""
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
  [[ -n "$AUTH_SNAPSHOT_DIR" && -d "$AUTH_SNAPSHOT_DIR" ]] && rm -rf "$AUTH_SNAPSHOT_DIR"
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

# Step 3b: accounts database snapshot (D-13, Phase 2). `.backup` is the only
# WAL-safe way to copy a live SQLite file — a plain `cp` of a WAL database is
# the same class of mistake as tarring a live world, which is exactly why
# this runs right alongside the RCON-paused world snapshot above rather than
# afterward. Staged into <tmp>/auth/campfire.db so the single tar invocation
# below can add it as a second root, producing an `auth/campfire.db` member
# alongside `world/` in the same archive — no second archive file. Missing
# AUTH_DB (Phase 1 hosts, or Phase 2 not yet installed) degrades to a
# world-only archive rather than failing the world backup; the world is the
# irreplaceable artifact.
AUTH_TAR_ARGS=()
if [[ -n "${AUTH_DB:-}" && -f "$AUTH_DB" ]]; then
  AUTH_SNAPSHOT_DIR="$(mktemp -d)"
  mkdir -p "$AUTH_SNAPSHOT_DIR/auth"
  if sqlite3 "$AUTH_DB" ".backup '$AUTH_SNAPSHOT_DIR/auth/campfire.db'" 2>/dev/null; then
    AUTH_TAR_ARGS=(-C "$AUTH_SNAPSHOT_DIR" auth)
  else
    echo "WARNING: sqlite3 .backup of AUTH_DB failed — archive will be world-only" >&2
    rm -rf "$AUTH_SNAPSHOT_DIR"
    AUTH_SNAPSHOT_DIR=""
  fi
else
  echo "INFO: AUTH_DB not set or file missing — archive will be world-only" >&2
fi

# Step 3c: ca/ + caddy/Caddyfile (D-12, Phase 3). Added as a second
# -C "$ROOT_DIR" root, straight from the repo tree (no staging needed — these
# are already-static files, unlike the live SQLite DB above), producing
# ca/... and caddy/Caddyfile members in the SAME archive alongside world/ and
# auth/ — no second archive file. The CA private key riding along here is an
# accepted risk (T-03-01-12, threat register, 03-01-PLAN.md): losing it would
# invalidate every launcher Phase 5 ships, so it must be recoverable.
# Deliberately does NOT include pack/: hundreds of megabytes, fully
# reproducible from scripts/publish-pack.sh at any time — including it would
# blow out BACKUP_KEEP rotations of archive size for zero durability benefit.
CA_TAR_ARGS=()
if [[ -d "$ROOT_DIR/ca" ]]; then
  CA_TAR_ARGS=(-C "$ROOT_DIR" ca caddy/Caddyfile)
else
  echo "INFO: $ROOT_DIR/ca not found — archive will not carry CA/Caddyfile material" >&2
fi

# Step 4: one tar of the whole world/ tree (plus the accounts snapshot and
# the ca/Caddyfile roots staged/named above, if any), relative paths only.
TS="$(date -u +%Y%m%d-%H%M%S)"
ARCHIVE_PATH="$BACKUP_DIR/world-$TS.tar.zst"
if ! tar --zstd -cf "$ARCHIVE_PATH" -C "$ROOT_DIR/server" world "${AUTH_TAR_ARGS[@]}" "${CA_TAR_ARGS[@]}"; then
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
