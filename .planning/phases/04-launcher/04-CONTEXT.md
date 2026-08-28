# Phase 4: Launcher - Context

**Gathered:** 2026-08-28
**Status:** Ready for planning

<domain>
## Phase Boundary

The Tauri 2 desktop launcher (Windows x64, macOS Intel, macOS Apple Silicon): register/login against the Phase-2 auth API via the Phase-3 HTTPS front, sync the client pack from `manifest.json`, provision Java 8, install Forge 1.12.2 client, launch RLCraft with the chosen RAM and the auth token, show progress/errors/status, remember the session, self-update. Covers AUTH-03, LNCH-01…LNCH-08. Release packaging/CI/Gatekeeper docs are Phase 5 — this phase must produce a launcher that builds and runs from source on the operator's Windows and Apple Silicon machines.

Integration contract (locked, from Phase 3): `docs/DIST-OPS.md` § "Phase 4 integration contract" — base URL `https://mc.campfire.pub:8444`, routes `/manifest.json`, `/pack/<url>`, `/api/register`, `/api/login`, `/status`; CA `ca/campfire-ca.pem` pinned (no system roots); token handoff `-Dcampfire.nick=… -Dcampfire.token=…`; nick casing must be preserved exactly.

</domain>

<decisions>
## Implementation Decisions

### Screen (single window, English UI)
- **UI language: English only for now** (operator decision, 2026-08-28) — no i18n framework needed, keep strings in one module for a later RU pass
- One form: nick + password fields, two buttons side by side: "Log in" and "Create account" (registration uses the same two fields)
- After successful login the form collapses to "Playing as **Nick** · Log out"; RAM slider and Play remain
- Visual style: **RLCraft art** — background/banner from the RLCraft client pack's own logo/art (taken from the client zip already cached on the Pi) + "campfire.pub" wordmark; if usable art can't be sourced, fall back to dark minimalism. Window ~480×560, non-resizable, accent Play button
- Top: status pill "● campfire.pub — Online · 2/10" / "Offline" (from `/status`, on start and every 15 s); Play is NOT blocked when offline
- RAM slider 3–10 GB step 0.5, default `min(8, round(total_ram/2))`, warning when > 70 % of physical RAM
- Bottom: progress bar + step label ("Downloading mods 120/187 · 45 MB/s"), driven by Tauri **channels** (not the event bus)
- Errors: red inline banner under Play in plain English (wrong password, server unreachable, Java download failed, disk full, …) + "Open log" button; launcher always writes `launcher.log`
- Extra controls: "Game folder" (opens game dir), "Verify files" (force full hash re-check + repair), launcher version + pack_version in small text at the bottom

