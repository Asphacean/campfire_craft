---
phase: 02-accounts-enforced-auth
verified: 2026-08-28T00:00:00Z
status: human_needed
score: 9/9 must-haves verified (automatable subset); 2 items require a real client (human_verification)
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Test A — join with a valid token (a real, fully-modded RLCraft client)"
    expected: "Player connects normally, floats/frozen for up to ~5s while the server validates, then can move, break a block, and send a chat message. server/logs/latest.log shows a `[campfireauth]` line with result=allow for this join, and replaying the same token afterward is kicked (invalid_token)."
    why_human: "There is no Minecraft client on the Pi. The dependency-free join-probe.py cannot complete the full FML modded handshake against the live server (~200 RLCraft mods) — it was only used, correctly, to prove the tokenless/vanilla-protocol refusal path. The live server's real gate — a genuinely modded client with a valid token — has only been proven on 02-02's disposable single-mod devserver, not on the production server. Procedure: docs/AUTH-OPS.md 'Client verification' section and 02-03-PLAN.md Task 1's <human-check> Test A. Never paste the token itself when reporting results."
  - test: "Test B — join with a registered nick but no token, using the operator's real modded RLCraft client (mod jar present, no -D flags set)"
    expected: "Kicked before being able to move, interact or chat, with the exact bilingual message: 'Зайди через лаунчер campfire.pub / Join via the campfire.pub launcher'. server/logs/latest.log shows result=kick reason=no_packet."
    why_human: "This is the one live-server case that specifically proves campfireauth's own gate (not Forge's generic FML mod-list rejection). The automated join-probe.py run during this verification, and the one 02-03 recorded, both got Forge's own 'mods that require FML/Forge' handshake refusal because the probe speaks raw vanilla protocol and is missing ~200 mods, not just this one — that is a real refusal but is explicitly not proof of this project's own gate. Only a client carrying the full RLCraft mod list plus the campfire-auth-0.1.1.jar, connecting without the -D flags, exercises ServerAuthHandler's freeze/timeout/kick path on the live server. Procedure: docs/AUTH-OPS.md 'Client verification' section and 02-03-PLAN.md Task 1's <human-check> Test B. Test C (an actual vanilla, unmodded client) is optional and either outcome (our gate or Forge's handshake) is acceptable — the point is only to record which one."
---

# Phase 2: Accounts & Enforced Auth Verification Report

**Phase Goal:** Only a registered nick presenting a valid token can play; anyone with a vanilla client is turned away
**Verified:** 2026-08-28
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth (ROADMAP success criteria) | Status | Evidence |
|---|---|---|---|
| 1 | Register nick+password; duplicate refused | ✓ VERIFIED | Live: `POST /register` for a fresh throwaway nick → `201`; re-registering the same nick in a different letter case → `409 {"error":"nick_taken"}`. Test nick removed from the DB after verification. |
| 2 | Correct password → short-lived token; wrong password → error, no token | ✓ VERIFIED | Live: correct password → `200 {"token":"...","expires":<unix ts ~12h out>}`; wrong password → `401 {"error":"invalid_credentials"}` with no `token` key. |
| 3 | Client with valid token joins and plays | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED (routed to human) | Fully proven on 02-02's disposable single-mod devserver (token issued → `result=allow`, replay → kicked, service-down → kicked `service_error`) and the code path is unchanged since. Not independently re-provable against the live, fully-modded server by any grep/curl/probe — join-probe.py cannot complete a real FML handshake with ~200 mods. Requires a real modded client (Test A). |
| 4 | Vanilla client as registered nick kicked before it can move/interact | ⚠️ PRESENT_BEHAVIOR_UNVERIFIED (routed to human) | A raw-protocol (fully unmodded) probe against the live server IS refused before any join — but by Forge's own FML mod-list handshake ("mods that require FML/Forge..."), confirmed live in this verification run, not by campfireauth's bilingual gate. This is an honest, correctly-distinguished result (both 02-03-SUMMARY.md and docs/AUTH-OPS.md record it the same way) but it does not prove *this project's* gate refuses a client that has all the mods except a token — that is Test B, and needs a real modded client. |
| 5 | DB holds only argon2/bcrypt hashes | ✓ VERIFIED | Live: `sqlite3 auth/campfire.db "select count(*) from users where pw_hash not like '$argon2id$%'"` → `0`; grep of the full `users`/`tokens` tables for the fixture password and the issued token both → `0` matches. |

