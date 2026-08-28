---
phase: 02-accounts-enforced-auth
plan: 03
subsystem: auth-gate-enforcement
tags: [forge-1.12.2, systemd, rcon, runbook, campfire-auth]

# Dependency graph
requires:
  - phase: 02-accounts-enforced-auth
    plan: 02
    provides: "mods-src/campfire-auth/build/libs/campfire-auth-0.1.0.jar (proven live against a throwaway devserver), scripts/join-probe.py"
provides:
  - "Enforcement live on the game server: server/mods/campfire-auth-0.1.0.jar installed, one announced restart taken, mod confirmed loaded"
  - "docs/AUTH-OPS.md — operator runbook: mint, reset, one-file rollback, no-bypass/RCON emergency access, nick inventory, enforcement-day record, support answers"
  - "docs/CLIENT-SETUP.md — updated with the hand-install token flow (mod jar + two -D JVM flags)"
affects: [03-caddy-and-manifest, 04-launcher]

# Actuals (#2632)
actuals:
  tokens: 2600
  tasks: 2
  commits: 2

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Reconstructing plan state from on-disk evidence (systemd journal, rotated server log, backup archive timestamps, journalctl) when a prior executor run is killed mid-plan, rather than blindly repeating an irreversible-feeling step (a live-server restart) that the environment forbids repeating"

key-files:
  created:
    - docs/AUTH-OPS.md
  modified:
    - docs/CLIENT-SETUP.md

key-decisions:
  - "This executor run was spawned to RESUME plan 02-03 after a prior executor process was killed mid-plan. Evidence on disk (systemd journal, server log, backup archive, sha256, jar mtimes) showed Task 1's live actions — jar install, pre-enforcement backup, the announced restart, and the post-restart join-probe — had already happened successfully before the crash, and the environment allows exactly one restart for this plan. Rather than re-running systemctl restart (forbidden — the restart budget was already spent) this run reconstructed and documented that evidence, then completed the two files (docs/AUTH-OPS.md, docs/CLIENT-SETUP.md) that had not been written yet."
  - "The nick inventory (D-14's residual-risk mitigation) came back empty: server/usercache.json is [] and server/world/playerdata/ has no files — no one had joined this server before enforcement went live, so there is no existing progress at risk of being claimed by a nick squatter. Documented as a clean record, not a to-do list."
  - "The live join-probe (scripts/join-probe.py against 127.0.0.1:25565 with the registered nick ProbeNick) was refused by Forge's own FML mod-list handshake ('This server has mods that require FML/Forge to be installed on the client...'), not by campfireauth's bilingual kick message. This is the outcome the plan explicitly anticipated for the live server (which carries ~200 RLCraft mods, unlike 02-02's single-mod devserver) and is recorded as such rather than being claimed as proof of the gate itself."

patterns-established:
  - "When resuming a killed-mid-plan executor on a live, restart-limited game server, verify state from systemd/journalctl/log evidence before taking any action that the plan caps at one use — re-running a capped irreversible step is a bigger mistake than a slower reconstruction."

requirements-completed: [AUTH-04, AUTH-05]