### Java / Forge / launch
- Java 8 JRE per platform, never system Java: Windows x64 → Adoptium Temurin 8 (API v3); macOS Intel → Adoptium Temurin 8; **macOS Apple Silicon → x86_64 Adoptium Temurin 8 under Rosetta** for v1 (LWJGL 2 has no arm64 natives); checksum from the vendor API verified. Follow-up spike (not blocking): Azul Zulu 8 arm64 + community ARM64 LWJGL2/jinput natives for performance
- Forge client install: spike first — run official `forge-1.12.2-14.23.5.2860-installer.jar --installClient <game dir>` headless with the provisioned Java 8 → standard `versions/<forge-id>/<forge-id>.json`; launcher then treats it as a vanilla version JSON (libraries, natives, main class `net.minecraft.launchwrapper.Launch`, tweak class). Fallback if headless install fails: construct the version JSON + libraries manually per the known 1.12.2 layout
- Vanilla client jar, libraries, natives, assets always from Mojang (`version_manifest_v2.json` → 1.12.2 json → asset index; SHA-1 verified) — never from our host
- Install root: Windows `%APPDATA%\campfire\`, macOS `~/Library/Application Support/campfire/`; layout `runtime/` (Java), `game/` (Minecraft dir: mods, config, saves, …), `versions/`, `libraries/`, `assets/`, `launcher.log`
- Never touched by sync: `saves/`, `options.txt`, `optionsof.txt`, `servers.dat`, `screenshots/`, `logs/`, `resourcepacks/` user additions outside the manifest. `servers.dat` with `mc.campfire.pub` is seeded once on first run only
- Launch: `java -Xms<ram> -Xmx<ram> <Aikar-ish client flags> -Dcampfire.nick=<nick> -Dcampfire.token=<token> -Djava.library.path=<natives> -cp <classpath> net.minecraft.launchwrapper.Launch --username <nick> --uuid <offline uuid> --accessToken 0 --userType legacy --version <forge-id> --gameDir <game> --assetsDir <assets> --assetIndex 1.12 --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker`; auto-connect to the server via `--server mc.campfire.pub --port 25565` if it works with Forge 1.12.2, else rely on the seeded servers.dat
- Offline UUID = `UUID.nameUUIDFromBytes("OfflinePlayer:"+nick)` (v3), nick case preserved exactly as registered

### Session / updates / status
- AUTH-03 via a **refresh token**: auth service change — `/api/login` additionally returns `refresh` (30-day, random 32 B, stored hashed, revoked by `campfire-auth reset`); new `POST /api/refresh {nick, refresh}` → fresh game token (+ rotated refresh). Launcher stores ONLY the refresh token in the OS keychain (`keyring` crate: Windows Credential Manager / macOS Keychain). Password never persisted. Caddy proxies `/api/refresh` like `/api/login`
- Each Play: refresh → game token → sync → launch. Expired/revoked refresh → form re-opens with a friendly message
- Client sync before every Play: fetch manifest → sha256 diff → download only changed/missing (≤ 4 parallel, tmp + atomic rename) → apply `delete[]` → block Play with a clear message on failure. "Verify files" = full re-hash of managed files
- Self-update (LNCH-08): Tauri updater plugin; feed `https://mc.campfire.pub:8444/launcher/latest.json` + artifacts under `/launcher/` on the Phase-3 file server; minisign signature (private key held by operator, public key embedded); check on startup, "Update now" dialog
- Status: `/status` on start + every 15 s

### Claude's Discretion
- Frontend stack inside Tauri (vanilla TS vs a small framework), exact crate versions, JVM flag set for the client, download concurrency, log rotation, how the RLCraft art asset is extracted/licensed-noted, keychain fallback when no keychain is available

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `docs/DIST-OPS.md` integration contract; `scripts/assemble-client.py` (reference implementation of manifest diff/verify in Python — port the logic to Rust)
- `auth-service/` (axum) — extend with refresh tokens; `scripts/auth-smoke.sh` — extend tests; `caddy/Caddyfile` — add `/api/refresh`
- `mods-src/campfire-auth` — client side reads `-Dcampfire.nick/-Dcampfire.token`
- `ca/campfire-ca.pem` — embed in launcher via `include_bytes!`
- Server-side pack: `pack/` + `scripts/publish-pack.sh` (can also host `/launcher/latest.json` + artifacts)

### Established Patterns
- Secrets never in git; loopback-first; scripts idempotent; Rust services with smoke tests; no game-server restarts without announcement
- GitHub Actions self-hosted runners on this Pi are aarch64 Linux — Windows/macOS builds must use GitHub-hosted runners (Phase 5); during Phase 4 the operator builds locally on Windows x64 and Apple Silicon

### Integration Points
- Phase 5 consumes: the Tauri project (`launcher/`), updater public key, `latest.json` format, the Gatekeeper bypass text for macOS
- Operator test machines available: Windows x64 and macOS Apple Silicon (no Intel Mac — Intel path verified only by CI build + reasoning)

</code_context>

<specifics>
## Specific Ideas

- "Press Play and end up in the world" is the core value — the tracer slice should be exactly that path on Windows first
- English-only UI for now; keep strings centralized
- Do not embed any Mojang-owned files in the launcher

</specifics>

<deferred>
## Deferred Ideas

- Russian UI / language switcher — later pass
- ARM64-native LWJGL2 for Apple Silicon — performance follow-up spike
- Skins (Drasl) — v2; multiple accounts — out of scope

</deferred>
