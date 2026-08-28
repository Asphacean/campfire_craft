---
phase: 03-modpack-distribution
fixed_at: 2026-08-28T15:46:28Z
review_path: .planning/phases/03-modpack-distribution/03-REVIEW.md
iteration: 1
findings_in_scope: 4
fixed: 4
skipped: 0
status: all_fixed
---

# Phase 3: Code Review Fix Report

**Fixed at:** 2026-08-28T15:46:28Z
**Source review:** .planning/phases/03-modpack-distribution/03-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 4 (CR-01, CR-02, WR-01, WR-02 — Info findings IN-01/IN-02 were optional and left for the operator)
- Fixed: 4
- Skipped: 0

**Verification environment:** all edits made and all gates (`bash -n`, `python3 -m py_compile`, `caddy validate`, `cargo build --release`) run directly in the main checkout at `/home/asphacean/rlcraft` on branch `master` — `workflow.use_worktrees` is `false` in `.planning/config.json`, so no isolated worktree was created. Every number below is reproducible from this same tree.

## Fixed Issues

### CR-01: A symlink in the CurseForge client zip bypasses the forbidden-content gate and is served verbatim over public HTTPS

**Files modified:** `scripts/gen-manifest.py`, `scripts/publish-pack.sh`, `caddy/Caddyfile`
**Commit:** `501954f`
**Applied fix:** Two-layer defense in depth, exactly as reviewed:
- `scripts/gen-manifest.py`'s `collect_paths()` now hard-fails (`FATAL` + `sys.exit(3)`) the instant it finds a symlink under the pack root, instead of silently skipping it — matching the review's exact suggested diff.
- `scripts/publish-pack.sh`'s `extract_overrides()` now runs `find "$overrides_dir" -type l -print0 -delete` on the just-unzipped CurseForge `overrides/` tree before the `rsync -a` into `PACK_DIR`, logging a `WARNING` per stripped symlink, so a bad zip never lands a symlink inside `PACK_DIR` at all.
- `caddy/Caddyfile`: added a comment at the `tls` directive documenting the CR-01 permission decision — kept the existing `chmod 640` group-read grant on the leaf key (rather than adding a caddy-owned 600 copy) because the two code-level gates above close the only path a symlink had into `pack/`; a synced-on-every-rotation key copy would add complexity without closing an attack surface the gates already close.

**Live verification:** `find pack -type l` confirmed 0 symlinks before and after; `bash scripts/publish-pack.sh --skip-fetch` re-ran clean (3545 files, 0 delete); `python3 scripts/assemble-client.py --dest ~/client-check --verify` returned `VERIFY OK — 3545 files, 367531501 bytes` (unchanged from pre-fix baseline); `caddy validate --config caddy/Caddyfile --adapter caddyfile` returned `Valid configuration` (pre-existing `header_up` warnings from IN-01 unchanged, out of scope); `bash -n`/`py_compile` clean on the shell/Python changes.

### CR-02: `publish-pack.sh` drops `set -e` and leaves its two riskiest steps unchecked

**Files modified:** `scripts/publish-pack.sh`
**Commit:** `c7996b7`
**Applied fix:**
- Restored `set -euo pipefail` (was `set -uo pipefail`).
- `extract_client_zip()`: `unzip -q ... || { log "FATAL..."; exit 3; }`.
- `extract_overrides()`: the `rsync -a` into `PACK_DIR` now `|| { log "FATAL: overrides rsync failed"; exit 5; }`.
- `overlay_own_content()`: the config `rsync -a --delete` now checked (`exit 5` on failure); the `campfire-auth-*.jar` glob is resolved into an array and its existence checked with `[ -e "${jar[0]}" ]` — if no jar matches (renamed, build not run, wrong version), the run exits 5 **before** `rm -f` deletes the previously-published jar, exactly closing the "delete-then-fail-silently" hole the review found live; the final `cp` is also checked.

**Live verification:** `bash -n scripts/publish-pack.sh` clean; `bash scripts/publish-pack.sh --skip-fetch` re-ran clean under `set -e` (3545 files, unchanged). **Negative test** (scratch copy, no live files touched): extracted the literal fixed `overlay_own_content()` function body out of the committed script via `awk` and ran it standalone against a scratch `REPO_ROOT`/`PACK_DIR` with no `campfire-auth-*.jar` present under the scratch `server/mods/` (simulating "build not yet run") and a pre-existing jar already in scratch `PACK_DIR/mods/` (simulating a previously-published pack). Result: `FATAL: no campfire-auth-*.jar found under server/mods/ — refusing to publish without it`, exit code `5`, and the old jar in scratch `PACK_DIR/mods/` was confirmed still present afterward (`rm -f` never ran) — proving the fix stops an incomplete publish before it can delete what was there before.

### WR-01: SLP response string length used to allocate a buffer with no upper bound

**Files modified:** `auth-service/src/slp.rs`
**Commit:** `2e1f1b3`
**Applied fix:** Added `const MAX_SLP_STRING: usize = 64 * 1024;` and a check immediately after decoding `str_len`, returning `None` (the module's existing "ordinary offline result" convention) if the decoded VarInt exceeds it, before `read_exact_n`'s `vec![0u8; n]` allocation ever runs — exactly the review's suggested diff.

**Live verification:** `cargo build --release --manifest-path auth-service/Cargo.toml` — clean build, no warnings from the new code. Installed via `sudo install -m 755 auth-service/target/release/campfire-auth /usr/local/bin/campfire-auth` (the service's actual `ExecStart` target, confirmed via `systemctl cat campfire-auth`), then `sudo systemctl restart campfire-auth` — service came back `active` immediately. `bash scripts/auth-smoke.sh` returned `SMOKE OK (35 checks)`, all passing, including the live-`/status` and offline-`/status` branches. `curl --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/status` returned `{"online":true,"players":0,"max":10,"motd":"campfire.pub"}`. `rlcraft.service` uptime unchanged (`uptime -s` = `2026-08-22 20:53:29` before and after — never touched); `caddy`/`campfire-auth` both `active`.

### WR-02: `assemble-client.py` crashes with an uncaught `KeyError` on a manifest entry missing a required field

**Files modified:** `scripts/assemble-client.py`
**Commit:** `55af89c`
**Applied fix:** Added a guard loop at the top of the per-entry loop in `validate_manifest_entries()` checking `("path", "url", "sha256", "size")` are all present, exiting cleanly with `FATAL: manifest entry missing required field '<field>': <entry>` and exit code `2` (the documented "manifest failed the guard" code) instead of letting a later unguarded `entry["path"]`/`entry["sha256"]` raise a raw `KeyError` — exactly the review's suggested diff.

**Live verification:** `python3 -m py_compile scripts/assemble-client.py` clean; `python3 scripts/assemble-client.py --dest ~/client-check --verify` against the real live manifest still returned `VERIFY OK — 3545 files, 367531501 bytes` (unchanged). **Self-test** (`importlib` loading the real committed module by path, not a reimplementation): called `validate_manifest_entries()` directly with a manifest entry missing `sha256` — result: clean `FATAL: manifest entry missing required field 'sha256': {...}` log line and `SystemExit(2)`, confirmed by the test harness (`PASS: missing 'sha256' field -> clean SystemExit(2), no KeyError traceback`), no raw traceback.

## Skipped Issues

None — all 4 in-scope findings were fixed. `IN-01` and `IN-02` (Info) were out of scope per this fix run's instructions and were not touched.

---

_Fixed: 2026-08-28T15:46:28Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
