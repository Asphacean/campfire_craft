# Playing on the RLCraft server

You've been invited to play RLCraft on a private server. This page walks you
through installing the launcher and getting into the world — no Minecraft or
server-admin experience needed.

## What you need

- A Windows or Mac computer (not a phone or tablet).
- A few gigabytes of free disk space — the modpack, a Java runtime, and the
  game's own files all get downloaded to your machine.
- A decent internet connection. The first time you press Play, the launcher
  downloads several gigabytes; on a slow connection that can take a while,
  but it only happens once.

## Which file to download

Everything comes from one place — the project's release page:

**<https://github.com/Asphacean/campfire_craft/releases/latest>**

That link always points at the newest version. On that page, download the
one file for your machine:

| Your machine | File to download |
|---|---|
| Windows | `Campfire-Launcher_0.1.0_x64-setup.exe` |
| Mac — Apple Silicon (M1/M2/M3/M4) | `Campfire-Launcher_0.1.0_aarch64.dmg` |
| Mac — Intel | `Campfire-Launcher_0.1.0_x64.dmg` |

**Not sure which Mac you have?** Click the Apple menu (top-left corner) →
**About This Mac**. If the line under the Mac's name says **Chip** (for
example "Chip: Apple M2"), you have Apple Silicon — download the `aarch64`
file. If it says **Processor** with an Intel model name instead, you have an
Intel Mac — download the `x64` file.

## Windows: installing and the one warning you'll see

1. Run the downloaded `.exe`.
2. Windows will show a blue screen titled **"Windows protected your PC"** —
   this is Microsoft's SmartScreen filter warning about a program from an
   unrecognized publisher. It is expected; the launcher is not signed with a
   paid Microsoft certificate. Click **"More info"**, then **"Run anyway"**.
3. The installer runs with no administrator prompt — it installs just for
   your Windows user account, not system-wide.

## macOS: installing and the one warning you'll see

1. Open the downloaded `.dmg` file, then drag **Campfire Launcher** into the
   **Applications** folder.
2. Open **Applications** and try to launch it. macOS will refuse, saying the
   app is from an unidentified developer (or that it "can't be opened"/"is
   damaged"). This is Gatekeeper — the app is not signed with a paid Apple
   developer certificate, so macOS makes you vouch for it once, the first
   time. We can't promise exactly which wording you'll see, but one of these
   two routes gets you past it:
   - **Right-click** (or Control-click) the app in Applications and choose
     **Open** from the menu, then click **Open** again in the dialog that
     appears. This is the easier route and usually works.
   - If that dialog doesn't offer an "Open anyway" option, open the
     **Terminal** app and run this one command, then try opening the app
     normally again:
     ```
     xattr -cr "/Applications/Campfire Launcher.app"
     ```
3. After this first time, the app opens normally like any other.

## First launch: creating your account and playing

1. Pick a nickname and a password — this creates your account on the
   server. Remember the exact spelling and capitalization; that's your
   permanent identity in the game.
2. Set how much memory (RAM) to give the game with the slider. The default
   is usually fine; the slider warns you in red if you push it past what's
   comfortable for your machine.
3. Press **Play**.
4. The first time, this downloads Java and the whole modpack — several
   gigabytes, and it can take a while depending on your connection. This
   only happens once; every launch after that is fast.
5. **On an Apple Silicon Mac**, you may be asked to install Apple's Rosetta
   translation layer during this step — say yes/let it install. This is a
   one-time system installation the game needs and is expected.
6. Once it's done, you'll be in the RLCraft world on the server.

## If something goes wrong

The launcher shows a plain-English sentence when something fails (a wrong
password, no internet connection, a Java problem, and so on) along with an
**"Open log"** button. Click it, and send the log file to whoever gave you
this link — they can tell what happened from it far faster than from a
description.

The log file itself lives at:

- **Windows:** `%APPDATA%\campfire\launcher.log`
- **macOS:** `~/Library/Application Support/campfire/launcher.log`

It never contains your password or any login token — those are always
redacted — so it's safe to share.
