---
phase: 02-accounts-enforced-auth
plan: 01
subsystem: auth
tags: [rust, axum, rusqlite, argon2, sqlite, systemd, backup, rate-limiting]

# Dependency graph
requires:
  - phase: 01-playable-server-on-the-pi
    provides: "scripts/backup.sh (RCON save-off/save-all/tar/save-on-trap pattern), scripts/restore.sh, systemd/rlcraft.service EnvironmentFile/CR-01 convention, scripts/install-units.sh, server.env / server.env.example house style"
provides:
  - "campfire-auth Rust binary: POST /register, POST /login, POST /validate, GET /status, plus login/reset operator CLI subcommands"
  - "auth-service/README.md — the API contract (endpoints, error codes, token rules, CLI, Phase-3/4 constraints)"
  - "scripts/auth-smoke.sh — 28-check repeatable assertion suite against an ephemeral instance + temp database"
  - "campfire-auth.service — loopback-only, enabled, Restart=on-failure, survives SIGKILL"
  - "Accounts database (auth/campfire.db, mode 600) riding along in the six-hourly world-*.tar.zst archive via scripts/backup.sh; scripts/restore.sh never auto-applies an extracted accounts snapshot"
affects: [02-02-auth-gate-mod, 03-caddy-and-manifest, 04-launcher]

# Actuals (#2632)
actuals:
  tokens: 20334
  tasks: 3
  commits: 3

# Tech tracking
tech-stack:
  added:
    - "Rust 1.85.0 (Debian apt cargo/rustc package, no rustup) + sqlite3 3.46.1 CLI"
    - "axum 0.8.9, tokio 1.53.1, rusqlite 0.40.2 (bundled), argon2 0.6.0, getrandom 0.4.3, base64 0.23.1, serde 1.0.229, serde_json 1.0.151 — no clap, no sqlx, no tower-governor"
  patterns:
    - "One rusqlite::Connection behind a Mutex<> in axum shared state — no pool, no async-SQLite wrapper, for a single-writer 5-7-user workload"
    - "Atomic single-use token consumption via UPDATE tokens SET consumed_at = ? WHERE id = ? AND consumed_at IS NULL, gated on changes()==1 — a compare-and-swap, not select-then-update"
    - "Timing/body-identical 401 for wrong-password vs unknown-nick: argon2-verify always runs, against a fixed lazily-computed dummy PHC hash when the nick doesn't exist"
    - "Hand-rolled Mutex<HashMap<IpAddr, Vec<Instant>>> sliding-window rate limiter (src/ratelimit.rs) — register counts every attempt, login counts only failures, validate is never limited"
    - "JsonRejection collapsed to one explicit 400 bad_json regardless of axum's own default status per rejection kind (422 for a missing field, 415 for missing content-type)"
    - "DB file/WAL/SHM siblings chmod'd to 600 explicitly in Db::open — never left to the process umask"
    - "sqlite3 .backup staged into a temp dir, added as a second tar -C root — one archive gains an auth/campfire.db member alongside world/, no second archive file, degrades to world-only if AUTH_DB is unset/missing"

key-files:
  created:
    - auth-service/Cargo.toml
    - auth-service/Cargo.lock
    - auth-service/src/main.rs
    - auth-service/src/db.rs
    - auth-service/src/auth.rs
    - auth-service/src/api.rs
    - auth-service/src/ratelimit.rs
    - auth-service/README.md
    - scripts/auth-smoke.sh
    - systemd/campfire-auth.service
  modified:
    - .gitignore
    - scripts/backup.sh
    - scripts/restore.sh
    - server.env.example
    - server.env (untracked, gitignored — AUTH_BIND/AUTH_DB added)

