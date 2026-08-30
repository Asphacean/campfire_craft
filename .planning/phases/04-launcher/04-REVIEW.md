---
phase: 04-launcher
reviewed: 2026-08-30T15:06:18Z
depth: standard
files_reviewed: 33
files_reviewed_list:
  - launcher/core/src/auth.rs
  - launcher/core/src/http.rs
  - launcher/core/src/manifest.rs
  - launcher/core/src/mojang.rs
  - launcher/core/src/forge.rs
  - launcher/core/src/java.rs
  - launcher/core/src/launch.rs
  - launcher/core/src/lib.rs
  - launcher/core/src/log.rs
  - launcher/core/src/paths.rs
  - launcher/core/src/play.rs
  - launcher/core/src/progress.rs
  - launcher/core/src/status.rs
  - launcher/core/src/strings.rs
  - launcher/core/src/system.rs
  - launcher/core/src/update.rs
  - launcher/core/src/bin/campfire-cli.rs
  - launcher/core/tests/launch_command.rs
  - launcher/core/tests/manifest_guard.rs
  - launcher/core/Cargo.toml
  - launcher/Cargo.toml
  - launcher/rust-toolchain.toml
  - launcher/src-tauri/src/lib.rs
  - launcher/src-tauri/src/main.rs
  - launcher/src-tauri/tauri.conf.json
  - launcher/src-tauri/capabilities/default.json
  - launcher/src-tauri/Cargo.toml
  - launcher/src-tauri/build.rs
  - launcher/ui/index.html
  - launcher/ui/main.js
  - launcher/ui/style.css
  - auth-service/src/api.rs
  - auth-service/src/db.rs
  - auth-service/src/auth.rs
  - auth-service/src/ratelimit.rs
  - auth-service/src/main.rs
  - auth-service/src/slp.rs
  - auth-service/README.md
  - caddy/Caddyfile
  - scripts/publish-launcher.sh
  - scripts/auth-smoke.sh
  - docs/LAUNCHER-BUILD.md
  - docs/DIST-OPS.md
  - server.env.example
  - .gitignore
findings:
  critical: 0
  warning: 5
  info: 3
  total: 8
status: issues_found
---

# Phase 04-launcher: Code Review Report

**Reviewed:** 2026-08-30T15:06:18Z
**Depth:** standard
**Files Reviewed:** 33 (core + campfire-cli + tests + Cargo/toolchain metadata + Tauri shell + UI + auth-service + Caddy + scripts + docs/env)
**Status:** issues_found

## Summary

This phase (Tauri 2 launcher + auth-service refresh-token additions) is unusually
well defended for the threat model it names: `cargo test --workspace` (56 tests)
and `cargo clippy --workspace --all-targets` both pass clean; `caddy validate`
accepts `caddy/Caddyfile`; both shell scripts pass `bash -n`. TLS pinning is
correctly split into two named client constructors (`campfire_client`/
`public_client`) so the two trust domains cannot be mixed up; the manifest path
guard, download hash verification, refresh-token rotation (CAS), rate limiting,
and secret redaction in logs were all traced end-to-end and hold up under
adversarial reading — including the specific things a shallower review would
have missed (SQL parameter binding throughout `db.rs`, timing-safe dummy-hash
comparison on `/login`, `-Dcampfire.token=` on the process command line being a
knowingly-accepted, already-documented tradeoff per `04-03-PLAN.md`'s threat
register T-04-03-06, not a fresh finding).

No BLOCKER-tier defects were found. The warnings below are real gaps — a
disabled CSP, a case-sensitivity hole in the "never touch player state" guard,
a non-atomic partial-write path, a logout that doesn't revoke server-side, and
an unclamped NaN edge case in the RAM argument — but none of them are exploitable
without either an already-compromised distribution host or an already-compromised
frontend/local machine, which are both bigger problems than the finding itself.

## Warnings

### WR-01: CSP is explicitly disabled in the shipped app

**File:** `launcher/src-tauri/tauri.conf.json:20-22`
**Issue:** `"security": { "csp": null }` disables Tauri's Content-Security-Policy
injection entirely for the production build. Today's `main.js` never uses
`innerHTML`/`dangerouslySetInnerHTML`/`eval` (verified — every render path uses
`.textContent`), so there is no live XSS vector this enables right now. But a
CSP is the standard defense-in-depth layer for exactly the kind of content this
app renders from a network response (`/status`'s `motd`, error/update-feed
text) and for any future dependency or contributor who reaches for
`innerHTML` without noticing the precedent. Tauri's own docs treat `csp: null`
as an explicit opt-out, not a default worth leaving in place for a shipped
binary.
**Fix:** Set an explicit restrictive policy, e.g.
```json
"security": {
  "csp": "default-src 'self'; img-src 'self' asset: https://asset.localhost; style-src 'self' 'unsafe-inline'"
}
```
tuned to whatever `index.html`/`style.css` actually need (no remote origins are
loaded today, so `default-src 'self'` plus the asset: scheme for local images
should suffice — verify against a real `cargo tauri dev` run before locking it in).

