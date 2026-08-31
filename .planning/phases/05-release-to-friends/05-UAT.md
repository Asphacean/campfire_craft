---
status: testing
phase: 05-release-to-friends
source: [05-VERIFICATION.md]
started: 2026-08-30T20:06:17Z
updated: 2026-08-31T00:45:00Z
---

## Current Test

number: 1
name: Windows clean install + play
expected: |
  Download Campfire-Launcher_0.1.0_x64-setup.exe from https://github.com/Asphacean/campfire_craft/releases/latest per docs/FRIENDS.md; SmartScreen "More info → Run anyway"; install; Create account; Play → in the RLCraft world on mc.campfire.pub. Relaunch: no password prompt, 0 bytes re-downloaded.
awaiting: user response

## Tests

### 1. Windows clean install + play (REL-01, REL-02; closes 04-UAT 1–3, 02-UAT 1–2, 01-UAT 1)
expected: Per docs/LAUNCHER-BUILD.md Phase 5 QA matrix, Windows section.
result: [pending]

### 2. Apple Silicon: Gatekeeper bypass + play + rendering (REL-02, REL-03; closes 04-UAT 4)
expected: Download aarch64 .dmg; right-click Open / xattr -cr "/Applications/Campfire-Launcher.app"; Rosetta prompt OK; game renders and plays; note framerate.
result: [issue] App opens, but a spurious "Update Available" modal appears on 0.1.0 (feed is also 0.1.0); "Update Now" flashes "Launching" then the modal vanishes with no effect; "Later" button does nothing. Reported 2026-08-31. **Fixed 2026-08-31, commit `41c7894`** — root cause: `.update-overlay { display: flex; }` is a normal-priority author CSS rule with the same specificity as the UA stylesheet's `[hidden] { display: none }`; origin/importance is resolved before specificity in the cascade, so the author rule always won regardless of the `hidden` attribute — the overlay was visible from first paint (before any version check ran) and stayed visible after "Later" set `hidden=true`, explaining both symptoms as one bug. Fixed with a global `[hidden] { display: none !important; }` rule (`.error-banner` carried the identical latent defect). Also fixed "Update Now" showing the Play button's "Launching…" label during download (added a dedicated `updateDownloading` string). Regression test added in `launcher/core/src/update.rs` decoding the real captured production feed. `cargo test --workspace`/`clippy`/`cargo tauri build --no-bundle` all pass. **Awaiting re-verification on real Apple Silicon hardware** — this Pi has no display to confirm the modal now stays hidden/dismissable in practice; a v0.1.x release build carries the fix (see test 3).

### 3. Next release exercises the CR-01 checksum gate (non-blocking follow-up)
expected: On the next scripts/release.sh tag, the publish job verifies assets against the build job's checksums artifact before signing (watch the run once).
result: [blocked] Three release attempts made 2026-08-31 (v0.1.1, v0.1.2, v0.1.3), all carrying the test-2 UI fix; none has published to the feed yet (`https://mc.campfire.pub:8444/launcher/latest.json` still reports `0.1.0`). Retry budget (≤3 per phase 5 rule) exhausted — **checkpoint requested, see Gaps below.**
  - **v0.1.1** (`596bcae`): `build (macos-14)` failed at the CR-01 checksum-recording step — macOS runners have no `sha256sum` (GNU coreutils; this image's GNU tools are `g`-prefixed) and `xargs --no-run-if-empty` is a GNU-only long option BSD/macOS `xargs` rejects outright. `build (windows-latest)` passed only because Git-for-Windows ships GNU coreutils, masking the defect there. **Diagnosed + fixed**, commit `279cc03` (branch on `$RUNNER_OS` for `shasum -a 256` vs `sha256sum`; `find -exec {} +` instead of `xargs`, which never invokes the hash command when nothing matches, on both BSD and GNU `find`).
  - **v0.1.2** (`fa2f74a`): all three `build` legs passed (confirms the v0.1.1 fix), but `publish` (self-hosted, this Pi) failed: `curl` exit 22 (HTTP failure under `-f`) on either the release-lookup GET or an asset download. This runner shares its public IP with this Pi's own unauthenticated GitHub API polling (used to watch this very run) — a transient 403 (60/hour unauthenticated rate limit) or a brief asset-CDN propagation 404 right after upload is a real, plausible cause; manually reproducing the same two `curl` calls minutes later succeeded cleanly. **Fixed**, commit `646e7f4` (`--retry 5 --retry-delay 5 --retry-all-errors` on both `publish` job `curl` calls).
  - **v0.1.3** (`b2772e7`): all three `build` legs passed again. `publish` failed differently — `Process completed with exit code 1` (not 22), meaning the retried `curl` calls this time succeeded and the script reached one of its own `exit 1` guards (either "no matching release assets", "no known-good checksum for `$f` — refusing to sign", or `sha256sum -c`'s own non-zero exit on a mismatch). **Not fully diagnosed**: the self-hosted runner's local `_diag` worker log records step *timing* but not step *stdout/stderr text* (that streams live to GitHub's Actions UI, which requires an authenticated token to fetch via the REST API — `GET .../actions/jobs/{id}/logs` returns 403 "Must have admin rights" even against this public repo when unauthenticated). Reproduced the checksum-recording → grep-match → `sha256sum -c` logic locally against v0.1.3's actual released bytes (downloaded live, hashed, checked): passed cleanly (`OK` for all three files, exit 0) — this rules out a logic bug in the matching/verification code itself and points back toward a timing/propagation condition specific to the live run, but that is inference, not confirmed root cause.

### 4. Remaining infra checks from earlier phases (01-UAT 2–4): Pi reboot survival, 3-player TPS ≥ 15 (scripts/tps-log.sh 20m 30s), in-game restore fidelity
expected: Per 01-UAT.md.
result: [pending]

## Summary

total: 4
passed: 0
issues: 1
pending: 2
skipped: 0
blocked: 1

## Gaps

- **CR-01 checksum gate not yet proven end-to-end** (test 3): three consecutive `scripts/release.sh` tags (v0.1.1, v0.1.2, v0.1.3), each hitting a different failure — a real macOS `sha256sum`/`xargs` portability bug (fixed, `279cc03`), a transient publish-job `curl` failure this Pi's own unauthenticated API polling likely aggravated (mitigated with retries, `646e7f4`), and a third `exit 1` in the publish job whose exact triggering line could not be confirmed without authenticated access to the GitHub Actions job log (unauthenticated `GET .../actions/jobs/{id}/logs` returns 403 even on this public repo). The retry rule's ≤3 cap is now reached for this investigation session. The live feed (`https://mc.campfire.pub:8444/launcher/latest.json`) still serves `0.1.0`.
  - **Options for a human to unblock:** (a) `gh auth login` (or export a PAT) on this Pi so a future session can pull the exact job-log text and pinpoint the failing `exit 1` line precisely instead of inferring it; (b) re-run the failed `publish` job from the GitHub Actions web UI ("Re-run failed jobs") for the `v0.1.3` tag — the build artifacts and release assets already exist, so only `publish` needs to succeed; (c) authorize additional release attempts beyond the ≤3 cap if (a)/(b) aren't available.
  - Test 2 (the original Mac UAT bug) has a code fix in place (`41c7894`, all workspace tests/clippy/build green) but has not been re-verified visually on real Apple Silicon hardware — no display exists on this Pi to confirm. Whichever v0.1.x tag eventually publishes successfully carries this fix; a friend/operator with the actual hardware should redo test 2 against that build.
