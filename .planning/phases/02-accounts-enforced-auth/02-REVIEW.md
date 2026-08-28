---
phase: 02-accounts-enforced-auth
reviewed: 2026-08-28T00:00:00Z
depth: standard
files_reviewed: 27
files_reviewed_list:
  - auth-service/Cargo.toml
  - auth-service/src/main.rs
  - auth-service/src/db.rs
  - auth-service/src/auth.rs
  - auth-service/src/api.rs
  - auth-service/src/ratelimit.rs
  - auth-service/README.md
  - systemd/campfire-auth.service
  - scripts/auth-smoke.sh
  - scripts/backup.sh
  - scripts/restore.sh
  - scripts/devserver.sh
  - scripts/join-probe.py
  - mods-src/campfire-auth/build.gradle
  - mods-src/campfire-auth/build.sh
  - mods-src/campfire-auth/gradle.properties
  - mods-src/campfire-auth/settings.gradle
  - mods-src/campfire-auth/src/main/java/pub/campfire/auth/CampfireAuth.java
  - mods-src/campfire-auth/src/main/java/pub/campfire/auth/client/ClientAuthHandler.java
  - mods-src/campfire-auth/src/main/java/pub/campfire/auth/network/AuthRequestMessage.java
  - mods-src/campfire-auth/src/main/java/pub/campfire/auth/network/AuthResponseMessage.java
  - mods-src/campfire-auth/src/main/java/pub/campfire/auth/network/NetworkHandler.java
  - mods-src/campfire-auth/src/main/java/pub/campfire/auth/server/ServerAuthHandler.java
  - mods-src/campfire-auth/src/main/resources/mcmod.info
  - docs/AUTH-OPS.md
  - docs/CLIENT-SETUP.md
  - server.env.example
  - .gitignore
findings:
  critical: 0
  warning: 5
  info: 3
  total: 8
status: issues_found
---

# Phase 02: Code Review Report

**Reviewed:** 2026-08-28T00:00:00Z
**Depth:** standard
**Files Reviewed:** 27
**Status:** issues_found

## Summary

Reviewed the Rust `campfire-auth` service (SQLite via `rusqlite`, argon2id,
axum handlers) and the Forge 1.12.2 `campfire-auth` mod (freeze-on-join,
loopback `/validate` handshake), plus the operational scripts and systemd
unit that surround them.

The core security invariant the task asked me to stress hardest —
`ServerAuthHandler`'s fail-closed guarantee (a player is only ever released
by an explicit HTTP 200, `valid` starts `false` and every exception/timeout
path leaves it `false`) — holds up under trace-through. I could not
construct a path that unfreezes or lets a player act without a genuine
single-use-token-consuming 200 from `/validate`: exceptions, connect/read
timeouts, DNS/IO errors, a null server instance during shutdown, and a
disconnect mid-flight are all handled by falling through to "do nothing" or
"kick," never "allow." SQL is fully parameterized (verified: every `rusqlite`
call in `db.rs` uses `params![...]`, no string-built query anywhere in
scope). Token consumption is a real compare-and-swap (`consumed_at IS NULL`
in the `UPDATE ... WHERE` clause), not select-then-update. The
unknown-nick/wrong-password timing equalization (`dummy_hash()`) is
implemented correctly. No secrets are present in any tracked file
(`server.env.example` ships only placeholders; the real `server.env` is
gitignored). `cargo build --release`, `cargo test`, `bash -n` on all four
shell scripts, and `python3 -m py_compile` on `join-probe.py` all pass
clean; the full `auth-smoke.sh` suite (28 assertions against an ephemeral
instance) passes. `cargo clippy` is not installed in this environment and
could not be run.

What I did find are real, provable gaps below the level of an outright
bypass: a genuine TOCTOU race in the login-path rate limiter, an unbounded
resource-exhaustion surface in the mod's response handling that can also
cause spurious kicks of legitimately-authenticated players under a race, a
file-permission TOCTOU window on the SQLite database, and a rate-limiting
gap on successful logins that lets the tokens table grow without bound.
None of these let an attacker authenticate without a valid token or bypass
the freeze — they are robustness/DoS/hardening gaps, not authentication
bypasses. All are classified Warning, not Critical, for that reason; they
should still be fixed before this ships to more than a handful of trusted
friends.

## Warnings

### WR-01: Rate limiter has a check-then-record race on the `/login` path

**File:** `auth-service/src/ratelimit.rs:50-65`, used from `auth-service/src/api.rs:194,219`