### WR-02: The manifest "never touch player state" guard is case-sensitive on case-insensitive filesystems

**File:** `launcher/core/src/manifest.rs:36-37, 233-241`
**Issue:** `NEVER_TOUCH_TOP_LEVEL_FILES`/`NEVER_TOUCH_DIRS` are compared with
plain `==`/`.contains()` string equality in `assert_never_touch`. The two
shipped targets (Windows/NTFS, macOS/APFS) both default to case-insensitive
filesystems, so a manifest entry naming `"OPTIONS.TXT"`, `"Options.txt"`, or
`"Saves/x"` resolves to the *same on-disk file or directory* as the protected
`"options.txt"`/`"saves/"` path, yet fails every check in
`assert_never_touch` (which does a case-sensitive comparison) and would be
downloaded/deleted right over the seeded/player file. This is exactly the
"contract slips" scenario the second lock's own doc comment says it exists
for (`manifest.rs:34-35`), and it isn't caught: `validate()` doesn't check
these lists at all (only `FORBIDDEN_PREFIXES`/the vanilla-jar-name check run
there), so nothing rejects the manifest up front either.
**Fix:** Compare case-insensitively for this specific guard (these are
platform-reserved literal names, not user content, so ASCII-lowercasing both
sides before comparing is sufficient and doesn't need a full Unicode
case-fold):
```rust
fn eq_ignore_platform_case(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}
// NEVER_TOUCH_TOP_LEVEL_FILES.iter().any(|p| eq_ignore_platform_case(p, rel_path))
// NEVER_TOUCH_DIRS.iter().any(|p| eq_ignore_platform_case(p, top))
```
Consider also folding this same check into `validate()` so a manifest
containing a case-varied protected path is rejected whole, before any
download begins, consistent with the rest of that function.

### WR-03: A never-touch violation mid-sync can still leave sibling downloads written to disk

**File:** `launcher/core/src/manifest.rs:494-597` (`sync()`), `283-364`
(`download_one`)
**Issue:** `sync()` queues every changed `files[]` entry into one
`futures_util::stream::iter(...).buffer_unordered(DOWNLOAD_CONCURRENCY)` and
`.collect()`s *all* results before inspecting them for an error. If one entry
happens to name a protected path (see WR-02 for how that can happen despite
`validate()` already having run), `assert_never_touch` correctly refuses to
write *that* file — but the other up to `DOWNLOAD_CONCURRENCY - 1` downloads
already in flight in the same batch, and every entry queued after it, still
complete and get written to disk before the stream finishes draining and
`sync()` returns `Err`. The module's own framing ("reject the whole manifest,
the sync never begins") and the existing test name
(`a_hostile_manifest_among_189_good_entries_is_rejected_before_any_file_would_be_written`)
only hold for the `validate()`-time checks (absolute path, `..`, control
chars, forbidden prefixes) — there is no equivalent test proving the
never-touch guard (checked lazily, per-file, at write time) leaves the game
directory untouched when it's the thing that fires, and by construction it
cannot: the specific protected file is never written, but its batch-mates
already were.
**Fix:** Either (a) pre-check every `files[]`/`delete[]` entry against
`NEVER_TOUCH_*` in `validate()` (matching WR-02's suggested fix, and making
the "whole manifest rejected, nothing written" property actually hold for
this guard too), or (b) if the lazy per-file check is intentionally kept as
the only enforcement point, document in `sync()`'s own doc comment that a
never-touch violation partway through a batch can still leave sibling
downloads on disk, so a future reader doesn't assume the same all-or-nothing
guarantee the `validate()` checks provide.

### WR-04: "Log out" never revokes the refresh token server-side

