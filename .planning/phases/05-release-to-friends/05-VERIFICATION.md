---
phase: 05-release-to-friends
verified: 2026-08-30T23:10:00Z
status: human_needed
score: 7/10 must-haves verified
behavior_unverified: 3
overrides_applied: 0
human_verification:
  - test: "On a clean Windows x64 machine with no Java installed: download `Campfire-Launcher_0.1.0_x64-setup.exe` from https://github.com/Asphacean/campfire_craft/releases/latest, run it, get past the SmartScreen warning using the steps in docs/FRIENDS.md, register a fresh nick, pick a RAM value, press Play once."
    expected: "SmartScreen shows the wording docs/FRIENDS.md describes ('Windows protected your PC' -> More info -> Run anyway); installer runs with no administrator prompt; first Play downloads Java + modpack and lands in the RLCraft world on campfire.pub; closing and reopening the launcher does not re-prompt for the password."
    why_human: "Requires an actual Windows machine and a live play session on the real server -- REL-01's 'installs, plays, nothing else required' claim and REL-02's exact warning wording cannot be observed from this Pi (headless, no Windows, no display)."
  - test: "On the operator's Apple Silicon Mac: download `Campfire-Launcher_0.1.0_aarch64.dmg` from the release page, follow the Gatekeeper bypass documented in docs/FRIENDS.md (right-click -> Open, or the `xattr -cr \"/Applications/Campfire-Launcher.app\"` fallback), open the app, log in or register, press Play, and report whether the game renders and roughly what framerate results standing still in a forest."
    expected: "One of the two documented Gatekeeper routes gets the app open with no undocumented step; the app launches, connects, and renders the world at a workable framerate on real Apple Silicon hardware."
    why_human: "REL-03 explicitly requires verification on real hardware -- LWJGL2 native loading and rendering behavior on Apple Silicon cannot be inferred from file-shape checks on a Linux host, no matter how much of the bundle's structure is inspected statically."
  - test: "Cut the next tag (e.g. v0.1.1) and watch the `publish` job's 'Checksum verification passed for all downloaded assets' log line actually execute in a live GitHub Actions run, using the artifact upload/download hand-off between the `build` and `publish` jobs (05-REVIEW.md CR-01, fixed in commit f827e4f)."
    expected: "The build job's per-platform `checksums-*.txt` artifacts upload successfully; the publish job downloads them and every release asset's sha256 matches before `publish-launcher.sh` is invoked; a deliberately tampered asset would cause the job to fail with 'no known-good checksum for <file> -- refusing to sign'."
    why_human: "The fix was committed at 2026-08-30T22:00:00Z, after the only real release run (v0.1.0, built 18:50:04Z). No tag has been cut since, so the cross-job artifact hand-off this fix depends on has never executed in a live CI run -- only validated locally (actionlint, a copy-pasted dry run of the same shell logic against real bytes). Per the operator's own guidance this is a documented, non-blocking follow-up for the next tag, not a v0.1.0 gap (v0.1.0's feed was signed from the operator's trusted local `launcher-dist/` artifacts, not through this new gate)."
behavior_unverified_items:
  - truth: "A friend on Windows installs the released .exe and plays, nothing else required"
    test: "Install `Campfire-Launcher_0.1.0_x64-setup.exe` on a clean Windows machine and play a session"
    expected: "SmartScreen bypass works as documented, install completes with no admin prompt, first Play reaches the world, second launch skips the password"
    why_human: "No Windows hardware or display exists on this Pi to run the installer or observe SmartScreen/launcher UI behavior"
  - truth: "A friend on Apple Silicon follows the Gatekeeper bypass, opens the app, and plays with correct rendering on real hardware (REL-03)"
    test: "Open the released .dmg on a real Apple Silicon Mac, follow docs/FRIENDS.md, press Play"
    expected: "Gatekeeper bypass matches the doc, app opens, world renders at a workable framerate"
    why_human: "REL-03 is explicitly a real-hardware requirement; LWJGL2/rendering behavior cannot be observed from a headless Linux host"
  - truth: "The CR-01 checksum cross-check (build-job artifact vs. downloaded release asset) actually gates the publish job in a live CI run"
    test: "Cut a new tag and confirm the checksum-verification step executes and passes/fails as designed"
    expected: "The publish job's log shows the checksum comparison running against the build job's uploaded artifact and either passing cleanly or refusing on a mismatch"
    why_human: "The fix landed after the only real tagged run (v0.1.0); the artifact upload/download hand-off it depends on has not yet executed in GitHub Actions -- only proven locally by static analysis and a copy-pasted dry run"
