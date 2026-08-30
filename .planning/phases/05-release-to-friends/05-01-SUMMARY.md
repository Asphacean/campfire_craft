---
phase: 05-release-to-friends
plan: 01
subsystem: infra
tags: [github-actions, tauri, gitleaks, actionlint, ci-cd, minisign, release-pipeline]

# Dependency graph
requires:
  - phase: 04-launcher (04-04)
    provides: campfire-launcher-core/src-tauri workspace, scripts/publish-launcher.sh, the pi-only minisign keypair, tauri.conf.json's updater pubkey/createUpdaterArtifacts
provides:
  - "Full-history proof (three independent checks: gitleaks pattern/entropy scan, by-value search of every secret-shaped server.env variable across all patch text, and an ever-added-path audit) that this repository's whole history holds no credential, before a single byte is pushed anywhere"
  - "launcher/src-tauri/tauri.conf.json corrected: productName -> Campfire-Launcher (space-free, so publish-launcher.sh's feed URL never needs escaping) and bundle.macOS.signingIdentity -> \"-\" (ad-hoc signing, makes an Apple Silicon download launchable)"
  - ".github/workflows/ci.yml — D-06's every-push smoke gate: full-history gitleaks scan, cargo test/clippy (deny warnings) for campfire-launcher-core, cargo test for auth-service, bash -n / py_compile syntax gates, all actions SHA-pinned"
  - ".github/workflows/release.yml — D-04/D-05's tag-triggered 3-leg build matrix (windows-latest, macos-14, macos-15-intel) via tauri-apps/tauri-action, a throwaway-keypair updater-signing recipe that never touches the operator's real key, and a self-hosted publish job that calls scripts/publish-launcher.sh from the operator's real tree"
  - "scripts/release.sh — the one command that bumps the version in tauri.conf.json/Cargo.toml/Cargo.lock, commits, tags v<ver>, and pushes, with four guard refusals proven in a disposable clone"
  - "A real bundling build on this Pi (cargo tauri build -b deb) proving the throwaway-key recipe satisfies the updater-pubkey gate without any access to the real signing key"
affects: [05-02-release-to-friends, 05-03-release-to-friends]

# Actuals (#2632)
actuals:
  tokens: 4108
  tasks: 3
  commits: 3

tech-stack:
  added:
    - gitleaks 8.30.1 (linux_arm64, checksum-verified GitHub release binary, installed to ~/.local/bin)
    - actionlint 1.7.12 (linux_arm64, checksum-verified GitHub release binary, installed to ~/.local/bin)
  patterns:
    - "Throwaway CI-only updater signing key, generated inside the run via `cargo tauri signer generate --ci`, never uploaded, destroyed with the runner — the correction this plan makes to the locked --no-sign assumption (--no-sign would also disable the ad-hoc macOS codesign D-08 needs; the throwaway key satisfies the build-time pubkey gate while leaving ad-hoc signing intact)"
    - "Every third-party GitHub Action pinned to a 40-hex commit SHA with a trailing human-readable tag comment, verified via git ls-remote against each action's own repo"
    - "The publish job in release.yml never checks the repo out — it calls scripts/publish-launcher.sh at its absolute path in the operator's real working tree, because that script needs server.env, the pi-only signing key and the Caddy-served launcher-dist/ to be the real ones, not a runner's scratch copy"

key-files:
  created:
    - .github/workflows/ci.yml
    - .github/workflows/release.yml
    - .github/actionlint.yaml
    - .gitleaksignore
    - scripts/release.sh
  modified:
    - launcher/src-tauri/tauri.conf.json

key-decisions:
  - "Both gitleaks findings across full history (137→140 commits scanned) were the same false positive — the entropy-based generic-api-key rule tripping on the hyphenated word \"argon2-verify\" inside Phase 2 design-doc prose. Triaged by hand, not allowlisted on pattern alone; both fingerprints recorded in .gitleaksignore with the exact commit+line reasoning."
  - "Value-level check covered RCON_PASSWORD, CF_API_KEY, DDNS_API_TOKEN, LAUNCHER_SIGNING_KEY_PASSWORD (the two empty ones skipped, logged as such), the CA private key's first line, and the minisign private key's first line — all zero occurrences across every commit's full patch text on every ref. No credential of any kind was ever committed at any point in this project's history."
  - "actionlint required a repo-local .github/actionlint.yaml allowlisting the campfire-publish custom self-hosted label — actionlint has no way to validate an operator-defined runner label without an explicit config, and the plan's <verify> command runs actionlint with no other config path, so this file is load-bearing for that check to pass at all."
  - "dtolnay/rust-toolchain is pinned to the current HEAD commit of its own `stable` branch (not a version tag) — this action's own convention is that the branch name itself is the toolchain channel, and the branch's action.yml accepts an explicit `toolchain` input override, which is how `1.98.0` gets passed explicitly per RESEARCH Pitfall 5."
  - "cargo commands scripts/release.sh runs against launcher/Cargo.toml are executed from inside launcher/ (via a subshell cd), not from the repo root with --manifest-path — rustup's toolchain-file override is resolved from the current working directory, not from --manifest-path, so running from the repo root would silently pick up the apt-packaged 1.85.0 toolchain instead of the pinned 1.98.0."

