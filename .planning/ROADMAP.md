# Roadmap: RLCraft Private Server

## Overview

The journey runs from "nothing on the Pi" to "a friend downloads a launcher, registers, and plays". Each phase is a vertical slice that friends can actually use at the end of it: first a real RLCraft server they can join with a hand-installed client, then their own nick+password accounts enforced at the server (not just in a UI), then the modpack served with a hash manifest so any client can be brought in sync, then the launcher that does all of that for them, and finally a tagged release they download from a link. Nothing is built for a future phase that the current phase cannot demonstrate.

## Phases

**Phase Numbering:**

- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Playable Server on the Pi** - RLCraft server friends can join over the internet with a manual client, that survives reboots and holds TPS
- [ ] **Phase 2: Accounts & Enforced Auth** - Own nick+password accounts, with token validation enforced by a server-side Forge mod
- [ ] **Phase 3: Modpack Distribution** - HTTPS file server with a hash manifest so any client can be synced to the exact server pack
- [ ] **Phase 4: Launcher** - Tauri launcher: register/login, RAM slider, auto Java 8, auto-update, Play
- [ ] **Phase 5: Release to Friends** - Tagged GitHub builds for Windows and macOS that friends download and run

## Phase Details

### Phase 1: Playable Server on the Pi

**Goal**: Friends can play RLCraft together on the Pi over the internet using a hand-installed client, and the server keeps itself alive
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: SRV-01, SRV-02, SRV-03, SRV-04, SRV-05
**Success Criteria** (what must be TRUE):

  1. A friend outside the home network joins the server by domain name with a manually installed RLCraft 2.9.3 client and plays
  2. The server is back online by itself after a Pi reboot and after a hard kill, with no operator action
  3. With 3 players online simultaneously, measured TPS stays at or above 15
  4. A scheduled backup has been restored into a running server, and the restored world loads with player progress intact

**Plans**: 4/4 plans executed

Plans:
**Wave 1**

- [x] 01-01-PLAN.md — Preflight: Temurin 8 + ops tooling, pack acquired and checksum-pinned, CGNAT verdict, operator facts (SRV-01, SRV-04)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 01-02-PLAN.md — Tracer: pack installed, running under systemd on Java 8, a real client joins; crash/boot/daily-restart resilience, RCON off-box unreachable (SRV-01, SRV-02)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 01-03-PLAN.md — Six-hourly rotated world backups and a restore actually performed into the running server (SRV-03)

**Wave 4** *(blocked on Wave 3 completion)*

- [x] 01-04-PLAN.md — Reachable by domain from outside, DNS following the public IP, TPS measured under 3-player load, client setup doc (SRV-04, SRV-05)

### Phase 2: Accounts & Enforced Auth

**Goal**: Only a registered nick presenting a valid token can play; anyone with a vanilla client is turned away
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: AUTH-01, AUTH-02, AUTH-04, AUTH-05
**Success Criteria** (what must be TRUE):

  1. An account can be created with nick + password, and a second registration of the same nick is refused
  2. Correct password returns a short-lived session token; wrong password returns an error and no token
  3. A client launched with a valid token joins and plays normally
  4. A vanilla Minecraft client with no token, connecting as a registered nick, is kicked with a clear message before it can move or interact
  5. Inspecting the account database shows only argon2/bcrypt hashes — no plaintext passwords anywhere

**Plans**: 3/3 plans executed

Plans:
**Wave 1**

- [x] 02-01-PLAN.md — Auth service on loopback: register/login/validate, argon2id, single-use tokens, operator CLI, systemd, accounts in the backup (AUTH-01, AUTH-02)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 02-02-PLAN.md — Auth-gate Forge mod built on the Pi, and the tokenless join proven refused on a throwaway server (AUTH-04, AUTH-05)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 02-03-PLAN.md — One announced restart arms enforcement on the live server; operator client verification and the ops runbook (AUTH-04, AUTH-05)

### Phase 3: Modpack Distribution

**Goal**: The exact pack the server runs is fetchable over HTTPS with per-file hashes, so any client can be brought into sync
**Mode:** mvp
**Depends on**: Phase 2
**Requirements**: DIST-01, DIST-02, DIST-03, DIST-04
**Success Criteria** (what must be TRUE):

  1. Requesting the manifest over HTTPS from the domain returns path + sha256 + size for every managed file, with a valid certificate
  2. The operator changes a mod or config, runs one command, and the new hashes are live on the file server
  3. A client assembled purely from the manifest's file list connects and plays on the server (verified by hand once)
  4. Minecraft jars, libraries and assets are never served from our host; only license-permitting mods/configs are
  5. A status endpoint reports server online/offline and current player count