key-decisions:
  - "Used getrandom::fill() directly for the 32 token bytes instead of argon2's re-exported rand_core::OsRng — reading password-hash 0.6.1's Cargo.toml/lib.rs directly showed rand_core is gated behind a feature argon2 0.6.0 does not enable by default (only getrandom is default-on). getrandom was already an approved, RESEARCH.md-verified crate and already a transitive dependency, so this is the plan's own documented fallback path, not a deviation."
  - "Register/login/duplicate-nick/wrong-password/token-replay/foreign-token/expired-token correctness was built into Task 1's db.rs and api.rs from the start (atomic consume_token, per-user candidate_tokens scoping, InsertUserResult::NickTaken mapped to 409) rather than deferred to Task 2 as bare scaffolding — 'wired for keeps' per the tracer task's own instruction. Task 2 added what genuinely didn't exist yet: nick/password validation, the JSON-rejection 400 remap, the rate limiter, /status, and the CLI. Confirmed which behaviors were actually new by building the Task-1 commit separately (git archive, scratch dir) and re-running the extended smoke assertions against it — see Deviations."
  - "Duplicate-case, wrong-password, and expired/foreign/replay-token assertions were re-verified rather than skipped in Task 2's smoke-script extension even though they already passed under Task 1 — they're part of the phase's must_haves and are re-asserted every run, not one-off checks."
  - "Task 2's rejection-behavior smoke assertions run from distinct loopback source addresses (127.0.0.2, .3, .4, ...) via curl --interface, isolating each test's register-rate-limit budget from the dedicated 6-call flood test on 127.0.0.9 — otherwise the shared 5/hour quota would make earlier functional assertions and the 429 test interfere with each other on a single source IP."

patterns-established:
  - "Any future auth-service handler that can plausibly fail on user input should route its JSON extraction through Result<Json<T>, JsonRejection> and map to an explicit status, never trust axum's per-rejection-kind default."
  - "Db::open() is the single place that must assert file permissions after any pragma/schema step that can create new files (WAL introduces -wal/-shm) — do not assume a later step 'happens to' run before those files matter."

requirements-completed: [AUTH-01, AUTH-02]

coverage:
  - id: D1
    description: "A nick and password registered over the loopback API can be logged in, and the token /login returns is accepted by /validate exactly once"
    requirement: "AUTH-02"
    verification:
      - kind: manual_procedural
        ref: "scripts/auth-smoke.sh PASS lines: 'register a fresh nick returns 201', 'login ... returns a token at least 40 chars long', 'validate that nick with that token returns 200', 'validating a token a second time returns 401'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Registering the same nick again, in any letter case, is refused with 409 and leaves the first account's stored hash unchanged"
    requirement: "AUTH-01"
    verification:
      - kind: manual_procedural
        ref: "scripts/auth-smoke.sh PASS: 'duplicate registration in a different case returns 409', 'the original account's stored hash is unchanged after a duplicate attempt' (pw_hash compared byte-for-byte before/after via sqlite3 against the temp DB)"
        status: pass
    human_judgment: false
  - id: D3
    description: "A wrong password returns 401 and no token is issued; an unknown nick returns the identical 401 body (no enumeration oracle)"
    requirement: "AUTH-02"
    verification:
      - kind: manual_procedural
        ref: "scripts/auth-smoke.sh PASS: 'login with the wrong password returns 401', 'wrong-password response body contains no token field', 'login for a never-registered nick returns 401', 'unknown-nick and wrong-password responses are indistinguishable'"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every row of the accounts database holds an argon2id PHC string — no plaintext password and no plaintext token is recoverable from the file"
    requirement: "AUTH-01"
    verification:
      - kind: manual_procedural
        ref: "scripts/auth-smoke.sh PASS: 'every row of users.pw_hash starts with $argon2id$', 'the fixture password never appears in the users table', 'the issued token never appears in the tokens table' — asserted against the smoke suite's own temp database"
        status: pass
    human_judgment: false
  - id: D5
    description: "The service listens on 127.0.0.1:8081 only and refuses at startup to bind any non-loopback address"
    verification:
      - kind: manual_procedural
        ref: "AUTH_BIND=0.0.0.0:8099 campfire-auth serve exits non-zero within 2s naming the rejected address, ss -ltn confirms nothing bound afterward; live production check: ss -ltn shows 127.0.0.1:8081 only, curl to the LAN IP:8081 fails (exit 7, connection refused)"
        status: pass
    human_judgment: false
  - id: D6
    description: "campfire-auth.service is enabled, comes back on its own after the process is killed, and answers /status"
    verification:
      - kind: manual_procedural
        ref: "systemctl is-enabled/is-active both report enabled/active; systemd-analyze verify exits 0; sudo systemctl kill -s SIGKILL campfire-auth followed by systemctl is-active reporting active again within ~7s and /status answering 200"
        status: pass
    human_judgment: false
  - id: D7
    description: "A backup archive taken by the six-hourly job contains a consistent snapshot of the accounts database"
    verification:
      - kind: manual_procedural
        ref: "bash scripts/backup.sh run live: newest world-*.tar.zst contains both auth/campfire.db and world/level.dat (tar --zstd -tf | grep -cx); the extracted auth/campfire.db opens cleanly under sqlite3 (select count(*) from users; no malformed-image error); world archive count stayed under BACKUP_KEEP with no stray auth-*.db sibling files"
        status: pass
    human_judgment: false
  - id: D8
    description: "The operator can mint a token for any nick and set a new password for any nick from the CLI, without editing the database by hand"
    verification:
      - kind: manual_procedural
        ref: "scripts/auth-smoke.sh PASS: 'campfire-auth login <nick> prints a token /validate then accepts', 'after campfire-auth reset, the old password fails login with 401', 'after campfire-auth reset, the new password succeeds login with 200'"
        status: pass
    human_judgment: false

