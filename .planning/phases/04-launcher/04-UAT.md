---
status: testing
phase: 04-launcher
source: [04-VERIFICATION.md]
started: 2026-08-30T15:33:39Z
updated: 2026-08-30T15:33:39Z
---

## Current Test

number: 1
name: Windows x64 — clean machine, register, RAM, Play → in the world
expected: |
  Build per docs/LAUNCHER-BUILD.md § Windows x64; on a machine without Java: Create account → RAM slider → Play; progress shows steps + bytes; game starts and joins mc.campfire.pub.
awaiting: user response

## Tests

### 1. Windows x64 — clean machine, register, RAM, Play → in world
expected: Per docs/LAUNCHER-BUILD.md QA matrix "On Windows x64 — the main path" items.
result: [pending]

### 2. Windows x64 — second launch: no password, 0 bytes re-downloaded, saves/options untouched
expected: Form collapsed to "Playing as Nick · Log out"; sync reports nothing changed.
result: [pending]

### 3. Windows x64 — error banners: wrong password, server unreachable, "Open log" opens the log
expected: Plain-English messages per UI-SPEC; log path opens.
result: [pending]

### 4. macOS Apple Silicon — Rosetta prompt (if needed), x86_64 Java 8 downloaded, game renders and plays
expected: Per QA matrix "On macOS Apple Silicon"; playable framerate noted.
result: [pending]

### 5. Self-update dialog appears when the feed advertises a newer version
expected: "Update now / Later" dialog per UI-SPEC after scripts/publish-launcher.sh publishes a higher version.
result: [pending]

### 6. Art + layout match UI-SPEC (status pill readable over art, Play focal)
expected: Visual check against 04-UI-SPEC.md.
result: [pending]

## Summary

total: 6
passed: 0
issues: 0
pending: 6
skipped: 0
blocked: 0

## Gaps
