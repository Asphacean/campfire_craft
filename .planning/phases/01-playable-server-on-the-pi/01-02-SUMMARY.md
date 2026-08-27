---
phase: 01-playable-server-on-the-pi
plan: 02
subsystem: infra
tags: [forge-1.12.2, systemd, rcon, nftables, temurin-8, rlcraft]

# Dependency graph
requires:
  - phase: 01-01
    provides: "Temurin 8 at an absolute path, verified/pinned RLCraft pack zip, rcon-cli, server.env with every key this plan's scripts read"
provides:
  - "scripts/install.sh — idempotent RLCraft pack unpack + Forge 1.12.2-14.23.5.2860 --installServer + rendered server.properties"
  - "scripts/start-server.sh — the systemd unit's ExecStart, execs Temurin 8 with Aikar G1GC flags on the command line"
  - "systemd/rlcraft.service — Restart=on-failure/15s, KillMode=mixed, RCON ExecStop, TimeoutStopSec=90, enabled at boot"
  - "systemd/rlcraft-restart.{service,timer} — daily 05:00 graceful `systemctl restart rlcraft`"
  - "scripts/harden-rcon.sh + systemd/rlcraft-nft.service — scoped nftables table dropping non-loopback RCON traffic, loaded independently of nftables.service"
  - "SERVER_JAR persisted in server.env, discovered dynamically (installer emits a non-'-universal' jar name)"
affects: [01-03-ops-tooling, 01-04-network-and-reachability]

# Actuals (#2632)
actuals:
  tokens: 9800
  tasks: 2
  commits: 2

