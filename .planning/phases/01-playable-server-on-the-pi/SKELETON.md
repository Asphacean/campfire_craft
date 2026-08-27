# Walking Skeleton — RLCraft Private Server

**Phase:** 1
**Generated:** 2026-08-27

## Capability Proven End-to-End

A whitelisted friend on a hand-installed RLCraft 2.9.3 client joins the Pi's server by domain name over the internet and plays — and the server brings itself back after a crash or reboot, keeps rotated backups of the world, and has a measured TPS figure under three-player load.

The tracer slice inside that (plan 01-02, Task 1) is the thinnest version: one client, on the LAN, joining a Forge 1.12.2 server that Temurin 8 runs, that systemd owns, that RCON controls. Every later slice in this phase and in Phases 2–4 expands outward from that one proven path.

## Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Process supervision | Bare systemd system unit `rlcraft.service` under user `asphacean` — **no Docker** | Docker is installed on this Pi but adds a layer with nothing to offer a single long-lived JVM; systemd already provides restart-on-failure, boot enable, timers and a stop hook. Explicitly out of scope in REQUIREMENTS.md |
| Java runtime | Eclipse Temurin 8 JDK aarch64, invoked by **absolute path** from `server.env` (`JAVA8_BIN`), installed from the Adoptium apt repo, with the Adoptium tarball as fallback | Forge 1.12.2 only runs on Java 8 (its LaunchWrapper classloader hack breaks on 9+). The Pi's system Java 25 must stay the default for every other service, so the path is pinned rather than `PATH`-resolved |
| Server content | Official RLCraft Server Pack 2.9.3 (CurseForge project 285109, file 4612990) unpacked by `scripts/install.sh`, sha256-pinned in `server.env`, Forge 1.12.2-14.23.5.2860 materialised by the official installer's `--installServer` | Reproducible from a pinned artifact rather than a hand-assembled directory. `unzip -n` semantics keep `server/mods/` operator-owned, which Phase 2 requires |
| Control channel | RCON on port 25575, loopback-only in effect, driven by `itzg/rcon-cli` (static aarch64 binary) | Every ops action — graceful stop, backup save-off/save-all, whitelist edits, TPS sampling — goes through one channel a script can drive. `mcrcon` publishes no arm64 binary, so it would need a build toolchain for no gain |
| Config management | Tracked `server/server.properties.template` rendered by `envsubst` into a gitignored `server/server.properties`; `scripts/install.sh --config-only` re-renders | Keeps the RCON password out of git while still versioning the config shape, and makes the SRV-05 tuning ladder a one-line env edit plus a re-render |
| Single source of operator facts | `server.env` (mode 600, gitignored) with a tracked `server.env.example` template | Domain, Java path, heap, RCON credentials, pack pin, backup retention, view distance, whitelist nicks, DDNS credentials — every script sources one file, so no fact is duplicated into a second place that can drift |
| World persistence + ops | systemd timers: backup every 6 h (`save-off` → `save-all` → `tar --zstd` of the single `world/` tree → `save-on` via trap → rotate to 14) and a graceful `systemctl restart` daily at 05:00 | Forge 1.12.2 leaks memory over long uptimes, and a live-world `tar` silently corrupts regions. The whole world including `DIM-1`/`DIM1` is one tree — dimensions are *not* sibling directories on this server |
| Network exposure | One forwarded port (TCP 25565) to a domain, DNS kept current by a packaged DDNS updater when the WAN address is dynamic; RCON dropped for non-loopback sources by a scoped `nftables` table | Only the game port crosses the boundary. The firewall table carries `policy accept` and two rules, so it can never lock the operator out of SSH — and it holds even if the router's UPnP opens something it should not |
| Interim access control | `white-list=true` + `enforce-whitelist=true`, nicks added over RCON | Offline-mode authenticates nobody; the whitelist is the only Phase 1 gate and is a documented interim, replaced by Phase 2's server-side auth-gate mod (AUTH-04) |
| Secrets | Never in git: `server.env` mode 600, rendered config gitignored, DDNS credential file root-owned mode 600, RCON password from `openssl rand -hex 24` | The repo will eventually be pushed; nothing that leaks a credential may be tracked |
| Directory layout | `~/rlcraft/{server,scripts,systemd,docs}` + `server.env` + `downloads/` (untracked) + `~/rlcraft-backups/` (outside the repo, mode 700) | Scripts, units and docs are versioned; regenerable pack payload, world data and archives are not |

## Stack Touched in Phase 1

- [x] **Project scaffold** — repo layout, `.gitignore`, `server.env.example`, idempotent `scripts/preflight.sh` (plan 01-01)
- [x] **Runtime** — Temurin 8 aarch64 installed and pinned, system Java 25 untouched (plan 01-01)
- [x] **The application itself** — RLCraft 2.9.3 on Forge 1.12.2 installed from a pinned artifact and running (plan 01-02, tracer)
- [x] **Supervision / deployment** — systemd unit enabled for boot, SIGKILL recovery proven, daily graceful restart timer (plan 01-02)
- [x] **Persistence: real write AND real read-back** — scheduled world archives, and a restore actually performed into the running server, proven by a `level.dat`-resident value that round-tripped (plan 01-03)
- [x] **Interactive client wired to the real thing** — a hand-installed RLCraft client joins and plays; documented in `docs/CLIENT-SETUP.md` (plans 01-02, 01-04)
- [x] **Network exposure** — one forwarded port, a domain that follows the public IP, verified from outside the LAN (plan 01-04)
- [x] **Measurement** — TPS sampled over a real three-player session against a stated threshold (plan 01-04)

## Out of Scope (Deferred to Later Slices)

Explicit, so no later phase re-litigates Phase 1's minimalism:

- Token authentication of any kind — no auth service, no client or server auth mod. The whitelist is the entire Phase 1 gate (Phase 2: AUTH-01/02/04/05)
- Any HTTPS file server, modpack manifest, hash generation, or status endpoint (Phase 3: DIST-01…04)
- The launcher: no Tauri app, no Java provisioning for clients, no manifest diffing, no RAM slider (Phase 4)
- Packaging, signing, GitHub Actions release pipelines (Phase 5)
- Offsite/cloud backup copies (rclone) — deferred in CONTEXT.md
- Discord webhooks on crash or restart — deferred in CONTEXT.md
- Player skins (needs Drasl/authlib-injector, which conflicts with the chosen auth design) — v2
- Web admin panel or RCON dashboard, password reset flows — v2
- Docker for the game server, anti-grief/moderation/ranks, a Linux launcher build — out of scope project-wide

## Subsequent Slice Plan

Each later phase adds one vertical slice on top of this skeleton without changing its architectural decisions:

- **Phase 2** — Only a registered nick presenting a valid token can play: auth service plus a server-side auth-gate Forge mod dropped into the operator-owned `server/mods/`, talking to the service over loopback. Replaces the whitelist as the access gate.
- **Phase 3** — The exact pack the server runs becomes fetchable over HTTPS with per-file hashes, generated from `server/mods/` and `server/config/` — which is precisely why this phase keeps those directories operator-owned and (for config) tracked.
- **Phase 4** — The launcher does the whole of `docs/CLIENT-SETUP.md` automatically: Java 8, Forge client, manifest diff, token injection, RAM choice, Play.
- **Phase 5** — Tagged GitHub releases of that launcher for Windows and macOS.