**Issue:** `/register`'s `RateLimiter::check()` is atomic — it tests and
records the hit under a single mutex acquisition. `/login`, however, is
deliberately split into `would_allow()` (peek, no record) and
`record_failure()` (record, called later after the password check) so that
successful logins never count against the limiter. That split reintroduces
the race `check()` was designed to avoid: under axum's multi-threaded
runtime, N concurrent failed-login requests from the same peer address can
all call `would_allow()` and all observe "under limit" before any of them
calls `record_failure()`, because the two calls take the mutex
independently rather than as one critical section. A burst of concurrent
requests can therefore push the failure count past `LOGIN_FAIL_LIMIT` (10)
before the limiter reflects it — the 429 only catches up on the *next*
request after the burst settles. At real attacker concurrency (a
script firing dozens of parallel connections) this meaningfully weakens the
brute-force throttle the limiter exists to provide.

**Fix:** Reserve the slot before doing the password check, and release it
back if the login succeeds (or simply record a "provisional" hit and remove
it on success), so the check-and-increment stays a single critical section
like `/register`'s does:
```rust
// reserve a slot atomically, refund on success
if !state.login_limiter.check(peer.ip()) {
    return Err(ApiError::RateLimited);
}
// ... verify password ...
if ok {
    state.login_limiter.refund(peer.ip()); // pop the reservation back off
}
```

### WR-02: No dedup/guard on repeated `AuthResponseMessage` packets while a join is pending

**File:** `mods-src/campfire-auth/src/main/java/pub/campfire/auth/server/ServerAuthHandler.java:129-181`

**Issue:** `onResponseReceived` only checks that `PENDING.get(uuid)` is
non-null before calling `validateAsync`; it never marks the pending entry
as "already validating." The server registers `AuthResponseMessage.Handler`
for `Side.SERVER` unconditionally (`NetworkHandler.java:21`) — any packet a
connected client sends on the `campfireauth` channel is dispatched to it,
regardless of whether the server's own single `AuthRequestMessage` round
trip has already been answered. A modified/malicious client (not the normal
mod, which only replies once, synchronously) can send an unbounded number
of `AuthResponseMessage` packets during the up-to-5-second pending window.
Each one:
- spawns a brand-new, unpooled `Thread` (`validateAsync`, line 151) that
  opens a fresh HTTP connection to `/validate`, and
- causes the auth service to run an argon2 verification loop over every
  outstanding token candidate for that nick (`db.rs candidate_tokens` +
  `auth::verify_secret` per row) — `/validate` is deliberately *never* rate
  limited (by design, since the game server is meant to be its only
  caller), so this path has no throttle at all.

Beyond the CPU/thread cost, this also creates a genuine correctness race:
if two responses are in flight and the *later*-arriving one resolves
*first* with `valid=false` (e.g. a stale/garbage token guess), it calls
`applyValidationResult(uuid, ..., false, ...)` which removes the pending
entry and kicks the player — even though the *first*, legitimately valid
response is still in flight and will find `PENDING.remove(uuid)` already
gone (`pending == null` at line 185) and silently no-op. A legitimately
authenticated player can be kicked by a race their own client didn't
intend to lose, and a malicious client can trivially manufacture this race
against itself or (more relevantly) burn CPU/threads on the auth service.

**Fix:** Mark the pending entry as "in flight" the instant the first valid
response is accepted, and drop any further `AuthResponseMessage` for that
UUID until it resolves:
```java
PendingJoin pending = PENDING.get(uuid);
if (pending == null || pending.validating) {
    return; // already resolved or already validating — ignore
}
pending.validating = true;
validateAsync(ownNick, token, uuid);
```

### WR-03: SQLite database file has a brief window at a non-restrictive mode before `chmod(0600)`

**File:** `auth-service/src/db.rs:53-94`, `systemd/campfire-auth.service`

