---
phase: 01-playable-server-on-the-pi
plan: 01
subsystem: infra
tags: [temurin-8, forge-1.12.2, rlcraft, curseforge, cgnat, bash]

# Dependency graph
requires: []
provides:
  - "Java 8 (Temurin) runtime resolved at an absolute path, system Java 25 untouched"
  - "Verified RLCraft Server Pack 2.9.3 zip on disk, sha256-pinned in server.env"
  - "rcon-cli and zstd installed as aarch64 binaries"
  - "Single server.env (mode 600, untracked) holding every operator fact the rest of the phase reads"
  - "CGNAT verdict recorded (stage 1 clear, stage 2 unconfirmed — deferred to plan 01-04)"
  - "D-09 whitelist override: WHITELIST_ENABLED=false recorded and reasoned"
affects: [01-02-install-and-boot, 01-03-ops-tooling, 01-04-network-and-reachability]

# Actuals (#2632)
actuals:
  tokens: 5387
  tasks: 3
  commits: 3

# Tech tracking
tech-stack:
  added: [temurin-8-jdk (apt/tarball fallback), itzg/rcon-cli, zstd, RLCraft Server Pack 2.9.3]
  patterns:
    - "Single server.env sourced by every script (D-15); set_env_var helpers always quote values written to it"
    - "Three-path acquisition with a hard integrity gate (size, file-type, unzip -t, sha256 pin) before any downloaded artifact is trusted"

key-files:
  created:
    - .gitignore
    - server.env.example
    - scripts/preflight.sh
    - scripts/fetch-pack.sh
    - scripts/cgnat-check.sh
    - server.env (generated, untracked, mode 600)
  modified:
    - server.env.example (Task 3: added SERVER_NAME, WHITELIST_ENABLED keys)

key-decisions:
  - "D-09 override: operator declined a whitelist for Phase 1 entirely (WHITELIST_ENABLED=false, WHITELIST_NICKS empty) — access is open to anyone who reaches the server until Phase 2's launcher-registration token auth ships. Accepted risk."
  - "CGNAT verdict recorded as a custom value (likely-clear-unconfirmed) outside cgnat-check.sh's detected|absent|unknown-needs-router-ip enum, because the operator confirmed static service + a public IP outside RFC 6598 but did not supply the router WAN IP needed for the script's stage 2. Final confirmation deferred to plan 01-04's outside-network reachability test."
  - "Unauthenticated CurseForge CDN path (Path 2, RESEARCH.md LOW confidence) succeeded live — no CF_API_KEY or manual pack staging was needed."

patterns-established:
  - "server.env's set_env_var helper (preflight.sh, fetch-pack.sh, cgnat-check.sh) always double-quotes the written value — PACK_ZIP's filename contains spaces, and an unquoted assignment breaks every later `. ./server.env` sourcing."

requirements-completed: [SRV-01, SRV-04]

coverage: []

# Metrics
duration: 25min
completed: 2026-08-27
status: complete
---

# Phase 01 Plan 01: Host Bootstrap, Pack Acquisition, Operator Facts Summary

**Temurin 8 + itzg/rcon-cli + zstd bootstrapped on the Pi, RLCraft Server Pack 2.9.3 (334397653 bytes) downloaded and sha256-pinned via the unauthenticated CurseForge CDN path, and every operator fact (domain, whitelist policy, CGNAT status) recorded in a single mode-600 server.env.**

## Performance

- **Duration:** 25 min
- **Started:** 2026-08-27T16:07:27+03:00
- **Completed:** 2026-08-27T16:32:29+03:00
- **Tasks:** 3
- **Files modified:** 5 (excluding `.planning/`)

