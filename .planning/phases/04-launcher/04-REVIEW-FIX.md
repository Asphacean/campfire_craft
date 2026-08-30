---
phase: 04-launcher
fixed_at: 2026-08-30T15:23:01Z
review_path: .planning/phases/04-launcher/04-REVIEW.md
iteration: 1
findings_in_scope: 5
fixed: 5
skipped: 0
status: all_fixed
---

# Phase 04-launcher: Code Review Fix Report

**Fixed at:** 2026-08-30T15:23:01Z
**Source review:** .planning/phases/04-launcher/04-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 5 (WR-01..WR-05 — Warning tier, `critical_warning` scope)
- Fixed: 5
- Skipped: 0
- Info tier (IN-01..IN-03) out of scope per task instructions, except IN-01 was evaluated per an explicit "fix if trivial and UI-SPEC allows" carve-out — see the Info section below for why it was not applied.

Gates run for every commit: `cargo test --workspace` and `cargo clippy --workspace --all-targets` (launcher, via the rustup 1.98.0 toolchain pinned in `launcher/rust-toolchain.toml`, invoked as `~/.cargo/bin/cargo`, never the bare apt `cargo`/`rustc` on `PATH`); `cargo build --release` / `cargo clippy --all-targets` / `cargo test` (auth-service, via the APT cargo 1.85 toolchain); `bash -n` on `scripts/auth-smoke.sh`. All clean before every commit.

## Fixed Issues

### WR-02 / WR-03: manifest never-touch guard — case-insensitive, and folded into `validate()` (one root-cause fix for both)

**Files modified:** `launcher/core/src/manifest.rs`, `launcher/core/tests/manifest_guard.rs`
**Commit:** `c586804`
**Applied fix:** `assert_never_touch` now compares `NEVER_TOUCH_DIRS`/`NEVER_TOUCH_TOP_LEVEL_FILES` with `.eq_ignore_ascii_case` instead of `==`/`.contains()`, closing the case-sensitivity hole where `"OPTIONS.TXT"`/`"Saves/x"` resolve to the same file as the protected path on the two shipped case-insensitive filesystems (Windows/NTFS, macOS/APFS). `validate()` now also calls `assert_never_touch` for every `files[]`/`delete[]` entry, which both `sync()` and `verify()` already call before any download or write begins — so a never-touch violation now rejects the *whole* manifest up front, giving the never-touch guard the same "sync never begins" guarantee the path-traversal/forbidden-prefix checks already had, rather than being caught lazily per-file mid-batch where already-in-flight sibling downloads could land on disk first. This single change addresses both WR-02 (case-sensitivity) and WR-03 (partial-write-before-rejection) as the review itself noted they share the same fix. Added case-varied guard tests (`OPTIONS.TXT`, `Saves/World/level.dat` in both `files[]` and `delete[]`) and a whole-manifest-rejected-before-any-write test specifically for the never-touch guard (mirroring the existing path-traversal test of the same shape).

### WR-05: non-finite RAM values clamped at the shared `build_launch_command` boundary

**Files modified:** `launcher/core/src/launch.rs`, `launcher/core/tests/launch_command.rs`
**Commit:** `7b031e4`
**Applied fix:** `f32::clamp` does not sanitize `NaN` (returns it unchanged), and `NaN as u64` saturates to `0`, silently producing `-Xms0M -Xmx0M`. Guarded once in `build_launch_command` (`let ram_gb = if ram_gb.is_finite() { ram_gb.clamp(3.0, 10.0) } else { 3.0 };`) — the boundary every caller (Tauri `play` command, `campfire-cli play`/`launch-cmd`) funnels through — rather than patching each caller separately. Added a test proving both `f32::NAN` and `f32::INFINITY` fall back to the 3GB default (`-Xmx3072M`/`-Xms3072M`) instead of producing a zero heap.

### WR-01: explicit restrictive CSP instead of `csp: null`

**Files modified:** `launcher/src-tauri/tauri.conf.json`
**Commit:** `83deb57`
**Applied fix:** Set `"csp": "default-src 'self'; img-src 'self'; style-src 'self'"`. Verified against the actual UI first — no inline style attributes or JS-set inline styles, no remote origins, no `data:`/`asset:` URIs anywhere in `index.html`/`style.css`/`main.js` (every image is a local file bundled under `frontendDist`, every network op goes through `invoke()` IPC, not `fetch()`) — so `'unsafe-inline'`/an `asset:` scheme were deliberately left out as unneeded today, narrower than the review's own example. Verified with a real `cargo tauri build --no-bundle` (via `PATH="$HOME/.cargo/bin:$PATH" cargo tauri build --no-bundle`, since the standalone `cargo-tauri` binary otherwise shells out to the apt cargo on `PATH`), which built cleanly and produced `launcher/target/release/campfire-launcher`.