**Issue:** `Connection::open(path)` creates the on-disk file (mode governed
by the process umask, per the file's own comment at line 76-78) *before*
the explicit `set_permissions(..., 0o600)` loop runs afterward. Between
those two points — and again for the `-wal`/`-shm` siblings, which are
created by the `CREATE TABLE` statements that run in between — the file(s)
can briefly exist at whatever the ambient umask allows (typically 644
under systemd's default `UMask=0022`). The unit file
(`systemd/campfire-auth.service`) does not set `UMask=`, so the fix in
`db.rs` is carrying 100% of the D-13 mode-600 guarantee, and is not atomic
with file creation.

**Fix:** Set a restrictive `UMask=0177` in the systemd unit so every file
this process creates is 600 from the instant of creation, making the
after-the-fact `chmod` in `db.rs` pure defense-in-depth rather than the
only thing closing the window:
```ini
# systemd/campfire-auth.service, alongside the existing hardening block
UMask=0177
```

### WR-04: Successful logins are never rate-limited and issued tokens are never pruned

**File:** `auth-service/src/api.rs:186-233`, `auth-service/src/db.rs:151-183`

**Issue:** This is a documented, deliberate design choice ("successes never
count, so normal launcher use and testing are never throttled" —
`main.rs:25-26`, `api.rs:23-25`), but it leaves a real gap: nothing bounds
how many times a caller who already knows a valid password can call
`/login`. Each call runs a full argon2id hash of the password (~tens of ms
of CPU by design) *and* a fresh argon2id hash of a newly generated token,
then inserts a new, never-expiring-until-TTL row into `tokens`. Because
`tokens` rows are only ever marked `consumed_at` (never deleted — there is
no GC/pruning path anywhere in `db.rs`), a compromised or merely
enthusiastic credential holder can grow the `tokens` table without bound
and impose sustained argon2 CPU load with no throttle at all. `/validate`'s
per-nick candidate loop (`candidate_tokens` + `verify_secret` per
candidate) also gets linearly more expensive as this table grows for a
given user.

**Fix:** At minimum, delete/expire consumed or past-TTL token rows
opportunistically (e.g. on each `/login`, `DELETE FROM tokens WHERE
consumed_at IS NOT NULL OR expires_at < ?1`), and consider counting
successful logins against a *much* looser limiter (e.g. 60/hour) purely as
a circuit breaker against runaway automation, separate from the existing
failed-attempt limiter.

### WR-05: `HttpURLConnection` response is never drained or explicitly closed

**File:** `mods-src/campfire-auth/src/main/java/pub/campfire/auth/server/ServerAuthHandler.java:150-181`

**Issue:** `validateAsync` calls `conn.getResponseCode()` and then does
nothing else with `conn` — the response body is never read via
`getInputStream()`/`getErrorStream()`, and `conn.disconnect()` is never
called. `HttpURLConnection` reuses persistent (keep-alive) connections
across calls to the same host:port by default; not fully consuming the
response body is a documented way to leave a connection in a state where
it cannot be safely returned to the reuse pool, causing the JVM to open a
fresh socket per call instead of reusing one (or, in the worst case, to
leak sockets under sustained load until GC finalizes the connection).
Every single player join makes one of these calls, so this runs constantly
in production.

**Fix:** Drain and close explicitly:
```java
int status = conn.getResponseCode();
valid = status == 200;
failureReason = valid ? null : "invalid_token";
try (java.io.InputStream is = valid ? conn.getInputStream() : conn.getErrorStream()) {
    if (is != null) { while (is.read() != -1) {} }
} finally {
    conn.disconnect();
}
```

## Info

### IN-01: No Rust unit tests — correctness relies entirely on the external smoke script

**File:** `auth-service/src/*.rs` (no `#[cfg(test)]` modules anywhere; `cargo test` reports "0 tests")

**Issue:** The security-critical logic — the token compare-and-swap in
`consume_token`, the constant-time-ish dummy-hash branch in `login`, the
case-fold uniqueness in `insert_user` — is exercised only by
`scripts/auth-smoke.sh`'s black-box HTTP assertions. That suite is good and
does cover the right behaviors, but it requires a full build + process
spin-up to run, and a regression in, say, `consume_token`'s WHERE clause
would only be caught by re-running the whole smoke suite rather than a
fast `cargo test`.

**Fix:** Add a small `#[cfg(test)]` module in `db.rs` for the CAS property
directly (open an in-memory/temp DB, insert a token, consume it twice,
assert the second call returns `false`) — cheap to write, much faster
feedback loop than the shell suite for this one property.

### IN-02: `valid_password` has no upper bound

**File:** `auth-service/src/api.rs:44-46`

**Issue:** `valid_password` only enforces `>= 8` characters, with no
maximum. This is soft-mitigated by axum's implicit default 2MB body limit
on JSON-extracted requests (confirmed by reading `axum-core`'s
`DefaultBodyLimit` docs directly — no explicit limit is configured in
`main.rs`, so the built-in default applies), but a 2MB password is still a
meaningfully larger argon2 input than intended, and the limit is implicit
rather than something this codebase asserts.

**Fix:** Add an explicit upper bound alongside the existing lower bound
(e.g. `(8..=128).contains(&password.chars().count())`) so the constraint
is visible in this file rather than inherited from an unconfigured axum
default elsewhere.

### IN-03: `cargo clippy` unavailable in this environment

**Issue:** The clippy component is not installed on this machine
(`error: no such command: 'clippy'`), so no lint pass could be run as part
of this review beyond manual reading.

**Fix:** Not a code defect — flagging so a clippy pass gets run in CI or by
whoever has the component installed before this ships.

---

_Reviewed: 2026-08-28T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
