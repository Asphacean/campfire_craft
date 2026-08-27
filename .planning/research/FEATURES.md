# Feature Research

**Domain:** Custom Minecraft launcher + private modded server (RLCraft, Forge 1.12.2, own auth, 5-7 friends)
**Researched:** 2026-08-27
**Confidence:** HIGH (launcher landscape is well-documented, stable for years; server-ops practices are standard sysadmin knowledge)

## Feature Landscape

### Table Stakes (Users Expect These)

Features friends will assume exist. Missing these = "it's broken", not "it's minimalist".

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Login (nick + password) | Explicit ask; every private launcher (TLauncher, GravitLauncher, SKLauncher) gates play behind an account | LOW | Simple form → auth API call |
| Register (self-service) | PROJECT.md requires anyone with the launcher to create an account | LOW | Same endpoint family as login; validate nick uniqueness |
| RAM slider/selector | Explicit ask; every launcher (Prism, GravitLauncher, TLauncher) exposes this because RLCraft needs 6-8GB and defaults vary by machine | LOW | Cap min/max sensibly (e.g. 3-10GB) so friends don't OOM their laptop or under-allocate for RLCraft |
| Play button that "just works" | Core value statement — press Play, end up in-game | LOW-MED | Orchestrates: check update → launch JVM with correct args |
| Auto-update of client (mods/configs) | PROJECT.md requirement; standard in GravitLauncher/private launchers via manifest+hash diffing | MED | Manifest with file hashes, only fetch changed files (already decided) |
| Auto Java 8 fetch | PROJECT.md requirement; Forge 1.12.2 requires Java 8, most players won't have it installed | MED | Bundle/download Temurin 8 per-OS/arch, launch JVM pointing at it, never touch system Java |
| Progress bar / status during download & launch | Every launcher (Prism, CurseForge app, TLauncher) shows this — silent multi-minute downloads read as "frozen" | LOW | Even a simple "Downloading mods (12/340)... / Starting Minecraft..." text label suffices |
| Basic error message on failure | If login/download/launch fails, a friend needs to know why (bad password, server down, disk full) without reading logs | LOW | Human-readable message in the UI, not a raw stack trace |
| Remember-me / stay logged in | Table stakes UX in every consumer launcher; re-typing password every launch is friction for a 5-7 person friend group who'll launch dozens of times | LOW | Store session/token locally (not plaintext password) |
| Launcher self-update | Every maintained launcher (Prism, GravitLauncher, TLauncher) updates itself; you WILL ship launcher bugfixes and this avoids "reinstall the exe" support tickets | LOW-MED | Check version against file server on startup, prompt or auto-replace binary (Tauri has built-in updater plugin) |
| World backups | Universal server-ops table stake; RLCraft players lose hours of progression to crashes/corruption without it | LOW | Cron/scheduled tar of world dir, rotate N backups |
| Server autostart/autorestart on boot & crash | Universal; Pi 5 will reboot (power loss, updates) and a modded server WILL crash occasionally | LOW | systemd service with `Restart=on-failure`, `WantedBy=multi-user.target` |
| Whitelist / access control | Server-side equivalent of the launcher's own auth — only registered+authenticated players should be able to connect | LOW-MED | Since it's offline-mode, "whitelist" = your auth service gatekeeping join, not vanilla whitelist.json (see Architecture doc) |

### Differentiators (Competitive Advantage)

