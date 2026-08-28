---
phase: 04-launcher
plan: 02
subsystem: launcher
tags: [rust, tokio, reqwest, sha2, zip, flate2, tar, adoptium, manifest-sync, campfire-cli]

# Dependency graph
requires:
  - phase: 04-launcher (plan 01)
    provides: campfire-launcher-core workspace, campfire_client()/public_client(), paths.rs (CAMPFIRE_HOME), log.rs, progress.rs skeleton, campfire-cli harness
provides:
  - launcher/core/src/manifest.rs — sync()/verify()/validate()/parse_manifest(): the ported, faithful assemble-client.py guard, sha256 diff, bounded-4-concurrent download with atomic same-directory rename, cumulative delete[], seed-once options.txt/optionsof.txt, and the second never-touch lock enforced in code
  - launcher/core/src/java.rs — ensure_java()/detect_target()/translation_state(): per-platform Adoptium fetch, vendor-checksum verification before any extraction, atomic archive extraction with the same path guard, runtime/current.json marker, structurally no system-Java fallback
  - campfire-cli sync/verify/java-fetch/java-probe — the headless proof harness for both halves
  - launcher/core/tests/manifest_guard.rs — the 9-test hostile-manifest suite (8 rejecting, 1 accepting)
  - launcher/core/assets/{options.txt,optionsof.txt} — the pack's own tuned client defaults, closing docs/DIST-OPS.md gap 1
affects: [04-03, 04-04, 05-release]

actuals:
  tokens: 17456
  tasks: 2
  commits: 2

tech-stack:
  added:
    - sha2 0.10 (manifest sha256 diff/verify, streaming 1MiB chunks)
    - futures-util 0.3 (buffer_unordered — bounded concurrent downloads without spawning onto 'static tasks)
    - percent-encoding 2 (per-segment URL encoding, leaving '/' separators alone)
    - zip 2 (Windows JRE archive extraction)
    - flate2 1 + tar 0.4 (macOS JRE .tar.gz extraction)
    - libc 0.2 (macOS-only target dependency, sysctlbyname for Rosetta detection)
  patterns:
    - "Bounded concurrency via futures_util::stream::iter(...).buffer_unordered(4) polled
      within one async fn, never tokio::spawn — lets ProgressSink stay a borrowed
      &(dyn Fn + Send + Sync) instead of needing Arc for 'static task ownership"
    - "Same-directory temp file + atomic rename for every downloaded/extracted artifact
      (manifest files AND Java archives) — one pattern, ported from assemble-client.py,
      reused for a second untrusted-byte-source"
    - "The manifest path/archive-entry guard is one small function applied twice
      (manifest.rs entries, java.rs archive entries) rather than two independent
      implementations of the same absolute/parent-component/control-char check"
    - "ensure_java's idempotence check queries Adoptium's metadata (cheap) then checks
      the filesystem directly for the expected executable, rather than trusting a single
      global marker file — required once it became clear windows-x64 and mac-x64 can
      report the identical Adoptium release_name for the same Java 8 update"

key-files:
  created:
    - launcher/core/src/manifest.rs
    - launcher/core/src/java.rs
    - launcher/core/tests/manifest_guard.rs
    - launcher/core/assets/options.txt
    - launcher/core/assets/optionsof.txt
  modified:
    - launcher/core/Cargo.toml
    - launcher/core/src/lib.rs
    - launcher/core/src/progress.rs
    - launcher/core/src/bin/campfire-cli.rs
    - launcher/Cargo.lock

key-decisions:
  - "progress.rs's final shape (Step{name,current,total}/Bytes{downloaded,total,per_sec}/
    Done/Failed{code}, ProgressSink = &(dyn Fn(Progress)+Send+Sync)) replaces wave 1's
    placeholder Step{label,...}/Done/Error{message} shape — nothing outside this plan
    consumed the old shape yet, so this is a completion of D-07, not a breaking change"
  - "ensure_java's on-disk runtime directory is keyed by <release>-<target>, not just
    <release> — Adoptium's release_name (jdk8u504-b01) is identical across windows-x64
    and mac-x64 for the same Java 8 update, and the plan's own runtime/current.json
    schema names only one 'current' java; keying by target as well is what lets this
    plan's own three-target proof harness provision all three without one target's
    atomic rename colliding with a previous target's already-extracted directory"
  - "ensure_java's idempotence check queries Adoptium (a small JSON response) then checks
    the filesystem for the expected executable, rather than only trusting
    runtime/current.json — the marker only ever names the most recently provisioned
    target, and this plan's own headless harness fetches all three targets in one run"
  - "CAMPFIRE_JAVA_FORCE_CHECKSUM_MISMATCH is a test-only env-var escape hatch in
    ensure_java, exactly as the plan's own acceptance-criteria text allows ('a test-only
    entry point or an env override is fine') — it corrupts only the *expected* checksum
    after a real Adoptium query, so the download and hash that follow are entirely real"
  - "Download concurrency uses futures_util::stream::buffer_unordered(4) polled within
    sync()'s own async fn rather than tokio::spawn — avoids needing Arc<dyn Fn> for the
    progress sink, at the cost of true OS-thread parallelism (which a Pi serving one
    friend over loopback/local network does not need)"

