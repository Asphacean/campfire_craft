---
phase: 04-launcher
plan: 03
subsystem: launcher
tags: [rust, mojang, forge, minecraft-1.12.2, sha1, md5, offline-uuid, headless-install]

# Dependency graph
requires:
  - phase: 04-launcher (04-01, 04-02)
    provides: workspace layout, http.rs (public_client/campfire_client), paths.rs, auth.rs (Session), java.rs (ensure_java, Target, assert_safe_archive_entry), manifest.rs (seed-once pattern), progress.rs, log.rs
provides:
  - "mojang.rs: the vanilla bootstrap (version manifest -> version JSON -> client jar -> rule-filtered libraries/natives -> asset index/objects), SHA-1 verified, public_client() only"
  - "forge.rs: headless Forge 1.12.2 install (profile stub, sha256-pinned installer, empty-URL library special case) and the vanilla-parent merge"
  - "launch.rs: offline UUID, natives extraction, classpath builder, the full java argv, seeded servers.dat, token-redacted logging, spawn()"
  - "campfire-cli vanilla / forge / launch-cmd / launch subcommands — the headless proof harness for all of the above"
affects: [04-04 (Play flow UI), phase-05 (packaging/CI, real Windows/macOS launch verification)]

actuals:
  tokens: 19365
  tasks: 3
  commits: 3