Not required, but this is where the "5 minutes to first Play" value prop is won or lost versus friends manually installing Forge + mods themselves.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Zero manual setup (no Forge install, no mod folder, no Java download) | This IS the core value proposition — the entire reason the launcher exists instead of "here's a Google Drive link and instructions" | MED (already scoped) | Already central to PROJECT.md; the "differentiator" here is doing it well, not adding more |
| Manifest-based incremental updates (hash diffing) | Faster updates after the first install — only changed mods re-download, matters on Pi upload bandwidth with 5-7 clients | LOW-MED | Already decided; standard technique used by GravitLauncher, Modrinth App, PolyMC |
| Single source of truth for modpack (no version drift) | Prevents "works on my client" bugs from friends having different mod versions — common RLCraft pain point since RLCraft updates frequently and mismatched versions desync/crash | LOW (falls out of manifest system) | Launcher refuses to launch (or auto-fixes) if local files don't match manifest |
| Tiny installer/binary (Tauri, ~10MB) | Faster download/install for friends vs. Electron-based CurseForge app (100MB+) or Java-based launchers requiring a JRE just to run the launcher itself | LOW (Tauri decision already made) | Already decided; genuine differentiator vs. TLauncher/CurseForge bloat |

### Anti-Features (Commonly Requested, Often Problematic)

Features every other launcher has, that would blow the "minimalist" budget for a 5-7 person friend server with no real payoff.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|------------------|-------------|
| Player skins via CustomSkinLoader/authlib-injector | "I want my custom skin in-game" | authlib-injector requires `online-mode=true` and a Yggdrasil-compatible skin server (e.g. Drasl) resolving offline UUIDs to skins — real infra to stand up and conflicts with the offline-mode + own-auth architecture already decided. Skins are cosmetic-only value for a 5-7 person server | Default Steve/Alex skins, or (if truly wanted later) a single shared static skin overlay — not now |
| Multiple accounts / account switching | Every mainstream launcher (TLauncher, GravitLauncher) supports it for shared-PC or multi-persona use | This is a single-friend-group launcher where each person runs it on their own machine with their own account — multi-account is solving a problem this project doesn't have | One account per install, remember-me covers the actual need |
| News feed / MOTD panel in launcher | Common in TLauncher/official launcher for engagement/ads | Explicitly out of scope in PROJECT.md ("minimalist was explicit"); adds a CMS/content pipeline for zero players who need it | Discord announcement channel already exists for a friend group |
| Server status / online player count in launcher UI | Nice-to-have polish seen in some private launchers | Extra endpoint + UI + polling logic for information 5-7 friends already get from Discord ("anyone on?") — output-vs-cost mismatch for MVP | Skip for v1; trivial to add later as a GET /status call if actually wanted |
| Offline play / singleplayer mode in launcher | Standard launcher feature (play without server) | This launcher's entire purpose is connecting to one specific private server — offline play is solving a problem (playing without the group) this project isn't for | N/A — not the product |
| Launcher UI theming / skins for the launcher itself | Cosmetic differentiation seen in TLauncher/SKLauncher forks | Explicitly out of scope in PROJECT.md; pure surface area with zero functional value | One clean default look |
| Crash report auto-upload / telemetry / analytics dashboard | "So I can debug remotely" | Real crash handling matters (see below) but a full telemetry pipeline (Sentry-style dashboards, opt-in flows) is overkill for 5-7 friends who can paste a log in Discord | Launcher writes a local crash log + points user to the log file path in the error message; ask for it in Discord when something breaks |
| RCON web dashboard / admin panel | "Nice to manage server without SSH" | You already have SSH to the Pi; building a web UI for RCON is infra for an audience of one (the operator) | Use RCON via CLI (`mcrcon`) or existing SSH access directly |
| Difficulty/gamerule config UI in launcher | Some private-server launcher forks expose server settings to admins in-app | RLCraft's difficulty is defined by the modpack itself (Hardcore Questing Mode progression) — this is a server.properties/config edit done once by the operator, not a per-launch player-facing feature | Set once in server config, not exposed in launcher |

## Feature Dependencies