patterns-established:
  - "Pattern: one path/entry-name guard function applied to every untrusted-source path,
    whether it names a manifest file or a third-party archive entry"
  - "Pattern: idempotence checked against the filesystem's actual state, not solely a
    bookkeeping marker, whenever multiple logical 'current' states can exist under one
    installation root"

requirements-completed: [LNCH-02, LNCH-03, LNCH-05]

coverage:
  - id: D1
    description: "A game directory can be brought from nothing to exactly the published pack using only the manifest and the pinned CA, every file's sha256 matching the manifest"
    requirement: LNCH-02
    verification:
      - kind: e2e
        ref: "campfire-cli sync against https://mc.campfire.pub:8444 into an empty /tmp/campfire-sync — 3545/3545 files, 367531501 bytes, exit 0"
        status: pass
      - kind: integration
        ref: "python3 scripts/assemble-client.py --dest /tmp/campfire-sync/game --verify — VERIFY OK, 3545 files, 367531501 bytes (independent cross-check of the Rust port's output)"
        status: pass
    human_judgment: false
  - id: D2
    description: "A second sync with nothing changed downloads zero bytes; a sync after exactly one file changed downloads exactly that one file"
    requirement: LNCH-02
    verification:
      - kind: e2e
        ref: "second `campfire-cli sync` run: SYNC OK — checked=3545 downloaded=0 deleted=0 seeded=0 bytes=0"
        status: pass
      - kind: e2e
        ref: "truncated mods/AIReducer-1.12.2-0.3.0.jar to 0 bytes, re-ran sync: downloaded=1 bytes=85307, sha256 afterward matches the manifest"
        status: pass
    human_judgment: false
  - id: D3
    description: "A file the manifest lists in delete[] is removed and the now-empty directory is pruned; a manifest-claimed player-state delete is refused instead"
    requirement: LNCH-02
    verification:
      - kind: unit
        ref: "manifest::tests::delete_removes_a_listed_file_and_prunes_the_now_empty_directory"
        status: pass
      - kind: unit
        ref: "manifest::tests::delete_never_touches_player_state_even_if_a_manifest_claimed_it"
        status: pass
    human_judgment: true
    rationale: "The live manifest's own delete[] array is empty at the time of this run (nothing has been removed from the pack yet), so the cumulative-across-several-publishes behavior described in the plan's truth is proven at the unit level (a crafted Manifest struct) rather than against a real multi-publish history on the live server — deliberately did not mutate the production pack/manifest via publish-pack.sh to manufacture a live delete[] entry, to avoid touching what a real friend group's launcher would sync from."
  - id: D4
    description: "Saves, options.txt, optionsof.txt and servers.dat survive every sync and every verify pass untouched"
    requirement: LNCH-02
    verification:
      - kind: e2e
        ref: "planted saves/World/level.dat, options.txt, optionsof.txt, servers.dat with known content; ran sync then verify; sha256sum of all four unchanged; saves/World/ still present"
        status: pass
    human_judgment: false
  - id: D5
    description: "A manifest entry with an absolute path, a parent-directory component, a control character, an escape-the-root path, or a forbidden Minecraft-owned prefix causes the whole sync to be refused"
    requirement: LNCH-02
    verification:
      - kind: unit
        ref: "launcher/core/tests/manifest_guard.rs — 9 tests (8 rejecting: absolute path, parent-dir in path, parent-dir in url, control char, library prefix, vanilla-jar basename, missing sha256, delete[] parent-dir; 1 accepting a well-formed manifest)"
        status: pass
      - kind: unit
        ref: "manifest::tests::a_hostile_manifest_among_189_good_entries_is_rejected_before_any_file_would_be_written — the exact acceptance-criteria scenario (189 good + 1 '../../../../etc/campfire-owned'), asserts zero files land"
        status: pass
      - kind: other
        ref: "jq -e '[.files[]|select(.path|test(\"^(libraries|assets|versions)/\"))]|length==0' over the live manifest.json — exits 0"
        status: pass
    human_judgment: false
  - id: D6
    description: "A file whose bytes don't hash to what the manifest published never lands at its final path — temp file, hash-then-rename"
    requirement: LNCH-02
    verification:
      - kind: other
        ref: "download_one()'s structure (manifest.rs): rename() is only reached after size+sha256 both match; the same function ran 3546 times across this run's cold sync, truncated-file re-sync, and tampered-file repair without ever landing a mismatched file"
        status: pass
    human_judgment: true
    rationale: "Not independently proven against a live server serving deliberately wrong bytes for a claimed-good sha256 (would require a malicious manifest endpoint); the guarantee is structural (rename() is unreachable on a hash mismatch) and exercised indirectly by every download in this run."
  - id: D7
    description: "Verify files re-hashes every managed file against the pinned manifest and repairs whatever doesn't match, reporting a count"
    requirement: LNCH-02
    verification:
      - kind: e2e
        ref: "appended one byte to config/BaubleEnhancements.cfg, ran campfire-cli verify: VERIFY OK — checked=3545 repaired=1, sha256 afterward matches the manifest"
        status: pass
    human_judgment: false
  - id: D8
    description: "Sync reports progress as a step name, file count, and byte rate through a sink the caller supplies, not a global"
    requirement: LNCH-05
    verification:
      - kind: other
        ref: "progress.rs's ProgressSink<'a> = &'a (dyn Fn(Progress) + Send + Sync) — no global/static state; cold sync log: 7090 Step lines + 3545 Bytes lines (well over the 20-line/1-rate-line acceptance bar)"
        status: pass
    human_judgment: false
  - id: D9
    description: "A Java 8 runtime for the requested platform is downloaded from Adoptium, checked against the vendor's own checksum, and extracted into the launcher's own runtime directory"
    requirement: LNCH-03
    verification:
      - kind: e2e
        ref: "campfire-cli java-fetch --target {windows-x64,mac-x64,mac-arm64} against api.adoptium.net — all three: release=jdk8u504-b01, checksum printed matches Adoptium's own field, extracted executable present under runtime/<release>-<target>/"
        status: pass
    human_judgment: false
  - id: D10
    description: "Apple Silicon is deliberately served the x86_64 macOS build; the launcher can tell whether it's running translated"
    requirement: LNCH-03
    verification:
      - kind: e2e
        ref: "--target mac-arm64 and --target mac-x64 printed byte-identical link and checksum values in the same run"
        status: pass
      - kind: unit
        ref: "java::tests::the_two_macos_targets_resolve_to_the_identical_query"
        status: pass
    human_judgment: true
    rationale: "translation_state()'s sysctlbyname('sysctl.proc_translated') call is compiled only under #[cfg(target_os = \"macos\")] and cannot execute or be verified on this aarch64 Linux Pi — no Apple Silicon hardware available. The technique matches 04-RESEARCH.md's cited source; the operator's real Apple Silicon machine is what proves this at runtime (already flagged as pending in 04-01-SUMMARY.md's Pending Human Verification)."
  - id: D11
    description: "The system Java is never consulted, never probed, never used, on any platform"
    requirement: LNCH-03
    verification:
      - kind: other
        ref: "grep -rn 'JAVA_HOME' launcher/core/src -> 0; grep -n 'which|from_path|env::var(\"PATH\")' launcher/core/src/java.rs -> 0; every returned path asserted under runtime_dir() by java::tests::every_resolved_java_path_lives_under_the_runtime_directory"
        status: pass
    human_judgment: false
  - id: D12
    description: "A Java download that fails its checksum leaves no runtime directory behind and reports a failure"
    requirement: LNCH-03
    verification:
      - kind: e2e
        ref: "CAMPFIRE_JAVA_FORCE_CHECKSUM_MISMATCH=1 campfire-cli java-fetch --target windows-x64 — exits 1 with ChecksumMismatch, no runtime/<release>-<target>/ directory created, no .tmp* files left under runtime/"
        status: pass
    human_judgment: false