tech-stack:
  added: [sha1_smol (Mojang's own hash domain), md-5 (offline-UUID digest, RustCrypto)]
  patterns:
    - "Rule-engine-once: mojang::rule_allows/current_os_name/resolve_native_classifier are pub(crate) and reused verbatim by launch.rs's classpath/natives builder — one platform-filter evaluation, not two"
    - "Shared archive-entry path guard: java::assert_safe_archive_entry (made pub(crate)) reused by launch.rs's natives extraction instead of a second copy"
    - "Success is a produced-artifact check, never an exit code (Forge installer observed exiting 0 on its own error path)"
    - "Test-only env overrides for real-target-only dependencies (CAMPFIRE_FORGE_JAVA), mirroring the existing CAMPFIRE_HOME/CAMPFIRE_JAVA_FORCE_CHECKSUM_MISMATCH convention"

key-files:
  created:
    - launcher/core/src/mojang.rs
    - launcher/core/src/forge.rs
    - launcher/core/src/launch.rs
    - launcher/core/tests/launch_command.rs
    - launcher/core/assets/servers.dat
  modified:
    - launcher/core/src/lib.rs
    - launcher/core/src/bin/campfire-cli.rs
    - launcher/core/src/java.rs (assert_safe_archive_entry made pub(crate))
    - launcher/core/Cargo.toml (sha1_smol, md-5)

key-decisions:
  - "mojang::VersionJson's downloads/assetIndex fields made Option — Forge's produced version JSON has neither, relying entirely on inheritsFrom for both (discovered live, not anticipated by research's JSON excerpt)"
  - "The installer's target directory is install_root() (matching paths.rs's versions_dir()/libraries_dir() layout), with the launcher_profiles.json stub written defensively to both install_root() and game_dir()"
  - "campfire-cli launch-cmd/launch default to Target::WindowsX64 when detect_target() has no entry for the running host (this Pi), to exercise the identical production ensure_java() path a real Windows machine would take"

patterns-established:
  - "Download-verify-rename with pre-existing-hash skip (mojang.rs's own copy, sha1 not sha256) — same shape as manifest.rs's sha256 version, kept as a second, clearly-named implementation rather than a shared generic to avoid conflating the two hash domains"

requirements-completed: [LNCH-04]

coverage:
  - id: D1
    description: "Vanilla bootstrap: client jar, rule-filtered libraries/natives, full asset tree, all SHA-1 verified through public_client() only"
    requirement: LNCH-04
    verification:
      - kind: integration
        ref: "campfire-cli vanilla against a fresh scratch CAMPFIRE_HOME — 34 libs included / 5 excluded, 3 natives, 1305 asset objects, independent sha1sum cross-check of the client jar and a 10-file asset spot-check"
        status: pass
      - kind: unit
        ref: "mojang::tests::every_constant_url_is_a_mojang_or_minecraft_host, rule_engine_* (3 tests), native_classifier_substitutes_arch_placeholder"
        status: pass
    human_judgment: false
  - id: D2
    description: "Headless Forge install via the official installer, profile-stub prerequisite, empty-URL library checked not fetched, vanilla-parent merge"
    requirement: LNCH-04
    verification:
      - kind: integration
        ref: "campfire-cli forge with DISPLAY unset — produced JSON shape asserted via jq, idempotent second-run skip, delete-both-and-reinstall proof, forge jar sha1 cross-check"
        status: pass
      - kind: unit
        ref: "forge::tests::merge_puts_child_libraries_first_and_dedupes_by_name, merge_fails_loudly_when_the_child_has_no_minecraft_arguments"
        status: pass
    human_judgment: false
  - id: D3
    description: "Complete launch command line: offline UUID, natives, classpath, token handoff, seeded server list, redacted logging"
    requirement: LNCH-04
    verification:
      - kind: unit
        ref: "core/tests/launch_command.rs (12 tests): UUID fixed-vector + casing-differs, system properties, -Xmx, main/tweak class, classpath existence + rejection, java-outside-runtime rejection, placeholder completeness, token redaction, autoconnect toggle, seed-once servers.dat"
        status: pass
      - kind: integration
        ref: "campfire-cli launch-cmd --nick TestNick --ram 6 on the bootstrapped scratch dir, plus a real register/login round trip proving the real token never reaches launcher.log"
        status: pass
    human_judgment: true
    rationale: "The mechanical half (LNCH-04) is fully proven on this Pi; the human-check in the plan's own <verify> block — the game actually starting and landing in the world on a Windows/Apple Silicon machine — cannot be exercised on this Linux host at all and is explicitly deferred to the operator per the plan's checkpoint protocol."

duration: 55min
completed: 2026-08-28
status: complete
---

# Phase 4 Plan 3: Vanilla bootstrap, headless Forge install, and the launch line Summary

**Minecraft's own files from Minecraft's own hosts (SHA-1 verified), Forge 1.12.2 installed headlessly via its official installer with the undocumented profile-stub prerequisite, and a complete, classpath-checked `java` command line with the token handoff and offline UUID — all proven live on this Pi.**

## Performance

- **Duration:** ~55 min
- **Started:** 2026-08-28 (session start, reading required context)
- **Completed:** 2026-08-28T19:09:23Z
- **Tasks:** 3/3
- **Files modified:** 8 (5 created, 3 modified across all three tasks combined; see per-commit breakdown below)

## Accomplishments

- `mojang.rs` fetches the version manifest, the pinned `1.12.2` version JSON, the client jar, every rule-filtered library/native, and the complete `1.12` asset index/objects — all through `public_client()`, with a unit test and four grep gates proving it has no way to reach `campfire.pub` at all.
- `forge.rs` shells out once to the official, sha256-pinned Forge installer with the `launcher_profiles.json` stub research found it silently needs, treats success as "the produced version JSON exists and parses" (not the exit code — the installer was observed exiting 0 on its own error path), and merges the result with the cached vanilla parent.
- `launch.rs` builds the complete `java` argv as an assertable `Vec<String>`: offline UUID, extracted natives, a classpath whose every entry is checked to exist, the two token-handoff system properties, and the removable autoconnect pair — with the token redacted everywhere it is logged.
- All three verified live and headless on this Pi (no display), against a fresh scratch `CAMPFIRE_HOME`, with `rlcraft.service` and the repository's `server`/`pack` trees untouched throughout.

## Task Commits

1. **Task 1: Minecraft's own files, from Minecraft's own servers, hash-checked** - `3da941e` (feat)
2. **Task 2: Forge installed headlessly, exactly the way research proved it works** - `305099f` (feat)
3. **Task 3: The launch line — classpath, natives, token handoff, and a server list to land on** - `1928d34` (feat)

_No separate plan-metadata commit for this SUMMARY per the orchestrator's instruction — this plan does not touch STATE.md/ROADMAP.md._

## Files Created/Modified

- `launcher/core/src/mojang.rs` - version manifest → version JSON → client jar → libraries/natives (rule-filtered) → asset index/objects, all SHA-1 verified through `public_client()`
- `launcher/core/src/forge.rs` - sha256-pinned installer download, profile-stub write, headless invocation, produced-JSON check, vanilla-parent merge, empty-URL library special case
- `launcher/core/src/launch.rs` - offline UUID, natives extraction (reusing `java::assert_safe_archive_entry`), classpath builder, JVM flag constant, `build_launch_command`, `seed_server_list`, `spawn`
- `launcher/core/tests/launch_command.rs` - 12-test regression suite for the launch line
- `launcher/core/assets/servers.dat` - 73-byte precomputed gzip'd NBT blob (one entry: `campfire.pub` / `mc.campfire.pub`)
- `launcher/core/src/lib.rs` - registers `mojang`, `forge`, `launch`
- `launcher/core/src/bin/campfire-cli.rs` - `vanilla`, `forge`, `launch-cmd`, `launch` subcommands
- `launcher/core/src/java.rs` - `assert_safe_archive_entry` made `pub(crate)` for reuse by `launch.rs`
- `launcher/core/Cargo.toml` - `sha1_smol` (task 1), `md-5` (task 3)

## Decisions Made

- **`VersionJson`'s `downloads`/`assetIndex` made `Option`** — discovered live during task 2: Forge's produced version JSON carries neither field, leaning entirely on `inheritsFrom` for both. The vanilla parent (task 1) always has them; `forge.rs`'s `merge()` reads them only from the parent.
- **Installer target directory is `install_root()`**, matching `paths.rs`'s existing `versions_dir()`/`libraries_dir()` layout (siblings of `game/`, not nested under it) — this is what makes the installer's own `versions/`/`libraries/` output land exactly where task 1's `mojang.rs` already put the vanilla files. The `launcher_profiles.json` stub is written to **both** `install_root()` (the installer's actual requirement) and `game_dir()` (defensive — Forge's client runtime has historically also looked for this file in the `--gameDir` it's launched with).
- **`campfire-cli launch-cmd`/`launch` default to `Target::WindowsX64`** when `java::detect_target()` fails (this Pi has no shipped Linux target) — this exercises the identical `ensure_java()` production code path a real Windows machine's own `detect_target()` would take, rather than adding a Linux-specific bypass.
- **`CAMPFIRE_FORGE_JAVA` test-only env override** added to `forge.rs`, mirroring the existing `CAMPFIRE_HOME`/`CAMPFIRE_JAVA_FORCE_CHECKSUM_MISMATCH` convention — the Pi has no Windows/macOS Java capable of actually executing, so the integration proof here borrows the Phase 1 Temurin 8 read-only; unset, the code always calls `java::ensure_java(java::detect_target()?)`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `VersionJson.downloads`/`asset_index` must be `Option`, not required**
- **Found during:** Task 2, first live `campfire-cli forge` run
- **Issue:** Forge's produced `1.12.2-forge-14.23.5.2860.json` has no `downloads` or `assetIndex` field at all (confirmed by inspecting the real file on disk); the struct from task 1 required both, so `try_load_forge_json` silently returned `None` and the whole install was reported as failed even though the installer had actually succeeded
- **Fix:** Made both fields `Option<...>` in `mojang::VersionJson`; `ensure_vanilla` and `forge::merge` now read them with an explicit "vanilla parent must have this" error instead of a struct-level requirement
- **Files modified:** `launcher/core/src/mojang.rs`, `launcher/core/src/forge.rs`
- **Verification:** `campfire-cli forge` now parses the real produced JSON, merges 58 libraries, and the acceptance JSON-shape/tweak-class/idempotence checks all pass
- **Committed in:** `305099f`

**2. [Rule 1 - Bug] DIST-03/anti-pattern grep gates tripped by the module's own doc comments**
- **Found during:** Task 1 and Task 2 acceptance verification
- **Issue:** `mojang.rs`'s module doc explicitly named `campfire_client`/`campfire.pub` (to describe that they're *not* used), and `forge.rs`'s doc named "processor/binpatch" (to describe that neither is reimplemented) — both tripped the literal `grep -c` gates meant to catch actual references
- **Fix:** Reworded both comments to convey the same guarantee without the literal forbidden substrings
- **Files modified:** `launcher/core/src/mojang.rs`, `launcher/core/src/forge.rs`
- **Verification:** All five DIST-03 grep gates (`campfire_client`→0, `public_client`→≥1, `campfire.pub`→0, `sha1`→≥1, `Sha256`→0) and the anti-pattern gate (`binpatch\|processor`→0) now pass exactly as specified
- **Committed in:** `3da941e`, `305099f`

