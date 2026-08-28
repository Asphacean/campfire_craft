---
phase: 03-modpack-distribution
verified: 2026-08-28T18:52:00Z
status: human_needed
score: 5/5 must-haves verified
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Phone-in-hand certificate-warning UX check (ROADMAP success criteria 1, DIST-01 'from the domain')"
    expected: "On a phone with Wi-Fi off (mobile data only), opening https://mc.campfire.pub:8444/manifest.json shows a certificate warning (expected — the CA is private and only the launcher pins it), then a wall of JSON beginning with pack_version after tapping through."
    why_human: "03-03-SUMMARY.md records this exact check was never performed — the operator substituted external automated probes (check-host.net) for it. That evidence is strong for raw TCP/HTTP reachability, but nobody has visually confirmed what a real friend's phone browser actually shows for this certificate. Re-verified independently in this session (3/3 check-host.net TCP nodes from Iran/USA connect to 91.193.195.130:8444), which further confirms reachability but still does not substitute for the human-eyes UX check the plan asked for."
  - test: "A client assembled from the manifest connects and plays a full session on the live server (ROADMAP success criterion 3)"
    expected: "A real Minecraft 1.12.2/Forge client, built from nothing but the manifest and pinned CA, launches and joins mc.campfire.pub and plays without error."
    why_human: "Explicitly and deliberately deferred by operator decision D-13 until Phase 4's launcher exists — no desktop Minecraft client exists yet to perform this check. The automated half (assemble-client.py building and hash-verifying an identical 3545-file/367,531,501-byte tree from the manifest over the pinned CA) was independently re-run and re-verified in this session: ASSEMBLE OK then VERIFY OK, both exact matches to the manifest's file count and byte total."
---

# Phase 3: Modpack Distribution Verification Report

**Phase Goal:** The exact pack the server runs is fetchable over HTTPS with per-file hashes, so any client can be brought into sync
**Verified:** 2026-08-28T18:52:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

**Note on ROADMAP mode:** `.planning/ROADMAP.md` marks this phase `mode: mvp`, but the phase goal is not in `As a / I want to / so that` User Story form — confirmed programmatically (`gsd-tools query user-story.validate` returns `valid: false`). All three PLAN.md files for this phase explicitly flag this same discrepancy themselves ("this Goal line is not in ... user-story form, and nothing has been invented in its place; run `/gsd mvp-phase 3` first if a real user story is wanted") and proceed anyway against the ROADMAP's five literal Success Criteria. This verification follows the same path: standard goal-backward verification against the five ROADMAP Success Criteria, not MVP-mode User Flow Coverage. This is an informational finding, not a blocker — the mode/goal-format mismatch predates this phase's plans and was a deliberate, disclosed choice, not a gap introduced here.

## Goal Achievement

### Observable Truths