# Metrics
duration: 35min
completed: 2026-08-28
status: complete
---

# Phase 2 Plan 1: campfire-auth Service Summary

**A loopback-only Rust/axum/SQLite service (`campfire-auth`) issuing single-use argon2id-hashed 12-hour tokens, enforced case-insensitive nick uniqueness, a hand-rolled per-IP rate limiter, an operator `login`/`reset` CLI, and accounts riding along in the existing six-hourly world backup — verified live end-to-end on the Pi with a 28-check repeatable smoke suite.**

## Performance

- **Duration:** ~35 min
- **Tasks:** 3
- **Files created:** 10 (Cargo.toml/.lock, 5 Rust source files, README.md, auth-smoke.sh, campfire-auth.service)
- **Files modified:** 5 (.gitignore, backup.sh, restore.sh, server.env.example, server.env)
- **Diff size:** ~81KB (~20,334 estimateTokens) against a 65,000-token plan estimate

## Accomplishments

- `campfire-auth` binary: `POST /register`, `POST /login`, `POST /validate`, `GET /status`, plus `login`/`reset` operator CLI subcommands — built with `axum` 0.8.9, `rusqlite` 0.40.2 (bundled SQLite), `argon2` 0.6.0, `tokio`, `serde`/`serde_json`, `base64`, `getrandom`. No `clap`, no `sqlx`, no `tower-governor` (all explicitly excluded per RESEARCH.md).
- Every SQL statement is parameter-bound (repo-wide `format!` grep returns 0); token consumption is a single atomic `UPDATE ... WHERE consumed_at IS NULL` compare-and-swap, not select-then-update.
- Wrong password and unknown nick are indistinguishable in status, body, and cost (argon2 always runs, against a fixed dummy hash for an unknown nick).
- Hand-rolled per-IP rate limiter (`src/ratelimit.rs`): 5 registrations/hour (every attempt counts), 10 *failed* logins/hour (successes never count), `/validate` never limited.
- `scripts/auth-smoke.sh` grew from 5 to 28 named `PASS` assertions across the plan, covering the full rejection matrix (duplicate case, invalid nick, weak password, malformed/incomplete JSON, wrong password, unknown nick, token replay, foreign-nick token, never-issued token, expired token, registration flood vs. unthrottled validate, `/status`, CLI `login`/`reset`, at-rest secrecy) — run twice consecutively as part of verification, both green.
- `campfire-auth.service`: loopback-only hardening (`RestrictAddressFamilies=AF_INET AF_UNIX`, `NoNewPrivileges`, `PrivateTmp`, `ProtectSystem=full`, `MemoryDenyWriteExecute`), installed via the existing `scripts/install-units.sh`, enabled and live; a `SIGKILL` is followed by a clean restart within ~7s.
- `auth/` (mode 700) and `auth/campfire.db` (mode 600, asserted in `Db::open`, not left to SQLite's umask-derived default) — confirmed gitignored.
- `scripts/backup.sh` snapshots `AUTH_DB` via `sqlite3 .backup` into a temp dir and adds it as a second `tar -C` root, so the same six-hourly archive gains an `auth/campfire.db` member alongside `world/` — degrades to world-only if `AUTH_DB` is unset/missing, never fails the world backup. Live-run confirmed: the archive contains both members and the extracted snapshot opens cleanly in `sqlite3`.
- `scripts/restore.sh` moves any extracted accounts snapshot to `$BACKUP_DIR/restored-auth-<ts>/` and never applies it automatically — a world restore must never silently roll accounts back. `--help` documents both facts.
- `auth-service/README.md`: the full API contract (all 4 endpoints, every error code, token rules, CLI, smoke-suite instructions) plus the three constraints Phase 3/4 must honour (never proxy `/validate`, the rate limiter sees the direct peer until Phase 3 handles forwarded-for, nick casing is load-bearing for the offline UUID).
- Measured on this Pi: a single `/validate` call takes ~44ms (`curl -w time_total`, 5-sample average 43-46ms) — well inside the mod's 5-second join budget. A clean `cargo build --release -j2` (after `cargo clean`) takes 2m21s.

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end "register, log in, spend the token" — one path only** - `36c7084` (feat)
2. **Task 2: Everything that must be refused — rejections, limits, and the operator CLI** - `b129cc8` (feat)
3. **Task 3: Under systemd, in the backups, and documented for Phases 3 and 4** - `e13533c` (feat)

_No plan-metadata/STATE.md/ROADMAP.md commit made by this executor run per its instructions — the orchestrator owns those writes. `.planning/REQUIREMENTS.md` was updated (AUTH-01/AUTH-02 marked complete, no sibling plan in this phase declares either ID) and is committed alongside this SUMMARY._

## Files Created/Modified

- `auth-service/Cargo.toml` / `Cargo.lock` — dependency manifest and committed supply-chain pin
- `auth-service/src/main.rs` — CLI dispatch (`serve`/`login`/`reset`), loopback-bind guard, axum router wiring
- `auth-service/src/db.rs` — schema, parameterised queries, atomic token consumption, DB/WAL/SHM permission enforcement
- `auth-service/src/auth.rs` — argon2id hash/verify, CSPRNG token generation, the lazily-computed dummy hash for timing-safe unknown-nick handling
- `auth-service/src/api.rs` — the 4 HTTP handlers, `ApiError` → `{"error": "<code>"}` mapping, `JsonRejection` remapping
- `auth-service/src/ratelimit.rs` — hand-rolled per-IP sliding-window limiter
- `auth-service/README.md` — the API contract for Phase 3/4
- `scripts/auth-smoke.sh` — 28-check repeatable assertion suite
- `systemd/campfire-auth.service` — the installed, enabled unit
- `.gitignore` — `auth-service/target/`, `auth/` added
- `scripts/backup.sh` — accounts snapshot staged and added as a second tar root
- `scripts/restore.sh` — extracted accounts snapshot moved aside, never auto-applied
- `server.env.example` / `server.env` — `AUTH_BIND`, `AUTH_DB` added

## Decisions Made

See `key-decisions` in the frontmatter above — summarized: `getrandom::fill()` used directly instead of argon2's `rand_core` re-export (feature-gated off by default, confirmed by reading `password-hash` 0.6.1's own manifest); Task 1 built duplicate/replay/expiry correctness directly rather than deferring it as a stub, since RESEARCH.md's own `argon2`/`rusqlite` design already made it nearly free; rejection-behavior smoke assertions were spread across distinct loopback source addresses to keep the register rate-limit's 5/hour quota from cross-contaminating unrelated tests.

