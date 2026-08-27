---
phase: 01-playable-server-on-the-pi
plan: 03
subsystem: infra
tags: [backup, restore, rcon, systemd-timer, tar-zstd, rlcraft]

# Dependency graph
requires:
  - phase: 01-02
    provides: "rlcraft.service (RCON-controlled, systemd-supervised), scripts/install-units.sh, server.env with RCON_*/BACKUP_* keys, a running world/ tree"
provides:
  - "scripts/backup.sh — RCON save-off/save-all/settle, tar --zstd of the whole world/ tree (DIM-1/DIM1 included), save-on always restored via an EXIT trap, BACKUP_KEEP rotation"
  - "scripts/restore.sh — archive validation, pre-restore safety snapshot (RCON-paused), stop-confirm-move-extract-start sequence, journalctl-based startup detection, --help runbook"
  - "systemd/rlcraft-backup.service + .timer — six-hourly Persistent=true schedule, installed and enabled"
  - "Live-proven restore round trip: an archived world value survives archive -> restore -> running server"
affects: [01-04-network-and-reachability]

# Actuals (#2632)
actuals:
  tokens: 3310
  tasks: 2
  commits: 2

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "RCON save-off/save-all/sleep-5/tar/save-on-via-EXIT-trap for any script that archives a live world tree — applied identically in both backup.sh's scheduled backup and restore.sh's own pre-restore safety snapshot, not just the scheduled path"
    - "journalctl --since <timestamp>, not a grep on server/logs/latest.log, to detect a fresh systemd-managed JVM's startup-complete line — log4j2 only rotates the old latest.log once the new JVM's appender initializes, which is not instantaneous, so a file-based check right after `systemctl start` can read a stale line from the previous session"
    - "ExecStop=-<command> (systemd's leading-dash 'ignore exit code' syntax) for any ExecStop that depends on a possibly-unreachable external service (RCON here) — a failed stop attempt must not turn a clean `systemctl stop` into a unit-level 'failed' result"

key-files:
  created:
    - scripts/backup.sh
    - scripts/restore.sh
    - systemd/rlcraft-backup.service
    - systemd/rlcraft-backup.timer
  modified:
    - systemd/rlcraft.service

key-decisions:
  - "Switched the restore round-trip probe value from the plan's suggested `randomTickSpeed` gamerule to `/worldborder set/get`. RLCraft bundles the globalgamerules mod, which re-applies every vanilla gamerule (including randomTickSpeed) from server/config/globalgamerules.cfg on every dimension load — a value round-tripped correctly through the archive but was silently overwritten back to the mod's config value moments after the restored server finished loading, which would have produced a false negative (or worse, a false confidence) using the plan's literal probe. worldborder is stored in level.dat and is not managed by any mod config on this pack, so it round-trips honestly."
  - "restore.sh's pre-restore safety snapshot now runs the same RCON save-off/save-all/sleep-5 sequence as backup.sh before archiving — the plan's action text only specified this consistency guard for the scheduled backup, but a raw tar of the live world for the safety archive hit the exact anti-pattern RESEARCH.md warns about (tar: file changed as we read it) during live testing. Applied identically to avoid a torn/corrupt safety archive being the one thing standing between a bad restore and data loss."

patterns-established:
  - "Any RCON-gated systemd ExecStop is prefixed with `-` so a transient RCON-unreachable window (e.g. a stop request that races a not-yet-fully-booted restart) degrades to KillMode's SIGTERM fallback instead of marking the unit 'failed'."

requirements-completed: [SRV-03]

