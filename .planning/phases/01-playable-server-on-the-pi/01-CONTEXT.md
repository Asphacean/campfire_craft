# Phase 1: Playable Server on the Pi - Context

**Gathered:** 2026-08-27
**Status:** Ready for planning

<domain>
## Phase Boundary

Stand up the RLCraft 2.9.3 (Forge 1.12.2-14.23.5.2860) dedicated server on this Raspberry Pi 5 so friends outside the home network can join by domain with a hand-installed client. Server survives reboots/crashes on its own, is backed up with a tested restore, and holds ≥15 TPS with 3 players. No auth service, no launcher, no file distribution — those are Phases 2–4. Covers SRV-01…SRV-05.

</domain>

<decisions>
## Implementation Decisions

### Server Runtime
- Server lives in `~/rlcraft/server/` (inside the project repo; world/, logs/, backups gitignored)
- Java 8 = Eclipse Temurin 8 JDK arm64 installed via the official Adoptium apt repository (Debian 13 ships no openjdk-8). System Java 25 stays untouched
- `install.sh` downloads the official RLCraft Server Pack 2.9.3 zip from CurseForge CDN by file ID, verifies checksum, unpacks, runs the Forge 1.12.2-14.23.5.2860 installer `--installServer`. Reproducible, no manual downloads
- Runs as systemd system unit `rlcraft.service` under user `asphacean`, `Restart=on-failure`, `RestartSec=15`, `ExecStop` = RCON `stop` (mcrcon) with `TimeoutStopSec=90` so the world saves cleanly
- JVM: `-Xms6G -Xmx6G` + Aikar flags for Java 8 (G1GC). Leaves ~9 GB for other Pi services. Bump to 8G only if load test demands

### Network
- Server addressed by a domain/subdomain the operator provides (placeholder `mc.example.com` in configs until given). A-record → home public IP
- First task: CGNAT check (router WAN IP vs `curl ifconfig.me`). If IP is dynamic → ddclient / registrar-API update script; if static → nothing
- Only TCP 25565 forwarded to the Pi. RCON bound to 127.0.0.1 only
- Interim access control until Phase 2: `white-list=true`, `enforce-whitelist=true`, friends' nicks added by operator. Phase 2 replaces this with token auth

### Ops
- Backups: systemd timer every 6 h → RCON `save-off` + `save-all` → `tar --zstd` of world dirs → `save-on`; keep 14 archives in `~/rlcraft-backups/`; `restore.sh` script. Restore tested once (SRV-03)
- Scheduled restart daily 05:00 via RCON `stop` (systemd restarts it) — Forge 1.12.2 leaks memory over long uptimes
- `server.properties`: RLCraft pack defaults except `online-mode=false`, `max-players=10`, `view-distance=8`, `difficulty=hard`, `enable-rcon=true` (localhost, random password in file)
- Load test (SRV-05): 3 real friends online for 20 min, `/forge tps` recorded in SUMMARY. If <15 TPS: try heap 8G, view-distance 6, then RLCraft-safe tweaks

### Claude's Discretion
- Exact Aikar flag set, mcrcon vs alternative RCON client, backup script language (bash preferred), whether old `~/mcserver` (Paper 1.21.6) is stopped to free RAM — recommend stopping if it is running

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `~/mcserver/start.sh` — trivial Paper start script, nothing reusable; server is unrelated
- Docker 29 present but not used (bare systemd decided)

### Established Patterns
- Greenfield repo; no code yet. Other services on this Pi use pm2 (`~/.pm2`) and systemd; systemd chosen for the game server

### Integration Points
- Phase 2 auth-gate mod will be dropped into `server/mods/` and talk to auth service on loopback — keep mods dir under operator control, not overwritten by install.sh re-runs
- Phase 3 manifest generator reads `server/mods/` and `server/config/` as source of truth for the client pack

</code_context>

<specifics>
## Specific Ideas

- Operator will supply the real domain later; all scripts read it from a single `server.env`
- Check that the router isn't behind CGNAT before anything else — it invalidates SRV-04

</specifics>

<deferred>
## Deferred Ideas

- Offsite backup copy (rclone to cloud) — nice-to-have, not in v1
- Discord webhook on server crash/restart — later

</deferred>
