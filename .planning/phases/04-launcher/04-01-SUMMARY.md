---
phase: 04-launcher
plan: 01
subsystem: auth
tags: [tauri, rust, rustup, reqwest, keyring, argon2, axum, caddy, refresh-token]

# Dependency graph
requires:
  - phase: 02-accounts-enforced-auth
    provides: campfire-auth service (register/login/validate, argon2id hashing, rate limiting)
  - phase: 03-modpack-distribution
    provides: Caddy HTTPS front on mc.campfire.pub:8444, own-CA pinning, manifest/pack routes
provides:
  - A rustup-scoped toolchain (1.98.0) that compiles Tauri 2 on this Pi, apt's 1.85.0 untouched
  - launcher/ Cargo workspace (campfire-launcher-core + campfire-launcher) with a no-npm static frontendDist
  - POST /api/refresh (AUTH-03): 30-day rotating, argon2id-hashed refresh tokens, live on the public front
  - refresh_tokens table + campfire-auth reset revoking all outstanding refresh tokens for a nick
  - /launcher/* static route on Caddy, rooted outside PACK_DIR, for wave 4's self-update feed
  - campfire-launcher-core: http (pinned + public clients), paths (D-13 layout), auth (session +
    keyring), status, log (redacting), strings (centralized copy), campfire-cli headless harness
  - A real single-window Tauri app: auth form, collapsed logged-in line, live status pill
affects: [04-02, 04-03, 04-04, 05-release]

actuals:
  tokens: 55714
  tasks: 3
  commits: 3

tech-stack:
  added:
    - rustup 1.98.0 (stable, scoped to launcher/ via rust-toolchain.toml)
    - tauri 2.11.5 / tauri-cli 2.11.4 (crates.io, no npm)
    - reqwest 0.13.1 (rustls + webpki-roots, no native-tls)
    - keyring 3.6.3 (linux-native/apple-native/windows-native, no secret-service)
    - directories 6.0.0
  patterns:
    - "Two named HTTP client constructors (campfire_client/public_client) instead of one
      configurable client, so pinned-vs-public can never be mixed up at a call site"
    - "CAMPFIRE_HOME env override resolved once in paths.rs, so every headless test runs
      against a scratch install root instead of a real profile"
    - "log::redact() at every call site that might carry a secret, rather than a global
      log-scrubbing filter — the secret never reaches the formatted line at all"
    - "refresh_tokens mirrors the existing tokens table's shape and compare-and-swap
      consume/revoke pattern exactly, rather than inventing a new one"

key-files:
  created:
    - launcher/rust-toolchain.toml
    - launcher/Cargo.toml
    - launcher/core/Cargo.toml
    - launcher/core/src/http.rs
    - launcher/core/src/paths.rs
    - launcher/core/src/auth.rs
    - launcher/core/src/status.rs
    - launcher/core/src/log.rs
    - launcher/core/src/strings.rs
    - launcher/core/src/progress.rs
    - launcher/core/src/bin/campfire-cli.rs
    - launcher/core/assets/campfire-ca.pem
    - launcher/src-tauri/Cargo.toml
    - launcher/src-tauri/tauri.conf.json
    - launcher/src-tauri/src/lib.rs
    - launcher/ui/index.html
    - launcher/ui/main.js
    - launcher/ui/style.css
  modified:
    - auth-service/src/db.rs
    - auth-service/src/api.rs
    - auth-service/src/main.rs
    - auth-service/README.md
    - caddy/Caddyfile
    - docs/DIST-OPS.md
    - scripts/auth-smoke.sh
    - .gitignore

key-decisions:
  - "keyring: dropped the sync-secret-service feature RESEARCH.md's feature list implied
    alongside linux-native — enabling both makes keyring's default backend prefer
    secret-service, which has no dbus daemon on this Pi; linux-native alone selects keyutils"
  - "campfire-cli prints the game token (not the refresh token) from login/refresh, mirroring
    campfire-auth's own `login <nick>` CLI — the game token is short-lived, single-use, and
    outside the threat register's protected-secret list, so redacting it too would make the
    CLI unusable as a proof harness for no security benefit"
  - "reqwest 0.13's tls_certs_only() replaces the add_root_certificate() + tls_built_in_root_certs(false)
    combo RESEARCH.md described for older reqwest — one call does both (adds the CA AND
    disables built-in roots), confirmed by reading the 0.13.1 source directly"

patterns-established:
  - "Pattern: named HTTP client constructors over a single configurable one, wherever a
    codebase needs both a pinned and a public trust anchor"
  - "Pattern: CAMPFIRE_HOME-style env override for install-root resolution, for headless
    testability without touching a real user profile"

requirements-completed: [AUTH-03, LNCH-01, LNCH-07]

coverage:
  - id: D1
    description: "rustup toolchain (1.98.0) scoped to launcher/ via rust-toolchain.toml; apt's rustc 1.85.0 (auth-service's toolchain) untouched"
    verification:
      - kind: other
        ref: "rustc --version (apt, unchanged) vs ~/.cargo/bin/rustc --version (1.98.0) — both checked live on this Pi"
        status: pass
    human_judgment: false
  - id: D2
    description: "cargo tauri build --no-bundle produces a working desktop binary from a tree with zero npm dependencies"
    verification:
      - kind: other
        ref: "cd launcher && ~/.cargo/bin/cargo tauri build --no-bundle (exit 0, 5m42s cold / ~1m incremental); find launcher -name node_modules -o -name package*.json -> 0 hits"
        status: pass
    human_judgment: false
  - id: D3
    description: "POST /api/refresh live on the public front: mints a fresh game token, rotates the refresh token, and dead-ends a replayed refresh token with 401"
    requirement: AUTH-03
    verification:
      - kind: integration
        ref: "scripts/auth-smoke.sh (48 checks, 13 new refresh assertions)"
        status: pass
      - kind: e2e
        ref: "live round trip via curl through https://mc.campfire.pub:8444/api/login + /api/refresh, ~152ms observed latency, replay of the spent refresh token returns 401 invalid_token"
        status: pass
    human_judgment: false
  - id: D4
    description: "refresh_tokens stored only as argon2id hashes; campfire-auth reset revokes every outstanding refresh token for that nick"
    requirement: AUTH-03
    verification:
      - kind: integration
        ref: "scripts/auth-smoke.sh: 'every row of refresh_tokens.token_hash starts with $argon2id$', 'campfire-auth reset leaves an outstanding refresh token unusable'"
        status: pass
    human_judgment: false
  - id: D5
    description: "The launcher's HTTPS client trusts only the embedded CA and nothing else — proven by making pinning fail, not by making it pass"
    verification:
      - kind: e2e
        ref: "campfire-cli pin-check: succeeds against mc.campfire.pub, fails with a certificate error against api.adoptium.net using the same client"
        status: pass
    human_judgment: false
  - id: D6
    description: "A real account logs in from the launcher core against the live server; the refresh token round-trips through the OS credential store (linux keyutils) and rotates on every use"
    requirement: AUTH-03
    verification:
      - kind: e2e
        ref: "campfire-cli login <CampfireTester> (stdin password) then two consecutive campfire-cli refresh calls, both succeeding — proves the rotated value was written back, not the stale one reused"
        status: pass
      - kind: unit
        ref: "cargo test --workspace (4 tests: embedded CA match, redact, timestamp shape, CAMPFIRE_HOME override)"
        status: pass
    human_judgment: false
  - id: D7
    description: "Password and tokens never appear in launcher.log; the refresh token never appears anywhere under the install root outside the OS credential store"
    verification:
      - kind: other
        ref: "grep -cF -- \"$PASSWORD\"/\"$TOKEN\" launcher.log -> 0; grep -rF -- \"$REFRESH\" $CAMPFIRE_HOME -> 0 (values captured live via the OS keyring during the verification run)"
        status: pass
    human_judgment: false
  - id: D8
    description: "Single window: nick/password fields, Log in / Create account side by side, form collapses to 'Playing as Nick · Log out' after login, status pill polls /status every 15s without ever disabling Play"
    requirement: LNCH-01
    verification: []
    human_judgment: true
    rationale: "This is a visual/interactive WebView behavior that needs a real display; this Pi builds and compiles the binary headlessly but cannot render or click it. The plan's own verification step defers this to a Windows x64 build-from-source human-check, which has not yet been performed — see 'Pending Human Verification' below."
  - id: D9
    description: "Status pill reports online/offline/checking with player count; blank-field prompt on submit; in-flight button disable/relabel; wrong-password and unreachable-server banners with an Open log button"
    requirement: LNCH-07
    verification: []
    human_judgment: true
    rationale: "Same as D8 — implemented in launcher/ui/main.js and wired to real Tauri commands, but the actual rendered behavior (colors, banner text, button states) has only been code-reviewed, not seen on a real display. Pending the Windows human-check."

duration: 61min
completed: 2026-08-28
status: complete
---

# Phase 4 Plan 1: Toolchain, Refresh Tokens, and the Session Tracer Summary

**A rustup-scoped Tauri 2 launcher workspace builds on this Pi with zero npm dependencies; `POST /api/refresh` (30-day rotating, argon2id-hashed) is live on the public HTTPS front; and a real nick/password session round-trips through the launcher's OS-credential-store-backed core against the live server.**

## Performance

- **Duration:** ~61 min (rustup install ~19:49 through final verification ~20:50, local time)
- **Started:** 2026-08-28T16:49:00Z (approx.)
- **Completed:** 2026-08-28T17:50:00Z
- **Tasks:** 3
- **Files modified:** 48 (32 created in task 1, 8 auth-service/Caddy files in task 2, 14 launcher core/frontend files in task 3 — some files touched across tasks)

## Accomplishments

- Installed `rustup` (stable 1.98.0) scoped entirely to `launcher/` via `rust-toolchain.toml`; the apt-packaged `cargo`/`rustc` 1.85.0 that `auth-service` builds against is untouched and still builds clean.
- Hand-authored a two-member Cargo workspace (`campfire-launcher-core`, `campfire-launcher`) with a static `ui/` `frontendDist` — no Node, no bundler, no `package.json` anywhere in the tree — and `cargo tauri build --no-bundle` produces a working `campfire-launcher` binary (15MB) on this Pi.
- Added `POST /api/refresh` to the live `campfire-auth` service and Caddy front: a 30-day rotating, argon2id-hashed refresh token (mirroring the existing `tokens` table's shape exactly), a 60/hour/peer circuit breaker, and `campfire-auth reset` now revokes every outstanding refresh token for that nick. Deployed live with a full smoke-suite re-run (48 checks) and a live round trip through `https://mc.campfire.pub:8444`.
- Filled `campfire-launcher-core` with the session tracer: two named HTTP clients (pinned CA vs. public roots), D-13's path layout, register/login/refresh/logout, keyring-backed refresh-token storage, a redacting log module, and every UI-SPEC copy string centralized in `strings.rs`.
- Proved the whole tracer end to end on this Pi against the live production service, not a mock: `campfire-cli pin-check` shows the built-in root store really is disabled (fails against a public-CA host with the same client that succeeds against `mc.campfire.pub`); `keyring-selftest` round-trips through the Linux keyutils backend; a real login + two consecutive `refresh` calls both succeed, proving the rotated refresh token was written back rather than the stale one reused; the password and both tokens are absent from `launcher.log` and everywhere else on disk, grepped for their actual observed values.

## Task Commits

1. **Task 1: A toolchain that can build Tauri 2, and an empty launcher that proves it** - `12bdb77` (feat)
2. **Task 2: Refresh tokens in the auth service, and the two new public routes** - `b9b787d` (feat)
3. **Task 3: The tracer — a real form, a real token from the live server, remembered** - `87341a1` (feat)

**Plan metadata:** (this commit, docs: complete plan)

## Files Created/Modified

- `launcher/rust-toolchain.toml` - pins stable 1.98.0 + rustfmt/clippy, scoped to `launcher/`
- `launcher/Cargo.toml`, `launcher/core/Cargo.toml`, `launcher/src-tauri/Cargo.toml` - the virtual workspace
- `launcher/src-tauri/tauri.conf.json` - 480×560 non-resizable window, `withGlobalTauri`, no `beforeBuildCommand`, `createUpdaterArtifacts: true`
- `launcher/ui/{index.html,main.js,style.css}` - the whole single screen: form skeleton (task 1), auth logic + status polling (task 3), UI-SPEC's CSS tokens
- `launcher/core/src/http.rs` - `campfire_client()`/`public_client()`, with a unit test pinning the embedded CA to `ca/campfire-ca.pem`
- `launcher/core/src/paths.rs` - D-13's install layout + `CAMPFIRE_HOME` test override
- `launcher/core/src/auth.rs` - the session: register/login/refresh/logout, keyring storage
- `launcher/core/src/status.rs`, `log.rs`, `strings.rs`, `progress.rs` - status client, redacting logger, centralized copy, the shared progress-event shape for later waves
- `launcher/core/src/bin/campfire-cli.rs` - the headless proof harness (5 subcommands)
- `launcher/src-tauri/src/lib.rs` - the Tauri bridge: `get_version`, `get_strings`, `get_status`, `login`, `register`, `restore_session`, `logout`, `get_log_path`
- `auth-service/src/db.rs` - `refresh_tokens` table + insert/candidate/revoke/revoke_all/prune methods
- `auth-service/src/api.rs` - `login()` now mints a refresh token too; new `refresh()` handler
- `auth-service/src/main.rs` - `/refresh` route, `refresh_limiter`, `reset` revokes all refresh tokens
- `caddy/Caddyfile` - `/api/refresh` proxy, `/launcher/*` static route (outside `PACK_DIR`)
- `auth-service/README.md`, `docs/DIST-OPS.md` - `POST /refresh` and `/launcher/<file>` documented
- `scripts/auth-smoke.sh` - 13 new refresh assertions; fixed a pre-existing grep-option flake

## Decisions Made

- Dropped keyring's `sync-secret-service` feature (see key-decisions in frontmatter) — `linux-native` alone is what actually selects the keyutils backend on this Pi.
- `reqwest` 0.13's `tls_certs_only()` (not `add_root_certificate()` + `tls_built_in_root_certs(false)`, which RESEARCH.md's older-reqwest-era description implied) does both jobs in one call; confirmed by reading the crate source directly rather than guessing from the doc comment.
- `campfire-cli login`/`refresh` print the game token (not the refresh token) to stdout, mirroring `campfire-auth login <nick>`'s own convention — the game token is short-lived, single-use, and not a protected secret in the threat register; only the refresh token and password are fully redacted everywhere including the log.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `scripts/auth-smoke.sh`'s plaintext-leak checks broke intermittently on a `-`-leading token**
- **Found during:** Task 2, first full smoke-suite run after adding refresh-token assertions
- **Issue:** `grep -cF "$TOKEN"` (and the equivalent `$PASSWORD`/`$REFRESH1` checks) fail with a `grep: Usage:` error whenever the base64url-encoded token/refresh value happens to start with `-`, because grep reads it as an option rather than a literal pattern — a ~1-in-64 chance per run, pre-existing for `$TOKEN` and newly relevant for the added `$REFRESH1` check.
- **Fix:** Added `--` before the pattern in all three affected `grep -cF` calls.
- **Files modified:** `scripts/auth-smoke.sh`
- **Verification:** Ran the full suite 6 consecutive times after the fix — 48/48 checks passed every time.
- **Committed in:** `b9b787d` (Task 2 commit)

**2. [Rule 1 - Bug] `keyring`'s `sync-secret-service` feature silently changed the default backend**
- **Found during:** Task 3, first `campfire-cli keyring-selftest` run
- **Issue:** RESEARCH.md's feature list (`apple-native`, `windows-native`, `linux-native`, and implicitly a secret-service feature for the async/sync variants) enabled `sync-secret-service` alongside `linux-native`. `keyring` 3.x's own `#[cfg]` logic makes secret-service the *default* backend whenever both are compiled in — which has no dbus daemon on this Pi, so every keyring call failed with `DBus error: The name org.freedesktop.secrets was not provided by any .service files`.
- **Fix:** Dropped `sync-secret-service` from `launcher/core/Cargo.toml`'s `keyring` feature list, leaving `linux-native` as the only enabled Linux backend so keyutils is selected by default.
- **Files modified:** `launcher/core/Cargo.toml`
- **Verification:** `campfire-cli keyring-selftest` now prints `PASS: keyring round-trip succeeded (linux keyutils backend)`.
- **Committed in:** `87341a1` (Task 3 commit)

**3. [Rule 4 - Architectural, resolved via existing precedent] `campfire-cli` prints the game token after all**
- **Found during:** Task 3, while writing the acceptance-criteria verification for the login/refresh round trip
- **Issue:** The plan's acceptance criteria expect `campfire-cli refresh` to visibly "print a new game token" so the executor can verify rotation and secrecy against an actual observed value — my first draft fully redacted every token in CLI output, following the threat model's "never log the raw value" instinct too literally.
- **Resolution:** `auth-service`'s own `campfire-auth login <nick>` CLI already prints its minted token to stdout by design (documented: "so the output pastes straight into a JVM flag"), precisely because the game token is short-lived, single-use, and not on the threat register's protected-secret list (only the refresh token and password are). Matched that existing precedent: `campfire-cli` now prints the game token, while the refresh token and password remain fully redacted in every log line and never appear in CLI output at all.
- **Files modified:** `launcher/core/src/bin/campfire-cli.rs`
- **Verification:** `grep -cF -- "$REFRESH"` across the whole `CAMPFIRE_HOME` tree and `grep -cF -- "$TOKEN"`/`"$PASSWORD"` in `launcher.log` all print `0`.
- **Committed in:** `87341a1` (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (2 bugs, 1 architectural resolved via existing precedent)
**Impact on plan:** All three were necessary for correctness (a flaky test suite, a broken credential store) or for the acceptance criteria's own verifiability. No scope creep beyond what the plan already specified.

## Issues Encountered

None beyond the deviations above — every `<verify>` block in the plan ran to completion on the first or second attempt.

## User Setup Required

None - no external service configuration required. `campfire-cli` is a build artifact under `launcher/target/`, not something the operator installs separately.

## Known Stubs

- **"Open log" button shows an `alert()` with the log path instead of opening the file** (`launcher/ui/main.js`, `openLogBtn` click handler). Actually revealing `launcher.log` in the OS file manager needs `tauri-plugin-opener`, which this plan deliberately does not add — it's the same dependency wave 4 needs for the "Game folder" button, so both are wired together then rather than adding the plugin twice. Does not block this plan's must-haves (the log file itself is written correctly and redacts every secret; only the reveal-in-file-manager convenience is deferred).
- **RAM slider, progress bar, and the Play button stay hidden** (`launcher/ui/index.html`: `ram-block`, `progress-area`, `play-btn` all carry `hidden`). This plan's scope is explicitly "the auth half only" (D-01/D-02/D-03/D-05 and the session half of D-18) — manifest sync, Java/Forge provisioning, and launch are waves 2–4's job, and there is nothing for Play to do yet.

## Pending Human Verification

The plan's `<human-check>` block (building the launcher from source on a real Windows x64 machine and clicking through the window) has **not** been performed — this Pi has no display and the plan's own environment note treats the human-only visual checks as deferred rather than blocking. Per the checkpoint protocol for this run, this is recorded here rather than as a blocking checkpoint:

1. The window opens at ~480×560, non-resizable.
2. The status pill reads "Checking…" then "Online · 0/10" (or similar) within a couple of seconds.
3. Submitting the form with both fields empty shows "Enter a nickname and password."
4. A wrong password shows "Wrong nickname or password." with a working "Open log" button (currently an `alert()` showing the log path — full file-reveal via `tauri-plugin-opener` is deferred to wave 4 alongside "Game folder").
5. A correct login collapses the form to "Playing as **Nick** · Log out" with exact registered casing.
6. Closing and reopening the launcher restores the logged-in state with no password prompt (**AUTH-03's real proof** — already proven headlessly via `campfire-cli` on this Pi, but not yet seen through the actual window).
7. "Log out" reopens the form immediately, no confirmation dialog.
8. `%APPDATA%\campfire\launcher.log` contains no password.

Steps 6 and 8 are the ones worth double-checking first, per the plan's own emphasis — everything they depend on (the core's `restore_session`/keyring/log-redaction logic) has already passed its headless proof on this Pi using the real production service, so a failure at this stage would most likely be Tauri-bridge wiring rather than the underlying logic.

**Test account for this phase:** `CampfireTester`, registered live against `https://mc.campfire.pub:8444/api/register` during Task 2's verification. Its password was generated randomly for this session, used only in-memory/in a `chmod 600` scratch file outside the repository, and is not recorded anywhere in this SUMMARY, the git history, or any tracked file. Reuse this account (rather than registering a new one) for the next plan's testing, since `/api/register` is rate-limited to 5/hour/peer from this Pi.

## Build-From-Source Instructions (for `docs/LAUNCHER-BUILD.md`, wave 4)

```bash
# One-time toolchain setup (Windows/macOS, and already done on this Pi):
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
# launcher/rust-toolchain.toml pins the exact version (1.98.0) automatically
# from here on — every command below must use the rustup-installed cargo,
# not any OS-bundled one.
~/.cargo/bin/cargo install tauri-cli --version "^2" --locked

# Linux only (this Pi's smoke build; irrelevant to the Windows/macOS builds):
sudo apt-get install -y libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev librsvg2-dev libayatana-appindicator3-dev libxdo-dev \
  build-essential pkg-config file wget

# Every platform, from the launcher/ directory:
cd launcher
~/.cargo/bin/cargo test --workspace        # the headless core suite
~/.cargo/bin/cargo tauri build --no-bundle # a plain binary, no installer
# drop --no-bundle for a real installer once Phase 5's packaging lands
```

**Observed on this Pi (aarch64, Debian 13):**
- `cargo tauri build --no-bundle`: 5m42s cold, ~1m incremental after a small source change; final binary `launcher/target/release/campfire-launcher` is 15MB, `campfire-cli` is 6.3MB.
- `campfire-cli keyring-selftest` → `PASS: keyring round-trip succeeded (linux keyutils backend)`.
- `POST /api/refresh` round-trip latency through the public front: ~152ms.
- `campfire-auth` and `caddy` were each restarted exactly once, both within the same second (2026-08-28T17:18:16–17Z); `campfire-auth` fails closed and comes back in under a second per its own design, `caddy`'s restart briefly dropped the public HTTPS front only (no game-server impact). `rlcraft.service` was never touched — `uptime -s` (`2026-08-22 20:53:29`) was identical before and after every task in this plan.

## Next Phase Readiness

- The toolchain, workspace, build command, and headless proof harness (`campfire-cli`) are all in place — every later wave in this phase (manifest sync, Java/Forge provisioning, launch) can be developed and verified on this same Pi without hardware nobody here has.
- AUTH-03's server half is live and battle-tested against the production database; the client half (register/login/refresh/keyring) is proven against that same live service, not a mock.
- Blocker for full UAT: the Windows x64 human-check (see "Pending Human Verification" above) has not been performed. This does not block wave 2/3/4 development, which continues to be verified headlessly on this Pi via `campfire-cli` and `cargo test`, but should be completed before this phase is considered fully done.
- `launcher-dist/` (the self-update static route) exists and is live but empty — wave 4 is what populates it.

---
*Phase: 04-launcher*
*Completed: 2026-08-28*

## Self-Check: PASSED

All 14 files claimed as created/modified were confirmed present on disk (`launcher/rust-toolchain.toml`, `launcher/Cargo.toml`, `launcher/core/src/{http,paths,auth}.rs`, `launcher/core/src/bin/campfire-cli.rs`, `launcher/src-tauri/src/lib.rs`, `launcher/ui/main.js`, `auth-service/src/{db,api}.rs`, `scripts/auth-smoke.sh`, `caddy/Caddyfile`, `launcher-dist/.gitkeep`, this SUMMARY), and all 3 task commit hashes (`12bdb77`, `b9b787d`, `87341a1`) were confirmed present in `git log --oneline --all`.