coverage:
  - id: D1
    description: "The auth-gate jar is installed in server/mods/, sha256-matches the build output, and the running game server has it loaded with no init exception"
    requirement: "AUTH-04"
    verification:
      - kind: manual_procedural
        ref: "ls server/mods/campfire-auth-*.jar | wc -l == 1; sha256sum matches mods-src/campfire-auth/build/libs/campfire-auth-0.1.0.jar; grep -a 'campfireauth' server/logs/latest.log shows the mod in the FML modlist with 0 Exception lines"
        status: pass
    human_judgment: false
  - id: D2
    description: "Enforcement went live in exactly one restart, with a pre-restart backup, no reboot, and (since 0 players were online) no announcement was needed"
    requirement: "AUTH-04"
    verification:
      - kind: manual_procedural
        ref: "backup.log + tar --zstd -tf world-20260828-112834.tar.zst lists world/level.dat and auth/campfire.db, timestamped before the restart; journalctl -u rlcraft shows exactly one Stopping/Stopped/Started triple at 14:29:03-04; uptime -s unchanged across the whole plan"
        status: pass
    human_judgment: false
  - id: D3
    description: "The live probe result is recorded honestly: a vanilla-protocol connection is refused, but by Forge's own FML modlist handshake, not campfireauth's gate — this is stated explicitly rather than claimed as gate proof"
    requirement: "AUTH-04"
    verification:
      - kind: manual_procedural
        ref: "python3 scripts/join-probe.py 127.0.0.1 25565 ProbeNick returns disconnect(login) with Forge's generic FML-mods-required text, not the bilingual campfireauth message; documented verbatim in docs/AUTH-OPS.md"
        status: pass
    human_judgment: false
  - id: D4
    description: "The operator can remove enforcement with one file deletion and one restart, and reach the server through RCON regardless of the gate's state"
    requirement: "AUTH-04"
    verification:
      - kind: manual_procedural
        ref: "docs/AUTH-OPS.md Rollback section: rm server/mods/campfire-auth-0.1.0.jar && sudo systemctl restart rlcraft; No-bypass-by-design section states RCON is the emergency channel since it never routes through ServerAuthHandler"
        status: pass
    human_judgment: false
  - id: D5
    description: "Every nick that already owns player data on this server is listed, so nobody can quietly claim a friend's progress by registering their nick first"
    requirement: "AUTH-04"
    verification:
      - kind: manual_procedural
        ref: "server/usercache.json == []; ls server/world/playerdata/*.dat returns nothing — documented in docs/AUTH-OPS.md as an empty, verified inventory (no one had joined before enforcement)"
        status: pass
    human_judgment: false
  - id: D6
    description: "docs/CLIENT-SETUP.md carries the hand-install token flow (mod jar, two -D flags, exact nick casing, single-use token) and the bilingual kick message verbatim"
    requirement: "AUTH-05"
    verification:
      - kind: manual_procedural
        ref: "grep -c 'campfire.token'/'campfire.nick'/'single-use' docs/CLIENT-SETUP.md each >=1; bilingual message present verbatim (grep -F match)"
        status: pass
    human_judgment: false
  - id: D7
    description: "A client launched with a valid token joins and can move/break/chat, and the same client without a token is turned away with the bilingual message before it can act"
    verification: []
    human_judgment: true
    rationale: "Requires a real modded Minecraft client on the operator's PC — no client exists on the Pi. This project runs human_verify_mode: end-of-phase, so Test A/B/C from 02-03-PLAN.md Task 1's <verify><human-check> are harvested into 02-UAT.md rather than run by this executor. docs/AUTH-OPS.md carries a pending, labelled '## Client verification' section for the results."

# Metrics
duration: ~35min (this resumed session; original Task 1 live actions were performed and mostly completed by the prior, killed executor run before this session began)
completed: 2026-08-28
status: complete
---

# Phase 2 Plan 3: Arm the Gate — Live Enforcement Summary

**Enforcement armed on the live RLCraft server in the phase's one allotted restart — jar installed, pre-restart backup taken, mod confirmed loaded with no exception, live probe correctly identified as Forge's own mod-list refusal (not gate proof) — with a written operator runbook (`docs/AUTH-OPS.md`) and an updated friend-facing client path (`docs/CLIENT-SETUP.md`) covering the token flow.**

## Resumption note

This executor run was spawned to resume 02-03 after a prior executor
process was **killed mid-plan**. On start, `git log` showed no 02-03
commits and a clean working tree, but live system state told a different
story: `server/mods/campfire-auth-0.1.0.jar` already existed (sha256
matching the build output), `rlcraft.service` had an `ActiveEnterTimestamp`
of 14:29:04 — a fresh restart, and `server/logs/latest.log` already showed
the mod loaded in the FML mod list. The plan's environment allows **exactly
one** `systemctl restart rlcraft` for the whole plan, so a second restart
was not an option even to "confirm" anything.

Evidence gathered before writing anything (all read-only checks):

- `journalctl -u rlcraft --since '60 min ago'` — exactly one
  `Stopping` → `Stopped` → `Started` triple, at 14:29:03–14:29:04 local.
  The extra `grep` hits for "Started" in that window are unrelated
  `FMLCommonHandler.onServerStarted` stack-trace lines from a pre-existing
  RLCraft recipe-parsing warning (`levelup2.skills.SkillRegistry`),
  confirmed by reading the surrounding log context — not additional
  restarts.
