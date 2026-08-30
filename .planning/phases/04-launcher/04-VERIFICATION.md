---
phase: 04-launcher
verified: 2026-08-30T15:45:00Z
status: human_needed
score: 9/9 must-haves verified
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Clean-machine launch on Windows x64 (Phase 4 success criterion 1) — docs/LAUNCHER-BUILD.md QA matrix item 1"
    expected: "On a machine with no Java installed, register a brand-new nick, pick a RAM value, press Play once, and end up in the RLCraft world on campfire.pub"
    why_human: "Requires a real Windows window, real GPU/display, and a real game session — none of which exist on this headless Pi. The orchestration itself (session refresh -> sync -> Java -> Mojang -> Forge -> launch line) was proven end to end headlessly on the Pi via campfire-cli play --no-spawn; only the actual window and the actual game rendering are unverified."
  - test: "Progress is informative during the first Play — QA matrix item 2 (LNCH-05)"
    expected: "The step label changes through the real stages (pack files, Java, Minecraft files, Forge) with a file count and a transfer rate, with no stretch of more than a few seconds with no visible change"
    why_human: "The Tauri Channel plumbing and progress events are proven to fire (verified live: >3500 step events during sync, asset, and byte-rate events during the headless play run); whether they render legibly and promptly in a real window needs a display."
  - test: "Second launch on Windows x64 — QA matrix item 3 (Phase 4 success criterion 2, AUTH-03)"
    expected: "Reopening does not ask for a password; almost nothing downloads; the player lands in the world with position/inventory intact"
    why_human: "The mechanism is proven live on the Pi (cold play run 11.9s, warm re-run 3.1s with downloaded=0 deleted=0 seeded=0 bytes=0, session refreshed from the stored refresh token with no password prompt); the actual window behavior and a real Minecraft session's saved state need real hardware."
  - test: "Nothing of yours was touched — QA matrix item 4"
    expected: "A changed video setting and the world in saves/ both survive a second Play"
    why_human: "The never-touch guard was proven mechanically (planted saves/options/servers.dat files survive sync+verify, case-insensitive variants now rejected whole per the WR-02/WR-03 fix); a real in-game settings change across two real Play presses needs a real client."
  - test: "RAM slider behavior — QA matrix item 5 (LNCH-01, D-06)"
    expected: "Slider covers 3-10GB in half-gigabyte steps, opens at roughly half physical memory, turns red with a warning past 70%, and Play still works at that setting"
    why_human: "The formula is unit-tested and confirmed live on this 15.84GB Pi (recommended_gb=7.5); the actual slider element, its red-fill styling, and the warning text rendering need a real window."
  - test: "Wrong password / unreachable server / disk-full / Java-setup-failure banners — QA matrix items 6, 7, 9, 13 (LNCH-06)"
    expected: "Each shows its own plain-English sentence with a working Open log button, never a stack trace or raw status code"
    why_human: "server_unreachable and session_expired were triggered live on the Pi and produced exactly the mapped sentence with no internals; wrong_password/java_error/disk_full are proven only at the error-mapping unit-test level (no wrong-password attempt was made against the live account to avoid burning the register/login rate budget, and no real disk-full/corrupted-Java condition was manufactured). The window rendering and the Open-log button opening a real file need a real display."
  - test: "Offline server pill state, Play still enabled — QA matrix item 8 (LNCH-07, D-05)"
    expected: "While the game server is stopped, the pill reads Offline and Play remains enabled"
    why_human: "status.rs's offline-mapping is unit/live-tested for the fetch layer; the pill's visual state and that Play stays clickable need a real window. Also requires stopping rlcraft.service, which this verifier was explicitly told never to do."
  - test: "Verify files / Game folder buttons — QA matrix items 9, 10 (D-09)"
    expected: "Verify files repairs a manually-deleted mod and reports the count; Game folder opens the install directory in Explorer/Finder"
    why_human: "The underlying repair mechanism (manifest::verify) was proven live in wave 2 (tampered file detected and repaired); the Tauri opener plugin's actual OS file-manager launch needs a real desktop environment."
  - test: "Self-update dialog and install round-trip — QA matrix item 11 (LNCH-08)"
    expected: "A published higher-version feed shows Update available with Later/Update now; Later dismisses cleanly; Update now (if a real newer build exists) replaces the launcher"
    why_human: "All three check states (no update / update available / malformed feed => silent) were proven live against the production feed on this Pi; the actual dialog rendering and the download-and-restart round trip require a real display and, per the plan's own scope, a real Phase-5 release artifact which does not exist yet."
  - test: "Window visual contract — QA matrix item 12 (D-04, UI-SPEC)"
    expected: "RLCraft art visible behind a dark translucent panel, all text legible on the panel, Play the only orange element, tab order in reading order with a visible focus ring, window non-resizable"
    why_human: "Art provenance and the flat-color fallback were proven mechanically (byte-identical to pack assets; build succeeds with art removed); actual rendered contrast, focus ring visibility, and resizability need a real window."
  - test: "macOS Apple Silicon build, Rosetta path, translation-layer-missing handling, session persistence — QA matrix items 14-17 (LNCH-03, REL-03 preview)"
    expected: "Launcher builds and opens with the same layout; Play fetches x86_64 Java 8 under Rosetta and the game starts/renders at a reportable framerate; a missing translation layer shows the Java-setup-failure sentence with the real cause in the log; second launch doesn't ask for a password and the macOS keychain holds the entry"
    why_human: "The Rosetta decision (mac-arm64 resolving to the identical mac-x64 Adoptium archive link) was proven live on the Pi; actually running the translated JVM, seeing the game render, and reading a framerate require real Apple Silicon hardware, which this Pi is not."
  - test: "Deferred Phase 1/2 checks now unblocked by a real client"
    expected: "A friend outside the home network joins by domain and plays; a vanilla client with no token is kicked with a clear message"
    why_human: "Both require a real client on real hardware outside the local network, which only the operator's machines can provide."