| # | Truth (ROADMAP Success Criterion) | Status | Evidence |
|---|---|---|---|
| 1 | Manifest over HTTPS from the domain returns path+sha256+size for every managed file, with a valid (own-CA) cert | ✓ VERIFIED | Live: `curl --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/manifest.json` → HTTP 200, 3545 files, 0 delete, `mc=1.12.2 forge=14.23.5.2860 java=8`. `openssl verify -CAfile ca/campfire-ca.pem ca/mc.campfire.pub-cert.pem` → OK; leaf SAN = `DNS:mc.campfire.pub`, not-after 2028-08-27; CA good 10+ years. Sampled 6 manifest entries (index 0, 100, 500, 1000, 2000, 3000) downloaded over the live HTTPS front and hashed — all 6 matched the manifest's published sha256 exactly. |
| 2 | Operator changes a mod/config, runs one command, new hashes live | ✓ VERIFIED | Live re-run of `bash scripts/publish-pack.sh --skip-fetch`: exit 0, re-generated manifest (3545 files, 0 delete, only `pack_version` changed — confirmed via `md5sum` before/after), and the served `/manifest.json` reflected the new `pack_version` on the next fetch. One command, no other step. |
| 3 | Client assembled from manifest connects and plays (human-verified once, deferred) | ⚠️ Automated half VERIFIED; human half deferred | `python3 scripts/assemble-client.py --dest <scratch>` → `ASSEMBLE OK — 3545 files, 367531501 bytes`; `--verify` on the same tree → `VERIFY OK`, identical counts, 0 symlinks, 0 `minecraft*.jar`/`libraries/`/`assets/`/`versions/` paths. Independently re-run and reconfirmed in this session (not just trusted from SUMMARY). The "connects and plays" half is explicitly deferred by operator decision D-13 until Phase 4's launcher exists — routed to human verification below, matching the plan's own documented deferral. |
| 4 | Minecraft jars, libraries and assets never served from our host; only license-permitting mods/configs are | ✓ VERIFIED | `jq -r '.files[].path'` over the live manifest: 0 matches for `^(libraries|assets|versions)/`, 0 matches for `minecraft.*\.jar` (case-insensitive). `assemble-client.py` enforces the same as a hard gate (exits non-zero on violation) — this is DIST-03 as the operator restated it (self-hosted mods/configs accepted; only the Mojang-sourced Minecraft jar/libraries/assets must never be mirrored), matching `.planning/REQUIREMENTS.md`'s DIST-03 wording verbatim. |
| 5 | Status endpoint reports online/offline + players | ✓ VERIFIED | Live: `curl --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/status` → `{"online":true,"players":0,"max":10,"motd":"campfire.pub"}`, HTTP 200, exactly 4 keys, well under 512 bytes. |

