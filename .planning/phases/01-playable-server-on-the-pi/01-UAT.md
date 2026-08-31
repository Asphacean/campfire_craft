---
status: testing
phase: 01-playable-server-on-the-pi
source: [01-VERIFICATION.md]
started: 2026-08-28T09:03:35Z
updated: 2026-08-28T09:03:35Z
---

## Current Test

number: 1
name: Friend outside the home network joins by domain
expected: |
  A friend NOT on your LAN installs RLCraft 2.9.3 per docs/CLIENT-SETUP.md, adds server mc.campfire.pub, joins, and can move/play.
awaiting: user response

## Tests

### 1. Friend outside the home network joins by domain
expected: Friend (mobile hotspot or other home) joins mc.campfire.pub with RLCraft 2.9.3 client and plays.
result: [passed] via 2026-08-31 Windows x64 operator QA (see 05-UAT.md test 1)

### 2. Pi reboot survival
expected: Run `sudo reboot` on the Pi; within ~5 min `systemctl is-active rlcraft` prints active and a client can join again, with no operator action.
result: [pending]

### 3. Three-player 20-minute TPS test
expected: With 3 players online, run `bash scripts/tps-log.sh 20m 30s` on the Pi; min TPS ≥ 15. If lower, apply tuning ladder (HEAP=8G, then VIEW_DISTANCE=6 via install.sh --config-only + restart) and re-test.
result: [pending]

### 4. Restored world fidelity in-game
expected: Note position/inventory/a chest/Nether portal; run `bash scripts/restore.sh <latest world-*.tar.zst>`; rejoin — all four intact.
result: [pending]

## Summary

total: 4
passed: 0
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps
