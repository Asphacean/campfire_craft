---
phase: 05-release-to-friends
reviewed: 2026-08-30T19:51:55Z
depth: standard
files_reviewed: 10
files_reviewed_list:
  - .github/workflows/ci.yml
  - .github/workflows/release.yml
  - .github/actionlint.yaml
  - scripts/release.sh
  - scripts/publish-launcher.sh
  - .gitleaksignore
  - docs/FRIENDS.md
  - README.md
  - docs/LAUNCHER-BUILD.md
  - launcher/src-tauri/tauri.conf.json
findings:
  critical: 2
  warning: 4
  info: 0
  total: 6
status: issues_found
---

# Phase 5: Code Review Report

**Reviewed:** 2026-08-30T19:51:55Z
**Depth:** standard
**Files Reviewed:** 10
**Status:** issues_found

## Summary

Reviewed the release pipeline (CI + release workflows, `release.sh`,
`publish-launcher.sh`, `.gitleaksignore`) and the friend/operator-facing
docs and `tauri.conf.json` changes for this phase.

Good news first: `actionlint` (v1.7.12) passes clean on both workflow
files with no findings. Both `bash -n scripts/release.sh` and
`bash -n scripts/publish-launcher.sh` pass. No fork-triggerable event
exists in either workflow (`ci.yml` is `push: branches: ['**']`,
`release.yml` is `push: tags: 'v*'` only — no `pull_request`,
`pull_request_target`, or `workflow_run` anywhere). Every `${{ }}`
interpolation that touches `github.ref_name` lands in a `with:`/`runs-on:`
key, never inside a `run:` block, so there's no classic Actions
script-injection vector here. Job-level `permissions:` are minimal
(`contents: write` only on the job that must create the release,
`contents: read` on the self-hosted publish job). `.gitleaksignore`
entries are commit+file+line fingerprints re-verified by hand against
actual content, not blanket rule-name suppressions — not over-broad.

Two real problems remain, both worth blocking on:

1. The self-hosted `publish` job signs whatever bytes it downloads from
   the GitHub Release by name pattern, with no check that those bytes are
   what the `build` job in the same run actually produced. A compromised
   or maliciously-edited release asset gets the real pi-only key's
   signature and gets served to every launcher's auto-updater.
2. The one documented Gatekeeper recovery command in `docs/FRIENDS.md`
   (the `xattr -cr` fallback) targets the wrong `.app` path, because
   `productName` was capitalized+hyphenated this phase but the doc's
   fallback command still assumes a space-separated app name. A friend
   who needs that fallback (no "Open anyway" option offered) will hit
   "No such file or directory" with no other documented recovery path.

## Critical Issues

### CR-01: Publish job signs unverified release assets with the real signing key

**File:** `.github/workflows/release.yml:78-108` (also `scripts/publish-launcher.sh:184-218`, `sign_artifact()`)

**Issue:** The `publish` job downloads assets purely by matching a
filename regex against whatever the GitHub Release API currently lists for
the tag (`.assets[] | select(.name | test(...))`), then hands every
downloaded file straight to `scripts/publish-launcher.sh`, which signs it
with the real, pi-only minisign key and republishes it as the live update
feed. Nothing in this path checks that the bytes downloaded are the exact
bytes the `build` job's `tauri-action` step just produced — there is no
shared checksum manifest, no `actions/upload-artifact`/`download-artifact`
hand-off (which would be immutable per-run and not editable after the
fact), and no comparison against any hash recorded during the build.

A GitHub Release's assets are mutable after creation by anyone with
release-edit permission on the repo (this is a public repo). Anything that
can replace/re-upload an asset with the same matched filename between the
`build` job finishing and the `publish` job's `curl` — a compromised
collaborator token, a leaked `GITHUB_TOKEN` reused elsewhere, an
over-privileged bot, or simply a maintainer mistake — gets that payload
signed by the real key and served to every friend's launcher as a trusted
update. This is exactly the "signing oracle" failure mode: the pipeline
treats "matches this filename pattern on this tag's release" as
sufficient proof of provenance, when it is not.

