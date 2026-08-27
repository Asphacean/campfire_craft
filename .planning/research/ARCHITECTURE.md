# Architecture Research

**Domain:** Private modded Minecraft (Forge 1.12.2) server + custom auth + custom launcher (Tauri)
**Researched:** 2026-08-27
**Confidence:** HIGH (Forge launch mechanics, manifest patterns, Caddy) / MEDIUM (auth-mod approach — niche, no single dominant open-source project to point at; recommendation is a synthesis of known-working pieces)

## Standard Architecture

### System Overview

```
┌───────────────────────────── Player's machine ─────────────────────────────┐
│  ┌────────────────────────────┐                                            │
│  │   Tauri Launcher (Win/Mac)  │                                            │
│  │  - login/register UI        │                                            │
│  │  - RAM slider, Play button  │                                            │
│  └──────┬───────────┬─────────┘                                            │
│         │HTTPS      │HTTPS         spawns──────────────┐                   │
└─────────┼───────────┼──────────────────────────────────┼───────────────────┘
          │            │                                  │
          ▼            ▼                                  ▼
┌──────────────┐ ┌───────────────┐              ┌──────────────────────┐
│ Auth Service │ │ File/Update   │              │ java (Forge 1.12.2   │
│ (register,   │ │ Server        │              │ launchwrapper)       │
│ login, token │ │ (manifest.json│              │ + client auth mod    │
│ validate)    │ │  + mod/config │              │ (sends token on join)│
└──────┬───────┘ │  files)       │              └──────────┬────────────┘
       │         └───────┬───────┘                          │ raw TCP 25565
       │  behind Caddy (HTTPS, auto TLS, port 443)           │ (Minecraft
       └──────────────────┬─────────────────────────────────┼─ protocol,
                           │                                 │  NOT HTTP)
┌──────────────────────────┴──────────── Raspberry Pi 5 ─────┴──────────────┐
│  ┌───────────────┐   ┌───────────────┐   ┌──────────────────────────┐   │
│  │ Auth Service   │   │ File Server    │   │ Forge 1.12.2 MC Server   │   │
│  │ process        │◄──┤ (static files) │   │ (offline-mode=true)      │   │
│  │ + SQLite DB    │  loopback HTTP    │   │ + server-side auth mod   │   │
│  │ (users, tokens)│   validate call   ├──►│   validates token against│   │
│  └───────────────┘   (mod build step) │   │   Auth Service over      │   │
│                                        │   │   loopback HTTP          │   │
│                                        └───┴──────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

Two very different transport worlds sit side by side on the same box: everything under Caddy is **HTTPS on 443** (auth, manifest, file downloads — normal web traffic, terminated by Caddy); the Minecraft server speaks its own **binary protocol on TCP 25565**, which Caddy's HTTP reverse proxy cannot understand or forward — that port is exposed directly (router port-forward straight to the Pi, no Caddy in the path).

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| Auth Service | Own account store: register (nick+password), login (issue short-lived token), validate token (called by the MC server mod) | Small HTTP API (any stack — Go/Node/Python all fine at this scale) + SQLite; bcrypt/argon2 password hashing |
| File/Update Server | Serves `manifest.json` + the modpack file tree (mods, configs, Forge version JSON) over HTTPS; source of truth for "what the client should have" | Static file serving (Caddy `file_server` directive is enough — no app server needed) + a small script that (re)generates the manifest when the pack changes |
| Minecraft Server (Forge 1.12.2) | Runs RLCraft, offline-mode=true, enforces auth via a server-side mod that gates spawn until the client's token is validated | Forge 1.12.2 server jar + RLCraft mods + one small custom "auth-gate" mod |
| Client Auth Mod | Sends the launcher-issued token to the server-side auth-gate mod automatically on join (no `/login` typing) | Thin Forge 1.12.2 client mod using a plugin/custom packet channel, paired 1:1 with the server-side mod |
| Reverse Proxy (Caddy) | Terminates TLS for Auth Service + File Server on 443, auto-provisions/renews Let's Encrypt certs | Caddy binary, ~10-line Caddyfile, requires port 80/443 forwarded to the Pi for the ACME HTTP-01 challenge (or DNS-01 if 80 can't be opened) |
| Tauri Launcher | Login/register UI → calls Auth Service; diffs local install against `manifest.json` → downloads deltas from File Server; provisions Java 8 if missing; constructs and spawns the Forge launch command with the session token injected | Tauri (Rust backend, web-tech UI); Rust does the HTTP calls, file hashing/download, JVM arg construction, and process spawn |

## Recommended Project Structure

This is a multi-repo-shaped system (or a monorepo with clearly separated top-level dirs) — the four runtime components have almost no code sharing (different languages/runtimes: launcher is Rust/Tauri, auth+file server is a small backend service, the Forge mods are Java 8, the server itself is data/config, not code you author beyond mod jars).

```
rlcraft/
├── server/                  # Everything that runs on the Pi as "the game server"
│   ├── forge-server/         # Forge 1.12.2 install, RLCraft mods, server.properties (offline-mode=true)
│   ├── mods/auth-gate/       # Custom server-side Forge mod (Java) — validates token, kicks on failure
│   └── ops/                  # systemd unit, backup cron/script, Caddyfile
├── auth-service/             # Standalone HTTP API — register/login/validate-token, own DB
├── file-server/              # manifest.json generator script + served file tree (mods/, config/, forge version json)
├── launcher/                 # Tauri app (Rust + web UI)
│   └── src-tauri/            # Java provisioning, manifest diff/download, Forge launch-command builder
└── client-mod/                # Client-side companion to auth-gate (sends token on join) — ships inside the modpack
```

### Structure Rationale

- **`server/mods/auth-gate/` and `client-mod/` are separate top-level concerns but a matched pair.** They must be versioned and released together (a protocol/token-format change breaks both sides simultaneously) — keep them in sync explicitly (shared version number, or a single mod source split into two build targets), not as independently evolving projects.
- **`auth-service/` and `file-server/` are decoupled from each other and from the game server.** Neither needs the other to function; the only shared dependency is Caddy in front of both and, later, the auth-gate mod calling `auth-service`'s validate endpoint over loopback.
- **`launcher/` depends on the *shape* of everything else** (auth API contract, manifest format, Forge version JSON) but not on their code — it only needs stable HTTP contracts. This is why it should be built last (see Build Order below).

## Architectural Patterns

### Pattern 1: Token issued by Auth Service, carried by the launcher, validated by a server-side mod

**What:** The launcher is the only thing that ever asks the player for a password. It POSTs nick+password to the Auth Service over HTTPS and gets back a short-lived opaque token (random string or JWT, a few minutes' TTL, single-use or tied to the player's IP). The launcher passes that token to the game process at launch (JVM system property, e.g. `-Dauth.token=…`, or a small token file written next to the client mod's config right before spawn — a system property is simpler and avoids stray files). A thin client-side Forge mod reads it and sends it to the server the moment the player joins, over a custom plugin-message packet (not a chat command — no typing). A thin server-side Forge mod intercepts login (before the player can act — e.g. gate movement/interaction, or delay spawn) and calls `POST /validate` on the Auth Service over **loopback HTTP** (same Pi, no TLS/public exposure needed for this hop). Valid → let the player in. Invalid/missing/expired → kick with a clear message.

**When to use:** This is the right fit here specifically because (a) the auth decision is already "own account system, offline-mode server" (per PROJECT.md), not Mojang-compatible identity, and (b) the explicit UX goal is "press Play — no manual setup," which rules out anything requiring the player to type a command in-game.

**Trade-offs:**
- Requires writing and maintaining two small Forge mods (client + server) — real but bounded work; a minimal plugin-channel handshake is on the order of 100–200 lines of Java total.
- The server-side mod becomes a single point of trust: if its validation call to the Auth Service fails open (e.g., network hiccup treated as "allow"), auth is defeated. Fail closed (any error → kick) is the only safe default.
- No dependency on Yggdrasil semantics, skins-via-Mojang, or `online-mode=true` — stays entirely in "offline-mode + own layer" territory, which matches the constraint that no Minecraft license should be required.

### Pattern 2 (rejected as primary, documented for comparison): authlib-injector + Drasl (Yggdrasil-compatible server)

**What:** [Drasl](https://github.com/unmojang/drasl) is a self-hostable Yggdrasil-compatible API server (register/login/skins/capes), paired with [authlib-injector](https://github.com/yushijinhun/authlib-injector) loaded via `-javaagent` on **both** the client and the server, redirecting Mojang's auth/session endpoints to your own Drasl instance.

**When it would apply:** Projects that want proper skins/capes and Yggdrasil-protocol compatibility with off-the-shelf launchers (Fjord Launcher, HMCL) that already support "custom API servers."

**Why not the primary choice here:**
- Requires `online-mode=true` on the server (Drasl impersonates Mojang's servers, it doesn't run in offline-mode) — this is a materially different server configuration than the "offline-mode server" already decided in PROJECT.md, and adds an extra moving service (Drasl itself) beyond what this project needs.
- Documented, currently-open compatibility bug: **Forge 1.12.2 clients using authlib-injector show only their own skin correctly; other players render as Steve/Alex** ([authlib-injector issue #33](https://github.com/yushijinhun/authlib-injector/issues/33)) — a real, version-specific gotcha for exactly this stack.
- Since the launcher is fully custom (Tauri, not an existing authlib-injector-aware launcher), none of Drasl's "works with existing launchers" value is captured — building the custom launcher already has to talk to *some* auth API, so it can just as easily talk to a purpose-built minimal Auth Service instead of a general-purpose Yggdrasil server.
- Net: more moving parts (Drasl + javaagent injection on both ends) for a benefit (skins/capes, Yggdrasil compatibility) that isn't a stated requirement.

### Pattern 3 (rejected): plain `/login <password>` chat-command mod (AuthMe-style)

**What:** Classic offline-server pattern — server-side mod (e.g. SimpleLogin, ServerAuth for Forge) freezes the player on join and requires typing `/login <password>` or `/register <password>` in chat before they can move.

**When it would apply:** Servers where players connect with an arbitrary/unmodified client and password entry has to happen in-game because there's no companion launcher to do it beforehand.

**Why not the primary choice here:** This project *has* a custom launcher whose whole purpose is to remove manual steps ("press Play — no manual setup" is the stated core value). Making players type a password in chat on every join is a strictly worse UX than the token-handoff pattern (Pattern 1) for no added security — the launcher already authenticated them seconds earlier. Note: SimpleLogin itself actually improved on the raw chat-command version by auto-submitting a client-stored password on join (closer to Pattern 1's UX) — if hand-rolling the mod pair turns out to be more work than expected, forking/adapting SimpleLogin's client-stores-credential-and-auto-sends approach is a reasonable fallback, just swap "password" for "launcher-issued token" so the raw password is never written to disk on the player's machine.

## Data Flow

### Registration / Login → Join Flow

```
[Launcher UI: nick+password]
        │ HTTPS POST /register or /login
        ▼
