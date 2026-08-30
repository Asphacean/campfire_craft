---
phase: 05-release-to-friends
plan: 03
subsystem: docs
tags: [release-verification, friend-onboarding, gatekeeper, smartscreen, qa-matrix]

# Dependency graph
requires:
  - phase: 05-release-to-friends (05-02)
    provides: "GitHub Release v0.1.0 with 6 real signed artifacts, the live update feed at mc.campfire.pub:8444/launcher/latest.json"
provides:
  - "Independent, anonymous, outside-the-API proof that all 6 published release assets download through the friend-facing releases/latest/download/ URL, byte-identical (sha256) to what the Pi published to the update feed, with correct file shapes"
  - "Evidence that both macOS bundles (aarch64 and x64) carry a code-signature directory (D-08's ad-hoc signing took effect) and report the correct bundle identifier/version via Info.plist"
  - "docs/FRIENDS.md — the one page a friend needs: which file for which machine, the SmartScreen and Gatekeeper detours in plain language, first-launch expectations, and where the log lives"
  - "README.md — the repository's real front page, replacing GitHub's auto-init stub"
  - "docs/LAUNCHER-BUILD.md rewritten: 'Publishing a build' -> 'Cutting a release' (the pipeline is now the path), plus a 17-item Phase 5 release QA matrix covering REL-01/REL-02/REL-03 and the four Phase 1-4 verifications this release unblocks"
affects: []

# Actuals (#2632)
actuals:
  tokens: 3351
  tasks: 3
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Linux's file(1) (5.46) has no magic rule for the koly-trailer UDIF format Apple .dmg files use — it reports 'zlib compressed data' instead of 'Apple disk image'. Verified the real signature by reading the last 512 bytes and confirming the koly magic directly rather than trusting file(1)'s label."
    - "curl -sI (no -L) on a GitHub releases/latest URL reports 302 (it's a redirect, not the final page) — the acceptance-criteria command as literally written would read that as non-200; verified with -L that it resolves 200 end-to-end and treated the redirect as expected GitHub behavior, not a broken link."

key-files:
  created:
    - docs/FRIENDS.md
  modified:
    - README.md
    - docs/LAUNCHER-BUILD.md

key-decisions:
  - "Task 1 produced no repository changes — it is a verification-only task (download, hash-compare, unpack, inspect) and its findings feed Tasks 2-3 and this summary's evidence table, not a file edit. No commit was made for Task 1 alone; its file (docs/LAUNCHER-BUILD.md) is the one Task 3 actually edits."
  - "docs/FRIENDS.md names only the three assets a friend actually needs (the Windows .exe and both macOS .dmg files), not the .msi or the two .app.tar.gz updater archives — those exist in the release for the updater feed and for people who prefer MSI, but naming them on the friend page would add confusion without adding a path anyone needs to take."
  - "The Phase 5 QA matrix continues the Phase 4 matrix's numbering (starting at 18) and voice, per the plan's explicit instruction to keep both matrices readable as one continuous operator checklist."
  - "'Cutting a release' watch-the-run link points at the repository's own /actions page rather than a specific run URL, since a specific run only exists after the command that triggers it — the operator finds the current run from there every time."

patterns-established:
  - "The verification method for a Linux host asserting facts about a macOS bundle it cannot run: extract from the real tarball, read Info.plist with plistlib (never assume it's textual — Tauri may write it binary), check for _CodeSignature/CodeResources's mere presence (not validity), and check the Mach-O executable's architecture with file(1) — each explicitly labeled as a file-shape check, not proof the app runs on real hardware."

requirements-completed: [REL-02, REL-03]

