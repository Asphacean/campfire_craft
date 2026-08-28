---
phase: 03-modpack-distribution
plan: 01
subsystem: infra
tags: [caddy, tls, openssl, private-ca, rust, axum, tokio, minecraft-slp, https, rate-limiting]

# Dependency graph
requires:
  - phase: 02-accounts-enforced-auth
    provides: "campfire-auth Rust/axum service (POST /register, /login, /validate, GET /status stub), auth-service/README.md API contract, the rate-limiter forwarded-for obligation this plan discharges"
provides:
  - "caddy.service on TCP 8444, official apt repo (Caddy 2.11.4), admin off, auto_https off, own-CA TLS"
  - "A private CA (ca/campfire-ca.pem, tracked) and scripts/renew-cert.sh — the trust anchor Phase 4 pins and Phase 5 embeds"
  - "caddy/Caddyfile: the whole public HTTPS route table — manifest+pack file_server, /api/register|login proxy (prefix stripped), /status proxy, write guard (405), terminal 404, no /api/validate route"
  - "scripts/install-caddy.sh: idempotent install, Caddyfile drift guard, ACL-based caddy-user traversal grant, leaf key permissions"
  - "GET /status: real Server List Ping (auth-service/src/slp.rs, hand-rolled, no crate), 10s cache, {online,players,max,motd}, always HTTP 200"
  - "Forwarded-for-aware rate limiting: Caddy SETS X-Forwarded-For at the edge, campfire-auth's client_ip() trusts it only from a loopback peer"
  - "ca/ + caddy/Caddyfile riding along in the six-hourly world-*.tar.zst backup (D-12)"
affects: [03-02-manifest-and-pack, 04-launcher]

# Actuals (#2632)
actuals:
  tokens: 11317
  tasks: 3
  commits: 3

# Tech tracking
tech-stack:
  added:
    - "Caddy 2.11.4 (official Cloudsmith apt repo) — no new crate/library, host package only"
    - "OpenSSL 3.5.6 CLI (already installed) for the own-CA chain — no new tooling"
  patterns:
    - "Caddyfile route{} wrapper to force literal top-to-bottom directive execution — bare respond/handle/handle_path are otherwise silently reordered by Caddy's fixed internal priority, which let a PUT slip past a 405 write guard into file_server's default 404 (confirmed live)"
    - "handle_path (not handle) for a file_server prefix that must be stripped before root resolution — handle preserves the full URI, producing a double-prefixed pack/pack/... lookup"
    - "header_up X-Forwarded-For <value> inside reverse_proxy{} SETS (never appends) the header, discarding anything the client supplied at the edge — must be nested inside reverse_proxy{}, not a standalone handle{} directive"
    - "client_ip() in api.rs: trust a forwarded-for header's last CSV element only when the direct TCP peer is loopback, else use the raw peer — belt-and-braces given the service already binds loopback only"
    - "Hand-rolled ~140-line Server List Ping client (protocol 340) over tokio::net::TcpStream, single async fn wrapping a 5s timeout, read-exactly loop for a multi-segment ~7.2kB response, defensive description (string vs {text} object) parsing"
    - "install-caddy.sh's port pre-flight skips itself when caddy.service already owns the port — a literal 'refuse if anything listens' check breaks idempotent re-runs once Caddy's own first install claims the port"

key-files:
  created:
    - scripts/renew-cert.sh
    - scripts/install-caddy.sh
    - caddy/Caddyfile
    - ca/campfire-ca.pem
    - auth-service/src/slp.rs
  modified:
    - auth-service/Cargo.toml
    - auth-service/Cargo.lock
    - auth-service/src/api.rs
    - auth-service/src/main.rs
    - auth-service/README.md
    - scripts/auth-smoke.sh
    - scripts/backup.sh
    - server.env
    - server.env.example
    - .gitignore