**Score:** 3/5 truths fully machine-verified; 2/5 present-and-wired but requiring a real client to exercise the runtime behavior (routed to human verification, not counted as failed).

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `auth-service/src/main.rs` | CLI dispatch, loopback bind guard, axum router | ✓ VERIFIED | Present, `campfire-auth serve\|login\|reset` usage confirmed live, `ss -ltn` shows only `127.0.0.1:8081` bound (no `0.0.0.0`/`[::]`). |
| `auth-service/src/db.rs` | SQLite schema, parameterised queries, atomic token consumption, pruning | ✓ VERIFIED | `consumed_at IS NULL` compare-and-swap present (line 225); `prune_tokens()` (WR-04 fix) present (line 209) and wired from `/login` (`api.rs:246`). |
| `auth-service/src/auth.rs` | argon2id hash/verify, CSPRNG tokens | ✓ VERIFIED | Live DB rows all `$argon2id$` PHC strings; token issued was 43-char base64url with no `+`/`/`/`=`. |
| `auth-service/src/ratelimit.rs` | Per-IP sliding window; register/login/validate policies | ✓ VERIFIED | `check()`/`refund()` present (WR-01 fix applied — single atomic reservation replacing the old peek/record split, confirmed in source at `ratelimit.rs:32,53` and `api.rs:233`). |
| `systemd/campfire-auth.service` | Loopback-only unit, Restart=on-failure, EnvironmentFile | ✓ VERIFIED | Live: `systemctl is-active`/`is-enabled` both positive; `UMask=0077` present (WR-03 fix); `ExecStart=/usr/local/bin/campfire-auth serve`. |
| `scripts/auth-smoke.sh` | Self-contained API assertion suite | ✓ VERIFIED (existence + syntax) | `bash -n` clean. Not re-run in full during this verification (would spin up an ephemeral instance/build) — SUMMARY records 28/28 passing post-fix, and this verification's own live curl sequence independently re-proved the core register/login/validate/replay/hash-at-rest behaviors it asserts. |
| `auth-service/README.md` | API contract for Phase 3/4 | ✓ VERIFIED | Read in full: 4 endpoints, every error code, token rules, CLI, and the 3 constraints for Phase 3/4 are all present. `grep -c '/validate'` ≥ 3. |
| `mods-src/campfire-auth/src/main/java/.../server/ServerAuthHandler.java` | Join freeze, timeout, off-thread validate, fail-closed kick, chat/command suppression | ✓ VERIFIED | Present in the built jar (`unzip -l` lists `ServerAuthHandler.class` and its `PendingJoin` inner class); WR-02 fix (`validating` guard, line 135/151/269) and WR-05 fix (response drain + `disconnect()`, line 180/187) both present in source and reflected in the currently-loaded `0.1.1` jar. |
| `mods-src/campfire-auth/.../network/AuthResponseMessage.java` | Client→server `{nick,token}`, bounded reads | ✓ VERIFIED | Present in jar; 02-02's own probe proved the wire format (discriminator byte) against a live Forge instance. |
| `scripts/join-probe.py` | Dependency-free login probe | ✓ VERIFIED | `ast.parse` clean; re-run live in this verification against `127.0.0.1:25565`, returned the same honestly-documented Forge-handshake refusal recorded in `docs/AUTH-OPS.md`. |
| `scripts/devserver.sh` | Disposable test server | ✓ VERIFIED (existence + syntax) | `bash -n` clean; not re-run in this verification (would spin up a second Forge process) — 02-02-SUMMARY documents its full live use. |
| `docs/AUTH-OPS.md` | Operator runbook | ✓ VERIFIED | Read in full: mint, reset, rollback (one file delete + one restart), no-bypass/RCON note, service-down guidance, nick inventory, enforcement-day record, pending Client verification section, support answers, link (not duplicate) to README. |
| `docs/CLIENT-SETUP.md` | Hand-install path with token flow | ✓ VERIFIED | Read in full: mod jar + two `-D` flags, exact-casing warning, bilingual kick message verbatim, single-use/12h token notes, Phase-4 stopgap framing. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `systemd/campfire-auth.service` | `/usr/local/bin/campfire-auth` | `ExecStart=... serve`, env from `server.env` | ✓ WIRED | Confirmed by `systemctl cat` output above; service `active`/`enabled` live. |
| `scripts/backup.sh` | `AUTH_DB` | `sqlite3 .backup` staged into the tar as `auth/campfire.db` | ✓ WIRED | Confirmed in script source (`backup.sh:103-114`); 02-01-SUMMARY documents a live archive containing both `world/level.dat` and `auth/campfire.db` that opened cleanly. |
| `auth-service/src/api.rs` | `auth-service/src/db.rs` | atomic `consumed_at IS NULL` compare-and-swap | ✓ WIRED | Confirmed in source and by this verification's live replay test (`/validate` first call → 200, second call with the same token → 401 `invalid_token`). |
| `server/mods/campfire-auth-0.1.1.jar` | `systemd/rlcraft.service` | installed jar, mod loaded, single announced restart | ✓ WIRED | Live: exactly one `campfire-auth-*.jar` in `server/mods/` (version `0.1.1`, matching the REVIEW-FIX deployment); `server/logs/latest.log` lists `campfireauth` in the FML mod list with zero exception lines. |
| `docs/AUTH-OPS.md` | `/usr/local/bin/campfire-auth` | `campfire-auth login`/`reset` CLI | ✓ WIRED | CLI binary present on `PATH` and its usage line matches what the doc describes. |