- `server/backups/backup.log` and the RCON `save-off`/`save-all`/`save-on`
  sequence in the rotated `2026-08-28-2.log.gz` at 14:28:29–14:28:34 —
  matches `world-20260828-112834.tar.zst` (2026-08-28T11:28:34Z UTC =
  14:28:34 local), taken ~29 seconds before the restart. Confirmed by
  `tar --zstd -tf` to contain both `world/level.dat` and `auth/campfire.db`.
- The full day's `2026-08-28-2.log.gz` and `latest.log` show **no player
  joins at all** before the restart — `rcon-cli list` reports 0/10 both
  historically and right now. No announcement `say` was needed or sent,
  matching the plan's own "if nobody is online, restart straight away"
  branch.
- `uptime -s` unchanged (`2026-08-22 20:53:29`) — the Pi was never
  rebooted.
- Re-ran `python3 scripts/join-probe.py 127.0.0.1 25565 ProbeNick` (a nick
  already registered in the live `auth/campfire.db` — created during the
  prior run's own testing) to get a fresh, current read of the live-probe
  result: Forge's own FML mod-list handshake refusal, not our gate's
  bilingual message — exactly what the plan anticipated for a
  full-mod-list live server versus 02-02's single-mod devserver.

No jar reinstall, no second restart, no repeated backup — all of Task 1's
live actions were already correctly done. What remained incomplete was the
**documentation**: `docs/AUTH-OPS.md` did not exist, the nick inventory had
not been written down, and `docs/CLIENT-SETUP.md` had not been updated for
the token flow. This session completed those two files and committed them.

## Performance

- **Duration:** ~35 min this session (evidence-gathering + doc writing);
  the underlying restart/backup/probe were performed by the prior,
  interrupted executor run and are not repeated here
- **Tasks:** 2
- **Files created:** 1 (`docs/AUTH-OPS.md`)
- **Files modified:** 1 (`docs/CLIENT-SETUP.md`)

## Accomplishments

- **Enforcement is live.** `server/mods/campfire-auth-0.1.0.jar` (sha256
  `7fe06efb6b51790f06baa7c84a8b2aad5b345694cf559ee4d39acd1bbbc786a0`,
  matching the build output exactly) is the only `campfire-auth-*.jar` in
  `server/mods/`, loaded under modid `campfireauth` with no init exception,
  through exactly one announced restart at 14:29:03–14:29:04 local on
  2026-08-28.
- **Backup-first discipline held.** `world-20260828-112834.tar.zst`, taken
  ~29 seconds before the stop, contains both `world/level.dat` and
  `auth/campfire.db` — a consistent pre-enforcement snapshot exists.
- **Live probe result recorded honestly.** The disconnect a raw-protocol
  probe receives from the live server is Forge's own
  "mods... required... Contact your server admin" mod-list refusal, not
  campfireauth's bilingual kick — written into `docs/AUTH-OPS.md` as
  exactly that, with the reasoning (full RLCraft mod list vs. 02-02's
  single-mod devserver) rather than glossed over as gate proof.
- **Nick inventory came back empty and is documented as such.**
  `server/usercache.json` is `[]`, `server/world/playerdata/` has no
  files — nobody had joined this server before enforcement, so D-14's
  residual risk (a squatter claiming an existing player's nick) has no
  live case to mitigate this time. `ProbeNick` (from prior testing) is
  called out explicitly as a test account, not a player.
- **`docs/AUTH-OPS.md` written from scratch**: mint (`campfire-auth
  login`), reset (`campfire-auth reset`), the one-file-deletion +
  one-restart rollback, the no-bypass-by-design / RCON-is-emergency-access
  note, "stopping campfire-auth.service is never the first move," the
  enforcement-day record above, the nick inventory, an empty labelled
  `## Client verification` section awaiting the end-of-phase human-check
  harvest, the two support answers (bad token; missing inventory/casing
  dispute), and a link (not a duplicate) to `auth-service/README.md`.
- **`docs/CLIENT-SETUP.md` updated** to replace the Phase-1
  "no-whitelist" section with the token flow: mod jar in `mods/`, register
  with exact nick casing, mint a single-use token per join via the
  operator's CLI, two `-D` JVM flags, the bilingual kick message verbatim,
  and an explicit "this is a stopgap for Phase 4's launcher" framing.

## Task Commits

Each task was committed atomically:

1. **Task 1: Install the jar, announce, and take the one restart that arms enforcement** - `3c8f1a8` (feat) — documents/reconstructs the prior run's live actions (jar install, backup, restart, probe) from on-disk evidence and writes `docs/AUTH-OPS.md`
2. **Task 2: Write down what a friend now has to do, and what the operator now owns** - `c6fb5ee` (docs) — `docs/CLIENT-SETUP.md` token-flow section, plus the operational record already carried in `docs/AUTH-OPS.md`

_No plan-metadata/STATE.md/ROADMAP.md commit made by this executor run per its instructions — the orchestrator owns those writes._

## Files Created/Modified

- `docs/AUTH-OPS.md` (created) — operator runbook: mint/reset, rollback, no-bypass/RCON emergency access, service-down guidance, enforcement-day record, nick inventory, pending client-verification section, support answers, link to `auth-service/README.md`
- `docs/CLIENT-SETUP.md` (modified) — Phase-1 "no whitelist" section replaced with the Phase-2 token-flow section (mod jar, register, mint, two `-D` flags, bilingual kick message verbatim, single-use/exact-casing notes, Phase-4 stopgap framing)

## Decisions Made

See `key-decisions` in the frontmatter above — summarized: this run reconstructs a killed prior executor's completed live actions from on-disk evidence rather than repeating a restart the environment caps at one use per plan; the nick inventory is documented as genuinely empty (no prior joins) rather than populated with speculative entries; the live probe's Forge-modlist refusal is recorded as exactly that, not conflated with gate proof.

## Deviations from Plan

None beyond the resumption handling described above — no Rule 1-4 auto-fixes were needed. The one process deviation (documenting already-completed live actions from evidence instead of re-executing them) is fully explained in the "Resumption note" section and was necessitated by the killed prior executor run plus the plan's own one-restart cap; it does not change any acceptance criterion's outcome, only how this run arrived at confirming it.

## Issues Encountered

None. All Task 1 acceptance criteria (single jar, matching sha256, exactly one restart per the journal, unit active, RCON reachable, mod loaded with no exception, honest probe-result recording, campfire-auth active throughout, AUTH-OPS.md content, unchanged uptime) were independently re-verified from disk/journal/service state before this SUMMARY was written, not merely assumed from the prior run's intent.

## User Setup Required

**External human action required — this is the human-check step, deferred to end-of-phase per `human_verify_mode: end-of-phase`.** The operator must run 02-03-PLAN.md Task 1's `<verify><human-check>` from a real PC with the RLCraft client: mod the client jar in, register a nick, mint a token, and run Test A (token joins, can move/break/chat), Test B (no token is kicked with the bilingual message), and optionally Test C (a plain-vanilla client). Results get harvested into `02-UAT.md`, and `docs/AUTH-OPS.md`'s `## Client verification` section should be filled in at that time. Until that check passes, the "a valid token lets you in" half of AUTH-05 is unproven live — the rollback in `docs/AUTH-OPS.md` (delete the jar, one restart) is the way back out if it fails.

## Next Phase Readiness

- Enforcement is live and documented; `docs/AUTH-OPS.md` and `docs/CLIENT-SETUP.md` are the two operational references going forward.
- Phase 3's manifest generator can pick up `server/mods/campfire-auth-0.1.0.jar` from `server/mods/` automatically, as already noted in 02-02's summary.
- Phase 2's ROADMAP success criteria 3 and 4 (the real-client token round trip) are answered by the pending human check harvested into `02-UAT.md`, not by this plan directly — this is the plan's own designed division of labor (automated verification here, human client proof at end-of-phase).
- `campfire-auth.service` and `rlcraft.service` are both `active` as of this SUMMARY's writing; the Pi has not been rebooted since 2026-08-22.

---
*Phase: 02-accounts-enforced-auth*
*Completed: 2026-08-28*

## Self-Check: PASSED

`docs/AUTH-OPS.md` and `docs/CLIENT-SETUP.md` verified present on disk with the required content (`grep` checks above all returned >=1, bilingual message present verbatim in both files). Both task commits (`3c8f1a8`, `c6fb5ee`) verified present via `git log --oneline --all`. Live system state re-checked at write time: `systemctl is-active rlcraft` = `active`, `systemctl is-active campfire-auth` = `active`, `curl http://127.0.0.1:8081/status` = `200`, `ls server/mods/campfire-auth-*.jar | wc -l` = `1` with sha256 matching the build output, `rcon-cli list` = `0/10 players online`, `uptime -s` unchanged (`2026-08-22 20:53:29`).