key-decisions:
  - "Wrapped the entire Caddyfile site body in route{} after discovering live that Caddy reorders bare directives by fixed internal priority, not file order — without it, a PUT to /pack/* fell through the 405 write guard into file_server's own 404 instead."
  - "Used handle_path (not handle, as RESEARCH.md's skeleton showed) for /pack/* so file_server resolves the prefix-stripped path — found live via a hash mismatch (empty-string sha256) on the first tracer verification."
  - "install-caddy.sh's port pre-flight only fires when caddy.service is not already active — a literal always-refuse-if-listening check broke the plan's own idempotency acceptance criterion on the second run."
  - "The plan's acceptance criterion expecting a non-zero curl exit for plaintext HTTP on :8444 does not hold on this Caddy/Go stack: Go's net/http server detects a plaintext request on a TLS listener and replies with a benign HTTP 400 'Client sent an HTTP request to an HTTPS server' instead of dropping the connection, so curl succeeds (exit 0) with a 400. The underlying security property (content is never served in plaintext — no manifest/pack bytes cross this port unencrypted) holds; only the literal curl-exit-code framing in the criterion doesn't match real behavior. Verified with -v; not a functional gap."

patterns-established:
  - "Any future Caddyfile edit that mixes bare directives with ordering-sensitive matchers (write guards, method gates) must live inside route{} — Caddy's automatic reordering is a live footgun, not a documentation nitpick."
  - "A file_server directive scoped under a path prefix must use handle_path, never handle, unless the root is set to include that prefix."

requirements-completed: [DIST-01, DIST-04]

coverage:
  - id: D1
    description: "A request to https://mc.campfire.pub:8444/manifest.json presenting only ca/campfire-ca.pem as trust anchor returns JSON with a files array, over a certificate that validates"
    requirement: "DIST-01"
    verification:
      - kind: manual_procedural
        ref: "curl -sf --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/manifest.json | jq -e '.files|length>=1'; openssl verify -CAfile ca/campfire-ca.pem ca/mc.campfire.pub-cert.pem"
        status: pass
    human_judgment: false
  - id: D2
    description: "A file listed in the manifest downloads over the same HTTPS front and its sha256 matches the manifest's published value"
    requirement: "DIST-01"
    verification:
      - kind: manual_procedural
        ref: "curl --cacert ca/campfire-ca.pem .../pack/mods/campfire-auth-0.1.1.jar | sha256sum, compared against manifest.json's files[0].sha256 — matched"
        status: pass
    human_judgment: false
  - id: D3
    description: "POST /api/login through the HTTPS front reaches campfire-auth and returns its real answer (401 for bad creds); POST /api/validate through the same front returns 404 — the join-path endpoint is not published"
    requirement: "DIST-01"
    verification:
      - kind: manual_procedural
        ref: "curl -X POST .../api/login with bad creds -> 401; curl -X POST .../api/validate -> 404"
        status: pass
    human_judgment: false
  - id: D4
    description: "GET /status reports a real player count from Server List Ping (four fields, exactly, <512 bytes, no Forge mod list) and returns online:false with HTTP 200 when the game server is unreachable"
    requirement: "DIST-04"
    verification:
      - kind: manual_procedural
        ref: "scripts/auth-smoke.sh PASS lines: 'live /status has exactly 4 keys...', 'live /status body is under 512 bytes', 'GET /status with SLP_ADDR pointed at a dead port still returns 200', 'offline /status: online false, players/max/motd all null'; live curl through Caddy: {\"online\":true,\"players\":0,\"max\":10,\"motd\":\"campfire.pub\"}, 58 bytes"
        status: pass
    human_judgment: false
  - id: D5
    description: "Registration rate limiting distinguishes two different client addresses arriving through Caddy, and a client-supplied X-Forwarded-For header cannot buy a fresh budget"
    verification:
      - kind: manual_procedural
        ref: "live curl through Caddy: six calls from --interface 127.0.0.2 print 400x5 then 429; --interface 127.0.0.3 immediately after prints 400; a forged X-Forwarded-For from the exhausted 127.0.0.2 still prints 429. scripts/auth-smoke.sh PASS: 'a loopback peer's request is rate-limited under its forwarded-for header's address...', 'the same peer without a forwarded-for header uses its own untouched budget'"
        status: pass
    human_judgment: false
  - id: D6
    description: "The caddy system user can read the leaf key and the pack tree, and cannot read the CA private key or the accounts database"
    verification:
      - kind: manual_procedural
        ref: "sudo -u caddy test -r ca/mc.campfire.pub-key.pem (0), sudo -u caddy test -r pack/manifest.json (0); sudo -u caddy cat ca/campfire-ca-key.pem (nonzero), sudo -u caddy cat auth/campfire.db (nonzero)"
        status: pass
    human_judgment: false
  - id: D7
    description: "Nothing is listening on Caddy's default admin port, a non-GET/HEAD request to the pack surface is refused (405), and no directory listing or dotfile is produced (404, 0 bytes)"
    verification:
      - kind: manual_procedural
        ref: "ss -ltn 'sport = :2019' -> 0 listeners; curl -X PUT .../pack/mods/probe.jar -> 405; curl .../pack/mods/ -> 404, 0 bytes; curl .../pack/.probe -> 404"
        status: pass
    human_judgment: false
  - id: D8
    description: "Re-running scripts/renew-cert.sh issues a fresh leaf certificate without changing the CA certificate, and the HTTPS round trip still works afterwards"
    verification:
      - kind: manual_procedural
        ref: "sha256sum ca/campfire-ca.pem unchanged across two renew-cert.sh runs; openssl x509 serial changed; openssl verify OK; after sudo systemctl restart caddy, manifest fetch still succeeds"
        status: pass
    human_judgment: false
  - id: D9
    description: "A backup archive contains ca/ and caddy/Caddyfile and does not contain the pack tree"
    verification:
      - kind: manual_procedural
        ref: "bash scripts/backup.sh run live: world-20260828-144406.tar.zst contains ca/campfire-ca.pem, ca/campfire-ca-key.pem, caddy/Caddyfile, world/level.dat, auth/campfire.db, and zero pack/ members; archive count stayed at BACKUP_KEEP (14)"
        status: pass
    human_judgment: false
  - id: D10
    description: "sing-box on :443, the pbwiki containers on :80 and :8443, and rlcraft.service are all exactly as they were before this plan ran"
    verification:
      - kind: manual_procedural
        ref: "ss -ltn confirms :443/:80/:8443 unchanged listener counts; docker ps count unchanged (5); systemctl is-active rlcraft = active and uptime -s unchanged (2026-08-22 20:53:29) throughout all three tasks"
        status: pass
    human_judgment: false