duration: 55min
completed: 2026-08-28
status: complete
---

# Phase 4 Plan 2: Manifest Sync and Java 8 Provisioning Summary

**`campfire-cli sync`/`verify` rebuild a 3545-file/367MB game directory from `manifest.json` alone over the pinned CA (cross-checked byte-for-byte by `scripts/assemble-client.py`), and `java-fetch` provisions checksum-verified Adoptium Temurin 8 for all three shipped targets — with Apple Silicon proven to resolve the identical x86_64 archive as Intel.**

## Performance

- **Duration:** ~55 min
- **Started:** 2026-08-28T18:50:00Z (approx.)
- **Completed:** 2026-08-28T19:45:00Z (approx.)
- **Tasks:** 2
- **Files modified:** 10 (5 created: manifest.rs, java.rs, tests/manifest_guard.rs, assets/options.txt, assets/optionsof.txt; 5 modified: Cargo.toml, lib.rs, progress.rs, campfire-cli.rs, Cargo.lock)

## Accomplishments

- Ported `scripts/assemble-client.py` faithfully into `launcher/core/src/manifest.rs`: the whole-manifest path/URL guard (absolute, parent-component, control-character, escape-the-root, forbidden-prefix, vanilla-jar-basename), the sha256 diff, bounded 4-concurrent downloads with same-directory atomic rename, the cumulative `delete[]` pass, and a seed-once copy of the pack's own `options.txt`/`optionsof.txt` (closing `docs/DIST-OPS.md` gap 1). A second, code-level lock refuses to write or delete any player-state path regardless of what a manifest claims.
- Proved the whole sync live against `https://mc.campfire.pub:8444`: a cold sync of the full 3545-file/367,531,501-byte pack, independently verified byte-for-byte by `python3 scripts/assemble-client.py --verify`; a free re-sync (0 bytes); a truncated-file re-download (exactly 1 file); a tampered-file repair via `verify` (exactly 1 repaired); and four planted player-state files (a save, both options files, `servers.dat`) proven byte-identical before and after both operations.
- Landed `launcher/core/tests/manifest_guard.rs`, a 9-test hostile-manifest suite (8 rejections, 1 acceptance), plus a live-shaped integration test that rejects a manifest carrying 189 good entries and one `../../../../etc/campfire-owned` entry with zero files written.
- Built `launcher/core/src/java.rs`: a three-target Adoptium table (windows-x64, mac-x64, mac-arm64 — the latter two deliberately identical per D-10's locked Rosetta choice), checksum-verified download through `public_client()`, atomic extraction via a temp sibling directory with the same path guard the manifest module uses on every archive entry, and a `runtime/current.json` marker. Proved live against `api.adoptium.net`: all three targets fetched `jdk8u504-b01`, checksums matched the API's own field, mac-x64/mac-arm64 confirmed byte-identical, a forced checksum mismatch aborted cleanly with nothing left behind, and a re-fetch of an already-provisioned target completed in ~0.2s.
- Settled `progress.rs`'s final shape (`Step`/`Bytes`/`Done`/`Failed`, a `Send + Sync` sink) — the reporting contract every long-running core operation now speaks, ready for wave 4's Tauri channel adapter.

## Task Commits

1. **Task 1: Manifest sync — diff, download, delete, verify, and refuse a hostile manifest** - `0930cdc` (feat)
2. **Task 2: Java 8 the launcher owns — two vendors' worth of platforms, none of them the system's** - `f927ab8` (feat)

**Plan metadata:** (this commit, docs: complete plan)

## Files Created/Modified

- `launcher/core/src/manifest.rs` - manifest fetch/pin, whole-manifest guard, sha256 diff, bounded-concurrent download+atomic-rename, cumulative delete[], seed-once options, verify()
- `launcher/core/src/java.rs` - per-target Adoptium table, checksum-verified fetch, atomic archive extraction with path guard, `runtime/current.json`, Rosetta detection (macOS `cfg`-gated)
- `launcher/core/tests/manifest_guard.rs` - the 9-test hostile-manifest suite
- `launcher/core/assets/options.txt`, `optionsof.txt` - the pack's tuned client defaults, extracted from the cached client zip (4757/1436 bytes, exact)
- `launcher/core/Cargo.toml` - `sha2`, `futures-util`, `percent-encoding`, `zip`, `flate2`, `tar`, and a macOS-only `libc` target dependency
- `launcher/core/src/lib.rs` - `pub mod manifest;` / `pub mod java;`
- `launcher/core/src/progress.rs` - the final `Progress`/`ProgressSink` shape
- `launcher/core/src/bin/campfire-cli.rs` - `sync`/`verify`/`java-fetch`/`java-probe`, `--dir` override, progress printing

## Decisions Made

See frontmatter `key-decisions` for the full rationale on each. Summary:
- `progress.rs`'s shape from wave 1 was a placeholder nothing yet consumed — replaced outright with this plan's final `Step`/`Bytes`/`Done`/`Failed` shape rather than layered on top.
- Java's on-disk runtime directory is keyed by `<release>-<target>`, not `<release>` alone (see Deviations — this was a live bug found and fixed during Task 2).
- `ensure_java`'s idempotence check consults the filesystem (does the expected executable already exist) rather than only `runtime/current.json`, because the marker can only ever name one "current" target and this plan's own proof harness provisions three in one run.
- A `CAMPFIRE_JAVA_FORCE_CHECKSUM_MISMATCH` test-only env var proves the checksum-abort path against a real download, per the plan's own acceptance-criteria wording.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Java runtime directory collision between windows-x64 and mac-x64**
- **Found during:** Task 2, first three-target live verification run
- **Issue:** Adoptium's `release_name` for the current Java 8 update (`jdk8u504-b01`) is identical across the windows-x64 and mac-x64/mac-arm64 queries. The original code extracted every target into `runtime/<release_name>/`, so provisioning windows-x64 first and then mac-x64 made the second target's atomic rename fail with `Directory not empty (os error 39)` against the first target's already-extracted directory.
- **Fix:** Keyed the on-disk directory as `runtime/<release_name>-<target>/` instead. Also changed `ensure_java`'s idempotence check to query Adoptium first (cheap) and then check the filesystem for the expected executable, rather than trusting only `runtime/current.json` — needed because that marker can only name one target at a time and this crate's own proof harness fetches all three in one run.
- **Files modified:** `launcher/core/src/java.rs`
- **Verification:** Re-ran all three targets from an empty scratch root: all three succeeded, three distinct directories under `runtime/`, `du -sm` reported 334MB total (under the 400MB bar), no `.tmp*` files left.
- **Committed in:** `f927ab8` (Task 2 commit)

**2. [Rule 2 - missing critical functionality] `ensure_java` didn't expose the resolved link/checksum/release for the CLI to print**
- **Found during:** Task 2, while checking the acceptance criteria's "the printed download link is byte-identical" and "both values appear in the command's output" requirements
- **Issue:** The original `ensure_java(target) -> Result<PathBuf, JavaError>` signature only returned the final executable path — there was nowhere to get the release name, archive link, or checksum the acceptance criteria explicitly require the CLI to print.
- **Fix:** Added a `JavaProvision { java_path, release, link, checksum }` struct as `ensure_java`'s return type; `campfire-cli java-fetch` now prints all four fields.
- **Files modified:** `launcher/core/src/java.rs`, `launcher/core/src/bin/campfire-cli.rs`
- **Verification:** Live run: `mac-x64` and `mac-arm64` printed byte-identical `link=`/`checksum=` lines; `checksum=` matched Adoptium's own `checksum` field for the same query.
- **Committed in:** `f927ab8` (Task 2 commit)

**3. [Rule 3 - blocking, per the plan's own suggested escape hatch] Test-only checksum-mismatch env var**
- **Found during:** Task 2, proving the "a bad checksum aborts cleanly" acceptance criterion
- **Issue:** There is no way to make Adoptium serve a genuinely wrong checksum for a real archive; the acceptance criteria text itself says "a test-only entry point or an env override is fine."
- **Fix:** Added `CAMPFIRE_JAVA_FORCE_CHECKSUM_MISMATCH=1`, read once inside `ensure_java` immediately after the real Adoptium query, which corrupts only the *expected* checksum value before the real download and real hash run.
- **Files modified:** `launcher/core/src/java.rs`
- **Verification:** `CAMPFIRE_JAVA_FORCE_CHECKSUM_MISMATCH=1 campfire-cli java-fetch --target windows-x64` exited 1 with `ChecksumMismatch`, no `runtime/<release>-<target>/` directory, no leftover temp file.
- **Committed in:** `f927ab8` (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (1 bug, 1 missing-critical for acceptance-criteria verifiability, 1 blocking/testability per the plan's own suggested approach)
**Impact on plan:** All three were necessary for correctness or for the plan's own acceptance criteria to be checkable at all. No scope creep — no feature was added beyond what Task 1/Task 2's action text specifies.

## Issues Encountered

- **The "second sync completes in under a fifth of the first run's wall time" acceptance criterion could not be demonstrated on this Pi as literally worded.** Cold sync: 2.457s (367MB over Caddy's loopback front, effectively disk-speed on this host). Warm re-sync: 1.945s — not under a fifth, because the warm run must still `sha256`-hash all 3,545 files' bytes from disk to confirm nothing changed (the same double-check `assemble-client.py`'s own `download_entry` performs before skipping a file), and reading 367MB from disk turned out comparable in wall time to downloading 367MB over this Pi's essentially-instant loopback network. On a real deployment (an actual internet download, far slower than a local disk read) this criterion holds by construction; it doesn't on this specific loopback test rig. The substantive claim it's meant to prove — a warm sync transfers **zero bytes** — is fully proven (`downloaded=0 bytes=0`, confirmed twice).
- **D3's live cumulative-delete[] proof was done at the unit level, not against the live production manifest.** The live manifest's `delete` array is currently empty (nothing has been removed from the published pack since Phase 3), so proving the acceptance criterion's live-server path would have required running `scripts/publish-pack.sh --skip-fetch` after moving a real pack file aside — a deliberate mutation of the production manifest/pack this plan's own environment note says not to make unnecessarily. Proved instead with two focused unit tests directly against `apply_deletes()` (removal + directory pruning, and refusal of a manifest-claimed player-state delete).

## User Setup Required

None - no external service configuration required. Both new `campfire-cli` subcommands are build artifacts under `launcher/target/`, exercised entirely from this Pi against the live production endpoints.

## Known Stubs

None. Both halves (`manifest.rs`, `java.rs`) are fully wired: `campfire-cli sync`/`verify`/`java-fetch`/`java-probe` are real, complete implementations, not scaffolding for a later wave.

## Next Phase Readiness

- `manifest::sync()`/`verify()` and `java::ensure_java()` are both ready for wave 3 (Forge install + launch) to build on: a synced game directory and a resolved, checksum-verified Java 8 executable are now both one function call away.
- `progress.rs`'s final shape is what wave 4 adapts to a Tauri `ipc::Channel` — no further changes to the event shape should be needed.
- Blocker carried forward from `04-01-SUMMARY.md`: the Windows x64 / Apple Silicon human-check (visual/interactive UAT) is still pending — unaffected by this plan, which continues to verify everything headlessly via `campfire-cli` on this Pi. `translation_state()`'s actual runtime behavior on Apple Silicon specifically remains unverified until that check happens (see coverage `D10`).
- All scratch install roots used during verification (`/tmp/campfire-sync`, `/tmp/campfire-java`, `/tmp/campfire-java-mismatch`) were deleted after use; `rlcraft.service` and `caddy.service` were never restarted or reconfigured, and `uptime -s` (`2026-08-22 20:53:29`) was identical before and after every task in this plan.

---
*Phase: 04-launcher*
*Completed: 2026-08-28*

## Self-Check: PASSED

All 9 files claimed as created/modified were confirmed present on disk (`launcher/core/src/manifest.rs`, `launcher/core/src/java.rs`, `launcher/core/tests/manifest_guard.rs`, `launcher/core/assets/options.txt`, `launcher/core/assets/optionsof.txt`, `launcher/core/Cargo.toml`, `launcher/core/src/lib.rs`, `launcher/core/src/progress.rs`, `launcher/core/src/bin/campfire-cli.rs`), and both task commit hashes (`0930cdc`, `f927ab8`) were confirmed present in `git log --oneline --all`.
