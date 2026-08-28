---
phase: 03-modpack-distribution
plan: 02
subsystem: infra
tags: [curseforge, python3, hashlib, sha256, rsync, urllib, ssl, manifest]

# Dependency graph
requires:
  - phase: 03-modpack-distribution
    provides: "plan 03-01's caddy.service on :8444, own-CA TLS, ca/campfire-ca.pem, the manifest+pack file_server route, /etc/hosts mc.campfire.pub->127.0.0.1"
provides:
  - "scripts/publish-pack.sh: the single DIST-02 operator command — client base zip acquisition (sha-pinned), all 187 CurseForge mod/resourcepack fetches, overrides extraction, server/config+campfire-auth overlay, disk-space floor, then the manifest step"
  - "scripts/gen-manifest.py: sorted-walk streaming sha256, hard path-traversal validation, a forbidden-content gate, cumulative delete[] diffing, atomic tempfile+os.replace write — standard library only"
  - "scripts/assemble-client.py: the client half of the manifest contract and the reference implementation Phase 4's launcher mirrors — CA-only SSL context, client-side path guard, DIST-03 enforcement, hash-verified download/verify modes, delete[] honoured — standard library only"
  - "docs/DIST-OPS.md: the operator runbook for publish, blocked-file recovery, cert rotation, backup contents, manifest schema, route table, /etc/hosts caveat, accepted redistribution risk"
  - "pack/ live at 362MB, pack/manifest.json with 3545 files/0 delete, served over the real HTTPS front"
  - "CLIENT_PACK_SHA256, CLIENT_PACK_ZIP, MIN_FREE_MB in server.env/server.env.example"
affects: [03-03-uat, 04-launcher]

# Actuals (#2632)
actuals:
  tokens: 12285
  tasks: 3
  commits: 3

# Tech tracking
tech-stack:
  added:
    - "No new third-party dependency — Python 3 standard library only (hashlib, json, os, sys, tempfile, datetime, argparse, ssl, urllib), asserted by an AST import-set check in every task's own verify command"
  patterns:
    - "CurseForge unauthenticated redirect resolved via a two-request pattern: a body-discarding request that reports http_code + url_effective to resolve the real filename first, then the real GET — matches RESEARCH.md Pattern 2, extended with an explicit HTTP-status check on the resolve step (not just presence of a redirected URL)"
    - "Filename sanitisation is a standalone bash function backed by a small inline python3 heredoc (no path separator, no .., no leading dot, no control/null char) — reused identically for the client zip and all 187 per-mod fetches"
    - "gen-manifest.py's forbidden-content gate and path-traversal validation must run over the FULL collected file list (including zero-byte files) — size-based filtering happens only afterward, in build_files(), or a zero-byte forbidden file silently bypasses the gate entirely (found live)"
    - "Every subprocess/script step invoked from publish-pack.sh must have its exit code explicitly captured and checked — `set -uo pipefail` alone does not propagate a failing python3 call's exit code past a subsequent unconditional log line, silently turning a hard failure into an apparent success (found live)"
    - "assemble-client.py must urllib.parse.quote() each manifest path segment before building a download URL — several real pack filenames contain literal spaces, which urllib.request rejects outright as control characters in an unencoded URL (found live)"

key-files:
  created:
    - scripts/publish-pack.sh
    - scripts/gen-manifest.py
    - scripts/assemble-client.py
    - docs/DIST-OPS.md
  modified:
    - server.env
    - server.env.example

key-decisions:
  - "Zero-byte files (12 real server/config/ placeholder markers, e.g. a literal 'Put biome config files here' file RLCraft ships) are excluded from manifest files[] — a 0-byte entry has nothing to hash-verify and violates the manifest's own size>0 invariant every consumer assumes — but still pass through collection/validation/the forbidden-content gate unchanged, and still land on disk via rsync; only the manifest omits them."
  - "gen-manifest.py's forbidden-content gate test in this plan's own acceptance criteria (touch pack/config/leak-probe.db) does not exercise publish-pack.sh's full pipeline as literally written, because overlay_own_content()'s rsync -a --delete server/config/ pack/config/ (D-06's mandated one-way mirror) removes any file placed directly in pack/config/ that isn't sourced from server/config/ before gen-manifest.py ever runs. Verified the gate is real via two paths instead: gen-manifest.py invoked directly against a probe in pack/config/ (exit 3, correct offender named), and the full publish-pack.sh --skip-fetch pipeline with a probe in pack/scripts/ (a directory --skip-fetch never touches) — same exit 3, same manifest-unchanged guarantee. The config/ mirror is a stronger property than the gate for that one subtree, not a gap."