---

# Phase 4: Launcher Verification Report

**Phase Goal:** A friend goes from opening the launcher to playing on the server with no manual setup of Java, Forge, or mods.
**Verified:** 2026-08-30T15:45:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Note on ROADMAP `mode: mvp`

ROADMAP.md marks this phase `Mode: mvp`, but its Goal line is deliberately **not** phrased as a formal `As a / I want to / so that` user story — `04-01-PLAN.md`'s objective states this explicitly and records that the roadmap's five numbered Phase 4 Success Criteria (verbatim below) stand in as the testable outcome instead, with a note to run `/gsd mvp-phase 4` first if a formal user story is later wanted. This verification therefore proceeds as standard goal-backward verification against those five numbered success criteria and the plan-level `must_haves`, per this task's own instructions, rather than the MVP-mode user-flow-coverage format. Flagging this so it isn't silently missed: the phase's mode tag and its actual goal-statement shape disagree, by the planners' own admission.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | AUTH-03: launcher remembers the session (refresh token), never asks for a password on a second launch | ✓ VERIFIED | Live round trip via `campfire-cli`: `login` → two consecutive `refresh` calls both succeed with rotating values; live `/api/refresh` and `/api/logout` through Caddy (curl, `--cacert ca/campfire-ca.pem`) confirmed 200/204/401 rotation-and-revocation; keyring round-trip confirmed (`keyring-selftest` PASS, linux keyutils backend); refresh token never found anywhere under the scratch install root, only in the credential store; `scripts/auth-smoke.sh` 52/52 PASS including 13 refresh + 4 logout assertions |
| 2 | LNCH-01: single screen — nick/password, Register/Login, RAM slider 3-10GB, Play | ✓ VERIFIED (code) / window rendering → human | `cargo test --workspace` 61 tests pass including RAM-formula unit tests (4GB→3, 32GB→8, 15GB→7.5, matching this real 15.84GB host's `system-memory` output); `cargo tauri build --no-bundle` succeeds producing a real desktop binary; markup/CSS tokens and Tauri command wiring present and grep-verified in prior review. Actual slider/window rendering needs a real display — QA matrix items 1, 5, 12 |
| 3 | LNCH-02: diff/download only changed files, delete stale, never touch saves/options/servers.dat | ✓ VERIFIED | Live against production manifest (3545 files, pack_version `2026-08-30T14:43:48Z`): cold `play` run downloaded 367,531,501 + 180,615,152 bytes; warm re-run reported `checked=3545 downloaded=0 deleted=0 seeded=0 bytes=0` in 3.1s vs 11.9s cold. WR-02/WR-03 case-insensitive never-touch fix confirmed present in `manifest.rs` (`eq_ignore_ascii_case`) and folded into `validate()`, with dedicated regression tests (`rejects_a_case_varied_never_touch_*`) passing |
| 4 | LNCH-03: Java 8 auto-downloaded per platform (Adoptium Temurin, x86_64 for Apple Silicon under Rosetta), never system Java | ✓ VERIFIED | Live `java-fetch` for all three targets: windows-x64 and mac-x64 checksums matched the vendor's own published sha256; mac-arm64 resolved to the **byte-identical** download link as mac-x64 (Rosetta decision, D-10), confirmed by direct comparison of printed links/checksums. `grep -rn 'JAVA_HOME' launcher/core/src` = 0; `grep -rn 'which\|from_path\|env::var("PATH")' launcher/core/src/java.rs` = 0 |
| 5 | LNCH-04: Forge installed headlessly, complete launch command with real classpath and token handoff | ✓ VERIFIED | Live `play --no-spawn` (production server, real account): headless Forge install succeeded (profile stub written, installer run under a real Java 8, produced version JSON parsed, empty-URL library checked-in-place with matching sha1, 58 merged libraries); full argv logged with every classpath entry existing on disk; `-Dcampfire.nick=`/`-Dcampfire.token=` both present in the built command and the token absent from `launcher.log` (grep for the exact observed token/password value = 0 hits in both cases); offline UUID and casing tests pass |
| 6 | LNCH-05: progress reporting (step, file count, byte rate) via a Tauri channel, not polling | ✓ VERIFIED (mechanism) / window rendering → human | Live run emitted >3500 distinct `[Checking files]`/`[Assets]` step events plus byte-rate ticks during sync and asset download; `grep -c 'Channel' launcher/src-tauri/src/lib.rs` ≥1 and `grep -c 'emit('` = 0 confirms channel-only, no event-bus. Actual bar/label rendering needs a real display — QA matrix item 2 |
| 7 | LNCH-06: plain-English error sentences (wrong password, unreachable server, failed Java, full disk) naming the log | ✓ VERIFIED (2/5 triggered live, rest unit-tested) | `server_unreachable` triggered live via a dead-port override, printed only the mapped sentence, no Rust type/status code; `session_expired` triggered live via `campfire-auth reset`, printed the mapped sentence plus `reopen_form=true`. `wrong_password`/`java_error`/`disk_full` verified only at the error-mapping unit-test level (deliberately not triggered live to avoid burning the shared account's rate budget / manufacturing a real disk-full condition). Window rendering of any banner and the Open-log button → human, QA matrix items 6, 7, 9, 13 |
| 8 | LNCH-07: server status (online/offline, player count) from `/status`, Play never disabled by it | ✓ VERIFIED (fetch layer) / pill rendering → human | `campfire-cli status` matches the live server's real-time online/player-count state; offline-mapping (any failure → offline state) is by design in `status.rs`. Pill rendering and the "Play still enabled while offline" behavior need a real window and stopping `rlcraft.service` (explicitly out of scope for this verifier) — QA matrix item 8 |
| 9 | LNCH-08: launcher checks its own version on startup and self-updates; a failed check is silent | ✓ VERIFIED (check + feed) / dialog and install round-trip → human | Live against the production feed, three states proven: running version (0.1.0) → "no update available"; feed advertising 9.9.9 → reports it; feed replaced with malformed JSON → silently reports nothing available, all exit 0. Feed live and well-formed via `curl --cacert` (both `windows-x86_64` and `darwin-aarch64` entries present with `url`+`signature`); embedded public key in `tauri.conf.json` matches the generated key file. Dialog rendering and the actual download-and-restart round trip need a real display and a real Phase-5 release artifact — QA matrix item 11 |

**Score:** 9/9 must-haves verified at the code/mechanism level (0 present-but-behavior-unverified — every behavior-dependent claim above was either exercised live on this Pi or is explicitly scoped to the human QA matrix, not silently assumed)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `launcher/core/src/manifest.rs` | Manifest sync/diff/delete/verify, path guard, never-touch guard | ✓ VERIFIED | Present, substantive (>200 lines), wired into `campfire-cli sync/verify` and `play`; WR-02/WR-03 fix confirmed landed |
| `launcher/core/src/java.rs` | Per-platform Adoptium fetch, checksum verify, extraction, never-system-Java | ✓ VERIFIED | Present, substantive; live-exercised for all 3 targets |
| `launcher/core/src/mojang.rs` | Vanilla client/library/asset fetch, SHA-1 verified, public client only | ✓ VERIFIED | `grep -c campfire_client mojang.rs` = 0, `grep -c public_client` ≥1, unit test asserts every constant URL is a Mojang/Minecraft host |
| `launcher/core/src/forge.rs` | Headless install, profile stub, merge, empty-URL library special-case | ✓ VERIFIED | Live-exercised: installer ran to completion headlessly (no DISPLAY), produced version JSON parsed, merge produced 58 libraries |
| `launcher/core/src/launch.rs` | Classpath, natives, JVM args, offline UUID, server list seed | ✓ VERIFIED | Live argv fully built and printed; every classpath entry confirmed to exist |
| `launcher/core/src/play.rs` | The whole Play sequence as one function, over a `ProgressSink` | ✓ VERIFIED | Live cold/warm runs both succeed end to end (minus final spawn, by `--no-spawn` design) |
| `launcher/core/src/update.rs` | Silent update check, semver comparison | ✓ VERIFIED | Live 3-state proof against production feed |
| `launcher/src-tauri/src/lib.rs` | Tauri commands: play, verify_files, open_game_folder, open_log, system_memory, pack_version, check_update, install_update, get_status, login, register, restore_session, logout | ✓ VERIFIED | `cargo tauri build --no-bundle` succeeds; commands present per prior review's grep checks; capability file narrowly scoped |
| `launcher/ui/{index.html,main.js,style.css}` | Full single screen, no bundler, CSP set | ✓ VERIFIED (code) | No Node/npm anywhere (`find launcher -name node_modules -o -name 'package*.json'` = 0); WR-01 CSP fix confirmed (`"csp": "default-src 'self'; img-src 'self'; style-src 'self'"`) |
| `launcher/core/tests/manifest_guard.rs` / `launch_command.rs` | Hostile-manifest and launch-line regression suites | ✓ VERIFIED | 12 + 14 tests, all passing, including the new case-varied never-touch tests |
| `scripts/publish-launcher.sh` | Operator publish command, atomic feed write, signing | ✓ VERIFIED | Feed live and well-formed at `/launcher/latest.json`; prior review/summary confirms idempotence and refusal behavior |
| `docs/LAUNCHER-BUILD.md` | Build recipe + QA matrix | ✓ VERIFIED | 277 lines, 8 `## ` sections, contains the exact toolchain pin, explicit no-Node/no-npm statement, Windows/Apple Silicon/Intel-Mac sections, headless harness reference, log locations, publish pointer, and the full 17-item QA matrix (harvested above into `human_verification`) |
| `auth-service/src/db.rs`/`api.rs` | `refresh_tokens` table + `/refresh`/`/logout` handlers | ✓ VERIFIED | Live-confirmed via curl through Caddy; `scripts/auth-smoke.sh` 52/52 |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `launcher/ui/main.js` | `launcher/src-tauri/src/lib.rs` | Tauri `invoke`/`Channel` bridge | ✓ WIRED | `grep -c '__TAURI__'`/`invoke` present per prior review; no `import` statements (no npm) |
| `launcher/src-tauri/src/lib.rs` | `launcher/core/src/manifest.rs` | `play`'s sync step, `verify_files` command | ✓ WIRED | Live-exercised: `play` orchestrates `sync()` for real, `verify()` was proven live in wave 2 |
| `launcher/core/src/mojang.rs` | `launcher/core/src/http.rs` | `public_client()` only, never `campfire_client()` | ✓ WIRED | grep-confirmed 0 / ≥1 split, structurally enforced |
| `launcher/core/src/java.rs` | `launcher/core/src/http.rs` | `public_client()` for Adoptium | ✓ WIRED | Live-exercised for all 3 targets |
| `caddy/Caddyfile` | `auth-service` | `/api/refresh`, `/api/logout`, `/launcher/*` routes | ✓ WIRED | Live curl through Caddy confirms 200/204/401 on refresh/logout; `/launcher/latest.json` live |
| `launcher/src-tauri/tauri.conf.json` | `launcher-dist/latest.json` | updater plugin endpoint | ✓ WIRED | Live feed fetch matches embedded pubkey |

### Behavioral Spot-Checks / Live E2E Runs

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace test suite | `cargo test --workspace` | 61 tests, 0 failed | ✓ PASS |
| Lint | `cargo clippy --workspace --all-targets` | Clean, no warnings | ✓ PASS |
| Release build | `cargo tauri build --no-bundle` | Exit 0, binary produced | ✓ PASS |
| Cold play sequence (live server) | `campfire-cli play --nick VerifierPhase4 --ram 6 --no-spawn` (with `CAMPFIRE_FORGE_JAVA` override — this Pi cannot execute a Windows `.exe`, a limitation of the test host, not the shipped Windows/macOS path) | 6/6 steps completed, full valid argv printed, 11.9s | ✓ PASS |
| Warm play sequence (idempotence) | same command, re-run | `downloaded=0 deleted=0 seeded=0 bytes=0`, 3.1s | ✓ PASS |
| Manifest sync twice | via `play`'s sync step | first: 3545 checked/downloaded; second: 0 downloaded | ✓ PASS |
| Java fetch × 3 targets | `campfire-cli java-fetch --target {windows-x64,mac-x64,mac-arm64}` | All checksum-verified; mac targets identical link | ✓ PASS |
| Update check × 3 states | `campfire-cli update-check` + live feed edits | no-update / update-available / malformed-silent, all as expected | ✓ PASS |
| Keyring self-test | `campfire-cli keyring-selftest` | PASS, linux keyutils backend | ✓ PASS |
| Pin-check | `campfire-cli pin-check` | Pinned client reaches campfire.pub; fails (cert error) against adoptium.net | ✓ PASS |
| Live `/api/refresh` via Caddy | curl `--cacert ca/campfire-ca.pem` | 200, rotates token | ✓ PASS |
| Live `/api/logout` via Caddy | curl `--cacert ca/campfire-ca.pem` | 204; subsequent refresh with same token → 401 | ✓ PASS |
| Auth smoke suite | `bash scripts/auth-smoke.sh` | 52/52 PASS | ✓ PASS |
| Secret leakage scan | grep for observed token/password value across `launcher.log` and the whole scratch install root | 0 hits in all cases | ✓ PASS |
| Service health throughout | `systemctl is-active rlcraft campfire-auth caddy` + `uptime -s` | All active, boot time unchanged, before/after every live check | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| AUTH-03 | 04-01, 04-04 | Remembered session via refresh token | ✓ SATISFIED | Live round trip, keyring, logout revocation |
| LNCH-01 | 04-01, 04-04 | Single screen, RAM slider, Play | ✓ SATISFIED (window render → human) | Code/build verified; visual QA deferred |
| LNCH-02 | 04-02 | Manifest diff/download/delete, never-touch | ✓ SATISFIED | Live cold/warm sync, never-touch fix confirmed |
| LNCH-03 | 04-02 | Java 8 auto-download, never system Java | ✓ SATISFIED | Live 3-target fetch, no JAVA_HOME/PATH references |
| LNCH-04 | 04-03 | Forge install, launch command | ✓ SATISFIED | Live headless install + full argv |
| LNCH-05 | 04-02, 04-04 | Progress reporting | ✓ SATISFIED (mechanism; render → human) | Channel-based, live events |
| LNCH-06 | 04-04 | Plain-English error messages | ✓ SATISFIED (2/5 live, 3/5 unit) | Live server_unreachable/session_expired |
| LNCH-07 | 04-01 | Server status pill | ✓ SATISFIED (fetch; render → human) | Live status match |
| LNCH-08 | 04-04 | Self-update check | ✓ SATISFIED (check/feed; dialog → human) | Live 3-state proof |

No orphaned requirements: all nine phase-mapped requirement IDs appear in at least one plan's `requirements` frontmatter field and are addressed above.

### Anti-Patterns Found

None found in the current code that are not already resolved. The 04-REVIEW.md code review (standard depth, 33 files) found 0 critical, 5 warnings, 3 info; all 5 warnings (CSP disabled, case-sensitive never-touch guard, non-atomic partial-write-before-rejection, logout not revoking server-side, unclamped NaN RAM) were fixed in 04-REVIEW-FIX.md and independently re-confirmed present in the current code by this verification (grep for `eq_ignore_ascii_case`, the CSP string, `is_finite()`, and the `/api/logout` route/handler, all above). The 3 info-tier items (unused `motd` field, an unreachable default branch in a JS switch, a Caddy linter note) are cosmetic/documented and out of the fix scope by the review's own framing.

### Human Verification Required

The 17-item Phase 4 operator QA matrix in `docs/LAUNCHER-BUILD.md` (harvested into this report's frontmatter `human_verification` list) plus the deferred Phase 1/2 checks. Everything on that list requires a real Windows x64 machine and a real macOS Apple Silicon machine with an actual display — none of which exist on this headless Pi. Every mechanism underlying those items (session refresh, sync, Java provisioning, Forge install, the launch command, progress events, error mapping, status fetch, and self-update) was independently proven live against the production server during this verification; only the visual rendering, the real game session, and Rosetta-hardware-specific behavior remain unverified.

### Gaps Summary

No gaps. All nine must-have truths (mapped 1:1 to the phase's requirement IDs and the ROADMAP's five numbered success criteria) are verified at the code and live-mechanism level, including two live end-to-end runs of the complete Play sequence against the production server and a live round trip through the newly-added `/api/refresh`/`/api/logout` public routes. Every code-review warning was fixed and the fix independently re-confirmed in the current source. The only remaining open items are the ones this phase's own plans explicitly scoped to a human with real Windows/macOS hardware and a display — collected in `docs/LAUNCHER-BUILD.md`'s QA matrix and reproduced above.

---

_Verified: 2026-08-30T15:45:00Z_
_Verifier: Claude (gsd-verifier)_
