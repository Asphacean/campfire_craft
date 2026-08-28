# Phase 4: Launcher (Tauri 2) - Research

**Researched:** 2026-08-28
**Domain:** Tauri 2 desktop launcher (Rust + web UI) — auth/session, Mojang+Forge bootstrap, modpack sync, JVM launch, self-update
**Confidence:** MEDIUM-HIGH — the riskiest unknown (Forge 1.12.2 headless client install) was verified empirically on this machine against the exact pinned installer build, not just read about. Tauri crate versions, MSRV, and the Mojang/Forge JSON schemas were all fetched/tested live this session. Apple Silicon rendering behavior and the exact Windows/macOS Rust toolchain state remain LOW confidence (untestable from this Linux host) and are flagged as spikes for the operator.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Screen (single window, English UI)**
- UI language: English only for now (operator decision, 2026-08-28) — no i18n framework needed, keep strings in one module for a later RU pass
- One form: nick + password fields, two buttons side by side: "Log in" and "Create account" (registration uses the same two fields)
- After successful login the form collapses to "Playing as **Nick** · Log out"; RAM slider and Play remain
- Visual style: RLCraft art — background/banner from the RLCraft client pack's own logo/art (taken from the client zip already cached on the Pi) + "campfire.pub" wordmark; if usable art can't be sourced, fall back to dark minimalism. Window ~480×560, non-resizable, accent Play button
- Top: status pill "● campfire.pub — Online · 2/10" / "Offline" (from `/status`, on start and every 15 s); Play is NOT blocked when offline
- RAM slider 3–10 GB step 0.5, default `min(8, round(total_ram/2))`, warning when > 70% of physical RAM
- Bottom: progress bar + step label ("Downloading mods 120/187 · 45 MB/s"), driven by Tauri **channels** (not the event bus)
- Errors: red inline banner under Play in plain English (wrong password, server unreachable, Java download failed, disk full, …) + "Open log" button; launcher always writes `launcher.log`
- Extra controls: "Game folder" (opens game dir), "Verify files" (force full hash re-check + repair), launcher version + pack_version in small text at the bottom

**Java / Forge / launch**
- Java 8 JRE per platform, never system Java: Windows x64 → Adoptium Temurin 8 (API v3); macOS Intel → Adoptium Temurin 8; macOS Apple Silicon → x86_64 Adoptium Temurin 8 under Rosetta for v1 (LWJGL 2 has no arm64 natives); checksum from the vendor API verified. Follow-up spike (not blocking): Azul Zulu 8 arm64 + community ARM64 LWJGL2/jinput natives for performance
- Forge client install: spike first — run official `forge-1.12.2-14.23.5.2860-installer.jar --installClient <game dir>` headless with the provisioned Java 8 → standard `versions/<forge-id>/<forge-id>.json`; launcher then treats it as a vanilla version JSON (libraries, natives, main class `net.minecraft.launchwrapper.Launch`, tweak class). Fallback if headless install fails: construct the version JSON + libraries manually per the known 1.12.2 layout
- Vanilla client jar, libraries, natives, assets always from Mojang (`version_manifest_v2.json` → 1.12.2 json → asset index; SHA-1 verified) — never from our host
- Install root: Windows `%APPDATA%\campfire\`, macOS `~/Library/Application Support/campfire/`; layout `runtime/` (Java), `game/` (Minecraft dir: mods, config, saves, …), `versions/`, `libraries/`, `assets/`, `launcher.log`
- Never touched by sync: `saves/`, `options.txt`, `optionsof.txt`, `servers.dat`, `screenshots/`, `logs/`, `resourcepacks/` user additions outside the manifest. `servers.dat` with `mc.campfire.pub` is seeded once on first run only
- Launch: `java -Xms<ram> -Xmx<ram> <Aikar-ish client flags> -Dcampfire.nick=<nick> -Dcampfire.token=<token> -Djava.library.path=<natives> -cp <classpath> net.minecraft.launchwrapper.Launch --username <nick> --uuid <offline uuid> --accessToken 0 --userType legacy --version <forge-id> --gameDir <game> --assetsDir <assets> --assetIndex 1.12 --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker`; auto-connect to the server via `--server mc.campfire.pub --port 25565` if it works with Forge 1.12.2, else rely on the seeded servers.dat
- Offline UUID = `UUID.nameUUIDFromBytes("OfflinePlayer:"+nick)` (v3), nick case preserved exactly as registered

**Session / updates / status**
- AUTH-03 via a refresh token: auth service change — `/api/login` additionally returns `refresh` (30-day, random 32 B, stored hashed, revoked by `campfire-auth reset`); new `POST /api/refresh {nick, refresh}` → fresh game token (+ rotated refresh). Launcher stores ONLY the refresh token in the OS keychain (`keyring` crate: Windows Credential Manager / macOS Keychain). Password never persisted. Caddy proxies `/api/refresh` like `/api/login`
- Each Play: refresh → game token → sync → launch. Expired/revoked refresh → form re-opens with a friendly message
- Client sync before every Play: fetch manifest → sha256 diff → download only changed/missing (≤ 4 parallel, tmp + atomic rename) → apply `delete[]` → block Play with a clear message on failure. "Verify files" = full re-hash of managed files
- Self-update (LNCH-08): Tauri updater plugin; feed `https://mc.campfire.pub:8444/launcher/latest.json` + artifacts under `/launcher/` on the Phase-3 file server; minisign signature (private key held by operator, public key embedded); check on startup, "Update now" dialog
- Status: `/status` on start + every 15 s

### Claude's Discretion
- Frontend stack inside Tauri (vanilla TS vs a small framework), exact crate versions, JVM flag set for the client, download concurrency, log rotation, how the RLCraft art asset is extracted/licensed-noted, keychain fallback when no keychain is available

### Deferred Ideas (OUT OF SCOPE)
- Russian UI / language switcher — later pass
- ARM64-native LWJGL2 for Apple Silicon — performance follow-up spike
- Skins (Drasl) — v2; multiple accounts — out of scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-------------------|
| AUTH-03 | Launcher remembers the session (token, never the plaintext password) | Refresh-token design below (auth-service schema change + `keyring` crate storage) — see "Refresh Token Design" and Security Domain |
| LNCH-01 | Single screen: nick, password, Register/Login, RAM slider, Play | UI-SPEC territory, not this file — Architectural Responsibility Map + Don't Hand-Roll (native RAM detection) inform it |
| LNCH-02 | Diff local files vs manifest, download changed/missing, delete stale, never touch saves/options/servers.dat | `scripts/assemble-client.py` is the reference implementation (read this session) — see "Manifest Sync" pattern |
| LNCH-03 | Auto-download Java 8 per platform, never system Java | Adoptium API v3 + Rosetta path — see "Java 8 Acquisition" |
| LNCH-04 | Install/construct Forge 1.12.2 client, launch with RAM + token, auto-connect | Empirically verified Forge installer behavior — see "Forge 1.12.2 Client Install — Verified" |
| LNCH-05 | Progress (step + file count/bytes) | Tauri `ipc::Channel` pattern — see "Code Examples" |
| LNCH-06 | Human-readable errors + log pointer | Pitfall 8/9 mitigations — see "Common Pitfalls" |
| LNCH-07 | Server status (online/offline, player count) | `/status` contract already read from `auth-service/README.md` — trivial `reqwest` GET, no new research needed |
| LNCH-08 | Self-update on startup (Tauri updater) | `tauri-plugin-updater` + minisign — see "Self-Update" |
</phase_requirements>

## Summary