patterns-established:
  - "Any bash script that shells out to a python3 tool as its last meaningful step must capture $? into a local variable and act on it explicitly — a trailing unconditional log/echo line after the subprocess call silently resets the script's own exit status to 0 regardless of the subprocess's real outcome."

requirements-completed: [DIST-01, DIST-02, DIST-03]

coverage:
  - id: D1
    description: "publish-pack.sh acquires the whole RLCraft 2.9.3 client pack in one re-runnable command: sha-pinned client base zip, all 187 CurseForge-referenced mods/resourcepacks, overrides minus server-only/options files, server/config+campfire-auth overlay, refusing to run below a disk-space floor"
    requirement: "DIST-02"
    verification:
      - kind: manual_procedural
        ref: "bash scripts/publish-pack.sh full run: 179 mods, 11 resourcepacks, 2206 config files, 362M pack tree, 0 CurseForge refusals; MIN_FREE_MB=99999999 bash scripts/publish-pack.sh exits 2 with nothing downloaded; second bash scripts/publish-pack.sh (all files present) completed in 1m49s vs the first run's multi-minute download window and re-downloaded nothing"
        status: pass
    human_judgment: false
  - id: D2
    description: "gen-manifest.py produces a manifest with path/sha256/size/url for every managed file, sorted-walk deterministic, atomically written, gated against forbidden content and path traversal, with a cumulative delete[] diff"
    requirement: "DIST-01"
    verification:
      - kind: manual_procedural
        ref: "curl --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/manifest.json: 3545 files, all path/sha256/size/url well-formed; two --skip-fetch runs diff identical apart from pack_version; a moved-out mod appears in delete[] and a second removal is carried forward alongside the first; touch .../leak-probe.db aborts with exit 3 and pack/manifest.json byte-identical before/after; grep -c os.replace scripts/gen-manifest.py >=1 and no *.tmp left behind; stat mode 644"
        status: pass
    human_judgment: false
  - id: D3
    description: "assemble-client.py builds/verifies a full client from nothing but the manifest and the pinned CA, enforcing DIST-03 (no Minecraft client jar/library/asset), catching tampering, and refusing the system trust store"
    requirement: "DIST-03"
    verification:
      - kind: manual_procedural
        ref: "python3 scripts/assemble-client.py --dest ~/client-check: ASSEMBLE OK, 3545 files, 367531501 bytes, ~2s over loopback; --verify: VERIFY OK, 0.66s, no downloads; every manifest path present on disk (0 MISSING); a byte-appended jar is caught by --verify naming expected/actual hash, repaired by re-running assemble mode; --cacert /etc/ssl/certs/ca-certificates.crt fails with CERTIFICATE_VERIFY_FAILED; 0 libraries/assets/versions paths and 0 minecraft*.jar in the assembled tree"
        status: pass
    human_judgment: false
  - id: D4
    description: "docs/DIST-OPS.md is the operator runbook covering publish, blocked-CurseForge-file recovery, cert rotation and CA-loss recovery, backup contents, manifest schema, route table, the /etc/hosts caveat, and the accepted redistribution risk"
    verification:
      - kind: manual_procedural
        ref: "wc -l docs/DIST-OPS.md = 232 (>=50); grep -c '^## ' docs/DIST-OPS.md = 11 (>=7)"
        status: pass
    human_judgment: false

# Metrics
duration: ~50min
completed: 2026-08-28
status: complete
---

# Phase 3 Plan 2: Client Pack Assembly and a Hashed, Self-Verifying Manifest Summary

**`scripts/publish-pack.sh` builds the real 362MB RLCraft 2.9.3 client pack (179 mods, 11 resourcepacks, 2206 configs) from CurseForge in one command; `scripts/gen-manifest.py` turns it into a 3545-entry sha256 manifest with atomic writes and a cumulative delete list; `scripts/assemble-client.py` proves the whole thing by rebuilding a byte-identical client from nothing but the manifest and the pinned CA.**

