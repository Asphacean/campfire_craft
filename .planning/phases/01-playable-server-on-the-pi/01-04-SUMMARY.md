---
phase: 01-playable-server-on-the-pi
plan: 04
subsystem: infra
tags: [dns, ddns, reachability, rcon, tps, curseforge, bash]

# Dependency graph
requires:
  - phase: 01-playable-server-on-the-pi (01-01)
    provides: "server.env with DOMAIN, WAN_IP_KIND=static, PUBLIC_IP_AT_SETUP, WHITELIST_ENABLED=false (D-09 override)"
  - phase: 01-playable-server-on-the-pi (01-02)
    provides: "rlcraft.service running under systemd, nftables RCON drop rule"
provides:
  - "Outside-in reachability proof for SRV-04: DNS resolves and a third-party vantage point (api.mcsrvstat.us) confirms TCP 25565 online"
  - "CGNAT verdict finalized (confirmed-absent-01-04) — the deferred stage-2 check from 01-01 is closed out"
  - "RCON forge-tps sampler (scripts/tps-log.sh) with a solo TPS baseline recorded"
  - "SRV-05 tuning ladder documented in server.env.example, its second rung (install.sh --config-only) proven runnable"
  - "docs/CLIENT-SETUP.md — the hand-install client path friends use until Phase 4's launcher"
affects: [01-05, Phase-2-auth, Phase-4-launcher]

# Actuals (#2632)
actuals:
  tokens: 3550
  tasks: 3
  commits: 2

# Tech tracking
tech-stack:
  added: [api.mcsrvstat.us (third-party reachability oracle), dig, python3 statistics module]
  patterns:
    - "Outside-in verification: a probe issued from the Pi cannot prove internet reachability (hairpin NAT can succeed locally while the outside world gets nothing) — a third-party vantage point is the only trustworthy signal"
    - "server.env CGNAT_VERDICT / PUBLIC_IP_AT_SETUP move through named non-enum states (likely-clear-unconfirmed -> confirmed-absent-01-04) with the reasoning kept in the adjacent comment, since the source script's own enum has no slot for 'confirmed later by a different plan'"

key-files:
  created:
    - scripts/reachability.sh
    - scripts/tps-log.sh
    - docs/CLIENT-SETUP.md
  modified:
    - server.env.example (SRV-05 tuning ladder comment block above HEAP/VIEW_DISTANCE)
    - server.env (untracked — CGNAT_VERDICT finalized; DDNS_* left empty, WAN_IP_KIND=static)

key-decisions:
  - "Operator configured the router port forward manually (TCP 25565 -> 192.168.31.247 on eth0) and created the mc.campfire.pub A record by hand in Namecheap rather than supplying a Cloudflare API token — no DDNS updater package was installed, matching the plan's static-IP branch (D-07). The A record is operator-maintained by hand going forward."
  - "CGNAT_VERDICT finalized from likely-clear-unconfirmed (01-01) to confirmed-absent-01-04: api.mcsrvstat.us, a vantage point outside the home network, reports the server online on TCP 25565, which is not possible if the connection were behind CGNAT. Recorded only in server.env (untracked) with the reasoning in an adjacent comment."
  - "Whitelist-refusal human-check from this plan's <verify> block does not apply: WHITELIST_ENABLED=false is a standing operator decision recorded in 01-01-SUMMARY (D-09 override) — access is open to anyone who reaches the server until Phase 2's token auth. Not re-litigated here."

patterns-established:
  - "The reachability script's retry loop (up to 5 minutes, 15s poll) tolerates DNS propagation lag and api.mcsrvstat.us's few-minute response cache rather than failing on the first attempt — this run needed zero retries because DNS had already fully propagated by execution time."

requirements-completed: []

