---
phase: 01-playable-server-on-the-pi
fixed_at: 2026-08-28T00:00:00Z
review_path: .planning/phases/01-playable-server-on-the-pi/01-REVIEW.md
iteration: 1
findings_in_scope: 10
fixed: 9
skipped: 1
status: partial
---

# Phase 01: Code Review Fix Report

**Fixed at:** 2026-08-28
**Source review:** .planning/phases/01-playable-server-on-the-pi/01-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope (Critical + Warning): 10
- Fixed: 9
- Skipped: 1 (CR-02, deliberate operator decision — see below)
- Info findings (IN-01..IN-06): out of scope for this run, not attempted

**Environment note:** All fixes were applied directly in the live working
tree (no isolated worktree was used for this run — it was invoked as a
direct task, not via the `/gsd-code-review --fix` orchestrator). rlcraft.service
was never stopped or restarted; it remained `active` throughout. systemd unit
changes were installed via `scripts/install-units.sh` + `sudo systemctl
daemon-reload` and verified with `systemd-analyze verify`, but take effect
only on the unit's next (re)start, not on the already-running instance.

## Fixed Issues

### CR-01: RCON password passed on the command line

**Files modified:** `scripts/backup.sh`, `scripts/restore.sh`, `scripts/tps-log.sh`, `systemd/rlcraft.service`
**Commit:** 3504e5e
**Applied fix:** Each script's `rcon()` helper now exports
`RCON_HOST`/`RCON_PORT`/`RCON_PASSWORD` into `rcon-cli`'s environment and
calls it with no `--host`/`--port`/`--password` flags. `rlcraft.service`'s
`ExecStop=` now just runs `rcon-cli stop` — the vars are already in the
process environment via `EnvironmentFile=`.
**Verification:** Confirmed `RCON_HOST=... RCON_PORT=... RCON_PASSWORD=...
rcon-cli list` (no flags) works against the live server (`There are 0/10
players online`). Ran `scripts/tps-log.sh 10s 5s` end-to-end (PASS, median
20.0 TPS) with the new `rcon()` helper — no password ever appears on any
`rcon-cli` argv going forward.

### WR-01: No mutual exclusion between backup.sh and restore.sh

**Files modified:** `scripts/backup.sh`, `scripts/restore.sh`, `.gitignore`
**Commit:** 0fe7392
**Applied fix:** Both scripts now `exec 9>"$ROOT_DIR/.backup.lock"` and
`flock -n 9` immediately after their env/var validation, before any RCON or
filesystem work. Added `.backup.lock` to `.gitignore`.

### WR-02: No boot-time ordering between rlcraft-nft.service and rlcraft.service

**Files modified:** `systemd/rlcraft.service`
**Commit:** 8f7d53a
**Applied fix:** Added `After=rlcraft-nft.service` and
`Wants=rlcraft-nft.service` to `rlcraft.service`. Installed live via
`scripts/install-units.sh` + `daemon-reload`; `systemd-analyze verify`
reported no errors for this unit. Effect is on next start only — the running
instance was left untouched (per environment constraints).

### WR-03: Temurin tarball fallback has no integrity check

**Files modified:** `scripts/preflight.sh`
**Commit:** 536abdb
**Applied fix:** Replaced the unauthenticated `/v3/binary/latest/...` fetch
with Adoptium's `/v3/assets/latest/8/hotspot?...` metadata endpoint, which
returns both a direct download link and its sha256 checksum. The script now
verifies the downloaded tarball against that checksum before `sudo tar`
extracts it, and fails with exit 2 if the API doesn't return both fields or
the checksum doesn't match. Parsed with `grep -oP` rather than `jq` — `jq`
isn't installed yet at this point in the script (Step 5 runs after this
block).
**Verification:** Ran the extraction logic live against
`api.adoptium.net`'s real response; confirmed both `link` and `checksum`
fields parse correctly (`OpenJDK8U-jdk_aarch64_linux_hotspot_8u504b01.tar.gz`,
sha256 `57b7ed8a...`). Did not exercise the `sudo tar` path itself since
Java 8 is already resolved on this host and the fallback branch is not hit.

### WR-04: Predictable, fixed temp-file paths instead of mktemp

**Files modified:** `scripts/preflight.sh`, `scripts/fetch-pack.sh`
**Commit:** 763faf1
**Applied fix:** `preflight.sh`'s `TARBALL` and `fetch-pack.sh`'s unzip-test
log now use `mktemp` with the same descriptive prefix/suffix instead of a
fixed path.

### WR-05: tps-log.sh swallows argument-parsing failures

**Files modified:** `scripts/tps-log.sh`
**Commit:** 3afccd9
**Applied fix:** `DURATION_SEC=$(parse_duration_secs "$DURATION_ARG") ||
exit 1` (same for `INTERVAL_SEC`) so a failed parse now aborts the script
instead of silently truncating the run.
**Verification:** Ran `bash scripts/tps-log.sh bogus 5s` — now prints the
`FATAL: cannot parse duration 'bogus'` message and exits 1 immediately,
instead of silently running one near-instant sample.

### WR-06: reachability.sh depends on dig, never installed by preflight.sh

**Files modified:** `scripts/preflight.sh`, `scripts/reachability.sh`
**Commit:** 3e246ed
**Applied fix:** Added `dnsutils` to `preflight.sh`'s package install list.
Also added a `command -v dig` fail-fast check at the top of
`reachability.sh` (defense in depth for hosts where `preflight.sh` hasn't
run or dnsutils was removed).
**Verification:** `dig` already present on this host; syntax-checked both
scripts.

### WR-07: restore.sh never verifies it isn't running as root

**Files modified:** `scripts/restore.sh`
**Commit:** 921181b
**Applied fix:** Added a guard immediately after the `--help` early-exit:
refuses to proceed unless `id -un` reports `asphacean`.
**Verification:** Ran `sudo bash scripts/restore.sh /nonexistent-archive.tar.zst`
live — confirmed it now prints `FATAL: run this as asphacean, not
root/sudo...` and exits 1 before touching anything. Confirmed
`scripts/restore.sh --help` still works both as `asphacean` and under `sudo`
(the guard sits after the `--help` early return).

### WR-08: cgnat-check.sh's set_env_var silently drops writes if server.env doesn't exist

**Files modified:** `scripts/cgnat-check.sh`
**Commit:** 09f7088
**Applied fix:** `set_env_var` now prints `WARNING: ... does not exist —
CGNAT verdict not persisted, run preflight.sh first` to stderr and returns
early when `$ENV_FILE` is missing, instead of silently no-oping in both
branches.
**Verification:** Extracted and ran the updated function body directly
against a nonexistent env file path — confirmed the warning prints and the
function returns cleanly (exit 0, no crash).

## Skipped Issues

### CR-02: online-mode=false + no whitelist allows username/UUID impersonation

**File:** `server/server.properties.template:40`
**Reason:** Deliberate operator decision, not a defect to auto-fix. This is
the D-09 override documented in `01-01-SUMMARY.md`: the operator explicitly
chose open access (no whitelist) for Phase 1's "friends join easily" goal,
accepting the impersonation risk until Phase 2's token-auth mod closes it.
Flipping `online-mode=true` or gating OP grants here would silently override
that recorded decision without operator sign-off. Recording as a known,
accepted risk per the review's own suggested mitigation — no code change
applied.
**Original issue:** `online-mode=false` combined with the currently open
(no-whitelist) server allows any player to impersonate any username,
including — once `server/ops.json` is populated — an operator's exact
nickname and therefore their offline-mode UUID and permissions, with no
credential check.

---

_Fixed: 2026-08-28_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
