# Technology Stack

**Project:** RLCraft private server + auth + launcher
**Researched:** 2026-08-27
**Confidence:** MEDIUM-HIGH (server/Java/auth verified against official sources; exact Forge-1.12.2 client-install mechanics and Tauri crate pinning are MEDIUM — flagged for phase-level spikes)

This is a 4-part stack (server, auth, distribution, launcher). Tables are grouped by part instead of a single flat list, because the "why" differs per part.

## Recommended Stack

### (a) RLCraft server on Pi 5 (aarch64)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Eclipse Temurin JDK 8 (aarch64) | 8u482 (latest 8 LTS as of Feb 2026) | Java runtime for Forge 1.12.2 | Adoptium ships official Linux **aarch64** builds for JDK 8 — confirmed on the [releases page](https://adoptium.net/temurin/releases/?version=8&arch=aarch64&os=mac) and Docker Hub `arm64v8/eclipse-temurin`. Install via Adoptium's apt repo or the tarball; do not replace the system Java 25 — point the systemd unit at the Temurin 8 binary explicitly (`ExecStart=/usr/lib/jvm/temurin-8-jdk-arm64/bin/java ...`). |
| Forge 1.12.2-14.23.5.2860 | 2860 (or later 2860.x patch) | Modloader matching RLCraft's shipped Forge build | RLCraft's server pack is built against 14.23.5.2860; 2854+ is also the minimum Forge advises for a known RCE fix, so 2860 satisfies both "matches the pack" and "not the vulnerable build." Confirmed on [files.minecraftforge.net](https://files.minecraftforge.net/net/minecraftforge/forge/index_1.12.2.html). |
| RLCraft Server Pack 1.12.2 — Release v2.9.3 | 2.9.3 | The actual modpack/world content | Latest official server-pack release on [CurseForge](https://www.curseforge.com/minecraft/modpacks/rlcraft/files/4612990). RLCraft development has been frozen since ~2022 (this is a mature, stable, no-longer-moving target — good for a long-lived private server). |
| systemd unit (bare metal), not Docker | — | Process supervision, autostart, crash-restart | `Restart=on-failure`, `RestartSec=20`, `SuccessExitStatus=143` (Minecraft server's clean-stop exit code), `TimeoutStopSec=90` (world save on shutdown). Simpler than Docker for a single, fully-owned instance: no container networking to route the `-javaagent` authlib-injector jar or JVM flags through, no image-tag caveats to track. |
| G1GC tuning flags ("Aikar's flags") | — | JVM GC tuning for modded MC | Standard, widely-used baseline for modded 1.12.2 servers; on a 4-core Pi 5 keep `-Xmx`/`-Xms` equal (avoid heap resizing pauses) and cap heap at **6–8 GB** per PROJECT.md's own budget. Treat exact flag values as a phase-level tuning task, not a stack decision — the RAM ceiling and GC family (G1GC) are the actual stack choice. |

**What NOT to do here:** Do not bolt extra performance mods (Phosphor, VanillaFix, additional FoamFix builds) onto RLCraft. RLCraft **already ships FoamFix**, and the RLCraft community consensus is that the pack is fragile to modification — many of its mods patch core mechanics (entity AI, tick behavior, world gen) in ways that assume the exact bundled mod set. Any mod added to the server must also ship in the client pack (Forge 1.12.2's per-mod `@Mod` version check will otherwise reject connecting clients, unless the mod author explicitly marked it side-agnostic) — so "just add a server-side perf mod" is not actually simpler than it looks. Tune via JVM flags, `server.properties` (`view-distance`, `max-tick-time`), and FoamFix's own config instead.

**Docker alternative (if you want it anyway):** `itzg/minecraft-server` does publish arm64/aarch64 images and supports `TYPE=FORGE`. Two caveats found in the itzg docs/issue tracker: (1) images built on Oracle Linux have a `zlib-ng` incompatibility with the Forge installer — stick to the default Debian-based `java8` tag, not an Oracle-Linux variant; (2) Forge does not run on OpenJ9 JVM tags — make sure the image resolves to HotSpot. This is all avoidable by not using Docker at all, which is the recommendation above.

### (b) Auth (offline-mode + own nick/password)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Small Rust `axum` service (own build) | axum ^0.7 | `/register`, `/login` endpoints, issues a short-lived session token the launcher then uses to launch the game as `--username <nick>` | Matches PROJECT.md's already-decided "offline-mode + own auth" (not Microsoft/Mojang, not a full Yggdrasil replacement). Reuses the Rust skillset the launcher already needs — no new language for the team. |
| `argon2` crate | ^0.5 | Password hashing | Modern default (Argon2id), memory-hard, resistant to GPU cracking — the standard choice over bcrypt for new Rust services in 2025/2026. |
| SQLite via `rusqlite` or `sqlx` | — | User accounts store | 5–7 users total; a full Postgres/MySQL server is unjustified weight on a Pi already running the MC server. Single file, trivial backup (copy alongside world backups). |
| Minecraft server `whitelist.json`, offline-mode `true` | — | Actual join-time gate | The auth service writes/removes entries in `whitelist.json` (via RCON `whitelist add <nick>` or direct file edit + `whitelist reload`) when an account is approved. This is what makes "unregistered players rejected" true at the Minecraft-server level, not just inside the launcher UI. |

**Known limitation, stated plainly:** offline-mode Minecraft trusts whatever username the connecting client sends — there is no cryptographic proof tying "knows the password" to "is the player using that name." A non-launcher client that already knows a friend's whitelisted nickname could still connect as them. For a small, mutually-trusted friend group this is the standard, accepted tradeoff (it's exactly what "offline-mode" servers everywhere do) and matches the Key Decision already recorded in PROJECT.md. Don't build past this without a reason.

**If you ever want real enforcement (upgrade path, not MVP):** [Drasl](https://github.com/unmojang/drasl) is a self-hosted, standalone Yggdrasil-compatible identity server (Go, own username+password accounts, own registration API — confirmed it does not require an existing Mojang account when self-hosted) that pairs with **authlib-injector** as a `-javaagent` on both the Minecraft server (`JVM_OPTS=-javaagent:/path/authlib-injector.jar=https://your-drasl-host`) and the launcher's Java invocation. This makes the server run `online-mode=true` against *your* identity server instead of Mojang's — genuine crypto-backed login, and it works with 1.7.2+ / Forge 1.12.2 (confirmed in Drasl's docs). This is a real second service (Go binary + its own DB) — don't build it for MVP; it's the documented answer if "unregistered/incorrect password rejected" needs to mean "the Minecraft protocol itself rejects it," not just "the launcher won't let you press Play."

**What NOT to use:** `SimpleLogin` (the Forge-mod, AuthMe-style approach) — its last 1.12.2 build is from December 2021 (beta, build 127), unmaintained for this MC version, and it requires bundling a client-side mod into the RLCraft pack (extra mod risk, see part (a)'s warning) plus in-chat `/login` UX that duplicates what the launcher is already supposed to do. AuthMeReloaded doesn't apply at all — it's a Bukkit/Spigot plugin, not compatible with Forge.

### (c) Client file/update server

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Caddy | 2.x (latest stable) | TLS termination + static file serving (or reverse-proxy to the axum service) | Project already has a public domain — Caddy's automatic HTTPS (Let's Encrypt built in, zero config) removes certbot/renewal maintenance entirely. Simpler `Caddyfile` than an nginx conf for the same job. |
| Custom JSON manifest, SHA-256 | — | `{path, sha256, size}[]` describing every file in the client pack (mods, configs, resource packs) | Launcher downloads Mojang's own libraries/assets separately (see part d) using **Mojang's SHA-1** hashes — the manifest for *our own* mod/config files is a separate, self-controlled format, and SHA-256 is the right modern choice there since we're not constrained by a legacy protocol. Generate the manifest with a short script (`sha256sum` + file size) run whenever the server pack updates; no need for a database. |
| Plain HTTP GET + hash-compare, not rsync/Syncthing | — | Update mechanism | The launcher already needs an HTTP client (for Mojang downloads and Java download) — reusing that for mod files means zero new protocols or daemons. rsync needs a persistent rsync daemon or SSH access on the server side; Syncthing needs a running peer on every client. A static manifest + `reqwest` GET is simpler and is exactly what tools like `packwiz`/`packwiz-installer` and `unsup` already do for this exact use case (hash-validated modpack sync) — validates this is the standard approach, not a one-off invention. |

**Alternative worth naming:** if you'd rather not hand-roll the manifest tool, `packwiz` (Go, TOML-based, native CurseForge/Modrinth mod resolution, ships `packwiz-installer` for the client side) solves the same problem end-to-end. Not recommended here only because the launcher already needs its own Rust-side downloader for Java/Mojang assets — adding a second, Java-based installer (`packwiz-installer` runs on the JVM) duplicates that machinery for no real gain. Reasonable to swap in if you want to avoid writing the manifest-diff logic yourself.

### (d) Tauri 2 launcher (Rust)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| `tauri` | ^2.11 (core 2.11.5, cli 2.11.4 as of July 2026) | App shell | Current stable Tauri 2 line; pin with `^2` and let patch/minor updates flow — the 2.x API has been stable since the 2.0 GA (Oct 2024). |
| `reqwest` (features: `stream`, `rustls-tls`) | ^0.12 | Downloading Java, Mojang libraries/assets, mod files | De facto standard async HTTP client in the Rust ecosystem; `stream` feature needed for progress bars on large downloads (Java runtime, asset index). |
| `sha2` | ^0.10 | SHA-256 verification of our manifest files | Pure-Rust, no OpenSSL dependency to cross-compile for Windows/macOS. |
| `sha1` (or `sha1_smol`) | latest | Verifying Mojang's library/asset hashes | Mojang's `version.json`/asset-index still use SHA-1 — a separate crate/algorithm from the manifest hashing above; don't try to force SHA-256 onto Mojang's files. |
| `serde` + `serde_json` | ^1 | Parsing Mojang's `version_manifest_v2.json`, per-version `.json`, asset index, and our own manifest | Already a Tauri dependency; no reason to add a second JSON library. |
| `tokio` | ^1 | Async runtime | Already pulled in by Tauri/reqwest. |
| `flate2` + `tar` | latest | Extracting Temurin's macOS `.tar.gz` | Needed for the mac Java download only. |
| `zip` | ^2 | Extracting Temurin's Windows `.zip` | Needed for the Windows Java download only. |
| `directories` | ^5 | Cross-platform app-data / cache directory resolution | Avoids hand-rolling `%APPDATA%` vs `~/Library/Application Support` logic. |
| `std::process::Command` (no plugin) | stdlib | Spawning the `java` process to launch Minecraft | All launch logic lives in Rust (Tauri commands), not the WebView JS — no need for `tauri-plugin-shell`, which exists specifically for invoking shell commands *from the frontend*. |

**Java 8 acquisition (the tricky part — three different sources, not one):**

| Platform | Source | Why |
|----------|--------|-----|
| Windows x64 | Adoptium Temurin 8, `os=windows&arch=x64` via the [Adoptium API v3](https://api.adoptium.net/v3/assets/feature_releases/8/ga?image_type=jre&os=windows&architecture=x64) | Official, free, first-class support. |
| macOS x64 (Intel) | Adoptium Temurin 8, `os=mac&arch=x64` | Also officially supported by Adoptium. |
| macOS aarch64 (Apple Silicon) | **Azul Zulu 8**, via the [Azul Metadata API](https://api.azul.com/metadata/v1/zulu/packages/) (`java_version=8&os=macos&arch=arm`) | **Adoptium does not ship JDK 8 for macOS aarch64** — confirmed open, unresolved since 2021 in [adoptium/adoptium#96](https://github.com/adoptium/adoptium/issues/96) and [adoptium-support#1000](https://github.com/adoptium/adoptium-support/issues/1000). Zulu is the standard fallback everyone uses for this exact gap; it's a drop-in OpenJDK 8 build for arm64 macOS. |

Design the launcher's Java-fetch logic around **two APIs, not one** (Adoptium for Windows/mac-Intel, Azul for mac-arm64) — treating "get Java 8" as a single uniform call across all three targets is the mistake to avoid; it will silently 404 on Apple Silicon.

**Launching Forge 1.12.2 — recommended approach:** don't reimplement Forge's installer "processors"/binpatch system in Rust. Forge 1.12.2 builds from roughly 2760+ (including 2860) use the "new" installer format with a processor pipeline (patches the vanilla client jar at install time) — this is real complexity that Forge's own installer jar already solves. The pragmatic path: once Java 8 and the vanilla client jar are present, shell out **once** to the official `forge-1.12.2-14.23.5.2860-installer.jar --installClient <mc-dir>` (the installer runs headless/no-GUI when a CLI flag is supplied) to materialize a standard Mojang-format `versions/<forge-id>/<forge-id>.json` + patched client jar. From then on, the launcher reads that version JSON like any vanilla version — normal libraries list, normal classpath/main-class construction — no Forge-specific parsing needed. This mirrors what launchers like Modrinth's Tauri-based app ("Theseus") do at the metadata level (they maintain a Rust crate, `daedalus`, that parses/executes this exact processor pipeline natively — worth studying as prior art if you outgrow "shell out to the installer jar," but not needed for MVP). **Confidence: MEDIUM** — the installer's headless-CLI behavior should be spiked/verified directly against the 2860 installer jar before committing to this in a phase plan; official Forge docs on this exact CLI flag are thin.

**Mojang assets — legal/technical note already correct in PROJECT.md:** the launcher must fetch the vanilla client jar, libraries, and assets from Mojang's own endpoints (`piston-meta.mojang.com` → `version_manifest_v2.json` → per-version JSON → `libraries[].downloads` and `assetIndex.url`), never redistribute them from your own file server. Only mods/configs (part c) come from your infrastructure.

**GitHub Actions matrix:**

| Runner | Rust target | Why |
|--------|-------------|-----|
| `windows-latest` | `x86_64-pc-windows-msvc` | Standard Tauri Windows build target. |
| `macos-13` | `x86_64-apple-darwin` | GitHub's `macos-latest` runner has been Apple Silicon since 2024 — you need the explicit Intel runner (`macos-13`) to still produce an x86_64 mac build. |
| `macos-latest` (or `macos-14`) | `aarch64-apple-darwin` | Apple Silicon build. |

Use the official `tauri-apps/tauri-action` GitHub Action to drive `tauri build` per matrix leg and attach artifacts to a release — it's the maintained, documented path (vs. hand-rolling `cargo tauri build` + upload steps). **Your two existing self-hosted runners are only useful here if they are themselves Windows/macOS machines** — Tauri does not support cross-compiling a Windows or macOS bundle from a Linux host (no cross-toolchain for the OS-native bundler/signing steps); if the self-hosted runners are Linux (likely, given they're Pi/Linux-adjacent infra), keep using GitHub-hosted `windows-latest`/`macos-13`/`macos-14` for the actual builds and reserve the self-hosted runners for anything Linux-only (e.g., CI checks, the server-side auth/file service).

## Installation

```bash
# Pi 5 server host (Debian 13)
sudo mkdir -p /etc/apt/keyrings
wget -O - https://packages.adoptium.net/artifactory/api/gpg/key/public | sudo tee /etc/apt/keyrings/adoptium.asc
echo "deb [signed-by=/etc/apt/keyrings/adoptium.asc] https://packages.adoptium.net/artifactory/deb $(awk -F= '/^VERSION_CODENAME/{print$2}' /etc/os-release) main" | sudo tee /etc/apt/sources.list.d/adoptium.list
sudo apt update && sudo apt install temurin-8-jdk   # installs alongside existing Java 25

# Launcher (Rust/Tauri side)
cargo add tauri --features "" # tauri = "2" in Cargo.toml, pin ^2.11
cargo add reqwest --features stream,rustls-tls
cargo add sha2 sha1_smol serde serde_json tokio directories flate2 tar zip
npm install -D @tauri-apps/cli@^2

# Auth/file server (separate small Rust binary)
cargo add axum argon2 rusqlite tower-http --features tower-http/fs
```

## Alternatives Considered

| Category | Recommended | Alternative | Why Not (here) |
|----------|-------------|-------------|-----------------|
| Server process supervision | systemd (bare metal) | `itzg/minecraft-server` Docker (arm64) | Extra abstraction layer for a single fully-owned instance; Oracle-Linux-tag + OpenJ9 caveats to track for no real benefit at this scale. |
| Auth enforcement | Launcher auth + `whitelist.json` (offline-mode) | Drasl + authlib-injector (`online-mode=true` against own identity server) | Real cryptographic enforcement, but a second stateful service (Go + DB) for 5–7 mutually-trusted friends is more than the threat model needs. Good upgrade path, not MVP. |
| Client update transport | Static manifest (SHA-256) over HTTP | rsync / Syncthing / packwiz | Reuses the HTTP client the launcher already has for Java/Mojang downloads; no daemon, no second protocol. `packwiz` is a fine swap-in if you don't want to write the diff logic yourself. |
| Forge 1.12.2 client install | Shell out to official Forge installer jar (`--installClient`) once | Reimplement the processor/binpatch pipeline natively in Rust (à la Modrinth's `daedalus`) | Correct long-term architecture, but real complexity not justified for a single fixed Forge build (2860) that never needs to change. |
| Static/TLS server | Caddy | nginx + certbot | Automatic HTTPS with zero renewal maintenance; smaller config for the same job given a domain is already available. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|--------------|
| `itzg/minecraft-server` with an Oracle-Linux-based tag | Forge installer breaks on Oracle Linux's `zlib-ng` (documented itzg issue) | Debian-based `java8` tag, or better, skip Docker entirely (see recommendation above) |
| SimpleLogin (Forge auth mod) | Unmaintained for 1.12.2 since Dec 2021; requires bundling an extra client mod into an already-fragile pack | Launcher-side auth + `whitelist.json`, or Drasl if you need protocol-level enforcement |
| AuthMeReloaded | It's a Bukkit/Spigot plugin — does not run on a Forge server at all | Same as above |
| Adoptium Temurin 8 for macOS aarch64 | Does not exist — will 404 | Azul Zulu 8 via the Azul Metadata API |
| Adding Phosphor/VanillaFix on top of RLCraft's bundled FoamFix | Duplicate/conflicting lighting & tick optimizations on a pack that assumes its exact bundled mod set; must also update the client pack or connections break | Tune via JVM flags + `server.properties`; trust RLCraft's own bundled FoamFix |
| rsync/Syncthing for client updates | Needs a persistent daemon or SSH access on the server side, or a running peer on every client — unnecessary given the launcher already speaks HTTP | Static SHA-256 manifest + `reqwest` GET |
| Hand-rolled cross-compiled Windows/macOS builds from a Linux self-hosted runner | Tauri's OS-native bundlers (NSIS/WiX, `.app`/`.dmg` + codesigning) are not cross-compilable from Linux | GitHub-hosted `windows-latest` / `macos-13` / `macos-14` matrix via `tauri-apps/tauri-action` |

## Stack Patterns by Variant

**If you decide protocol-level auth enforcement matters (not just launcher-gated):**
- Add Drasl (self-hosted) + authlib-injector on server and launcher-launched client.
- Because whitelist-based offline-mode auth only stops casual joins, not a determined user with a vanilla client who already knows a whitelisted name.

**If you'd rather not write a Rust auth/file backend:**
- Use Caddy for static file serving (manifest + mod files) and a minimal separate script/service (any language) just for register/login, still writing to `whitelist.json`.
- Because the "one Rust binary for everything" recommendation above is an efficiency choice, not a hard requirement — the manifest/whitelist architecture doesn't care what language issues the tokens.

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|------------------|-------|
| Forge 1.12.2-14.23.5.2860 | Java 8 (any recent 8u update, 8u402+) | Forge 1.12.2 will not run on Java 11+; do not let the system Java 25 leak into `PATH` for the MC service — set `ExecStart` to the Temurin 8 binary explicitly. |
| RLCraft Server Pack v2.9.3 | Forge 1.12.2-14.23.5.2860 exactly | Do not substitute a different Forge build; RLCraft pins to this one. |
| Drasl | Minecraft/Forge 1.12.2 via authlib-injector | Native (no-agent) auth-server support is 1.16+ only; 1.12.2 requires the authlib-injector javaagent path specifically. |
| Tauri 2.11.x | Rust edition 2021+, Node 18+ for the CLI/frontend tooling | Standard current requirement, not a special constraint here. |

## Sources

- [Adoptium — Temurin releases (JDK 8, aarch64, mac filter)](https://adoptium.net/temurin/releases/?version=8&arch=aarch64&os=mac) — HIGH confidence, official vendor
- [adoptium/adoptium#96 — no Apple Silicon JDK 8/11 builds](https://github.com/adoptium/adoptium/issues/96) — HIGH, official repo issue tracker
- [adoptium-support#1000 — JDK8 macOS aarch64 unavailable](https://github.com/adoptium/adoptium-support/issues/1000) — HIGH
- [Azul Metadata API docs](https://docs.azul.com/core/install/metadata-api) — HIGH, official vendor docs
- [Forge downloads index, 1.12.2](https://files.minecraftforge.net/net/minecraftforge/forge/index_1.12.2.html) — HIGH, official
- [RLCraft Server Pack v2.9.3 (CurseForge)](https://www.curseforge.com/minecraft/modpacks/rlcraft/files/4612990) — HIGH, official pack page
- [itzg/docker-minecraft-server — Forge platform docs](https://github.com/itzg/docker-minecraft-server/blob/master/docs/types-and-platforms/server-types/forge.md) — MEDIUM, community-maintained but authoritative for this image
- [unmojang/drasl README + config docs](https://github.com/unmojang/drasl) — MEDIUM-HIGH, verified via direct fetch of README
- [SeraphJACK/SimpleLogin + CurseForge file history](https://github.com/SeraphJACK/SimpleLogin) — MEDIUM, staleness confirmed via CurseForge file dates
- [Tauri Core/CLI release pages](https://tauri.app/release/core/) — HIGH, official
- [Adoptium API v3 docs](https://github.com/adoptium/api.adoptium.net) — HIGH, official
- General knowledge of Forge 1.12.2's installer processor pipeline and Mojang's `version_manifest_v2` / SHA-1 asset scheme — MEDIUM; recommend a short spike in the launcher phase to confirm the exact `--installClient` headless behavior against the pinned 2860 installer jar before finalizing the launch flow.

---
*Stack research for: RLCraft private server + auth + launcher*
*Researched: 2026-08-27*