---

**Total deviations:** 2 auto-fixed (both Rule 1 — bugs discovered during this plan's own live verification, not scope creep).
**Impact on plan:** No architectural change; both fixes were required for the acceptance criteria to hold against real Mojang/Forge data rather than the plan's illustrative excerpts.

## Issues Encountered

- **Asset object count is 1305 (1290 unique content hashes), not "roughly 3,700" as research estimated.** Verified independently via a direct `curl` of Mojang's own `1.12` asset index — this is the real, current count for this exact Minecraft version, not a bug in the sync code. The 15-entry gap between `jq '.objects|length'` (1305) and `find assets/objects -type f | wc -l` (1290) is Mojang's own content-addressed dedup (multiple named assets sharing one hash) — inherent to the storage scheme, not something any correct implementation can avoid.
- **`try_load_forge_json`'s silent `.ok()` initially masked the real cause** of the first `campfire-cli forge` failure (a successful install reported as "did not appear") — diagnosed by reading the produced JSON directly with Python rather than trusting the Rust error message, which named the wrong symptom (missing file) instead of the real one (unparseable struct).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Wave 4 (04-04, the Play flow UI) can call one function per step exactly as the plan promised: `manifest::sync`, `java::ensure_java`, `mojang::ensure_vanilla`, `forge::ensure_forge`, `launch::build_launch_command`, `launch::spawn` — nothing left to invent.
- **Blocker for full LNCH-04 sign-off:** the human-check in this plan's own `<verify>` block — the game actually starting and landing in the world with the right nick/inventory persistence, and the auth mod's property read being exercised by a real client for the first time — requires the operator's Windows or Apple Silicon machine, which is unavailable on this Pi. That check is deferred to the operator per the plan's checkpoint protocol (autonomous execution, no interactive checkpoint reached because no blocker occurred on this host).
- A test account (`LnchTest<random>`) was registered against the live `campfire-auth` service during token-redaction verification; it is a harmless, real but throwaway account on this private low-volume server and was not cleaned up (no delete-account path exists).

## Self-Check: PASSED

All created files confirmed present on disk (`launcher/core/src/{mojang,forge,launch}.rs`, `launcher/core/tests/launch_command.rs`, `launcher/core/assets/servers.dat`, this SUMMARY). All three task commit hashes (`3da941e`, `305099f`, `1928d34`) confirmed present in `git log`.
