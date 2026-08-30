# Building the launcher from source

This is the operator's own build recipe — not something a friend ever
needs to do (friends download a built binary from a link the operator
sends, once Phase 5 automates that). It exists because this project has
no Windows or Apple Silicon machine wired into any CI yet: the operator's
own two machines are where every real, human-driven check in this
document happens, following `docs/DIST-OPS.md`'s "launcher self-update
feed" section for how to publish whatever gets built here.

Everything in this whole phase was developed and proven headlessly on a
Raspberry Pi 5 (aarch64 Debian 13) with no display — the same commands
below run there too, and `campfire-cli` (this document's "headless proof
harness" section) is how that Pi verified all of it without ever opening
a window.

## Prerequisites (every platform)

1. **rustup**, not whatever Rust your OS package manager ships. The exact
   toolchain version is pinned in `launcher/rust-toolchain.toml`
   (currently `1.98.0`, plus `rustfmt`/`clippy`) — rustup reads that file
   automatically the moment you run any `cargo` command from inside
   `launcher/`, so you never have to type the version yourself. Install
   rustup:

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
   ```

   (On Windows, use the graphical installer from
   [rustup.rs](https://rustup.rs/) instead of the shell one-liner above.)

2. **Check your toolchain first, before building anything.** A rustup
   install from months ago may be below the pin. From inside `launcher/`:

   ```bash
   cargo --version   # once inside launcher/, this already resolves through rust-toolchain.toml
   rustup update      # if the printed version is older than 1.98.0
   ```

3. **The Tauri CLI**, from crates.io:

   ```bash
   cargo install tauri-cli --version "^2" --locked
   ```

4. **No Node.js. No npm. No bundler.** This is a genuine difference from
   almost every Tauri tutorial you'll find online, and it is easy to
   install Node out of habit and then wonder why nothing uses it — don't.
   `launcher/ui/` is a plain static directory (`index.html`/`main.js`/
   `style.css`, no build step, no `package.json` anywhere in this repo)
   that Tauri copies in as-is. If a guide tells you to run `npm install`
   or `npm run tauri build`, it is describing a different, JS-frontend
   Tauri project, not this one.

## Windows x64

From the repository root:

```bash
cd launcher
cargo test --workspace          # the headless core suite — no window needed
cargo tauri build --no-bundle   # a plain campfire-launcher.exe, no installer
```

`--no-bundle` produces `launcher\target\release\campfire-launcher.exe`
directly, with no NSIS/MSI installer wrapped around it — the fastest way
to get something runnable while iterating. Drop `--no-bundle` (just
`cargo tauri build`) once you want the real installer artifact
(`campfire-launcher_<version>_x64-setup.exe`) — the shape
`scripts/publish-launcher.sh` expects on this platform.

**The installer will show a Windows SmartScreen warning ("Windows
protected your PC").** This is expected and is not a build failure — it
means the binary is unsigned, which is true of every build produced by
this document. Code-signing so SmartScreen stops complaining is Phase 5's
job (see `.planning/REQUIREMENTS.md`'s `REL-02`), not something this
build recipe attempts.

## macOS Apple Silicon

Same two commands, from the repository root:

```bash
cd launcher
cargo test --workspace
cargo tauri build --no-bundle    # or `cargo tauri build` for the real .app.tar.gz
```

**One-time system step, before the first Play press on this machine —
not part of the build itself.** This project deliberately ships an
x86_64 Java 8 runtime to every macOS target, Apple Silicon included
(D-10/LNCH-03's locked decision — see `04-RESEARCH.md`'s "Rosetta"
discussion for why: Adoptium has no arm64 Java 8 build at all, Java 8
being end-of-life for that architecture). That means the *launcher*
binary you just built is native Apple Silicon, but the *game's* Java
process it spawns runs translated under Rosetta 2. If Rosetta isn't
already installed, install it once:

```bash
softwareupdate --install-rosetta --agree-to-license
```

(Drop `--agree-to-license` to see and accept Apple's license text
interactively instead — either way, this is a one-time system install
the user has to explicitly agree to; there is no way to force it
silently, and this document does not attempt to.) Without this step,
Play will fail with "Couldn't set up Java." — see "What the log does and
doesn't contain" below for how to confirm that's really what happened.

**Gatekeeper will refuse to open an unsigned, unnotarized `.app` at all**
on a real macOS machine (not just warn, like Windows SmartScreen) — this
is REL-02's job in `.planning/REQUIREMENTS.md`, covered by Phase 5's
signing/notarization pipeline, not this document. Building from source
with `cargo tauri build --no-bundle` and running the resulting binary
directly from the terminal sidesteps Gatekeeper's Finder-launch check
entirely, which is exactly why every check in this document's QA matrix
below is written to run the binary that way.

## macOS Intel

**No Intel Mac exists to build or test on for this project.** The build
path is identical to Apple Silicon above — same two commands, same
Rosetta reasoning does *not* apply (an Intel Mac's Java 8 runs natively,
no translation layer involved at all) — but nothing in this section has
been run on real Intel hardware. This is verified by reasoning (the
`darwin-x86_64` code paths are the same Rust source the Apple Silicon
build compiles, and `campfire-cli java-fetch --target mac-x64` has been
run and proven on the Pi against the real Adoptium API) and by whatever
Phase 5's CI produces once it exists, not by a human running it here.

## The headless proof harness

Every capability in this launcher can be exercised without a display,
via `campfire-cli` (built alongside the GUI app by the same `cargo build`/
`cargo tauri build` commands above — no separate build step). This is
how the whole of Phase 4 was developed and verified on a Pi with no
monitor, and it's the fastest way to debug a friend's problem without
attaching a debugger to the GUI:

```
campfire-cli status                              # GET /status
campfire-cli register <nick>                      # password from stdin
campfire-cli login <nick>                          # password from stdin
campfire-cli refresh                               # rotates the stored refresh token
campfire-cli keyring-selftest                      # OS credential store round-trip
campfire-cli pin-check                             # proves TLS pinning is really enforced
campfire-cli sync [--dir <path>]                   # manifest sync (pack files)
campfire-cli verify [--dir <path>]                 # re-hash + repair every managed file
campfire-cli java-fetch [--target windows-x64|mac-x64|mac-arm64] [--dir <path>]
campfire-cli java-probe                            # runs `java -version` on the provisioned JRE
campfire-cli vanilla [--dir <path>]                 # Mojang's own client + assets
campfire-cli forge [--dir <path>]                   # headless Forge 1.12.2 install
campfire-cli launch-cmd --nick <n> --ram <g> [--target ...] [--dir <path>]   # prints the argv, doesn't spawn
campfire-cli launch --nick <n> --ram <g> [--target ...] [--dir <path>]      # prints the argv, spawns the game
campfire-cli play --nick <n> --ram <g> [--no-spawn] [--dir <path>]          # the whole Play sequence
campfire-cli system-memory                          # the RAM slider's own machine facts
campfire-cli update-check                           # the self-update feed check, standalone
```

`--dir <path>` (accepted by every subcommand that touches the filesystem)
points the whole run at a scratch directory instead of your real profile
— use it freely; it's the same mechanism the `CAMPFIRE_HOME` environment
variable sets, and it's how this entire phase was proven repeatedly
against fresh installs without ever touching a real one.

## Where the log lives, and what it does and doesn't contain

- **Windows:** `%APPDATA%\campfire\launcher.log`
- **macOS:** `~/Library/Application Support/campfire/launcher.log`

One file, rotated once at a couple of megabytes (`launcher.log.1` holds
the previous generation) — this is a diagnostic a friend pastes into
chat, not an audit trail. It contains: every step the Play sequence ran,
the nick involved, and the real cause of a failure (a Java error's actual
underlying reason, a network error's real host/status, etc.). It never
contains a password, a refresh token, or a game token — every one of
those is redacted to `<redacted, N bytes>` at the point of logging, not
scrubbed afterward.

## Cutting a release

The path that ships to friends is one command, from the repository root:

```bash
scripts/release.sh <version>
```

This bumps the version everywhere it's written (`tauri.conf.json`,
`Cargo.toml`, `Cargo.lock`), commits, tags `v<version>`, and pushes. That
push triggers `.github/workflows/release.yml`: three GitHub-hosted runners
(`windows-latest`, `macos-14`, `macos-15-intel`) build the Windows
installer and both macOS bundles, attach them to a new GitHub Release, and
a job on this Pi's own self-hosted runner (`campfire-publish`) downloads
those assets, signs them with the operator's pi-only minisign key, and
republishes `/launcher/latest.json` — the same job `scripts/publish-launcher.sh`
has always done, now driven by CI instead of by hand.

Watch the run at <https://github.com/Asphacean/campfire_craft/actions> —
it takes several minutes per platform leg. When it finishes, the release
carries six assets: the Windows NSIS installer and MSI
(`_x64-setup.exe`, `_x64_en-US.msi`), both macOS `.dmg` disk images
(`_aarch64.dmg`, `_x64.dmg`), and both macOS updater archives
(`_aarch64.app.tar.gz`, `_x64.app.tar.gz`) — the last two are what the
self-update feed serves, not something a friend downloads directly. The
canonical link for anyone fetching the result, friend or operator, is
<https://github.com/Asphacean/campfire_craft/releases/latest> — it always
resolves to whatever this command just cut.

**If a macOS leg fails right inside `bundle_dmg.sh`,** that's a known,
intermittent upstream flake in the DMG-creation tooling, not a
configuration problem — re-run the same workflow run (or push an empty
commit and re-tag) and it typically succeeds on the retry.

The signing key never leaves this Pi — it's what makes every signature the
self-update feed publishes a real one, verifiable against the public key
compiled into every installed launcher. The hand-build steps in the
sections above still apply if you need to build one platform locally
(iterating on a change, or the QA matrix's Apple Silicon checks below);
`scripts/publish-launcher.sh` (`docs/DIST-OPS.md`'s "The launcher
self-update feed" section) remains the fallback for publishing something
built that way instead of through the pipeline.

## The Phase 4 operator QA matrix

Everything below needs real hardware and a real display; nothing on the
development Pi can answer any of it. Build the launcher from source on
each machine following the sections above, then work through the list and
report each line as pass, fail, or something-else-happened.

### On Windows x64 — the main path

1. **Clean-machine launch (Phase 4 success criterion 1).** On a machine
   with no Java installed, register a brand-new nick, pick a RAM value,
   press Play once, and end up in the RLCraft world on campfire.pub.
   Report the total wall time and roughly how much disk it used.
2. **Progress is informative (LNCH-05).** During that first Play, the
   step label changes through the real stages — pack files, Java,
   Minecraft files, Forge — with a file count and a transfer rate, and
   there is never a stretch of more than a few seconds with no visible
   change.
3. **Second launch (Phase 4 success criterion 2).** Close everything,
   reopen, press Play. You are not asked for a password, almost nothing
   downloads, and you land in the world with your position and inventory
   intact.
4. **Nothing of yours was touched.** Change a video setting in-game,
   quit, press Play again, and confirm the setting survived. Confirm your
   world is still in the saves folder.
5. **RAM slider (LNCH-01, D-06).** The slider covers 3 to 10 in half
   steps, opens at roughly half your machine's memory, and turns red with
   a warning sentence past seventy percent — and Play still works at that
   setting.
6. **Wrong password (LNCH-06).** Log out, log in with a wrong password,
   and see "Wrong nickname or password." with a working Open log button.
7. **Server unreachable (LNCH-06).** Turn off your network and press
   Play. You get "Can't reach campfire.pub. Check your internet
   connection." — not a stack trace, not a frozen progress bar, and not a
   hang.
8. **Offline server (LNCH-07, D-05).** While the game server is stopped
   (coordinate with the operator), the pill reads Offline and **Play is
   still enabled**.
9. **Verify files (D-09).** Delete a mod jar from the game folder by
   hand, press Verify files, and see it report the repair; then Play
   works.
10. **Game folder (D-09).** The button opens the install directory in
    Explorer.
11. **Self-update (LNCH-08).** With a feed published advertising a higher
    version, reopening the launcher shows "Update available" with
    "Version X.Y.Z is ready." and the two buttons. Press Later and
    confirm the launcher works normally. If a real newer build is
    available, press Update now and report whether it replaced itself.
12. **The window itself (D-04, UI-SPEC).** The RLCraft art is visible
    behind a dark translucent panel, all text is legible on the panel,
    the Play button is the only orange element, tabbing moves through the
    controls in reading order with a visible focus ring, and the window
    cannot be resized.
13. **Log hygiene.** Open the log and confirm you can see your nick and
    the launch steps, and that you cannot find your password or a token
    value.

### On macOS Apple Silicon

14. **It builds and it opens.** Following the sections above, the
    launcher builds and the window opens with the same layout.
15. **Rosetta path (LNCH-03, REL-03 preview).** Press Play. The launcher
    fetches an x86_64 Java 8 by deliberate decision. Report whether the
    game starts, whether it renders correctly, and roughly what framerate
    you get standing still in a forest — this is the one number that
    decides whether the deferred arm64 follow-up ever needs to happen.
16. **Translation layer missing.** If the machine has never installed
    the translation layer, report what the launcher did: it should show
    "Couldn't set up Java." with the real cause named in the log, not a
    silent failure.
17. **Same session behaviour.** Second launch does not ask for a
    password, and the macOS keychain holds the entry.

### Deferred checks this phase finally unblocks (Phases 1-2)

The Phase 1 and Phase 2 items that were waiting for a real client — a
friend outside the home network joining by domain and playing, and a
vanilla client with no token being kicked with a clear message — are now
runnable with this launcher and should be run alongside item 1 above.

## The Phase 5 release QA matrix

Everything below needs the actual released artifacts on real hardware —
not a local build. Download from
<https://github.com/Asphacean/campfire_craft/releases/latest> exactly the
way a friend would, work through the list, and report each line as pass,
fail, or something-else-happened. This continues the numbering above and
closes out REL-01 through REL-03, plus the Phase 1-4 items that were
waiting on exactly this release.

### On Windows x64, from the released installer (REL-01, REL-02)

18. **Download it like a friend.** From the release page above, download
    `Campfire-Launcher_0.1.0_x64-setup.exe` and run it.
19. **The SmartScreen detour.** Report the exact wording of the warning
    dialog, and confirm "More info" → "Run anyway" gets past it with no
    further prompt.
20. **No administrator prompt.** The installer completes without ever
    asking to elevate — it installs per-user, per `docs/FRIENDS.md`.
21. **Start menu entry.** The app appears in the Start menu under its
    released name ("Campfire Launcher").
22. **Clean-machine Play.** On a machine with no Java installed, register
    a fresh nick, press Play once, and report the total wall time and the
    disk space used.
23. **Second launch.** Close and reopen; you are not asked for a password
    again.

### On Apple Silicon, from the released disk image (REL-03, D-11)

24. **Download it like a friend.** From the same release page, download
    `Campfire-Launcher_0.1.0_aarch64.dmg`, open it, and drag the app to
    Applications.
25. **The Gatekeeper detour.** Report which of the two documented routes
    (right-click → Open → Open, or the `xattr -cr` Terminal command from
    `docs/FRIENDS.md`) was needed, and the exact wording seen.
26. **The app opens at all.** This is the question ad-hoc signing exists
    to answer — report whether it does.
27. **Play, on real hardware.** Report whether the Rosetta translation
    layer was offered and accepted, whether the game renders, and roughly
    what framerate you get standing still in a forest — that number
    decides whether an arm64 follow-up build is ever needed.
28. **Second launch.** Close and reopen; you are not asked for a password
    again.

### The update path

29. **Deferred to the next release.** After the next version is cut, an
    installed launcher should offer the update on startup and install it.
    This needs a second version to exist, so it's the first item of the
    *next* release's matrix, not this one.

### Intel macOS

30. **Built, never run.** The Intel disk image
    (`Campfire-Launcher_0.1.0_x64.dmg`) is produced by CI and downloadable
    from the same release page, but no Intel Mac exists for this project
    to run it on. Recorded here as unverified (D-11), not silently
    omitted.

### The four deferred phase verifications this release unblocks

31. **Phase 1 — a friend outside the home network.** Joining
    `mc.campfire.pub` by domain from outside the LAN and playing, this
    time via the launcher instead of the old hand-install path.
32. **Phase 2 — a vanilla client with no token.** Confirm a vanilla
    (non-launcher) client connecting without a valid token is kicked with
    the bilingual message before it can act.
33. **Phase 3 — a client assembled from the manifest.** Confirm the
    launcher-assembled client (built purely from the manifest this
    launcher fetches) joins and plays normally.
34. **Phase 4 — the window itself.** Progress-step legibility during the
    first Play, the RAM slider's >70% warning state, the wrong-password
    and server-unreachable error banners with their "Open log" button,
    the status pill's Online/Offline/Checking states, and the "Game
    folder"/"Verify files" secondary buttons — all per `04-UI-SPEC.md`.

The operator's own Mac is Apple Silicon, and that is one machine. Anything
a friend reports that this matrix doesn't cover is a finding, not a
failure.

### One thing to watch for

**If the game fails to start with an error about an unrecognised option**,
that is the optional autoconnect attempt from wave 3 (`launch.rs`'s
autoconnect argument pair) — turn it off by omitting the autoconnect flag
when invoking `campfire-cli launch`/`launch-cmd` (`build_launch_command`'s
`autoconnect: bool` parameter; the GUI's own `play` command always passes
`true`, so this only surfaces via the headless harness today). Report it
if you see it in the real GUI, because it would mean those two arguments
need to come out permanently rather than stay optional.