coverage:
  - id: D1
    description: "Six-hourly, RCON-paused, zstd-compressed backup of the whole world/ tree with BACKUP_KEEP rotation, save-on guaranteed by an EXIT trap even on failure, archive directory operator-only (mode 700)"
    requirement: "SRV-03"
    verification:
      - kind: manual_procedural
        ref: "bash scripts/backup.sh exits 0 while rlcraft stays active; stat -c %a $BACKUP_DIR == 700; tar --zstd -tf <newest> contains world/level.dat and world/region/ entries, zero absolute paths; two BACKUP_KEEP=1 runs leave exactly 1 world-*.tar.zst; BACKUP_TEST_FAIL_AFTER_SAVEOFF=1 run exits 1 but backup.log still logs 'save-on ok'; systemctl is-enabled rlcraft-backup.timer == enabled, systemd-analyze verify exits 0"
        status: pass
    human_judgment: false
  - id: D2
    description: "restore.sh performs a real archive-to-running-server round trip: pre-restore safety archive first (RCON-paused), service stop confirmed before touching world/, world moved aside not deleted, extraction, restart, startup confirmed via journalctl, refuses a bad archive with nothing touched"
    requirement: "SRV-03"
    verification:
      - kind: manual_procedural
        ref: "bash scripts/restore.sh --help exits 0; a non-archive file is refused (exit 1, rlcraft stays active, no pre-restore archive created); real round trip: /worldborder set 7654321 -> save-all -> backup.sh -> set 60000000 -> confirm 60000000 -> restore.sh <archive> -> /worldborder get reports 7654321; rlcraft.service active after restore, zero 'failed to load'/'exception in world tick' log lines; pre-restore-*.tar.zst present and never pruned by backup.sh's rotation glob"
        status: pass
    human_judgment: false
  - id: D3
    description: "Restored world is the world being played, not a fresh one — position/inventory/chest contents/Nether survive a restore visited in-game"
    requirement: "SRV-03"
    verification: []
    human_judgment: true
    rationale: "Requires a real client on the LAN and human play verification (login, walk through the existing Nether portal, check chest contents) — cannot be exercised from the Pi. Recorded as pending human verification per this plan's autonomous instructions, same pattern as 01-02's LAN-join item."

# Metrics
duration: 22min
completed: 2026-08-27
status: complete
---

# Phase 01 Plan 03: World Backup and a Real Restore Summary

**Six-hourly RCON-paused zstd world backups with BACKUP_KEEP rotation and a guaranteed save-on trap, plus a restore that actually stopped the live server, extracted an archive, and restarted it — proven by a `/worldborder` value that round-tripped through the archive and back into the running world.**

## Performance

- **Duration:** 22 min
- **Started:** ~2026-08-27T13:54:07Z (immediately after 01-02)
- **Completed:** 2026-08-27T14:16:06Z
- **Tasks:** 2
- **Files modified:** 5 (4 created, `systemd/rlcraft.service` amended)

