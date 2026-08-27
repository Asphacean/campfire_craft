# Project Research Summary

**Project:** RLCraft Private Server + Custom Launcher
**Domain:** Custom Minecraft launcher + offline-mode private server with own authentication (Tauri frontend, Forge 1.12.2 server on Raspberry Pi 5, 5-7 friends)
**Researched:** 2026-08-27
**Confidence:** HIGH (stack/architecture well-established; critical security fix identified and documented)

## Executive Summary

This project combines four interlinked systems: a Tauri launcher for Windows/macOS that handles authentication and file updates, a small Rust HTTP auth service, a Forge 1.12.2 Minecraft server running RLCraft on a Raspberry Pi 5, and a file server for distributing mods/configs. The recommended stack leverages existing expertise (Rust for launcher + auth service, systemd for server supervision, Caddy for TLS) and avoids over-engineering for a 5-7 person friend group.

**Critical architectural decision:** Research identified a significant security gap in the initial STACK.md recommendation. Offline-mode Minecraft trusts any client that connects with an arbitrary username — password checking in the Tauri launcher alone does not prevent a friend (or anyone who learns the IP) from joining the server with a vanilla client and impersonating another player. The recommended approach enforces password/token validation at the **server layer** via a lightweight Forge mod that gates movement until the launcher-issued token is validated, not just in the launcher UI. This is non-negotiable for any "access control" guarantee.

The largest technical risk is RLCraft's CPU intensity on ARM: the Pi 5 is powerful, but Minecraft's single-threaded tick loop combined with RLCraft's dense mob AI and Forge's per-tick overhead means 2-3 simultaneous players can saturate CPU and cause noticeable TPS drops. Performance testing with real concurrent load must happen before the server is called "done," and view-distance / mob-cap tuning should be built into the server configuration from day one, not added as an afterthought.