**Plans**: 3/3 plans executed

Plans:
**Wave 1**

- [x] 03-01-PLAN.md — Private CA, Caddy on :8444, the route table, and a real Server List Ping /status (DIST-01, DIST-04)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 03-02-PLAN.md — publish-pack.sh: the full client pack, the hashed manifest, and a client assembled from nothing but it (DIST-01, DIST-02, DIST-03)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 03-03-PLAN.md — Router forward for TCP 8444, an honest reachability check, and the Phase 4 integration contract (DIST-01, DIST-03, DIST-04)

### Phase 4: Launcher

**Goal**: A friend goes from opening the launcher to playing on the server with no manual setup of Java, Forge, or mods
**Mode:** mvp
**Depends on**: Phase 3
**Requirements**: AUTH-03, LNCH-01, LNCH-02, LNCH-03, LNCH-04, LNCH-05, LNCH-06, LNCH-07, LNCH-08
**Success Criteria** (what must be TRUE):

  1. On a clean machine with no Java installed, a friend registers, picks RAM, presses Play, and lands in the RLCraft world on our server
  2. The second launch does not ask for a password, and re-downloads only the files whose hashes changed — saves and options are untouched
  3. Windows x64, macOS Intel and macOS Apple Silicon each get a working Java 8 from the correct vendor, never the system Java
  4. Download and launch show the current step and file/byte progress instead of an unexplained wait
  5. Wrong password, unreachable server, failed Java download and full disk each show a plain-language message naming the log file; server status and launcher self-update work on startup

**Plans**: 3/4 plans executed
**UI hint**: yes

Plans:
**Wave 1**

- [x] 04-01-PLAN.md — Tracer: rustup/Tauri toolchain and the Node-free launcher workspace, refresh tokens in the auth service plus the two new Caddy routes, and a real form that logs in against the live server and shows its status (AUTH-03, LNCH-01, LNCH-07)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 04-02-PLAN.md — Manifest sync ported from the Phase 3 reference implementation, and a checksum-verified Java 8 the launcher owns for all three shipped platforms (LNCH-02, LNCH-03, LNCH-05)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 04-03-PLAN.md — Mojang's own files SHA-1 verified, Forge installed headlessly via the profile stub, and a complete launch command with the token handoff and a seeded server list (LNCH-04)

**Wave 4** *(blocked on Wave 3 completion)*

- [ ] 04-04-PLAN.md — Play wired end to end over a channel with plain-English errors, the RAM slider, self-update on the file server, the RLCraft skin, and the operator QA matrix (AUTH-03, LNCH-01, LNCH-05, LNCH-06, LNCH-08)

### Phase 5: Release to Friends

**Goal**: Friends get the launcher from a link and run it, on both Windows and macOS
**Mode:** mvp
**Depends on**: Phase 4
**Requirements**: REL-01, REL-02, REL-03
**Success Criteria** (what must be TRUE):

  1. Pushing a tag produces a GitHub release containing a Windows x64 installer and a macOS app, built on the self-hosted runners
  2. A friend downloads the Windows installer from that release, installs it, and plays — nothing else required
  3. A friend on an Apple Silicon Mac follows the written one-time Gatekeeper bypass, opens the app, and plays with the game rendering correctly on real hardware

**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Playable Server on the Pi | 4/4 | In Progress|  |
| 2. Accounts & Enforced Auth | 3/3 | In Progress|  |
| 3. Modpack Distribution | 3/3 | In Progress|  |
| 4. Launcher | 3/4 | In Progress|  |
| 5. Release to Friends | 0/TBD | Not started | - |

## Requirement Coverage

25 of 25 v1 requirements mapped, each to exactly one phase.

| Phase | Requirements | Count |
|-------|--------------|-------|
| 1 | SRV-01 … SRV-05 | 5 |
| 2 | AUTH-01, AUTH-02, AUTH-04, AUTH-05 | 4 |
| 3 | DIST-01 … DIST-04 | 4 |
| 4 | AUTH-03, LNCH-01 … LNCH-08 | 9 |
| 5 | REL-01 … REL-03 | 3 |

---
*Roadmap created: 2026-08-27*