## Accomplishments
- `scripts/backup.sh` takes a consistent, relative-path zstd archive of the whole `world/` tree (Forge 1.12.2's Nether/End live at `world/DIM-1`/`world/DIM1`, inside the same tree — no sibling `world_nether`/`world_the_end` to miss) via RCON `save-off`/`save-all`/5s-settle, restores `save-on` through an EXIT trap on every exit path including forced failure, and rotates to `BACKUP_KEEP` newest `world-*.tar.zst` archives without ever touching `pre-restore-*` archives.
- Rotation proven live, not assumed: two consecutive `BACKUP_KEEP=1` runs left exactly one archive.
- The trap path proven live: a deliberate forced failure after `save-off`/`save-all` (via a guarded `BACKUP_TEST_FAIL_AFTER_SAVEOFF` test hook) still logged `save-on ok` in `backup.log` with exit code 1.
- `systemd/rlcraft-backup.service` + `.timer` installed via the existing `scripts/install-units.sh`, enabled with `systemctl enable --now`; `systemd-analyze verify` clean, next elapse under 6 hours.
- `scripts/restore.sh` performed a real archive-to-running-server round trip: `/worldborder set 7654321` → `save-all` → `backup.sh` → `/worldborder set 60000000` (confirmed) → `restore.sh <archive>` → `/worldborder get` reports `7654321` again — world state proven to travel through the archive and back into the live server, not just file timestamps.
- A bad (non-archive) file handed to `restore.sh` is refused: exit 1, `rlcraft.service` stays active, no pre-restore archive is created, nothing on disk touched.
- Ends the plan with `rlcraft.service` active, `world border 60000000` (default) restored, and a fresh final backup on disk.

## Task Commits

Each task was committed atomically:

1. **Task 1: Six-hourly consistent backups with rotation** - `22c112d` (feat)
2. **Task 2: Restore, actually performed** - `fcf04c5` (feat)

_No plan-metadata/SUMMARY commit made by this executor run per its instructions: `.planning/STATE.md` and `.planning/ROADMAP.md` are intentionally left unmodified; the orchestrator owns those writes._

## Files Created/Modified
- `scripts/backup.sh` - RCON save-off/save-all/settle, tar --zstd of `world/`, save-on via EXIT trap, `BACKUP_KEEP` rotation, `BACKUP_DIR`/`BACKUP_KEEP` overridable from the caller's environment
- `scripts/restore.sh` - validate → RCON-paused pre-restore safety archive → stop+confirm → move-aside → extract → start → journalctl-polled startup confirmation → verdict; `--help` is the runbook
- `systemd/rlcraft-backup.service` + `.timer` - six-hourly `Persistent=true` schedule
- `systemd/rlcraft.service` - `ExecStop` prefixed with `-` (see Deviations)

## Decisions Made
- **Round-trip probe value:** switched from the plan's suggested `randomTickSpeed` gamerule to `/worldborder set`/`get`. See Deviations #3 — `randomTickSpeed` is actively re-enforced by the bundled globalgamerules mod on every dimension load and is not a valid level.dat-resident probe on this modpack.
- **Pre-restore archive consistency:** `restore.sh`'s own safety snapshot of the current world now pauses saving via RCON first, identically to `backup.sh`, rather than a raw `tar` of a live-writing world.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `restore.sh`'s archive validation intermittently false-failed under `set -o pipefail`**
- **Found during:** Task 2, live round-trip testing
- **Issue:** `tar --zstd -tf "$ARCHIVE" | grep -q '^world/level\.dat$'` — under `set -o pipefail`, `grep -q`'s early exit on match can deliver `SIGPIPE` to a still-writing `tar`, making the pipeline report a non-zero status even though the match was found. This fired intermittently (timing-dependent) and produced a false "not a valid world archive" FATAL on genuinely valid archives — confirmed by running the identical `tar | grep -q` pipeline in isolation 5 times with 100% pass, then reproducing the false failure inside the actual script under real system load.
- **Fix:** Capture the full `tar --zstd -tf` listing into a variable first, then `grep -q` against the variable (here-string), which fully avoids the pipe/SIGPIPE race since `tar` completes and exits before `grep` ever runs.
- **Files modified:** `scripts/restore.sh`
- **Verification:** Re-ran the fixed validation against the same archive repeatedly with no false failures; full round trip completed cleanly afterward.
- **Committed in:** `fcf04c5` (Task 2 commit)

**2. [Rule 1 - Bug] `restore.sh`'s pre-restore safety archive read a live-writing world directory**
- **Found during:** Task 2, live round-trip testing
- **Issue:** The safety snapshot of the *current* world (taken before every restore, per D-10) ran a raw `tar --zstd -cf ... -C server world` against the still-running server, hitting exactly the anti-pattern RESEARCH.md's Pattern 2 warns against: `tar: world/region/r.-1.0.mca: file changed as we read it`, aborting the restore under `set -e` before anything else was touched (safe failure, but the safety archive itself would have been silently corrupt had `set -e` not caught it).
- **Fix:** Applied the identical RCON `save-off`/`save-all`/5-second-settle/`save-on` sequence `backup.sh` uses, around the pre-restore archive's `tar` call. `save-on` is restored on both the success and failure paths of this step since the server keeps running if this specific step aborts.
- **Files modified:** `scripts/restore.sh`
- **Verification:** Re-ran the full round trip after the fix — pre-restore archive created cleanly with no tar warnings, restore proceeded to completion.
- **Committed in:** `fcf04c5` (Task 2 commit)

**3. [Rule 1 - Bug] `systemd/rlcraft.service`'s `ExecStop` turned a mid-boot stop attempt into a unit-level "failed" result**
- **Found during:** Task 2, live round-trip testing (rapid back-to-back manual test invocations caught the server mid-restart, before RCON was listening)
- **Issue:** `ExecStop=/usr/local/bin/rcon-cli ... stop` (unprefixed) exits non-zero when RCON is unreachable (e.g. the server is still mid-boot after a prior restart). systemd then marks the unit `Failed (Result: exit-code)` rather than a clean stop, even though `KillMode=mixed` still delivered `SIGTERM` to the JVM and the main process exited with the accepted status 143. `restore.sh`'s stop-and-confirm step correctly detected and refused to proceed on this (leaving nothing touched, pre-restore archive safe), but the underlying fragility would bite any future stop attempt that races a not-yet-fully-booted server, not just this test.
- **Fix:** Prefixed `ExecStop` with `-` (systemd's "ignore this executable's exit code" syntax) — a failed RCON stop attempt no longer marks the unit failed; the graceful-then-SIGTERM fallback via `KillMode=mixed` still applies.
- **Files modified:** `systemd/rlcraft.service` (not in this plan's original `<files>` list — carried forward from 01-02, touched here because it directly blocked Task 2's stop-confirm step)
- **Verification:** Re-installed and re-loaded the unit; the subsequent real round trip's `systemctl stop rlcraft` reached a clean `inactive` state and the restore proceeded normally.
- **Committed in:** `fcf04c5` (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (all Rule 1 — bugs found live during the mandated real round-trip test, exactly the kind of thing SRV-03's "restore actually performed" requirement exists to catch before an operator hits them during a real emergency).
**Impact on plan:** All three were necessary for correctness. #1 and #2 are pure `restore.sh` script-quality/consistency fixes. #3 touches a file outside this plan's listed scope but was strictly necessary to unblock Task 2's stop-confirm requirement — same precedent as 01-02's own out-of-scope-file deviations. No unrelated scope creep.

## Verification-text adjustments (not deviations — the underlying facts match plan intent, only the probe value needed correcting for this modpack)

- **Round-trip probe value:** the plan's `<verify>` block uses `gamerule randomTickSpeed` as the level.dat-resident value proving the round trip. On this server, RLCraft's bundled `globalgamerules` mod re-applies every vanilla gamerule from `server/config/globalgamerules.cfg` on every dimension load, silently overwriting whatever was archived back to the mod's config value (`randomTickSpeed=3`) within seconds of the restored server finishing boot. This was caught live (see Deviations context above) and the probe was switched to `/worldborder set`/`get`, which is level.dat-resident and untouched by any mod config on this pack. Same underlying claim — "world state travels through the archive" — proven with a value that actually survives on this specific modpack.
- **`grep -c 'world/level.dat'` on Task 1's acceptance criterion:** the plan expects this to print `1`; it prints `2` on this world because `level.dat_old` (a standard Minecraft backup-of-the-previous-save file, always present) also contains the substring `level.dat`. Both `world/level.dat` and `world/level.dat_old` are present in every archive, which is the actually-intended fact this criterion checks for.

## Issues Encountered
None beyond the deviations above — all three were caught and fixed within the mandated live-testing loop itself, which is exactly what SRV-03's "restore tested once" requirement is designed to surface.

## User Setup Required
None — no external service configuration required for this plan.

## Pending Human Verification

Per this plan's `autonomous: true` instructions, the following `<human-check>` item from Task 2's `<verify>` block was **not** exercised by this executor (requires a real client and human play-testing) and is recorded here as pending, not blocking:

1. **Restored-world fidelity (Task 2):** join the server from a real client and confirm: (a) spawn position matches where you last logged off, (b) inventory contents match, (c) any chest you left items in still holds them, (d) walking back through your existing Nether portal leads to the same Nether you generated, not freshly generated terrain. A fresh-looking Nether would indicate the exact RESEARCH.md Pitfall 4 failure mode (dimension folders backed up incompletely) — the backup/restore mechanics themselves are proven correct by the automated `/worldborder` round trip, but this is the human-observable confirmation ROADMAP Phase 1 success criterion 4 calls for.

Also recorded in the cross-phase defect ledger (`.planning/WINDOWS.md`, `unrun-verify`, open): `restore.sh`'s "refuses when the rlcraft unit cannot be stopped" acceptance criterion was not forced live — inducing a genuine stop failure would have required destabilizing the single live production instance beyond what this plan's testing budget justified. The refusal logic itself (post-stop `systemctl is-active` check, abort before touching `world/` if not `inactive`) is implemented and was exercised indirectly by deviation #3 above (a real mid-boot stop-attempt failure that the check correctly caught before the `ExecStop` fix).

## Next Phase Readiness
- Plan 01-04 (network/reachability) can proceed unaffected — this plan touched no networking surface.
- `~/rlcraft-backups/` now holds 7 `world-*.tar.zst` archives (well under the 14-archive retention) and 5 `pre-restore-*.tar.zst` safety archives from this plan's own testing; none require cleanup but an operator doing a first real emergency restore should be aware the directory currently also carries this plan's test artifacts.
- `rlcraft-backup.timer` is live and enabled — the next scheduled run will occur within 6 hours of this plan's completion, independent of any operator action.
- `ExecStop`'s `-` prefix (deviation #3) is a durability improvement worth keeping in mind for any future plan that touches `rlcraft.service` again — it changes stop-failure semantics from "unit marked failed" to "best-effort RCON stop, SIGTERM fallback always applies."

---
*Phase: 01-playable-server-on-the-pi*
*Completed: 2026-08-27*

## Self-Check: PASSED
All key files verified present on disk (`scripts/backup.sh`, `scripts/restore.sh`, `systemd/rlcraft-backup.service`, `systemd/rlcraft-backup.timer`, `systemd/rlcraft.service`). Both task commits (`22c112d`, `fcf04c5`) verified present in `git log --oneline --all`. Live system state re-checked at write time: `systemctl is-active rlcraft` = `active`, `systemctl is-enabled rlcraft-backup.timer` = `enabled`, `world border` = `60000000` (default, restored), `~/rlcraft-backups/` contains 7 `world-*.tar.zst` + 5 `pre-restore-*.tar.zst` archives, `backup.log` shows 8 `save-on ok` lines including the deliberate trap-path failure.