coverage:
  - id: D1
    description: "Outside-in reachability: DOMAIN resolves to the current public IP and a third-party vantage point confirms TCP 25565 online"
    requirement: "SRV-04"
    verification:
      - kind: other
        ref: "bash scripts/reachability.sh — VERDICT: PASS, mc.campfire.pub -> 91.193.195.130 (matches public IP), api.mcsrvstat.us online=true"
        status: pass
    human_judgment: false
  - id: D2
    description: "A friend outside the home network installs the hand-install client, adds the server by domain, and joins and plays"
    requirement: "SRV-04"
    verification: []
    human_judgment: true
    rationale: "Requires a real client install and a real join by a person outside the network — nothing on the Pi can simulate or assert this. Not yet performed; procedure recorded below for the operator to run and report back."
  - id: D3
    description: "RCON forge-tps sampler produces a CSV and a numeric PASS/FAIL verdict against the 15 TPS threshold; solo baseline recorded"
    requirement: "SRV-05"
    verification:
      - kind: other
        ref: "bash scripts/tps-log.sh 2m 30s — server/logs/tps-2026-08-27.csv, 5 samples, min/median/mean 20.000/20.000/20.000, flagged not-evidence (max players observed 0)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Overall TPS with three concurrent players over twenty minutes, evaluated against the 15 TPS threshold, with the tuning ladder applied if it lands under 15"
    requirement: "SRV-05"
    verification: []
    human_judgment: true
    rationale: "Requires three real people playing simultaneously for twenty minutes — this is the measurement the plan explicitly states nothing on the Pi can fake. Not yet performed; procedure recorded below."
  - id: D5
    description: "Client setup document (docs/CLIENT-SETUP.md) names the domain and no raw IP, pins RLCraft 2.9.3, and covers the whitelist status"
    verification:
      - kind: other
        ref: "grep -REc dotted-quad docs/CLIENT-SETUP.md -> 0; grep 2.9.3, mc.campfire.pub, 25565 all present"
        status: pass
    human_judgment: false

# Metrics
duration: 15min (this resumed session; router/DNS checkpoint itself spanned roughly 15h of operator/wall-clock wait between the 2026-08-27 checkpoint and this 2026-08-28 resume)
completed: 2026-08-28
status: complete
---

# Phase 01 Plan 04: Network Reachability and Load Measurement Summary

**Outside-in reachability proven live (DNS + third-party api.mcsrvstat.us both confirm mc.campfire.pub:25565 reachable from outside the home network, closing out the CGNAT question), TPS sampler and tuning ladder built with a 20.0 TPS solo baseline recorded, and a hand-install client doc shipped — the two human-only measurements (a real outside join, a real 3-player 20-minute load test) remain open and are documented below with exact procedures.**

## Performance

- **Duration:** ~15 min of active executor work in this resumed session (Task 2/3 scripting was completed and committed in a prior session on 2026-08-27; this session verified live reachability now that the operator finished the router/DNS checkpoint, and closed out the plan)
- **Started (this session):** 2026-08-28T08:30Z (approx)
- **Completed:** 2026-08-28T08:43Z
- **Tasks:** 3 (1 checkpoint resolved + verified this session; 2 scripted tasks previously committed, re-verified live this session)
- **Files modified:** 4 tracked (`scripts/reachability.sh`, `scripts/tps-log.sh`, `docs/CLIENT-SETUP.md`, `server.env.example`) + `server.env` (untracked)

## Accomplishments