coverage:
  - id: D1
    description: "Every release asset (6 total) downloads anonymously through the exact URL friends will use, matches the sha256 the Pi published to the update feed for the 3 platforms it tracks, and has the file shape its name claims"
    requirement: REL-02
    verification:
      - kind: e2e
        ref: "curl -fsSL through https://github.com/Asphacean/campfire_craft/releases/latest/download/<name> for all 6 assets listed by the API; ls -1 | wc -l == 6 == API asset count; all 6 files >1MB (7.6-10MB range, no stubs)"
        status: pass
      - kind: other
        ref: "sha256sum of the 3 feed-tracked artifacts (aarch64.app.tar.gz, x64-setup.exe, x64.app.tar.gz) downloaded via the friend URL, compared byte-for-byte against the same files in launcher-dist/ on this Pi (what 05-02 published) — all 3 pairs identical"
        status: pass
      - kind: other
        ref: "file(1) on x64-setup.exe reports PE32 executable (NSIS); both .app.tar.gz archives list cleanly under tar tzf; both .dmg files carry the koly UDIF trailer signature (verified by reading the last 512 bytes directly, since Linux's file(1) 5.46 has no magic rule for this trailer-based format and instead reports the leading zlib stream)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Both macOS bundles (aarch64, x64) carry the code-signature directory ad-hoc signing produces, and report the tauri.conf.json-configured bundle identifier and version"
    requirement: REL-02
    verification:
      - kind: other
        ref: "tar tzf on both .app.tar.gz archives lists Contents/_CodeSignature/CodeResources; python3 plistlib.load on each extracted Contents/Info.plist reports CFBundleIdentifier=pub.campfire.launcher and CFBundleShortVersionString=0.1.0 for both, matching tauri.conf.json's identifier"
        status: pass
      - kind: other
        ref: "file(1) on Contents/MacOS/campfire-launcher: arm64 Mach-O executable in the aarch64 archive, x86_64 Mach-O executable in the x64 archive"
        status: pass
    human_judgment: false
  - id: D3
    description: "docs/FRIENDS.md is the one page a friend needs: which file for which machine, both OS warning detours in plain language, first-launch expectations, log locations — naming only real filenames, linking only the canonical release page, never the :8444 endpoint"
    requirement: REL-02
    verification:
      - kind: other
        ref: "grep -c 8444 across docs/FRIENDS.md + README.md == 0; all 3 installer asset names (aarch64.dmg, x64.dmg, x64-setup.exe) present verbatim; no invented filename (regex scan of all Campfire-Launcher_* strings, all match the API's real asset list); 'run anyway' >=1, 'xattr -cr' ==1, 'right-click' >=1, 'unsigned|not signed' >=1; only download link is releases/latest (no other releases/ form found)"
        status: pass
      - kind: other
        ref: "curl -sI -L on every https:// URL in docs/FRIENDS.md and README.md (just the one releases/latest link, shared by both) reports 200; all docs/ relative paths referenced from README.md exist on disk"
        status: pass
    human_judgment: false
  - id: D4
    description: "docs/LAUNCHER-BUILD.md's release procedure is rewritten around scripts/release.sh and the pipeline, and a Phase 5 QA matrix of real-hardware checks exists, naming the exact released artifacts and recording the Intel build as unverified rather than omitting it"
    requirement: REL-03
    verification:
      - kind: other
        ref: "'Phase 5 release QA matrix' header present once; 17 numbered items (18-34, >= the required 10); scripts/release.sh named; the repo's own actions URL named; the matrix's own releases/latest reference present; Windows section names _x64-setup.exe verbatim, Apple Silicon section names _aarch64.dmg verbatim; Intel item states plainly it is CI-built and never run, no hardware available; 5 occurrences of 'Phase [1-4]' (4 required) covering all four deferred verifications; both original '## Windows x64'/'## macOS Apple Silicon' hand-build headers survive unchanged"
        status: pass
    human_judgment: false
  - id: D5
    description: "The three human-check items this plan's Task 3 <verify> carries (REL-01, REL-02, REL-03) are recorded for the end-of-phase human verification batch, not silently skipped"
    requirement: REL-03
    verification: []
    human_judgment: true
    rationale: "These are explicit human-check items requiring a Windows x64 machine and the operator's own Apple Silicon Mac — no automation on this Pi can answer whether SmartScreen/Gatekeeper wording matches, whether the game renders, or what framerate results. Recorded below under 'Pending Human Verification' exactly as the plan's checkpoint_protocol requires."

duration: ~20min
completed: 2026-08-30
status: complete
---

# Phase 5 Plan 3: Release-to-Friends Documentation and Verification Summary

**Anonymous, byte-for-byte proof that every published release asset is what friends will actually receive, plus the friend-facing install page (docs/FRIENDS.md), a real README, and a 17-item real-hardware QA matrix that finally unblocks four phases' deferred verifications.**

## Performance

- **Duration:** ~20 min
- **Started:** 2026-08-30T19:26:00Z (approx, from init context)
- **Completed:** 2026-08-30T19:46:04Z
- **Tasks:** 3 (Task 1 verification-only, no commit; Tasks 2-3 committed)
- **Files modified:** 3 (1 created, 2 modified)

## Accomplishments

- Downloaded all 6 GitHub Release v0.1.0 assets anonymously through the exact `releases/latest/download/<name>` URL a friend would use — every file matches, sha256-for-sha256, the corresponding artifact 05-02 published to the live update feed (`launcher-dist/`), and every file type matches its name (PE32 installer, gzip-compressed updater archives, genuine koly-signed Apple disk images that Linux's `file(1)` merely can't label as such)
- Extracted both macOS updater archives and confirmed, from real bytes rather than assumption: each carries a `_CodeSignature/CodeResources` directory (ad-hoc signing per D-08 took effect on the CI runner), each `Info.plist` reports `pub.campfire.launcher`/`0.1.0` via proper `plistlib` parsing (never assumed textual), and each `Contents/MacOS/campfire-launcher` is a native Mach-O binary for its architecture (arm64 / x86_64)
- Wrote `docs/FRIENDS.md`: the single page a friend needs — which file for which machine (with an Apple-menu check for telling Apple Silicon from Intel), the Windows SmartScreen detour, the macOS Gatekeeper detour (both documented routes), first-launch expectations (Java + modpack download, Rosetta prompt), and where the log lives on each platform — naming only real filenames and linking only to the canonical `releases/latest` page, never the `:8444` private-CA endpoint
- Replaced the repository's GitHub-auto-init README stub with a real front page pointing friends at `docs/FRIENDS.md` and operators at the existing ops docs, plus the no-credentials-in-history statement
- Rewrote `docs/LAUNCHER-BUILD.md`'s "Publishing a build" into "Cutting a release" (the pipeline — `scripts/release.sh <version>` — is now the path, hand-building kept as the fallback/local-iteration route) and added a 17-item "Phase 5 release QA matrix" (continuing the Phase 4 matrix's numbering from 18) covering the released Windows and Apple Silicon artifacts, the deferred self-update check, the Intel build's honestly-recorded unverified status, and the four Phase 1-4 verifications this release finally unblocks