patterns-established:
  - "Task-order dependency: filenames flow from tauri.conf.json's productName, so that fix came first in Task 2, before either workflow file was written — every filename acceptance criterion downstream (the artifact-naming contract with publish-launcher.sh's detect_platform()) depends on that ordering."

requirements-completed: [REL-01, REL-02]

coverage:
  - id: D1
    description: "Every secret this project holds is provably absent from all of the repository's history — checked by the actual value, not only by pattern — before anything is pushed anywhere"
    requirement: REL-01
    verification:
      - kind: other
        ref: "gitleaks detect --source . --log-opts=--all --redact --report-format json exits 0 with 0 findings after .gitleaksignore triage (both raw findings were the same argon2-verify false positive)"
        status: pass
      - kind: other
        ref: "Fixed-string search of git log -p --all output for every non-empty server.env PASSWORD|KEY|TOKEN|SECRET variable, the CA key's first line, and the minisign key's first line — all zero occurrences"
        status: pass
      - kind: other
        ref: "git log --all --pretty=format: --name-only --diff-filter=A | grep -cE '^(server\\.env|ca/.*-key\\.pem|auth/|.*\\.key)$' == 0; git check-ignore -q server.env and ca/campfire-ca-key.pem both exit 0"
        status: pass
    human_judgment: false
  - id: D2
    description: "A tag-triggered workflow builds the Windows x64 installer and both macOS bundles, and it never asks for, references, or could receive the operator's real signing key"
    requirement: REL-01
    verification:
      - kind: other
        ref: "actionlint .github/workflows/*.yml exits 0; grep -rniE 'campfire\\.key|LAUNCHER_SIGNING_KEY_PASSWORD|TAURI_SIGNING_PRIVATE_KEY_PATH' .github/workflows/ | wc -l == 0; grep -rn 'secrets\\.' .github/workflows/ | grep -vc 'secrets.GITHUB_TOKEN' == 0"
        status: pass
      - kind: other
        ref: "Every uses: line across both workflow files is SHA-pinned (7 of 7); grep -rniE 'pull_request|workflow_call|repository_dispatch' .github/workflows/ | wc -l == 0"
        status: pass
    human_judgment: true
    rationale: "The workflow YAML is validated statically (actionlint, grep gates) and the signing recipe is proven locally (D3 below), but no CI run has actually executed this matrix yet — that only happens once the repo is public and a tag is pushed, which is 05-02's job. Whether the real GitHub-hosted matrix schedules and completes successfully is unverifiable until then."
  - id: D3
    description: "The build's updater-signing gate is satisfied by a keypair generated inside the CI run and thrown away with the runner, so no signing secret exists to leak"
    requirement: REL-01
    verification:
      - kind: e2e
        ref: "With TAURI_SIGNING env vars absent beforehand (env | grep -c TAURI_SIGNING == 0), generated a throwaway keypair the same way release.yml does, exported it, and ran ~/.cargo/bin/cargo tauri build -b deb from launcher/ on this Pi: exit 0, produced Campfire-Launcher_0.1.0_arm64.deb, with the expected harmless \"key from TAURI_SIGNING_PRIVATE_KEY does not match the public key\" warning. Artifact deleted afterward; ~/.tauri/campfire.key untouched at mode 600."
        status: pass
    human_judgment: false
  - id: D4
    description: "Artifact filenames match, character for character, the patterns scripts/publish-launcher.sh already parses, and carry no character that would need escaping in the feed URL it writes"
    requirement: REL-02
    verification:
      - kind: other
        ref: "jq -r .productName tauri.conf.json == 'Campfire-Launcher'; grep -c ' ' on that output == 0"
        status: pass
    human_judgment: false
  - id: D5
    description: "Every third-party Action the pipeline uses is pinned to a 40-character commit SHA, not to a tag or branch that someone else can move"
    verification:
      - kind: other
        ref: "grep -rhcE '^\\s*(- )?uses:' .github/workflows/ == 7; grep -rhcE '^\\s*(- )?uses: [^@]+@[0-9a-f]{40}' .github/workflows/ == 7 — every reference is SHA-pinned"
        status: pass
    human_judgment: false
  - id: D6
    description: "A push to any branch runs the tests, the lints and a full-history secret scan, so the repository cannot silently drift back into an unsafe state after it goes public"
    requirement: REL-01
    verification:
      - kind: other
        ref: "grep -cE 'cargo (test|clippy)' ci.yml == 6 (>= 3); grep -c py_compile ci.yml == 1; grep -c 'bash -n' ci.yml == 1; grep -c gitleaks ci.yml == 9 (>= 1); clippy step already warning-free on this Pi, so -D warnings is a real gate, not an aspirational one"
        status: pass
    human_judgment: false
  - id: D7
    description: "One command bumps the version in every file that states it, commits, tags and pushes — the version is stated in one place and derived everywhere else"
    requirement: REL-02
    verification:
      - kind: e2e
        ref: "In a disposable clone of the real committed repo (git clone -q ., origin removed): scripts/release.sh 9.9.9 --no-push bumped tauri.conf.json + Cargo.toml + Cargo.lock (2 occurrences), committed as 'release: v9.9.9', tagged v9.9.9; re-running the same version (exit 4), a lower version 0.0.1 (exit 5), and a malformed version 1.2 (exit 2) each refused; a dirty tree also refused (exit 3); the real tree's version stayed 0.1.0 throughout"
        status: pass
    human_judgment: false