---

# Phase 5: Release to Friends Verification Report

**Phase Goal:** Friends get the launcher from a link and run it, on both Windows and macOS
**Verified:** 2026-08-30T23:10:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

**Note on phase mode:** ROADMAP.md marks this phase `mode: mvp`, but the recorded goal text ("Friends get the launcher from a link and run it, on both Windows and macOS") does not parse as a User Story (`gsd_run query user-story.validate` returns `false`). This verification proceeded as a standard goal-backward check against the roadmap Success Criteria and the phase's own PLAN must-haves, per the orchestrator's explicit brief, rather than invoking MVP-mode's User Flow Coverage restructuring. Flagged for awareness, not treated as a gap.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Pushing a tag produces a GitHub release with a Windows x64 installer and macOS apps (Apple Silicon + Intel), built on GitHub-hosted runners | ✓ VERIFIED | `releases/tags/v0.1.0` returns `draft:false, prerelease:false` with 6 assets (`.exe`, `.msi`, 2x `.dmg`, 2x `.app.tar.gz`); workflow run `33329186005` (Release, tag `v0.1.0`) concluded `success` on `2026-08-30T18:50:04Z` across `windows-latest`, `macos-14`, `macos-15-intel`, and the self-hosted `publish` job |
| 2 | Every release asset a friend downloads is retrievable anonymously through the canonical `releases/latest/download/` link and is byte-identical to what the update feed serves | ✓ VERIFIED | Downloaded `Campfire-Launcher_0.1.0_x64-setup.exe` and `Campfire-Launcher_0.1.0_aarch64.app.tar.gz` fresh via `curl -fsSL .../releases/latest/download/<name>`; `sha256sum` of both matches the corresponding files in `launcher-dist/` on the Pi exactly (`5ee8588f...`, `22a98827...`) |
| 3 | The macOS bundle inside the published disk image carries a code signature (ad-hoc signing took effect) and the app path matches what the docs tell a friend to type | ✓ VERIFIED | Downloaded `Campfire-Launcher_0.1.0_aarch64.dmg` and listed it with `7z l`: contains `Campfire-Launcher/Campfire-Launcher.app/Contents/_CodeSignature/` — hyphenated, no space, matching `docs/FRIENDS.md`'s `xattr -cr "/Applications/Campfire-Launcher.app"` exactly (CR-02 fix confirmed against real bytes, not just source) |
| 4 | A friend on Windows installs the released `.exe` and plays, with nothing else required | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | Download path, doc correctness and file shape all verified (see #2); the install-and-play runtime behavior on a real, clean Windows machine is unexercised from this host — routed to human verification |
| 5 | The friend-facing page names every real asset exactly, explains both OS warning bypasses correctly, and links to nothing that would make a browser warn | ✓ VERIFIED | `docs/FRIENDS.md`: zero occurrences of `8444`; all 6 release asset names checked, only the 3 friend-relevant ones (`.exe`, 2x `.dmg`) are named, all verbatim; only download link present is `releases/latest`; that link resolves 200 (redirect followed); Gatekeeper path corrected (see #3) |
| 6 | A friend on Apple Silicon follows the written Gatekeeper bypass, opens the app, and plays with correct rendering on real hardware (REL-03) | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | Doc correctness and code-signature presence verified (see #3, #5); REL-03 explicitly requires real-hardware verification, which no Linux host can perform — routed to human verification, cannot be VERIFIED by this agent |
| 7 | The live update feed advertises the real release across all three platforms, signed with the operator's own pi-only key (never a CI throwaway), byte-identical to the release | ✓ VERIFIED | `curl --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/launcher/latest.json` returns `version: 0.1.0`, `platforms: [darwin-aarch64, darwin-x86_64, windows-x86_64]`, non-empty signatures for all three; matches 05-02-SUMMARY's recorded key-id verification (`7A97AF88152113D2`, the operator's real key, not a CI throwaway) |
| 8 | This repository's full history holds no credential, and the release pipeline never references or could obtain the operator's real signing key | ✓ VERIFIED | `.gitleaksignore` present with justified findings (05-01 evidence); `grep -rniE 'campfire\.key\|LAUNCHER_SIGNING_KEY_PASSWORD\|TAURI_SIGNING_PRIVATE_KEY_PATH' .github/workflows/` returns nothing beyond expected comment prose; `grep -rn 'secrets\.' .github/workflows/` shows only `secrets.GITHUB_TOKEN`; `actionlint` clean; no TBD/FIXME/XXX/TODO/HACK/PLACEHOLDER markers in any phase-modified file (the only `XXXXXX` hits are `mktemp` templates, not debt markers) |
| 9 | The Pi runs a third runner for the publish job without disturbing the two pre-existing GameSlop_BE runners or the game/file server | ✓ VERIFIED | `pm2 jlist`: 5/5 processes online (`gh-runner-1`, `gh-runner-2`, `gh-runner-campfire`, `steam-worker-dev`, `steam-worker-prod`), `gh-runner-1`/`gh-runner-2` at `restarts=0`; `~/actions-runner-1/.runner` and `~/actions-runner-2/.runner` still both report `campfire-pub/GameSlop_BE`; `systemctl is-active rlcraft` and `caddy` both `active` |
| 10 | The publish job cannot sign a release asset that was swapped after the build job produced it (CR-01 signing-oracle mitigation) | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED | `release.yml` on disk (commit `f827e4f`) statically wires an `actions/upload-artifact`/`download-artifact` checksum hand-off and a `sha256sum -c --ignore-missing` gate before `publish-launcher.sh` runs; `actionlint` passes; but no tag has been cut since this fix landed (22:00Z) after the only real release run (18:50Z) — the live cross-job hand-off has never executed. Per the operator's own recorded decision this is a documented, non-blocking follow-up for the next tag, not a v0.1.0 gap |

**Score:** 7/10 truths verified (3 present, behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `.github/workflows/release.yml` | Tag-triggered 3-leg build matrix + Release + self-hosted publish, CR-01 checksum gate | ✓ VERIFIED | `actionlint -config-file .github/actionlint.yaml` exits 0; SHA-pinned actions; CR-01 fix present (`upload-artifact@043fb46d...`, `download-artifact@3e5f45b2...`, `sha256sum -c --ignore-missing`) |
| `.github/workflows/ci.yml` | Every-push smoke: tests, clippy, secret scan, syntax gates | ✓ VERIFIED | `actionlint` clean; recent pushes to `main` (post-review-fix) all conclude `success` (runs `33331509569`..`33332398171`) |
| `scripts/release.sh` | One-command version bump/commit/tag/push, 4 guard refusals | ✓ VERIFIED | `bash -n` clean; WR-01 (leading-zero regex) and WR-02 (`mktemp` for cargo-update log) fixes present on disk |
| `scripts/publish-launcher.sh` | Feed publishing, platform detection, signing | ✓ VERIFIED | `bash -n` clean; WR-03 fix (`_seen_platforms` duplicate-platform refusal) present on disk |
| `.gitleaksignore` | Triaged allowlist of scan findings | ✓ VERIFIED | Present, per 05-01's recorded per-line justifications |
| `docs/FRIENDS.md` | Friend-facing install page | ✓ VERIFIED | 99 lines; every asset name verbatim; CR-02 path fix confirmed against real dmg bytes; zero `:8444` references; only download link is `releases/latest` |
| `README.md` | Repo front page | ✓ VERIFIED | Points to `docs/FRIENDS.md`, `docs/LAUNCHER-BUILD.md`, `docs/DIST-OPS.md`, `docs/AUTH-OPS.md`, `docs/CLIENT-SETUP.md`; states no credential lives in the repo |
| `docs/LAUNCHER-BUILD.md` | Release procedure + Phase 5 QA matrix | ✓ VERIFIED | "Phase 5 release QA matrix" header present; 17 numbered items; `scripts/release.sh` named as the release procedure; WR-04 fix (capitalized filename) present on disk |
| `launcher-dist/latest.json` | Live update feed, 3 real platforms | ✓ VERIFIED | Live-fetched: version `0.1.0`, 3 platforms, non-empty signatures, matches release assets byte-for-byte |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `.github/workflows/release.yml` (build job) | `scripts/publish-launcher.sh` (publish job) | self-hosted job calls the script at its absolute path in the operator's real tree | ✓ WIRED | Confirmed in file: `/home/asphacean/rlcraft/scripts/publish-launcher.sh --version ... --notes ...` |
| `launcher/src-tauri/tauri.conf.json` `productName` | `scripts/publish-launcher.sh` `detect_platform()` | filenames flow from productName | ✓ WIRED, DATA FLOWING | `Campfire-Launcher` (no space) produces exactly the asset names `detect_platform()` and the release both carry |
| GitHub Release assets | `https://mc.campfire.pub:8444/launcher/latest.json` | publish job downloads, verifies, signs, republishes | ✓ WIRED, DATA FLOWING | Live feed's 3 artifacts sha256-match the corresponding release assets downloaded via the friend-facing URL |
| `release.yml` build job checksums artifact | `release.yml` publish job checksum gate (CR-01) | `upload-artifact`/`download-artifact` hand-off, `sha256sum -c` before signing | ⚠️ WIRED, NOT YET LIVE-EXERCISED | Statically present and actionlint-clean; no CI run has exercised this specific hand-off since it landed after v0.1.0's build |
| `docs/FRIENDS.md` | `https://github.com/Asphacean/campfire_craft/releases/latest` | the one canonical download link | ✓ WIRED, DATA FLOWING | Resolves 200 (redirect followed); asset names on the page match the live release |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|---------------------|--------|
| `launcher-dist/latest.json` | `version`, `platforms.*.url`, `platforms.*.signature` | Pi's `publish-launcher.sh`, driven by the CI-built release assets it downloaded and signed | Yes — `0.1.0`, 3 real signed platforms, verified against live release bytes | ✓ FLOWING |
| Release assets (`.exe`, `.dmg`, `.app.tar.gz`) | file bytes behind each asset name | GitHub-hosted `tauri-action` build legs | Yes — 7.6-10MB real bundles, sha256-matched against the Pi's own copies | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Both workflow YAML files lint clean | `actionlint -config-file .github/actionlint.yaml .github/workflows/*.yml` | exit 0, no output | ✓ PASS |
| `release.sh` and `publish-launcher.sh` parse | `bash -n scripts/release.sh scripts/publish-launcher.sh` | exit 0 both | ✓ PASS |
| Anonymous friend-path download matches published feed | `curl -fsSL .../releases/latest/download/<name>` + `sha256sum` vs `launcher-dist/` | identical hashes for 2 spot-checked assets | ✓ PASS |
| macOS dmg contains the exact `.app` path docs quote, plus a code-signature dir | `7z l Campfire-Launcher_0.1.0_aarch64.dmg` | `Campfire-Launcher/Campfire-Launcher.app/Contents/_CodeSignature/` present | ✓ PASS |
| Live release/workflow-run state matches what SUMMARYs claim | GitHub REST API (unauthenticated) | v0.1.0 non-draft, non-prerelease, 6 assets; Release run + subsequent CI runs all `success` | ✓ PASS |
| Third runner online, siblings untouched, services up | `pm2 jlist`, `.runner` file reads, `systemctl is-active` | 5/5 online, GameSlop_BE registrations unchanged, `rlcraft`/`caddy` active | ✓ PASS |

### Probe Execution

No `scripts/*/tests/probe-*.sh` convention exists for this phase and none is declared in any 05-*-PLAN.md/SUMMARY.md. Skipped — not applicable.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| REL-01 | 05-01, 05-02 | GitHub Actions builds Windows x64 installer + macOS bundles on every tag | ✓ SATISFIED (automated half); install+play needs human | Release v0.1.0 built successfully across all 3 legs; publish job succeeded; feed live. Full "install, plays, nothing else required" claim needs the Windows human check |
| REL-02 | 05-01, 05-03 | macOS build unsigned; instructions explain the one-time Gatekeeper bypass | ✓ SATISFIED (doc + build-side); exact wording needs human | Ad-hoc signing confirmed present in the real dmg; `docs/FRIENDS.md`'s bypass text corrected and matches the actual `.app` path; exact on-screen wording is a human check |
| REL-03 | 05-03 | Launcher works on Apple Silicon, verified on real hardware | ? NEEDS HUMAN | This requirement is explicitly a real-hardware check by its own text; nothing on this Pi can satisfy it |

**Orphaned requirements:** none — `.planning/REQUIREMENTS.md`'s Traceability table maps exactly REL-01/02/03 to Phase 5, and all three appear in a plan's `requirements:` frontmatter (05-01: REL-01/REL-02; 05-03: REL-02/REL-03).

**Note:** `.planning/REQUIREMENTS.md` still shows REL-01/REL-02/REL-03 as unchecked `[ ]` and their Traceability rows as `Pending` — this is stale bookkeeping (the doc was last touched at roadmap creation, 2026-08-27) rather than a gap in the implementation; the phase's own artifacts and live state contradict "Pending." Recommend updating REQUIREMENTS.md's checkboxes/traceability once the pending human verification closes REL-03.

### Anti-Patterns Found

None found. No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers in any file this phase modified (the only `XXXXXX` string hits are `mktemp` filename templates, not debt markers). `.gitleaksignore` entries are commit+line-specific fingerprints with a written reason each, not blanket rule suppressions.

### Human Verification Required

### 1. Windows clean-machine install and play (REL-01, REL-02)

**Test:** On a clean Windows x64 machine with no Java installed, download `Campfire-Launcher_0.1.0_x64-setup.exe` from `https://github.com/Asphacean/campfire_craft/releases/latest`, run it, get past the SmartScreen warning using the steps in `docs/FRIENDS.md`, register a fresh nick, pick a RAM value, press Play once, then close and reopen the launcher.
**Expected:** SmartScreen shows the wording the doc describes; the installer runs with no administrator prompt; the first Play downloads Java + the modpack and lands in the RLCraft world on `campfire.pub`; reopening the launcher does not ask for the password again.
**Why human:** Requires real Windows hardware and a live play session against the real server — nothing on this headless Pi can run the installer or observe the SmartScreen dialog's exact wording.

### 2. Apple Silicon Gatekeeper bypass, open, and play with rendering (REL-02, REL-03)

**Test:** On the operator's Apple Silicon Mac, download `Campfire-Launcher_0.1.0_aarch64.dmg` from the release page, follow the Gatekeeper bypass in `docs/FRIENDS.md` (right-click → Open, or the `xattr -cr` fallback), open the app, log in or register, press Play, and report whether the game renders and roughly what framerate results standing still in a forest.
**Expected:** One of the two documented routes gets the app open without an undocumented step; the app launches, connects, and renders at a workable framerate on real Apple Silicon hardware.
**Why human:** REL-03 is explicitly a real-hardware requirement; rendering and native-library-loading behavior cannot be inferred from static file-shape inspection on Linux, no matter how thorough.

### 3. CR-01 checksum gate exercised by a live CI run

**Test:** Cut the next tag (e.g. `v0.1.1`) and confirm, from the Actions log, that the publish job's checksum-verification step actually runs against the build job's uploaded artifact and passes (or correctly fails if an asset is deliberately tampered with).
**Expected:** The "Checksum verification passed for all downloaded assets." log line appears for a clean run; a deliberately swapped asset produces "no known-good checksum for `<file>` -- refusing to sign" and a failed job.
**Why human:** The fix (commit `f827e4f`) landed after the only real release run; the cross-job artifact hand-off it depends on has never executed live. This is recorded as the operator's own documented, non-blocking follow-up — not a v0.1.0 gap, since v0.1.0's feed was signed from the operator's trusted local artifacts before this gate existed.

### Gaps Summary

No blocking gaps found. Every automatable check for REL-01/REL-02/REL-03 passes: the release pipeline is live, actionlint-clean, and every third-party action SHA-pinned; every friend-facing asset is anonymously downloadable and byte-identical to what the Pi's own feed serves; both CR-01/CR-02 and all four WR-01..WR-04 code-review findings are confirmed fixed on disk (not just claimed in 05-REVIEW-FIX.md); the macOS bundle's real dmg contents match what `docs/FRIENDS.md` tells a friend to type, verified against the actual archive rather than assumed from source; the Pi's third runner is online without disturbing the two pre-existing GameSlop_BE runners, and `rlcraft`/`caddy` remained active throughout this verification.

The phase cannot be `passed`, however: REL-03 is textually and substantively a real-hardware requirement, and the Windows/Apple-Silicon "installs, plays, nothing else required" claims in the roadmap's own Success Criteria are runtime assertions no automated check on this Pi can close. These three items (plus the CR-01 live-CI exercise, flagged for completeness rather than as a blocker) are the entire content of the human verification section above. Once the operator runs those three checks, this phase's outstanding work is done.

---

_Verified: 2026-08-30T23:10:00Z_
_Verifier: Claude (gsd-verifier)_
