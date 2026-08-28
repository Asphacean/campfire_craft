# campfire-auth

The account and token-issuance service Phase 3 (Caddy) and Phase 4 (the
launcher) build against. A Rust `axum` + SQLite binary that answers the one
question the game server asks on every join: *is this nick presenting a
token I issued?* Loopback-only this phase (D-16) — binds `127.0.0.1:8081`
only, and refuses to start on any other bind address.

## API contract

All requests and responses are JSON. Error bodies are always
`{"error": "<code>"}` with one of the stable, machine-readable codes below —
Phase 4's launcher matches on these to show a human-readable message.

### `POST /register`

Request: `{"nick": "<3-16 chars, [A-Za-z0-9_]>", "password": "<>=8 chars>"}`

| Status | Body | When |
|--------|------|------|
| 201 | (empty) | Account created |
| 400 | `{"error":"invalid_nick"}` | Nick fails `^[A-Za-z0-9_]{3,16}$` |
| 400 | `{"error":"weak_password"}` | Password shorter than 8 characters |
| 400 | `{"error":"bad_json"}` | Body is not valid JSON, or a field is missing/mistyped |
| 409 | `{"error":"nick_taken"}` | Nick already registered, case-insensitively |
| 429 | `{"error":"rate_limited"}` | More than 5 registration attempts from this peer address in the last hour (every attempt counts, successful or not) |

Nick uniqueness is case-insensitive, but the *exact registration casing* is
stored and always echoed back by `/validate` — see "Nick casing" below.
Passwords are hashed with argon2id; the plaintext password is never stored
or logged.

### `POST /login`

Request: `{"nick": "<nick>", "password": "<password>"}`

| Status | Body | When |
|--------|------|------|
| 200 | `{"token": "<base64url, 43 chars>", "expires": <unix seconds>, "refresh": "<base64url, 43 chars>"}` | Correct password |
| 401 | `{"error":"invalid_credentials"}` | Wrong password OR unknown nick — the two cases are deliberately indistinguishable in status, body, and timing (argon2 always runs, against a fixed dummy hash for an unknown nick) |
| 400 | `{"error":"bad_json"}` | Malformed/incomplete body |
| 429 | `{"error":"rate_limited"}` | More than 10 *failed* login attempts from this peer address in the last hour — successful logins never count |

Tokens are 32 CSPRNG bytes, base64url-encoded (no padding), TTL 12 hours,
and are themselves stored argon2id-hashed — the raw value exists only in
this response and the caller's memory. A token is single-use: consumed on
its first successful `/validate` call, not on issuance.

`refresh` (D-17/AUTH-03, Phase 4) is a separate 30-day random token, also
argon2id-hashed at rest in its own `refresh_tokens` table. The launcher
stores only this value, in the OS credential store — never the password.
See `POST /refresh` below for how it is spent and rotated.

### `POST /refresh`

Request: `{"nick": "<nick>", "refresh": "<refresh token from /login or a prior /refresh>"}`

| Status | Body | When |
|--------|------|------|
| 200 | `{"token": "<base64url, 43 chars>", "expires": <unix seconds>, "refresh": "<new base64url, 43 chars>"}` | The presented refresh token was live |
| 401 | `{"error":"invalid_token"}` | Unknown nick, no matching unexpired/unrevoked token, or the presented token was already used — no distinction between the cases |
| 400 | `{"error":"bad_json"}` | Malformed/incomplete body |
| 429 | `{"error":"rate_limited"}` | More than 60 calls from this peer address in the last hour — a circuit breaker, not a brute-force control (a refresh token has no guessable surface) |

Every successful call **rotates**: the presented refresh token is revoked
in the same compare-and-swap that accepts it, and a brand-new 30-day
refresh token is issued alongside a brand-new 12-hour game token.
Rotation is unconditional — not just on age — which caps a stolen refresh
token at exactly one unrotated use. `campfire-auth reset <nick>` revokes
every outstanding refresh token for that nick, so a password reset ends
remembered sessions too, not just future logins.

### `POST /validate`

Request: `{"nick": "<nick>", "token": "<token from /login>"}`

| Status | Body | When |
|--------|------|------|
| 200 | `{"nick": "<original registration casing>"}` | Token belongs to this nick, is unexpired, and had not already been consumed |
| 401 | `{"error":"invalid_token"}` | Unknown nick, wrong nick for this token, expired, never-issued, or already consumed |
| 400 | `{"error":"bad_json"}` | Malformed/incomplete body |

**Never rate limited** — this is the join path and its only caller is the
game server, over loopback; throttling it would throttle joins.

Consumption is atomic: a single `UPDATE tokens SET consumed_at = ? WHERE id
= ? AND consumed_at IS NULL` compare-and-swap, not a select-then-update, so
a token cannot be validated twice even under concurrent calls.

### `GET /status`

No request body. Never rate limited.

| Status | Body |
|--------|------|
| 200 | `{"online": <bool>, "players": <number\|null>, "max": <number\|null>, "motd": <string\|null>}` |

Performs a real Minecraft Server List Ping (protocol 340, hand-rolled — no
crate, see `src/slp.rs`) against `SLP_ADDR` (default `127.0.0.1:25565`),
cached for 10 seconds so a burst of launcher polls doesn't re-ping on every
request. When the server is reachable: `online: true`, `players` and `max`
are the real counts, `motd` is the message of the day (always a plain
string — the raw ping response can send this as `{"text": "..."}`, which
this handler unwraps). When the server is unreachable, the ping times out
(5s), or the response fails to parse: `online: false` with `players`,
`max`, and `motd` all `null` — **always HTTP 200**, never a 5xx, because
"the game is off" is a normal answer this endpoint must be able to give.
The raw Server List Ping response also carries a Forge mod list (~7.2kB on
this server); this handler discards it entirely and returns only the four
fields above.