**Score:** 5/5 truths verified (criterion 3's automated half fully re-verified live; its human-play-test half is a deliberately deferred human check, not a failure).

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `caddy/Caddyfile` | Whole public HTTPS surface, own-CA TLS, write guard, 3 proxied routes, pack file_server, terminal 404 | ✓ VERIFIED | Read in full; matches SUMMARY exactly including the CR-01 fix comment. `caddy validate` implied passing (service active, live routes all behave as specified). |
| `scripts/renew-cert.sh` | Creates CA once, reissues leaf every run | ✓ VERIFIED | Live cert inspected: root good 10yr+, leaf SAN correct, chain verifies. |
| `ca/campfire-ca.pem` | Public trust anchor | ✓ VERIFIED | Tracked in git, present on disk, `openssl verify` OK. |
| `scripts/publish-pack.sh` | Single DIST-02 operator command | ✓ VERIFIED (with CR-02 fix confirmed) | `set -euo pipefail` confirmed present (line 32) — the reviewed `set -uo pipefail` regression from 03-REVIEW.md is fixed. Live re-run: exit 0, correct idempotent behavior. |
| `scripts/gen-manifest.py` | Hashing, validation, forbidden-content gate, delete diff, atomic write | ✓ VERIFIED (with CR-01 fix confirmed) | `islink()` hard-fail gate present at line 78-81, matching the reviewed fix exactly. Live manifest has 0 symlink-derived entries; `find pack -type l` → 0. |
| `scripts/assemble-client.py` | Manifest-driven download/verify over pinned CA, reference implementation | ✓ VERIFIED (with WR-02 fix confirmed) | Required-field guard present at lines 118-121 exactly as reviewed. Live full assemble+verify cycle re-run in this session: exact match. |
| `auth-service/src/slp.rs` | Hand-rolled SLP client with bounded allocation | ✓ VERIFIED (with WR-01 fix confirmed) | `MAX_SLP_STRING = 64 * 1024` and the bound check present at lines 28, 119-121 exactly as reviewed. |
| `docs/DIST-OPS.md` | Operator runbook + Phase 4 integration contract | ✓ VERIFIED | 12 `## ` sections (≥7 required), includes "Phase 4 integration contract" section, names `campfire-ca.pem` 8 times, states TLS pinning is "the only trust anchor" and hashes alone don't defend against impersonation (line 307). |
| `docs/CLIENT-SETUP.md` | Hand-install guide updated for file server existence | ✓ VERIFIED | Contains "8444", a new "§7 The file server (for the curious, not a download page)" section; still names the CurseForge app as the supported manual path. |
| `pack/` tree | Complete RLCraft 2.9.3 client, no traversal/symlink hazards | ✓ VERIFIED | `find pack -type l` → 0. 3545 manifest entries, all sampled hashes match. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `caddy/Caddyfile` | `127.0.0.1:8081` | `reverse_proxy` for register/login/status | ✓ WIRED | Live: `/api/login` reaches the real auth service; `/api/validate` (deliberately unproxied) → 404. |
| `caddy/Caddyfile` | `pack/` tree | `file_server` via `handle_path /pack/*` | ✓ WIRED | Live: manifest-listed file downloads correctly through this route with matching hash; non-GET/HEAD → 405; directory path → 404 with no listing. |
| `scripts/publish-pack.sh` | `scripts/gen-manifest.py` | final step of the one-command pipeline | ✓ WIRED | Live re-run confirmed manifest regeneration is the terminal step of the single command, exit code propagates (CR-02 fix confirmed — `set -e` restored). |
| `scripts/assemble-client.py` | `ca/campfire-ca.pem` | SSL context built from CA file only | ✓ WIRED | Live assemble+verify succeeded trusting only the pinned CA (per SUMMARY, confirmed unable to fall back to system trust store — not independently re-tested with `--cacert` swap in this session, but the SUMMARY's live evidence plus the current code's unchanged `ssl.create_default_context(cafile=...)` construction is consistent). |
| `scripts/reachability.sh --https` | resolved public IP | `curl --resolve` bypassing `/etc/hosts` | ✓ WIRED | Live re-run in this session: `VERDICT: PASS`, exit 0 — confirms the router forward is live and the check design (forcing the public IP) is real, not a local-only pass. |

### External Reachability (independently re-verified this session)

| Check | Result |
|---|---|
| `ss -tlnp` — externally-relevant listeners | `:80`, `:443`, `:8443` (pbwiki/sing-box, untouched), `:25565` (game server), `:8444` (Caddy). Admin port `:2019` — 0 listeners. |
| check-host.net TCP probe, 91.193.195.130:8444, 3 nodes (Iran x2, USA) | 3/3 succeeded (response times 0.095–0.108s) — independently reconfirms the SUMMARY's own external-vantage-point claim, not merely trusted from the document. |
| `systemctl is-active caddy campfire-auth rlcraft` | All `active` |
| `uptime -s` | `2026-08-22 20:53:29` — unchanged throughout this verification session |
| `docker ps` container count | 5 — unchanged |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| DIST-01 | 03-01, 03-02, 03-03 | File server serves modpack + manifest with path/sha256/size over HTTPS | ✓ SATISFIED | Live manifest fetch, hash-verified sample, valid own-CA cert chain. |
| DIST-02 | 03-02 | Manifest regenerated by one command | ✓ SATISFIED | Live `publish-pack.sh --skip-fetch` re-run, exit 0, correct idempotent output. |
| DIST-03 | 03-02, 03-03 | Minecraft jar/libraries/assets never mirrored; self-hosted mods/configs accepted per operator's 2026-08-28 restatement | ✓ SATISFIED | Live manifest grep: 0 forbidden paths. `assemble-client.py` enforces as a hard gate. REQUIREMENTS.md documents the restated form explicitly, matching the task brief's framing. |
| DIST-04 | 03-01 | Status endpoint online/offline + players | ✓ SATISFIED | Live `/status` returns exactly the documented 4-field shape. |

No orphaned requirements found — REQUIREMENTS.md traceability table lists exactly DIST-01..04 against Phase 3, and all four appear in at least one plan's `requirements:` frontmatter.

### Code Review Fix Verification (03-REVIEW.md → 03-REVIEW-FIX.md)

All four in-scope findings from `03-REVIEW.md` (2 critical, 2 warning) were independently re-checked against the current code in this session, not merely trusted from `03-REVIEW-FIX.md`'s narrative:

| Finding | Fix location | Verified present in current code |
|---|---|---|
| CR-01 (symlink bypasses forbidden-content gate, served publicly) | `scripts/gen-manifest.py:78-81`, `scripts/publish-pack.sh:344-346` | ✓ Confirmed: `islink()` hard-fail gate and `find -type l -delete` strip step both present; live pack tree has 0 symlinks. |
| CR-02 (`set -e` dropped, unchecked rsync/cp) | `scripts/publish-pack.sh:32` | ✓ Confirmed: `set -euo pipefail` present (the reviewed regression is fixed). |
| WR-01 (unbounded SLP allocation) | `auth-service/src/slp.rs:28,119-121` | ✓ Confirmed: `MAX_SLP_STRING = 64 * 1024` bound check present before the allocation. |
| WR-02 (uncaught KeyError on malformed manifest entry) | `scripts/assemble-client.py:118-121` | ✓ Confirmed: required-field guard present, exits 2 with a clean message. |

Info-level findings IN-01 (redundant `header_up`) and IN-02 (login rate-limit check ordering) were explicitly left unfixed by the fix run as out-of-scope/optional — this is correct per the review's own severity classification (info, not warning/critical) and does not block phase goal achievement.

### Anti-Patterns Found

Searched all phase-modified scripts and Caddyfile for `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER`: no matches except one benign `mktemp ... .XXXXXX.log` template string (not a debt marker — `XXXXXX` here is `mktemp`'s own placeholder-character convention, not an unresolved-work marker). No blockers found.

### Behavioral Spot-Checks / Probe Execution

No formal `scripts/*/tests/probe-*.sh` convention exists for this phase; the phase's own PLAN.md `<verify><automated>` blocks function as its probes and were re-run live in this session rather than trusted from SUMMARY narration:
- Full manifest fetch + hash verification of 6 sampled files: PASS
- `publish-pack.sh --skip-fetch` one-command re-publish: PASS (exit 0, idempotent)
- `assemble-client.py` full assemble + verify cycle: PASS (ASSEMBLE OK, VERIFY OK, exact byte/file-count match)
- `reachability.sh --https`: PASS (VERDICT: PASS, exit 0)
- Method guard (`PUT` → 405), terminal 404 (`/api/validate` → 404), plaintext HTTP (→ 400, no content served): PASS
- External TCP reachability (check-host.net, 3 independent nodes): PASS

### Human Verification Required

1 and 2 above (frontmatter `human_verification`) — both are pre-existing, explicitly-disclosed deferrals from the phase's own plans (D-13 for the play test; the operator's own substitution decision for the phone check), not gaps introduced by incomplete work. Both have strong automated evidence already in place; what remains is specifically the human-eyes/human-hands part.

### Gaps Summary

No gaps found. All five ROADMAP success criteria are met by live, independently-reconfirmed evidence (not merely trusted from SUMMARY.md narration), all four DIST requirements are satisfied, and all four in-scope code-review findings (2 critical, 2 warning) from `03-REVIEW.md` are confirmed fixed in the current codebase. The phase goal — "the exact pack the server runs is fetchable over HTTPS with per-file hashes, so any client can be brought into sync" — is demonstrably true today from the Pi and from three independent external vantage points.

Two items route to `human_needed` rather than `passed`: the phone-in-hand certificate-warning visual check (never performed; substituted with stronger but different automated evidence) and the real-client play test (deliberately deferred by operator decision D-13 until Phase 4's launcher exists). Neither blocks Phase 4 planning — `docs/DIST-OPS.md`'s Phase 4 integration contract and `auth-service/README.md`'s route table give Phase 4 everything it needs without re-deriving anything, and both deferred items are already recorded for end-of-phase/end-of-Phase-4 harvest per the plans' own design.

---

_Verified: 2026-08-28T18:52:00Z_
_Verifier: Claude (gsd-verifier)_