## Accomplishments
- Temurin 8 JDK installed via the Adoptium apt repo; `JAVA8_BIN=/opt/temurin-8/jdk8u504-b01/bin/java` reports `openjdk version "1.8.0_504"`, while system `java -version` still reports `25.0.3` — Forge 1.12.2's classloader gets the right JVM without touching the default.
- `itzg/rcon-cli` (aarch64, checksum-verified against the GitHub release) and `zstd` (v1.5.7) installed; old `~/mcserver` Paper server confirmed absent (`OLD_PAPER: absent`) so the 6 GB heap has full headroom.
- RLCraft Server Pack 1.12.2 - Release v2.9.3.zip acquired live via the unauthenticated CurseForge CDN path (Path 2 — RESEARCH.md rated this LOW confidence; it worked): 334397653 bytes, `file` confirms Zip archive, `unzip -t` clean, sha256 `a29b3c0c99b41e1c7d404a78f48b7f9698f15b068141a042d5ca1108a8636c55` pinned trust-on-first-use in `server.env`.
- CGNAT stage 1 (public IP `91.193.195.130` vs RFC 6598 `100.64.0.0/10`) ran clean; stage 2 (router WAN IP) could not run because the operator did not supply the router WAN IP, so the plain script verdict is `unknown-needs-router-ip`. Operator separately confirmed the ISP contract is static, which combined with the RFC-6598-clear public IP is stronger than "unknown" — recorded as a hand-amended `likely-clear-unconfirmed` verdict, with plan 01-04's outside-network reachability test carrying the final word.
- Operator facts landed in `server.env`: `DOMAIN=mc.campfire.pub`, `SERVER_NAME=campfire.pub`, `WAN_IP_KIND=static`, `WHITELIST_ENABLED=false` (D-09 override — see Deviations), TCP 25565 as the only port plan 01-04 needs to forward.
- `server.env` remains mode 600, `git check-ignore -q server.env` passes, and `git ls-files --error-unmatch server.env` fails (untracked) — only `server.env.example` is tracked.

## Task Commits

Each task was committed atomically:

1. **Task 1: Host bootstrap — Temurin 8, ops tooling, single env file, git hygiene** - `9026abd` (feat)
2. **Task 2: Acquire the RLCraft Server Pack 2.9.3 and settle the CGNAT question** - `2ae4da6` (feat)
3. **Task 3: Operator facts — domain, whitelist policy, CGNAT verdict** - `dbaf2f6` (feat)

_No plan-metadata commit made per this execution's instructions: `.planning/STATE.md` and `.planning/ROADMAP.md` are intentionally left unmodified by this executor run._

## Files Created/Modified
- `.gitignore` - ignores `server.env`, `downloads/`, and regenerable `server/` runtime content; `server/config/` intentionally stays tracked
- `server.env.example` - tracked template of every key every script reads (D-15); Task 3 added `SERVER_NAME` and `WHITELIST_ENABLED`
- `scripts/preflight.sh` - idempotent host bootstrap: Temurin 8 (apt with tarball fallback), zstd/unzip/curl/jq, rcon-cli, old-Paper standdown, server.env generation with a CSPRNG RCON password
- `scripts/fetch-pack.sh` - three-path pack acquisition (CurseForge API, unauthenticated CDN, operator-staged file) with size/type/unzip integrity gating and sha256 trust-on-first-use pinning
- `scripts/cgnat-check.sh` - two-stage CGNAT detection, verdict persisted to `server.env`
- `server.env` (untracked, mode 600) - every operator/system fact this phase's scripts read: `JAVA8_BIN`, `RCON_PASSWORD`, `PACK_SHA256`/`PACK_ZIP`, `DOMAIN`, `SERVER_NAME`, `WHITELIST_ENABLED`, `WAN_IP_KIND`, `PUBLIC_IP_AT_SETUP`, `CGNAT_VERDICT`

## Decisions Made
- **D-09 override (whitelist):** Operator explicitly chose no whitelist for Phase 1 rather than "operator adds friends' nicks" as CONTEXT.md's default assumed. `WHITELIST_ENABLED=false` was added as a new key (not in the original plan's key list) so `server.properties` rendering in plan 01-02 can emit `white-list=false` / `enforce-whitelist=false` instead of D-09's `true` default. Accepted risk: the server is open to anyone who can reach it on TCP 25565 until Phase 2's launcher-registration token auth lands.
- **CGNAT verdict extension:** `cgnat-check.sh`'s three-value enum (`detected|absent|unknown-needs-router-ip`) has no slot for "stage 1 clear, stage 2 unconfirmed." Rather than silently leaving the plain-script `unknown-needs-router-ip` verdict (which undersells what the operator did confirm) or inventing a false "absent" (which the script never actually proved), the verdict was hand-amended to `likely-clear-unconfirmed` with an inline comment in `server.env` explaining the reasoning and pointing at plan 01-04 for final proof. `cgnat-check.sh` itself was NOT modified — its documented enum and exit codes stay intact for any future re-run.
- **Ports:** Only TCP 25565 needs forwarding in Phase 1 (RCON stays loopback-only per D-08); carried forward for plan 01-04.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Quoted values written into server.env (Task 2, during preflight.sh review)**
- **Found during:** Task 2 (`scripts/fetch-pack.sh` — `PACK_ZIP` resolves to a path containing spaces: `RLCraft Server Pack 1.12.2 - Release v2.9.3.zip`)
- **Issue:** `preflight.sh`'s original `set_env_var` helper wrote `KEY=$val` unquoted. An unquoted value containing spaces breaks every later `. ./server.env` sourcing (the shell splits it into multiple words/commands).
- **Fix:** Quoted the written value (`KEY="$val"` with embedded quotes escaped) in `preflight.sh`'s helper, and applied the identical pattern to `fetch-pack.sh`'s and `cgnat-check.sh`'s own `set_env_var` helpers so all three scripts are consistent.
- **Files modified:** `scripts/preflight.sh`, `scripts/fetch-pack.sh`, `scripts/cgnat-check.sh`
- **Verification:** `. ./server.env` sources cleanly with `PACK_ZIP` as a single quoted value; `stat -c %s "$PACK_ZIP"` and `file "$PACK_ZIP"` operate on the correct path.
- **Committed in:** `2ae4da6` (Task 2 commit)