## Operator CLI

Run as the `asphacean` user against the same `AUTH_DB` the service uses
(`AUTH_DB=... campfire-auth <subcommand>`, or via `EnvironmentFile=` when
invoked through the installed unit's environment):

- `campfire-auth login <nick>` — mints a token for `<nick>` through the same
  issuance path `/login` uses, and prints **only** the token (nothing else),
  so the output pastes straight into a JVM `-Dcampfire.token=` flag. It asks
  for no password: this can only ever run for someone who can already open
  the mode-600 database file, which is strictly more privilege than knowing
  the account's password, so a password prompt here would be theatre, not a
  security control. Exits non-zero for an unknown nick.
- `campfire-auth reset <nick>` — reads a new password from stdin, applies
  the same 8-character minimum as registration, and replaces the stored
  hash. Also revokes every outstanding refresh token for that nick
  (D-17) — a reset ends remembered launcher sessions, not just future
  logins. Exits non-zero for an unknown nick or a too-short password.

## Environment variables

| Variable | Default | Meaning |
|----------|---------|---------|
| `AUTH_BIND` | `127.0.0.1:8081` | Listen address. The binary refuses to start if this is not a loopback address (D-16) — a config typo cannot expose this service. |
| `AUTH_DB` | *(required, no default)* | Path to the SQLite accounts database. Directory `auth/` is mode 700; the database file is mode 600 (asserted, not assumed from SQLite's own default) and gitignored. |
| `SLP_ADDR` | `127.0.0.1:25565` | Server List Ping target for `GET /status` (D-11). Not a listener — no bind guard. Overriding this to a dead port is how the offline branch is exercised without stopping the game server. |

## Running the smoke suite

```bash
cargo build --release --manifest-path auth-service/Cargo.toml
bash scripts/auth-smoke.sh
```

Starts an ephemeral instance on `127.0.0.1:8099` against a fresh temp
database, runs the full assertion suite (happy path, every rejection, the
registration rate limit, the operator CLI, and at-rest secrecy checks
against the temp database's own contents), and tears everything down —
repeatable an unlimited number of times, never touches the production
database.

## Constraints for Phase 3 and Phase 4

1. **`/validate` is for the game server on loopback and must never be
   published through the reverse proxy.** It has no rate limit by design
   (joins must never be throttled) and no authentication of its own beyond
   the token itself — it is safe only because its only caller today is the
   game server on the same host. `caddy/Caddyfile` (Phase 3) has no route
   for this path at all, and its terminal handler answers a deterministic
   404 for it and everything else unrouted.
2. **The per-IP rate limiter now sees through Caddy (Phase 3, done).**
   `caddy/Caddyfile` **sets** (not appends) `X-Forwarded-For` on the two
   proxied `/api` routes to its own view of the immediate client address —
   so a value the client itself supplied in that header is discarded at the
   edge, never forwarded. `register`/`login` resolve the rate-limiting
   address via `client_ip()` in `src/api.rs`: when the direct TCP peer is
   loopback (true for every request Caddy forwards) and a forwarded-for
   header is present, the *last* comma-separated element is used (correct
   whether the edge sets or appends); otherwise the direct peer is used.
   `/validate` deliberately never calls this helper — it is never rate
   limited and never proxied.
3. **`/validate`'s `nick` field is the exact registration casing, not the
   lowercased uniqueness key.** A client must connect to the game server
   with that exact casing — Minecraft's offline-mode UUID is derived from
   the exact username byte string
   (`UUID.nameUUIDFromBytes("OfflinePlayer:" + nick)`), so a differently-cased
   connection computes a *different* UUID and the player silently loses
   their inventory and progress. The launcher must always pass through the
   casing `/validate` returns, never a player-retyped variant.

## Public route table

Base URL: `https://mc.campfire.pub:8444`. Trust anchor: `ca/campfire-ca.pem`
(own private CA — the launcher must pin this, never fall back to the system
trust store). Full detail, including the write guard and the 404 terminal
handler, is in `caddy/Caddyfile` and `.planning/phases/03-modpack-distribution/03-01-PLAN.md`.

| Public route | Method | Reaches (internal) | Notes |
|---|---|---|---|
| `/manifest.json` | GET, HEAD | `file_server` at `~/rlcraft/pack/manifest.json` | The pack contract |
| `/pack/<url>` | GET, HEAD | `file_server` at `~/rlcraft/pack/<url>` | `<url>` is a manifest `files[].url` value verbatim |
| `/api/register` | POST | `127.0.0.1:8081/register` | `/api` prefix stripped by Caddy; this service's own route table is unchanged |
| `/api/login` | POST | `127.0.0.1:8081/login` | `/api` prefix stripped by Caddy; this service's own route table is unchanged |
| `/api/refresh` | POST | `127.0.0.1:8081/refresh` | `/api` prefix stripped by Caddy; see `POST /refresh` above |
| `/status` | GET | `127.0.0.1:8081/status` | See `GET /status` above |
| `/launcher/<file>` | GET, HEAD | `file_server` at `~/rlcraft/launcher-dist/<file>` (Phase 4) | The self-update feed's static tree, rooted outside `PACK_DIR` so the pack manifest generator never walks it |
| anything else | any | — | 404 from the terminal handler |
| non-GET/HEAD on `/manifest.json`, `/pack/*` or `/launcher/*` | — | — | 405 |

**`/validate` has no public route and must never get one.** There is no
wildcard under `/api` in `caddy/Caddyfile` — only these three exact paths
are routed. Adding a prefix wildcard would republish the token-validation
endpoint, which is unauthenticated beyond the token itself and has no rate
limit by design.