### WR-04: `POST /api/logout` — server-side refresh-token revocation

**Files modified:** `auth-service/src/api.rs`, `auth-service/src/main.rs`, `auth-service/README.md`, `caddy/Caddyfile`, `launcher/core/src/auth.rs`, `launcher/src-tauri/src/lib.rs`, `scripts/auth-smoke.sh`
**Commit:** `86f7073`
**Applied fix:** Added `POST /logout` to auth-service, mirroring `/refresh`'s candidate-walk compare-and-swap exactly (look up user, walk unexpired/unrevoked refresh-token candidates, argon2-verify, atomic revoke) minus the reissue step — revoke only, no new token minted. Given its own named rate limiter (`logout_limiter`, 60/hour/peer, same shape/limit as `refresh_limiter`) so a logout burst never eats `/refresh`'s budget. Published at `/api/logout` through Caddy identically to `/api/refresh` (same `header_up X-Forwarded-For` anti-spoofing set). `launcher/core/src/auth.rs::logout` is now `async`: reads the stored refresh token before clearing it, calls `/api/logout` best-effort, and clears local credential-store state **unconditionally** regardless of the network call's outcome — a network failure, rate limit, or already-revoked token must never leave a user stuck logged in locally. The Tauri `logout` command was updated to `async fn` + `.await` to match; `launcher/ui/main.js` already `await`ed `invoke("logout", ...)`, so no JS change was needed. `auth-service/README.md` documents the new endpoint and updates the route-count prose ("three" → "four" exact `/api` paths).

**Live deployment (Raspberry Pi, `mc.campfire.pub:8444`):**
- auth-service rebuilt with the APT cargo 1.85 toolchain (`cargo build --release`) — builds clean under the same toolchain the live systemd unit expects.
- `cargo clippy --all-targets` clean (pre-existing `collapsible_if` warnings in `client_ip`/`status` untouched by this change, not new).
- `scripts/auth-smoke.sh` extended with 4 new logout assertions (happy-path 204, revoked token dies on `/refresh` with 401, double-logout 401, unknown-nick logout 401) — **52/52 checks pass**, both before and after the binary install.
- Binary installed to `/usr/local/bin/campfire-auth`; `sudo systemctl restart campfire-auth` — service active.
- `caddy validate --config caddy/Caddyfile` passed (a 4th "Unnecessary header_up X-Forwarded-For" warning appeared, matching the already-documented IN-03 pattern for the 3 pre-existing `/api/*` handles — expected, not a regression).
- **Correction to task instructions:** `sudo systemctl reload caddy` fails on this host by design — the Caddyfile's own header comment states `admin off` disables the admin API socket entirely, so there is no live-reload path; `scripts/install-caddy.sh` itself uses `restart`, not `reload`. Deployed via `sudo install -m 644 caddy/Caddyfile /etc/caddy/Caddyfile && sudo systemctl restart caddy` instead (the documented redeploy path) — `caddy.service` active afterward.
- Verified end-to-end against the live public HTTPS endpoint (`https://mc.campfire.pub:8444`, pinned to `ca/campfire-ca.pem`): register → login → logout (204) → refresh with the now-revoked token (401) — confirms a logged-out session's refresh token can no longer mint a fresh game token.
- `rlcraft.service` (the live Minecraft server) was never touched, per the environment constraint.

## Info (evaluated, not fixed)

### IN-01: `/status`'s `motd` field — not wired into a trivial fix

Evaluated per the task's explicit carve-out ("fix IN-01 motd rendering only if trivial and UI-SPEC allows; otherwise skip"). `04-UI-SPEC.md`'s status-pill contract is fully locked (text states "Checking…"/"Online"/"Offline", color-pairing rule, 4px chrome padding) with no tooltip, motd, or any additional status-pill element specified anywhere in the document. Adding a motd tooltip would be inventing a UI element outside the locked design contract, not a trivial fix within it — skipped, consistent with the review's own framing ("not a defect... whichever the actual roadmap intends").

### IN-02 / IN-03: out of scope (`critical_warning` fix scope; Info tier not requested for these two)

Not evaluated — task instructions scoped this run to WR-01..WR-05 plus the single named IN-01 carve-out.

---

_Fixed: 2026-08-30T15:23:01Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