**Fix:** Have the `build` job emit a checksum artifact (e.g.
`actions/upload-artifact` of a `SHA256SUMS` file, or a job `output`) scoped
to the workflow run, and have the `publish` job verify each downloaded
asset's `sha256sum` against that artifact **before** calling
`publish-launcher.sh`, refusing the whole run if any hash doesn't match.
Workflow-run artifacts (unlike release assets) can't be silently swapped
after upload, so pulling the checksums from there (rather than trusting
the mutable Release page a second time) closes the gap:

```yaml
# in the build job, after tauri-action:
- name: Record artifact checksums
  shell: bash
  run: sha256sum <path-to-bundle-outputs> > checksums-${{ matrix.platform }}.txt
- uses: actions/upload-artifact@<pinned-sha>
  with:
    name: checksums-${{ matrix.platform }}
    path: checksums-${{ matrix.platform }}.txt

# in the publish job, before invoking publish-launcher.sh:
- uses: actions/download-artifact@<pinned-sha>
  with: { pattern: 'checksums-*', path: checksums, merge-multiple: true }
- name: Verify downloaded assets against build-time checksums
  run: |
    set -euo pipefail
    cd "$WORKDIR"
    for f in *; do
      grep -F "  $f" ../checksums/*.txt || { echo "FATAL: no known-good checksum for $f"; exit 1; }
    done
    sha256sum -c <(cat ../checksums/*.txt) --ignore-missing
```

### CR-02: FRIENDS.md's Gatekeeper fallback command targets the wrong app path

**File:** `docs/FRIENDS.md:49,64` (root cause: `launcher/src-tauri/tauri.conf.json:3`)

**Issue:** This phase changed `productName` from `campfire-launcher` to
`Campfire-Launcher` (confirmed via `git diff 09f7748^..HEAD` and the
actual built artifacts in `launcher-dist/`, e.g.
`Campfire-Launcher_0.1.0_aarch64.app.tar.gz`). Tauri names the macOS `.app`
bundle directly from `productName`, so the real installed bundle is
`Campfire-Launcher.app` (hyphen, no space) — there is no `Info.plist`
override in `launcher/src-tauri` that would rename it, and `Cargo.toml`'s
own binary name (`campfire-launcher`, lowercase) is unrelated to the
bundle name.

`docs/FRIENDS.md` line 64 tells a friend, as the *documented fallback* for
when right-click → Open doesn't offer an "Open anyway" option, to run:

```
xattr -cr "/Applications/Campfire Launcher.app"
```

That path does not exist on a real install — the actual folder is
`/Applications/Campfire-Launcher.app`. A friend who needs this fallback
(exactly the audience this whole phase targets — someone with "no
Minecraft or server-admin experience") will get `xattr: /Applications/Campfire
Launcher.app: No such file or directory`, has no other documented recovery
path in the doc, and is stuck. Line 49's prose ("drag **Campfire
Launcher** into the Applications folder") has the same space-vs-hyphen
mismatch but is cosmetic there since dragging doesn't require typing a
path.

**Fix:** Match the doc to the actual `productName`:

```diff
- 1. Open the downloaded `.dmg` file, then drag **Campfire Launcher** into the
+ 1. Open the downloaded `.dmg` file, then drag **Campfire-Launcher** into the
    **Applications** folder.
...
-     xattr -cr "/Applications/Campfire Launcher.app"
+     xattr -cr "/Applications/Campfire-Launcher.app"
```

## Warnings

### WR-01: `release.sh` version comparison can crash on a leading-zero version component

**File:** `scripts/release.sh:69-119`

**Issue:** The version regex (`^([0-9]+)\.([0-9]+)\.([0-9]+)$`, used both
for the CLI argument at line 69 and for the current `tauri.conf.json`
value at line 97) accepts leading zeros in any component (e.g. `0.08.0`).
`is_lower()` then compares those captured strings with bash's `[ ... -lt
... ]` / `[ ... -gt ... ]`, which evaluate operands in bash **arithmetic**
context. A numeric string with a leading zero is parsed as octal there —
`08` or `09` are invalid octal digits, so bash raises `value too large for
base` and the script aborts with an unhandled shell error (not one of the
script's own documented exit codes) instead of the intended "not
major.minor.patch" rejection.

**Fix:** Either tighten the regex to reject leading zeros
(`^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$`), or strip leading
zeros before the arithmetic comparison (`10#$a_major` forces base-10
interpretation in bash arithmetic, e.g. `[ "10#$a_major" -lt "10#$b_major" ]`).