[Auth Service] ──validates, hashes/checks password, writes session──► [DB: users, tokens]
        │ HTTPS response: { token, expiresAt }
        ▼
[Launcher] stores token in memory only (not written to disk)
        │
        ├─► HTTPS GET /manifest.json  (File Server) ──diff against local install──► download changed files
        ├─► check local Java 8 ──if missing──► download from Adoptium API, unpack to launcher-managed dir
        ├─► build classpath + JVM args (merge vanilla + Forge version JSON)
        └─► spawn: java -Xmx<RAM> -Dauth.token=<token> -cp <classpath> \
                    net.minecraft.launchwrapper.Launch --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker ...
                        │
                        ▼
        [Game process: client auth mod reads -Dauth.token, holds it]
                        │ TCP 25565 (Minecraft protocol) — player joins world
                        ▼
        [Client auth mod] --custom plugin packet, token--► [Server auth-gate mod]
                        │
                        ▼
        [Server auth-gate mod] --loopback HTTP POST /validate {token}--► [Auth Service]
                        │
              valid ──────────┴────────── invalid/expired/missing
                │                                  │
         allow spawn/movement              kick with message
```

### Update Flow (launcher startup, before Play is pressable)

```
[Launcher] ──HTTPS GET /manifest.json──► [File Server]
manifest = { files: [{path, sha256, size}], delete: [paths removed from pack] }
[Launcher] walks manifest:
  for each file: compute local sha256 (if exists) → mismatch/missing → download from File Server
  for each path in `delete`: if present locally and inside a *managed* dir (mods/, config/) → remove
  never touches: saves/, options.txt, servers.dat, screenshots/, logs/  (never listed in manifest, never in `delete`)