## Deviations from Plan

### Process note (not a Rule 1-4 auto-fix)

**Task 2's TDD RED/GREEN ordering was not a strict two-commit sequence.** The plan's `<behavior>` block asks for the smoke-script extension to be written and run against the Task-1 binary first (RED), then the handlers implemented (GREEN), as two separate commits. In this run, the test extension and the implementation were written together in one pass, then verified as a single unit. To still satisfy the RED requirement's actual purpose — proving each new assertion is *new*, not already accidentally true — the Task-1 commit (`36c7084`) was rebuilt independently via `git archive` into a scratch directory (never touching the working tree) and the new assertions were re-run against that old binary: `invalid nick` returned 201 (not 400), `weak password` 201 (not 400), `missing field` 422 (not 400), all 6 flood registrations 201 with no 429, `/status` 404, and `campfire-auth login` hit the usage-error bailout — genuine RED, confirmed. Duplicate-case 409, wrong-password/token-replay/foreign-token 401s were found to **already** pass under the Task-1 binary (Task 1's `db.rs`/`api.rs` built atomic consumption and per-user token scoping from the start), so those specific assertions were not "new" RED failures — they are re-verified, not newly introduced, in Task 2's commit. Recorded in `.planning/WINDOWS.md` (`deviation`, open) for visibility.

