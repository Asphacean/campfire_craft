# RLCraft Private Server

## What This Is

Private RLCraft (Forge 1.12.2) Minecraft server for a group of 5–7 friends, hosted on a Raspberry Pi 5, plus a minimalist Windows/macOS launcher (Tauri) that registers/logs in the player with nickname + password, auto-downloads and keeps the modpack client up to date from our own file server, fetches Java 8 if missing, and lets the player set the RAM allocation. Authentication is our own (offline-mode server + auth service), no Mojang/Microsoft account required.

## Core Value

A friend downloads the launcher, registers a nick + password, presses Play — and ends up on the RLCraft server with no manual setup.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] RLCraft server (official CurseForge pack, latest stable) runs on the Pi 5 under Java 8, reachable via public IP/domain
- [ ] Server in offline-mode with password auth: player must register in launcher before joining; unregistered/incorrect password is rejected
- [ ] Self-registration: anyone with the launcher can create an account (nick + password)
- [ ] File server hosts the client modpack with a manifest (file hashes); launcher downloads only changed files
- [ ] Launcher: login/register form (nick, password), RAM slider, Play button
- [ ] Launcher fetches Java 8 runtime automatically for Windows x64, macOS Intel and Apple Silicon
- [ ] Launcher launches Forge 1.12.2 client with the chosen RAM and auto-connects credentials to the server
- [ ] Launcher builds for Windows and macOS via GitHub Actions (self-hosted runners exist)
- [ ] Server autostarts on boot and survives restarts; world backups

### Out of Scope

- Microsoft/Mojang authentication — friends-only, own auth chosen
- Linux launcher build — no Linux players
- Public server features (anti-grief, moderation, ranks) — closed friend group
- Custom launcher skins/news feed — "minimalist" was explicit
- Modpack editing / mod selection UI — launcher ships the exact server pack

## Context

- Host: Raspberry Pi 5, aarch64, 4 cores, 15 GB RAM, 133 GB free disk, Debian 13, Docker 29 present. Java 25 installed system-wide; Java 8 (Temurin aarch64) must be added for Forge 1.12.2.
- An old Paper 1.21.6 server lives in `~/mcserver` (online-mode=false) — unrelated, may be retired to free RAM.
- Public IP/domain available; port forwarding on the router is possible.
- Two GitHub Actions self-hosted runners installed (`~/actions-runner-1`, `-2`) — can build launcher binaries.
- RLCraft is heavy: server needs ~6–8 GB heap; expect tuning (view-distance, tick optimisation mods) for 5–7 players on ARM.

## Constraints

- **Tech stack**: Forge 1.12.2 / Java 8 — dictated by RLCraft
- **Hardware**: Pi 5, 15 GB RAM shared with other services — server heap ≤ 8 GB
- **Launcher**: Tauri (Rust + web UI), Windows + macOS (Intel & ARM) — small binary, native builds
- **Auth**: offline-mode + own auth — no game license required
- **Legal**: launcher must not redistribute Minecraft client jar/assets — download them from Mojang's official servers; only mods/configs are served from our file server

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Own auth (offline-mode), not Microsoft | Friends without license; nick+password UX requested | — Pending |
| Tauri for launcher | ~10 MB binary, native Win/Mac builds, existing GH runners | — Pending |
| Manifest-based auto-update of client | Change mods/configs server-side, players get them automatically | — Pending |
| Launcher downloads Java 8 itself | Zero manual setup for friends | — Pending |
| Host on Pi 5 | Already running 24/7, 15 GB RAM sufficient for 5–7 players | — Pending |
| Project lives in `~/rlcraft` | Keep .planning/git out of home dir | ✓ Good |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-08-27 after initialization*