**File:** `launcher/core/src/auth.rs:158-163` (`logout`),
`launcher/src-tauri/src/lib.rs:96-99` (`logout` command),
`launcher/ui/main.js:255-260`
**Issue:** `auth::logout` is documented as "Clears local state only — no
network call. There is nothing server-side to revoke synchronously; the
refresh token is simply forgotten locally." That's accurate as written, but
it means pressing "Log out" in the UI does not end the session anywhere
except this one machine's credential store: the 30-day refresh token the
server issued is still live and will happily mint fresh 12-hour game tokens
for anyone who still has a copy of it (a prior keyring dump, a stolen backup,
a compromised second device that was logged in under the same nick) until it
naturally expires or the *password* is reset via `campfire-auth reset`
(which does revoke it, per `db.rs`'s `revoke_all_refresh_for_user`). A user
who logs out expecting that to end their session everywhere gets a false
sense of security.
**Fix:** `auth-service` already has the exact primitive needed
(`revoke_refresh`/CAS on `revoked_at IS NULL`) — expose a `POST /api/logout
{nick, refresh}` (or reuse `/api/refresh` with a "don't reissue" flag) that
revokes the presented token without minting a new one, and have
`auth::logout` call it (best-effort, still clearing local state
unconditionally) before deleting the keyring entry.

### WR-05: `ram: f32` is not defended against `NaN`, producing a silent `-Xms0M -Xmx0M`

**File:** `launcher/src-tauri/src/lib.rs:130-132` (`ram.clamp(3.0, 10.0)`),
`launcher/core/src/launch.rs:283-291` (`ram_mb` computation),
`launcher/core/src/bin/campfire-cli.rs:405-407, 496-498` (`--ram` parsing)
**Issue:** `f32::clamp` is documented to panic only if `min`/`max` are NaN or
`min > max` — it does *not* sanitize a NaN `self`; `f32::NAN.clamp(3.0, 10.0)`
returns `NaN` unchanged. `build_launch_command`'s
`(ram_gb * 1024.0).round() as u64` then saturates `NaN as u64` to `0`,
silently producing `-Xms0M -Xmx0M`. The Tauri `play` command can only receive
this from a compromised/rogue frontend (the real `<input type=range>` can't
produce it), but `campfire-cli play`/`launch-cmd --ram` apply *no* clamp at
all and `"nan".parse::<f32>()` succeeds, so `--ram nan` reaches
`build_launch_command` unmodified today. The resulting JVM failure
("Invalid initial heap size" or similar) isn't one of `PlayError`'s mapped
stable codes, so it would surface as an unhelpful generic failure rather than
a clear one.
**Fix:** Guard at the shared boundary rather than each caller:
```rust
let ram_gb = if ram_gb.is_finite() { ram_gb.clamp(3.0, 10.0) } else { 3.0 };
```
placed in `build_launch_command` itself (or a small `sanitize_ram` helper
both `play`/`campfire-cli` funnel through) closes this for every caller at
once rather than patching the Tauri command and the CLI separately.

## Info

### IN-01: `/status`'s `motd` field is fetched but never rendered

**File:** `launcher/ui/main.js:180-195` (`pollStatus`),
`auth-service/src/api.rs:194-235`, `launcher/core/src/status.rs:12-18`
**Issue:** `ServerStatus`/`StatusResponse` both carry a `motd: Option<String>`
that the SLP ping populates end-to-end, but `pollStatus()` only ever reads
`status.online`/`status.players`/`status.max` — the MOTD is fetched over the
wire on every 15-second poll and then discarded. Not a defect, just dead
data/an incomplete feature; worth a one-line note either in code or the
backlog so a future reader doesn't wonder if it's supposed to be wired up.
**Fix:** Either surface it somewhere in the UI (a tooltip on the status pill
would be a small addition) or drop the field from the wire contract if it's
staying unused, whichever the actual roadmap intends.

### IN-02: `mapErrorCode`'s default branch is currently unreachable

**File:** `launcher/ui/main.js:85-105`
**Issue:** The `switch (code)` covers every variant `map_auth_error` (Rust
side) can actually produce, so the `default: return STRINGS.errorServerUnreachable`
branch can never be hit today. Harmless, but if a new `AuthError` variant is
ever added on the Rust side without updating this switch in lockstep, the new
code would silently show "Can't reach campfire.pub" regardless of what
actually went wrong (matches `PlayError`'s `mapPlayErrorCode`, which by
contrast has a real generic fallback by design). Low-value to fix now; worth
a comment noting the two switches must stay in sync with their Rust
counterparts.

### IN-03: Redundant `header_up X-Forwarded-For` flagged by Caddy itself

**File:** `caddy/Caddyfile:58-82`
**Issue:** `caddy validate` prints three
`"Unnecessary header_up X-Forwarded-For: the reverse proxy's default
behavior is to pass headers to the upstream"` warnings for the three proxied
`/api/*` handles. Not a bug — the explicit `header_up ... {http.request.remote.host}`
is deliberately kept (per the Caddyfile's own comment) to *guarantee* the SET
(not append) semantics the rate limiter's anti-spoofing property depends on,
which is a defensible reason to keep it despite the linter noise — but the
warning is worth a one-line acknowledgment in the comment block so a future
maintainer doesn't "clean it up" per Caddy's own advice and reopen the
spoofing gap the comment above it explains.

---

_Reviewed: 2026-08-30T15:06:18Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
