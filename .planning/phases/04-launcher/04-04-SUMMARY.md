---
phase: 04-launcher
plan: 04
subsystem: launcher
tags: [tauri, rust, tokio, sysinfo, minisign, tauri-plugin-updater, tauri-plugin-opener, rlcraft-art]

# Dependency graph
requires:
  - phase: 04-launcher (04-01, 04-02, 04-03)
    provides: campfire-launcher-core workspace, auth/session tracer, manifest sync, Java provisioning, Mojang/Forge/launch line
provides:
  - "campfire_launcher_core::play::play() as one Play button: refresh session -> sync pack -> ensure Java -> ensure vanilla -> ensure Forge -> build+spawn, over an owned Arc<dyn Fn> ProgressSink, proven headlessly via campfire-cli play --no-spawn"
  - "The whole single window filled in: RAM slider (D-06 formula), Play button, progress bar/step label, error/info banners, Game folder / Verify files, version+pack footer, all wired via main.js to real Tauri commands"
  - "campfire_launcher_core::update::check() — the silent LNCH-08 startup check against /launcher/latest.json over the pinned CA, numeric semver comparison, any failure -> None"
  - "src-tauri check_update/install_update commands + the update-available dialog (index.html/main.js/style.css), the latter using tauri-plugin-updater's own Updater/Update (the only thing in this project that verifies the minisign signature)"
  - "A generated minisign keypair (public half embedded in tauri.conf.json, private half at ~/.tauri/campfire.key, pi-only custody per this plan's checkpoint) and scripts/publish-launcher.sh, the one operator command that signs artifacts and writes launcher-dist/latest.json atomically"
  - "launcher/ui/art/{background.png,logo.png} — RLCraft's own mainmenu art, bundled at build time, with the flat-color fallback proven by building with the files moved aside"
  - "docs/LAUNCHER-BUILD.md — build-from-source recipe for Windows x64 and Apple Silicon, the campfire-cli subcommand reference, log locations, and the full Phase 4 operator QA matrix"
affects: [05-release]

actuals:
  tokens: 37300
  tasks: 3
  commits: 4

tech-stack:
  added:
    - sysinfo 0.39 (D-06 RAM default, added in Task 1)
    - tauri-plugin-updater 2.10.1 (LNCH-08 — the only crate in this project that verifies a minisign signature)
  patterns:
    - "The silent-check / signed-install split: campfire_launcher_core::update owns only the plain version comparison (testable headlessly, no signature verification of its own); the actual download-and-install crosses through tauri-plugin-updater's own Updater/Update types in src-tauri, which re-fetches the feed once more via its own check() to get an Update handle capable of verifying the signature — two fetches of the same feed, deliberately, rather than one implementation doing both jobs"
    - "Signature caching keyed by the artifact's own sha256 (scripts/publish-launcher.sh): minisign embeds a signing timestamp, so re-signing identical bytes produces a different signature string every time — without the cache, re-publishing the same version+artifacts would silently churn the feed's signature field on every idempotent re-run"
    - "Platform resolved from every artifact's filename before any artifact is copied (scripts/publish-launcher.sh's resolve_all_platforms pass) — a whole-run refusal on one bad filename, never a partial publish"

key-files:
  created:
    - launcher/core/src/update.rs
    - launcher/ui/art/background.png
    - launcher/ui/art/logo.png
    - scripts/publish-launcher.sh
    - docs/LAUNCHER-BUILD.md
  modified:
    - launcher/core/src/lib.rs
    - launcher/core/src/strings.rs
    - launcher/core/src/bin/campfire-cli.rs
    - launcher/src-tauri/Cargo.toml
    - launcher/src-tauri/src/lib.rs
    - launcher/src-tauri/tauri.conf.json
    - launcher/ui/index.html
    - launcher/ui/main.js
    - launcher/ui/style.css
    - docs/DIST-OPS.md
    - server.env.example
    - server.env (untracked/gitignored — LAUNCHER_SIGNING_KEY_PASSWORD added)