**2. [Rule 2 - Missing critical, operator-directed] Added WHITELIST_ENABLED key not in the original env-key list**
- **Found during:** Task 3 (operator facts)
- **Issue:** The plan's `server.env.example` key list (Task 1 `<action>`) has `WHITELIST_NICKS` but no on/off switch — it assumed whitelisting stays enabled per D-09 and only the nick list varies. The operator's explicit "no whitelist" decision needs a way for plan 01-02's `server.properties` renderer to know to emit `white-list=false`, not just an empty nick list (which would still leave `white-list=true` blocking everyone, including the operator, per the plan's own Task 3 instructions text: "a nick that is not on this list cannot join — including yours").
- **Fix:** Added `WHITELIST_ENABLED=false` to both `server.env` and `server.env.example`, with a comment on each explaining the D-09 override and directing plan 01-02 to read it.
- **Files modified:** `server.env`, `server.env.example`
- **Verification:** `grep -c '^WHITELIST_ENABLED=' server.env` and `server.env.example` each print 1; value is `false` in the live file, `true` (D-09 default) in the tracked template.
- **Committed in:** `dbaf2f6` (Task 3 commit)

---

**Total deviations:** 2 (1 auto-fixed bug pre-dating this checkpoint resume, 1 operator-directed missing-key addition applied at this resume)
**Impact on plan:** Both were necessary for correctness — the quoting fix prevents every later script from silently mis-sourcing `server.env`; the `WHITELIST_ENABLED` key is the only way to honor the operator's explicit access-control decision without hacking around an empty nick list. No unrelated scope creep.

## Issues Encountered
None beyond the deviations above.

## User Setup Required
None — no external service configuration required. (The operator already supplied domain, WAN-IP-kind, and whitelist policy at the Task 3 checkpoint; no CurseForge API key was needed since Path 2 of `fetch-pack.sh` succeeded.)

## Next Phase Readiness
- Plan 01-02 (install and boot) can proceed: Java 8 resolved, pack verified and pinned, `server.env` carries every key it needs including the new `SERVER_NAME`/`WHITELIST_ENABLED` pair.
- Plan 01-04 (network and reachability) has two open items to close: (1) confirm the CGNAT verdict definitively via an outside-network reachability test — current status is `likely-clear-unconfirmed`, not proven `absent`; (2) forward only TCP 25565 to the Pi (RCON stays loopback-only).
- SRV-04 is NOT yet fully proven — it is `likely-clear-unconfirmed`, one step short of the plan's own "record SRV-04 as blocked if CGNAT detected" bar. It is also one step short of a positive proof; do not treat it as closed until plan 01-04 confirms reachability from outside the LAN.
- Whitelist is OFF for the whole of Phase 1 by operator decision (D-09 override) — anyone who can reach `mc.campfire.pub:25565` can join until Phase 2 ships token auth. This should stay visible to whoever runs plan 01-02/01-03.

---
*Phase: 01-playable-server-on-the-pi*
*Completed: 2026-08-27*

## Self-Check: PASSED
All key files (`.gitignore`, `server.env.example`, `scripts/preflight.sh`, `scripts/fetch-pack.sh`, `scripts/cgnat-check.sh`, this SUMMARY) exist on disk; all three task commits (`9026abd`, `2ae4da6`, `dbaf2f6`) verified present in `git log --oneline --all`.
