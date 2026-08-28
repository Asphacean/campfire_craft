---
phase: 02-accounts-enforced-auth
fixed_at: 2026-08-28T12:55:52Z
review_path: .planning/phases/02-accounts-enforced-auth/02-REVIEW.md
iteration: 1
findings_in_scope: 5
fixed: 5
skipped: 0
status: all_fixed
---

# Phase 02: Code Review Fix Report

**Fixed at:** 2026-08-28T12:55:52Z
**Source review:** .planning/phases/02-accounts-enforced-auth/02-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 5 (WR-01..WR-05; Info findings out of scope per fix_scope)
- Fixed: 5
- Skipped: 0

## Fixed Issues

### WR-01: Rate limiter has a check-then-record race on the `/login` path

**Files modified:** `auth-service/src/ratelimit.rs`, `auth-service/src/api.rs`
**Commit:** `3304d5f`
**Applied fix:** Replaced the `would_allow()` (peek) / `record_failure()` (record-after-the-fact) split with a single atomic reservation: `/login` now calls the same `check()` `/register` already used (test-and-record under one mutex acquisition) right before the password verification, and calls a new `RateLimiter::refund()` to pop the reservation back off on a successful login. `would_allow`/`record_failure` were removed as dead code. Verified: `cargo build --release` clean, `scripts/auth-smoke.sh` 28/28 against a fresh ephemeral instance.

### WR-02: No dedup/guard on repeated `AuthResponseMessage` packets while a join is pending

**Files modified:** `mods-src/campfire-auth/src/main/java/pub/campfire/auth/server/ServerAuthHandler.java`
**Commit:** `df928ff`
**Applied fix:** Added a `boolean validating` field to `PendingJoin` (main-thread-only, no synchronization needed). `onResponseReceived` now ignores any packet if `pending.validating` is already `true`, and sets it `true` before spawning the single `validateAsync` call — so a modified/malicious client sending repeated response packets during the pending window can no longer spawn unbounded HTTP validate calls or race a legitimately-valid response into a spurious kick.

### WR-03: SQLite database file has a brief window at a non-restrictive mode before `chmod(0600)`

**Files modified:** `auth-service/src/db.rs`, `systemd/campfire-auth.service`
**Commit:** `4a105fe`
**Applied fix:** Added `UMask=0077` to `campfire-auth.service`'s hardening block (matches the existing 600-file-mode intent for both files and directories, unlike `UMask=0177` which would strip the owner execute bit needed for directories) and set the same restrictive process umask in `Db::open()` via a direct `unsafe extern "C" { fn umask(...) }` FFI call (no new crate — libc is already linked) before `Connection::open()` runs, so `campfire-auth login`/`reset`/devserver invocations outside the unit get the same guarantee. The existing post-creation `chmod(0600)` loop is kept as defense in depth. Verified: `cargo build --release` clean, unit parses (`systemd-analyze verify`), `scripts/auth-smoke.sh` 28/28, and live-checked `auth/campfire.db*` are mode 600 with the unit's `UMask` confirmed `0077` via `systemctl show`.

### WR-04: Successful logins are never rate-limited and issued tokens are never pruned

**Files modified:** `auth-service/src/api.rs`, `auth-service/src/db.rs`, `auth-service/src/main.rs`
**Commit:** `29abc54`
**Applied fix:** Added `Db::prune_tokens()` (`DELETE FROM tokens WHERE consumed_at IS NOT NULL OR expires_at < ?1`), called opportunistically on every successful `/login`. Added a second, much looser `login_success_limiter` (`RateLimiter`, 60/hour/peer) as a circuit breaker against runaway automation, separate from and not interacting with the existing 10/hour *failure* limiter. Verified: `cargo build --release` clean, `scripts/auth-smoke.sh` 28/28 (including the flood/`/validate`-never-throttled assertions, unaffected by the new success limiter at normal test volumes).

### WR-05: `HttpURLConnection` response is never drained or explicitly closed

**Files modified:** `mods-src/campfire-auth/src/main/java/pub/campfire/auth/server/ServerAuthHandler.java`, `mods-src/campfire-auth/build.gradle`, `docs/AUTH-OPS.md`, `docs/CLIENT-SETUP.md`
**Commit:** `81864b0`
**Applied fix:** `validateAsync` now drains the full response body via `getInputStream()`/`getErrorStream()` (whichever applies) inside a try-with-resources, then calls `conn.disconnect()` in a `finally` block — letting `HttpURLConnection` return the connection to its keep-alive pool instead of leaking/reopening a socket per join. Bumped the mod version to `0.1.1` (`build.gradle`) so the jar filename changes, and updated the jar-path references in `docs/AUTH-OPS.md` and `docs/CLIENT-SETUP.md` accordingly.

## Deployment (Rust + Java, both live)

- **auth-service (Rust):** rebuilt with `cargo build --release` after each of WR-01/WR-03/WR-04; `scripts/auth-smoke.sh` stayed 28/28 against an ephemeral instance after every change. The systemd unit (WR-03's `UMask=0077`) was reinstalled via `scripts/install-units.sh` + `daemon-reload`, `campfire-auth.service` was restarted, and the live loopback service was re-verified: `systemctl is-active campfire-auth` = `active`, `systemctl show campfire-auth -p UMask` = `UMask=0077`, `curl http://127.0.0.1:8081/status` = `200`, `auth/campfire.db*` files verified mode `600` on disk, and `scripts/auth-smoke.sh` passed 28/28 again post-restart.
- **campfire-auth mod (Java):** rebuilt with the documented `JAVA_HOME=/opt/temurin-8/jdk8u504-b01` + `./build.sh build` procedure (BUILD SUCCESSFUL in 13s), producing `campfire-auth-0.1.1.jar` (sha256 `7da5988b8af8a250ab23d087b73f1a0d92990f39ecfb71e68215d4e7a27962d2`). `rcon-cli list` showed 0/10 players online both before and immediately after the swap, so per the environment rules the jar was installed live: took a fresh world backup (`scripts/backup.sh` → `world-20260828-123317.tar.zst`), replaced `server/mods/campfire-auth-0.1.0.jar` with the new `0.1.1.jar` (sha256-verified to match the build output), and ran the single permitted `sudo systemctl restart rlcraft`.

**Game-server restart: YES.** `rlcraft.service` was restarted exactly once (`15:34:09` local) — 0 players were online at the time (confirmed via `rcon-cli list` immediately before), so no announcement was needed. The server reached `Done (9.400s)!` cleanly and `campfireauth` appears in the FML mod list with no exception attributable to it (verified via `grep` over `server/logs/latest.log` for `campfireauth`-tagged `ERROR`/`FATAL` lines — none found). `systemctl is-active rlcraft` and `campfire-auth` are both `active` as of this report.

## Skipped Issues

None — all 5 in-scope Warning findings were fixed, verified, and deployed.

---

_Fixed: 2026-08-28T12:55:52Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