key-decisions:
  - "Checkpoint decision: key custody is 'pi-only' — ~/.tauri/campfire.key (mode 600), generated with `cargo tauri signer generate`, deliberately NOT added to the existing backup set. Accepted consequence, recorded in server.env/server.env.example and docs/DIST-OPS.md: this host's system disk is an SSD (softening the plan's own disk-death framing versus a typical Pi SD card), but losing the key or its password anyway permanently ends self-update for every already-installed launcher (no password reset exists for minisign); recovery is a brand-new keypair + a new launcher build via Phase 5 + a manual reinstall ask to every friend, not a restore."
  - "The password lives in server.env's LAUNCHER_SIGNING_KEY_PASSWORD (gitignored, same file as RCON_PASSWORD), documented empty in server.env.example with the loss-consequence spelled out in the comment — the key file itself never enters git (it lives outside the repository tree entirely, so no .gitignore entry was needed for it)."
  - "scripts/publish-launcher.sh signs locally with the pi-only key rather than accepting a pre-signed artifact — the direct consequence of the checkpoint's chosen option; the plan's alternate 'operator-machine' signing path was not built since it wasn't chosen."
  - "Deviation (Rule 1 - bug, found during Task 2 verification): publish-launcher.sh's log() function wrote to stdout; sign_artifact() returns its signature via stdout capture, so a signature-cache-hit's log line was captured as part of the 'signature' value, corrupting the feed's JSON on a re-run. Fixed by sending log() to stderr — the standard split between diagnostic output and a function's actual return value."
  - "Deviation (Rule 2 - accessibility contract, found during Task 3): the status pill (built in wave 1, before any real art existed) had no background of its own and would sit directly on raw art pixels the moment a real image was behind it — violating UI-SPEC's 'Contrast over art' rule literally the instant this task's background-image was added. Gave it the same translucent panel tint the main panel uses."

requirements-completed: [AUTH-03, LNCH-01, LNCH-05, LNCH-06, LNCH-08]