This phase has one genuinely hard unknown and it has now been resolved empirically, not by reading forum posts: **the official Forge 1.12.2-14.23.5.2860 installer's `--installClient <dir>` flag runs fully headlessly — no X11/display needed at all — provided the target directory already contains a `launcher_profiles.json` stub file.** Without that stub file it fails cleanly (`There is no minecraft launcher profile in "<dir>", you need to run the launcher first!`, exit 0, no crash) rather than doing anything destructive. This was verified live on this Pi against the exact pinned Forge build, downloading the real client.jar from Mojang and the real Forge/Scala/akka/log4j libraries from Forge's and Mojang's library CDNs. The resulting `versions/1.12.2-forge-14.23.5.2860/1.12.2-forge-14.23.5.2860.json` is a completely ordinary Mojang-shaped version JSON (`inheritsFrom: "1.12.2"`, `mainClass: net.minecraft.launchwrapper.Launch`, tweak class embedded in `minecraftArguments`) with **no processor/binpatch metadata surviving into the final file** — confirming the launcher never needs to reimplement Forge's install-time patching pipeline. One library entry (Forge's own jar) has an empty `download.url` because the installer extracts it from its own embedded copy rather than downloading it — the classpath/download builder must special-case that (skip fetch, trust the file the installer already wrote).

The second major finding is a hard environment blocker, also verified empirically on this machine: **a bare `tauri = "2.11"` dependency (nothing else) will not `cargo check` on this Pi's rustc 1.85.0** — the actual transitive MSRV (via `icu_*`, `plist`, `time`, `darling`) is rustc **1.88**, two full minor versions above the Debian-trixie-packaged toolchain, despite Tauri's own declared `rust-version = "1.77.2"`. This is not a Linux-only or plugin-only problem; it reproduces with zero plugins. `rustup` is not installed on this Pi; only the apt-packaged `cargo`/`rustc` 1.85.0 exists. A Wave-0 task must install `rustup` and pin a toolchain ≥1.88 (current stable is ~1.97 as of this research date) scoped to the launcher crate, without touching the apt toolchain the already-working `auth-service` crate builds against (confirmed unaffected — `auth-service` still builds clean under the apt 1.85 toolchain).

Third: there is no evidence anywhere (official docs, Forge forums, or the actual legacy `minecraftArguments` template pulled live from Mojang) of a working native `--server`/`--port` autoconnect flag for a 1.12.2/Forge client — the vanilla legacy argument template only exposes `--username --version --gameDir --assetsDir --assetIndex --uuid --accessToken --userType --versionType`, and Forge's own template adds only `--tweakClass`/`--versionType`. CONTEXT.md's own fallback (seed `servers.dat` on first run, never rely on a CLI autoconnect) should be treated as the **primary** mechanism, not a fallback — don't spend execution time chasing a flag that doesn't exist for this version.

**Primary recommendation:** Vanilla TypeScript + Vite frontend (no framework — one screen, no routing, no component reuse need); Rust-side does everything else (`reqwest` for downloads, `keyring` v3 for the refresh token, `tauri::ipc::Channel` for progress). Treat the Forge-install and MSRV findings above as settled (not spikes) — the CONTEXT.md-mandated "spike first" for the Forge installer is satisfied by this research; what remains as genuine operator-side spikes is Apple Silicon rendering (Rosetta path) and the Windows/macOS local build environments, neither of which this Linux host can test.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Login/register form, RAM slider, status pill, progress bar | Browser/Client (Tauri WebView) | — | Pure presentation; no business logic belongs here |
| Session token exchange, refresh rotation | Rust launcher core (Tauri commands) | API/Backend (auth-service) | The WebView never sees the raw password or holds long-lived secrets; Rust commands call the auth-service HTTPS API and store only the OS-keychain-backed refresh token |
| Manifest diff/download, Java provisioning, Forge install, JVM launch | Rust launcher core | — | All filesystem, network, and process-spawn work must live in Rust — `std::process::Command`, `reqwest`, no `tauri-plugin-shell` needed since nothing is invoked from JS |
| Account existence, password hashing, token issuance/validation, refresh-token rotation | API/Backend (`auth-service`, axum) | — | Already built (Phase 2); this phase only adds `/api/refresh` and a `refresh_tokens` table — no auth logic duplicated in the launcher |
| Manifest content, mod/config files, self-update artifacts | CDN/Static (Caddy `file_server` on the Pi) | — | Already built (Phase 3); this phase is a pure consumer, adds one new proxied route (`/api/refresh`) and one new static tree (`/launcher/`) |
| Refresh token storage (hashed), account rows | Database/Storage (SQLite via `auth-service`) | — | Same `campfire.db`, new `refresh_tokens` table — no new datastore |
| Auth-gate join enforcement | API/Backend (server-side Forge mod, already built) | — | Out of this phase's scope entirely — the launcher only ever sets `-Dcampfire.nick`/`-Dcampfire.token`, `ClientAuthHandler` (already read this session) does the rest client-side |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tauri` | `2.11` (2.11.5 on crates.io) [VERIFIED: crates.io registry, via `cargo search`] | App shell | Current stable Tauri 2 line |
| `@tauri-apps/cli` | `2.11.4` [VERIFIED: npm registry, `npm view @tauri-apps/cli version`] | Build/dev CLI | Matches the Rust `tauri` crate's 2.11 line |
| `@tauri-apps/api` | `2.11.1` [VERIFIED: npm registry] | Frontend JS bindings (`invoke`, `Channel`) | Official, required for any Tauri v2 frontend |
| `tauri-plugin-updater` | Rust `2.10.1`, npm `@tauri-apps/plugin-updater` `2.10.1` [VERIFIED: crates.io + npm registry] | LNCH-08 self-update | Official Tauri plugin, only maintained option |
| `tauri-plugin-opener` | Rust `2.5.4`, npm `@tauri-apps/plugin-opener` `2.5.4` [VERIFIED: crates.io + npm registry] | "Game folder" button (`reveal_item_in_dir`), "Open log" button | Official; replaces the deprecated `tauri-plugin-shell`-based file-opening pattern |
| `reqwest` | `^0.13` (0.13.4 current) [VERIFIED: crates.io registry, via `cargo search`; STACK.md's `^0.12` recommendation from 2026-08-27 is one minor behind current] | Manifest/Mojang/Java/Forge/update downloads | De facto async HTTP client; `stream` feature for progress |
| `sha2` | `^0.10` | Manifest sha256 verification | Pure-Rust, no OpenSSL cross-compile pain |
| `sha1_smol` (or `sha1`) | latest | Mojang/Forge library sha1 verification | Mojang's own manifests use sha1, not sha256 — do not conflate the two hash domains |
| `serde` / `serde_json` | `^1` | Parsing every JSON contract in this phase | Already a Tauri dependency |
| `tokio` | `^1` | Async runtime | Already pulled in by Tauri/reqwest |
| `directories` | `^5` (or `^6`) | `%APPDATA%`/`~/Library/Application Support` resolution | Avoids hand-rolling per-OS path logic |
| `zip` | `^2` | Extract Temurin Windows JRE `.zip` | — |
| `flate2` + `tar` | latest | Extract Temurin/Zulu macOS JRE `.tar.gz` | — |
| `keyring` | `^3` (current major on crates.io is actually `4.1.6` [VERIFIED: `cargo search keyring`] — recommend pinning `^3` anyway, see note below) | AUTH-03 refresh-token storage | Windows Credential Manager / macOS Keychain backends |

**Why pin `keyring` at `^3` when `4.x` is current:** v4 replaced `Entry::new_with_target` with a modifiers-map API and moved credential stores into separate crates (`keyring-core` + per-platform store crates) [CITED: github.com/open-source-cooperative/keyring-rs releases]. The basic `Entry::new(service, username).set_password()/get_password()` surface this phase actually needs is unchanged across the churn, but v3 (712k downloads/week [VERIFIED: package-legitimacy check]) is the version every existing tutorial/example targets, and this phase has exactly one call site — there is no reason to absorb v4's API reshuffle for zero functional gain. If the planner prefers to track current, `^4` with the `windows-native`/`apple-native` features is equally valid; just don't mix v3 tutorial code with a v4 `Cargo.toml`.

**No `tauri-plugin-shell` needed anywhere** — all process spawning (the `java` launch, `xvfb-run` if ever used) happens from Rust Tauri commands via `std::process::Command`, never invoked from the frontend, so the shell plugin (which exists specifically for frontend-initiated shell calls) is out of scope. [ASSUMED — matches STACK.md's existing recommendation, not independently re-verified this session beyond confirming the plugin still exists on crates.io.]

### Frontend

| Choice | Recommendation | Why |
|--------|-----------------|-----|
| Framework | **Vanilla TypeScript + Vite** | One screen, no routing, no list rendering, no component reuse — a framework (Svelte/React) adds a build-config surface and a dependency for zero benefit here. `vite` `8.2.2` [VERIFIED: npm registry] as the dev server/bundler is the standard Tauri frontend tool regardless of framework choice. |
| State management | Plain module-level TS variables + `@tauri-apps/api` `Channel`/`invoke` calls | The whole UI state (logged-in nick, RAM value, progress, status) fits in a handful of variables — a store library is unjustified |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Rust-side manifest diff (port `assemble-client.py`'s logic) | Reuse Python via a bundled interpreter | Would require shipping/embedding a Python runtime inside the launcher bundle — strictly worse than a ~150-line Rust port of logic that's already read and understood this session |
| `keyring` crate for refresh-token storage | Encrypted file on disk | OS keychain is the documented CONTEXT.md decision and the correct pattern (Architecture Anti-Pattern 2 in ARCHITECTURE.md warns explicitly against plaintext/encrypted-file token persistence) |
| Shell out to Forge's official installer once | Reimplement Forge's processor/binpatch pipeline natively (à la Modrinth's `daedalus`) | Correct long-term architecture for a launcher supporting many Forge versions; unjustified for one pinned build (2860) that never changes |

**Installation:**
```bash
# Rust side (src-tauri/Cargo.toml) — run under a rustup toolchain >=1.88, see Pitfall "MSRV" below
cargo add tauri --features ""
cargo add tauri-plugin-updater tauri-plugin-opener
cargo add reqwest --features stream,rustls-tls
cargo add sha2 sha1_smol serde serde_json tokio directories flate2 tar zip
cargo add keyring --features apple-native,windows-native,sync-secret-service

# Frontend (repo root or launcher/)
npm install -D @tauri-apps/cli@^2 vite typescript
npm install @tauri-apps/api @tauri-apps/plugin-updater @tauri-apps/plugin-opener
```

## Package Legitimacy Audit

All packages checked via `gsd-tools query package-legitimacy check` against the npm and crates.io registries this session.

| Package | Registry | Age | Downloads/wk | Source Repo | Verdict | Disposition |
|---------|----------|-----|--------------|-------------|---------|-------------|
| `@tauri-apps/cli` | npm | published 2026-06-28 | 2,266,205 | github.com/tauri-apps/tauri | OK | Approved |
| `@tauri-apps/api` | npm | published 2026-06-17 | 2,464,012 | github.com/tauri-apps/tauri | OK | Approved |
| `@tauri-apps/plugin-updater` | npm | published 2026-04-04 | 701,026 | github.com/tauri-apps/plugins-workspace | OK | Approved |
| `@tauri-apps/plugin-opener` | npm | published 2026-05-02 | 1,008,103 | github.com/tauri-apps/plugins-workspace | OK | Approved |
| `typescript` | npm | published 2026-07-08 | 275,912,226 | github.com/microsoft/TypeScript | OK | Approved |
| `vite` | npm | published 2026-08-20 | 175,869,000 | github.com/vitejs/vite | **SUS** (reason: "too-new" — latest patch release is recent) | Flagged by the heuristic on release recency alone; 175M weekly downloads and a 2016-origin, well-known maintainer org make this almost certainly a false positive. Planner should still add a `checkpoint:human-verify` before first install per protocol, but no substantive concern exists. |
| `tauri` (crate) | crates.io | first published 2019-11-27 | 816,532 | github.com/tauri-apps/tauri | OK | Approved |
| `tauri-plugin-updater` (crate) | crates.io | first published 2023-05-24 | 315,785 | github.com/tauri-apps/plugins-workspace | OK | Approved |
| `tauri-plugin-opener` (crate) | crates.io | first published 2024-11-11 | 327,974 | github.com/tauri-apps/plugins-workspace | OK | Approved |
| `reqwest` | crates.io | first published 2016-10-16 | 13,258,323 | github.com/seanmonstar/reqwest | OK | Approved |
| `sha2` | crates.io | first published 2016-05-06 | 18,055,326 | github.com/RustCrypto/hashes | OK | Approved |
| `sha1_smol` | crates.io | first published 2022-01-16 | 3,402,578 | github.com/mitsuhiko/sha1-smol | OK | Approved |
| `serde` / `serde_json` | crates.io | first published 2014/2015 | 22M+ each | github.com/serde-rs/* | OK | Approved |
| `tokio` | crates.io | first published 2016-07-01 | 16,489,235 | github.com/tokio-rs/tokio | OK | Approved |
| `directories` | crates.io | first published 2017-12-18 | 931,715 | github.com/soc/directories-rs | OK | Approved |
| `flate2` | crates.io | first published 2014-11-11 | 11,222,335 | github.com/rust-lang/flate2-rs | OK | Approved |
| `tar` | crates.io | first published 2014-11-11 | 3,862,621 | github.com/composefs/tar-rs | OK | Approved |
| `zip` | crates.io | first published 2014-11-21 | 5,016,874 | github.com/zip-rs/zip2 | OK | Approved |
| `keyring` | crates.io | first published 2016-02-10 | 712,071 | github.com/open-source-cooperative/keyring-rs | OK | Approved |

No `postinstall` scripts found on any checked npm package (checked via `npm view <pkg> scripts.postinstall`, all empty). No SLOP-verdict packages.

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** `vite` (see note above — near-certain false positive, but still gate behind `checkpoint:human-verify` per protocol).

## Architecture Patterns

### System Architecture Diagram

```
┌─────────────────────────── Tauri Launcher (Rust core) ───────────────────────────┐
│                                                                                     │
│  [WebView: login form, RAM slider, Play, status pill, progress bar]               │
│         │ invoke() / Channel                                                       │
│         ▼                                                                          │
│  ┌─────────────────┐   HTTPS (pinned CA)   ┌───────────────────────────────────┐  │
│  │ Auth flow        │──────────────────────▶│ auth-service (via Caddy :8444)    │  │
│  │ login/register/  │◀──────────────────────│ /api/register /login /refresh     │  │
│  │ refresh           │  token/refresh        │ /status                           │  │
│  └────────┬──────────┘                       └───────────────────────────────────┘  │
│           │ store refresh token only                                               │
│           ▼                                                                        │
│  ┌─────────────────┐                                                               │
│  │ OS Keychain      │  (keyring crate: Credential Manager / Keychain)              │
│  └──────────────────┘                                                              │
│                                                                                     │
│  ┌─────────────────┐   HTTPS (pinned CA)   ┌───────────────────────────────────┐  │
│  │ Manifest sync     │──────────────────────▶│ Caddy /manifest.json, /pack/*     │  │
│  │ sha256 diff       │◀──────────────────────│ (Phase 3, already built)          │  │
│  └────────┬──────────┘                                                             │
│           │ tmp + atomic rename, delete[]                                          │
│           ▼                                                                        │
│  ┌───────────────────────────────────────────────────────────────────────────┐    │
│  │ game/ (Minecraft dir), never touches saves/ options.txt servers.dat        │    │
│  └───────────────────────────────────────────────────────────────────────────┘    │
│                                                                                     │
│  ┌─────────────────┐   HTTPS (Mojang CDN)  ┌───────────────────────────────────┐  │
│  │ Vanilla bootstrap │──────────────────────▶│ piston-meta / piston-data /        │  │
│  │ (version_manifest,│◀──────────────────────│ resources.download.minecraft.net  │  │
│  │  client.jar, libs,│                        └───────────────────────────────────┘  │
│  │  assets)          │                                                              │
│  └────────┬──────────┘                                                             │
│           │                                                                        │
│  ┌─────────────────┐  local, once  ┌────────────────────────────────────────┐    │
│  │ Forge installer   │──────────────▶│ versions/1.12.2-forge-…/…json (produced) │    │
│  │ (shelled out to)  │  headless      │ libraries/net/minecraftforge/forge/…jar   │    │
│  └────────┬──────────┘                └────────────────────────────────────────┘    │
│           │                                                                        │
│  ┌─────────────────┐  HTTPS (vendor API)  ┌────────────────────────────────────┐  │
│  │ Java 8 provision  │──────────────────────▶│ Adoptium API v3 (Win/mac-Intel)   │  │
│  │                   │                        │ Azul/Rosetta path (mac-arm64)     │  │
│  └────────┬──────────┘                        └────────────────────────────────┘  │
│           │                                                                        │
│           ▼                                                                        │
│  ┌───────────────────────────────────────────────────────────────────────────┐    │
│  │ Classpath + JVM-arg builder → std::process::Command spawn                  │    │
│  │ java -Xmx<ram> ... -Dcampfire.nick=<nick> -Dcampfire.token=<token> \        │    │
│  │      -cp <classpath> net.minecraft.launchwrapper.Launch --tweakClass ...    │    │
│  └───────────────────────────────────────────────────────────────────────────┘    │
│                                                                                     │
│  ┌─────────────────┐   HTTPS   ┌────────────────────────────────────────────┐    │
│  │ Self-updater      │──────────▶│ /launcher/latest.json + artifacts (Caddy)  │    │
│  │ (checks on start) │  minisign  │                                             │    │
│  └───────────────────┘           └────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                        │ TCP 25565 (raw MC protocol, spawned java process)
                                        ▼
                              [Forge 1.12.2 server + auth-gate mod]
```

### Recommended Project Structure
```
launcher/
├── src/                      # Frontend: vanilla TS + Vite, one entry point
│   ├── main.ts               # form state, invoke() calls, Channel listeners
│   ├── strings.ts            # all UI copy, centralized for the future RU pass
│   └── style.css
├── src-tauri/
│   ├── src/
│   │   ├── main.rs
│   │   ├── auth.rs           # login/register/refresh HTTP calls, keyring storage
│   │   ├── manifest.rs       # fetch + sha256 diff + download + delete[], ports assemble-client.py
│   │   ├── mojang.rs         # version_manifest_v2 → version json → asset index, sha1 verify
│   │   ├── forge.rs          # shell out to installer, merge version JSON with parent
│   │   ├── java.rs           # per-platform Adoptium/Azul fetch + extract + Rosetta detect
│   │   ├── launch.rs         # classpath builder, JVM arg builder, process spawn
│   │   └── progress.rs       # Channel event types shared with the frontend
│   ├── icons/
│   ├── ca/campfire-ca.pem    # embedded via include_bytes!, copied from repo ca/ at build time
│   └── tauri.conf.json       # updater plugin config (pubkey, endpoint)
└── package.json
```

### Pattern 1: Manifest Sync (ports `scripts/assemble-client.py`)

**What:** Fetch `manifest.json` over the pinned-CA HTTPS client, validate every entry's `path`/`url` against a path-traversal guard (reject absolute paths, `..` components, control characters, anything resolving outside the install dir), reject any entry under `libraries/`, `assets/`, `versions/` or named like the vanilla client jar (DIST-03 enforcement — this file already asserts our host never serves those), then for each `files[]` entry: if present locally with matching size+sha256, skip; else download to a tmp file in the same directory and `rename()` into place, verifying sha256 before the rename. Apply `delete[]` for any locally-present path in the list.

**When to use:** Every "Play" press, before launch. "Verify files" runs the same code with the download step forced/re-checked, matching `assemble-client.py --verify`'s semantics.

**Example (the exact reference logic, already read this session — port faithfully, don't reinvent):**
```python
# Source: scripts/assemble-client.py (this repo, read 2026-08-28) — path-guard logic
for field in ("path", "url"):
    value = entry.get(field, "")
    if os.path.isabs(value): reject()
    if ".." in value.split("/"): reject()
    if any(ord(c) < 0x20 or ord(c) == 0x7F for c in value): reject()
    real_path = os.path.realpath(os.path.join(dest, value))
    if os.path.commonpath([real_dest, real_path]) != real_dest: reject()
```
`FORBIDDEN_PREFIXES = ("libraries/", "assets/", "versions/")` and `looks_like_minecraft_client_jar()` (basename starts with "minecraft", ends ".jar") are the DIST-03 gate — the Rust port must keep both checks, not just the path-traversal guard.

### Pattern 2: Forge 1.12.2 Client Install — Verified This Session

**What:** Shell out once to the pinned `forge-1.12.2-14.23.5.2860-installer.jar` with `--installClient <game-dir>`, using the provisioned Java 8. **Verified live on this Pi** (Temurin 8, `/opt/temurin-8/jdk8u504-b01/bin/java`, no X11 display active, exit code 0 both times):

1. `java -jar forge-installer.jar --installClient <dir>` **fails** on a fresh directory with `There is no minecraft launcher profile in "<dir>", you need to run the launcher first!` and prints `There was an error during installation` — but does **not** crash, does **not** need a display, and exits with the same code as a normal run (the installer's own error path, not an X11/AWT failure).
2. Pre-seeding a minimal `launcher_profiles.json` in the target directory — `{"profiles":{},"selectedProfile":"","clientToken":"00000000-0000-0000-0000-000000000000","authenticationDatabase":{}}` — makes the **exact same command succeed completely headlessly**: it downloads `client.jar` directly from `piston-data.mojang.com` (confirmed live: `https://piston-data.mojang.com/v1/objects/0f275bc1547d01fa5f56ba34bdc87d981ee12daf/client.jar`), extracts Forge's own jar from inside the installer (not downloaded — see below), downloads ~18 Forge-side libraries (asm, launchwrapper, jline, akka, scala-*, log4j) from `maven.minecraftforge.net`/`libraries.minecraft.net`, runs a "Building Processors" step, and finishes with `Successfully installed client into launcher.`
3. The produced `versions/1.12.2-forge-14.23.5.2860/1.12.2-forge-14.23.5.2860.json` [VERIFIED: this exact file, generated live this session — quoted fields below] is a plain Mojang-shaped version JSON:
   ```json
   {
     "id": "1.12.2-forge-14.23.5.2860",
     "mainClass": "net.minecraft.launchwrapper.Launch",
     "inheritsFrom": "1.12.2",
     "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userType ${user_type} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker --versionType Forge",
     "libraries": [ /* 21 entries, see below */ ]
   }
   ```
4. **The Forge library entry has an empty download URL** — `{"name": "net.minecraftforge:forge:1.12.2-14.23.5.2860", "downloads": {"artifact": {"path": "net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860.jar", "url": "", "sha1": "029250575d3aa2cf80b56dffb66238a1eeaea2ac", "size": 4466148}}}` — this jar is written directly to `libraries/` by the installer (extracted from inside the installer jar itself; sha1 confirmed to match by recomputing it on the extracted file this session). **The classpath/download builder must special-case `url == ""`: never attempt to fetch it, only check it already exists at `path` with the expected sha1.** Every other library in this list has a real `url` on Forge's or Mojang's own library CDN and a real sha1 — none of them are self-hosted.
5. **No processor/binpatch metadata survives into the final JSON.** Whatever the installer's "Building Processors" step does at install time, the launcher's job afterward is identical to loading any vanilla-shaped version JSON: merge `libraries`/`assetIndex`/`downloads.client` from the `inheritsFrom` parent (`1.12.2`), build the classpath, spawn `java`. Nothing Forge-specific needs to be interpreted at launch time.
6. **No `--server`/`--port` placeholder exists anywhere** in either the vanilla 1.12.2 `minecraftArguments` template [VERIFIED: fetched live from `piston-meta.mojang.com` this session — `"--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userType ${user_type} --versionType ${version_type}"`] or the Forge version's template above. **There is no native CLI autoconnect for this version** — treat CONTEXT.md's `servers.dat` seeding as the primary mechanism, not a fallback to try after a CLI flag.
7. The installer's own `_comment_` field politely asks automated tooling not to bypass its download page's ad revenue [CITED: the installer's own embedded JSON, read live this session] — not a technical blocker, but worth a one-line acknowledgment in the plan; this project's use (a private, closed friend group, one-time install per player) is a reasonable, low-volume case, not the scraping this note is aimed at.
8. **For CI/headless Linux builds specifically** (not this phase's target platforms, but relevant to the Pi smoke test): a documented GitHub issue confirms the *interactive* installer path (no CLI flag at all) crashes on true headless Linux with `No X11 DISPLAY variable was set` [CITED: github.com/MinecraftForge/MinecraftForge/issues/5478, reporting this exact behavior as of 14.23.5.2815]. This session's test shows that passing `--installClient` (a recognized CLI flag) avoids that path entirely — the crash only happens when the installer falls through to its GUI wizard. On the operator's real Windows/macOS desktops (interactive session, real display available) this is a non-issue either way.

**When to use:** Once per fresh install and again whenever the pinned Forge/MC version changes (never, for this project — the version is fixed). Re-running the installer is idempotent (it re-verifies/re-extracts) but unnecessary once the version JSON exists — the launcher should check for `versions/<forge-id>/<forge-id>.json` first and skip the installer entirely if already present.

**Fallback (only if the operator's real Windows/macOS run of this exact sequence somehow fails, which this session's Linux test did not suggest):** construct the version JSON and library list by hand from the known 1.12.2-14.23.5.2860 layout captured verbatim above — the full 21-library list and exact URLs/sha1s are already in the plan's working set from this test run.

### Pattern 3: Java 8 Acquisition — Two Vendors, Three Platform Targets

| Platform | Source | Endpoint (verified live this session) |
|----------|--------|----------------------------------------|
| Windows x64 | Adoptium Temurin 8 | `https://api.adoptium.net/v3/assets/feature_releases/8/ga?image_type=jre&os=windows&architecture=x64&vendor=eclipse` — returns `binaries[].package.{link,checksum}` (a `.zip`) and `binaries[].installer` (a `.msi`); this session's live fetch returned `jdk8u504-b01`, checksum `82e2cdc6...` for the `.zip` [VERIFIED: fetched live from api.adoptium.net] |
| macOS Intel | Adoptium Temurin 8 | Same endpoint, `os=mac&architecture=x64` |
| macOS Apple Silicon | x86_64 Adoptium Temurin 8 **run under Rosetta** (CONTEXT.md's locked v1 decision — Zulu arm64-native is explicitly deferred) | Fetch the `os=mac&architecture=x64` build (same as Intel), launch under Rosetta rather than fetching an arm64 JRE at all — simpler than the Azul-arm64 path STACK.md described, and matches the locked decision exactly |

Prefer `binaries[].package.link` (the archive) over `binaries[].installer.link` (a platform installer/MSI) — the launcher needs a directory it can silently extract, not something that runs an installer UI.

**Rosetta detection:** call `sysctlbyname("sysctl.proc_translated", ...)` — returns `0` for a native arm64 process, `1` when running translated under Rosetta, `-1` on error/non-Apple-Silicon [CITED: multiple corroborating sources on the standard `sysctl.proc_translated` technique]. Use this (or simply attempt to launch and check the exit/error) to detect whether Rosetta itself is installed before relying on it; if absent, prompt to run `softwareupdate --install-rosetta --agree-to-license` (may require the user to confirm, cannot be silently forced without agreement).

### Anti-Patterns to Avoid
- **Downloading Java/Mojang assets with one hardcoded URL template across all three platforms** — Pitfall research already flagged this (STACK.md/PITFALLS.md); confirmed still correct: Adoptium's endpoint shape genuinely differs only by `os`/`architecture` query params, so a small per-platform table (not a single template) is the right shape, and this phase's locked decision to run x64-under-Rosetta on Apple Silicon sidesteps the "Adoptium has no arm64 Java 8" gap entirely rather than needing a third vendor.
- **Re-running the Forge installer on every launch** — it's a one-time step; gate it behind "does `versions/<forge-id>/<forge-id>.json` already exist."
- **Treating the empty-`url` Forge library entry as a bug** — it's intentional (extracted from the installer, not fetched); don't "fix" the download builder to error on it.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| OS credential storage for the refresh token | Custom encrypted file + hand-rolled key derivation | `keyring` crate (Credential Manager / Keychain) | This is exactly the problem the crate exists for; a hand-rolled file is the Anti-Pattern 2 mistake ARCHITECTURE.md already flags |
| Self-update distribution + signature verification | Custom update-check + custom signing scheme | `tauri-plugin-updater` + `tauri signer generate` (minisign) | Official, maintained, and the `latest.json` schema (`platforms.windows-x86_64/darwin-x86_64/darwin-aarch64`, each `{url, signature}`) is a settled contract [VERIFIED: fetched live from v2.tauri.app this session] |
| Download progress streaming to the UI | Polling a shared variable, or the general Tauri event bus at high frequency | `tauri::ipc::Channel` | Tauri's own docs recommend channels specifically for this; the event bus is documented as unsuited to high-frequency progress (already noted in PITFALLS.md Pitfall 8, independently corroborated this session) |
| Cross-platform app-data directory resolution | `if cfg!(windows) { ... } else if cfg!(target_os = "macos") { ... }` scattered through the codebase | `directories` crate | One well-tested crate vs. hand-maintained per-OS path logic in every call site |
| Forge 1.12.2's install-time binpatch/processor pipeline | Reimplementing FML's `ClassPatchManager`/processor logic in Rust | Shell out to the official installer once (Pattern 2 above) | Verified this session: the pipeline runs once at install and leaves no artifact the launcher needs to reinterpret — reimplementing it would be pure waste for one pinned Forge build |

**Key insight:** every "hard" part of this phase (Forge install, OS keychain, signed self-update, progress streaming) already has an official or de-facto-standard tool. The actual engineering work is wiring them together correctly, not inventing new mechanisms.

## Common Pitfalls

### Pitfall 1: rustc 1.85 (this Pi's toolchain) cannot build Tauri 2.11 at all

**What goes wrong:** `cargo check` on a bare `tauri = "2.11"` dependency fails outright with a wall of `requires rustc 1.88`/`1.87`/`1.86` errors from transitive crates (`icu_collections`, `icu_normalizer`, `icu_properties`, `icu_provider`, `plist`, `serde_with`, `time`, `time-core`, `time-macros`, `darling`, `zbus` and friends), even though Tauri's own declared `rust-version` is `1.77.2`.

**Why it happens:** Tauri 2.11's dependency tree includes crates (Unicode/ICU data, `time`, macOS `plist` parsing used by `tauri-build` for bundle metadata regardless of host OS, `zbus` for the Linux secret-service keyring backend) whose own MSRVs have moved past what Tauri's manifest declares — Cargo does not transitively enforce MSRV consistency, so the mismatch only surfaces as a hard compile error, not a warning at `cargo add` time.

**How to avoid:**
- Install `rustup` (not present on this Pi as of this research — only apt's `cargo`/`rustc` 1.85.0 exist) and add a stable toolchain ≥1.88 (current stable was ~1.97 at research time) **scoped to the launcher crate only**, e.g. a `rust-toolchain.toml` inside `launcher/src-tauri/` pinning `channel = "stable"` or an explicit ≥1.88 version — this does not disturb the apt toolchain the already-working `auth-service` crate builds against (confirmed this session: `auth-service` still builds clean under the apt 1.85 toolchain untouched).
- Do this as a Wave 0 task before any other launcher Rust code is written — every subsequent `cargo check`/`cargo build` command in the plan depends on it.

**Warning signs:** `error: rustc 1.85.0 is not supported by the following packages` on the very first `cargo check`.

**Verified:** empirically, this session, on this exact Pi (`rustc 1.85.0 (4d91de4e4 2025-02-17)`).

### Pitfall 2: Assuming Forge's headless install "just needs a CLI flag"

**What goes wrong:** STACK.md's prior research (2026-08-27) characterized `--installClient` as working headlessly "when a CLI flag is supplied" with MEDIUM confidence and flagged it for a spike. The spike is done (see Pattern 2 above) — the flag alone is *not* sufficient; the installer also silently requires a `launcher_profiles.json` stub to already exist in the target directory, or it fails with a specific, easily-misread error.

**How to avoid:** Have the launcher write the minimal stub JSON (`{"profiles":{},"selectedProfile":"","clientToken":"00000000-0000-0000-0000-000000000000","authenticationDatabase":{}}`) into the game directory before invoking the installer, every time (idempotent — the installer will overwrite/append to it).

**Warning signs:** installer exits 0 but prints `There was an error during installation` and no `versions/<forge-id>/` directory appears.

### Pitfall 3: LWJGL 2 / Apple Silicon rendering (carried forward from PITFALLS.md, not re-verified this session — no Apple Silicon hardware available on this host)

Unchanged from the existing project research: LWJGL 2.9.4 predates ARM64 desktop chips; CONTEXT.md's locked v1 decision is x86_64 Temurin under Rosetta (not a native ARM64 LWJGL2 swap, which is explicitly deferred). **This must be spiked on real Apple Silicon hardware by the operator** — nothing on this Linux host can validate rendering/framerate behavior. Detect Rosetta via `sysctl.proc_translated` (Pattern 3 above) before relying on it; if Rosetta itself isn't installed, the launcher must detect this and prompt, not silently fail.

### Pitfall 4: Unsigned Windows/macOS binaries (carried forward — packaging is Phase 5's problem, not this phase's)

Not this phase's blocker (REL-01/02/03 are Phase 5), but worth flagging early: this phase must produce a launcher that **builds and runs from source** on the operator's Windows and Apple Silicon machines (per CONTEXT.md's phase boundary) — signing/notarization is explicitly out of scope here. Don't let Phase 4 tasks drift into packaging work.

### Pitfall 5: Manifest race / half-updated client (carried forward from PITFALLS.md Pitfall 9)

Pin the manifest for the whole sync run (don't re-fetch mid-download), verify every file's hash against that pinned manifest before considering the sync complete, write to tmp + atomic rename. `scripts/assemble-client.py` already implements this correctly — port it, don't redesign it.

### Pitfall 6: `vite` "SUS" package-legitimacy flag is very likely a false positive, but don't skip the checkpoint

The legitimacy check flags `vite` `8.2.2` as `SUS` purely because its latest release is recent — 175M weekly downloads and a well-known GitHub org make this a near-certain false positive, but the planner must still gate the `npm install vite` step behind a `checkpoint:human-verify` per the Package Legitimacy protocol rather than silently overriding the verdict.

## Refresh Token Design (AUTH-03)

The existing `auth-service` schema [VERIFIED: read `auth-service/src/db.rs` this session] has `users(id, nick, nick_lower, pw_hash, created_at)` and `tokens(id, user_id, token_hash, expires_at, consumed_at, created_at)` — the game-session token table. CONTEXT.md's locked design adds a **separate, longer-lived** refresh token, not a change to the existing 12-hour game token (`TOKEN_TTL_SECS: i64 = 12 * 60 * 60` [VERIFIED: `auth-service/src/api.rs:65`]) or its single-use consumption semantics (`consume_token`'s `consumed_at IS NULL` compare-and-swap [VERIFIED: `auth-service/src/db.rs:222-229`] — this must NOT change, `/validate`'s existing single-use guarantee is load-bearing for the auth-gate mod).

**New table** (additive migration, `CREATE TABLE IF NOT EXISTS`, following the existing `Db::open` pattern):
```sql
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id           INTEGER PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users(id),
    token_hash   TEXT NOT NULL,
    expires_at   INTEGER NOT NULL,
    revoked_at   INTEGER,
    created_at   INTEGER NOT NULL
);
```
Mirror the existing `tokens` table's pattern exactly: store only the argon2id hash (never the raw value) [pattern already established for game tokens via `auth::hash_secret`, read this session], 30-day TTL per CONTEXT.md, rotate on every use (issue a new refresh token + revoke the old one whenever `/api/refresh` is called, not just on expiry — limits the blast radius of a stolen refresh token to one un-rotated use).

**New endpoint `POST /api/refresh {nick, refresh}` → `{token, expires, refresh}`** (rotated): follows the exact same shape as the existing `login()` handler in `auth-service/src/api.rs` — validate the presented refresh token the same way `/validate` validates game tokens (argon2-verify against `candidate_tokens`-style query, compare-and-swap consume), then mint a fresh game token via the same path `login()` already uses, plus a fresh refresh token via the new table. `campfire-auth reset <nick>` (already implemented, read this session) should also revoke all outstanding refresh tokens for that nick — a password reset must invalidate "remember me" state, not just future logins.

**Caddy change** (additive, mirrors the existing two proxied routes exactly — read `caddy/Caddyfile` this session):
```
handle /api/refresh {
    uri strip_prefix /api
    reverse_proxy 127.0.0.1:8081 {
        header_up X-Forwarded-For {http.request.remote.host}
    }
}
```
Add this inside the existing `route { }` block, alongside `/api/register`/`/api/login`, before the terminal `handle { respond 404 }`. Rate-limiting: apply the same `client_ip()` + `RateLimiter` pattern the existing `login`/`register` handlers use (read this session) — a refresh endpoint that mints fresh game tokens is exactly the kind of endpoint the existing rate-limit design is meant to cover; do not leave it unlimited like `/validate` (which is unlimited only because its sole caller is the trusted, loopback-only auth-gate mod — `/api/refresh` is reachable from the public internet by definition).

**Launcher side:** on every Play, call `/api/refresh` first (using the keychain-stored refresh token) → get a fresh game token → proceed to manifest sync/launch. Store *only* the new refresh token back to the keychain (rotation); the game token is used once for that launch and discarded (never persisted, matching the existing 12-hour game-token design's own "raw value exists only in the response and the caller's memory" property [CITED: `auth-service/README.md`, already read]).

## Self-Update (LNCH-08)

`tauri-plugin-updater`'s `latest.json` schema [VERIFIED: fetched live from `v2.tauri.app/plugin/updater/` this session]:
```json
{
  "version": "1.0.0",
  "notes": "...",
  "pub_date": "2026-08-28T00:00:00Z",
  "platforms": {
    "windows-x86_64": { "signature": "...", "url": "https://mc.campfire.pub:8444/launcher/campfire-launcher_1.0.0_x64-setup.exe" },
    "darwin-x86_64":  { "signature": "...", "url": "https://mc.campfire.pub:8444/launcher/campfire-launcher_1.0.0_x64.app.tar.gz" },
    "darwin-aarch64": { "signature": "...", "url": "https://mc.campfire.pub:8444/launcher/campfire-launcher_1.0.0_aarch64.app.tar.gz" }
  }
}
```
Required per-platform fields are only `url` and `signature`; everything else is metadata. Generate the keypair once with `tauri signer generate -w ~/.tauri/campfire.key` [CITED: v2.tauri.app] — the private key stays with the operator (never committed), the public key content goes into `tauri.conf.json`'s `plugins.updater.pubkey`. Host `latest.json` and the artifacts under `/launcher/` on the existing Phase-3 Caddy file server (same pattern as `/pack/*` — a static tree, no new service). No Windows/macOS release artifacts exist yet in this phase (that's Phase 5's CI build) — this phase should stand up the updater plugin wiring and the endpoint contract, with the operator manually placing a `latest.json` + local build artifact for testing.

Progress during download uses the plugin's own callback, not a manual Channel — `update.download_and_install(|chunk_length, content_length| { ... }, || { /* finished */ })` [CITED: v2.tauri.app] — bridge this to the frontend via a Tauri command emitting through a `Channel`, same pattern as the manifest-sync progress (LNCH-05).

## Code Examples

### Progress reporting via Tauri Channel (LNCH-05)
```rust
// Source: v2.tauri.app/develop/calling-frontend/ (fetched live this session)
use tauri::ipc::Channel;

#[derive(Clone, serde::Serialize)]
#[serde(tag = "event", content = "data")]
enum SyncEvent {
    Progress { current: u32, total: u32, bytes_per_sec: u64, label: String },
    Done,
    Error { message: String },
}

#[tauri::command]
async fn sync_client(on_event: Channel<SyncEvent>) -> Result<(), String> {
    // ... diff manifest, download each changed file, on_event.send(SyncEvent::Progress { .. })
    on_event.send(SyncEvent::Done).map_err(|e| e.to_string())
}
```
```typescript
// Frontend
import { invoke, Channel } from '@tauri-apps/api/core';
const channel = new Channel<SyncEvent>();
channel.onmessage = (msg) => { /* update progress bar + step label */ };
await invoke('sync_client', { onEvent: channel });
```

### JVM launch command construction (LNCH-04)
Built from the merged vanilla+Forge version JSON (Pattern 2 above) and CONTEXT.md's locked launch line. `<forge-id>` is exactly `1.12.2-forge-14.23.5.2860` [VERIFIED: this exact string produced by the installer this session] — note the `-forge-` separator, not `forge` bare.
```
java -Xms<ram> -Xmx<ram> <jvm-flags> \
  -Dcampfire.nick=<nick> -Dcampfire.token=<token> \
  -Djava.library.path=<natives-dir> \
  -cp <classpath-from-merged-version-json> \
  net.minecraft.launchwrapper.Launch \
  --username <nick> --uuid <offline-uuid> --accessToken 0 --userType legacy \
  --version 1.12.2-forge-14.23.5.2860 --gameDir <game> --assetsDir <assets> --assetIndex 1.12 \
  --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker
```
`ClientAuthHandler.buildResponse()` [VERIFIED: read `mods-src/campfire-auth/src/main/java/pub/campfire/auth/client/ClientAuthHandler.java` this session] reads exactly these two system properties and nothing else: `System.getProperty("campfire.nick", "")` and `System.getProperty("campfire.token", "")` — confirming `-Dcampfire.nick=` / `-Dcampfire.token=` are the complete and correct handoff contract, no additional property or file needed.

### JVM flags (Claude's discretion per CONTEXT.md)
Aikar-family flags scaled for a client (not server) with a 6–8 GB heap [CITED: aikar.co, general knowledge, widely reproduced]:
```
-XX:+UseG1GC -XX:+ParallelRefProcEnabled -XX:MaxGCPauseMillis=200 \
-XX:+UnlockExperimentalVMOptions -XX:+DisableExplicitGC -XX:+AlwaysPreTouch \
-XX:G1NewSizePercent=30 -XX:G1MaxNewSizePercent=40 -XX:G1HeapRegionSize=8M \
-XX:G1ReservePercent=20 -XX:G1HeapWastePercent=5 -XX:G1MixedGCCountTarget=4 \
-XX:InitiatingHeapOccupancyPercent=15 -XX:G1MixedGCLiveThresholdPercent=90 \
-XX:G1RSetUpdatingPauseTimePercent=5 -XX:SurvivorRatio=32 \
-XX:+PerfDisableSharedMem -XX:MaxTenuringThreshold=1
```
Plus the two Forge-1.12.2-specific system properties commonly needed to avoid spurious certificate/patch-discrepancy warnings on a client pointed at a private, non-Mojang-session server flow: `-Dfml.ignoreInvalidMinecraftCertificates=true -Dfml.ignorePatchDiscrepancies=true` [ASSUMED — standard, widely-documented Forge 1.12.2 flags, not independently re-verified against this exact build this session; low risk, well-known].

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|-------------------|---------------|--------|
| STACK.md's `reqwest ^0.12` recommendation (2026-08-27) | `reqwest ^0.13` is current on crates.io | Confirmed this session via `cargo search` | Minor — no known breaking API concern flagged in this session's research; planner may pin either, `^0.13` is simply more current |
| STACK.md's "Azul Zulu 8 arm64" plan for Apple Silicon | CONTEXT.md's locked decision: x86_64 Temurin under Rosetta for v1, Zulu-arm64 deferred to a follow-up spike | CONTEXT.md, 2026-08-28 (this phase's own scoping) | Simplifies this phase to two Java sources total (Adoptium Windows + Adoptium mac, both x64) instead of three vendors across three platforms |
| `keyring` v3 API assumed by most existing tutorials | v4.1.6 is now the current major on crates.io, with a modifiers-map API replacing `new_with_target` | Ongoing crate evolution, confirmed this session | Recommend `^3` for this phase's single simple use case (see Standard Stack note) unless the planner has a reason to track `^4` |

**Deprecated/outdated:** none directly relevant beyond the above — this is a fast-moving stack (Tauri, Rust MSRV) so re-check crate versions again at plan time if execution is delayed by more than a few weeks.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | No `tauri-plugin-shell` needed anywhere in this phase | Standard Stack | Low — if a frontend-initiated shell call is later needed (unlikely given the architecture), add the plugin then; no rework of existing code |
| A2 | Aikar-family client JVM flags + the two `-Dfml.*` properties are safe defaults for this exact Forge 1.12.2 build | Code Examples | Low-medium — worst case is a slightly suboptimal GC profile or a spurious warning; not a launch-blocking risk. Verify empirically during the tracer-slice task (first real "Play" on the operator's Windows machine) |
| A3 | Apple Silicon LWJGL2-under-Rosetta rendering/framerate behavior | Pitfall 3 | Medium — this is explicitly called out as needing operator hardware verification; if performance is unacceptable, CONTEXT.md already has a named follow-up spike (Zulu arm64-native), not a phase blocker |
| A4 | `vite` `8.2.2`'s SUS flag is a false positive | Package Legitimacy Audit | Low — 175M weekly downloads, well-known org; still gated behind a human-verify checkpoint per protocol regardless |

## Open Questions

1. **Exact JVM heap/flag tuning for this specific RAM range (3–10 GB slider)**
   - What we know: Aikar-family flags are the standard baseline for modded 1.12.2 clients; CONTEXT.md leaves exact flags to Claude's discretion.
   - What's unclear: whether any RLCraft-specific client mod has a known JVM-flag incompatibility (the existing PITFALLS.md's server-side flag warnings don't necessarily transfer to the client).
   - Recommendation: use the flags above as the default; treat any launch-time JVM crash during the tracer-slice task as a signal to simplify (drop to vanilla `-Xmx`/`-Xms` only) rather than debugging exotic GC tuning first.

2. **Whether the operator's Windows/macOS machines already have `rustup` with a sufficiently new toolchain**
   - What we know: this Pi does not, and needed a fresh `rustup` install to satisfy Tauri 2.11's real MSRV.
   - What's unclear: the operator's own machine state — untested from this session.
   - Recommendation: the plan should include "verify `rustc --version` ≥ 1.88 (or run `rustup update`)" as an explicit first step on each of the operator's build machines, not assume it silently works.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Node.js | Frontend build (Vite/TS), `@tauri-apps/cli` | ✓ | v20.20.2 | — |
| npm | Package installs | ✓ | 10.8.2 | — |
| cargo/rustc (apt) | Existing `auth-service` crate | ✓ | 1.85.0+dfsg3-1 (Debian trixie) | — |
| rustc ≥1.88 (via rustup) | Launcher (`tauri`) crate — **hard requirement, verified this session** | ✗ | — | Install `rustup`, pin a stable channel ≥1.88 scoped to `launcher/src-tauri/` (Wave 0 task, no fallback — this blocks every subsequent launcher `cargo` command) |
| `webkit2gtk-4.1`/`javascriptcoregtk-4.1`/`libsoup-3.0`/`librsvg2`/`libayatana-appindicator3`/`libxdo` dev packages | Linux smoke build of the Tauri app on this Pi | ✗ (not installed; confirmed available in apt as of Debian 13 trixie via `apt-cache policy` this session — `libwebkit2gtk-4.1-dev` candidate `2.52.6-1~deb13u1`) | — | `apt install` the dev packages listed (Wave 0 task); this is a Linux-only smoke-build concern, irrelevant to the shipped Windows/macOS builds |
| `xvfb`/`xvfb-run` | Only if the Forge installer spike is ever re-run in a truly headless context without the `launcher_profiles.json` workaround | ✓ (`/usr/bin/xvfb-run` present) | — | Not actually needed — the `launcher_profiles.json` stub (Pattern 2) avoids the X11 dependency entirely; documented here in case a future Forge version regresses this |
| Temurin 8 JDK (aarch64, for the game **server**, unrelated to this phase) | Already installed for Phase 1 | ✓ | `1.8.0_504-b01` at `/opt/temurin-8/jdk8u504-b01/bin/java` | — |
| A real Windows x64 machine | Build/run the launcher for the tracer slice, verify SmartScreen/signing questions are Phase-5-only | ✓ (operator-owned, not this host) | — | — |
| A real Apple Silicon Mac | Build/run the launcher, verify Rosetta/LWJGL2 rendering | ✓ (operator-owned, not this host) | — | No Intel Mac available — Intel path is reasoning-only per CONTEXT.md, already acknowledged there |

**Missing dependencies with no fallback:** rustc ≥1.88 on this Pi (blocks the Linux smoke build only — has a fallback: install rustup, see above; not truly "no fallback," just not yet done).
**Missing dependencies with fallback:** webkit2gtk-4.1 dev packages (apt install, confirmed available).

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V2 Authentication | Yes | Existing argon2id hashing (`auth-service`, already built) — this phase adds no new password handling, only refresh-token issuance |
| V3 Session Management | Yes | New refresh-token table: hashed at rest, rotated on every use, revocable via existing `campfire-auth reset` CLI extended to also revoke refresh tokens; TTL 30 days per CONTEXT.md |
| V4 Access Control | N/A | No new authorization boundary introduced — the auth-gate mod's join-time enforcement is unchanged and out of this phase's scope |
| V5 Input Validation | Yes | Manifest path/URL validation ports `scripts/assemble-client.py`'s existing guard (path traversal, control characters, absolute paths, forbidden-prefix DIST-03 gate) verbatim |
| V6 Cryptography | Yes | Never hand-roll: refresh-token hashing reuses the existing `auth::hash_secret` (argon2id) pattern; self-update signatures use `tauri-plugin-updater`'s minisign integration, not a custom scheme; TLS uses the pinned private CA (`ca/campfire-ca.pem`, embedded via `include_bytes!`) with the system trust store explicitly disabled, mirroring `scripts/assemble-client.py`'s `ssl.create_default_context(cafile=cacert)` pattern exactly |

### Known Threat Patterns for This Stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|-----------------------|
| MITM serving a forged manifest + matching forged hashes | Tampering/Spoofing | TLS pinned to the project's own private CA, system trust store disabled — already the documented, non-negotiable design in `docs/DIST-OPS.md`'s "trust anchor" section (read this session); the launcher must mirror `assemble-client.py` exactly, no exceptions |
| Stolen/leaked refresh token used indefinitely | Elevation of Privilege | Rotation on every use (old token revoked the instant a new one is issued) limits a stolen token to one un-rotated use before the legitimate owner's next launch invalidates it |
| Refresh token stored in a plaintext file instead of the OS keychain | Information Disclosure | `keyring` crate only — this is the exact mistake ARCHITECTURE.md's Anti-Pattern 2 already documents and this phase must not repeat it |
| Path traversal via a malicious/compromised manifest entry | Tampering | Reuse `assemble-client.py`'s existing path-traversal guard verbatim (absolute path / `..` / control-char / commonpath checks) — do not write a new, weaker version |
| Self-update artifact tampering | Tampering | minisign signature verification via `tauri-plugin-updater`, public key embedded at build time, private key never leaves the operator's machine |
| `/api/refresh` abused for token-minting DoS or credential-stuffing-adjacent automation | Denial of Service | Apply the same per-IP `RateLimiter` pattern the existing `login`/`register` handlers use — do not leave this endpoint unlimited like `/validate` (which is safe unlimited only because it's loopback-only) |

## Sources

### Primary (HIGH confidence — verified live this session, tool-confirmed)
- Empirical test: `forge-1.12.2-14.23.5.2860-installer.jar --installClient` against Temurin 8, this Pi, twice (fail-then-succeed with the `launcher_profiles.json` stub) — full stdout captured, produced version JSON read in full, sha1 of the extracted Forge jar recomputed and matched
- Empirical test: `cargo check` with a bare `tauri = "2.11"` dependency on this Pi's rustc 1.85.0 — full MSRV error list captured
- Empirical test: `auth-service` crate (existing) still builds clean under the same apt rustc 1.85.0
- `apt-cache policy` for `libwebkit2gtk-4.1-dev` et al. on this Debian 13 trixie host
- `npm view` for `@tauri-apps/cli`, `@tauri-apps/api`, `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-opener`, `vite`, `typescript` versions and postinstall scripts
- `cargo search` for `tauri`, `tauri-plugin-updater`, `tauri-plugin-opener`, `reqwest`, `keyring` versions
- `gsd-tools query package-legitimacy check` against npm and crates.io for all listed packages
- Live fetch: `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` → `1.12.2.json` → `assetIndex`, full library list with `rules`/`natives`/`classifiers`, `minecraftArguments` template
- Live fetch: `https://api.adoptium.net/v3/assets/feature_releases/8/ga?image_type=jre&os=windows&architecture=x64` — real binary metadata for `jdk8u504-b01`
- `Read` this session: `auth-service/src/db.rs`, `auth-service/src/api.rs`, `caddy/Caddyfile`, `server.env`, `scripts/assemble-client.py`, `mods-src/campfire-auth/src/main/java/pub/campfire/auth/client/ClientAuthHandler.java`, `docs/DIST-OPS.md`, `auth-service/README.md`, `.planning/phases/04-launcher/04-CONTEXT.md`

### Secondary (MEDIUM confidence — official docs, WebSearch/WebFetch corroborated)
- v2.tauri.app/plugin/updater/ — `latest.json` schema, `tauri signer generate`, `download_and_install` API
- v2.tauri.app/develop/calling-frontend/ — `tauri::ipc::Channel` pattern
- github.com/MinecraftForge/MinecraftForge/issues/5478 — headless installer X11 crash (interactive path only)
- github.com/xfl03/ForgeInstallerHeadless README — confirms this workaround targets "Forge Installer 2.0 only, which is used in 1.13+" (i.e. does not apply to/is not needed for 1.12.2's older installer once the `launcher_profiles.json` prerequisite is known)
- github.com/MinecraftForge/FML — `ClassPatchManager` reads `binpatches.pack.lzma` as a runtime resource stream (corroborates no install-time binpatch artifact needing separate handling)
- github.com/open-source-cooperative/keyring-rs releases — v3→v4 API changes
- aikar.co/2018/07/02/tuning-the-jvm-g1gc-garbage-collector-flags-for-minecraft/ — G1GC flag baseline

### Tertiary (LOW confidence — WebSearch only, not independently corroborated)
- The exact `-Dfml.ignoreInvalidMinecraftCertificates`/`-Dfml.ignorePatchDiscrepancies` flag pair — standard/widely-repeated Forge 1.12.2 guidance, not verified against this specific build's source this session
- Apple Silicon LWJGL2/Rosetta rendering performance figures (carried forward from PITFALLS.md, not retestable from this Linux host)

## Metadata

**Confidence breakdown:**
- Standard stack (versions, package legitimacy): HIGH — every version/verdict came from a live registry query this session
- Forge 1.12.2 install mechanics: HIGH — reproduced empirically against the exact pinned build, not inferred
- MSRV/toolchain requirement: HIGH — reproduced empirically on the exact target machine
- Mojang manifest/version-JSON schema: HIGH — fetched live, fields quoted verbatim
- Apple Silicon rendering behavior, exact JVM flag tuning: LOW — untestable from this host, carried forward from prior research or general knowledge, explicitly flagged for operator verification

**Research date:** 2026-08-28
**Valid until:** ~30 days for the Mojang/Forge/Adoptium contract pieces (stable, unlikely to move); ~7-14 days for exact npm/crates.io version pins given this stack's release cadence — re-run `npm view`/`cargo search` at plan time if execution starts more than two weeks after this research.