Secondary risks include macOS/Apple Silicon compatibility (LWJGL 2 has no native ARM64 build; Rosetta 2 emulation is slow) and the fragility of the RLCraft modpack (adding extra mods risks breaking the mod interactions the pack's mods assume about each other). Both are documented with mitigation paths and test-before-shipping requirements.

## Key Findings

### Recommended Stack

**Server-side (Pi 5):**
- **Eclipse Temurin JDK 8** — Forge 1.12.2 runs only on Java 8; system Java 25 must not be used. Explicit `ExecStart` path in systemd unit pointing to `/usr/lib/jvm/temurin-8-jdk-arm64/bin/java` prevents accidental version confusion.
- **Forge 1.12.2-14.23.5.2860** — Pinned to RLCraft's bundled version; later patch builds (2860.x) acceptable; 2854+ minimum for RCE fix.
- **RLCraft Server Pack v2.9.3** — Latest official release (2022); frozen, stable, no longer updated.
- **systemd bare-metal service** — Simpler than Docker for a single fully-owned Pi instance. Includes `Restart=on-failure`, `RestartSec=20`, `SuccessExitStatus=143`.
- **G1GC tuning (Aikar's flags baseline)** — Standard for modded 1.12.2 servers; keep `-Xms`/`-Xmx` equal, cap heap at 6-8 GB, tune via JVM flags and `server.properties`.

**Auth service (small Rust backend on Pi):**
- **Rust `axum` HTTP service** — `/register`, `/login` (POST with nick+password), `/validate-token` (called by server-side mod over loopback).
- **`argon2` password hashing** — Modern default (Argon2id), memory-hard, GPU-resistant.
- **SQLite database** — Adequate for 5-7 users; trivial backup (single file).

**Launcher (Tauri, Windows/macOS):**
- **Tauri 2.11+** — Stable API, current maintenance. Rust backend (no separate runtime).
- **`reqwest` + `sha2`/`sha1`** — HTTP downloads with streaming progress (Tauri channels), hash verification of manifest files (SHA-256) and Mojang assets (SHA-1).
- **Java 8 provisioning:** Adoptium Temurin for Windows x64 and macOS Intel; **Azul Zulu 8** (only vendor with official macOS ARM64 builds) for Apple Silicon. Treat as a per-platform vendor matrix.
- **Manifest + diff download** — Static JSON with `{path, sha256, size}[]`; launcher diffs against local install and downloads only changed files.

**File & proxy infrastructure (Pi):**
- **Caddy reverse proxy** — TLS termination for both auth service + file server on 443; automatic HTTPS via Let's Encrypt; simple `Caddyfile` configuration.
- **Static manifest + file server** — Caddy's `file_server` directive serves `manifest.json` + mod/config tree.

### Expected Features

**Table stakes (P1 — must have for launch):**
- Login/register (nick + password), RAM slider, Play button, Auto Java 8 fetch
- Manifest-based client auto-update (hash diff, only download changed files)
- Progress bar / status label during download & launch
- Basic readable error messages (auth failure, server unreachable, disk full)
- Remember-me / stay logged in (session token stored locally, not plaintext password)
- Server-side whitelist / access control (auth service gates `whitelist.json` updates)
- Server autostart on boot + autorestart on crash (systemd)
- Scheduled world backups with rotation (cron + `tar`, not live-folder backup)

**Differentiators (already central to PROJECT.md):**
- Zero manual setup (no Forge install, no mod folder, no Java hunt)
- Manifest-based incremental updates (faster after first install)
- Single source of truth for modpack (no version drift between clients)
- Tiny launcher binary (~10MB Tauri vs 100MB+ Electron-based launchers)

**Anti-features to skip (explicitly out of scope for v1):**
- Player skins, multiple accounts, news feed, offline play, launcher UI theming, crash telemetry, RCON dashboard, server status widget

### Architecture Approach

The recommended architecture separates four runtime concerns: the launcher, the auth service, the file server, and the Minecraft server itself. These are intentionally decoupled — the launcher only needs stable HTTP contracts (auth API shape, manifest format); the auth service is fully independent (testable with `curl`); the file server is a static-file proxy; and the Minecraft server is standalone (runs offline-mode with a custom Forge mod pair for authentication).

**Core pattern:** A launcher-issued token (random string, minutes-long TTL, single-use preferred) is passed to the game process as a JVM system property (`-Dauth.token=…`) at launch. A thin client-side Forge mod reads it and sends it to the server via a custom plugin-message packet on join. A matching server-side Forge mod intercepts the login, validates the token by calling `POST /validate` on the auth service over **loopback HTTP** (same Pi, no TLS needed, not exposed to the internet), and either allows the player to spawn/move or kicks them with a clear message. This keeps the password database, rate-limiting, and all credential logic entirely inside the auth service; the game server never sees or hashes a password.

**Major components:**
1. **Tauri Launcher** — Login/register UI, manifest download & diff, Java 8 provisioning, Forge launch-command construction, process spawn with token injected.
2. **Auth Service** — `/register`, `/login`, `/validate-token` endpoints. Single SQLite DB file.
3. **File Server** — Serves `manifest.json` + modpack file tree (mods/, config/). Read-only from the launcher's perspective.
4. **Forge Server** — Offline-mode, listens on 25565. Includes a custom server-side "auth-gate" Forge mod that validates tokens.
5. **Client Auth Mod** — On join, reads `-Dauth.token=`, sends it to the server mod via a custom plugin packet.
6. **Server Auth-Gate Mod** — Freezes movement/interaction until it receives a valid token from the client.
7. **Caddy Reverse Proxy** — TLS termination for auth service + file server on port 443.

**Build order** (dependency-driven):
1. Forge server on Pi
2. Auth service (standalone, testable)
3. Client + server auth mods
4. File server + manifest generator
5. Caddy + domain/TLS
6. Tauri launcher
7. GitHub Actions CI

### Critical Pitfalls

1. **Offline-mode auth enforced client-side only (Pitfall 3) — CRITICAL SECURITY GAP.** Minecraft offline-mode accepts any username from any client with zero verification. If password checking lives only in the Tauri launcher, the actual game server is unauthenticated — anyone who knows the IP and has a vanilla Minecraft client can connect as any player and hijack their progress. **Mitigation:** Implement server-side auth-gate mod that kicks unauthenticated players before they can move/act. Verify manually: connect with a vanilla Minecraft client (no launcher) and confirm rejection.

2. **RLCraft CPU-bound on ARM, not RAM-bound (Pitfall 1).** "Can't keep up! Running 50ms behind" appears within minutes of 2-3 simultaneous players. **Mitigation:** (a) load-test with 2-3 real concurrent players; (b) tune server from day one: reduce `view-distance` to 6-8, cap mob spawning; (c) set expectations that this is a real risk. TPS should target ≥18-19.

3. **Wrong Java version breaks everything silently (Pitfall 2).** Forge 1.12.2 requires Java 8 exactly; Java 9+ breaks the classloader. **Mitigation:** Install Eclipse Temurin 8 aarch64 explicitly, use full path in systemd `ExecStart`; Launcher side: verify downloaded JDK reports major version 8.

4. **LWJGL 2 not Apple Silicon native (Pitfall 5).** Minecraft 1.12.2 uses LWJGL 2.9.x, which predates ARM64 support. **Mitigation:** Bundle ARM64-native LWJGL 2 replacement jars for Apple Silicon installs, detect architecture automatically, test on real hardware.

5. **Java 8 vendor gap on macOS ARM64 (Pitfall 6).** Eclipse Temurin does NOT publish Java 8 builds for macOS aarch64. **Mitigation:** Use Azul Zulu 8 for Apple Silicon; use Temurin for Windows x64 and macOS Intel. Encode as an explicit per-platform vendor matrix.

## Implications for Roadmap

Research informs a 7-phase build order, with the critical addition that auth enforcement must happen at the **server layer** (Pitfall 3 mitigation).

### Phase 1: Server Setup & Performance Foundation
**Rationale:** Everything downstream depends on a working Forge 1.12.2 server on the Pi.
**Delivers:** Pi 5 running RLCraft, systemd autostart, JVM tuning, scheduled backups (with verified restore), TPS stable with 2-3 simultaneous players.
**Research flag:** None — well-documented server-ops pattern.

### Phase 2: Auth Service & Database
**Rationale:** Fully standalone, testable with `curl`. Validates the auth API contract.
**Delivers:** Rust `axum` HTTP service with `/register`, `/login`, `/validate-token`. SQLite DB. Argon2 hashing. Rate-limiting.
**Research flag:** None — standard HTTP service + password hashing.

### Phase 3: Auth Mods (Client + Server Pair)
**Rationale:** Depends on phase 1 & 2. Matched pair — must be released together.
**Delivers:** Server-side Forge mod that validates tokens; client-side Forge mod that sends tokens. Custom packet protocol.
**Research flag:** MEDIUM — Forge 1.12.2 plugin-packet channel implementation details need spike.

### Phase 4: File Server & Manifest Generator
**Rationale:** Depends on phase 3. Produces stable manifest contract for launcher.
**Delivers:** Caddy configuration, manifest generator script (cron-triggered), served modpack files, versioned manifest (prevents race conditions).
**Research flag:** None — Caddy is standard, manifest diffing is established pattern.

### Phase 5: TLS/Domain & Reverse Proxy
**Rationale:** Fronts phases 2 & 4 with automatic HTTPS. Deferred during LAN testing but required for external access.
**Delivers:** Caddy with domain + TLS. Let's Encrypt cert provisioning. HTTPS endpoints.
**Research flag:** None — Caddy automatic HTTPS well-documented.

### Phase 6: Tauri Launcher (Multi-Step, Windows/macOS)
**Rationale:** Built last, integrates all prior phases. Most complex; built only after all stable contracts finalized.
**Delivers:** Windows launcher (x86_64), macOS launcher (Intel + Apple Silicon). All P1 features. Includes login UI, manifest diff/download, Java 8 per-platform provisioning, Forge launch, token injection, macOS code-signing.
**Research flag:** HIGH-MEDIUM — Tauri's channel-based download-progress API, Forge `--installClient` headless verification, macOS code-signing automation need spikes.

### Phase 7: GitHub Actions CI & Binary Distribution
**Rationale:** Deferred until launcher is stable.
**Delivers:** GitHub Actions matrix builds for Windows + macOS (Intel + ARM). Tauri-action integration. Binary distribution.
**Research flag:** None — GitHub Actions + Tauri action well-documented.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| **Stack** | HIGH | Verified against official sources (Adoptium, Forge Files, CurseForge, Tauri). Caveat: Forge `--installClient` headless behavior is MEDIUM (recommend spike). |
| **Features** | HIGH | Launcher UX patterns are stable conventions across GravitLauncher, Modrinth App, Prism Launcher. |
| **Architecture** | HIGH | Pattern 1 (token handoff, server-side validation) is synthesis of known-working pieces. Alternatives well-researched and justified rejection. Exception: Forge 1.12.2 mod specifics are MEDIUM. |
| **Pitfalls** | HIGH | Critical pitfalls well-corroborated. Security pitfalls are best-practice. |
| **Overall** | HIGH | Research is thorough, sources primary. Critical security gap identified with clear mitigation. Build order is sound. Ready for roadmap. |

### Gaps to Address

1. **Forge 1.12.2 `--installClient` headless behavior** — Verify directly against 2860 installer jar before Phase 6 commits. Phase 6 spike recommended.

2. **Forge 1.12.2 mod development specifics** — Auth mods (Phase 3) codebase not created yet. Gradle build, plugin-channel registration, login-event listening patterns need confirmation during Phase 3.

3. **Real hardware load testing of RLCraft on Pi 5** — Phase 1 must test with 2-3 actual concurrent players. Single-player smoke tests won't catch CPU saturation (Pitfall 1).

4. **macOS Apple Silicon hardware** — Phases 5 & 6 need real M-series Mac for testing. Gatekeeper, notarization, LWJGL 2 rendering issues cannot be tested remotely.

5. **CGNAT status of home ISP** — Phase 5 networking depends on knowing if ISP uses CGNAT. Verify: does router WAN IP match public "what's my IP" check? If not, port forwarding alone won't work.

6. **CurseForge mod redistribution permissions** — Phase 4's file server must audit each mod in RLCraft pack for redistribution permission. Do during Phase 4 planning, not implementation.

7. **Backup restore test before go-live** — Phase 1's backup requirement includes actually restoring and verifying a backup works. Must happen before "done."

---

*Research completed: 2026-08-27*
*Synthesized by: GSD Research Synthesizer*
*Ready for roadmap creation: YES*