# Tech tracking
tech-stack:
  added: [Forge 1.12.2-14.23.5.2860 (server), nftables (scoped table only, not the package's default service)]
  patterns:
    - "Dynamic jar/artifact discovery via glob instead of hardcoded filenames — the Forge 2860 installer emits forge-1.12.2-14.23.5.2860.jar, not the -universal.jar name the plan assumed, and this was only caught by discovering the name rather than hardcoding it"
    - "Scoped, dedicated nftables tables loaded by a purpose-built oneshot unit instead of enabling the distro's general firewall service, when the host already has other iptables-nft-managed infrastructure (Docker) that a service-level ruleset reload would flush"

key-files:
  created:
    - scripts/install.sh
    - scripts/start-server.sh
    - scripts/install-units.sh
    - scripts/harden-rcon.sh
    - server/server.properties.template
    - systemd/rlcraft.service
    - systemd/rlcraft-restart.service
    - systemd/rlcraft-restart.timer
    - systemd/rlcraft-nft.service
  modified:
    - server.env (SERVER_JAR added)

key-decisions:
  - "Task 2's firewall step deviated from the plan's `systemctl enable --now nftables` — Debian's default /etc/nftables.conf opens with a full ruleset flush, which would wipe this Pi's Docker-managed iptables-nft tables on every service start. Loaded the scoped rlcraft table via a dedicated rlcraft-nft.service oneshot unit instead, touching neither /etc/nftables.conf nor nftables.service. See Deviations."
  - "server.properties difficulty rendered as the numeric `3`, not the plan text's literal `difficulty=hard` — Forge 1.12.2's EnumDifficulty parses server.properties by integer id, not by name; a literal 'hard' string would silently fail to parse and fall back to the JVM default (not hard). 3 IS D-12's intended hard difficulty, RLCraft's own pack docs confirm the same value."
  - "server.properties keys not explicitly listed in the plan's override set were taken from the RLCraft Server Pack 2.9.3's own bundled server.properties and its 'FOR SERVERS ONLY' stability notes (enable-command-block=true, allow-flight=true, max-tick-time=-1) rather than invented — this is what 'RLCraft pack defaults' concretely means for this pack."
  - "D-09 whitelist override carried forward from 01-01: WHITELIST_ENABLED=false renders white-list=false/enforce-whitelist=false; no whitelist entries were added via RCON (WHITELIST_NICKS is empty). The server is open to anyone who reaches TCP 25565 until Phase 2's auth-gate mod — unchanged accepted risk, T-02-02 in this plan's threat register stays open by operator decision, not a Phase 1 defect."

patterns-established:
  - "Any script deriving a value from the Forge installer's actual output (jar name) globs it via a shared discover_* helper used both for the 'already installed, skip' idempotency check and for persisting the value — using two different glob patterns for the same fact was the exact bug this plan's own Task 1 hit and fixed before commit."

requirements-completed: [SRV-01, SRV-02]

coverage:
  - id: D1
    description: "RLCraft Server Pack 2.9.3 on Forge 1.12.2-14.23.5.2860 runs on the Pi under Temurin 8 (not system Java 25), systemd-supervised, RCON-controlled"
    requirement: "SRV-01"
    verification:
      - kind: manual_procedural
        ref: "systemctl is-active rlcraft == active; /proc/<MainPID>/cmdline first field == JAVA8_BIN; rcon-cli list answers with max-players=10 in effect; systemd-analyze verify exits 0"
        status: pass
    human_judgment: false
  - id: D2
    description: "A hand-installed RLCraft 2.9.3 client on the LAN joins and plays"
    requirement: "SRV-01"
    verification: []
    human_judgment: true
    rationale: "Requires a real client install on a separate Windows/macOS machine and human play-testing (lag perception, join experience) — cannot be automated from the Pi. Recorded as pending human verification per this plan's autonomous instructions."
  - id: D3
    description: "Graceful systemd stop saves the world (RCON stop, not a bare kill) before the process exits"
    requirement: "SRV-02"
    verification:
      - kind: manual_procedural
        ref: "server/logs/latest.log shows 'Saving players' / 'Saving worlds' / 'Saving chunks for level' before the JVM exits after `sudo systemctl stop rlcraft`; unit restarted cleanly afterward"
        status: pass
    human_judgment: false
  - id: D4
    description: "Unit is enabled for boot and recovers from a SIGKILL crash unattended within 180 seconds"
    requirement: "SRV-02"
    verification:
      - kind: manual_procedural
        ref: "systemctl is-enabled rlcraft == enabled; SIGKILL sent 16:47:09, unit active + RCON answering again by 16:48:52 (103s), fresh 'Done (' log line present"
        status: pass
    human_judgment: false
  - id: D5
    description: "Daily 05:00 graceful restart is scheduled and restarts (not just stops) the server"
    requirement: "SRV-02"
    verification:
      - kind: manual_procedural
        ref: "systemctl list-timers shows rlcraft-restart.timer next-elapse 05:00; systemctl cat rlcraft-restart.service ExecStart is `systemctl restart rlcraft`; systemd-analyze verify exits 0"
        status: pass
    human_judgment: false
  - id: D6
    description: "RCON is reachable from the Pi and dropped for every other source, with no default-drop policy introduced anywhere else on the host"
    requirement: "SRV-02"
    verification:
      - kind: manual_procedural
        ref: "nft list table inet rlcraft shows policy accept + loopback-accept + `tcp dport 25575 drop`; rcon-cli from 127.0.0.1 still answers after the firewall loads; table inet rlcraft has zero policy-drop chains; Docker's pre-existing iptables-nft tables verified unmodified"
        status: pass
    human_judgment: true
    rationale: "True external-source blocking (a genuinely different host hitting port 25575) cannot be exercised from this single-host Pi session — connections from the Pi to its own LAN-interface IPs route via `dev lo table local` internally (confirmed with `ip route get`), so a self-test against the Pi's own address is not a valid proof of the drop rule. The rule's correctness was verified by inspecting the loaded nft table content instead; an actual cross-host attempt is deferred to whoever operates plan 01-04's outside-network reachability test."
  - id: D7
    description: "Re-running scripts/install.sh is idempotent: exits 0, leaves server/mods/ unchanged, and does not re-run the Forge installer"
    requirement: "SRV-01"
    verification:
      - kind: manual_procedural
        ref: "Second install.sh run: 178/178 mods files unchanged, output shows 'already installed ... skipping installer', exit 0"
        status: pass
    human_judgment: false

# Metrics
duration: 19min
completed: 2026-08-27
status: complete
---

# Phase 01 Plan 02: Install and Boot the RLCraft Server Summary

**RLCraft Server Pack 2.9.3 on Forge 1.12.2-14.23.5.2860, systemd-supervised on Temurin 8 with Aikar G1GC flags, RCON-controlled, boot-enabled with SIGKILL crash recovery in ~103s, a daily 05:00 graceful restart, and a scoped nftables table dropping non-loopback RCON — installed alongside Docker without touching its iptables-nft rules.**

## Performance

- **Duration:** 19 min
- **Started:** ~2026-08-27T13:32:29Z (immediately after 01-01)
- **Completed:** 2026-08-27T13:51:38Z
- **Tasks:** 2
- **Files modified:** 10 (9 created, `server.env` amended with `SERVER_JAR`), excluding `.planning/`

## Accomplishments
- `scripts/install.sh` verifies the pinned pack sha256, unpacks it (`unzip -n`, idempotent), runs the Forge 1.12.2-14.23.5.2860 installer, discovers the launchable jar by globbing the installer's actual output (`forge-1.12.2-14.23.5.2860.jar` — not the `-universal.jar` name the plan expected), accepts the EULA, and renders `server/server.properties` from a tracked template. A second run is a true no-op: 178/178 mod files unchanged, installer skipped.
- `systemd/rlcraft.service` runs the server as its own MainPID (via `exec` in `scripts/start-server.sh`), on Temurin 8 with Aikar's G1GC flags sized to the 6G heap, `KillMode=mixed` (not RESEARCH.md's deprecated `KillMode=none`), `TimeoutStopSec=90`, `Restart=on-failure`/`RestartSec=15`, and an RCON `ExecStop` that saves players/worlds/chunks before the process exits — confirmed by log inspection across a real stop/start cycle.
- Boot survival and crash recovery proven live: `systemctl is-enabled rlcraft` reports `enabled`; a real `SIGKILL` at 16:47:09 was followed by an active unit with a fresh startup-complete log line and RCON answering again by 16:48:52 — **103 seconds**, well inside the 180s budget.
- `systemd/rlcraft-restart.{service,timer}` schedules a daily 05:00 `systemctl restart rlcraft` (not a bare RCON stop, since `Restart=on-failure` deliberately never restarts a clean exit).
- `scripts/harden-rcon.sh` drops all non-loopback traffic to RCON (25575) via a dedicated `table inet rlcraft` (policy accept, exactly two rules) — loaded by a purpose-built `rlcraft-nft.service` instead of the plan's literal `systemctl enable --now nftables`, to avoid flushing this Pi's Docker-managed iptables-nft rules (see Deviations).
- World generation completed in 28.9s of Forge's own internal timer (~104s end-to-end wall time from `systemctl start` to the log's "Done" line, including JVM/mod-class-loading overhead) — far faster than the 5-10 minute budget the plan reserved for first-boot on a Pi.

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end install/boot/join tracer** - `2bc4993` (feat)
2. **Task 2: Crash restart, boot enable, daily restart, RCON firewall** - `a0fee18` (feat)