### Data-Flow Trace (Level 4)

Not broadly applicable — this phase's user-facing "data" is the token/nick round trip, which the live curl sequence directly exercised end-to-end (register → login issues a real token → validate consumes it from the real SQLite file → replay fails). No dashboard/UI rendering is in scope for this phase.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Register a fresh nick | `curl -X POST .../register` with a throwaway nick | `201` | ✓ PASS |
| Duplicate nick, different case | `curl -X POST .../register` with the same nick uppercased | `409 {"error":"nick_taken"}` | ✓ PASS |
| Wrong password | `curl -X POST .../login` with a bad password | `401 {"error":"invalid_credentials"}`, no `token` key | ✓ PASS |
| Correct login issues token | `curl -X POST .../login` with the right password | `200`, 43-char base64url token, `expires` ~12h out | ✓ PASS |
| Validate consumes token once | `curl -X POST .../validate` twice with the same token | first `200`, second `401 invalid_token` | ✓ PASS |
| DB holds only hashes | `sqlite3 auth/campfire.db` — no plaintext password/token found | `0` matches for both | ✓ PASS |
| Mod loaded on live server | `grep campfireauth server/logs/latest.log` | present in FML mod list, 0 exception lines | ✓ PASS |
| Live vanilla-protocol probe | `python3 scripts/join-probe.py 127.0.0.1 25565 ProbeNick` | disconnected — Forge's own FML mod-list message (honestly not our gate, matches docs/AUTH-OPS.md's own recorded result) | ✓ PASS (as a refusal) / see Truth #4 above for scope caveat |
| `cargo test` (auth-service) | `cargo test --release` | `0 tests` (no `#[cfg(test)]` modules — IN-01 from 02-REVIEW.md, an accepted Info-level gap, not fixed) | ? SKIP (no unit tests exist to run; behavior is covered by the live curl checks above instead) |
| Real client + valid token (Test A) | — | not run (no Minecraft client on the Pi) | ? SKIP → human_verification |
| Real modded client, no token (Test B) | — | not run (no Minecraft client on the Pi) | ? SKIP → human_verification |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| AUTH-01 | 02-01 | Register nick+password; nick uniqueness enforced; password hashed | ✓ SATISFIED | Live register/duplicate/hash-at-rest checks above. |
| AUTH-02 | 02-01 | Log in and receive a short-lived session token | ✓ SATISFIED | Live login/wrong-password checks above; token TTL ~12h confirmed. |
| AUTH-04 | 02-02, 02-03 | Server rejects any join with missing/invalid token; vanilla clients cannot join under a registered nick | ✓ SATISFIED (mechanism); real-client proof pending (see Truth #4 / human_verification) | Mod loaded live with no exception; fail-closed logic reviewed and confirmed unchanged post-fix; the mod-list-vs-gate distinction is the one gap needing a real client. |
| AUTH-05 | 02-02, 02-03 | Client-side auth mod ships in the modpack and transmits the launcher-provided token on join | ✓ SATISFIED (mechanism); real-client proof pending (see Truth #3 / human_verification) | Client classes present in the jar, wire format proven on 02-02's devserver; awaiting a real client on the live server. |

No orphaned requirements: `.planning/REQUIREMENTS.md`'s traceability table lists AUTH-01/02/04/05 as "Complete" for Phase 2, matching what all three plans declared in their `requirements:` frontmatter.

### Anti-Patterns Found

None blocking. Scanned the phase's key files (`main.rs`, `db.rs`, `auth.rs`, `api.rs`, `ratelimit.rs`, `ServerAuthHandler.java`, `AuthResponseMessage.java`, `NetworkHandler.java`, `ClientAuthHandler.java`) for `TODO`/`FIXME`/`HACK`/`XXX`/`TBD`/placeholder patterns and hardcoded-empty stubs — none found. The `IN-01`/`IN-02`/`IN-03` items from `02-REVIEW.md` are Info-level (no Rust unit tests, no explicit password upper bound, no clippy run) and were explicitly out of scope for `02-REVIEW-FIX.md`'s fix pass (only the 5 Warning-level findings were in scope, and all 5 are confirmed fixed and deployed live in this verification — see Key Link and Artifact sections above).

### Human Verification Required

See frontmatter `human_verification` — both items are Test A and Test B from `02-03-PLAN.md` Task 1's `<verify><human-check>`, carried through `docs/AUTH-OPS.md`'s "Client verification" section (currently empty/pending, exactly as documented). These are the only two gaps preventing `passed`:

1. **Test A — valid token, real client:** join, move, break a block, chat. Only fully proven on 02-02's throwaway devserver so far.
2. **Test B — registered nick, mod jar present, no token, real client:** must be kicked with the bilingual message before acting. The live-server probe run in this verification, like the one in 02-03-SUMMARY.md, was refused by Forge's own mod-list handshake, not campfireauth's gate — this is a documented, honest limitation of a raw-protocol probe against a ~200-mod server, not a defect. It leaves the live-server proof of *this project's own gate* resting entirely on a real modded client.

Test C (optional, a genuinely vanilla client) may be run at the same time; either outcome is acceptable per the plan.

### Gaps Summary

No code-level gaps. Every automatable check — the full register/login/validate/replay/rate-limit/at-rest-hash behavior, the live systemd state, the mod's presence and clean load in the live server log, and all 5 Warning-level code-review findings (WR-01 through WR-05) — passed against the live Pi, at the current `campfire-auth-0.1.1` jar and rebuilt Rust binary (post-`02-REVIEW-FIX.md`). The only remaining work is the phase's own explicitly-designed human checkpoint: a real modded RLCraft client proving the token-in/no-token-out round trip against the *live, fully-modded* server, which no probe or grep can substitute for. This is not a regression or an oversight — 02-03-PLAN.md and 02-03-SUMMARY.md both call this out by design (`human_verify_mode: end-of-phase`), and `docs/AUTH-OPS.md`'s "Client verification" section is deliberately left blank pending it.

---

_Verified: 2026-08-28_
_Verifier: Claude (gsd-verifier)_
