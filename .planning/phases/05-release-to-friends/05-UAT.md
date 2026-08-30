---
status: testing
phase: 05-release-to-friends
source: [05-VERIFICATION.md]
started: 2026-08-30T20:06:17Z
updated: 2026-08-30T20:06:17Z
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
result: [issue] App opens, but a spurious "Update Available" modal appears on 0.1.0 (feed is also 0.1.0); "Update Now" flashes "Launching" then the modal vanishes with no effect; "Later" button does nothing. Reported 2026-08-31.

### 3. Next release exercises the CR-01 checksum gate (non-blocking follow-up)
expected: On the next scripts/release.sh tag, the publish job verifies assets against the build job's checksums artifact before signing (watch the run once).
result: [pending]

### 4. Remaining infra checks from earlier phases (01-UAT 2–4): Pi reboot survival, 3-player TPS ≥ 15 (scripts/tps-log.sh 20m 30s), in-game restore fidelity
expected: Per 01-UAT.md.
result: [pending]

## Summary

total: 4
passed: 0
issues: 1
pending: 4
skipped: 0
blocked: 0

## Gaps