```
Register
    └──requires──> Auth API (own auth service)

Login
    └──requires──> Auth API (own auth service)
    └──enables───> Remember-me (session/token storage)

Play button
    └──requires──> Login (must be authenticated)
    └──requires──> Auto Java 8 fetch (JVM must exist before launch)
    └──requires──> Auto-update of client (files must be current before launch)
    └──enables───> Progress bar (surfaces the above steps)

Auto-update of client
    └──requires──> Manifest w/ file hashes (file server)
    └──enables───> Single source of truth / no version drift

Whitelist (server-side gating)
    └──requires──> Auth API (same account system as launcher login)
    └──conflicts──> Vanilla Mojang whitelist.json (mutually exclusive w/ offline-mode)

Player skins (CustomSkinLoader/authlib-injector)
    └──requires──> online-mode=true
    └──conflicts──> Own auth / offline-mode architecture (already decided)

Launcher self-update
    └──enhances──> Auto-update of client (same "check version, fetch, replace" pattern reused)

World backups
    └──enhances──> Server autorestart (restore-from-backup path after a bad crash)
```

### Dependency Notes

- **Play requires Login, Java 8, and client update to all succeed first:** this ordering is why the progress bar exists — it's not decorative, it's the only way a friend understands why Play takes 30 seconds sometimes and 2 seconds other times.
- **Whitelist conflicts with vanilla Mojang whitelist.json:** because the server is offline-mode, standard Mojang whitelist (UUID-based, requires Mojang account lookup) doesn't apply. Access control must be enforced by the same auth service gatekeeping the launcher login (see ARCHITECTURE.md for the mechanism — e.g. plugin/mod checking a shared secret or session token on join).
- **Player skins conflicts with the offline-mode + own-auth decision:** authlib-injector's skin resolution needs `online-mode=true` pointed at a Yggdrasil-compatible server (e.g. Drasl) that maps offline UUIDs to skins. Since PROJECT.md already locked in offline-mode + custom auth (no Mojang), adding real skins means running a second identity-adjacent service. Confirmed via GravitLauncher/authlib-injector/Drasl documentation — this is a deliberate architectural fork, not a small add-on, which is why it's an anti-feature for this milestone.
- **Launcher self-update enhances client auto-update:** both are "compare local version/hash against server manifest, fetch delta" — implement once, reuse the pattern for both the client files and the launcher binary itself (Tauri's updater plugin can also handle this natively).

## MVP Definition

### Launch With (v1)

Minimum viable product — matches PROJECT.md Active Requirements exactly, no additions.

- [ ] Register (nick + password) — required before first Play
- [ ] Login (nick + password) + remember-me — required every Play thereafter
- [ ] RAM slider — explicit ask, required for RLCraft's heavy footprint
- [ ] Play button — orchestrates Java check → update check → launch
- [ ] Auto Java 8 fetch (Win x64, macOS Intel + ARM) — no manual runtime install
- [ ] Manifest-based client auto-update (hash diff) — mods/configs stay in sync
- [ ] Progress indicator during download/launch — prevents "is it frozen?" confusion
- [ ] Basic readable error messages (auth failure, server unreachable, disk/space issues)
- [ ] Server-side: whitelist/access-control tied to the same auth accounts
- [ ] Server-side: autostart on boot + autorestart on crash (systemd)
- [ ] Server-side: scheduled world backups with rotation

### Add After Validation (v1.x)

Add only if friends actually hit the friction these solve.

- [ ] Launcher self-update — add once you ship the first post-launch bugfix and don't want to distribute a new .exe manually
- [ ] Crash log surfacing in UI (path to log file, not full telemetry) — add if "it crashed, no idea why" comes up in Discord
- [ ] Simple `/status` — is the server up? — add if friends repeatedly ask in Discord before launching

### Future Consideration (v2+)

Defer indefinitely unless the group's needs genuinely change.

- [ ] Player skins (would require standing up online-mode-compatible skin resolution, e.g. Drasl) — defer: conflicts with own-auth architecture, cosmetic-only payoff for 5-7 people
- [ ] Multiple accounts per install — defer: no shared-PC use case exists in this friend group
- [ ] Server status/online-count widget in launcher — defer: Discord already serves this
- [ ] RCON web dashboard — defer: SSH access already covers server admin

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Login/Register | HIGH | LOW | P1 |
| RAM slider | HIGH | LOW | P1 |
| Play + orchestration | HIGH | MEDIUM | P1 |
| Auto Java 8 fetch | HIGH | MEDIUM | P1 |
| Manifest-based auto-update | HIGH | MEDIUM | P1 |
| Progress bar | MEDIUM | LOW | P1 |
| Remember-me | MEDIUM | LOW | P1 |
| Readable error messages | MEDIUM | LOW | P1 |
| Server whitelist/access control | HIGH | LOW-MEDIUM | P1 |
| Server autostart/autorestart | HIGH | LOW | P1 |
| World backups | HIGH | LOW | P1 |
| Launcher self-update | MEDIUM | LOW-MEDIUM | P2 |
| Crash log surfacing | MEDIUM | LOW | P2 |
| Server status ping | LOW | LOW | P3 |
| Player skins | LOW | HIGH | P3 (likely never) |
| Multiple accounts | LOW | LOW | P3 (likely never) |
| RCON dashboard | LOW | MEDIUM | P3 (likely never) |
| News feed / launcher theming | LOW | LOW | Rejected (explicit out-of-scope) |

**Priority key:**
- P1: Must have for launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

## Competitor Feature Analysis

| Feature | GravitLauncher (private-server launcher standard) | TLauncher (cracked/consumer launcher) | Our Approach |
|---------|---|---|---|
| Auth | Own auth server, DB-backed (username/password/UUID/token) | Choice of Mojang or offline nickname (no real auth) | Own auth service, same pattern as GravitLauncher — real password check, not just a nickname field |
| Update system | Manifest + module system, hash-verified selective file sync | Full modpack ZIP re-download, no diffing in most configs | Manifest + hash diff (matches GravitLauncher's approach, better than TLauncher) |
| Skins | Supported via LauncherAuthlib modules, requires setup | Full skin support (mimics official launcher) | Explicitly skipped (anti-feature) — not worth the online-mode conflict for 5-7 friends |
| Java management | ServerWrapper can bundle/manage runtimes | Bundles its own JVM per version | Auto-fetch Java 8 per-OS/arch (same idea, scoped to exactly one version since RLCraft is pinned to Forge 1.12.2) |
| Multi-account | Not a core focus, single profile typical in most setups | Yes, account switching supported | Skipped — one account per install/friend |
| Binary size / stack | Java-based, requires bundled JRE for the launcher itself | Java-based, similarly heavier | Tauri (Rust + web UI, ~10MB) — smaller and simpler distribution than either |

## Sources

- [GravitLauncher GitHub](https://github.com/GravitLauncher/Launcher) — auth provider architecture, module/update system (HIGH confidence, official repo)
- [GravitLauncher Wiki — Auth setup](https://gravitlauncher.com/auth/) (MEDIUM confidence — Russian-language community wiki, but consistent with repo code)
- [authlib-injector README](https://github.com/yushijinhun/authlib-injector/blob/develop/README.en.md) — online-mode requirement for skin resolution (HIGH confidence, official repo)
- [drasl usage docs](https://github.com/unmojang/drasl/blob/master/doc/usage.md) — offline-UUID skin resolution mechanism and why plain authlib-injector fails on `online-mode=false` (HIGH confidence, official repo)
- [FjordLauncher issue #18](https://github.com/unmojang/FjordLauncher/issues/18) — confirms skins invisible on offline-mode servers without a UUID-resolving skin server (MEDIUM confidence — issue thread, but consistent with drasl/authlib-injector docs)
- General domain knowledge: TLauncher, PolyMC/Prism Launcher, Modrinth App, CurseForge app, SKLauncher feature sets (table-stakes UX patterns — login, RAM slider, progress bar, self-update, multi-account) — long-standing, stable ecosystem conventions
- General server-ops domain knowledge: systemd autorestart, cron/scheduled backups, RCON, whitelist patterns — standard Minecraft server administration practice

---
*Feature research for: Custom Minecraft launcher + private modded server*
*Researched: 2026-08-27*