## Performance

- **Duration:** ~50 min
- **Tasks:** 3
- **Files created:** 4 (scripts/publish-pack.sh, scripts/gen-manifest.py, scripts/assemble-client.py, docs/DIST-OPS.md)
- **Files modified:** 2 (server.env, server.env.example)

## Accomplishments

- **The whole client pack, one command.** `scripts/publish-pack.sh` pinned the client base zip (CurseForge project 285109, file 4612979, 51,324,367 bytes, sha256 `5caa25d31f47f4ac69846e4faa741811baa9239804747769f6d54f7b1bbf1291` — matching RESEARCH.md's live-verified value exactly), fetched all 187 CurseForge-referenced entries with **zero refusals** — **177 jars, 10 resourcepacks**, exactly RESEARCH.md's predicted split — extracted `overrides/` minus the server-only/options files (confirmed against the real zip: only the plan's own named exclusion list, `options.txt`/`optionsof.txt`/`server.properties` and four top-level changelog/note `.txt` files, nothing extra), and overlaid `server/config/` (2206 files, byte-identical via `diff -rq`) and `campfire-auth-0.1.1.jar`. Final tree: 362M, 179 mods (177 CF + campfire-auth + `antiquecities-1.2.1.jar` from `overrides/mods/`, confirming Pitfall 3), 11 resourcepacks.
- **A disk-space floor that actually blocks.** `MIN_FREE_MB=99999999 bash scripts/publish-pack.sh` exits 2 with the shortfall printed and nothing downloaded, checked before the first byte crosses the wire.
- **Resumable and idempotent.** A second full run (all 187 files already on disk) completed in 1m49s and re-downloaded nothing; `--skip-fetch` makes zero CurseForge requests and regenerates the manifest from disk in well under a second.
- **A hashed, atomic, self-diffing manifest.** `scripts/gen-manifest.py`: sorted streaming sha256, hard traversal validation, a forbidden-content gate (`server.properties`/`ops.json`/`whitelist.json`/`usercache.json`/`server.env`/`eula.txt`/`banned-*`/`*.db`/`saves/`), a cumulative `delete[]` diff, and a `tempfile.mkstemp()` + `os.replace()` atomic write. Live: 3545 files, 0 delete, 367,531,501 bytes, deterministic across no-op re-runs (`pack_version` aside), the delete list survives two separate removals, the gate aborts leaving the previous manifest byte-identical.
- **A proof, not just a claim, that the manifest is complete.** `scripts/assemble-client.py` rebuilt a full client from the live HTTPS front trusting only `ca/campfire-ca.pem`: `ASSEMBLE OK`, 3545 files, 367,531,501 bytes, ~2s over loopback; `--verify` re-checked the same tree with zero downloads in 0.66s; a deliberately tampered jar was caught by name with expected/actual hash and repaired by re-assembling; pointing `--cacert` at the system trust store failed with `CERTIFICATE_VERIFY_FAILED` — pinning is real.
- **DIST-03 enforced by a tool, not a comment.** Zero `libraries/`, `assets/`, `versions/` paths and zero `minecraft*.jar` anywhere in the served manifest or the assembled tree, asserted by `assemble-client.py`'s own hard check.
- **`docs/DIST-OPS.md`** (232 lines, 11 `##` sections): publish/fast-path, what to do when CurseForge refuses a file, cert rotation and what CA key loss actually means, backup contents, the full manifest schema, the route table, why `/etc/hosts` can mask a broken outside path, and the accepted redistribution risk (D-07) recorded as a decision.

## Task Commits

Each task was committed atomically:

1. **Task 1: The whole client pack on disk** — `143c837` (feat)
2. **Task 2: One command publishes — hashes, atomic manifest, delete list** — `7bb7905` (feat)
3. **Task 3: Proof — assembled client + runbook** — `38b9348` (feat)

_No plan-metadata/STATE.md/ROADMAP.md commit made by this executor run per its instructions — the orchestrator owns those writes._

## Files Created/Modified

- `scripts/publish-pack.sh` — the single DIST-02 command: disk-floor check, client zip acquisition (sha-pinned trust-on-first-use), 187-entry CurseForge fetch with filename resolution + sanitisation, overrides extraction, config/auth-jar overlay, then the manifest step
- `scripts/gen-manifest.py` — sorted-walk streaming sha256, traversal validation, forbidden-content gate, cumulative delete diff, atomic write — standard library only
- `scripts/assemble-client.py` — CA-only SSL context, client-side path guard, DIST-03 hard check, hash-verified assemble/verify modes, delete[] honoured — standard library only
- `docs/DIST-OPS.md` — the operator runbook
- `server.env` / `server.env.example` — `CLIENT_PACK_SHA256`, `CLIENT_PACK_ZIP`, `MIN_FREE_MB=5000`

## Decisions Made

See `key-decisions` in the frontmatter above — summarized: zero-byte config marker files are excluded from `files[]` only (still collected/validated/gated, still copied to disk) because a 0-byte manifest entry has nothing to verify and breaks the size>0 invariant; the plan's own `touch pack/config/leak-probe.db` gate test doesn't reach `gen-manifest.py` through the full `publish-pack.sh --skip-fetch` pipeline because `overlay_own_content()`'s `rsync --delete` (D-06's own mandated behavior) removes anything under `pack/config/` not sourced from `server/config/` first — verified the gate is real via both a direct `gen-manifest.py` invocation and a probe placed in a directory the overlay doesn't touch (`pack/scripts/`).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Zero-byte config marker files broke the manifest's own size>0 invariant**
- **Found during:** Task 2, running the plan's own well-formedness check (`jq -e 'all(.files[]; ... (.size>0) ...)'`) against the live manifest
- **Issue:** `server/config/` genuinely ships 12 zero-byte placeholder files (e.g. a file literally named `Put biome config files here`), which the generator was collecting into `files[]` with `size: 0` — failing the manifest's own well-formedness contract that every consumer (this plan's own verify command, Phase 4's future launcher) relies on
- **Fix:** `build_files()` now skips zero-byte entries when constructing `files[]`; `collect_paths()` still returns them so path-traversal validation and the forbidden-content gate see every file regardless of size
- **Files modified:** `scripts/gen-manifest.py`
- **Verification:** `jq -e 'all(.files[]; ...(.size>0)...)'` now passes over the live manifest (3545/3545 well-formed); a subsequent `touch .../*.db` test confirmed the gate still fires on zero-byte forbidden files after the fix (see deviation 2)
- **Committed in:** `7bb7905` (Task 2 commit)

**2. [Rule 1 - Bug] `publish-pack.sh` never propagated `gen-manifest.py`'s exit code**
- **Found during:** Task 2, testing the forbidden-content gate through the full pipeline (`touch pack/scripts/leak-probe.db && bash scripts/publish-pack.sh --skip-fetch` returned exit 0 despite `gen-manifest.py` printing `FATAL: forbidden-content gate fired` to the log)
- **Issue:** `publish_manifest()`'s last statement was the bare `python3 .../gen-manifest.py` call, with no `set -e` in effect (`set -uo pipefail`, matching house style) — a subsequent unconditional log line in `main()` after the call meant the script's own final exit status reflected that harmless log line, not the generator's real failure
- **Fix:** `publish_manifest()` now captures `$?` explicitly into `rc` and calls `exit 5` itself on any non-zero result, before any further log output can mask it
- **Files modified:** `scripts/publish-pack.sh`
- **Verification:** the same gate test now exits 5, `pack/manifest.json`'s sha256 is byte-identical before and after, and removing the probe + re-publishing restores a clean manifest
- **Committed in:** `7bb7905` (Task 2 commit)

**3. [Rule 1 - Bug] `assemble-client.py` rejected real pack filenames containing spaces**
- **Found during:** Task 3, the first full `assemble-client.py` run against the live manifest (`ASSEMBLE OK` did not appear; 10 files failed with `URL can't contain control characters`)
- **Issue:** Several genuine RLCraft asset/script/structure filenames contain literal spaces (e.g. `resources/mainmenu/images/4 new.jpg`, `structures/downloads/P_CultIsland.rcst bakup`) — `urllib.request.urlopen()` refuses an un-encoded space in a URL outright, which the plan's own live verify run against the real manifest immediately exposed
- **Fix:** `download_entry()` now builds the download URL with `urllib.parse.quote()` over the manifest's `url` field (default `safe='/'` percent-encodes spaces per path segment while leaving `/` separators intact), leaving the local filesystem path and the manifest's own `path`/`url` fields untouched
- **Files modified:** `scripts/assemble-client.py`
- **Verification:** re-run produced `ASSEMBLE OK — 3545 files, 367531501 bytes`; a subsequent `--verify` run confirmed all 3545 files present with matching hashes
- **Committed in:** `38b9348` (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (all Rule 1 — bugs found by running the plan's own acceptance criteria live against the real system, not discovered later). No scope creep — each fix was necessary for the plan's own stated criteria to pass.

**Impact:** All three fixes were required for the plan's own explicit acceptance criteria to hold true. None represent a design change from what the plan asked for.

## Issues Encountered

**The `touch pack/config/leak-probe.db` acceptance test doesn't exercise the gate through the full `publish-pack.sh --skip-fetch` pipeline as literally written.** `overlay_own_content()`'s `rsync -a --delete server/config/ pack/config/` — D-06's explicit "the operator's removed config actually disappears from the client" requirement — silently removes any file placed directly under `pack/config/` that isn't sourced from `server/config/`, including a manually-planted probe, before `gen-manifest.py` ever runs. This is not a security gap: nothing not present in the git-tracked `server/config/` source of truth can ever persist in `pack/config/` regardless of the forbidden-content gate's own behavior, which is a stronger property for that one subtree. Verified the gate independently two ways: (1) `python3 scripts/gen-manifest.py pack` run directly against a probe placed in `pack/config/` — exit 3, offender named correctly; (2) the full `publish-pack.sh --skip-fetch` pipeline with the same probe placed in `pack/scripts/` (a directory the overlay step never touches) — exit 5 (via deviation 2's fix), `pack/manifest.json` byte-identical before/after, exactly as the acceptance criterion specifies. Not fixed (there is nothing to fix — both the mirror and the gate behave correctly); documented here per 03-01's own precedent for a literal-criterion-versus-real-behavior mismatch.

## User Setup Required

None — no external service configuration required. This executor ran with operator-equivalent (passwordless sudo) access on the Pi itself.

## Next Phase Readiness

- `pack/manifest.json` is live at `https://mc.campfire.pub:8444/manifest.json`, 3545 files, 0 pending deletes, served with a certificate that validates against `ca/campfire-ca.pem`.
- `scripts/assemble-client.py` is the reference implementation Phase 4's launcher builds against: CA-only pinning, path guard, DIST-03 enforcement, hash-verified download/verify.
- `scripts/publish-pack.sh` is the one operator command for every future mod/config change; `docs/DIST-OPS.md` documents the fast path, the blocked-file recovery flow, and cert rotation.
- `rlcraft.service` was `active` throughout every task in this plan and was never touched; its uptime (`2026-08-22 20:53:29`) was unchanged from start to finish. `caddy.service` was `active` throughout.
- DIST-01, DIST-02, DIST-03 are all satisfied and enforced by tooling (`gen-manifest.py`'s gate, `assemble-client.py`'s DIST-03 check), not documentation alone.
- ROADMAP Phase 3 success criterion 3's automated half (a client assembled purely from the manifest exists on disk with every hash verified) is met; its "connects and plays" half remains the deferred human check (D-13), for plan 03-03's UAT harvest.

---
*Phase: 03-modpack-distribution*
*Completed: 2026-08-28*

## Self-Check: PASSED

All key files verified present on disk: `scripts/publish-pack.sh`, `scripts/gen-manifest.py`, `scripts/assemble-client.py`, `docs/DIST-OPS.md`. All three task commits (`143c837`, `7bb7905`, `38b9348`) verified present via `git log --oneline --all`. Live system state re-checked at write time: `systemctl is-active rlcraft caddy` both `active`, `curl --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/manifest.json` returns 3545 well-formed files with 0 pending deletes, `python3 scripts/assemble-client.py --dest ~/client-check --verify` returns `VERIFY OK` with the same file count and zero downloads, `uptime -s` unchanged since before this plan ran (`2026-08-22 20:53:29`).