coverage:
  - id: D1
    description: "Pressing Play runs the whole sequence (session refresh, pack sync, Java, Mojang, Forge, launch) with the window showing which step is running the whole time"
    requirement: LNCH-01
    verification:
      - kind: e2e
        ref: "campfire-cli play --nick <n> --ram 6 --no-spawn against the live production server: cold run 14.1s, all six steps in order; warm re-run 3.0s, zero bytes downloaded"
        status: pass
      - kind: other
        ref: "grep -c 'Channel' launcher/src-tauri/src/lib.rs >=1, grep -c 'emit(' launcher/src-tauri/src/lib.rs == 0 — progress is channel-driven, not the event bus"
        status: pass
    human_judgment: true
    rationale: "The orchestration and its progress events are proven end to end headlessly; whether the actual window renders the step name/count/rate live and legibly needs a real display, which this Pi does not have — deferred to the operator QA matrix (docs/LAUNCHER-BUILD.md item 2)."
  - id: D2
    description: "A RAM slider offers 3 to 10 GB in half-gigabyte steps, defaults to half of physical memory capped at 8, and warns without blocking above seventy percent"
    requirement: LNCH-01
    verification:
      - kind: unit
        ref: "system::tests — a 4GB machine recommends 3, a 32GB machine recommends 8, a 15GB machine recommends 7.5 exactly"
        status: pass
      - kind: other
        ref: "campfire-cli system-memory on this real 15.84GB host: total_gb=15.84 recommended_gb=7.5"
        status: pass
    human_judgment: true
    rationale: "The formula and its clamping are proven; the slider's visual warn-red state and that Play still works past 70% needs a real display and a real Play press — operator QA matrix item 5."
  - id: D3
    description: "Wrong password, unreachable server, failed Java setup, full disk and an expired session each show their own plain-English sentence with an Open log button, never a stack trace or raw status code"
    requirement: LNCH-06
    verification:
      - kind: e2e
        ref: "server_unreachable triggered for real via CAMPFIRE_BASE_URL_OVERRIDE pointed at a dead port: play exited non-zero printing only \"Can't reach campfire.pub. Check your internet connection.\", no Rust type name, no reqwest string, no HTTP status"
        status: pass
      - kind: e2e
        ref: "session_expired triggered for real via `campfire-auth reset` revoking the stored refresh token, then play: exited non-zero with \"Your session expired — log in again.\" and reopen_form=true"
        status: pass
      - kind: unit
        ref: "play::tests::every_named_error_category_maps_to_its_own_distinct_code — all five UI-SPEC codes plus generic are distinct non-empty strings"
        status: pass
    human_judgment: true
    rationale: "Two of the five sentences (server_unreachable, session_expired) were triggered end-to-end for real on this Pi; wrong_password/java_error/disk_full are proven only at the mapping-unit-test level (no wrong-password attempt against the live service was made to avoid the account rate limit, no real disk-full/Java-corruption condition was manufactured). The window's own rendering of any of the five, and the Open log button actually opening a file, needs a real display — operator QA matrix items 6, 9, 13."
  - id: D4
    description: "A failed step stops the progress bar rather than leaving it spinning, and the error names the log file the player can open with one click"
    verification:
      - kind: other
        ref: "main.js's handleProgress: a Failed event renders nothing further, leaving the bar where it stopped; the invoke() promise rejection (not the channel) owns the error banner + Open log button"
        status: pass
    human_judgment: true
    rationale: "Code-level inspection confirms the bar is never reset or advanced on failure; the actual frozen-bar-plus-Open-log behavior needs a real display to see rendered — operator QA matrix item 7."
  - id: D5
    description: "Game folder opens the install directory in the system file manager and Verify files re-hashes every managed file and reports how many were repaired"
    requirement: LNCH-01
    verification:
      - kind: e2e
        ref: "manifest::verify (wave 2): a tampered file was repaired and reported (checked=3545 repaired=1) — the same core function verify_files/Verify-files calls"
        status: pass
    human_judgment: true
    rationale: "The repair-counting mechanism was already proven live in wave 2; whether tauri-plugin-opener's reveal_item_in_dir/open_path actually open the real OS file manager/log viewer needs a real display — operator QA matrix items 9, 10, 13."
  - id: D6
    description: "The launcher checks for a newer version on startup and offers it; a failed check is silent"
    requirement: LNCH-08
    verification:
      - kind: e2e
        ref: "Three states proven live against the real file server: feed advertising the running version (0.1.0) -> \"no update available\"; feed advertising 9.9.9 -> \"update available: 9.9.9 (...)\"; feed replaced with malformed JSON -> \"no update available\", all exit 0"
        status: pass
      - kind: unit
        ref: "update::tests — 0.10.0 correctly newer than 0.9.0 (numeric, not lexical, comparison); malformed versions on either side are never \"newer\""
        status: pass
    human_judgment: true
    rationale: "The check/silence contract is fully proven headlessly in all three states; the actual startup dialog appearing in the window, and the download-and-restart round trip, need a real display and a real prior release — operator QA matrix item 11, explicitly deferred to Phase 5's real builds per this plan's own environment note."
  - id: D7
    description: "Update artifacts are served from the launcher route and are signed; the embedded public key is the public half of the operator's own key"
    requirement: LNCH-08
    verification:
      - kind: e2e
        ref: "curl --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/launcher/latest.json | jq — live, well-formed, both windows-x86_64 and darwin-aarch64 present with url+signature"
        status: pass
      - kind: other
        ref: "tauri.conf.json's plugins.updater.pubkey compared byte-for-byte (Python) against ~/.tauri/campfire.key.pub — identical"
        status: pass
      - kind: other
        ref: "git ls-files | grep -c campfire.key -> 0 (private key never in the repository, lives entirely outside the repo tree)"
        status: pass
    human_judgment: false
  - id: D8
    description: "The window renders RLCraft art behind a translucent panel; every piece of text sits on the panel, never raw artwork"
    requirement: LNCH-01
    verification:
      - kind: other
        ref: "cmp: launcher/ui/art/{background,logo}.png byte-identical to pack/resources/mainmenu/{background,rlcraft_logo_1_edit}.png; cargo tauri build --no-bundle succeeds both with the art present and with both files moved aside (fallback path)"
        status: pass
    human_judgment: true
    rationale: "The fallback mechanism and asset provenance are proven mechanically; whether the art actually renders behind the panel with legible contrast in a real window needs a real display — operator QA matrix item 12. Note: the status pill and the update dialog are DOM siblings of `.panel`, not literal descendants of it — each carries its own translucent/opaque panel-colored background (a Rule 2 fix this task made to the status pill specifically) so no text sits on raw art pixels in practice, even though the acceptance criterion's literal 'descendant of the panel element' wording is satisfied by intent rather than by DOM nesting."
  - id: D9
    description: "The launcher version and the pack version are both visible in the window; the error banner never grows past two lines"
    verification:
      - kind: other
        ref: "grep -c 'invoke' launcher/ui/main.js >= 6 (get_version/pack_version among them); .error-banner's max-height: calc(line-height * font-size * 2) with overflow hidden — a two-line cap by construction"
        status: pass
    human_judgment: true
    rationale: "Both are wired and CSS-constrained; the actual rendered footer text and truncation-in-practice need a real display — operator QA matrix items 12, 13."

