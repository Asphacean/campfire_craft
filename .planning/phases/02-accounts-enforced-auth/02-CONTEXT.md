# Phase 2: Accounts & Enforced Auth - Context

**Gathered:** 2026-08-28
**Status:** Ready for planning

<domain>
## Phase Boundary

Own account system + server-side enforcement for the offline-mode RLCraft server: an auth service (register/login/validate) on the Pi, and a Forge 1.12.2 mod that makes the game server accept only players presenting a valid launcher-issued token, kicking vanilla/tokenless clients. Covers AUTH-01, AUTH-02, AUTH-04, AUTH-05. No launcher UI (Phase 4), no public HTTPS exposure of the auth API (Caddy, Phase 3) — the service listens on loopback only in this phase; testing uses a CLI login and a hand-launched client with -D flags.

</domain>

<decisions>
## Implementation Decisions

### Auth Service
- Rust `axum` + SQLite (`sqlx`), single binary `campfire-auth`, systemd unit `campfire-auth.service`, binds `127.0.0.1:8081` only (Caddy fronts it in Phase 3)
- API: `POST /register {nick,password}` → 201 / 409 on duplicate; `POST /login {nick,password}` → `{token, expires}` / 401; `POST /validate {nick,token}` → 200 / 401 (loopback-only caller: the server mod); `GET /status` → server online/player count placeholder for Phase 3/4
- Passwords hashed with argon2id. Tokens: 32 random bytes base64url, TTL 12 h, stored hashed, single-use — consumed on first successful `/validate`
- Registration rules: nick `^[A-Za-z0-9_]{3,16}$`, unique case-insensitively; password ≥ 8 chars; rate limit 5 registrations/hour per IP; open self-registration (no invite code — operator decision)
- Password reset only by operator via CLI: `campfire-auth reset <nick>`; CLI also has `campfire-auth login <nick>` (prints a token) for manual testing

### Auth-gate Mod (Forge 1.12.2)
- ONE mod `campfire-auth` with shared source and `@SideOnly` split; the same jar ships in server `mods/` and in the client pack — versions can't diverge
- Built LOCALLY on the Pi (operator decision): Gradle 4.10.x + ForgeGradle 2.3, Temurin 8 JDK already installed. Expect a slow first `setupDecompWorkspace`; if ForgeGradle proves unworkable on aarch64, fall back to building on an x64 GitHub Actions runner and document it
- Client side: reads `-Dcampfire.nick` / `-Dcampfire.token` JVM properties; on client `PlayerLoggedInEvent`/connect sends `TokenPacket{nick,token}` via `SimpleNetworkWrapper`
- Server side: on `PlayerLoggedInEvent` freezes the player (block movement, interaction, chat, damage), waits ≤ 5 s for the packet, then `POST http://127.0.0.1:8081/validate`; any failure (timeout, HTTP error, 401, missing packet) → kick with message "Зайди через лаунчер campfire.pub / Join via the campfire.pub launcher". Fail-closed always
- No operator bypass list: operator also joins with a token (`campfire-auth login` + -D flags, or the launcher later). Emergency access = RCON

### Testing & Operations
- Manual test path before the launcher exists: `campfire-auth login <nick>` → run the hand-installed client with `-Dcampfire.nick=… -Dcampfire.token=…` and the mod in `mods/`; vanilla-client test = same client without the flags → must be kicked
- Enforcement goes live only after the auth service is up and `/validate` answers; installing the mod on the server = one announced restart. Until then the server stays open as in Phase 1
- DB at `~/rlcraft/auth/campfire.db` (mode 600, gitignored); `scripts/backup.sh` gains a `sqlite3 .backup` step so accounts are in the 6-hourly archive
- Existing Phase-1 players register their own nick (offline UUID is derived from nick → progress preserved). Nick disputes resolved by operator `reset`

### Claude's Discretion
- Exact crate versions, sqlx vs rusqlite, rate-limit implementation, packet channel name, freeze technique (event cancellation vs teleport-back), mod id/version scheme, whether `/status` is stubbed or reads RCON `list`

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `scripts/install-units.sh` + `systemd/` — pattern for adding `campfire-auth.service`
- `server.env` (mode 600, untracked) — add `AUTH_BIND`, `AUTH_DB` keys; `server.env.example` mirrors keys
- `scripts/backup.sh` — flock + RCON save-off pattern; extend with DB backup
- Temurin 8 at `/opt/temurin-8/...` (JAVA8_BIN in server.env) — reuse for Gradle/ForgeGradle

### Established Patterns
- Bash scripts source `server.env`, `set -euo pipefail`, idempotent installers, `bash -n` clean; systemd units installed via script + `daemon-reload`
- Server managed only via `systemctl` / RCON; never reboot the Pi from an executor

### Integration Points
- Mod jar lands in `server/mods/` (tracked source under `mods-src/campfire-auth/` or similar; built jar gitignored or released)
- Phase 3 manifest generator will pick the mod jar up from `server/mods/` automatically
- Phase 4 launcher calls `/register`, `/login` (via Caddy HTTPS) and passes `-Dcampfire.*` to the JVM — keep the API contract documented in `auth-service/README.md`

</code_context>

<specifics>
## Specific Ideas

- Kick message bilingual (RU/EN) — friends are Russian-speaking
- Keep everything on loopback in this phase; nothing new exposed to the internet

</specifics>

<deferred>
## Deferred Ideas

- Invite codes / closed registration — rejected for now, revisit if strangers register
- Skins for offline mode (Drasl) — v2
- Password reset self-service — v2 (AUTH-06)

</deferred>
