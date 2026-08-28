---
status: testing
phase: 03-modpack-distribution
source: [03-VERIFICATION.md]
started: 2026-08-28T15:52:07Z
updated: 2026-08-28T15:52:07Z
---

## Current Test

number: 1
name: Client assembled from the manifest connects and plays
expected: |
  A client built purely from https://mc.campfire.pub:8444/manifest.json (via the Phase 4 launcher, or scripts/assemble-client.py copied to a PC + Forge 1.12.2-2860 + Java 8) joins mc.campfire.pub with a valid token and plays normally.
awaiting: user response

## Tests

### 1. Client assembled from the manifest connects and plays
expected: Joins and plays; no missing-mod / mod-mismatch rejection from the server.
result: [pending]

### 2. Certificate warning UX on a phone (optional)
expected: Opening https://mc.campfire.pub:8444/manifest.json off-LAN shows the expected private-CA warning, then JSON.
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
