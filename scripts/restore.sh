#!/usr/bin/env bash
# Restore a world-*.tar.zst backup into rlcraft.service (D-10). --help is
# also the operator's restore runbook.
#
# Safety sequence: validate the archive -> archive the CURRENT world first
# (pre-restore-<ts>.tar.zst, never rotated away by backup.sh) -> stop the
# service and confirm it reached inactive -> move the current world/ aside
# to a timestamped sibling (never deleted outright) -> extract -> start the
# service -> poll for the startup-complete log line -> verdict. The
# moved-aside copy is removed only after extraction succeeds. A bad archive
# or a service that will not stop leaves nothing on disk touched.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<'USAGE'
Usage: scripts/restore.sh [archive-path] [--help]

Restores a world-*.tar.zst backup (default: the newest one in BACKUP_DIR)
into the rlcraft server.

Safety behaviour (this is also the operator's restore runbook):
  1. The archive is validated (tar --zstd -tf, must list world/level.dat)
     before anything on disk is touched. A bad archive is refused, nothing
     is touched.
  2. The CURRENT world is archived first to
     $BACKUP_DIR/pre-restore-<UTC timestamp>.tar.zst — a restore never
     destroys the only copy of the present state. This archive is never
     pruned by scripts/backup.sh's rotation.
  3. rlcraft.service is stopped and confirmed inactive before any
     extraction happens; the world is moved aside to a timestamped
     sibling directory, not deleted, and that sibling is removed only
     after extraction succeeds.
  4. rlcraft.service is started again and this script waits for the
     startup-complete log line before printing a verdict naming the
     restored archive and the pre-restore safety archive.

Examples:
  scripts/restore.sh                       # restore the newest world-*.tar.zst
  scripts/restore.sh /path/to/world-X.tar.zst
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"
: "${BACKUP_DIR:?BACKUP_DIR not set in server.env}"
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

ARCHIVE="${1:-}"
if [[ -z "$ARCHIVE" ]]; then
  ARCHIVE="$(ls -t "$BACKUP_DIR"/world-*.tar.zst 2>/dev/null | head -1)"
fi
if [[ -z "$ARCHIVE" || ! -f "$ARCHIVE" ]]; then
  echo "FATAL: no archive found (looked for $BACKUP_DIR/world-*.tar.zst, and no path was given)" >&2
  exit 1
fi

echo "== Validating $ARCHIVE =="
# Captured to a variable rather than piped straight into `grep -q`: under
# `set -o pipefail`, grep -q's early exit on match can SIGPIPE a still-writing
# tar, making the pipeline report failure even though the match was found —
# observed live during 01-03 Task 2 testing (intermittent false FATAL).
ARCHIVE_LISTING="$(tar --zstd -tf "$ARCHIVE" 2>/dev/null)" || {
  echo "FATAL: $ARCHIVE cannot be read as a zstd tar archive — refusing, nothing touched" >&2
  exit 1
}
if ! grep -q '^world/level\.dat$' <<<"$ARCHIVE_LISTING"; then
  echo "FATAL: $ARCHIVE does not look like a valid world archive (no world/level.dat) — refusing, nothing touched" >&2
  exit 1
fi

TS="$(date -u +%Y%m%d-%H%M%S)"
PRE_RESTORE="$BACKUP_DIR/pre-restore-$TS.tar.zst"
echo "== Safety archive of the current world -> $PRE_RESTORE =="
# Same live-write hazard scripts/backup.sh guards against (RESEARCH.md: never
# tar a live-writing world) applies here too — the server is still running
# at this point. save-off/save-all/settle before the tar, save-on right
# after regardless of outcome, since the server keeps running if this step
# aborts. Found live during 01-03 Task 2 testing ("file changed as we read
# it" on a raw tar of the running world).
rcon save-off >/dev/null || { echo "FATAL: RCON unreachable (save-off before pre-restore archive)" >&2; exit 1; }
rcon save-all >/dev/null || { echo "FATAL: RCON unreachable (save-all before pre-restore archive)" >&2; rcon save-on >/dev/null 2>&1 || true; exit 1; }
sleep 5
if ! tar --zstd -cf "$PRE_RESTORE" -C "$ROOT_DIR/server" world; then
  echo "FATAL: pre-restore safety archive failed — refusing to proceed, nothing else touched" >&2
  rcon save-on >/dev/null 2>&1 || true
  rm -f "$PRE_RESTORE"
  exit 1
fi
rcon save-on >/dev/null || echo "WARNING: save-on failed after pre-restore archive" >&2

echo "== Stopping rlcraft.service =="
sudo systemctl stop rlcraft
if [[ "$(systemctl is-active rlcraft 2>/dev/null || true)" != "inactive" ]]; then
  echo "FATAL: rlcraft.service did not reach inactive — refusing to touch world/ (pre-restore archive is safe at $PRE_RESTORE)" >&2
  exit 1
fi

MOVED_ASIDE="$ROOT_DIR/server/world.pre-restore-$TS"
mv "$ROOT_DIR/server/world" "$MOVED_ASIDE"

echo "== Extracting $ARCHIVE =="
if ! tar --zstd -xf "$ARCHIVE" -C "$ROOT_DIR/server"; then
  echo "FATAL: extraction failed — putting the moved-aside world back" >&2
  rm -rf "$ROOT_DIR/server/world"
  mv "$MOVED_ASIDE" "$ROOT_DIR/server/world"
  exit 1
fi
rm -rf "$MOVED_ASIDE"

echo "== Starting rlcraft.service =="
RESTART_START_TS=$(date +%s)
# A couple of seconds' margin before the journalctl --since cutoff, and
# journalctl (not server/logs/latest.log) for the startup-complete poll below
# — log4j2 only rotates the OLD latest.log to a dated .gz once the NEW JVM's
# appender initializes, which is not instantaneous. Polling the file right
# after `systemctl start` can read a *stale* "Done (" line left over from the
# previous session and report a false, ~0s "restart". journalctl timestamps
# each line at the point systemd received it, so filtering by --since is
# race-free. Found live during 01-03 Task 2 testing.
RESTART_START_ISO="$(date -d "@$((RESTART_START_TS - 2))" '+%Y-%m-%d %H:%M:%S')"
sudo systemctl start rlcraft

echo "== Waiting for startup-complete =="
STARTED=0
for _ in $(seq 1 90); do
  if journalctl -u rlcraft --since "$RESTART_START_ISO" --no-pager 2>/dev/null | grep -q 'Done ('; then
    STARTED=1
    break
  fi
  sleep 2
done
ELAPSED=$(( $(date +%s) - RESTART_START_TS ))

if [[ "$STARTED" -ne 1 ]] || ! systemctl is-active --quiet rlcraft; then
  echo "FATAL: rlcraft.service did not reach a confirmed startup within the poll window" >&2
  exit 1
fi

echo "== Restore complete =="
echo "Restored archive: $ARCHIVE"
echo "Pre-restore safety archive: $PRE_RESTORE"
echo "Restart duration: ${ELAPSED}s"