duration: "~35 min (this continuation session, Tasks 2-3, checkpoint resolved 2026-08-30 pi-only) + Task 1 completed by a prior executor before the checkpoint (2026-08-30T11:51:44Z commit e364a18) — see 04-04-PLAN.md's checkpoint for the human decision that separates the two sessions"
completed: 2026-08-30
status: complete
---

# Phase 4 Plan 4: Press Play, Self-Update, and the RLCraft Skin Summary

**One Play button orchestrates the whole session-refresh -> sync -> Java -> Mojang -> Forge -> launch sequence over a Tauri channel; the launcher checks its own signed update feed (minisign, pi-only key custody) on startup and stays silent on failure; and the window now wears RLCraft's own mainmenu art with a proven flat-color fallback.**

## Performance

- **Duration:** see frontmatter `duration` — this continuation (Tasks 2-3) plus Task 1 from the prior, pre-checkpoint session
- **Started (this continuation):** 2026-08-30 (after the checkpoint's `pi-only` decision)
- **Completed:** 2026-08-30T14:54:35Z (UTC)
- **Tasks:** 3 (Task 1 pre-checkpoint, Tasks 2-3 this continuation)
- **Files modified:** 20 across all three tasks (5 created, 15 modified — see `key-files`)

## Accomplishments

- **The Play button works, headlessly proven end to end.** `campfire_launcher_core::play::play()` runs refresh -> sync -> Java -> Mojang -> Forge -> build+spawn as one function, reporting through an owned `Arc<dyn Fn>` `ProgressSink` (both the Tauri `play` command and `campfire-cli play --no-spawn` share this one implementation — no event-bus duplication). Live against the production server: **cold run 14.1s** (all six steps, in order); **warm re-run 3.0s, zero bytes downloaded** — the idempotence the ROADMAP's own success criteria promise.
- **Every error crosses the boundary as a sentence, never internals.** Two of the five UI-SPEC error sentences were triggered for real on this Pi — `server_unreachable` (a dead-port override) and `session_expired` (`campfire-auth reset` revoking the stored refresh token) — both printing only the mapped plain-English sentence, no Rust type name, no `reqwest` string, no HTTP status. The other three (`wrong_password`, `java_error`, `disk_full`) are proven at the mapping-unit-test level only (see coverage `D3`).
- **The RAM slider's default is the real formula on real hardware.** `system::recommended_ram_gb` gives 3/8/7.5 for 4GB/32GB/15GB machines (unit-tested); this actual 15.84GB Pi reports `recommended_gb=7.5`, exactly matching the formula.
- **Self-update is real and proven in all three states against the live production file server**, not a mock: a feed advertising the running version (`0.1.0`) reports no update; a feed advertising `9.9.9` reports it; a feed replaced with malformed JSON reports nothing available — all three exit `0`, silent-on-failure as the contract requires. A minisign keypair was generated once (`cargo tauri signer generate`); the embedded public key in `tauri.conf.json` was compared byte-for-byte against the generated `.pub` file and is identical.
- **`scripts/publish-launcher.sh` is the one operator command**: disk-space floor, filename-based platform detection that refuses the whole run before copying anything if any artifact can't be named, minisign signing with a sha256-keyed cache (so re-publishing the same artifacts twice produces a feed byte-identical apart from `pub_date`, not a churned signature), an atomic feed write, and world-readable permissions. Proved with two placeholder artifacts (explicitly *not* real Windows/macOS builds — this aarch64 Pi cannot produce those; Phase 5's CI does) signed with the real operator key and served live.
- **The window wears RLCraft's own art.** `background.png`/`logo.png` copied byte-identical from `pack/resources/mainmenu/`, no resize step. The flat-color fallback was proven by moving both files aside and confirming `cargo tauri build --no-bundle` still succeeds — the contract holds either way, exactly as D-04 requires.
- **`docs/LAUNCHER-BUILD.md`** gives the operator an exact, Node-free build recipe for both real machines, the full `campfire-cli` subcommand reference, log locations, and the complete 17-item Phase 4 QA matrix — including the Phase 1/2 checks that have been waiting for a real client since those phases closed.