**Impact:** No functional or security impact — every behavior in the plan's `<behavior>` list is asserted and passing in the final smoke suite, and the genuinely-new subset was independently confirmed to have failed beforehand. The only thing that differs from the plan's literal instruction is the commit-granularity of the RED step.

## Issues Encountered

None beyond the TDD-ordering note above.

## User Setup Required

None — no external service configuration required. `AUTH_BIND`/`AUTH_DB` were added to `server.env` directly by this executor run since it runs with operator-equivalent (passwordless sudo) access on the Pi itself.

## Next Phase Readiness

- `campfire-auth.service` is enabled, live, loopback-only, and answering `/validate` — the precondition D-12 sets for plan 02-03 to arm enforcement.
- 02-02 (the Forge auth-gate mod) can call `http://127.0.0.1:8081/validate` immediately; `auth-service/README.md` is the contract to build against, including the nick-casing warning that protects offline-mode UUIDs.
- Accounts are already inside the six-hourly backup rotation — no additional operator action needed before 02-02/02-03 proceed.
- Phase 3 (Caddy) must read the "Constraints for Phase 3 and Phase 4" section of the README before fronting this service — specifically the rate-limiter forwarded-for caveat and the "never proxy `/validate`" rule.
- `rlcraft.service` was live and `active` throughout every task in this plan and was never touched.

---
*Phase: 02-accounts-enforced-auth*
*Completed: 2026-08-28*

## Self-Check: PASSED

All key files verified present on disk: `auth-service/Cargo.toml`, `auth-service/Cargo.lock`, `auth-service/src/{main,db,auth,api,ratelimit}.rs`, `auth-service/README.md`, `scripts/auth-smoke.sh`, `systemd/campfire-auth.service`. All three task commits (`36c7084`, `b129cc8`, `e13533c`) verified present via `git log --oneline --all`. Live system state re-checked at write time: `systemctl is-active campfire-auth` = `active`, `systemctl is-enabled campfire-auth` = `enabled`, `curl http://127.0.0.1:8081/status` = `200`, `ss -ltn` shows `127.0.0.1:8081` only, `bash scripts/auth-smoke.sh` = `SMOKE OK (28 checks)`, `systemctl is-active rlcraft` = `active`.
