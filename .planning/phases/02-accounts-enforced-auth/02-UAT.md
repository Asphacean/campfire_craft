---
status: testing
phase: 02-accounts-enforced-auth
source: [02-VERIFICATION.md]
started: 2026-08-28T13:00:15Z
updated: 2026-08-28T13:00:15Z
---

## Current Test

number: 1
name: Modded client with valid token joins and plays
expected: |
  Register nick in launcher (or campfire-auth CLI), launch RLCraft client with campfire-auth jar and -Dcampfire.nick/-Dcampfire.token (or via launcher); join mc.campfire.pub; can move, break a block, chat.
awaiting: user response

## Tests

### 1. Modded client with valid token joins and plays
expected: Player with valid token spawns normally and can move/break/chat (see docs/AUTH-OPS.md "Client verification").
result: [passed] via 2026-08-31 Windows x64 operator QA (see 05-UAT.md test 1)

### 2. Modded client without token is kicked
expected: Same client, registered nick, no -D flags: kicked with bilingual "Зайди через лаунчер campfire.pub / Join via the campfire.pub launcher" before acting; server/logs/latest.log shows result=kick reason=no_packet.
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