## Task Commits

1. **Task 1: Download it the way a friend would, and look inside what arrives** — no commit (verification-only; findings feed Tasks 2-3 and the coverage table above)
2. **Task 2: The page a friend is actually sent** — `2bca873` (docs)
3. **Task 3: The checklist for the hardware this Pi does not have** — `a5443ea` (docs)

**Plan metadata:** this SUMMARY's own commit (see below)

_No TDD tasks in this plan._

## Files Created/Modified

- `docs/FRIENDS.md` (new, 99 lines) — the friend-facing install page: which file, both OS warning detours, first launch, troubleshooting
- `README.md` (26 lines, was 1) — real repository front page
- `docs/LAUNCHER-BUILD.md` (393 lines, was ~278) — "Cutting a release" section + Phase 5 QA matrix (items 18-34)

## Decisions Made

See `key-decisions` in frontmatter. Notably: Task 1 made no repository edits (it is pure verification); `docs/FRIENDS.md` deliberately omits the `.msi` and `.app.tar.gz` asset names since friends never need them; the Phase 5 QA matrix continues the Phase 4 matrix's numbering and voice per the plan's instruction.

## Deviations from Plan

None - plan executed exactly as written. Every acceptance criterion in all three tasks was checked with its literal command (or the `-L`-following equivalent, noted below) before moving on, with no fix cycles needed.

## Issues Encountered

- `curl -sI` (no `-L`) on `https://github.com/Asphacean/campfire_craft/releases/latest` reports `302` (it's a redirect to the tagged release page), not `200` — the acceptance criteria's literal command would read this as a failure. Re-ran with `-L` and confirmed `200` end-to-end; this is expected GitHub behavior for the `releases/latest` alias, not a broken link. Documented as a pattern in frontmatter so future plans checking this URL know to follow redirects.
- Linux's `file` 5.46 has no magic rule for Apple's koly-trailer UDIF `.dmg` format and reports "zlib compressed data" instead of "Apple disk image" for both downloaded `.dmg` files. Verified the real signature by reading the last 512 bytes of each file directly and confirming the `koly` magic bytes present — both files are genuine, correctly-formed Apple disk images; this is a tooling limitation on this Pi, not a defect in the release.

## User Setup Required

None - no external service configuration required.

## Pending Human Verification

Three `human-check` items from this plan's Task 3 `<verify>` block are pending, requiring hardware this Pi does not have:

1. **REL-03 (Apple Silicon Mac).** Download `Campfire-Launcher_0.1.0_aarch64.dmg` from the release page, follow the Gatekeeper steps in `docs/FRIENDS.md`, open the app, log in, press Play, and report whether the game renders and roughly what framerate results standing still in a forest. This is the whole of REL-03.
2. **REL-02 (both platforms).** Report the exact wording of the SmartScreen (Windows) and Gatekeeper (macOS) warnings, and whether `docs/FRIENDS.md`'s steps got past them without further help.
3. **REL-01 (Windows x64).** Install from `Campfire-Launcher_0.1.0_x64-setup.exe` on a machine with no Java, register a fresh nick, pick a RAM value, press Play once, report total wall time/disk used, confirm you land in the world, then close/reopen and confirm no password re-prompt.

These same checks, per `docs/LAUNCHER-BUILD.md`'s Phase 5 QA matrix (items 18-34), also close the four deferred Phase 1-4 UATs recorded in `.planning/STATE.md`'s "Deferred Verification" table (Phases 1-4, all `verification_deferred_human`, all noted as waiting for exactly this release).

## Next Phase Readiness

- Every artifact a friend is told to download has been independently, anonymously verified byte-for-byte against what the Pi published — no further engineering work is needed before the operator runs the pending human verification above
- `docs/FRIENDS.md` and `README.md` are ready to be shared as-is; every link and filename in both was checked against the live release
- `rlcraft.service` was active before, during, and after this plan (verified: `systemctl is-active rlcraft.service` → `active`, `ActiveEnterTimestamp` predates this plan's start) — never touched, never restarted
- This is the last plan in Phase 5; once the pending human verification above is run, the phase (and the four blocked Phase 1-4 UATs) can close

---
*Phase: 05-release-to-friends*
*Completed: 2026-08-30*

## Self-Check: PASSED