# Metrics
duration: ~35min
completed: 2026-08-28
status: complete
---

# Phase 3 Plan 1: Private CA, Caddy on :8444, and a Real /status Summary

**The project's first public HTTPS surface: a private CA, Caddy 2.11.4 from the official apt repo on TCP 8444, own-CA TLS, a manifest+pack file server, `/api/register|login` proxied through with `/api/validate` deliberately unpublished, and `GET /status` now a real Server List Ping instead of a placeholder — with a rate limiter that still sees the real client through the proxy.**

## Performance

- **Duration:** ~35 min
- **Tasks:** 3
- **Commits:** 3
- **Files created:** 5 (scripts/renew-cert.sh, scripts/install-caddy.sh, caddy/Caddyfile, ca/campfire-ca.pem, auth-service/src/slp.rs)
- **Files modified:** 10

## Accomplishments

- **Private CA + Caddy on TCP 8444.** `scripts/renew-cert.sh` generates the ECDSA P-256 root once (3650 days) and re-issues the `mc.campfire.pub` leaf (730 days) on every run; `ca/campfire-ca.pem` is the tracked public trust anchor Phase 4 pins and Phase 5 embeds. Caddy 2.11.4 installed from the official Cloudsmith apt repo (Debian's own bundled candidate is 2.6.2, three years old) via `scripts/install-caddy.sh`, idempotent, with a drift guard asserting `caddy/Caddyfile` still agrees with `server.env`'s `HTTPS_PORT`/`PACK_DIR`.
- **The whole public route table, live.** `caddy/Caddyfile`: `admin off` (no admin socket at all), `auto_https off` (explicit own-CA `tls`), a 405 write guard on non-GET/HEAD to the manifest/pack surface, `/api/register` and `/api/login` proxied to `campfire-auth` with the `/api` prefix stripped and `/api/validate` deliberately absent (no wildcard), `/status` proxied unchanged, and a terminal 404 for everything else. Everything is wrapped in `route{}` after live testing showed Caddy's automatic directive reordering let a PUT slip past the write guard.
- **Real `GET /status`.** `auth-service/src/slp.rs` — a ~140-line hand-rolled Server List Ping client (protocol 340, no crate; RESEARCH.md flagged both available crates as low-adoption for a frozen sub-100-line protocol). Returns `{online, players, max, motd}`, cached 10s, always HTTP 200 (never a 5xx for "the game is off"), discarding the raw ping's 162-entry/7.2kB Forge mod list entirely. Live: 58-byte response, 7.9ms wall time uncached, `max` matches `server.env`'s `MAX_PLAYERS=10`.
- **Rate limiting sees through the proxy.** `caddy/Caddyfile`'s `header_up X-Forwarded-For {http.request.remote.host}` SETS (never appends) the header at the edge; `client_ip()` in `api.rs` trusts a forwarded-for header's last element only from a loopback peer. Live through Caddy: two distinct client interfaces get distinct 5/hour budgets, and a forged `X-Forwarded-For` from an already-exhausted interface still gets 429 — Caddy discards the client-supplied value before campfire-auth ever sees it.
- **Filesystem isolation for the `caddy` system user.** Granted traversal into `/home/asphacean` via `setfacl -m u:caddy:--x` (the `acl` package installed automatically; the `chmod 711` fallback was not needed), read on the leaf key (`chgrp caddy` + `640`) and the `pack/` tree — confirmed it can read the leaf key and manifest, and cannot read the CA private key or the accounts database.
- **Certificate rotation proven live, not just described.** Re-ran `scripts/renew-cert.sh`: root byte-identical, leaf serial changed, chain still verified, and the pinned HTTPS round trip worked immediately after restarting Caddy.
- **The CA now survives a disk loss on the world's own schedule.** `scripts/backup.sh` adds `ca/` and `caddy/Caddyfile` as a second `-C ROOT_DIR` root on the existing single tar invocation (D-12) — no second archive file, rotation loop and RCON save-off window untouched, `pack/` deliberately excluded. Live backup run confirmed both present, `pack/` absent, archive count still at `BACKUP_KEEP`.
- **`scripts/auth-smoke.sh` grew from 28 to 35 named PASS checks** (online/offline `/status` shape and size, 10s cache identity, forwarded-for trust from a loopback peer) — two consecutive runs both green.

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end "curl the manifest over HTTPS with our own CA"** — `6dad4eb` (feat)
2. **Task 2: A real /status, and a rate limiter that can still see the real client** — `f84cd54` (feat)
3. **Task 3: The certificate rotates and the CA survives a disk loss** — `27df366` (feat)

_No plan-metadata/STATE.md/ROADMAP.md commit made by this executor run per its instructions — the orchestrator owns those writes._

## Files Created/Modified

- `scripts/renew-cert.sh` — own-CA generation (once) + leaf reissue (every run), ECDSA P-256, house bash style
- `scripts/install-caddy.sh` — idempotent official-apt-repo Caddy install, Caddyfile drift guard, ACL-based caddy-user access grant, port pre-flight (skipped once caddy.service owns the port)
- `caddy/Caddyfile` — the whole public HTTPS surface: `tls`, write guard, `/api/register|login` proxy with forwarded-for override, `/status` proxy, manifest+pack file_server, terminal 404, all inside `route{}`
- `ca/campfire-ca.pem` — tracked public trust anchor (Phase 4/5 dependency)
- `auth-service/src/slp.rs` — hand-rolled Server List Ping client
- `auth-service/src/api.rs` — real `/status` handler with 10s cache; `client_ip()` forwarded-for resolution used by register/login
- `auth-service/src/main.rs` — `mod slp;`, `SLP_ADDR` env var, `AppState` wiring
- `auth-service/Cargo.toml`/`Cargo.lock` — `net`/`time`/`io-util` tokio features, no new crate
- `auth-service/README.md` — real `GET /status` contract, `SLP_ADDR`, the actual forwarded-for mechanism, new "Public route table" section
- `scripts/auth-smoke.sh` — 7 new named PASS checks (28 → 35)
- `scripts/backup.sh` — `ca/` + `caddy/Caddyfile` added to the existing tar invocation
- `server.env` / `server.env.example` — `HTTPS_PORT`, `PACK_DIR`, `CA_DIR`, `SLP_ADDR`; backup-retention comment updated
- `.gitignore` — CA/leaf private key material, leaf cert, `.srl`, `pack/` staging tree, and the stray pre-existing `caddy_*.deb` download

## Decisions Made

See `key-decisions` in the frontmatter above — summarized: `route{}` wraps the entire Caddyfile body (Caddy silently reorders bare directives by fixed internal priority, not file order — found live via a PUT slipping past the 405 write guard); `handle_path` (not `handle`, as RESEARCH.md's skeleton showed) for `/pack/*` so `file_server` resolves the prefix-stripped path (found live via a hash mismatch — empty-string sha256 — on the first tracer run); `install-caddy.sh`'s port pre-flight only fires when `caddy.service` isn't already active, or the idempotency acceptance criterion breaks on every re-run after the first install; the plan's plaintext-HTTP acceptance criterion (expecting a non-zero curl exit) doesn't hold against this Go/Caddy stack's benign "Client sent an HTTP request to an HTTPS server" 400 response — the actual security property (no plaintext content served) still holds, verified with `-v`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Caddy's automatic directive reordering let a PUT slip past the 405 write guard**
- **Found during:** Task 1, first tracer verification (`curl -X PUT .../pack/mods/probe.jar` returned 404, not the required 405)
- **Issue:** Caddy reorders bare top-level directives (`respond`, `handle`, `handle_path`, ...) by a fixed internal priority list, not by their order in the Caddyfile. The `respond @write_guard 405` line, though written first, was executed *after* the `handle_path /pack/*` block in the adapted config — so a PUT resolved through `file_server` (which returned its own bare 404 for an unsupported method) before the write guard ever ran.
- **Fix:** Wrapped the entire site body in `route { ... }`, which forces literal top-to-bottom execution instead of Caddy's automatic reordering.
- **Files modified:** `caddy/Caddyfile`
- **Verification:** `curl -X PUT .../pack/mods/probe.jar` now returns 405; `curl -X PUT .../manifest.json` also returns 405; all other routes unaffected.
- **Committed in:** `6dad4eb` (Task 1 commit)

**2. [Rule 1 - Bug] `/pack/*` needed `handle_path`, not `handle`, for the file_server prefix to resolve correctly**
- **Found during:** Task 1, first tracer verification (downloaded pack file hashed to the sha256 of an empty string — a 404 with an empty body, not the actual jar)
- **Issue:** `handle /pack/* { root * PACK_DIR; file_server }` preserves the full request URI when resolving against `root`, so a request for `/pack/mods/x.jar` resolved to `PACK_DIR/pack/mods/x.jar` (double-prefixed) instead of `PACK_DIR/mods/x.jar` — a lookup that never exists, hence the silent 404.
- **Fix:** Changed to `handle_path /pack/*`, which strips the matched `/pack` prefix before `file_server` resolves the remaining path against `root`.
- **Files modified:** `caddy/Caddyfile`
- **Verification:** `curl .../pack/mods/campfire-auth-0.1.1.jar | sha256sum` now matches the manifest's published hash.
- **Committed in:** `6dad4eb` (Task 1 commit)

**3. [Rule 3 - Blocking] `header_up` is a `reverse_proxy` subdirective, not a standalone top-level directive**
- **Found during:** Task 2, applying the Caddyfile forwarded-for change (`caddy adapt` failed: "unrecognized directive: header_up")
- **Issue:** The initial Caddyfile placed `header_up X-Forwarded-For {http.request.remote.host}` as a bare line inside `handle { }`, alongside `reverse_proxy` — Caddy 2.11.4 does not recognize `header_up` outside a `reverse_proxy { }` block.
- **Fix:** Nested `header_up` inside `reverse_proxy 127.0.0.1:8081 { header_up ... }`.
- **Files modified:** `caddy/Caddyfile`
- **Verification:** `caddy validate` passes; live curl through Caddy confirms the forged-header test (see D5 above).
- **Committed in:** `f84cd54` (Task 2 commit)

**4. [Rule 1 - Bug] `install-caddy.sh`'s port pre-flight broke idempotent re-runs**
- **Found during:** Task 1, explicit idempotency check (`bash scripts/install-caddy.sh` a second time exited non-zero: "something is already listening on :8444")
- **Issue:** The port pre-flight (`ss -ltn ... | grep -q LISTEN` → refuse) was written to fire on *any* listener, including Caddy's own listener from the first successful install — so every re-run after the first would always refuse itself, violating the plan's own explicit idempotency acceptance criterion.
- **Fix:** The check now only fires when `caddy.service` is not already active (`! systemctl is-active --quiet caddy && ss ...`) — still refuses a genuinely different service holding the port (T-03-01-13's actual concern), but no longer trips on Caddy's own steady-state listener.
- **Files modified:** `scripts/install-caddy.sh`
- **Verification:** A second `bash scripts/install-caddy.sh` run exits 0; a third run (after cert rotation) also exits 0.
- **Committed in:** `6dad4eb` (Task 1 commit)

---

**Total deviations:** 4 auto-fixed (3 bugs found via live acceptance-criteria testing, 1 blocking Caddyfile syntax error). All four were caught by running the plan's own `<verify>`/`<acceptance_criteria>` commands live, not discovered later — no scope creep, no functionality added beyond what the plan specified.

**Impact:** All four fixes were necessary for the plan's own stated acceptance criteria to pass (the write guard, correct file serving, the Caddyfile applying at all, and re-runnability). None represent a design change from what the plan asked for — each is a correction of how Caddy 2.11.4 actually behaves versus what a first-pass Caddyfile assumed.

## Issues Encountered

**The plaintext-HTTP acceptance criterion's literal curl-exit-code expectation does not hold on this stack.** `curl http://mc.campfire.pub:8444/manifest.json` (no TLS) returns HTTP `400` with the body "Client sent an HTTP request to an HTTPS server" — a benign, Go-`net/http`-stdlib-level safety response — rather than failing the TCP/TLS handshake outright, so curl's exit code is `0`, not non-zero as the criterion literally states. Verified with `curl -v`: no manifest content, no pack content, and no API response is ever produced by this path — the actual security property (nothing is served in plaintext) holds completely; only the specific tool-exit-code framing in the written criterion doesn't match reality. Not fixed (there is nothing to fix — the behavior is correct and safe), documented here for the verifier.

**The D-14 "no file under ~/pbwiki has an mtime from today" check is not literally satisfiable on a live host.** `~/pbwiki` runs its own health-check and backup cron jobs (`backup.log`, `health.log`, `data/uploads/directus-health-file`) that write with today's mtime on their own schedule, entirely independent of anything this plan's scripts or commands touched. Confirmed nothing this plan ran wrote to `~/pbwiki` (no script references that path; `docker ps` container set and `:80`/`:8443` listener counts were unchanged across every task).

## User Setup Required

None — no external service configuration required. This executor ran with operator-equivalent (passwordless sudo) access on the Pi itself, and all steps (apt install, `/etc/hosts` edit, service enable/restart) completed directly.

## Next Phase Readiness

- `caddy.service` is enabled, live, and serving a hardened static+proxy surface at `mc.campfire.pub:8444` — plan 03-02 only needs to grow `pack/manifest.json` and `pack/` to the real 187-file set; the route table, TLS, permissions, and write guard need no changes.
- `auth-service/README.md`'s "Public route table" section is the contract Phase 4's launcher builds against.
- `ca/campfire-ca.pem` is the trust anchor Phase 4 pins and Phase 5 embeds — tracked, verified, and already survives a leaf rotation without changing.
- `GET /status` is real and ready for the launcher to poll (LNCH-07).
- `rlcraft.service` was live and `active` throughout every task in this plan and was never touched; its uptime (`2026-08-22 20:53:29`) was unchanged from start to finish.

---
*Phase: 03-modpack-distribution*
*Completed: 2026-08-28*

## Self-Check: PASSED

All key files verified present on disk: `scripts/renew-cert.sh`, `scripts/install-caddy.sh`, `caddy/Caddyfile`, `ca/campfire-ca.pem`, `auth-service/src/slp.rs`. All three task commits (`6dad4eb`, `f84cd54`, `27df366`) verified present via `git log --oneline --all`. Live system state re-checked at write time: `systemctl is-active caddy campfire-auth rlcraft` all `active`, `curl --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/manifest.json` returns real manifest data with a hash-matching pack file, `/status` returns real player data, rate limiting through Caddy distinguishes clients and ignores a forged forwarded-for header, `bash scripts/auth-smoke.sh` = `SMOKE OK (35 checks)`, `uptime -s` unchanged since before this plan ran.