- **Task 1 (router + DNS checkpoint) resolved by the operator:** TCP 25565 forwarded to `192.168.31.247` (this Pi's current eth0 address, confirmed live via `ip addr`) on eth0; a DHCP reservation for MAC `88:a2:9e:33:fb:1c` was requested on the router (not independently confirmed applied — see Known Risks). The `mc.campfire.pub` A record was created manually in Namecheap (not Cloudflare, no API token supplied), so no DDNS updater was installed — matches the plan's static-IP branch, and `server.env`'s `WAN_IP_KIND=static` / empty `DDNS_*` fields already reflected this from 01-01. Public IP confirmed still `91.193.195.130`, still static.
- **Other router forwards/UPnP rules: not reported by the operator.** This is recorded as an open gap against T-04-01, not as a confirmed "no other rules exist" — see Threat Flags below.
- **`bash scripts/reachability.sh` PASSED on the first run, no retries needed:** DNS had already fully propagated (`dig +short mc.campfire.pub` returns `91.193.195.130` from the system resolver, `@1.1.1.1`, and `@8.8.8.8` alike) and `api.mcsrvstat.us` reports `online=true` (version `1.12.2`, 0 players) — an outside-the-LAN vantage point confirms the game port is actually reachable through the router, not just resolvable in DNS.
- **CGNAT verdict closed out:** `server.env`'s `CGNAT_VERDICT` updated from 01-01's `likely-clear-unconfirmed` to `confirmed-absent-01-04` — a third party outside the home network reaching the server on 25565 is not possible under CGNAT, so this is now proven rather than inferred.
- All other Task 2 acceptance criteria re-verified live this session: `dig +short` equals `curl ifconfig.me` (both `91.193.195.130`); zero dotted-quad IP literals in `docs/` or `scripts/reachability.sh`; `sudo nft list table inet rlcraft` still shows the `tcp dport 25575 drop` rule after all the network changes.
- **Task 3 (TPS sampler, tuning ladder, client doc) — committed in a prior session (`623d7f2`), re-verified this session:** `scripts/tps-log.sh` produced `server/logs/tps-2026-08-27.csv` with a 5-sample solo baseline of 20.000 min/median/mean Overall TPS (flagged not-evidence for SRV-05 since max players observed was 0, as the script itself notes). `bash scripts/install.sh --config-only` re-run this session, exits 0, rewrites `server/server.properties` without touching the Forge installer or `server/mods/` — the ladder's second rung is proven runnable.
- `docs/CLIENT-SETUP.md` names the real domain (`mc.campfire.pub`), pins RLCraft 2.9.3, covers CurseForge app install, 6 GB RAM allocation, port 25565, current no-whitelist status (D-09 override), and where the client log lives.

## Task Commits

Each task was committed atomically:

1. **Task 1: Router port forward, DNS A record, current forwarding rule list** — no code commit (the only file this task writes, `server.env`, is untracked/gitignored; operator facts recorded in this SUMMARY instead)
2. **Task 2: DNS that follows the public IP, outside-in reachability proof** — `bf823da` (feat) — committed in a prior session while DNS had not yet propagated; live-verified PASS this session with no code changes needed
3. **Task 3: TPS sampler, tuning ladder, client setup doc** — `623d7f2` (feat) — committed in a prior session; re-verified this session with no code changes needed

**Plan metadata:** _pending — this SUMMARY commit follows immediately_

_Note: Tasks 2 and 3 were fully implemented and committed on 2026-08-27 (per the prior session's completed_tasks handoff); this resumed session's work was verifying them live against the now-completed router/DNS checkpoint and closing out the plan._

## Files Created/Modified
- `scripts/reachability.sh` - DNS + outside-in reachability check for SRV-04 (retries up to 5 min for DNS propagation and mcsrvstat.us cache staleness)
- `scripts/tps-log.sh` - RCON forge-tps sampler, CSV log, min/median/mean summary with PASS/FAIL verdict against 15 TPS
- `docs/CLIENT-SETUP.md` - hand-install client path (CurseForge app, RLCraft 2.9.3, domain, no-whitelist status)
- `server.env.example` - SRV-05 tuning ladder comment block above HEAP/VIEW_DISTANCE
- `server.env` (untracked) - `CGNAT_VERDICT` finalized to `confirmed-absent-01-04`; `WAN_IP_KIND=static`, `DDNS_PROVIDER`/`DDNS_API_TOKEN`/`DDNS_ZONE_ID` left empty (Namecheap manual A record, no Cloudflare token supplied)

## Decisions Made
- No DDNS updater installed: the connection is static and the operator maintains the A record by hand in Namecheap. This matches the plan's explicit static-IP branch (D-07) — nothing was skipped, this is the designed path for a static WAN address.
- CGNAT_VERDICT finalized to a custom value (`confirmed-absent-01-04`) outside `cgnat-check.sh`'s own three-value enum, continuing the same deliberate deviation 01-01 recorded, now with the outside-network proof that plan explicitly deferred to.
- The plan's whitelist-refusal human-check is not applicable under the standing D-09 override (`WHITELIST_ENABLED=false`) — not treated as a new deviation, just a restatement of the 01-01 decision in this plan's context.

## Deviations from Plan

None beyond what 01-01 already recorded (D-09 whitelist override, custom CGNAT verdict value) — this plan's own tasks executed as written. No Rule 1/2/3 auto-fixes were needed this session.

## Issues Encountered

None. `scripts/reachability.sh` passed on its first invocation with zero retries — by the time this session resumed, DNS had already propagated globally (confirmed against the system resolver, `1.1.1.1`, and `8.8.8.8` independently) and `api.mcsrvstat.us` returned a fresh `online=true` with no stale-cache retry needed.

## Known Risks (not blockers)

- **DHCP reservation requested, not independently confirmed applied.** The operator reported requesting a reservation for MAC `88:a2:9e:33:fb:1c`, but this Pi's `eth0` address is still reported as `dynamic` by `ip addr` (expected — a reservation pins a DHCP-assigned lease, it does not make the interface config itself static). If the reservation has not actually taken effect on the router, a future lease renewal could hand the Pi a different LAN address than `192.168.31.247` and silently break the port forward, exactly as the Task 1 instructions warned. Recommended follow-up: operator should confirm in the router's DHCP/reservation page that the reservation is saved and bound to that MAC, not just requested.
- **Other router forwarding/UPnP rules: not reported by the operator.** Task 1 asked the operator to list every other forward or UPnP rule the router shows, specifically because UPnP is the realistic path by which RCON (25575) or something else could end up exposed despite everything configured on the Pi. The operator's reply did not include this list. This is recorded as an **open gap**, not as a confirmed "no other rules" — see Threat Flags below. The Pi-side nftables RCON drop (`tcp dport 25575 drop`, reconfirmed present this session) remains the second, independent layer of defense regardless.

## Pending Human Verification (SRV-04, SRV-05 — not yet performed, not fabricated)

Both of the following are the human-only measurements the plan objective calls out as impossible to assert from the Pi alone. They are recorded here exactly as open, with the procedure the operator should run:

1. **Outside join (SRV-04, D2 above).** Have a friend who is **not** on the home network install the client per `docs/CLIENT-SETUP.md` (CurseForge app, RLCraft 2.9.3, ≥6 GB RAM), add `mc.campfire.pub:25565` under Multiplayer, and connect. Since `WHITELIST_ENABLED=false` (D-09 override), no nick needs to be added first — anyone reaching the server can join. Expected: they join and can play. Report back: whether the join worked, their ping, and any error text if it did not.
2. **Three-player, twenty-minute load test (SRV-05, D4 above).** With three people online simultaneously and actually playing (not standing at spawn — chunk generation and mob AI are what load this CPU), run on the Pi: `bash scripts/tps-log.sh 20m 30s`. When it finishes, report its final summary block (sample count, max players observed, min/median/mean Overall TPS, and the PASS/FAIL line against the 15 TPS threshold). If it reports FAIL, the tuning ladder is already written in `server.env.example` directly above `HEAP`: raise `HEAP` to `8G` and `sudo systemctl restart rlcraft`, retest; if still short, set `VIEW_DISTANCE=6`, re-render with `bash scripts/install.sh --config-only` (proven working this session), restart, and retest. Report the failing numbers rather than silently retuning, so any retune is tracked.

Both `REQUIREMENTS.md` entries for SRV-04 and SRV-05 remain `Pending` until these two human checks return their results — everything automatable ahead of them is now built and proven.

## Threat Flags

| Flag | File | Description |
|------|------|--------------|
| threat_flag: T-04-01 gap | router (operator-owned) | Task 1 asked the operator to list every other port-forward/UPnP rule the router currently shows, specifically to catch UPnP quietly opening something else (e.g., RCON 25575). The operator's reply covered only the intentional 25565 forward and the DHCP reservation request — no list of other rules was supplied. This is an **open gap in the mitigation**, not a clean bill of health; the Pi-side nftables RCON drop is the compensating control that holds regardless, but the router-side picture is incomplete. |

## User Setup Required

None - no external service configuration required beyond what the operator already completed at the Task 1 checkpoint (router port forward, Namecheap A record).

## Next Phase Readiness

- SRV-04's automatable half is proven: DNS resolves, CGNAT is confirmed absent, and an outside-the-LAN vantage point confirms the game port reachable. What remains is the human outside-join test (procedure above).
- SRV-05's instrumentation is proven: the sampler runs, parses `forge tps` robustly (including the no-newline-before-Overall quirk documented in the script), and produces a CSV plus a numeric verdict. What remains is the real three-player twenty-minute run (procedure above) — the tuning ladder is written and its second rung's config re-render is proven runnable in advance.
- Friends have `docs/CLIENT-SETUP.md` to follow independently of the operator once whitelisted (currently: whitelist is open, so no operator action is needed to let a friend in).
- Follow-up for the operator, not blocking this plan: confirm the DHCP reservation actually took effect, and supply the full router forwarding/UPnP rule list that Task 1 asked for but did not receive.

---
*Phase: 01-playable-server-on-the-pi*
*Completed: 2026-08-28*