### WR-02: Non-random temp log path in `release.sh`'s cargo-update fallback

**File:** `scripts/release.sh:157-166`

**Issue:** Every other temp file this script writes uses `mktemp` (lines
129, 136), specifically noted in the script's own comment as intentional
("an interrupted run never leaves a half-written config"). The
cargo-update fallback instead redirects to the fixed path
`/tmp/release-cargo-update.log` twice. On a shared or multi-user host this
is a classic symlink/TOCTOU race: anything with write access to `/tmp` can
pre-create that path as a symlink to a file the invoking user can write,
and the `2>...`/`2>>...` redirects will follow it.

**Fix:** Use `mktemp` here too, consistent with the rest of the script:

```bash
CARGO_UPDATE_LOG="$(mktemp)"
trap 'rm -f "$CARGO_UPDATE_LOG"' EXIT
if ! (cd launcher && "$CARGO_BIN" update --workspace --offline) 2>"$CARGO_UPDATE_LOG"; then
  ...
```

### WR-03: `publish-launcher.sh` silently lets a duplicate platform key clobber an earlier one

**File:** `scripts/publish-launcher.sh:147-155` (`detect_platform`), `226-254` (`publish`)

**Issue:** `detect_platform()` maps both `*_x64-setup.exe` and
`*_x64_en-US.msi` to the same platform key `windows-x86_64`. The script's
own header and `resolve_all_platforms()` comment state the design intent
is "a whole-run refusal, not a partial publish" whenever something can't
be resolved cleanly — but nothing detects *two different artifacts*
resolving to the *same* platform key. In `publish()`'s loop, `jq --arg k
"$platform" '.[$k] = {...}'` simply overwrites the earlier entry with the
later one, with no log line or refusal. Today's CI path avoids this only
because `release.yml`'s asset-selection regex happens to exclude `.msi`
(line 93) — but an operator manually running `publish-launcher.sh` with
both a `.exe` and a `.msi` for the same release (a case the script's own
usage text explicitly documents as supported input) gets a silently
incomplete/wrong feed with no error.

**Fix:** Track seen platform keys in `resolve_all_platforms()` and refuse
before copying anything, the same way an unresolvable filename is refused:

```bash
declare -A _seen_platforms
for artifact in "${ARTIFACTS[@]}"; do
  ...
  if [ -n "${_seen_platforms[$platform]:-}" ]; then
    log "FATAL: platform $platform already resolved from ${_seen_platforms[$platform]}, refusing duplicate from $filename"
    exit 3
  fi
  _seen_platforms[$platform]="$filename"
  ...
```

### WR-04: `LAUNCHER-BUILD.md` still documents the pre-rename lowercase installer filename

**File:** `docs/LAUNCHER-BUILD.md:70`

**Issue:** Line 70 tells a builder that dropping `--no-bundle` produces
`campfire-launcher_<version>_x64-setup.exe` (all-lowercase). Since
`productName` was changed to `Campfire-Launcher` this same phase, the
real artifact — confirmed both by `docs/FRIENDS.md`'s own table
(`Campfire-Launcher_0.1.0_x64-setup.exe`) and the built files already in
`launcher-dist/` — is capitalized/hyphenated. This is the same
`productName` rename that caused CR-02; here it's a doc-only
inconsistency (an operator comparing a real build's output filename
against this line will think something's wrong), not a functional break,
so it's a warning rather than a blocker.

**Fix:**

```diff
- (`campfire-launcher_<version>_x64-setup.exe`) — the shape
+ (`Campfire-Launcher_<version>_x64-setup.exe`) — the shape
```

---

_Reviewed: 2026-08-30T19:51:55Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
