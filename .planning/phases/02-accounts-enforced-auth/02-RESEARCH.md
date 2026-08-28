# Phase 2: Accounts & Enforced Auth - Research

**Researched:** 2026-08-28
**Domain:** Small Rust HTTP auth service (axum + SQLite) + a paired Forge 1.12.2 client/server mod that gates join on a validated token
**Confidence:** MEDIUM-HIGH (Rust stack versions and Forge networking API verified against crates.io/official docs and a real production 1.12.2 mod's source; ForgeGradle-on-aarch64 build feasibility is MEDIUM — reasoned from first principles + Gradle/JDK compatibility docs, not directly tested on this exact Pi)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Auth Service**
- Rust `axum` + SQLite (`sqlx`), single binary `campfire-auth`, systemd unit `campfire-auth.service`, binds `127.0.0.1:8081` only (Caddy fronts it in Phase 3)
- API: `POST /register {nick,password}` → 201 / 409 on duplicate; `POST /login {nick,password}` → `{token, expires}` / 401; `POST /validate {nick,token}` → 200 / 401 (loopback-only caller: the server mod); `GET /status` → server online/player count placeholder for Phase 3/4
- Passwords hashed with argon2id. Tokens: 32 random bytes base64url, TTL 12 h, stored hashed, single-use — consumed on first successful `/validate`
- Registration rules: nick `^[A-Za-z0-9_]{3,16}$`, unique case-insensitively; password ≥ 8 chars; rate limit 5 registrations/hour per IP; open self-registration (no invite code — operator decision)
- Password reset only by operator via CLI: `campfire-auth reset <nick>`; CLI also has `campfire-auth login <nick>` (prints a token) for manual testing

**Auth-gate Mod (Forge 1.12.2)**
- ONE mod `campfire-auth` with shared source and `@SideOnly` split; the same jar ships in server `mods/` and in the client pack — versions can't diverge
- Built LOCALLY on the Pi (operator decision): Gradle 4.10.x + ForgeGradle 2.3, Temurin 8 JDK already installed. Expect a slow first `setupDecompWorkspace`; if ForgeGradle proves unworkable on aarch64, fall back to building on an x64 GitHub Actions runner and document it
- Client side: reads `-Dcampfire.nick` / `-Dcampfire.token` JVM properties; on client `PlayerLoggedInEvent`/connect sends `TokenPacket{nick,token}` via `SimpleNetworkWrapper`
- Server side: on `PlayerLoggedInEvent` freezes the player (block movement, interaction, chat, damage), waits ≤ 5 s for the packet, then `POST http://127.0.0.1:8081/validate`; any failure (timeout, HTTP error, 401, missing packet) → kick with message "Зайди через лаунчер campfire.pub / Join via the campfire.pub launcher". Fail-closed always
- No operator bypass list: operator also joins with a token (`campfire-auth login` + -D flags, or the launcher later). Emergency access = RCON

**Testing & Operations**
- Manual test path before the launcher exists: `campfire-auth login <nick>` → run the hand-installed client with `-Dcampfire.nick=… -Dcampfire.token=…` and the mod in `mods/`; vanilla-client test = same client without the flags → must be kicked
- Enforcement goes live only after the auth service is up and `/validate` answers; installing the mod on the server = one announced restart. Until then the server stays open as in Phase 1
- DB at `~/rlcraft/auth/campfire.db` (mode 600, gitignored); `scripts/backup.sh` gains a `sqlite3 .backup` step so accounts are in the 6-hourly archive
- Existing Phase-1 players register their own nick (offline UUID is derived from nick → progress preserved). Nick disputes resolved by operator `reset`

### Claude's Discretion
- Exact crate versions, sqlx vs rusqlite, rate-limit implementation, packet channel name, freeze technique (event cancellation vs teleport-back), mod id/version scheme, whether `/status` is stubbed or reads RCON `list`

### Deferred Ideas (OUT OF SCOPE)
- Invite codes / closed registration — rejected for now, revisit if strangers register
- Skins for offline mode (Drasl) — v2
- Password reset self-service — v2 (AUTH-06)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| AUTH-01 | Player can register an account (nick + password) from the launcher; nick uniqueness enforced, password stored hashed (argon2/bcrypt) | Standard Stack (axum/argon2/rusqlite), Code Examples (`/register` handler + argon2 hashing), Security Domain (V2/V5/V6) |
| AUTH-02 | Player can log in from the launcher and receive a short-lived session token | Standard Stack, Code Examples (`/login` handler, token generation/storage), Common Pitfalls (single-use consumption semantics) |
| AUTH-04 | Game server rejects any join whose token is missing or invalid (server-side auth-gate Forge mod validating against the auth service over loopback); vanilla clients cannot join under a registered nick | Architecture Patterns (server-initiated handshake, freeze technique), Code Examples (SimpleNetworkWrapper, kick, off-thread HTTP call), Common Pitfalls (fail-open trap, event timing) |
| AUTH-05 | Client-side auth mod ships in the modpack and transmits the launcher-provided token on join | Architecture Patterns (shared-jar `@SideOnly` split), Code Examples (client message handler reading `-D` properties), Common Pitfalls (channel name limit, one-jar-both-sides) |
</phase_requirements>

## Summary

This phase is two independent-but-paired pieces of work: a small, boring Rust HTTP service (axum + SQLite, argon2id hashing, opaque bearer tokens) that would look the same on any project, and a genuinely niche piece — a Forge 1.12.2 mod pair that must gate a Minecraft login before the vanilla client-join sequence completes. The Rust side has no real risk: every crate involved (axum 0.8.9, argon2 0.6.0, rusqlite 0.40.2, tower-governor 0.8.0) is current, actively maintained, and verified live on crates.io today. The mod side has one genuine open question (does ForgeGradle 2.3 run cleanly on this aarch64 Pi) and one design decision this research resolves with primary-source evidence: **have the server-side mod initiate the auth handshake by sending a request packet the instant `PlayerLoggedInEvent` fires, and have the client mod's message handler reply synchronously with the token** — not the reverse (client waits for its own "connected" event and sends first). This sidesteps a documented Forge 1.12.2 pitfall (the client-side connect event can fire before the player entity exists) and is exactly the pattern a real, working 1.12.2 auth mod (SeraphJACK/SimpleLogin) uses in production, confirmed by reading its source directly.

Cargo/rustup is **not installed** on the Pi, but Debian trixie's own `cargo`/`rustc` package (1.85.0) comfortably exceeds axum 0.8's MSRV (1.80) — `apt install cargo` is sufficient, no rustup needed. Gradle 4.10.3 (paired with ForgeGradle 2.3) tops out at JDK 11 support; the Pi has only Temurin 8 and system Java 25, neither of which is a coincidence-free fit — Temurin 8 (already installed for the game server) is also the correct JVM to run Gradle itself, and `JAVA_HOME` must be pinned to it explicitly for every `./gradlew` invocation, exactly as `start-server.sh` already pins `JAVA8_BIN` for the game server.

**Primary recommendation:** Build the auth service as a single `campfire-auth` Rust binary (axum 0.8 + rusqlite, not sqlx — simpler for a single-writer, 5–7-user SQLite workload with no async-SQLite payoff), argon2id via the `argon2` crate's default features (no extra flags needed — password-hash + getrandom + alloc are on by default in 0.6.0). Build the mod pair as one Gradle project (ForgeGradle 2.3 + Gradle 4.10.3, MCP `stable_39`, run under `JAVA_HOME=Temurin-8`), with the server mod always initiating the token handshake and using `GameType.SPECTATOR` (not manual event-cancellation) as the freeze mechanism, matching SimpleLogin's verified production pattern.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Account storage (nick, argon2id hash) | API/Backend (`campfire-auth` service) | Database/Storage (SQLite) | Only the auth service ever sees a password; DB is a passive store it owns exclusively |
| Registration validation (nick regex, uniqueness, rate limit) | API/Backend | — | Pure request-handling logic, no game-server involvement |
| Token issuance / TTL / single-use consumption | API/Backend | Database/Storage (hashed token row) | Token lifecycle is entirely the auth service's responsibility; the mod only ever calls `/validate` |
| Token validation on join | Game Server (Forge server mod) | API/Backend (answers the call) | The mod is the enforcement point; the service is the source of truth it defers to — this split must not collapse (mod must never accept a token itself without calling out) |
| Player freeze / gate until validated | Game Server (Forge server mod) | — | Pure game-state manipulation (GameType, position, event cancellation) — has no equivalent in a web tier |
| Token transmission on connect | Client (Forge client mod) | — | Reads JVM system properties injected by the (future) launcher; no server-side or API involvement |
| DB backup | Database/Storage | Ops (`scripts/backup.sh`) | Same tier split as Phase 1's world backup — SQLite file, RCON not involved |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|---------------|
| `axum` | 0.8.9 [VERIFIED: crates.io API, checked 2026-08-28] | HTTP routing/handlers for `/register /login /validate /status` | De facto standard async Rust web framework; Phase-1 research (STACK.md) already named it, version has moved 0.7→0.8.9 since — verify at implementation, do not pin `^0.7` from the older stack doc |
| `tokio` | 1.53.1 [VERIFIED: crates.io API] | Async runtime axum requires | Pulled in transitively by axum; pin `^1` |
| `rusqlite` | 0.40.2 [VERIFIED: crates.io API] | SQLite access for the accounts DB | Sync API is simpler than `sqlx` for a single-writer, ~5-7-row-growth workload with no real query concurrency; avoids sqlx's compile-time query macro/DB-URL-at-build-time ceremony for a two-table schema |
| `argon2` | 0.6.0 [VERIFIED: crates.io API + docs.rs] | Password hashing (argon2id) | `argon2.hash_password(password)?.to_string()` — **0.6.0's default features (`alloc`, `getrandom`, `password-hash`) already include everything needed**; no manual `SaltString::generate(&mut OsRng)` ceremony required (that was the 0.5.x-era pattern; confirmed changed by reading the 0.6.0 docs.rs usage example directly) [VERIFIED: docs.rs/argon2/0.6.0, fetched 2026-08-28] |
| `tower-governor` | 0.8.0 [VERIFIED: crates.io API] | Per-IP rate limiting middleware (5 registrations/hour/IP) | Depends on `axum ^0.8`, `tower ^0.5.1`, `governor ^0.10` — all compatible with the axum 0.8.9 pin above [VERIFIED: crates.io dependency listing for tower-governor 0.8.0] |
| `serde` + `serde_json` | 1.0.229 / 1.0.151 [VERIFIED: crates.io API] | Request/response JSON | Standard, already assumed by every axum service |
| `base64` | 0.23.1 [VERIFIED: crates.io API] | base64url-encode the 32 random token bytes for the API response | `base64::engine::general_purpose::URL_SAFE_NO_PAD` is the correct engine for a URL/header-safe opaque token |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `rusqlite_migration` | 2.6.0 [VERIFIED: crates.io API] | Tiny schema-migration helper (users, tokens tables) using SQLite's `user_version` | Optional — two tables that never change shape could also be a single `CREATE TABLE IF NOT EXISTS` in `main()`; only add this crate if a second migration is anticipated. Ponytail call: **skip it for v1**, a hand-written `CREATE TABLE IF NOT EXISTS` is one function and needs no new dependency |
| `clap` | current 4.x (not separately verified this session — standard, low-risk pin `^4`) [ASSUMED] | `campfire-auth reset <nick>` / `campfire-auth login <nick>` CLI subcommands | Only needed if the single binary also serves as the CLI tool per CONTEXT.md's decision; a hand-rolled `std::env::args()` match is also fine given only 2 subcommands exist — **ponytail: skip clap, match on `args[1]` directly, add clap only if a third subcommand appears** |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `rusqlite` (sync) | `sqlx` 0.9.0 (async) [VERIFIED: crates.io API] | sqlx's SQLite driver is not truly async under the hood (SQLite itself is blocking) — it wraps blocking calls in a thread pool internally either way. For 5-7 users, `rusqlite` + a single writer avoids sqlx's compile-time `DATABASE_URL` macro setup for zero real-world throughput benefit here |
| `argon2` crate's built-in random salt | `rand` 0.8/0.10 crate for the 32-byte token | Do **not** add the `rand` crate as a new dependency: `argon2`'s `password-hash` re-export already exposes `rand_core` — the same OS-backed RNG can generate the 32 token bytes with zero extra crates. **Ponytail: reuse it, don't add `rand`** |
| `tower-governor` | Hand-rolled in-memory `HashMap<IpAddr, Vec<Instant>>` rate limiter behind a `Mutex` | CONTEXT.md marks rate-limit implementation as Claude's discretion. A single global `Mutex<HashMap>` is ~15 lines and needs zero new dependencies for one endpoint (`/register`) at this traffic scale (5-7 users, ever). **Ponytail take: for a single endpoint with this little traffic, the hand-rolled map is arguably simpler than learning `tower-governor`'s `GovernorConfigBuilder` API — pick either, but do not reach for a distributed/Redis-backed limiter, there is exactly one process and one IP range (friends' home networks)** |

**Installation:**
```bash
# Rust toolchain — Debian trixie's own package is sufficient (MSRV check below)
sudo apt install cargo rustc   # installs 1.85.0+dfsg3-1, exceeds axum 0.8's MSRV of 1.80

cd auth-service
cargo init --name campfire-auth
cargo add axum
cargo add tokio --features rt-multi-thread,macros
cargo add rusqlite --features bundled   # `bundled` compiles SQLite from source — no libsqlite3-dev needed
cargo add argon2
cargo add serde serde_json --features serde/derive
cargo add base64
cargo add tower-governor   # only if not hand-rolling the rate limiter
```

**Version verification performed this session:**
```bash
curl -sA "campfire-research/1.0" https://crates.io/api/v1/crates/<name>   # crates.io requires a User-Agent header or returns HTTP 403
```
All eight core/supporting crate versions above were checked live against the crates.io registry API on 2026-08-28 (not from training memory) — see per-row `[VERIFIED]` tags.

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|--------------|---------|-------------|
| axum | crates | since 2021-07-22 | 8.56M/wk | github.com/tokio-rs/axum | OK | Approved |
| argon2 | crates | since 2017-02-28 | 1.38M/wk | github.com/RustCrypto/password-hashes | OK | Approved |
| tower-governor | crates | since 2022-10-16 | 124.9k/wk | github.com/benwis/tower-governor | OK | Approved |
| rusqlite | crates | since 2014-11-21 | 2.42M/wk | github.com/rusqlite/rusqlite | OK | Approved |
| rusqlite_migration | crates | since 2020-11-13 | 175.4k/wk | github.com/cljoly/rusqlite_migration | OK | Approved (optional, see Supporting table) |
| serde / serde_json | crates | since 2014/2015 | 21.9M / 22.3M per wk | github.com/serde-rs/* | OK | Approved |
| tokio | crates | since 2016-07-01 | 16.4M/wk | github.com/tokio-rs/tokio | OK | Approved |

All checked via `gsd-tools query package-legitimacy check --ecosystem crates`. **Packages removed due to [SLOP] verdict:** none. **Packages flagged as suspicious [SUS]:** none.

`clap` was not run through the legitimacy gate this session (it's an optional, skippable dependency per the ponytail note above) — if the planner decides to use it anyway, gate its install behind a quick `npm view`-equivalent check (`cargo search clap` / crates.io lookup) before adding.

## Architecture Patterns

### System Architecture Diagram

```
                         ┌─────────────────────────────────────────┐
                         │      campfire-auth (Rust, axum)          │
                         │      binds 127.0.0.1:8081 ONLY           │
                         │                                           │
CLI (operator, testing)  │  POST /register {nick,pw} ──► 201/409    │
 `campfire-auth login`──►│  POST /login {nick,pw}    ──► {token}/401│◄─┐
 `campfire-auth reset`   │  POST /validate {nick,token}──► 200/401  │  │ loopback
                         │  GET  /status  ──► stub or RCON `list`   │  │ HTTP only
                         │                                           │  │
                         │  SQLite: users(nick, argon2_hash),        │  │
                         │  tokens(nick, argon2_hash_of_token, exp,  │  │
                         │         consumed)                         │  │
                         └───────────────────────────────────────────┘  │
                                                                          │
┌───────────────────────────── Forge 1.12.2 game server ─────────────┐  │
│                                                                       │  │
│  1. PlayerEvent.PlayerLoggedInEvent fires (server-side)              │  │
│  2. Mod: player.setGameType(SPECTATOR); send AuthRequest packet ──┐  │  │
│  3. (background thread, ≤5s timeout)                              │  │  │
│  4. Client mod's IMessageHandler replies synchronously with       │  │  │
│     AuthResponse{nick, token} read from -D JVM properties         │  │  │
│  5. Server mod: new Thread → HttpURLConnection POST /validate ────┼──┘  │
│     result delivered back via addScheduledTask(...) ───────────────────┘
│  6a. valid  → restore original GameType, let player play             │
│  6b. invalid/timeout/missing → player.connection.disconnect(kick msg) │
└────────────────────────────────────────────────────────────────────┘
```

### Recommended Project Structure
```
auth-service/
├── Cargo.toml
├── src/
│   ├── main.rs          # axum router, server bind, CLI dispatch (register/login/validate/status vs `reset`/`login` subcommands)
│   ├── db.rs             # rusqlite connection + schema init + queries
│   ├── auth.rs           # argon2 hash/verify, token generate/hash/consume
│   └── ratelimit.rs       # only if hand-rolling; otherwise tower-governor config lives in main.rs
└── campfire.db            # gitignored, mode 600, created at first run (path from server.env AUTH_DB)

mods-src/campfire-auth/     # tracked Gradle project, builds server+client jar
├── build.gradle             # ForgeGradle 2.3, mappings stable_39, forge 1.12.2-14.23.5.2860
├── src/main/java/pub/campfire/auth/
│   ├── CampfireAuth.java             # @Mod entry point, mcmod.info
│   ├── network/
│   │   ├── NetworkHandler.java        # NetworkRegistry.INSTANCE.newSimpleChannel("campfireauth")
│   │   ├── AuthRequestMessage.java    # server → client (empty payload)
│   │   └── AuthResponseMessage.java   # client → server {nick, token}
│   ├── server/
│   │   └── ServerAuthHandler.java     # @SideOnly(Side.SERVER): PlayerLoggedInEvent, freeze, HTTP validate, kick
│   └── client/
│       └── ClientAuthHandler.java     # @SideOnly(Side.CLIENT): IMessageHandler reads -Dcampfire.nick/-Dcampfire.token
└── src/main/resources/mcmod.info
```

### Pattern 1: Server-initiated auth handshake (not client-initiated)

**What:** The server mod sends the `AuthRequest` packet the instant `PlayerEvent.PlayerLoggedInEvent` fires server-side. The client's registered `IMessageHandler<AuthRequestMessage, AuthResponseMessage>` responds **synchronously**, returning an `AuthResponseMessage` built from `System.getProperty("campfire.nick")` / `System.getProperty("campfire.token")`. The client mod never needs to listen for its own "connected to server" event at all.

**When to use:** This is the only pattern this research recommends for AUTH-04/AUTH-05 — see Common Pitfalls for why the client-initiated alternative (listening for `FMLNetworkEvent.ClientConnectedToServerEvent`) is fragile.

**Example (verified pattern, adapted from SeraphJACK/SimpleLogin's production 1.12.2 source, read directly from `github.com/SeraphJACK/SimpleLogin` branch `mc-1.12.2`):**
```java
// Server side — server/ServerAuthHandler.java
// Source pattern: SeraphJACK/SimpleLogin, src/main/java/.../server/ServerSideEventHandler.java
@Mod.EventBusSubscriber(value = Side.SERVER, modid = CampfireAuth.MODID)
public class ServerAuthHandler {
    @SubscribeEvent
    public static void onPlayerJoin(PlayerEvent.PlayerLoggedInEvent event) {
        EntityPlayerMP player = (EntityPlayerMP) event.player;
        FMLCommonHandler.instance().getMinecraftServerInstance().addScheduledTask(() -> {
            player.setGameType(GameType.SPECTATOR);          // freeze: see Pattern 2
            NetworkHandler.INSTANCE.sendTo(new AuthRequestMessage(), player);
            // schedule a ≤5s timeout kick (e.g. via a ScheduledExecutorService or a tick-counted map)
        });
    }
}
```
```java
// Client side — client/ClientAuthHandler.java
// Verified IMessageHandler shape from Forge's own SimpleImpl docs
// (docs.minecraftforge.net/en/1.12.x/networking/simpleimpl/, fetched 2026-08-28)
public class AuthRequestMessage implements IMessage {
    @Override public void toBytes(ByteBuf buf) {}
    @Override public void fromBytes(ByteBuf buf) {}

    public static class Handler implements IMessageHandler<AuthRequestMessage, AuthResponseMessage> {
        @Override
        public AuthResponseMessage onMessage(AuthRequestMessage message, MessageContext ctx) {
            String nick = System.getProperty("campfire.nick", "");
            String token = System.getProperty("campfire.token", "");
            return new AuthResponseMessage(nick, token);   // reply, no scheduling needed — read-only property access is thread-safe
        }
    }
}
```

**Trade-offs:** The server must now own a short timeout (kick if no `AuthResponseMessage` arrives within 5s) instead of relying on the client to have sent something proactively — a small amount of extra bookkeeping (a `Map<UUID, long>` of join timestamps checked on a tick handler), but it removes an entire class of "did the client's connect event fire before or after the player entity existed" bugs.

### Pattern 2: Freeze via `GameType.SPECTATOR`, not per-event cancellation

**What:** On join, before validation completes, `player.setGameType(GameType.SPECTATOR)`. Spectator mode natively disables collision-based movement into the world (the player can still fly through blocks, which is visually a bit odd for ~1-5 seconds but is not a security gap since nothing they do in spectator mode persists — no block breaking, no entity interaction, no attacking, no item use). On successful validation, restore the player's real game mode (`GameType.SURVIVAL` or whatever the world default is) and re-run any position/inventory sync the game mode switch requires.

**When to use:** Recommended default for this phase. **Verified as the exact technique used by a real, working, currently-listed Forge 1.12.2 auth mod** (SeraphJACK/SimpleLogin, `server/PlayerLoginHandler.java`, read directly) — not a synthesized guess.

**Gap this doesn't close:** Spectator mode does **not** block chat. If chat-before-auth needs to be blocked too (CONTEXT.md's freeze list includes "chat"), also cancel `ServerChatEvent` while the player's UUID is in the pending-auth set — this is the one piece SimpleLogin's own source doesn't demonstrate directly (it instead blocks *commands* via `CommandEvent`, not chat messages) — treat as a small addition alongside the spectator-mode base pattern, not a replacement for it.

**Trade-offs vs. cancelling `LivingUpdateEvent`/`PlayerInteractEvent`/`LivingAttackEvent` individually:** One line to enter, one line to exit, versus 3-4 separate `@SubscribeEvent` cancellation handlers that each have their own edge cases (e.g., does cancelling `LivingAttackEvent` also stop fall damage/environmental damage, or only PvP/mob attacks — the two are different events in 1.12.2, `LivingAttackEvent` vs `LivingHurtEvent`/`LivingDamageEvent`). Spectator mode is a single Mojang-maintained state that already handles all of these consistently. **Ponytail: use spectator mode as the base, add exactly one more cancellation (`ServerChatEvent`) for chat — do not hand-roll a fourth or fifth event cancellation unless a real gap is found during testing.**

### Pattern 3: HTTP validate call off the main thread, result delivered back via `addScheduledTask`

**What:** Forge 1.12.2's main server thread must never block on network I/O (an unresponsive/slow `/validate` call would stall the whole game tick for every player). Spawn a plain `java.lang.Thread` that performs the blocking `HttpURLConnection` call to `http://127.0.0.1:8081/validate`, then hands the boolean result back to the main thread via `FMLCommonHandler.instance().getMinecraftServerInstance().addScheduledTask(Runnable)` — the exact API a real 1.12.2 mod uses for its own background-thread-to-main-thread handoff (verified: `PlayerLoginHandler.java`'s login-handler thread does exactly this for position resets).

**Example:**
```java
// Server side — off-thread HTTP call, main-thread result delivery
// addScheduledTask API verified against SeraphJACK/SimpleLogin production source
new Thread(() -> {
    boolean valid;
    try {
        HttpURLConnection conn = (HttpURLConnection) new URL("http://127.0.0.1:8081/validate").openConnection();
        conn.setRequestMethod("POST");
        conn.setDoOutput(true);
        conn.setConnectTimeout(3000);
        conn.setReadTimeout(3000);
        conn.getOutputStream().write(jsonBody.getBytes(StandardCharsets.UTF_8));
        valid = conn.getResponseCode() == 200;
    } catch (Exception e) {
        valid = false;   // fail-closed: any network error, timeout, or non-200 => reject
    }
    boolean finalValid = valid;
    FMLCommonHandler.instance().getMinecraftServerInstance().addScheduledTask(() -> {
        if (finalValid) {
            player.setGameType(worldDefaultGameType);
        } else {
            player.connection.disconnect(new TextComponentString(KICK_MESSAGE));
        }
    });
}, "campfire-auth-validate").start();
```

**Trade-offs:** A raw `Thread` per join is fine at 5-7 concurrent players; do not build a thread pool/executor abstraction for this scale (**ponytail: one `Thread` per validation call, no `ExecutorService`, no connection pooling — add only if join volume ever becomes a real number**).

### Anti-Patterns to Avoid

- **Client listens for `FMLNetworkEvent.ClientConnectedToServerEvent` and sends the token proactively:** documented timing hazard — the player entity (`Minecraft.getMinecraft().player`) can be `null` at the moment this event fires, since it fires during the network handshake before the player object exists in the world. Combined with the fact that a server-initiated request (Pattern 1) sidesteps the need for this event entirely, there's no reason to touch it.
- **Treating any auth-service error as "allow":** every failure mode (timeout, connection refused, HTTP 5xx, malformed response, missing packet) must resolve to "kick" — CONTEXT.md already locks this as fail-closed; this research corroborates it as the load-bearing security property of the whole design (see Phase 1's own PITFALLS.md Pitfall 3, and STACK.md's "known limitation" callout: offline-mode itself has zero protocol-level enforcement, so this mod IS the entire enforcement layer).
- **Reimplementing the freeze via 4-5 separate event cancellations** when `GameType.SPECTATOR` already covers movement/interaction/attack in one line (see Pattern 2).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|--------------|-----|
| Password hashing | Custom PBKDF2/SHA loop | `argon2` crate, default features | Memory-hard, side-channel-resistant, maintained by RustCrypto; CONTEXT.md already locks argon2id |
| Constant-time password comparison | Manual byte-compare loop | `argon2::Argon2::verify_password` | The crate's verify path already handles this correctly; a hand-rolled comparison is the classic place to accidentally introduce a timing side-channel |
| Random token bytes | `java.util.Random` / a hand-rolled PRNG (Java side) or a non-cryptographic RNG (Rust side) | Rust: `argon2`'s re-exported `rand_core::OsRng` (no new crate). Java (mod side never generates tokens — only the Rust service does) | Tokens are bearer credentials; a predictable RNG defeats the whole scheme |
| SQLite schema migrations | A bespoke version-tracking table | `rusqlite_migration` (optional) or a plain `CREATE TABLE IF NOT EXISTS` for this small a schema | Don't add the crate unless a second migration is actually anticipated — ponytail call already made above |
| Rate limiting | A hand-rolled sliding-window algorithm with edge-case bugs | `tower-governor`, OR a single `Mutex<HashMap<IpAddr, Vec<Instant>>>` for one endpoint at this traffic scale | Both are legitimate; don't build a distributed/production-grade limiter for 5-7 friends |
| Offline-mode UUID derivation | A custom UUID scheme for accounts | Vanilla's own `UUID.nameUUIDFromBytes(("OfflinePlayer:" + nick).getBytes(UTF_8))` — do not invent a different offline-UUID formula, the game client computes its own UUID this way regardless of what the auth service thinks, and it must match | Player data files are keyed by this UUID; diverging from Mojang's own offline formula orphans a player's inventory/progress |

**Key insight:** Every "don't hand-roll" item above already has an existing, load-bearing implementation somewhere in the stack (the Rust crypto ecosystem, or Minecraft/Forge itself) — the actual custom work in this phase is entirely in *wiring these together* (the packet handshake, the freeze-then-release state machine, the fail-closed error handling), not in reimplementing any of the primitives.

## Common Pitfalls

### Pitfall 1: `ClientConnectedToServerEvent` fires with a null player

**What goes wrong:** A client mod that tries to read `Minecraft.getMinecraft().player` (or the 1.12.2-era `thePlayer`) inside a handler for `FMLNetworkEvent.ClientConnectedToServerEvent` gets `null`, because this event fires during the network handshake, before the player entity is constructed.
**Why it happens:** The event is about the *connection*, not the *player* — its existence predates the player object in the client's world state.
**How to avoid:** Use Pattern 1 (server-initiated handshake) so the client mod never needs this event — it just replies to a packet whenever the server sends it, at which point `System.getProperty(...)` reads (no player-object dependency at all) are always safe.
**Warning signs:** `NullPointerException` on `player.something()` inside a connect-event handler, or a token that "sometimes" doesn't send on slower connections.

### Pitfall 2: Gradle/JDK version mismatch for the local ForgeGradle build

**What goes wrong:** Gradle 4.10.3 (the version ForgeGradle 2.3 expects) only supports running its daemon on JDK versions up to roughly 10/11 [CITED: Gradle 4.10 release notes / compatibility discussion]. This Pi has exactly two JDKs installed: system Java 25 (way too new) and Temurin 8 (already installed for the game server). If `./gradlew` is invoked with `JAVA_HOME` pointing at the system Java 25, Gradle will fail to even start the daemon.
**Why it happens:** Old Gradle versions predate modern JDKs by 7+ years and were never updated to understand their bytecode/module changes.
**How to avoid:** Explicitly export `JAVA_HOME` to the Temurin 8 install path (same absolute path already used as `JAVA8_BIN` in `server.env`) before every `./gradlew` invocation — mirror the existing pattern in `scripts/start-server.sh` that refuses to run if the JVM doesn't report `1.8.0`. A wrapper script (`mods-src/campfire-auth/build.sh` or similar) that sets `JAVA_HOME` and calls `./gradlew build` (not `runClient`/`runServer` — those need LWJGL natives this project never needs, since the mod is only ever compiled here, not run here) is the safe path.
**Warning signs:** Gradle daemon fails with an "Unsupported class file major version" or similar error the instant `./gradlew` is invoked.

### Pitfall 3: `setupDecompWorkspace` / build memory pressure while the game server is also running

**What goes wrong:** ForgeGradle 2.3's decompile task is commonly reported needing ~3GB+ of JVM heap for the Gradle daemon itself [CITED: multiple ForgeGradle GitHub issues/forum threads, corroborated across independent sources] — separate from whatever heap the actual Minecraft server JVM is using.
**Why it happens:** MCP decompilation of the full vanilla+Forge jar is a heavyweight one-time operation.
**How to avoid:** Checked live on this Pi (2026-08-28): with the RLCraft server running (6GB heap reserved via `-XX:+AlwaysPreTouch`), `free -h` reports only ~150MB literally free but **5.8GB "available"** (reclaimable from buff/cache) — enough headroom for a 3-4GB Gradle heap without stopping the game server. If the build still OOMs, the fallback is a one-time `sudo systemctl stop rlcraft` for the duration of the first `setupDecompWorkspace` run only (it caches its output; subsequent builds are much cheaper), then restart.
**Warning signs:** `GC overhead limit exceeded` or `OutOfMemoryError: Java heap space` during `./gradlew build`. Fix: add `org.gradle.jvmargs=-Xmx3G` to `gradle.properties` (this is the exact fix reported across multiple independent ForgeGradle issue threads, not a guess).

### Pitfall 4: Channel name length limit (Forge 1.12.2 `SimpleNetworkWrapper`)

**What goes wrong:** A channel name over 20 characters throws a `RuntimeException` at mod init.
**Why it happens:** Forge 1.12.2's networking layer hard-codes a 20-character limit on the plugin-channel identifier.
**How to avoid:** `NetworkRegistry.INSTANCE.newSimpleChannel("campfireauth")` (12 chars) or the modid itself if it's short enough — verified against Forge's own `docs.minecraftforge.net/en/1.12.x/networking/simpleimpl/` guidance to keep the channel name to "a short identifier ... typically just your mod ID."
**Warning signs:** Crash at server/client startup referencing channel name length, not a runtime networking failure.

### Pitfall 5: Nick case sensitivity vs. offline UUID derivation

**What goes wrong:** CONTEXT.md's registration rule enforces nick uniqueness *case-insensitively* (good for account collision avoidance), but the game itself derives the offline UUID from the *exact byte string* the client sends as its username (`UUID.nameUUIDFromBytes(("OfflinePlayer:" + nick).getBytes(UTF_8))`). If a player registers as `Steve` but the launcher (or a hand-launched client, during manual testing) ever connects with `steve`, the game computes a *different* UUID and treats them as a brand-new player — losing inventory/progress silently, with no error at all.
**Why it happens:** The account layer's case-insensitivity and the game protocol's case-sensitive identity derivation are two independent systems that only agree if the exact same casing is used every time.
**How to avoid:** Store and always echo back the nick's *original registration casing* from the auth service (not the lowercased form used for the uniqueness index), and make sure the client mod/launcher always connects with that exact casing — never let the player free-type a differently-cased nick into the join flow.
**Warning signs:** A returning player reports "my stuff is gone" despite a correct login — check whether the connecting username's case matches their original registration exactly.

### Pitfall 6: Token single-use semantics vs. retry-on-transient-failure

**What goes wrong:** CONTEXT.md locks tokens as single-use, "consumed on first successful `/validate`." If the mod's HTTP call to `/validate` times out or the response is lost in transit *after* the auth service already marked the token consumed, a retry from the mod will get a 401 even though the original attempt was "morally" successful — the player gets kicked despite having done everything right.
**Why it happens:** Consuming a token and confirming delivery of that fact to the caller are two separate steps that can be split by a network failure.
**How to avoid:** Given a loopback-only call (no real network unreliability expected) and a 12-hour token TTL, the pragmatic answer for this phase's scale is: don't build retry logic into the mod at all — one `/validate` attempt, fail-closed on any non-200. If a legitimate player gets unlucky, they re-run `campfire-auth login <nick>` for a fresh token (this is already the CLI-testing workflow CONTEXT.md describes) — this is an acceptable manual-recovery cost at 5-7 users and should not be engineered around.
**Warning signs:** None expected on loopback; document this as an accepted, low-probability rough edge rather than building complexity to prevent it.

## Code Examples

### `/register` handler (axum + rusqlite + argon2, sketch)
```rust
// Source: argon2 0.6.0 API confirmed via docs.rs/argon2/0.6.0 usage example, fetched 2026-08-28
use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, PasswordHash, PasswordVerifier};

async fn register(/* nick, password, State(db) */) -> StatusCode {
    // nick regex ^[A-Za-z0-9_]{3,16}$ checked before this point (CONTEXT.md)
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(password.as_bytes())
        .expect("hashing failure")   // OOM-class failure only; not a request-data problem
        .to_string();                 // "$argon2id$v=19$..." — store this whole string
    // INSERT OR IGNORE / check uniqueness case-insensitively (COLLATE NOCASE index or lowercase-column uniqueness)
    // 201 on success, 409 if nick already exists
    StatusCode::CREATED
}
```

### Kick with a bilingual message (verified exact API)
```java
// Source: player.connection.disconnect(...) API confirmed by reading
// SeraphJACK/SimpleLogin's server/PlayerLoginHandler.java directly (mc-1.12.2 branch)
player.connection.disconnect(new TextComponentString(
    "Зайди через лаунчер campfire.pub / Join via the campfire.pub launcher"));
```

### SimpleNetworkWrapper registration (verified against Forge's own docs)
```java
// Source: docs.minecraftforge.net/en/1.12.x/networking/simpleimpl/, fetched 2026-08-28
public static final SimpleNetworkWrapper INSTANCE =
    NetworkRegistry.INSTANCE.newSimpleChannel("campfireauth");

static {
    INSTANCE.registerMessage(AuthRequestMessage.Handler.class, AuthRequestMessage.class, 0, Side.CLIENT);
    INSTANCE.registerMessage(AuthResponseMessage.Handler.class, AuthResponseMessage.class, 1, Side.SERVER);
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|-------------------|---------------|--------|
| `argon2` crate manual salt generation (`SaltString::generate(&mut OsRng)`) | `argon2.hash_password(pw)?.to_string()` with default features auto-generating the salt | argon2 crate 0.6.0 (per docs.rs, checked 2026-08-28) | Fewer lines, one less import; older tutorials/blog posts referencing 0.4.x/0.5.x API will show the longer form — don't copy that pattern verbatim |
| axum `^0.7` (named in Phase 1's STACK.md) | axum 0.8.9 | Some point after Phase 1's research (2026-08-27) | Re-verify axum's router/handler API shape at implementation time — 0.7→0.8 was not zero-diff historically (routing/state API had breaking changes across the 0.7→0.8 boundary in axum's own changelog); don't assume Phase 1's STACK.md pin is still accurate |

**Deprecated/outdated:**
- SimpleLogin's own chat-based `/login <password>` UX (relevant only as a fallback per Phase 1's ARCHITECTURE.md) is explicitly NOT what this phase builds — the token-handshake pattern (Pattern 1 above) replaces it entirely; SimpleLogin's source was consulted here purely for its verified freeze/kick/networking *mechanics*, not its user-facing login flow.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `clap` crate is a reasonable, low-risk choice if the CLI subcommand count grows past 2 | Standard Stack (Supporting) | Low — not currently recommended for v1; only matters if scope grows |
| A2 | `getrandom`/`rand_core::OsRng`'s `fill_bytes` API signature is stable in the way described | Don't Hand-Roll | Low — extremely standard, long-stable API shape in the Rust ecosystem; worst case is a one-line API name fix at implementation time, not a design problem |
| A3 | Forge 1.12.2 mods still use `mcmod.info` (not `mods.toml`, which is 1.13+) for mod metadata | Recommended Project Structure | Low — well-established fact about 1.12.2 specifically, not verified by opening a live example this session |
| A4 | `LivingAttackEvent` vs `LivingHurtEvent`/`LivingDamageEvent` distinction (mentioned only to justify preferring spectator mode over hand-cancelling events) | Architecture Patterns, Pattern 2 | Low — this claim isn't load-bearing for the recommended design (spectator mode is recommended precisely to avoid needing to get this distinction right) |

**If this table is empty:** N/A — see rows above. Everything else load-bearing in this document (crate versions, the SimpleNetworkWrapper API, the freeze/kick technique, the offline-UUID formula, the Gradle/JDK compatibility ceiling, live registry reachability) was checked against an authoritative source or a real production mod's source code during this research session.

## Open Questions

1. **Does ForgeGradle 2.3's `setupDecompWorkspace` actually complete cleanly on this specific aarch64 Debian 13 Pi?**
   - What we know: No native-library dependency should block it (the concern only applies to `runClient`/`runServer` Gradle tasks, which this phase never needs — only `build`/`setupDecompWorkspace`/`compileJava` are required). Gradle 4.10.3 + Temurin 8 is a documented-compatible combination. Both the official ForgeGradle 2.3-SNAPSHOT artifact and a maintained fork (`anatawa12/ForgeGradle-2.3`, latest `2.3-1.0.8`) resolve live from their respective Maven repos as of 2026-08-28.
   - What's unclear: Nobody has run this exact combination (ForgeGradle 2.3, Gradle 4.10.3, aarch64 Debian 13, Temurin 8) and confirmed a clean decompile — this is a first-principles-reasoned "should work," not an observed pass.
   - Recommendation: Attempt the local build first (CONTEXT.md's own decision). If `setupDecompWorkspace` fails in a way that looks aarch64-specific (not just a memory/JAVA_HOME misconfiguration, which are both fixable per Pitfalls 2/3 above), fall back to the anatawa12 fork before escalating to CONTEXT.md's documented fallback (an x64 GitHub Actions runner).

2. **Exact `/status` implementation: stub vs. RCON `list` parse?**
   - What we know: CONTEXT.md marks this as Claude's discretion. RCON is already loopback-only and password-protected per Phase 1's `harden-rcon.sh`/`rcon-cli` pattern, and `scripts/backup.sh` already demonstrates the exact `rcon-cli` invocation shape (env-var credentials, never CLI flags).
   - What's unclear: Whether Phase 3/4 need real player-count data from `/status` immediately, or whether a placeholder (`{"online": true, "players": null}`) is acceptable until then.
   - Recommendation: Ship a stub for this phase (`/status` returns a fixed "online":true shape, no RCON call) — Phase 3/4's own research/planning is the right place to decide if/when it becomes a real RCON-backed value. Ponytail: don't build the RCON integration until a consumer actually needs the number.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|----------|
| Rust toolchain (cargo/rustc) | auth-service build | ✗ (not installed) | Debian trixie apt candidate: 1.85.0+dfsg3-1 | `sudo apt install cargo rustc` — no rustup needed, apt version exceeds axum 0.8's MSRV (1.80) |
| `sqlite3` CLI | `scripts/backup.sh`'s planned `.backup` step | ✗ (not installed; only `libsqlite3-0` lib present) | apt candidate 3.46.1-7+deb13u1 | `sudo apt install sqlite3`, OR skip the CLI entirely and use `VACUUM INTO 'path'` (a plain SQL statement, supported since SQLite 3.27, works via `rusqlite`/any connection with zero new packages) |
| Gradle (for ForgeGradle build) | Mod build | ✗ (not installed) | Need 4.10.3 specifically (paired with ForgeGradle 2.3) | Use the Gradle Wrapper (`gradlew`) checked into the mod project — never require a system-wide Gradle install; the wrapper self-downloads the pinned version on first run |
| Temurin 8 JDK | Both the game server (existing) AND running Gradle itself for this build | ✓ | 1.8.0_504-b01 at `/opt/temurin-8/jdk8u504-b01` | None needed — already installed and working (Phase 1) |
| System Java | N/A — must NOT be used for either Gradle or the mod's target JVM | ✓ (25.0.3) | 25.0.3 | Actively avoid: `JAVA_HOME` must point at Temurin 8 for every Gradle invocation (Pitfall 2) |

**Missing dependencies with no fallback:** none — every gap above has a documented, low-risk fix (all are simple `apt install` or wrapper-script commands).

**Missing dependencies with fallback:** Rust toolchain (apt, not rustup), `sqlite3` CLI (apt, or skip via `VACUUM INTO`), Gradle (wrapper, not system install).

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|----------------|---------|-------------------|
| V2 Authentication | yes | argon2id password hashing (default-feature `argon2` crate), opaque random bearer tokens (32 bytes, CSPRNG via `rand_core::OsRng`), rate-limited registration |
| V3 Session Management | yes | Short TTL (12h), single-use consumption on `/validate`, tokens stored hashed (not plaintext) in SQLite — a DB leak does not leak usable tokens, only argon2-hashed ones |
| V4 Access Control | partial | Single-role system (no admin/player distinction in this phase) — operator actions (`reset`) are CLI-only, not exposed over HTTP at all, which is itself the access-control boundary |
| V5 Input Validation | yes | Nick regex (`^[A-Za-z0-9_]{3,16}$`), password minimum length (≥8), rejecting malformed JSON via axum's own extractor error handling |
| V6 Cryptography | yes | argon2id via a vetted crate (never hand-rolled); random token generation via OS CSPRNG (never `rand::thread_rng()`'s non-cryptographic paths, and never a Java-side `java.util.Random`) |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|-----------------------|
| Credential stuffing / brute-force login | Spoofing | Rate limiting on `/register` (locked at 5/hr/IP per CONTEXT.md); consider the same for `/login` at implementation time even though CONTEXT.md doesn't explicitly lock it — argon2id's own cost already slows brute force, but a rate limit is cheap insurance |
| SQL injection | Tampering | `rusqlite`'s parameterized query API (`?1`/named params) — never string-format SQL with user input |
| Password/token DB leak | Information Disclosure | argon2id hashes only (passwords), hashed tokens only (never store the raw bearer token server-side) — CONTEXT.md already locks both; DB file is mode 600 |
| Token replay after consumption | Tampering / Elevation of Privilege | Single-use enforcement: the `/validate` handler must mark-and-check consumption atomically (a single SQL `UPDATE ... WHERE consumed=0 RETURNING ...`-style statement, or an explicit transaction) — a check-then-set race would allow a captured token to be replayed twice before expiry |
| Fail-open on service/network error | Elevation of Privilege | The entire mod design is fail-closed by CONTEXT.md decision; this research corroborates it as the single most important property to preserve during implementation and testing |
| Vanilla client joining under someone else's registered nick | Spoofing | This is exactly what AUTH-04's server-side gate exists to prevent — the mod, not the launcher, is the enforcement boundary (Phase 1 STACK.md's "known limitation" section already documents that offline-mode alone provides zero such protection) |

## Sources

### Primary (HIGH confidence)
- crates.io registry API (`crates.io/api/v1/crates/<name>`) — live version/download/repo checks for axum, argon2, tower-governor, rusqlite, rusqlite_migration, serde, serde_json, tokio, base64, getrandom — checked 2026-08-28
- docs.rs/argon2/0.6.0 — usage example and feature-flag listing, fetched directly, 2026-08-28
- docs.minecraftforge.net/en/1.12.x/networking/simpleimpl/ — SimpleNetworkWrapper/IMessage/IMessageHandler API, fetched directly, 2026-08-28
- github.com/SeraphJACK/SimpleLogin (branch `mc-1.12.2`) — real production Forge 1.12.2 auth mod, read directly: `server/ServerSideEventHandler.java`, `server/PlayerLoginHandler.java`, `client/ClientProxy.java`, `client/PasswordStorage.java`, `network/NetworkLoader.java`, `network/MessageRequestLogin.java`, `network/MessageLogin.java` — confirms `PlayerEvent.PlayerLoggedInEvent` fires server-side, `player.connection.disconnect(...)` kick API, `GameType.SPECTATOR` freeze technique, `NetworkRegistry.INSTANCE.newSimpleChannel(MODID)` channel pattern, `FMLCommonHandler.instance().getMinecraftServerInstance().addScheduledTask(...)` main-thread handoff
- Live connectivity checks against `files.minecraftforge.net`, `maven.minecraftforge.net` (ForgeGradle 2.3-SNAPSHOT metadata), `repo1.maven.org` (anatawa12 fork metadata) — all HTTP 200, checked 2026-08-28
- `apt-cache policy cargo rustc sqlite3` on the actual Pi — checked live, 2026-08-28
- `free -h` on the actual Pi (with the game server running) — checked live, 2026-08-28

### Secondary (MEDIUM confidence)
- WebSearch corroborated by multiple independent forum/GitHub-issue threads: ForgeGradle 2.3's `setupDecompWorkspace` memory requirement (~3GB+ heap)
- WebSearch: OWASP 2026 argon2id baseline parameters (t=2, m=19MiB, p=1, tune for 50-250ms on target hardware)
- WebSearch: offline-mode UUID derivation formula (`MD5("OfflinePlayer:" + nick)` as UUID v3), corroborated across multiple independent implementations (Bukkit/Spigot/Paper all documented as calling the same method)
- Gradle 4.10 JDK-daemon-support ceiling (~JDK 11) — pieced together from Gradle release notes search results, not a single canonical compatibility table fetched directly

### Tertiary (LOW confidence)
- `clap` crate suitability (not independently verified this session — marked ASSUMED, low-risk, and not recommended for v1 anyway)
- Exact `getrandom`/`rand_core` API signature for filling a byte buffer (extremely standard, but not opened/read directly this session)
- `mcmod.info` vs `mods.toml` for Forge 1.12.2 metadata (well-established but not verified against a live example this session)

## Metadata

**Confidence breakdown:**
- Standard stack (Rust crates): HIGH — every version live-verified against crates.io on 2026-08-28
- Architecture (Forge networking/freeze/kick patterns): HIGH — verified against a real production mod's source, not synthesized
- ForgeGradle-on-aarch64 build feasibility: MEDIUM — reasoned from compatibility docs + live repo reachability checks, not an observed successful build on this exact hardware
- Pitfalls: HIGH for the ones tied to verified source/live checks (channel length, GameType freeze, Gradle/JDK ceiling); MEDIUM for the ForgeGradle memory pitfall (community-corroborated, not first-party Forge documentation)

**Research date:** 2026-08-28
**Valid until:** ~30 days for the Rust crate versions (fast-moving ecosystem norms), effectively indefinite for the Forge 1.12.2 API surface (frozen since ~2022 per Phase 1's own STACK.md finding)
