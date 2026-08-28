---
phase: 01-playable-server-on-the-pi
verified: 2026-08-28T09:00:00Z
status: human_needed
score: 10/10 must-haves verified (presence + live automated evidence); 4 items require human confirmation
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Outside-network friend join (ROADMAP success criterion 1 / SRV-04)"
    expected: "A friend who is NOT on the home network installs the RLCraft 2.9.3 client per docs/CLIENT-SETUP.md, adds mc.campfire.pub:25565 under Multiplayer, connects, and can move/interact in the world."
    why_human: "Requires a real client install on a machine outside the home network and a real join — nothing on the Pi can simulate this. Automated outside-in reachability (DNS resolution + api.mcsrvstat.us online=true) is already proven live, but a passing third-party ping is not the same as a real client actually connecting and playing."
  - test: "Pi reboot survival (ROADMAP success criterion 2 / SRV-02)"
    expected: "After `sudo reboot` (run by the operator, not this verifier/executor — a reboot would kill any live session), within a few minutes `systemctl is-active rlcraft` reports `active` with no operator action taken, and a rejoining player's position/inventory persisted."
    why_human: "systemctl is-enabled rlcraft reports enabled and a live SIGKILL-crash recovery was already proven automated (103s, see 01-02-SUMMARY.md), which demonstrates the same Restart=on-failure/boot-enable mechanism reboot survival depends on — but an actual reboot has never been performed against this live production instance, by design (the executor's own session runs on this Pi and a reboot would terminate it mid-task)."
  - test: "Three-player, twenty-minute TPS load test (ROADMAP success criterion 3 / SRV-05)"
    expected: "With three real people online simultaneously, actually playing (not standing at spawn) for 20 minutes, `bash scripts/tps-log.sh 20m 30s` reports median Overall TPS >= 15."
    why_human: "Requires three real people playing at once — the only TPS evidence gathered so far is a solo 5-sample baseline (20.000 min/median/mean, server/logs/tps-2026-08-27.csv and tps-2026-08-28.csv), which the sampler's own output explicitly flags as 'not-evidence' for SRV-05 since max players observed was 0. The tuning ladder (HEAP 6G->8G, then VIEW_DISTANCE 8->6) is written in server.env.example and its config-only re-render path (`scripts/install.sh --config-only`) is proven runnable, ready to apply if the real test comes in under 15."
  - test: "Restored-world fidelity in-game (ROADMAP success criterion 4 / SRV-03)"
    expected: "After joining from a real client: spawn position matches where the player last logged off, inventory matches, a chest the player left items in still holds them, and walking back through an existing Nether portal leads to the same generated Nether, not fresh terrain."
    why_human: "The restore mechanism itself is proven by an automated round trip (a /worldborder value set before a backup was archived, overwritten, then correctly restored by scripts/restore.sh — see 01-03-SUMMARY.md), and a pre-restore safety archive is always taken first. But that proves the archive/restore machinery moves world state correctly, not that a human's actual position/inventory/chest contents/Nether survive the experience of being restored — RESEARCH.md Pitfall 4 specifically calls out that a backup missing dimension folders looks fine mechanically but produces a fresh-looking Nether, which only a human walking through the portal can catch."
---

# Phase 1: Playable Server on the Pi — Verification Report

**Phase Goal:** Friends can play RLCraft together on the Pi over the internet using a hand-installed client, and the server keeps itself alive
**Verified:** 2026-08-28T09:00 (live checks run directly against the running Pi)
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