```

### Key Data Flows

1. **Credential flow never touches the game server directly** — the server only ever sees an opaque, short-lived token over loopback, never the password. This keeps the password database, hashing, and rate-limiting entirely inside the Auth Service, which is the only component that needs to reason about account security.
2. **Manifest diffing is one-directional and idempotent** — the File Server is the sole source of truth for "what should exist"; the launcher never uploads anything back. Re-running the update check is always safe (hash comparison is deterministic).
3. **Two independent trust boundaries cross the Pi's network edge**: HTTPS (443, Caddy-terminated, public) for anything account/file-related, and raw TCP (25565, unencrypted Minecraft protocol) for gameplay. Nothing bridges these — the token handoff happens *inside* the game protocol via a custom packet, not by tunnelling HTTP through it.

## Scaling Considerations

Explicitly out of scope for this project (5–7 friends, one Pi 5), but worth stating so nobody over-builds:

| Scale | Architecture Adjustments |
|-------|--------------------------|
| 5–7 players (this project) | Single Pi, SQLite for Auth Service, Caddy `file_server` for the manifest/mods, no load balancing, no CDN. This is more than sufficient. |
| ~20–50 players | Auth Service DB choice starts to matter less (SQLite is still fine at this scale for a login-rate workload); the real limit becomes the MC server's own tick rate on Pi-class hardware, not the auth/update layer. |
| 100+ players | Not a realistic target for a friends-only offline-mode server; if it ever happened, the auth/file-server layer would need almost no changes (stateless HTTP, trivially horizontally scalable) — the Minecraft server itself would be the bottleneck (single-threaded tick loop), an orthogonal problem. |

### Scaling Priorities

Given the actual scale (5–7 players), there is no realistic near-term bottleneck in the auth/update/launcher layer. The only real capacity concern in this project is the Minecraft server's tick performance on ARM (RLCraft is heavy; see PITFALLS research for tuning), which is unrelated to this architecture question.

## Anti-Patterns

### Anti-Pattern 1: Proxying the Minecraft port through Caddy

**What people do:** Assume "Caddy is my reverse proxy" means all traffic, including port 25565, should route through it.
**Why it's wrong:** Minecraft's protocol is a custom binary TCP protocol, not HTTP/HTTPS. Caddy's `reverse_proxy` directive is HTTP-aware (it parses/rewrites HTTP requests); it cannot proxy the raw Minecraft handshake without a TCP/layer-4 plugin (`caddy-l4`), which adds complexity for zero benefit here since there's nothing to gain (no TLS termination applies to the MC protocol; server-list pings and the protocol itself don't benefit from HTTPS semantics).
**Do this instead:** Port-forward 25565 directly from the router to the Pi. Only 80/443 (Auth Service + File Server, both genuinely HTTP) go through Caddy.

### Anti-Pattern 2: Writing the raw password (or the game-session token) to a plaintext file the launcher reads at every launch

**What people do:** For convenience, some client-side auth mods (and some launchers) store the password/token on disk so it "auto-fills" on every launch, including outside the launcher's control.
**Why it's wrong:** Anyone with filesystem access to the player's machine gets a persistent, possibly long-lived credential. It also decouples "the launcher authenticated the player" from "the game process is allowed to join" — if the file persists, someone could copy just the mods folder + token file to a different machine and join without ever using the launcher/entering credentials.
**Do this instead:** Keep the token in-memory in the launcher process, inject it only as a JVM argument for that one launch, and make it short-lived (minutes) and ideally single-use (Auth Service invalidates it once validated by the server mod). If persistence for "remember me" is wanted, store a separate long-lived *refresh* credential in the OS keychain (Tauri has keychain plugins) — not a plain file, and never the raw password.

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| Mojang piston-meta / resources.download.minecraft.net | Launcher downloads vanilla 1.12.2 client jar, libraries, and assets directly from Mojang's official CDN at first launch / on version mismatch | Legally required per PROJECT.md constraint (no redistributing the client jar/assets) — only mods/configs come from the project's own File Server |
| Forge (Forge Files / Maven) | Either the launcher fetches the Forge 1.12.2 installer/version JSON + universal jar from Forge's own maven at first run, or (simpler) the File Server bundles the exact Forge version JSON + jar alongside the modpack so the launcher never needs to know which Forge build to pick | Bundling via the project's own File Server is simpler and pins the exact Forge build used by the server — recommended |
| Adoptium API (`api.adoptium.net`) | Launcher queries `v3/assets/latest/8/hotspot?architecture=…&image_type=jre&os=…` per platform to fetch a matching Java 8 JRE archive, unpack locally | Only needs to run once (or when missing) per machine; cache the extracted JRE in a launcher-managed directory |
| Let's Encrypt (via Caddy) | Automatic — Caddy handles ACME issuance/renewal for Auth Service + File Server domains | Requires port 80 (or a DNS-01 challenge via a supported DNS provider plugin) reachable from the internet for issuance; home ISP CGNAT would block this — verify the Pi's public IP is not behind carrier-grade NAT before relying on HTTP-01 |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| Launcher ↔ Auth Service | HTTPS, public (443, via Caddy) | Only boundary that ever sees the raw password |
| Launcher ↔ File Server | HTTPS, public (443, via Caddy) | Read-only from the launcher's perspective |
| Server auth-gate mod ↔ Auth Service | HTTP, loopback only (Pi-internal) | No TLS needed for this hop since it never leaves the machine; bind Auth Service's internal validate endpoint to `127.0.0.1` or a private port not exposed by Caddy/router, to prevent it being reachable from the internet |
| Client auth mod ↔ Server auth-gate mod | Minecraft custom plugin-message packet, over the existing game TCP connection (25565) | Piggybacks on the already-established game connection — no separate socket needed |
| Launcher ↔ spawned java process | JVM system property (`-Dauth.token=…`) + constructed classpath/args | One-way, at process-spawn time only |

## Build Order (dependency-driven)

1. **Forge 1.12.2 server on the Pi, offline-mode=true, no custom auth yet.** Validates the hardest infra unknowns first (Java 8 install, RLCraft heap/tuning on ARM, systemd autostart, backups, port forwarding) independent of everything else. Nothing downstream matters if this doesn't work.
2. **Auth Service** (register/login/validate-token HTTP API + DB). Fully standalone — testable with `curl`, no dependency on the launcher, the mods, or the running MC server.
3. **Server-side auth-gate mod + client-side companion mod** (the pair from Pattern 1). Depends on (2) existing (loopback validate call) and (1) existing (a server to install the mod into). Can be developed/tested with a manually-launched vanilla-ish java command (hand-set `-Dauth.token=`) before the launcher exists.
4. **File/update server + manifest generator.** Depends on the modpack file set being reasonably final (RLCraft mods + the two custom auth mods from step 3), but not on the Auth Service or the launcher.
5. **Reverse proxy (Caddy) + domain/TLS**, put in front of (2) and (4). Can be deferred during local development (talk to services over plain HTTP on the LAN) but must be in place before any friend outside the LAN needs to log in.
6. **Tauri launcher.** Built last and largest, because it's the integration point that consumes the *stable contracts* produced by every prior step: the Auth Service's request/response shape, the manifest format, the Forge version JSON, and Java provisioning. Internally sequence it as: (a) login/register UI calling the now-stable Auth Service, (b) manifest diff + download against the now-stable File Server, (c) Java 8 provisioning via Adoptium API, (d) Forge launch-command construction (version JSON merge, classpath, natives, tweakClass) and process spawn with the token injected.
7. **GitHub Actions CI (self-hosted runners) building Windows + macOS launcher binaries.** Deferred until the launcher works reliably from a local dev build — CI should package a known-good app, not be used to debug launcher logic.

Rationale for this order: everything up through step 5 can be fully validated with manual tools (a browser, `curl`, a hand-typed `java` command) *before* the most complex and highest-risk component (the launcher's Forge-launch construction, step 6) is attempted — so any protocol/format mistakes in the auth API or manifest are caught cheaply, not discovered while debugging a cross-platform Rust/Java integration.

## Sources

- [unmojang/drasl (GitHub)](https://github.com/unmojang/drasl) — Yggdrasil-compatible self-hosted auth server; requires online-mode=true, not chosen here
- [yushijinhun/authlib-injector (GitHub)](https://github.com/yushijinhun/authlib-injector) — javaagent-based Yggdrasil redirection
- [authlib-injector issue #33 — Forge client skin bug](https://github.com/yushijinhun/authlib-injector/issues/33) — documented Forge-specific compatibility problem, informed the decision against Pattern 2
- [SeraphJACK/SimpleLogin (GitHub)](https://github.com/SeraphJACK/SimpleLogin) — Forge 1.12+ auth mod, client-stores-credential-and-auto-sends pattern (fallback reference for the mod pair)
- [FMLTweaker Javadoc (Forge 1.12.2)](https://nekoyue.github.io/ForgeJavaDocs-NG/javadoc/1.12.2/net/minecraftforge/fml/common/launcher/FMLTweaker.html) — confirms tweakClass for the Forge 1.12.2 launch chain
- [Minecraft Wiki — version_manifest.json / Game files](https://minecraft.wiki/w/Version_manifest.json) — vanilla version JSON structure (libraries, assets index, sha1 verification) that the launcher must merge with Forge's version JSON
- [GDLauncher — Modpack Manifest Format](https://gdlauncher.com/docs/modpack-manifest-format/) — reference shape for a file-hash-based modpack manifest
- [Adoptium API cookbook (GitHub)](https://github.com/adoptium/api.adoptium.net/blob/main/docs/cookbook.adoc) — `v3/assets/latest/8/hotspot` endpoint for automated per-platform Java 8 JRE fetch
- Caddy automatic HTTPS/reverse-proxy pattern — general knowledge, corroborated by multiple current (2026) setup guides found via search; standard practice, high confidence

---
*Architecture research for: Private modded Minecraft server + custom auth + custom launcher*
*Researched: 2026-08-27*
