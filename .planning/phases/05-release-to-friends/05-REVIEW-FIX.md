---
phase: 05-release-to-friends
fixed_at: 2026-08-30T22:00:00Z
review_path: .planning/phases/05-release-to-friends/05-REVIEW.md
iteration: 1
findings_in_scope: 6
fixed: 6
skipped: 0
status: all_fixed
---

# Phase 5: Code Review Fix Report

**Fixed at:** 2026-08-30T22:00:00Z
**Source review:** .planning/phases/05-release-to-friends/05-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 6 (CR-01, CR-02, WR-01, WR-02, WR-03, WR-04)
- Fixed: 6
- Skipped: 0

## Fixed Issues

### CR-01: Publish job signs unverified release assets with the real signing key

**Files modified:** `.github/workflows/release.yml`
**Commit:** `f827e4f`
**Applied fix:** The `build` job now computes a `sha256sum` of every
matched platform artifact after `tauri-action` runs, writes it to a
per-platform `checksums-${{ matrix.platform }}.txt` (basenames only, so
it lines up with the flat download directory the `publish` job uses),
and uploads it via `actions/upload-artifact@043fb46d...` (pinned, v7.0.1).
The `publish` job downloads all `checksums-*` artifacts via
`actions/download-artifact@3e5f45b2...` (pinned, v8.0.1, `merge-multiple:
true`) into `checksums/`, then — before invoking
`scripts/publish-launcher.sh` — refuses (`exit 1`) if any downloaded
release asset has no matching checksum line, and runs
`sha256sum -c ... --ignore-missing` to hard-fail on any mismatch. Because
workflow-run artifacts are immutable after upload (unlike Release page
assets, which anyone with release-edit rights can swap), this closes the
signing-oracle gap: a bad-faith or accidental asset swap between the
`build` and `publish` jobs of the same run is now caught before signing.

**What this does NOT yet prove:** the cross-job artifact upload/download
hand-off itself has not run in a live GitHub Actions execution — no new
tag has been cut since this fix landed. What *was* verified locally: (1)
`actionlint -config-file .github/actionlint.yaml` passes clean on both
workflow files with no findings; (2) a dry run of the verification logic
(the same `sed`-basename-stripped `sha256sum` + `sha256sum -c
--ignore-missing` commands, copy-pasted verbatim) against the real
`launcher-dist/` artifacts from the v0.1.0 release passes clean on
correct bytes and correctly fails with exit 1 when one file is tampered
with. **The CI path (artifact upload in the build job actually reaching
the publish job of the same run) is only provable on the next tagged
release** — recorded here honestly per the task's own guidance, not
silently glossed over.

### CR-02: FRIENDS.md's Gatekeeper fallback command targets the wrong app path

**Files modified:** `docs/FRIENDS.md`
**Commit:** `70bb604`
**Applied fix:** Corrected both occurrences of the app name to match the
real `productName` (`Campfire-Launcher`, hyphenated, no space) from
`launcher/src-tauri/tauri.conf.json`: the drag-to-Applications prose
(line 49) and, more importantly, the `xattr -cr` Terminal fallback (line
64), which now reads `xattr -cr "/Applications/Campfire-Launcher.app"` —
the actual installed bundle path, confirmed against the real artifact
filenames (`Campfire-Launcher_0.1.0_aarch64.app.tar.gz`, etc.) already
published to the v0.1.0 release.

### WR-01: release.sh version comparison can crash on a leading-zero version component

**Files modified:** `scripts/release.sh`
**Commit:** `8e22106`
**Applied fix:** Tightened both version-format regexes (the CLI-argument
check and the current-`tauri.conf.json`-value check) from
`^([0-9]+)\.([0-9]+)\.([0-9]+)$` to `^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$`,
so a leading-zero component like `0.08.0` is rejected with the script's
own documented exit code (2) before it ever reaches `is_lower()`'s bash
arithmetic comparison (which would otherwise crash on `08`/`09` as
invalid octal).

**Verified** in a lightweight throwaway git clone (rehearsal style, no
real remote, matching the project's own `--no-push` rehearsal
convention from `05-01-PLAN.md`): `bash scripts/release.sh 0.08.0
--no-push` now exits `2` with the documented FATAL message and leaves
the tree untouched (previously would have run the regex through fine
and later crashed inside `cargo update`'s Cargo.toml parse — confirmed
that crash reproduces on the pre-fix code, `exit_code=6`, an *undocumented*
failure mode). Also confirmed a leading-zero *current* version in
`tauri.conf.json` is now rejected the same clean way, and that a normal
version bump (`9.9.9`) still works end-to-end unaffected. `bash -n`
passes.

### WR-02: Non-random temp log path in release.sh's cargo-update fallback

**Files modified:** `scripts/release.sh`
**Commit:** `48e9e31`
**Applied fix:** Replaced the fixed `/tmp/release-cargo-update.log` path
with `CARGO_UPDATE_LOG="$(mktemp)"` plus an `EXIT` trap for cleanup,
consistent with every other temp file this script already writes via
`mktemp` (the script's own comment calls this out as intentional). `bash
-n` passes; no live symlink-race exploit attempt was run (out of scope —
the fix is the same pattern already proven safe elsewhere in this file).

### WR-03: publish-launcher.sh silently lets a duplicate platform key clobber an earlier one

**Files modified:** `scripts/publish-launcher.sh`
**Commit:** `fc30a6e`
**Applied fix:** `resolve_all_platforms()` now tracks seen platform keys
in a `declare -A _seen_platforms` map and refuses the whole run (`exit
3`, the same code an unresolvable filename already uses) the moment a
second artifact resolves to a platform key already claimed by an earlier
one — before anything is copied or signed, matching the script's own
"whole-run refusal, not a partial publish" design intent.

**Verified** by extracting `detect_platform`/`resolve_all_platforms`
into an isolated harness (no `server.env`/signing key needed): a
`.exe` + `.msi` pair (both mapping to `windows-x86_64`) now exits `3`
with a clear FATAL message identifying both filenames (previously would
have silently overwritten the `.exe` entry with the `.msi` one, no
error, no log line). Regression-checked that three genuinely
distinct-platform artifacts (`.exe`, both `.app.tar.gz`) still resolve
cleanly with no behavior change. `bash -n` passes. Today's CI path is
unaffected either way (`release.yml`'s asset regex already excludes
`.msi`), so this closes the gap only an operator running the script
manually with both artifacts could hit.

### WR-04: LAUNCHER-BUILD.md still documents the pre-rename lowercase installer filename

**Files modified:** `docs/LAUNCHER-BUILD.md`
**Commit:** `53850e8`
**Applied fix:** Corrected the example filename on line 70 from
`campfire-launcher_<version>_x64-setup.exe` to
`Campfire-Launcher_<version>_x64-setup.exe`, matching the same doc's own
QA-matrix table (line 317) and the real released artifacts, both of
which already used the capitalized/hyphenated name.

## Skipped Issues

None — all 6 in-scope findings were fixed.

---

_Fixed: 2026-08-30T22:00:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
