---
phase: 03-modpack-distribution
reviewed: 2026-08-28T15:39:45Z
depth: standard
files_reviewed: 19
files_reviewed_list:
  - caddy/Caddyfile
  - scripts/install-caddy.sh
  - scripts/renew-cert.sh
  - scripts/publish-pack.sh
  - scripts/gen-manifest.py
  - scripts/assemble-client.py
  - scripts/reachability.sh
  - scripts/backup.sh
  - scripts/auth-smoke.sh
  - auth-service/src/slp.rs
  - auth-service/src/api.rs
  - auth-service/src/main.rs
  - auth-service/Cargo.toml
  - auth-service/README.md
  - ca/campfire-ca.pem
  - docs/DIST-OPS.md
  - docs/CLIENT-SETUP.md
  - server.env.example
  - .gitignore
findings:
  critical: 2
  warning: 2
  info: 2
  total: 6
status: issues_found
---

# Phase 3: Code Review Report

**Reviewed:** 2026-08-28T15:39:45Z
**Depth:** standard
**Files Reviewed:** 19
**Status:** issues_found

## Summary

Reviewed the full HTTPS distribution front: `caddy/Caddyfile`, the CA/cert/publish/manifest/assemble Python+shell tooling, `campfire-auth`'s Server List Ping client and HTTP handlers, and the operational docs. `git ls-files ca/` confirms only the public `campfire-ca.pem` is tracked — no private key is committed. `caddy validate`, `bash -n` on all six shell scripts, `python3 -m py_compile` on both Python scripts, and `cargo build --release` all pass clean.

Two things were empirically verified rather than just read:

1. **The Caddy `X-Forwarded-For` defense is sound.** I stood up a throwaway Caddy `reverse_proxy` (with and without the Caddyfile's explicit `header_up X-Forwarded-For {http.request.remote.host}`) and sent a spoofed `X-Forwarded-For: 6.6.6.6, 7.7.7.7`. In both cases the upstream received only Caddy's own view of the peer (`127.0.0.1`) — the client-supplied value never reached the backend. `caddy validate` itself flags the explicit `header_up` line as "Unnecessary" for exactly this reason (see IN-01).
2. **A malicious symlink inside the CurseForge client zip's `overrides/` tree survives the entire publish pipeline and gets served verbatim by Caddy's `file_server`.** I built a zip containing `overrides/mods/evil.jar` as a symlink to an arbitrary file, ran it through the same `unzip -q` → `rsync -a` sequence `scripts/publish-pack.sh` uses, pointed a `file_server` block identical to the reviewed Caddyfile's at the result, and `curl`'d the path — the target file's contents came back over plain HTTP. `scripts/gen-manifest.py`'s traversal/forbidden-content gate never sees this path at all, because `collect_paths()` silently excludes symlinks from consideration (CR-01).

`scripts/publish-pack.sh` also has a design regression relative to every sibling script in this phase: it drops `set -e` (uses `set -uo pipefail` instead of `set -euo pipefail`) and two of its most consequential steps — the `overrides/` rsync and the `campfire-auth` jar overlay — have no exit-code checking at all, contradicting the script's own documented guarantee that an incomplete pack is never published as complete (CR-02).

## Critical Issues

### CR-01: A symlink in the CurseForge client zip bypasses the forbidden-content gate and is served verbatim over public HTTPS

**File:** `scripts/gen-manifest.py:77-78`, `scripts/publish-pack.sh:337-343`, `caddy/Caddyfile:75-80`

**Issue:** `scripts/publish-pack.sh`'s `extract_overrides()` does `rsync -a "$WORK_DIR/overrides/" "$PACK_DIR/"` straight from a third-party CurseForge zip extracted with `unzip -q` (line 229). Both `unzip` and `rsync -a` preserve Unix symlinks by default, so any symlink entry present in that zip's `overrides/` tree lands as a real filesystem symlink inside `PACK_DIR`.

`scripts/gen-manifest.py`'s `collect_paths()` explicitly skips symlinks:
```python
full = os.path.join(dirpath, name)
if not os.path.isfile(full) or os.path.islink(full):
    continue
```
This means a symlink is never added to `rel_paths`, and therefore `validate_paths()` (the traversal guard) and `apply_forbidden_content_gate()` (T-03-02-02's secrets gate) never inspect it at all — the file it never gets manifested, but it is still physically present under `PACK_DIR` at that path.

`caddy/Caddyfile`'s `handle_path /pack/* { root * .../pack; file_server { hide .* } }` serves any real path under `PACK_DIR` by direct request, following symlinks like any standard file server (Caddy has no built-in "don't follow symlinks" option). A client that requests the exact path of the symlinked entry gets the *target* file's contents, not a 404 — even though the manifest never advertised it.

**Verified live** (reproduction, not speculation): built a zip with `overrides/mods/evil.jar` symlinked to an arbitrary out-of-tree file, ran it through `unzip -q` → `rsync -a` exactly as `publish-pack.sh` does, served the resulting tree with a `file_server` block identical to the reviewed Caddyfile's, and `curl http://.../pack/mods/evil.jar` returned the symlink target's full contents.

Impact is not theoretical: `scripts/install-caddy.sh` explicitly `chgrp caddy` + `chmod 640`s the live TLS leaf private key (`ca/mc.campfire.pub-key.pem`) so Caddy's own worker process can read it — a crafted symlink at any published pack path pointing at that key would leak it to the entire internet with a single HTTP GET, along with any other file readable by the `caddy` user (world-readable files anywhere the granted `/home/asphacean` traversal ACL reaches).

**Fix:** Make the pipeline reject symlinks instead of silently skipping them. Two changes needed together:
```python
# gen-manifest.py — fail the whole publish instead of pretending the entry doesn't exist
for name in filenames:
    ...
    full = os.path.join(dirpath, name)
    if os.path.islink(full):
        log(f"FATAL: {os.path.relpath(full, pack_root)} is a symlink — refusing to publish a tree containing symlinks")
        sys.exit(3)  # same gate class as the forbidden-content gate
    if not os.path.isfile(full):
        continue
```
And, as defense-in-depth so a bad zip never even reaches `PACK_DIR`, strip symlinks right after extraction in `publish-pack.sh`:
```bash
extract_overrides() {
  ...
  find "$overrides_dir" -type l -print -delete | while read -r l; do
    log "WARNING: dropped symlink from client zip overrides/: $l"
  done
  rsync -a --exclude ... "$overrides_dir/" "$PACK_DIR/"
}
```

### CR-02: `publish-pack.sh` drops `set -e` and leaves its two riskiest steps unchecked — an incomplete pack can be published and reported as a success

**File:** `scripts/publish-pack.sh:32,337-350,372-388`

**Issue:** Every other script touched in this phase (`install-caddy.sh:11`, `renew-cert.sh:9`, `backup.sh:16`) uses `set -euo pipefail`. `publish-pack.sh:32` uses `set -uo pipefail` — `-e` is deliberately absent, with no comment explaining why. Combined with that, `extract_overrides()` and `overlay_own_content()` never check the exit status of their commands:

```bash
extract_overrides() {          # line 329
  ...
  rsync -a \
    --exclude '/server.properties' ... \
    "$overrides_dir/" "$PACK_DIR/"    # line 337-343 — return code never checked
}

overlay_own_content() {        # line 346
  rsync -a --delete "$REPO_ROOT/server/config/" "$PACK_DIR/config/"   # unchecked
  rm -f "$PACK_DIR/mods/campfire-auth-"*.jar                          # unchecked
  cp "$REPO_ROOT"/server/mods/campfire-auth-*.jar "$PACK_DIR/mods/"   # unchecked
}
```

Concretely: if the `server/mods/campfire-auth-*.jar` glob doesn't match anything (jar renamed, build not yet run, wrong version string) the preceding `rm -f "$PACK_DIR/mods/campfire-auth-"*.jar` still deletes whatever jar was previously published, then `cp` fails with a `cp: cannot stat ...: No such file or directory` printed to stderr and a non-zero exit — which, because `-e` is off and nothing checks `$?`, is silently swallowed. `main()` proceeds straight into `finish_tree` and `publish_manifest()`, which atomically publishes a manifest describing a pack tree that is now missing the auth mod entirely. The run still prints `Confirm the new manifest is live: curl ...` and exits 0, i.e. it reports success for a broken publish. The same failure mode applies to the `overrides/` rsync in `extract_overrides` (e.g. transient disk-full mid-copy): a non-zero rsync exit is never observed, and the run continues to publish whatever partial tree resulted.

This directly contradicts the script's own header comment: *"an incomplete pack is never published as if complete"* (documented for the CurseForge-fetch failure path via `fail_count`/`exit 4`, but not honored for these two steps).

**Fix:** Restore `-e` and let the existing script structure do the rest (every other explicit `exit N` call already accounts for `-e`'s interaction with `||`):
```bash
set -euo pipefail
```
If any specific command inside these functions is expected to legitimately fail sometimes, guard it explicitly (as `publish_manifest()` already does with `|| rc=$?`) rather than leaving the whole script un-guarded. At minimum, add explicit checks:
```bash
overlay_own_content() {
  rsync -a --delete "$REPO_ROOT/server/config/" "$PACK_DIR/config/" || { log "FATAL: config overlay failed"; exit 5; }
  local jar
  jar=("$REPO_ROOT"/server/mods/campfire-auth-*.jar)
  [ -e "${jar[0]}" ] || { log "FATAL: no campfire-auth-*.jar found under server/mods/ — refusing to publish without it"; exit 5; }
  rm -f "$PACK_DIR/mods/campfire-auth-"*.jar
  cp "${jar[@]}" "$PACK_DIR/mods/"
}
```

## Warnings

### WR-01: SLP response string length is used to allocate a buffer with no upper bound

**File:** `auth-service/src/slp.rs:109-110`

**Issue:**
```rust
let str_len = read_varint(&mut stream).await.ok()? as usize;
let body = read_exact_n(&mut stream, str_len).await.ok()?;
```
`read_varint` bounds the *number of bytes read* (max 5, via the `shift >= 35` check) but not the *value* it produces. A VarInt can encode any `i32`, including values whose top bit is effectively set after the 28-bit shift, which `as usize` then turns into a very large (or, via `i32::MIN`-adjacent bit patterns, huge-after-cast) allocation request. `read_exact_n` immediately does `vec![0u8; n]` with that unbounded `n` (`auth-service/src/slp.rs:69`) before any await point the surrounding `tokio::time::timeout(SLP_TIMEOUT, ...)` could actually interrupt. A malformed or corrupted response (garbled TCP data, a misbehaving `SLP_ADDR` target, or a future misconfiguration pointing `SLP_ADDR` at something other than the trusted local Minecraft server) can trigger a multi-gigabyte allocation attempt. Rust's default allocation-failure behavior aborts the whole process — which would take down `/register`, `/login`, and `/validate` for every player, not just the `/status` endpoint this code nominally protects (the module's own doc comment says a hung/slow game server "must never hang every `/status` caller" — an OOM abort is strictly worse than a hang).

**Fix:**
```rust
const MAX_SLP_STRING: usize = 64 * 1024; // real response here is ~7.2kB; generous headroom
let str_len = read_varint(&mut stream).await.ok()? as usize;
if str_len > MAX_SLP_STRING {
    return None;
}
let body = read_exact_n(&mut stream, str_len).await.ok()?;
```

### WR-02: `assemble-client.py` crashes with an uncaught `KeyError` on a manifest entry missing a required field, instead of the documented controlled exit codes

**File:** `scripts/assemble-client.py:133,163-168,207-210`

**Issue:** `validate_manifest_entries()` only checks `entry.get(field, "")` for `"path"`/`"url"` inside its guard loop, but then immediately does `path = entry["path"]` (line 133) unguarded. `download_entry()` and `verify_entry()` likewise index `entry["path"]`, `entry["sha256"]`, `entry["size"]`, `entry["url"]` directly. Since `manifest.json` in this script is untrusted input by design (this is explicitly the reference implementation for validating a server-controlled manifest, per the module docstring's exit-code table: "2 = the manifest itself failed the client-side path guard or DIST-03 gate"), a manifest entry missing `sha256` or `size` (a bug on the publish side, or a manifest served by a MITM'd/compromised host with a malformed entry) raises an uncaught `KeyError`, terminating the script with a raw Python traceback and exit code 1 — not the documented "manifest failed the guard" exit code 2, and with no actionable log line.

**Fix:** Validate required keys up front, in the same guard loop that already exists:
```python
for entry in files:
    for field in ("path", "url", "sha256", "size"):
        if field not in entry:
            log(f"FATAL: manifest entry missing required field '{field}': {entry}")
            sys.exit(2)
    ...
```

## Info

### IN-01: `header_up X-Forwarded-For` override is redundant and its comment overstates what it does

**File:** `caddy/Caddyfile:40-56`

**Issue:** The comment above `handle /api/register`/`handle /api/login` claims the explicit `header_up X-Forwarded-For {http.request.remote.host}` is *what* discards a client-supplied `X-Forwarded-For` at the edge. `caddy validate --config caddy/Caddyfile --adapter caddyfile` flags both occurrences: `"Unnecessary header_up X-Forwarded-For: the reverse proxy's default behavior is to pass headers to the upstream"`. I confirmed this empirically: a throwaway `reverse_proxy` with the override removed produced byte-identical (correctly overwritten, spoofed value discarded) `X-Forwarded-For` behavior to the version with it. The protection is real, but it's Caddy's built-in default in this version, not something this directive is adding — the comment should say so, or the (harmless but validate-flagged) directive should be dropped.

**Fix:** Either drop the two `header_up` lines and update the comment to say the SET behavior is Caddy's default (cite the `caddy validate` warning as evidence), or keep the explicit lines as intentional defense-in-depth documentation and note in the comment that `caddy validate` will flag them as unnecessary — don't leave the comment implying they are the sole mechanism.

### IN-02: `/login`'s rate-limit check runs after JSON body parsing, unlike `/register`'s documented ordering

**File:** `auth-service/src/api.rs:211-224` vs `247-268`

**Issue:** `register()`'s comment explains the rate limiter is checked "before validation or DB work so a flood cannot spend CPU past this point," and the code does check `state.register_limiter.check(limit_ip)` before `body?`. `login()` does the opposite — `let Json(req) = body?;` (line 255) runs before `state.login_limiter.check(limit_ip)` (line 266) — so a flood of malformed-JSON `/login` requests never touches the limiter at all. Real-world impact is low (JSON parsing is cheap and the expensive path — argon2 — is still gated correctly for well-formed requests), but the inconsistency contradicts the design rationale stated for the sibling handler and is easy to overlook during a future refactor.

**Fix:** Move the `login_limiter.check()` call above `body?` in `login()`, mirroring `register()`'s ordering, or add a comment explaining why the two handlers deliberately differ if that's intentional.

---

_Reviewed: 2026-08-28T15:39:45Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