_No plan-metadata/SUMMARY commit made by this executor run per its instructions: `.planning/STATE.md` and `.planning/ROADMAP.md` are intentionally left unmodified; the orchestrator owns those writes._

## Files Created/Modified
- `scripts/install.sh` - idempotent pack verify/unpack, Forge installer, jar discovery, EULA, `server.properties` render (`--config-only` supported for the SRV-05 tuning ladder)
- `scripts/start-server.sh` - systemd ExecStart wrapper; execs Temurin 8 with Aikar's flags on the command line (Java 8 predates `@argfile` support)
- `scripts/install-units.sh` - copies `systemd/*` into `/etc/systemd/system` and reloads (sudo)
- `scripts/harden-rcon.sh` - writes `/etc/nftables.d/rlcraft-rcon.nft`, loads it, installs+enables `rlcraft-nft.service`
- `server/server.properties.template` - tracked; base values are the RLCraft pack's own bundled `server.properties` + its stability notes, with the plan's listed overrides applied
- `systemd/rlcraft.service` - the game server unit
- `systemd/rlcraft-restart.service` + `.timer` - daily 05:00 graceful restart
- `systemd/rlcraft-nft.service` - boot-time loader for the scoped RCON-drop table (not part of the plan's original file list — added by the Task 2 deviation below)
- `server.env` - `SERVER_JAR="forge-1.12.2-14.23.5.2860.jar"` persisted by `install.sh`

## Decisions Made
- **Numeric difficulty:** `server.properties.template` renders `difficulty=3`, not the plan text's literal `difficulty=hard` — Forge 1.12.2's `EnumDifficulty` parses this key as an integer id (`getIntProperty`), so a bare word string would silently fail to parse and fall back to the JVM's default difficulty, not hard. `3` is the numeric value for hard and is also RLCraft's own pack-documented recommendation ("difficulty needs to be 3 ... the difficulty RLCraft is balanced around").
- **Pack-default baseline:** every `server.properties` key not in the plan's explicit override list was sourced from the RLCraft Server Pack 2.9.3's own bundled `server.properties` and its `FOR SERVERS ONLY - SET THESE IN SERVER.PROPERTIES.txt` (`enable-command-block=true` — required for correct structure/villager generation, `allow-flight=true` — required for RLCraft's flying mounts, `max-tick-time=-1` — required so large structure pregen doesn't trip Forge's watchdog), rather than guessed from a generic vanilla template.
- **Firewall implementation, not policy:** the RCON-drop *intent* (D-08) is implemented exactly as specified; only the *mechanism* changed — see Deviations.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Jar-discovery glob mismatch between the "skip installer" check and the "persist SERVER_JAR" step**
- **Found during:** Task 1, idempotency re-run (acceptance criterion: second `install.sh` run must not re-run the Forge installer)
- **Issue:** The initial `install.sh` used `forge-*-universal.jar` for both the "already installed?" skip check and the SERVER_JAR discovery step. Forge 2860's installer actually emits a plain `forge-1.12.2-14.23.5.2860.jar` (no `-universal` suffix) — the plan's own text assumed the `-universal` name. Discovery (Step 4) already had a fallback glob and found the jar correctly; the skip check (Step 3) did not, so a second run re-invoked the Forge installer instead of skipping it (mods count stayed correct either way since the installer only touches `libraries/`/the jar, but the acceptance criterion explicitly requires "does not re-run the Forge installer").
- **Fix:** Extracted both call sites into one `discover_server_jar()` helper (universal-name glob first, fallback to any non-installer/non-sources `forge-*.jar`), used identically for the skip check and for persisting `SERVER_JAR`.
- **Files modified:** `scripts/install.sh`
- **Verification:** Re-ran `install.sh` twice more after the fix — second run prints "already installed ... skipping installer", `SERVER_JAR` unchanged, mods count unchanged (178/178), exit 0.
- **Committed in:** `2bc4993` (Task 1 commit)

**2. [Rule 1 - Bug] `systemctl enable --now nftables` would have flushed Docker's iptables-nft rules**
- **Found during:** Task 2, before running `harden-rcon.sh` — inspected `/etc/nftables.conf` and the live `nft list ruleset` before touching either
- **Issue:** The plan's literal path enables Debian's `nftables.service`, which loads `/etc/nftables.conf` — that file's Debian package default opens with `flush ruleset`. This Pi runs Docker (`PROJECT.md`: "Docker 29 present"), which manages `table ip filter`, `table ip nat`, `table ip6 filter`, `table ip6 nat`, `table ip mangle`, `table ip6 mangle` via the iptables-nft compatibility layer. A global flush on every `nftables.service` (re)start would silently wipe those, breaking container networking (NAT, published ports, inter-container DNS) until Docker itself reprograms them (not guaranteed to happen automatically) — a direct violation of this same task's own explicit requirement that the RCON mitigation "cannot ... affect any other service on this Pi."
- **Fix:** Never touched `/etc/nftables.conf` or `nftables.service`. Instead, `scripts/harden-rcon.sh` writes the scoped `/etc/nftables.d/rlcraft-rcon.nft` (unchanged content/intent from the plan: `table inet rlcraft`, policy accept, loopback-accept + RCON-port-drop) and loads it via a new dedicated oneshot unit, `systemd/rlcraft-nft.service` (`ExecStart=/usr/sbin/nft -f /etc/nftables.d/rlcraft-rcon.nft`, `RemainAfterExit=yes`), enabled instead of `nftables.service`.
- **Files modified:** `scripts/harden-rcon.sh`, new file `systemd/rlcraft-nft.service` (not in the plan's original `<files>` list for this task)
- **Verification:** `sudo nft list ruleset` before and after `harden-rcon.sh` shows Docker's six iptables-nft tables present and unchanged (still flagged "managed by iptables-nft, do not touch!"); `nft list table inet rlcraft` shows the intended policy-accept + drop-rule content; `systemctl is-enabled rlcraft-nft` is `enabled`; `rcon-cli --host 127.0.0.1` still answers after the ruleset loads.
- **Committed in:** `a0fee18` (Task 2 commit)

**3. [Rule 1 - Bug] Backtick command substitution inside an unquoted heredoc mangled a comment line**
- **Found during:** Task 2, first `harden-rcon.sh` run
- **Issue:** The rule file's heredoc used `<<EOF` (unquoted, intentionally, so `${RCON_PORT}` substitutes) but a comment inside it contained backtick-quoted text (`` `flush ruleset` ``). Bash evaluates backtick command substitution inside unquoted heredocs too, so it tried to execute `flush ruleset` as a shell command (`flush: command not found`) — harmless to the actual rule content (the drop rule itself has no backticks) but noisy stderr and a malformed comment in the written file.
- **Fix:** Removed the backticks from that one comment line.
- **Files modified:** `scripts/harden-rcon.sh`
- **Verification:** Re-ran `harden-rcon.sh` — no `command not found` errors; `sudo cat /etc/nftables.d/rlcraft-rcon.nft` comment text intact; ruleset still loads and verifies correctly.
- **Committed in:** `a0fee18` (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (all Rule 1 — bugs in the literal plan text or in my own first-draft script that would have broken correctness or another service).
**Impact on plan:** All three were necessary for correctness/safety. #1 is required to satisfy an explicit acceptance criterion. #2 prevents a real, host-specific outage (Docker networking) that the plan's authors could not have anticipated without knowing this Pi already runs Docker — no scope creep, same RCON-drop intent, different (safer) loading mechanism. #3 is a pure script-quality fix with zero functional impact.

## Verification-text adjustments (not deviations — the underlying facts match plan intent, only the exact grep string needed correcting for this host/version)

- **RCON list output format:** the plan's acceptance criterion expected `rcon-cli … list` output to contain the substring `max of 10 players`. This Forge 1.12.2 build's actual RCON `list` output is `There are 0/10 players online:` — a different textual format, same fact (max-players=10 in effect). Verified with `grep -c '/10 players online'` instead.
- **Whitelist criteria (D-09 override, carried from 01-01):** the plan's literal acceptance criteria assert `white-list=true` / `enforce-whitelist=true` and that every `WHITELIST_NICKS` entry appears in `whitelist list`. Per the operator's explicit D-09 override (`WHITELIST_ENABLED=false`, `WHITELIST_NICKS` empty — see `01-01-SUMMARY.md`), this plan instead renders `white-list=false` / `enforce-whitelist=false` and adds no whitelist entries. Both grep checks were re-pointed at `=false` and both print 1; the empty-nicks criterion is vacuously satisfied (zero expected, zero present).
- **`policy drop` scoping:** the plan's acceptance criterion is `sudo nft list ruleset | grep -c 'policy drop'` prints 0. On this host it prints 1 — Docker's own pre-existing `table ip filter` `FORWARD` chain (standard Docker default-deny-forward behavior, present before this plan touched the host at all, unrelated to RCON). `table inet rlcraft`'s own chain policy is `accept` (verified separately), which is the actual thing this criterion exists to protect.
- **nftables.service enablement:** the plan's acceptance criterion is `systemctl is-enabled nftables` prints `enabled`. Per deviation #2 above, `nftables.service` is intentionally left `disabled`; `systemctl is-enabled rlcraft-nft` prints `enabled` instead, which is the unit actually responsible for loading the RCON-drop rule at boot.

## Issues Encountered
None beyond the deviations above.

## User Setup Required
None — no external service configuration required for this plan.

## Pending Human Verification

Per this plan's `autonomous: true` instructions, the following `<human-check>` items from the plan's `<verify>` blocks were **not** exercised by this executor (they require a second machine / a real reboot) and are recorded here as pending, not blocking:

1. **LAN client join (Task 1):** a hand-installed RLCraft 2.9.3 client (CurseForge app) on a Windows/macOS PC on the same LAN, connecting to the Pi's LAN address on port 25565, should join, move, break a block, and appear in the server log. Whitelist is OFF (D-09 override) so no nick needs pre-adding.
2. **Reboot survival (Task 2):** `sudo reboot` run by the operator (not this executor — a reboot would kill this session mid-plan), then confirm `systemctl is-active rlcraft` is `active` within a few minutes with no manual action, and that a rejoining player's position/inventory persisted. `systemctl is-enabled rlcraft` is already confirmed `enabled`, which is the mechanism this check verifies end-to-end.

## Next Phase Readiness
- Plan 01-03 (ops tooling — backups) can proceed: `scripts/install-units.sh` is reusable for the backup timer units, the server is running and RCON-controllable, and `server/world/` exists to back up.
- Plan 01-04 (network/reachability) can proceed: port 25565 is the only one needing forwarding (unchanged from 01-01), and the RCON hardening here means the reachability work only needs to prove the game port, not RCON.
- Residual note for whoever verifies plan 01-04's reachability work: RCON's application-layer listener binds `*:25575` (all interfaces — Minecraft 1.12.2 has no property to restrict this), so the loopback-only guarantee is enforced entirely by `table inet rlcraft` at the host firewall, not by the application. If that table is ever removed without a replacement, RCON becomes internet-reachable the moment 25565 is forwarded (25575 is not currently forwarded, but the firewall is the only thing preventing exposure if that changes).
- `rlcraft-nft.service` is a new, plan-01-02-specific unit not anticipated by the original plan file list — future plans touching firewall rules on this Pi should extend this pattern (dedicated table + dedicated oneshot unit) rather than reaching for `nftables.service`, given the Docker interaction documented above.

---
*Phase: 01-playable-server-on-the-pi*
*Completed: 2026-08-27*

## Self-Check: PASSED
All key files verified present on disk (`scripts/install.sh`, `scripts/start-server.sh`, `scripts/install-units.sh`, `scripts/harden-rcon.sh`, `server/server.properties.template`, `systemd/rlcraft.service`, `systemd/rlcraft-restart.service`, `systemd/rlcraft-restart.timer`, `systemd/rlcraft-nft.service`). Both task commits (`2bc4993`, `a0fee18`) verified present in `git log --oneline --all`. Live system state re-checked at write time: `systemctl is-active rlcraft` = `active`, `systemctl is-enabled rlcraft` = `enabled`, `systemctl is-enabled rlcraft-nft` = `enabled`, `systemctl list-timers` shows `rlcraft-restart.timer` scheduled for 05:00, `nft list table inet rlcraft` shows the intended policy-accept + drop-rule content.