duration: ~15min
completed: 2026-08-30
status: complete
---

# Phase 5 Plan 1: Release-Safety Foundation Summary

**Full-history secret-scan proof, a tag-triggered 3-leg Tauri build matrix with a throwaway updater-signing key, and the one-command `scripts/release.sh` — all validated locally before a single byte leaves the Pi.**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-08-30T16:40:45Z (per STATE.md)
- **Completed:** 2026-08-30T16:52:00Z
- **Tasks:** 3
- **Files modified:** 6 (1 modified, 5 created)

## Accomplishments

- Proved, with three independent checks, that this repository's entire git history (140 commits) holds no credential: a full-history gitleaks pattern/entropy scan (both raw findings triaged as the same false positive and allowlisted with per-line justification), a by-value search of every secret-shaped `server.env` variable plus both private key files' first lines against every commit's full patch text (all zero), and an ever-added-path audit confirming `server.env`, `ca/*-key.pem`, `auth/`, and `*.key` were never committed at any point
- Corrected `launcher/src-tauri/tauri.conf.json`'s `productName` (space-free `Campfire-Launcher`) and added `bundle.macOS.signingIdentity: "-"` — both load-bearing for the artifact-naming contract `scripts/publish-launcher.sh` already parses and for Apple Silicon ad-hoc launchability
- Wrote `.github/workflows/ci.yml` (every-push smoke: full-history gitleaks, cargo test/clippy with deny-warnings, auth-service tests, shell/python syntax gates) and `.github/workflows/release.yml` (tag-triggered `windows-latest`/`macos-14`/`macos-15-intel` matrix via `tauri-apps/tauri-action`, plus a self-hosted publish job wired to the operator's real tree), every third-party action pinned to a 40-hex commit SHA
- Resolved the updater-pubkey CI blocker with a throwaway keypair generated inside each CI run rather than `--no-sign` (which would also silently disable D-08's ad-hoc macOS codesign) — proved the whole recipe on real hardware: a `cargo tauri build -b deb` with no real key in the environment exited 0 and produced a `.deb`, with exactly the harmless key-id-mismatch warning the plan's objective predicted
- Wrote `scripts/release.sh`, the one command that bumps the version in `tauri.conf.json`/`Cargo.toml`/`Cargo.lock`, commits, tags, and pushes, refusing four kinds of mistake before touching anything — proved end to end in a disposable clone of the real committed repo

## Task Commits

1. **Task 1: Prove the history holds no credential** - `09f7748` (chore)
2. **Task 2: The release pipeline, written and proven on this Pi** - `5ab7d00` (feat)
3. **Task 3: One command cuts a release** - `b12c4e8` (feat)

_No TDD tasks in this plan; each commit is a single atomic change._

## Files Created/Modified

- `.gitleaksignore` - Two allowlisted findings (same argon2-verify false positive), each with a per-line justification
- `.github/workflows/ci.yml` - Every-push smoke gate: gitleaks, cargo test/clippy, auth-service tests, shell/python syntax checks
- `.github/workflows/release.yml` - Tag-triggered 3-leg build matrix + self-hosted publish job
- `.github/actionlint.yaml` - Allowlists the `campfire-publish` custom self-hosted runner label
- `launcher/src-tauri/tauri.conf.json` - `productName` → `Campfire-Launcher`; added `bundle.macOS.signingIdentity: "-"`
- `scripts/release.sh` - The one-command version bump + commit + tag + push, with four guard refusals

## Decisions Made

See `key-decisions` in frontmatter — five decisions recorded there, all made during execution to satisfy the plan's acceptance criteria and threat model without deviating from the locked design.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `.github/actionlint.yaml` was not in the plan's file list but is required for the plan's own `<verify>` command to pass**
- **Found during:** Task 2 verification
- **Issue:** `actionlint` has no built-in knowledge of the operator-defined `campfire-publish` self-hosted runner label and fails with an "unknown label" error without an explicit allowlist config
- **Fix:** Added `.github/actionlint.yaml` with a `self-hosted-runner.labels` entry for `campfire-publish`
- **Files modified:** `.github/actionlint.yaml` (new)
- **Verification:** `actionlint .github/workflows/ci.yml .github/workflows/release.yml` now exits 0
- **Committed in:** `5ab7d00` (Task 2 commit)

**2. [Rule 1 - Bug] Task 2's own explanatory comments tripped its own acceptance-criteria greps**
- **Found during:** Task 2 verification
- **Issue:** Comments explaining *why* `macos-13`, `campfire.key`, and `pull_request`/`workflow_call`/`repository_dispatch` are absent literally mentioned those strings, so the negative-match grep gates (checking those strings are absent) failed against the workflow files' own prose
- **Fix:** Reworded the three comments to explain the same reasoning without repeating the literal banned strings
- **Files modified:** `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- **Verification:** All three grep gates (`macos-13`, the real-key-reference grep, the fork-trigger grep) now report 0
- **Committed in:** `5ab7d00` (Task 2 commit)

**3. [Rule 1 - Bug] Plan's own Task 1 `<verify>` command had a shell bug**
- **Found during:** Task 1 verification
- **Issue:** `git check-ignore -q server.env ca/campfire-ca-key.pem` — `-q` is documented by git to only be valid with a single pathname, so the combined invocation exits 128 (`fatal: --quiet is only valid with a single pathname`) regardless of whether the ignore rules are correct
- **Fix:** No code fix needed (nothing in the repo was wrong) — verified each path individually (`git check-ignore -q server.env; git check-ignore -q ca/campfire-ca-key.pem`), both exit 0, confirming the substance of the check passes; the plan's verify line itself is what's malformed
- **Files modified:** none
- **Verification:** Both individual invocations exit 0, matching-rule output confirmed via `-v`
- **Committed in:** n/a (no repo change; documented here for the record)

---

**Total deviations:** 3 auto-fixed (1 blocking/missing-config, 1 bug in own comments, 1 bug noted in the plan's verify script itself — no repo change needed)
**Impact on plan:** All three were necessary to make the plan's own acceptance criteria pass or to correctly interpret a criterion whose literal command was broken. No scope creep — nothing outside Task 1/2/3's stated files was touched.

## Issues Encountered

None beyond the deviations above. The `cargo tauri build -b deb` bundling spike (bounded to ≤30 min per the environment note) completed in about 1m 40s total, well within budget.

## User Setup Required

None - no external service configuration required. (The repo creation, runner registration, and first push are explicitly 05-02's job, not this plan's.)

## Next Phase Readiness

- 05-02 can proceed: the repository is provably clean of credentials, both workflow files are actionlint-clean and SHA-pinned, and the signing recipe is proven on real hardware — the only remaining unknowns (whether `macos-15-intel` schedules as a free standard runner, whether the `bundle_dmg.sh` flake needs a manual re-run) are empirical questions that only a real CI run in 05-02 can answer, exactly as RESEARCH.md anticipated
- `scripts/release.sh` is ready for 05-02's first real invocation (`v0.1.0`, D-12) — every guard has been proven against the real committed repository
- No blockers. `rlcraft.service` and `caddy` remained active throughout; nothing was pushed anywhere

---
*Phase: 05-release-to-friends*
*Completed: 2026-08-30*

## Self-Check: PASSED

All created/modified files confirmed present on disk (`.gitleaksignore`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.github/actionlint.yaml`, `scripts/release.sh`, `launcher/src-tauri/tauri.conf.json`, this SUMMARY). All three task commit hashes (`09f7748`, `5ab7d00`, `b12c4e8`) confirmed present in git log.
