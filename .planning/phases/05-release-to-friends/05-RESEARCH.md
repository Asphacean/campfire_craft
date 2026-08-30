# Phase 5: Release to Friends - Research

**Researched:** 2026-08-30
**Domain:** GitHub Actions CI/CD for a pure-Rust/Cargo Tauri 2 app (no npm), cross-platform release builds, self-hosted runner publish step, minisign-signed update feed, secret-scanning a repo before first public push
**Confidence:** MEDIUM-HIGH — CI mechanics and package legitimacy verified directly against crates.io/GitHub APIs and official docs; two locked CONTEXT.md assumptions turned out to be stale/incomplete and are corrected below (flagged, not silently overridden)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Repository & CI
- New GitHub repository **`campfire-pub/rlcraft`**, **public**, containing the whole `~/rlcraft` tree as-is (secrets already gitignored: `server.env`, `ca/campfire-ca-key.pem`, `auth/`, `pack/`, `launcher-dist/`, keys). Pushed from the Pi over SSH (key `~/.ssh/id_ed25519`, GitHub user Asphacean). A pre-push secret scan (grep for `RCON_PASSWORD=`, `BEGIN .* PRIVATE KEY`, `.key` files) runs before the first push
- Public visibility chosen for unlimited GitHub-hosted macOS/Windows minutes; no secrets in code; domain/IP are already public
- Build matrix via `tauri-apps/tauri-action` on tag `v*`: `windows-latest` → x64 NSIS `.exe`; `macos-14` → aarch64 `.dmg`; `macos-13` → x86_64 `.dmg`. Artifacts + Tauri updater `.sig` files attached to a GitHub Release *(see this research's Summary/Pitfall 2 — `macos-13` is retired, `macos-15-intel` is the corrected replacement)*
- Publish job on self-hosted runner **`rpi5-1`** (register a runner for the new repo in addition to the existing registration): downloads the release assets and runs `scripts/publish-launcher.sh` so `latest.json` + artifacts land on `https://mc.campfire.pub:8444/launcher/`. minisign private key stays pi-only (never in GitHub Secrets) — the Pi job signs
- CI smoke on every push (ubuntu-latest): `cargo test --workspace` + `cargo clippy` for `launcher/` (rustup toolchain from rust-toolchain.toml), `cargo test` for `auth-service/`, `bash -n scripts/*.sh`, `python3 -m py_compile scripts/*.py`

#### Artifacts & bypass instructions
- Windows: NSIS `.exe`, per-user install (no admin). Friends doc: SmartScreen → "More info → Run anyway"
- macOS: unsigned `.dmg` (REL-02) **plus ad-hoc `codesign --sign -`** in CI to avoid the "damaged" variant *(see this research's Pitfall 3 — ad-hoc signing is required for Apple Silicon to launch at all, but does not by itself avoid the "damaged" dialog; the doc workaround below is still required)*; doc: right-click → Open → Open Anyway, or `xattr -cr "/Applications/Campfire Launcher.app"`; Rosetta installs on prompt for the x86_64 Java
- Naming: app **"Campfire Launcher"**, bundle id `pub.campfire.launcher`; artifacts `Campfire-Launcher_<ver>_x64-setup.exe`, `Campfire-Launcher_<ver>_aarch64.dmg`, `Campfire-Launcher_<ver>_x64.dmg`; version source of truth `launcher/src-tauri/tauri.conf.json`; `scripts/release.sh <ver>` bumps version, commits, tags `v<ver>`, pushes
- Friend-facing page: `docs/FRIENDS.md` (English) linking to GitHub Releases "latest"; repo README summarises. No human-facing links to `:8444` (private-CA warning)

#### Verification & first release
- REL-03: human check on the operator's Apple Silicon Mac with the release `.dmg`, per `docs/LAUNCHER-BUILD.md` QA matrix; same session closes deferred UATs of Phases 1–4 (01/02/03/04-UAT.md). Intel macOS = built in CI, unverified (no hardware) — recorded honestly
- First release `v0.1.0` cut as soon as CI is green (operator decision) — QA happens on the release artifacts

### Claude's Discretion
- tauri-action version pin, Rust cache action, runner registration mechanics for the second repo (new runner dir vs `config.sh` multi-registration), release notes template, whether Intel build stays in the matrix if it fails repeatedly

### Deferred Ideas (OUT OF SCOPE)
- Apple Developer signing/notarization (REL-04, v2)
- Windows code signing certificate
- Linux launcher build
</user_constraints>

## Summary

This phase turns the already-built `launcher/` Cargo workspace (Tauri 2.11.5, tauri-cli 2.11.4, no npm anywhere in the project) into a `campfire-pub/rlcraft` public GitHub repo with a release pipeline. The mechanics are well-trodden (`tauri-apps/tauri-action` driving `tauri build` per matrix leg, GitHub Release with attached artifacts, a self-hosted publish job that re-signs with the pi-only minisign key) — but two locked pieces of CONTEXT.md are no longer accurate as of today and must be corrected before planning:

1. **`macos-13` is retired.** GitHub fully retired the macOS 13 runner image on **December 4, 2025** (brownout warnings ran through November 2025). The x86_64/Intel matrix leg must use **`macos-15-intel`** instead — a standard (non-"larger") GitHub-hosted label, free for public repos, supported through roughly August 2027 (Apple's last Intel-capable macOS runner generation).
2. **Ad-hoc codesigning (`signingIdentity: "-"`) does not, by itself, "avoid the damaged variant."** It is a hard *requirement* for the app to launch at all on Apple Silicon (Apple mandates signing — even ad-hoc — for all internet-downloaded ARM64 binaries), but it does **not** suppress Gatekeeper's quarantine dialog. Friends still need the `xattr -cr` / right-click-Open workaround that CONTEXT.md already documents for `docs/FRIENDS.md` — so the doc plan is unaffected, only the stated *reason* for ad-hoc signing needs correcting (it's "makes it launchable," not "avoids the damaged prompt").

A third finding is more structural and needs an explicit operator decision before planning: **the shared `launcher/src-tauri/tauri.conf.json` already has `plugins.updater.pubkey` set with `bundle.createUpdaterArtifacts: true`.** Tauri's CLI treats the mere presence of a configured public key as a signal that it must find a matching private key at build time — on *every* platform, for *every* bundle target, not just updater artifacts — and refuses to build without one (`tauri-apps/tauri#14581`, filed Nov 2025). Left unaddressed, this would make every CI-hosted build leg fail outright, because the private key is deliberately pi-only and never goes into GitHub Actions. The good news: this exact bug was fixed and shipped in **tauri-cli 2.9.5** (merged PR #14582, Nov 30, 2025) — our pinned 2.11.4 is well past that fix, so `--no-sign` should now correctly skip the private-key check on every platform without needing any key present in CI at all. This is MEDIUM confidence (the fix's exact scope — whether the macOS `.app.tar.gz` updater-artifact bundle is still produced *unsigned*, vs. skipped outright — is not explicitly documented) and should be the very first thing a Wave-0/spike task in this phase confirms, since `publish-launcher.sh`'s `detect_platform()` requires that exact `.app.tar.gz` filename to exist for macOS legs.

**Primary recommendation:** Build the matrix with `tauri-apps/tauri-action@v1` + `tauriScript: tauri` (cargo-installed `tauri-cli`, pinned to `--version 2.11.4 --locked`) across `windows-latest`, `macos-14` (aarch64), and `macos-15-intel` (x86_64, replacing the retired `macos-13`) — passing `--no-sign` as a build arg on every leg and zero minisign secrets in GitHub Actions. Verify with a single-leg spike build early in the phase that `--no-sign` still yields the unsigned `.app.tar.gz` publish-launcher.sh needs; if it instead skips the updater artifact entirely, the fallback is a disposable, openly-committed (non-secret) CI-only minisign keypair whose output signature is discarded and never uploaded (`uploadUpdaterSignatures: false`), which satisfies the locked "never in Actions secrets" constraint by not being a secret in the first place.

## Architectural Responsibility Map

This phase is CI/CD and distribution infrastructure, not a client-server web app — the standard Browser/API/DB tiers don't map cleanly. Adapted tiers for this domain:

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Repo creation, secret scan, initial push | Source Control (GitHub) | Operator (Pi shell) | GitHub is system of record; the Pi is where the push originates and where the pre-push scan runs |
| Cross-platform bundle build (Windows/macOS) | CI — GitHub-hosted runners | — | Tauri cannot cross-compile OS-native bundlers from Linux; must run on the actual target OS, which only GitHub-hosted runners provide here |
| CI smoke (lint/test on every push) | CI — GitHub-hosted (ubuntu-latest) | — | Fast feedback loop, no OS-native bundling involved, cheapest runner suffices |
| Release creation + artifact upload | CI — GitHub-hosted (same job) | GitHub Releases (storage) | `tauri-action` both builds and uploads in one job |
| Signing (real minisign key) | Self-hosted (Pi, `rpi5-1`) | — | Locked decision: key never leaves the Pi; publish job downloads release assets and signs there |
| Update-feed hosting (`latest.json`) | Self-hosted (Caddy on Pi) | — | Existing `/launcher/` route from Phase 4, unchanged by this phase |
| Friend-facing install docs | Docs (`docs/FRIENDS.md`, README) | — | Static content, no runtime component |

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|--------------|---------|-------------|
| `tauri-cli` | crates.io | since 2019-11-27 | ~25.3k/week | github.com/tauri-apps/tauri | OK [VERIFIED: crates.io, package-legitimacy seam] | Approved — pin `--version 2.11.4 --locked` to match `launcher/rust-toolchain.toml`/`Cargo.toml` project pin |
| `tauri-apps/tauri-action` | GitHub Action (Marketplace) | maintained since Tauri v1 era, official `tauri-apps` org | high (thousands of repos) | github.com/tauri-apps/tauri-action | OK [CITED: github.com/tauri-apps/tauri-action] | Approved — pin to `@v1` (major version tag), not `@dev`/`@main` |
| `dtolnay/rust-toolchain` | GitHub Action | maintained since 2021, dtolnay is a well-known Rust org member (serde/tokio-adjacent) | very high | github.com/dtolnay/rust-toolchain | OK [CITED: github.com/dtolnay/rust-toolchain] | Approved — note: does NOT auto-read `rust-toolchain.toml`; must pass `toolchain: 1.98.0` explicitly or switch to `actions-rust-lang/setup-rust-toolchain` which does auto-detect |
| `Swatinem/rust-cache` | GitHub Action | maintained since 2021 | very high, de-facto standard | github.com/Swatinem/rust-cache | OK [CITED: github.com/Swatinem/rust-cache] | Approved |
| `gitleaks` | GitHub Releases (binary) | maintained since 2019, `linux_arm64` build present | very high (60k+ GitHub stars) | github.com/gitleaks/gitleaks | OK [VERIFIED: api.github.com/repos/gitleaks/gitleaks/releases/latest — `v8.30.1`] | Approved — download the pinned `linux_arm64` tarball checksum-verified from the GitHub release, not a random mirror |
| `gh` (GitHub CLI) | apt (GitHub's own `cli.github.com/packages` repo) | official first-party GitHub product | — | github.com/cli/cli | OK [CITED: cli.github.com official install docs] | Approved — only needed if the operator wants a scripted repo-create/`gh auth login`; the manual "create repo in the web UI" path avoids installing anything |

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** none.

*A throwaway CI-only minisign keypair (if the phase ends up needing the fallback described in the Summary) is not a "package" — it's a locally-generated Ed25519 keypair via `tauri signer generate`, committed as plaintext (not a secret) if used at all. No registry check applies.*

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|---------------|
| `tauri-apps/tauri-action` | `@v1` (major tag; resolves to latest v1.x) [CITED: github.com/tauri-apps/tauri-action] | Drives `tauri build` per matrix leg and creates/updates the GitHub Release with artifacts | The maintained, official action for exactly this job — avoids hand-rolling `cargo tauri build` + manual `actions/upload-release-asset` steps |
| `tauri-cli` (crates.io) | `2.11.4` [VERIFIED: crates.io, matches project's already-pinned CLI version per 04-04-SUMMARY.md] | The actual `tauri build`/`tauri signer` binary, installed via `cargo install tauri-cli --version 2.11.4 --locked` | Project has no npm anywhere; crates.io install matches the existing `--locked`, no-npm convention already established in Phase 4 |
| `dtolnay/rust-toolchain@stable` (with explicit `toolchain: 1.98.0`) | latest (unversioned tag; pin toolchain input, not action version) [CITED: github.com/dtolnay/rust-toolchain] | Installs the exact Rust toolchain per matrix leg | Minimal, single-purpose action; matches `launcher/rust-toolchain.toml`'s explicit `1.98.0` pin (real Tauri 2.11 MSRV is 1.88, not the declared 1.77.2 — Phase 4's own Pitfall 1) |
| `Swatinem/rust-cache@v2` | v2 [CITED: github.com/Swatinem/rust-cache] | Caches `~/.cargo` + `target/` keyed on lockfile + rustc version | Standard, avoids multi-minute full rebuilds on every CI run across 3 build legs + 1 smoke leg |
| `gitleaks` | `v8.30.1` [VERIFIED: api.github.com/repos/gitleaks/gitleaks/releases/latest] | Scans full git history for secrets before the first public push, and on every subsequent push via CI | Purpose-built, `linux_arm64` binary available for the Pi; catches what a hand-rolled grep (per CONTEXT.md's stopgap) would miss (entropy-based detection, not just fixed patterns) |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `gh` CLI | latest via GitHub's own apt repo [CITED: cli.github.com] | Scripted `gh repo create` + release inspection | Only if the operator wants repo creation scripted; otherwise create via the GitHub web UI (checkpoint) and skip this dependency entirely |
| `actions-rust-lang/setup-rust-toolchain` | latest [CITED: github.com/actions-rust-lang/setup-rust-toolchain] | Alternative to `dtolnay/rust-toolchain` that auto-detects `rust-toolchain.toml` | Use instead of `dtolnay/rust-toolchain` if the plan wants to avoid duplicating the `1.98.0` version string in workflow YAML — but note it's a third-party (not dtolnay/official) action, do a quick legitimacy glance before adopting |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tauri-action` cargo/`tauriScript: tauri` path | Hand-rolled `cargo install tauri-cli && tauri build && actions/upload-release-asset` | tauri-action already handles per-target artifact naming, updater JSON generation, and release attach in one step — hand-rolling gains nothing since this project has no npm build step to integrate with anyway |
| `macos-15-intel` for the Intel leg | Drop the Intel leg entirely, ship Apple Silicon only | CONTEXT.md's locked matrix wants both; Rosetta 2 exists as a fallback for Intel-only friends running an ARM `.dmg`, but a native Intel build is still the better UX and costs nothing extra (free standard runner) |
| Throwaway CI-only minisign key (if `--no-sign` fallback is needed) | Real pi-only key temporarily in GitHub Secrets | Explicitly forbidden by the locked decision — never use this even as a "just this once" shortcut |

**Installation:**
```bash
# CI job step (all matrix legs)
cargo install tauri-cli --version 2.11.4 --locked

# Pi (one-time, for gh CLI if the operator wants scripted repo creation)
curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | sudo dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg
sudo chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list
sudo apt update && sudo apt install gh -y

# Pi (one-time, gitleaks arm64 binary)
curl -fsSL -o gitleaks.tar.gz https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_linux_arm64.tar.gz
```

**Version verification:** `tauri-cli 2.11.4` confirmed present on crates.io via the package-legitimacy seam (`gsd-tools query package-legitimacy check --ecosystem crates tauri-cli` → verdict `OK`, published since 2019, ~25.3k weekly downloads). `gitleaks v8.30.1` confirmed as the current `latest` release via a direct, unauthenticated `curl https://api.github.com/repos/gitleaks/gitleaks/releases/latest` call in this session.

## Architecture Patterns

### System Architecture Diagram

```
 developer/operator (Pi, ~/rlcraft)
        │
        │ 1. gitleaks detect --source . (full history scan)   [pre-push gate]
        │ 2. git remote add origin git@github.com:campfire-pub/rlcraft.git
        │ 3. git push -u origin master
        ▼
 GitHub repo: campfire-pub/rlcraft (public)
        │
        │ every push                              on tag v*
        ▼                                          ▼
 ┌─────────────────────────┐         ┌──────────────────────────────────────┐
 │ CI smoke (ubuntu-latest) │         │ Release build matrix (GitHub-hosted)  │
 │ cargo test/clippy        │         │ windows-latest │ macos-14 │ macos-15- │
 │  (launcher + auth-svc)   │         │  (x64 NSIS)     │ (aarch64)│ intel(x64)│
 │ bash -n / py_compile     │         │ tauri-action --no-sign per leg        │
 └─────────────────────────┘         └───────────────┬────────────────────────┘
                                                       │ uploads artifacts
                                                       ▼
                                       GitHub Release "v<ver>"
                                       (.exe, aarch64.dmg, x64.dmg,
                                        + unsigned *.app.tar.gz)
                                                       │
                                                       │ triggers (tag push OR release published)
                                                       ▼
                              ┌─────────────────────────────────────────┐
                              │ Publish job — self-hosted rpi5-1        │
                              │ (registered to campfire-pub/rlcraft)    │
                              │ 1. curl release assets (public, no auth)│
                              │ 2. scripts/publish-launcher.sh          │
                              │    (signs with pi-only minisign key)    │
                              └───────────────────┬──────────────────────┘
                                                   ▼
                              Caddy /launcher/ (mc.campfire.pub:8444)
                              latest.json + signed artifacts
                                                   │
                                                   ▼
                                     friend's launcher (update check)
```

### Recommended Project Structure
```
.github/
└── workflows/
    ├── ci.yml           # smoke: push to any branch, ubuntu-latest, cargo test/clippy + bash -n + py_compile
    └── release.yml      # build+release: on tag v*, matrix (windows-latest, macos-14, macos-15-intel)
                          # + publish job (needs:, runs-on: [self-hosted]) that calls scripts/publish-launcher.sh
scripts/
└── release.sh           # NEW: bumps tauri.conf.json + Cargo.toml versions, commits, tags v<ver>, pushes
docs/
└── FRIENDS.md            # NEW: friend-facing install/bypass instructions
```

### Pattern 1: Cargo-only tauri-action invocation (no npm anywhere)
**What:** Drive `tauri build` through `tauri-action` using `tauriScript: tauri` (the globally cargo-installed binary name) instead of the default npm/yarn detection, with `projectPath: launcher` since the Tauri project isn't at repo root.
**When to use:** Any Tauri project, like this one, with no `package.json`/frontend build tooling.
**Example:**
```yaml
# Source: github.com/tauri-apps/tauri-action README (fetched this session)
strategy:
  fail-fast: false
  matrix:
    include:
      - platform: macos-14
        args: '--target aarch64-apple-darwin'
      - platform: macos-15-intel     # replaces retired macos-13
        args: ''                     # native Intel runner, no cross-target needed
      - platform: windows-latest
        args: ''
runs-on: ${{ matrix.platform }}
steps:
  - uses: actions/checkout@v4
  - uses: dtolnay/rust-toolchain@stable
    with:
      toolchain: '1.98.0'
  - uses: Swatinem/rust-cache@v2
    with:
      workspaces: launcher -> target
  - run: cargo install tauri-cli --version 2.11.4 --locked
  - uses: tauri-apps/tauri-action@v1
    env:
      GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
    with:
      projectPath: launcher
      tauriScript: tauri
      tagName: ${{ github.ref_name }}
      releaseName: 'Campfire Launcher ${{ github.ref_name }}'
      releaseDraft: false
      prerelease: false
      args: ${{ matrix.args }} --no-sign
```

### Pattern 2: Self-hosted publish job scoped to the new repo, without disturbing existing runners
**What:** Register a *second* runner directory on `rpi5-1` (the existing `~/actions-runner-1` is already registered to `campfire-pub/GameSlop_BE` [VERIFIED: ~/actions-runner-1/.runner, read this session — `"gitHubUrl": "https://github.com/campfire-pub/GameSlop_BE"`], confirmed via pm2 that `gh-runner-1`/`gh-runner-2` are the only two runner processes currently managed, both under `/home/asphacean/actions-runner-1` and a sibling directory). GitHub requires one `config.sh` registration per runner directory — you cannot register the same directory to two repos, and reusing GameSlop_BE's directory would break its existing registration.
**When to use:** Always, for this phase — do not touch `~/actions-runner-1`.
**Example:**
```bash
mkdir ~/actions-runner-rlcraft && cd ~/actions-runner-rlcraft
# download same runner package version already used by actions-runner-1
./config.sh --url https://github.com/campfire-pub/rlcraft --token <REG_TOKEN> --name rpi5-1-rlcraft --labels rlcraft-publish
# then wrap in a NEW pm2 process (do not reuse the gh-runner-1 pm2 entry)
pm2 start ./run.sh --name gh-runner-rlcraft
pm2 save
```
`<REG_TOKEN>` is a short-lived token the operator fetches from the new repo's Settings → Actions → Runners → New self-hosted runner — this is a **checkpoint**, not something scriptable without a human in the loop (or a PAT with `admin:org`/repo-admin scope, which is heavier than needed for a one-time registration).

**Org-level runner alternative:** [CITED: docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/managing-access-to-self-hosted-runners-using-groups] GitHub supports registering a runner at the *organization* level once, then granting specific repos (or all repos) access via a runner group — this would let one registration serve both `GameSlop_BE` and `rlcraft` without a second directory. This requires the operator to be an owner of the `campfire-pub` org (very likely, given they created it) and is worth considering, but changes the existing `GameSlop_BE` runner's registration model too (repo-level → org-level is not a small edit) — treat as Claude's Discretion / a question for the planner to raise with the operator rather than silently restructuring the existing runner, since CONTEXT.md's "must not disturb the GameSlop_BE runners" constraint makes the simple new-directory approach the safer default.

### Anti-Patterns to Avoid
- **Storing the real pi-only minisign private key in GitHub Actions Secrets, "just for the CI build gate":** defeats the entire point of the pi-only custody decision from Phase 4. Use `--no-sign` (verified fix in tauri-cli ≥2.9.5) instead; only fall back to a *disposable, non-secret, openly-committed* CI-only keypair if the spike shows `--no-sign` doesn't produce the updater artifact at all.
- **Reusing `~/actions-runner-1`'s directory for the new repo:** breaks the existing GameSlop_BE registration; always a fresh directory (or an explicit, confirmed org-level runner-group migration).
- **Assuming `macos-13` still exists:** it was fully retired December 4, 2025; any workflow YAML referencing it will fail to schedule.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|--------------|-----|
| Cross-platform Tauri bundling + release upload | Custom `cargo tauri build` + `curl` upload-asset scripting | `tauri-apps/tauri-action` | Already handles per-target artifact naming (the exact filenames `publish-launcher.sh` parses), updater JSON generation, and release creation/update in one maintained action |
| Secret scanning across full git history before going public | A one-off grep for `RCON_PASSWORD=`/`PRIVATE KEY` (CONTEXT.md's stated stopgap) | `gitleaks detect` over full history | Entropy-based + pattern-based detection catches classes of secret the fixed-pattern grep would miss; the grep is a fine *first* pass but should not be the only pass before a repo goes public |
| Rust toolchain install matching a project pin | Manual `rustup toolchain install` shell steps in each job | `dtolnay/rust-toolchain` (or `actions-rust-lang/setup-rust-toolchain` for auto-detection) | One line, well-tested, avoids re-deriving `rustup show`/component-install logic per workflow |

**Key insight:** Every piece of this phase's CI/CD surface has a well-maintained, first-party or de-facto-standard action — the only genuinely new code this phase should introduce is `scripts/release.sh` (project-specific version bump/tag) and `docs/FRIENDS.md` (project-specific content).

## Common Pitfalls

### Pitfall 1: `plugins.updater.pubkey` in `tauri.conf.json` blocks ALL CI builds, not just updater artifacts
**What goes wrong:** Any `tauri build` invocation (any bundle target — `.deb`, `.dmg`, NSIS `.exe`, doesn't matter) fails with `Error: A public key has been found, but no private key. Make sure to set the TAURI_SIGNING_PRIVATE_KEY environment variable.` as soon as `plugins.updater.pubkey` is present in the config — even with `createUpdaterArtifacts: false`, even (until the fix) with `--no-sign` passed explicitly.
**Why it happens:** Our `launcher/src-tauri/tauri.conf.json` already has this pubkey set (Phase 4, LNCH-08's update-check feature) [VERIFIED: launcher/src-tauri/tauri.conf.json, read this session — `"plugins": {"updater": {"endpoints": [...], "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDdBOTdBRjg4MTUyMTEzRDIKUldUU0V5RVZpSytYZXZpblNrbU9mb25QMW5CSklHR2RkcEUwSEpmSmc5ZjdrT1Q4WkgzeUdYSDMK"}}, "bundle": {"createUpdaterArtifacts": true}`], and the private key intentionally never leaves the Pi (Phase 4's checkpoint:decision).
**How to avoid:** Pin `tauri-cli` to **2.11.4** (already the project's pin) — the fix landed in 2.9.5 [CITED: tauri-apps/tauri PR #14582, merged 2025-11-30, confirmed as resolving issue #14581]. Pass `--no-sign` as a build arg on every CI matrix leg. As the very first task of this phase (before committing to the full matrix), spike a single build leg and confirm: (a) the build succeeds without any `TAURI_SIGNING_PRIVATE_KEY*` env var set, and (b) for the macOS legs specifically, the `*.app.tar.gz` updater-artifact file is still produced (unsigned, no `.sig`) — this is what `publish-launcher.sh`'s `detect_platform()` requires to exist by filename. This exact sub-behavior (does `--no-sign` still emit the tar.gz, or skip it outright) is **not explicitly documented** — confidence MEDIUM, verify empirically.
**Warning signs:** CI build logs show "A public key has been found, but no private key" on any platform; or the build succeeds but no `_aarch64.app.tar.gz`/`_x64.app.tar.gz` file appears among the uploaded release assets.
**Fallback if the spike shows `.app.tar.gz` is skipped entirely:** Generate a disposable Ed25519/minisign keypair via `tauri signer generate`, solely to satisfy the build-time check. Commit its private key **openly, in plaintext**, in the workflow file or a small checked-in file with a comment explaining it protects nothing and is never trusted by any client (the app only trusts the pubkey baked into `tauri.conf.json`, which is a different key). This is not a secret and therefore does not violate the "never put the minisign key... into Actions secrets" locked decision — but flag this fallback explicitly to the operator before adopting it, since it's a non-obvious reading of that constraint.

### Pitfall 2: `macos-13` no longer exists as a schedulable runner label
**What goes wrong:** A workflow referencing `runs-on: macos-13` will fail to be scheduled (or, during the brownout window that already passed, would have failed intermittently).
**Why it happens:** GitHub fully retired the macOS 13 (Ventura) runner image on **December 4, 2025** [CITED: github.blog/changelog/2025-09-19-github-actions-macos-13-runner-image-is-closing-down]. `macos-latest` itself migrated to macOS 15 earlier in 2025.
**How to avoid:** Use **`macos-15-intel`** for the x86_64 leg — a new, standard (non-"larger") GitHub-hosted label specifically introduced to keep a free Intel option available for public repos, expected to last until Apple's Intel-capable image line ends around Fall 2027 [CITED: github.blog/changelog/2025-09-19-...; github.blog/changelog/2025-07-11-upcoming-changes-to-macos-hosted-runners...]. Confirm "standard, not larger-runner" status at implementation time — larger runners are billed even on public repos and require a paid org plan; `macos-15-intel` was announced as a same-tier replacement for `macos-13`, not a larger-runner variant, but this specific free/standard classification is MEDIUM confidence (not found in an explicit pricing table during this session, inferred from the changelog's phrasing and the "standard GitHub-hosted runners are free for public repos" general rule).
**Warning signs:** Workflow run shows "Waiting for a runner" indefinitely, or a scheduling error mentioning an unrecognized/retired label.

### Pitfall 3: Ad-hoc macOS signing does not prevent the "damaged app" dialog
**What goes wrong:** CONTEXT.md's locked reasoning ("ad-hoc `codesign --sign -` in CI to avoid the 'damaged' variant") overstates what ad-hoc signing does. Friends will still see a Gatekeeper quarantine warning on first open.
**Why it happens:** Ad-hoc signing (`signingIdentity: "-"`) satisfies Apple Silicon's *mandatory* code-signing requirement for internet-downloaded ARM64 binaries — without it, the app is simply killed on launch on Apple Silicon. It does **not** carry Apple's authentication, so Gatekeeper still treats it as untrusted and still requires the user to explicitly allow it [CITED: v2.tauri.app/distribute/sign/macos/ — "Ad-hoc code signing does not prevent MacOS from requiring users to whitelist the installation in their Privacy & Security settings"].
**How to avoid:** No plan change needed — CONTEXT.md's `docs/FRIENDS.md` content ("right-click → Open → Open Anyway, or `xattr -cr`") already covers the actual required workaround. Just don't let the plan drop that doc content on the theory that ad-hoc signing made it unnecessary.
**Warning signs:** A friend reports "the app is damaged and can't be opened" even after the CI pipeline reports successful ad-hoc signing.

### Pitfall 4: Ad-hoc signing + `bundle_dmg.sh` has a known intermittent CI flake
**What goes wrong:** A `.dmg` build occasionally fails at the `bundle_dmg.sh` step right after ad-hoc signing, with no config change between a failing and a subsequent successful run.
**Why it happens:** Reported and reproduced upstream [CITED: github.com/tauri-apps/tauri/issues/13804, marked duplicate of #3055] — looks like a race condition in the DMG-creation tooling on the macOS runner image, not a project misconfiguration.
**How to avoid:** Set `retryAttempts` on `tauri-action` (input exists, default 0) to 1–2 for the macOS legs, or accept an occasional manual re-run of the release workflow.
**Warning signs:** macOS leg fails with an error referencing `bundle_dmg.sh` immediately after a "signing..." log line; re-running the same commit/tag succeeds without any change.

### Pitfall 5: `dtolnay/rust-toolchain` does not auto-read `rust-toolchain.toml`
**What goes wrong:** A workflow author might assume the presence of `launcher/rust-toolchain.toml` (pinned `1.98.0`) is enough and add a bare `- uses: dtolnay/rust-toolchain@stable` step, silently getting whatever "stable" resolves to on the runner image instead of `1.98.0`.
**Why it happens:** [CITED: github.com/dtolnay/rust-toolchain — the maintainer's own repo explicitly declined to add toolchain-file auto-detection; `dsherret/rust-toolchain-file` and `actions-rust-lang/setup-rust-toolchain` exist specifically to fill that gap]
**How to avoid:** Either pass `toolchain: '1.98.0'` explicitly to `dtolnay/rust-toolchain`, or switch to `actions-rust-lang/setup-rust-toolchain` which does read the toolchain file.
**Warning signs:** CI passes with a different rustc version than what a local Pi build uses (version-skew bugs that don't reproduce locally).

## Code Examples

### GitHub REST API — get a release by tag (no auth needed, repo is public)
```bash
# Source: docs.github.com/en/rest/releases/releases (confirmed pattern this session;
# repo is public so no GITHUB_TOKEN is required at all for this GET or for downloading
# a browser_download_url asset — unauthenticated rate limits (60/hr) are more than
# sufficient for a project that cuts a handful of releases)
curl -s "https://api.github.com/repos/campfire-pub/rlcraft/releases/tags/v0.1.0" \
  | jq -r '.assets[] | select(.name | test("x64-setup.exe|aarch64.app.tar.gz|x64.app.tar.gz")) | .browser_download_url' \
  | while read -r url; do curl -fsSL -O "$url"; done
```
This is simpler than what CONTEXT.md's additional_context anticipated ("prefer `GITHUB_TOKEN` of the workflow run") — since the repo and its releases are public, the Pi publish job doesn't need a token at all for this step. A `GITHUB_TOKEN`/PAT would only be needed if the publish job itself needs to write back to GitHub (it doesn't — it only reads assets and writes to the local `launcher-dist/` tree).

### Pre-push secret scan (operator's one-time first push)
```bash
# Source: gitleaks.io / github.com/gitleaks/gitleaks (this session)
./gitleaks detect --source /home/asphacean/rlcraft --report-format json --report-path /tmp/gitleaks-report.json
# Review /tmp/gitleaks-report.json for zero findings before the first `git push` to a public remote.
# Then, for every push going forward, run the same scan in CI (ubuntu-latest smoke job) as a gate.
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|-------------------|---------------|--------|
| `macos-13` for Intel Tauri builds | `macos-15-intel` | Runner retired Dec 4, 2025; new label available since | Any workflow copy-pasted from a pre-2026 tutorial referencing `macos-13` will fail to schedule |
| `cargo tauri build -b X --no-sign` erroring with pubkey present | Fixed to correctly skip signing | tauri-cli 2.9.5, Nov 30 2025 | Removes the need for a signing key of any kind in CI, if the fix covers updater-artifact generation too (verify via spike) |

**Deprecated/outdated:**
- `macos-13` GitHub-hosted runner — fully retired, do not reference in any new workflow.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|----------------|
| A1 | `macos-15-intel` is a *standard* (free, non-"larger") GitHub-hosted runner label for public repos, not a paid larger-runner variant | Standard Stack, Pitfall 2 | If wrong, the Intel matrix leg either fails to schedule on a public-repo free plan or silently bills the operator — verify at implementation time by simply running the workflow and checking for a billing/plan error |
| A2 | `--no-sign` (fixed in tauri-cli 2.9.5) still produces the unsigned `*.app.tar.gz` updater artifact on macOS legs, rather than skipping updater-artifact generation entirely | Pitfall 1 | If wrong, `publish-launcher.sh`'s macOS detection has nothing to consume — the fallback (disposable, openly-committed CI signing key) becomes necessary, which needs an explicit operator go-ahead per the note in Pitfall 1 |
| A3 | Org-level runner groups would let one Pi registration serve both `GameSlop_BE` and `rlcraft` without touching the existing runner's registration model | Pattern 2 | Not verified end-to-end (would require testing against the operator's actual org settings); the plan should default to the simpler new-directory approach and treat org-level sharing as an optional discretionary improvement, not a requirement |
| A4 | `retryAttempts` on `tauri-action` is a sufficient mitigation for the `bundle_dmg.sh` intermittent flake | Pitfall 4 | If the flake persists across retries, the macOS release leg needs a manual re-run process documented for the operator |

**If this table is empty:** N/A — see entries above; every other claim in this document is either `[VERIFIED: ...]` (tool-confirmed this session against crates.io, GitHub's API, or a project file read directly) or `[CITED: ...]` (an official docs/changelog/GitHub-issue page fetched this session).

## Open Questions

1. **Does `--no-sign` on tauri-cli 2.11.4 still emit the `.app.tar.gz` updater artifact, unsigned?**
   - What we know: the specific bug that made `--no-sign` error out entirely (issue #14581) is fixed as of 2.9.5, and our project is pinned well past that at 2.11.4.
   - What's unclear: whether "skip signing" means "still bundle the tar.gz, just without a `.sig`" or "skip creating updater artifacts altogether when unsigned." Neither the CLI reference nor the updater plugin docs spell this out explicitly.
   - Recommendation: make this the very first spike/verification task in the phase plan — one matrix leg, one build, inspect the output directory — before committing the full 3-leg matrix + publish job to the plan.

2. **Should the Pi's second runner registration be repo-scoped (new directory) or migrated to an org-level runner group?**
   - What we know: repo-scoped (new directory, same pattern as `~/actions-runner-1`) is guaranteed not to disturb the existing `GameSlop_BE` registration. Org-level sharing is supported by GitHub and would reduce runner-management overhead going forward.
   - What's unclear: whether the operator is comfortable restructuring the existing runner's registration model as part of this phase, given CONTEXT.md's explicit "must not disturb the GameSlop_BE runners" constraint.
   - Recommendation: default the plan to the new-directory approach (Pattern 2); leave org-level runner groups as a documented "could revisit later" note, not a phase task.

3. **Is a repo-scoped GitHub token needed anywhere in this phase at all?**
   - What we know: the CI build/release job gets `GITHUB_TOKEN` automatically (workflow-scoped, already how `tauri-action` authenticates to create the release). The Pi publish job only needs to *read* public release assets, which requires no auth.
   - What's unclear: nothing significant — CONTEXT.md anticipated needing "a repo-scoped token needed for the Pi publish job," but this research found that's unnecessary for a public repo.
   - Recommendation: the plan should NOT provision any additional PAT/secret for the publish job; only the built-in `GITHUB_TOKEN` (scoped to the workflow run, already ephemeral) is needed, and only inside the CI job itself.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|----------|
| `git` | Repo push, secret scan | ✓ [VERIFIED: `git status`/`git log` ran successfully this session] | (system) | — |
| SSH key `~/.ssh/id_ed25519` | Push to GitHub over SSH | Not directly probed this session (CONTEXT.md states it exists and works) | — | — |
| `gh` CLI | Optional scripted repo creation | ✗ [VERIFIED: `which gh` / `gh --version` both failed this session — "command not found"] | — | Create the repo via the GitHub web UI (checkpoint) — no functional loss, just one manual step |
| `gitleaks` (arm64) | Pre-push + CI secret scan | ✗ (not yet installed; download step is part of this phase) | target `v8.30.1` | A hand-written grep (CONTEXT.md's stopgap) if the binary download is somehow blocked — weaker coverage, use only as last resort |
| `~/.cargo/bin/cargo` (rustup shim) | Local Pi builds, `tauri signer sign` in `publish-launcher.sh` | ✓ [VERIFIED: `ls -la ~/.cargo/bin/cargo` this session — symlink to `rustup`] | rustup-managed, `1.98.0` per `launcher/rust-toolchain.toml` | — |
| pm2 | Process supervision for the new runner | ✓ [VERIFIED: `pm2 list` ran this session — `gh-runner-1`/`gh-runner-2` both `online`] | — | — |
| npm / Node.js | NOT required anywhere in this project | N/A — deliberately absent | — | — |

**Missing dependencies with no fallback:** none — every missing tool (`gh`, `gitleaks`) has a documented, low-cost fallback above.
**Missing dependencies with fallback:** `gh` CLI (→ web UI), `gitleaks` (→ grep stopgap, last resort only).

## Security Domain

`security_enforcement: true`, `security_asvs_level: 1` (from `.planning/config.json`).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|----------------|---------|--------------------|
| V2 Authentication | No | Out of scope — this phase touches CI/release infra, not the game's own auth (already covered in Phase 2) |
| V3 Session Management | No | Same as above |
| V4 Access Control | Partial | GitHub repo visibility (public) + branch/release permissions — standard GitHub repo settings, no custom code |
| V5 Input Validation | No | No new user-facing input surface in this phase |
| V6 Cryptography | Yes | minisign (Ed25519) signing already established in Phase 4 — this phase must not weaken key custody (no private key in Actions secrets); standard, don't hand-roll a different signing scheme |
| V9 Communications | Yes | Release downloads and the update feed already run over HTTPS (Caddy, from Phase 3) — no new transport to design, just confirm nothing in this phase routes artifacts over plain HTTP |
| V10 Malicious Code / Supply Chain | Yes | Every new CI Action/CLI dependency verified above (Package Legitimacy Audit); pin action versions (`@v1`, not `@main`); pin `tauri-cli` to an exact `--locked` version |
| V14 Configuration/Deployment | Yes | GitHub Actions secrets scoping (this phase deliberately keeps the real signing key OUT of Actions secrets — the core security decision of the whole phase), CI runner isolation (new self-hosted runner must not gain access to `GameSlop_BE`'s secrets/workflows) |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|------------------------|
| Secret committed to a public repo's git history (RCON password, CA private key, minisign key) | Information Disclosure | Full-history `gitleaks detect` scan before the first push, and again on every subsequent push in CI (Pitfall/Standard Stack above) |
| A compromised third-party GitHub Action (`tauri-action`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`) pulling a moving tag (`@main`) that later ships malicious code | Tampering | Pin to major-version release tags (`@v1`, `@v2`), not branch names; the Package Legitimacy Audit above already confirms each is an established, official/de-facto-standard action |
| Self-hosted runner (`rpi5-1-rlcraft`) compromised via a malicious PR from a public-repo fork, able to reach the same host as `GameSlop_BE`'s runner and its secrets | Elevation of Privilege | Self-hosted runners on a **public** repo are a known risk for `pull_request` triggers from forks — restrict the release/publish workflow to `push`/tag triggers only (not `pull_request_target` or PR-triggered workflows) so an external contributor's PR can never execute code on the Pi runner |
| Minisign private key exfiltrated from GitHub Actions if ever placed there | Information Disclosure | The entire phase's key architectural decision (never put it in Actions secrets) — this research reinforces that decision rather than finding a reason to relax it |

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|--------------|--------------------|
| REL-01 | GitHub Actions builds Windows x64 installer and macOS aarch64+x64 bundles on every tag | Standard Stack (tauri-action + tauri-cli 2.11.4), Architecture Pattern 1 (matrix YAML using `macos-15-intel` in place of retired `macos-13`), Pitfall 1 (the signing-gate blocker and its fix/spike) |
| REL-02 | macOS build is unsigned; README/first-run instructions explain the Gatekeeper bypass | Pitfall 3 (ad-hoc signing does not remove the need for this doc content — confirms CONTEXT.md's `docs/FRIENDS.md` plan is still correct) |
| REL-03 | Launcher works on Apple Silicon (LWJGL2 natives / Rosetta path verified on real hardware) | Out of CI-research scope — this is the human QA step CONTEXT.md already scopes to the operator's own Mac; no new research needed beyond confirming the `.dmg` this phase produces is the artifact under test |
</phase_requirements>

## Sources

### Primary (HIGH confidence)
- `launcher/src-tauri/tauri.conf.json` — read directly this session (updater pubkey, `createUpdaterArtifacts`, `productName`, `identifier`, version)
- `scripts/publish-launcher.sh` — read directly this session (`detect_platform()` filename patterns, signing flow)
- `~/actions-runner-1/.runner` — read directly this session (`"gitHubUrl": "https://github.com/campfire-pub/GameSlop_BE"`)
- crates.io API (`tauri-cli`) — queried directly this session via the package-legitimacy seam
- GitHub API (`gitleaks` latest release, `campfire-pub/rlcraft` existence check) — queried directly this session
- `.planning/config.json`, `.planning/phases/05-release-to-friends/05-CONTEXT.md`, `.planning/REQUIREMENTS.md`, `.planning/STATE.md` — read directly this session

### Secondary (MEDIUM confidence)
- [tauri-apps/tauri-action README](https://github.com/tauri-apps/tauri-action/blob/dev/README.md) — fetched this session, full input list + example workflow
- [GitHub Changelog — macOS 13 runner retirement](https://github.blog/changelog/2025-09-19-github-actions-macos-13-runner-image-is-closing-down/) — fetched this session
- [GitHub Changelog — macOS hosted runner changes](https://github.blog/changelog/2025-07-11-upcoming-changes-to-macos-hosted-runners-macos-latest-migration-and-xcode-support-policy-updates/) — fetched this session
- [Tauri v2 macOS Code Signing docs](https://v2.tauri.app/distribute/sign/macos/) — fetched this session
- [tauri-apps/tauri issue #14581](https://github.com/tauri-apps/tauri/issues/14581) and [PR #14582](https://github.com/tauri-apps/tauri/pull/14582) — fetched this session, confirms bug + fix + fixed-in version
- [tauri-apps/tauri issue #13804](https://github.com/tauri-apps/tauri/issues/13804) — fetched/searched this session, `bundle_dmg.sh` flake
- [dtolnay/rust-toolchain](https://github.com/dtolnay/rust-toolchain), [Swatinem/rust-cache](https://github.com/Swatinem/rust-cache) — searched this session
- [gitleaks](https://github.com/gitleaks/gitleaks) — searched this session, arm64 release confirmed
- [GitHub CLI official install docs](https://cli.github.com/) via LinuxCapable/community summary — searched this session
- [GitHub Docs — managing access to self-hosted runners using groups](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/managing-access-to-self-hosted-runners-using-groups) — searched this session

### Tertiary (LOW confidence)
- Whether `macos-15-intel` is definitively a "standard" (not "larger") free-for-public-repos label — inferred from changelog phrasing, not found in an explicit current pricing table this session (Assumption A1)
- Exact behavior of `--no-sign` regarding updater-artifact (tar.gz) generation on macOS — not explicitly documented (Assumption A2)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every package/action verified against crates.io, GitHub's API, or official repo pages this session
- Architecture: MEDIUM-HIGH — the matrix/workflow pattern is directly sourced from tauri-action's own README; the runner-registration mechanics are corroborated by reading the existing `.runner` file
- Pitfalls: MEDIUM — the two corrections (macos-13, ad-hoc signing) are HIGH confidence (official changelogs/docs); the signing-gate fix (tauri-cli 2.9.5) is MEDIUM (confirmed merged, but exact CI-produced-artifact behavior on `--no-sign` not empirically tested this session — hence the recommended spike)

**Research date:** 2026-08-30
**Valid until:** ~2026-09-13 (GitHub Actions runner-image and Tauri-CLI behavior both move fast; re-verify the `macos-15-intel` pricing tier and the `--no-sign` artifact behavior at plan/execution time if this research is more than ~2 weeks old)
