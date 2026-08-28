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
| 200 | `{"token": "<base64url, 43 chars>", "expires": <unix seconds>}` | Correct password |
| 401 | `{"error":"invalid_credentials"}` | Wrong password OR unknown nick — the two cases are deliberately indistinguishable in status, body, and timing (argon2 always runs, against a fixed dummy hash for an unknown nick) |
| 400 | `{"error":"bad_json"}` | Malformed/incomplete body |
| 429 | `{"error":"rate_limited"}` | More than 10 *failed* login attempts from this peer address in the last hour — successful logins never count |

Tokens are 32 CSPRNG bytes, base64url-encoded (no padding), TTL 12 hours,
and are themselves stored argon2id-hashed — the raw value exists only in
this response and the caller's memory. A token is single-use: consumed on
its first successful `/validate` call, not on issuance.

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

No request body.

| Status | Body |
|--------|------|
| 200 | `{"online": true, "players": null}` |

Fixed placeholder for this phase — no RCON call. A real player count is
added only once Phase 3/4 actually need one.

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
  hash. Exits non-zero for an unknown nick or a too-short password.

## Environment variables

| Variable | Default | Meaning |
|----------|---------|---------|
| `AUTH_BIND` | `127.0.0.1:8081` | Listen address. The binary refuses to start if this is not a loopback address (D-16) — a config typo cannot expose this service. |
| `AUTH_DB` | *(required, no default)* | Path to the SQLite accounts database. Directory `auth/` is mode 700; the database file is mode 600 (asserted, not assumed from SQLite's own default) and gitignored. |

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
   game server on the same host.
2. **The per-IP rate limiter sees the direct TCP peer address.** Once Phase
   3 puts Caddy in front of this service, every `/register`/`/login`
   request will arrive from Caddy's own address (127.0.0.1), collapsing
   every real client into one rate-limit bucket. Phase 3 must either keep
   `/register` and `/login` reachable directly (bypassing Caddy) or teach
   this limiter to read a forwarded-for header from a trusted proxy only.
3. **`/validate`'s `nick` field is the exact registration casing, not the
   lowercased uniqueness key.** A client must connect to the game server
   with that exact casing — Minecraft's offline-mode UUID is derived from
   the exact username byte string
   (`UUID.nameUUIDFromBytes("OfflinePlayer:" + nick)`), so a differently-cased
   connection computes a *different* UUID and the player silently loses
   their inventory and progress. The launcher must always pass through the
   casing `/validate` returns, never a player-retyped variant.