All automatable groundwork for the phase goal is built, live, and independently re-checked in this verification pass (not merely re-stated from SUMMARY.md claims). The four things nothing on the Pi can prove by itself — a real outside join, a real reboot, a real three-player load test, and a human eyeballing their own inventory/position/Nether after a restore — remain open. Nothing was fabricated to close them; each is listed under Human Verification with the exact procedure the operator must run and the automated proof already backing the mechanism.

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | RLCraft Server Pack 2.9.3 (Forge 1.12.2-14.23.5.2860) runs on the Pi under Java 8 (Temurin), not system Java 25 | ✓ VERIFIED | Live: `systemctl is-active rlcraft` = `active`; MainPID 2995864's `/proc/<pid>/cmdline` first field = `/opt/temurin-8/jdk8u504-b01/bin/java`, matching `JAVA8_BIN` in `server.env`; `java -version` (system default) still reports `25.0.3`; `server/server.properties` shows `online-mode=false` (offline-mode, as required) |
| 2 | Server starts on boot and restarts on crash (systemd `Restart=on-failure`) | ✓ VERIFIED (mechanism) / ⚠️ human_needed (real reboot) | Live: `systemctl is-enabled rlcraft` = `enabled`. 01-02-SUMMARY.md documents a real `SIGKILL` sent to the live process recovering unattended in 103s (well under the 180s budget) — the crash-restart half is proven live, not just asserted. Real reboot survival (the boot half) has never been exercised; see Human Verification |
| 3 | World is backed up on a schedule with rotation, and a restore has been tested once | ✓ VERIFIED | Live: `~/rlcraft-backups/` (mode 700) holds 11 rotated `world-*.tar.zst` archives with timestamps at 15:00/21:00/03:00/09:00 (confirming the 6-hourly `rlcraft-backup.timer`, `is-active`+`is-enabled` both `enabled`/`active`), plus 5 `pre-restore-*.tar.zst` safety archives never pruned. 01-03-SUMMARY.md documents rotation proven by two `BACKUP_KEEP=1` runs leaving exactly one archive, and a restore round-trip proven live via a `/worldborder` value that survived archive→overwrite→restore→live-server |
| 4 | Server is reachable from the internet on TCP 25565 via domain; CGNAT verified absent | ✓ VERIFIED | Live, re-run in this verification pass: `dig +short mc.campfire.pub` = `91.193.195.130` = `curl ifconfig.me`; `curl https://api.mcsrvstat.us/2/mc.campfire.pub` (third-party, outside-the-LAN vantage point) reports `"online":true`, lists the live mod list and `"players":{"online":0,"max":10}` — a CGNAT connection could not produce this result, closing the verdict recorded as `confirmed-absent-01-04` in `server.env` |
| 5 | Server holds >=15 TPS with 3 concurrent players (measured, not guessed) | ⚠️ instrumentation VERIFIED / ⚠️ human_needed (real measurement) | Live: `scripts/tps-log.sh` runs and parses correctly (re-run in this session produced a fresh 3-sample CSV); solo baseline 20.000 min/median/mean recorded (`server/logs/tps-2026-08-27.csv`), explicitly flagged by the script itself as not-evidence for SRV-05 since 0 players were online. The tuning ladder (HEAP 6G->8G, then VIEW_DISTANCE->6) is written in `server.env.example`, and `scripts/install.sh --config-only` was proven to re-render config without touching mods or re-running the Forge installer. The actual 3-player 20-minute measurement has not been performed |
| 6 | A friend outside the home network joins by domain with a hand-installed client and plays | ⚠️ human_needed | `docs/CLIENT-SETUP.md` exists, names the real domain, pins 2.9.3, covers RAM/port/whitelist status, contains zero raw-IP literals (verified: `grep -REc` dotted-quad = 0). Outside-in reachability is proven (truth 4). The actual human join has not been performed |
| 7 | RCON reachable only from the Pi itself | ✓ VERIFIED | Live: `sudo nft list table inet rlcraft` shows `iif "lo" accept` then `tcp dport 25575 drop`, chain policy `accept` (scoped table, no default-drop introduced elsewhere — `nft list ruleset \| grep -c 'policy drop'` = 1, which is Docker's own pre-existing FORWARD chain, unrelated); `rlcraft.service` now `After=rlcraft-nft.service`/`Wants=rlcraft-nft.service` (WR-02 fix, confirmed live in the installed unit file) so the drop rule loads before the listener at boot |
| 8 | RCON password is not exposed via argv/`ps`/`/proc` | ✓ VERIFIED | CR-01 fix confirmed live in `scripts/backup.sh`'s, `scripts/restore.sh`'s and `scripts/tps-log.sh`'s `rcon()` helpers (env-var passthrough, no `--password` flag) and in the installed `rlcraft.service`'s `ExecStop=-/usr/local/bin/rcon-cli stop` (no password on the command line) |
| 9 | Restore refuses to run as root, and backup/restore cannot race each other | ✓ VERIFIED | WR-07 (`id -un != asphacean` guard) and WR-01 (`flock -n 9` in both scripts) both present and live in `scripts/restore.sh`/`scripts/backup.sh` on disk |
| 10 | Whitelist gap and access-open decision are operator-directed and documented, not silent | ✓ VERIFIED (documented deviation, per task instructions not a gap) | `WHITELIST_ENABLED=false` in `server.env`, `white-list=false`/`enforce-whitelist=false` live in `server/server.properties`; documented as D-09 override in 01-01-SUMMARY.md, carried through every later plan's SUMMARY, and named explicitly in `docs/CLIENT-SETUP.md` §5 and the code review's CR-02 (skipped deliberately per 01-REVIEW-FIX.md, "deliberate operator decision, not a defect to auto-fix") |

**Score:** 10/10 automatable truths verified live; 4 items (embedded above as sub-notes on truths 2, 5, 6, and the restore-fidelity check) require human execution and are listed in full under Human Verification.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `scripts/preflight.sh`, `scripts/fetch-pack.sh`, `scripts/cgnat-check.sh` | Host bootstrap, pack acquisition, CGNAT check | ✓ VERIFIED | All three pass `bash -n`; git history shows atomic commits; `server.env` shows all keys populated (`JAVA8_BIN`, `PACK_SHA256`, `CGNAT_VERDICT`) |
| `scripts/install.sh`, `scripts/start-server.sh`, `systemd/rlcraft.service` | Idempotent install, ExecStart wrapper, supervised unit | ✓ VERIFIED | Live process confirmed running the discovered jar under Temurin 8; unit shows `Restart=on-failure`, `RestartSec=15`, `KillMode=mixed`, `TimeoutStopSec=90`, `After=rlcraft-nft.service` |
| `scripts/backup.sh`, `scripts/restore.sh`, `systemd/rlcraft-backup.{service,timer}` | Six-hourly backup + tested restore | ✓ VERIFIED | Live timer active/enabled, 11 rotated archives on disk with 6h-spaced timestamps, restore round-trip documented and re-confirmed live (script syntax, flock, root guard) |
| `scripts/reachability.sh`, `scripts/tps-log.sh`, `docs/CLIENT-SETUP.md` | Outside-in check, TPS sampler, client doc | ✓ VERIFIED | All three exist, syntax-clean, and their outputs were independently re-produced live in this verification (mcsrvstat PASS, fresh TPS CSV, client doc content grep-checked) |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `systemd/rlcraft.service` | `scripts/start-server.sh` | `ExecStart` | ✓ WIRED | Live MainPID cmdline matches `JAVA8_BIN` — the unit really launches the wrapper which execs Temurin 8 |
| `scripts/start-server.sh` | `server.env` | sources `JAVA8_BIN`/`HEAP`/`SERVER_JAR` | ✓ WIRED | Confirmed by matching live process to `server.env` values |
| `systemd/rlcraft.service` | `rcon-cli` | `ExecStop` | ✓ WIRED | `ExecStop=-/usr/local/bin/rcon-cli stop`, env-sourced credentials (CR-01 fix live) |
| `systemd/rlcraft-restart.timer` | `rlcraft-restart.service` | daily 05:00 `systemctl restart rlcraft` | ✓ WIRED | `systemctl list-timers` shows next elapse `2026-08-29 05:00:00`, last run `2026-08-28 05:00:04` (~7h ago — already fired once live) |
| `scripts/backup.sh` | `rcon-cli` | `save-off`/`save-all` around the tar | ✓ WIRED | 01-03-SUMMARY.md documents live trap-path exercise; `backup.log` present with `save-on ok` sentinels |
| `systemd/rlcraft-backup.timer` | `rlcraft-backup.service` | six-hourly trigger | ✓ WIRED | Live: 11 archives at 6h-spaced timestamps (15:00, 21:00, 03:00, 09:00...), next elapse in ~6h |
| `scripts/restore.sh` | `systemd/rlcraft.service` | stop before extract, start after | ✓ WIRED | 01-03-SUMMARY.md documents a live stop-confirm-extract-start cycle with journalctl-polled startup detection |
| `scripts/reachability.sh` | `server.env` | reads `DOMAIN`, compares against public IP | ✓ WIRED | Live re-run: `DOMAIN=mc.campfire.pub` resolves to the same IP `curl ifconfig.me` reports |
| `docs/CLIENT-SETUP.md` | `server.env` | documents `DOMAIN`, never a raw IP | ✓ WIRED | Grep-confirmed: contains `mc.campfire.pub`, `25565`, `2.9.3`; zero dotted-quad literals |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|--------------|-------------|--------------|--------|----------|
| SRV-01 | 01-01, 01-02 | Server pack runs under Java 8 offline-mode | ✓ SATISFIED | Live process + config confirmed |
| SRV-02 | 01-02 | Boot start + crash restart | ✓ SATISFIED (crash) / ? NEEDS HUMAN (reboot) | `is-enabled`=enabled, live SIGKILL recovery documented; real reboot pending |
| SRV-03 | 01-03 | Scheduled backup + tested restore | ✓ SATISFIED (mechanism) / ? NEEDS HUMAN (in-game fidelity) | Live timer + archives + round-trip proof; human in-game check pending |
| SRV-04 | 01-01, 01-04 | Internet-reachable by domain, CGNAT absent | ✓ SATISFIED (reachability) / ? NEEDS HUMAN (real friend join) | Live DNS + third-party check PASS; human join pending |
| SRV-05 | 01-04 | >=15 TPS with 3 players, measured | ⚠️ instrumentation SATISFIED / ? NEEDS HUMAN (real measurement) | Sampler proven, solo baseline recorded and explicitly flagged not-evidence; 3-player test pending |

No orphaned requirements: REQUIREMENTS.md's Phase 1 traceability table lists exactly SRV-01..SRV-05, and all five appear in the `requirements:` frontmatter across the four PLAN files (01-01: SRV-01,SRV-04; 01-02: SRV-01,SRV-02; 01-03: SRV-03; 01-04: SRV-04,SRV-05).

### Anti-Patterns Found

None (blocker-level). `grep -rn -E "TBD|FIXME|XXX"` across `scripts/`, `systemd/`, `server/server.properties.template`, `docs/` returned only two false positives (`mktemp ... XXXXXX.tar.gz` template placeholders, not debt markers). No `TODO`/`HACK`/`PLACEHOLDER` markers, no empty stub implementations, no hardcoded-empty data flowing to output.

### Code Review Findings (01-REVIEW.md / 01-REVIEW-FIX.md)

2 critical + 8 warning findings were raised by a prior code review pass. 9 of 10 (critical+warning) were fixed and are confirmed live in this verification: CR-01 (RCON password off argv — confirmed in `rcon()` helpers and `ExecStop`), WR-01 (flock guards — confirmed), WR-02 (boot ordering — confirmed in the installed unit file), WR-03 (Temurin tarball checksum), WR-04 (mktemp), WR-05 (tps-log arg-parse abort), WR-06 (dnsutils installed, `dig` present), WR-07 (restore.sh root guard — confirmed), WR-08 (cgnat-check.sh warns instead of silently dropping). CR-02 (`online-mode=false` + open access enables username/UUID impersonation) was explicitly and deliberately left unfixed — it is the direct code-level consequence of the operator's own D-09 whitelist-override decision, documented as an accepted risk in both `01-REVIEW-FIX.md` and `docs/CLIENT-SETUP.md` §5, not a silently-missed defect. Per this verification's brief, this is treated as intentional, not a gap.

### Human Verification Required

See YAML frontmatter `human_verification` for the four items in full (outside-network friend join, Pi reboot, three-player 20-minute TPS test, restored-world in-game fidelity). Each is backed by proven automated mechanism; none can be closed without a human performing the real-world action.

### Gaps Summary

No blocking gaps. Every artifact, script, systemd unit, and key link this phase's plans specified exists, is substantively implemented (not a stub), is wired to its dependents, and was independently re-confirmed live against the running Pi in this verification pass (not merely trusted from SUMMARY.md text). The nine actionable code-review findings that were fixed are confirmed present in the live, currently-installed configuration — not just committed to git. The one skipped review finding (CR-02) is a direct, documented consequence of an explicit operator decision (D-09 whitelist override), not an oversight. The phase's only remaining work is the four measurements/actions that categorically require a human: a real outside-network client join, a real Pi reboot, a real three-player twenty-minute play session with TPS sampling, and a human eyeballing their restored position/inventory/Nether in-game. All four have exact, ready-to-run procedures recorded above and in the phase's own SUMMARY files.

---

_Verified: 2026-08-28T09:00Z_
_Verifier: Claude (gsd-verifier)_