## Task Commits

1. **Task 1: Press Play — the whole sequence, on a channel, with every failure spoken plainly** - `e364a18` (feat) — completed by the prior, pre-checkpoint session
2. **Checkpoint:decision — key custody** - resolved by the user: `pi-only` (`~/.tauri/campfire.key`, not backed up; system disk is an SSD, softening the disk-death framing; accepted consequence recorded above)
3. **Task 2: The launcher can replace itself — key, feed, publish command, startup check** - `e5423e0` (feat)
4. **Task 3: The RLCraft skin, the build instructions, and the operator's QA matrix** - `751b4ba` (feat)

**Plan metadata:** (this commit, docs: complete plan — SUMMARY.md and the WINDOWS.md ledger entry only; STATE.md/ROADMAP.md are intentionally untouched per this continuation's instructions)

_Note: per this continuation's instructions, no separate STATE.md/ROADMAP.md-updating commit was made._

## Files Created/Modified

- `launcher/core/src/update.rs` - the silent LNCH-08 startup check, numeric semver comparison
- `launcher/core/src/{lib,strings,bin/campfire-cli}.rs` - `update` module registered, `update-check` subcommand, update-dialog copy strings
- `launcher/src-tauri/{Cargo.toml,src/lib.rs,tauri.conf.json}` - `tauri-plugin-updater` dependency + plugin init, `check_update`/`install_update` commands, updater endpoint + embedded pubkey
- `launcher/ui/{index.html,main.js,style.css}` - the update-available dialog, the status-pill panel-tint fix, the RLCraft background/logo wiring
- `launcher/ui/art/{background.png,logo.png}` - RLCraft's own mainmenu art, bundled verbatim
- `scripts/publish-launcher.sh` - the operator publish command
- `docs/LAUNCHER-BUILD.md` - build recipe + QA matrix
- `docs/DIST-OPS.md` - the self-update feed section + cross-link
- `server.env` / `server.env.example` - `LAUNCHER_SIGNING_KEY_PASSWORD`

## Decisions Made

See frontmatter `key-decisions` for full detail. Summary:
- Checkpoint: key custody is **pi-only** (`~/.tauri/campfire.key`, not backed up). Accepted consequence recorded in `server.env.example` and `docs/DIST-OPS.md`.
- The signing password lives in `server.env` (gitignored); the key file itself lives entirely outside the repository tree, so no `.gitignore` entry was needed for it.
- `publish-launcher.sh` signs locally rather than accepting a pre-signed artifact, per the checkpoint's chosen option.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `publish-launcher.sh`'s log() polluted a captured return value**
- **Found during:** Task 2, first idempotency test (publishing the same version twice)
- **Issue:** `log()` wrote to stdout; `sign_artifact()` returns its signature via a stdout capture (`sig="$(sign_artifact ...)"`). On a signature-cache hit, `sign_artifact`'s own log line ("Signature cache hit for ...") was captured as part of the returned signature string, corrupting the `signature` field in the written feed with a JSON-breaking log line embedded inside it.
- **Fix:** Changed `log()` to write to stderr (`>&2`) — the standard split between a script's diagnostic output and an actual function return value.
- **Files modified:** `scripts/publish-launcher.sh`
- **Verification:** Re-ran the idempotency test: `diff <(jq 'del(.pub_date)' before) <(jq 'del(.pub_date)' after)` — empty, feed byte-identical apart from `pub_date`.
- **Committed in:** `e5423e0` (Task 2 commit)

**2. [Rule 2 - Missing Critical] The status pill had no background of its own, violating "Contrast over art" the moment real art existed**
- **Found during:** Task 3, while wiring the real background image behind the window
- **Issue:** Wave 1's status pill (`position: absolute`, top-left corner) was authored against the flat fallback color and had no background of its own — legible then, but UI-SPEC's "Contrast over art" rule ("all text sits on the ... panel, never directly on raw art pixels") would be violated the instant a real, possibly busy image sat behind it, which this task's `background-image` now does.
- **Fix:** Gave `.status-pill` the same translucent panel tint (`color-mix` formula) the main `.panel` element already uses.
- **Files modified:** `launcher/ui/style.css`
- **Verification:** Code inspection — the pill's background now matches `.panel`'s own formula exactly; visual confirmation is part of the operator QA matrix (item 12), since this Pi cannot render the window.
- **Committed in:** `751b4ba` (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 accessibility-contract gap this task's own change exposed).
**Impact on plan:** Both were necessary for correctness (a corrupted feed) and for the accessibility contract this task's own art change was about to violate. No scope creep.

## Issues Encountered

- **The literal "descendant of the panel element" acceptance-criterion wording** (Task 3) is satisfied by intent (no text sits on raw art pixels — every text-bearing element has its own panel-tinted background: the main panel, the status pill after this task's fix, and the update dialog's own solid panel-colored background) rather than by strict DOM nesting (the status pill and update dialog are siblings of `<main class="panel">`, not children). Recorded honestly in coverage `D8`'s rationale rather than restructuring the DOM to force a literal match, since the status pill's fixed top-left window-corner position and the update dialog's full-window overlay both depend on *not* being nested inside the panel's own margin/padding box.
- **The five error sentences were not all triggered end-to-end for real** — `server_unreachable` and `session_expired` were (Task 1); `wrong_password` was not attempted against the live account (avoiding the auth service's rate limit), and `disk_full`/`java_error` were not manufactured for real (no real disk-full or Java-corruption condition exists to trigger on this host). All five are proven distinct and non-empty at the unit-test level. See coverage `D3`.
- **The actual placeholder artifacts published to the live feed are not real Windows/macOS builds** — this aarch64 Linux Pi cannot cross-compile a genuine Tauri NSIS installer or macOS `.app` bundle. Two small placeholder files (clearly labeled as such inside their own content) were signed with the real operator key and published to prove the sign→publish→serve→check pipeline end to end; Phase 5's CI is what produces the genuine artifacts this same pipeline will then publish unchanged.

## User Setup Required

None — no external service configuration required beyond what this plan itself performed (the key generation and the `server.env`/`server.env.example` entries it made).

## Known Stubs

None. Every function this plan built is a complete, real implementation — the two placeholder *artifacts* published to the test feed are not stubs in the code sense (nothing in the launcher itself is mocked or hardcoded-empty); they are deliberately-labeled substitutes for build outputs this host cannot produce, exactly as `04-RESEARCH.md`'s "Self-Update" section anticipated ("this phase should stand up the updater plugin wiring and the endpoint contract, with the operator manually placing a latest.json + local build artifact for testing").

## Pending Human Verification

**The entire Phase 4 operator QA matrix in `docs/LAUNCHER-BUILD.md` (17 items across Windows x64 and Apple Silicon) has not been performed** — this Pi has no display, and every genuinely visual/interactive/gameplay claim in this plan's `must_haves.truths` is marked `human_judgment: true` in the coverage block above for exactly that reason. This is the expected, planned state per this plan's own environment note ("This host cannot run the game or open a window"), not a gap introduced by this execution. Recorded in `.planning/WINDOWS.md` as an `unrun-verify` entry so it stays visible at ship time.

Also still pending from `04-01-SUMMARY.md`: the smaller Task-3 human-check from wave 1 (building and clicking through the auth-only window) — superseded in scope by this plan's full matrix, but not yet separately closed out.

## Self-Update Feed — Live Verification Detail

Recorded here per this plan's own `<output>` requirement:

- **Cold headless Play** (production server): 14.1s, all six steps in order (session refresh, pack sync, Java, Mojang, Forge, launch-command build).
- **Warm re-run**: 3.0s, zero bytes downloaded — full idempotence.
- **RAM recommendation on this 15.84GB host**: `7.5` GB, matching the formula exactly.
- **Update-check, three states, against the real file server:**
  1. Feed advertising `0.1.0` (== running version) -> `no update available`
  2. Feed advertising `9.9.9` -> `update available: 9.9.9 (test-higher-version)`
  3. Feed body replaced with malformed JSON -> `no update available` (silent by contract)
  4. Feed restored to `0.1.0` (the built version) as the final, settled state
- **Key custody**: `pi-only` — private key at `~/.tauri/campfire.key` (mode 600), password in `server.env`'s `LAUNCHER_SIGNING_KEY_PASSWORD`. Public key embedded in `tauri.conf.json`, verified byte-identical to the generated `.pub` file.
- **Launcher binary size with art bundled**: `campfire-launcher` 25,754,208 bytes (~24.6 MiB, up from wave 1's 15MB — the ~3.7MB art plus the updater plugin); `campfire-cli` 8,218,256 bytes (~7.8 MiB).

### `docs/LAUNCHER-BUILD.md` command sequences (both machines, condensed — full text in that file)

**Prerequisites (both):**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
cargo install tauri-cli --version "^2" --locked
```

**Windows x64:**
```bash
cd launcher
cargo test --workspace
cargo tauri build --no-bundle   # or `cargo tauri build` for the real installer
```

**macOS Apple Silicon** (same two commands as Windows, plus the one-time Rosetta step for the game's x86_64 Java 8, not part of the build):
```bash
softwareupdate --install-rosetta --agree-to-license
```

**macOS Intel:** identical commands to Apple Silicon; no real hardware exists to run them on — verified by reasoning and by Phase 5's CI only.

### The full operator QA matrix

Reproduced in full in `docs/LAUNCHER-BUILD.md`'s "The Phase 4 operator QA matrix" section (17 numbered items: 13 on Windows x64, 4 on macOS Apple Silicon, plus the Phase 1/2 deferred checks) — not duplicated here a second time to avoid the two copies drifting; the verifier should harvest directly from that file, which is the canonical, operator-facing copy.

## Next Phase Readiness

- Every artifact Phase 5 inherits is in place: crate names, build commands, the updater's artifact-emission config, the signing key's location and custody decision, and the feed schema/hosting — all listed in `04-04-PLAN.md`'s "What Phase 5 inherits, in one place" table, unchanged by this execution.
- **Blocker for full Phase 4 sign-off**: the operator QA matrix above must be run on real Windows x64 and Apple Silicon hardware before this phase's ROADMAP success criteria can be marked demonstrated rather than mechanically proven.
- `rlcraft.service`, `campfire-auth.service`, and `caddy.service` were never restarted or reconfigured across either task in this continuation; `uptime -s` (`2026-08-22 20:53:29`) was identical before and after.

---
*Phase: 04-launcher*
*Completed: 2026-08-30*

## Self-Check: PASSED

All 5 files claimed as created were confirmed present on disk (`launcher/core/src/update.rs`, `launcher/ui/art/background.png`, `launcher/ui/art/logo.png`, `scripts/publish-launcher.sh`, `docs/LAUNCHER-BUILD.md`), and all 3 task commit hashes (`e364a18`, `e5423e0`, `751b4ba`) were confirmed present in `git log --oneline --all`.
