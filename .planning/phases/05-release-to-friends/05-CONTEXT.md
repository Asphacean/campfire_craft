# Phase 5: Release to Friends - Context

**Gathered:** 2026-08-28
**Status:** Ready for planning

<domain>
## Phase Boundary

Turn the Phase-4 launcher source into downloadable releases friends can install: a GitHub repository with CI that builds Windows x64 and macOS (Apple Silicon + Intel) bundles on tag, publishes them as a GitHub Release, and pushes the signed update feed to the Pi's `/launcher/` endpoint; plus the friend-facing install instructions incl. the Gatekeeper/SmartScreen bypass. Covers REL-01, REL-02, REL-03. Correction to ROADMAP success criterion 1: the existing self-hosted runners (rpi5-1/rpi5-2) are aarch64 Linux and cannot build Windows/macOS Tauri bundles — builds run on GitHub-hosted runners; the Pi runner is used for the publish step only.

</domain>

<decisions>
## Implementation Decisions

### Repository & CI
- New GitHub repository **`campfire-pub/campfire_craft`**, **public**, containing the whole `~/rlcraft` tree as-is (secrets already gitignored: `server.env`, `ca/campfire-ca-key.pem`, `auth/`, `pack/`, `launcher-dist/`, keys). Pushed from the Pi over SSH (key `~/.ssh/id_ed25519`, GitHub user Asphacean). A pre-push secret scan (grep for `RCON_PASSWORD=`, `BEGIN .* PRIVATE KEY`, `.key` files) runs before the first push
- Public visibility chosen for unlimited GitHub-hosted macOS/Windows minutes; no secrets in code; domain/IP are already public
- Build matrix via `tauri-apps/tauri-action` on tag `v*`: `windows-latest` → x64 NSIS `.exe`; `macos-14` → aarch64 `.dmg`; `macos-13` → x86_64 `.dmg`. Artifacts + Tauri updater `.sig` files attached to a GitHub Release
- Publish job on self-hosted runner **`rpi5-1`** (register a runner for the new repo in addition to the existing registration): downloads the release assets and runs `scripts/publish-launcher.sh` so `latest.json` + artifacts land on `https://mc.campfire.pub:8444/launcher/`. minisign private key stays pi-only (never in GitHub Secrets) — the Pi job signs
- CI smoke on every push (ubuntu-latest): `cargo test --workspace` + `cargo clippy` for `launcher/` (rustup toolchain from rust-toolchain.toml), `cargo test` for `auth-service/`, `bash -n scripts/*.sh`, `python3 -m py_compile scripts/*.py`

### Artifacts & bypass instructions
- Windows: NSIS `.exe`, per-user install (no admin). Friends doc: SmartScreen → "More info → Run anyway"
- macOS: unsigned `.dmg` (REL-02) **plus ad-hoc `codesign --sign -`** in CI to avoid the "damaged" variant; doc: right-click → Open → Open Anyway, or `xattr -cr "/Applications/Campfire Launcher.app"`; Rosetta installs on prompt for the x86_64 Java
- Naming: app **"Campfire Launcher"**, bundle id `pub.campfire.launcher`; artifacts `Campfire-Launcher_<ver>_x64-setup.exe`, `Campfire-Launcher_<ver>_aarch64.dmg`, `Campfire-Launcher_<ver>_x64.dmg`; version source of truth `launcher/src-tauri/tauri.conf.json`; `scripts/release.sh <ver>` bumps version, commits, tags `v<ver>`, pushes
- Friend-facing page: `docs/FRIENDS.md` (English) linking to GitHub Releases "latest"; repo README summarises. No human-facing links to `:8444` (private-CA warning)

### Verification & first release
- REL-03: human check on the operator's Apple Silicon Mac with the release `.dmg`, per `docs/LAUNCHER-BUILD.md` QA matrix; same session closes deferred UATs of Phases 1–4 (01/02/03/04-UAT.md). Intel macOS = built in CI, unverified (no hardware) — recorded honestly
- First release `v0.1.0` cut as soon as CI is green (operator decision) — QA happens on the release artifacts

### Claude's Discretion
- tauri-action version pin, Rust cache action, runner registration mechanics for the second repo (new runner dir vs `config.sh` multi-registration), release notes template, whether Intel build stays in the matrix if it fails repeatedly

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `scripts/publish-launcher.sh` (Phase 4) — signs/publishes feed; the CI publish job wraps it
- `docs/LAUNCHER-BUILD.md` — build prerequisites and QA matrix → base for CI steps and FRIENDS.md
- `launcher/src-tauri/tauri.conf.json` — updater endpoint + pubkey; version field
- `.gitignore` — already excludes secrets; verify before first push
- `~/actions-runner-1` (`rpi5-1`, pm2-managed `gh-runner-1`) — pattern for registering another runner

### Established Patterns
- Bash scripts idempotent, `set -euo pipefail`; secrets never in git; docs in `docs/`; game server never restarted without announcement

### Integration Points
- updater feed: `https://mc.campfire.pub:8444/launcher/latest.json` (Caddy `/launcher/` route exists)
- GitHub: SSH from Pi works; `gh` CLI is NOT installed (plan may install it or use the REST API with a token the operator provides via checkpoint)

</code_context>

<specifics>
## Specific Ideas

- Never put the minisign key, server.env, CA key or Namecheap/GitHub tokens into the repo or Actions secrets except a repo-scoped token needed for the Pi publish job (prefer `GITHUB_TOKEN` of the workflow run)
- Keep pbwiki/sing-box untouched; the new runner must not disturb the GameSlop_BE runners

</specifics>

<deferred>
## Deferred Ideas

- Apple Developer signing/notarization (REL-04, v2)
- Windows code signing certificate
- Linux launcher build

</deferred>
