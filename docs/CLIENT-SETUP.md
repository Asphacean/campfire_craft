# Joining the RLCraft server (Phase 1 hand-install path)

This is the temporary, hand-install client path friends follow to join the
server. Phase 4 replaces every step below with a one-click launcher — treat
this as scaffolding, not a habit worth building.

Server address: `mc.campfire.pub`, port `25565`. Always use the domain —
never a raw IP address. The domain is the only address that is guaranteed
to keep working if the server's connection or hosting details ever change.

## 1. Install the CurseForge app

1. Go to [curseforge.com/download/app](https://www.curseforge.com/download/app)
   and download the installer for your OS (Windows or macOS).
2. Run the installer and sign in (or create a free CurseForge/Overwolf
   account — this is unrelated to your Minecraft account).

This is the supported install route for this server. Prism Launcher can
probably import the RLCraft modpack too, but that path has not been tested
against this server — stick to the CurseForge app unless you already know
what you're doing with Prism.

## 2. Install RLCraft — exactly version 2.9.3

1. In the CurseForge app, go to **Minecraft** → **Browse Modpacks**.
2. Search for **RLCraft**.
3. Before installing, click the version dropdown and pick **2.9.3**
   specifically. Do not install "latest" blindly — if a newer version has
   shipped since this was written, use the version selector to force 2.9.3.
   A different pack version will be refused by the server's mod list (the
   mod versions won't match what the server is running, and you won't be
   able to join).
4. Click **Install** and wait for the download/setup to finish — this pack
   is large (300+ MB) and will take a while depending on your connection.

## 3. Give the profile enough RAM

RLCraft is heavy. Before launching:

1. In the CurseForge app, open the RLCraft profile's **Options** (gear
   icon).
2. Find the memory/RAM allocation slider.
3. Set it to **at least 6 GB**. Less than that will cause crashes or
   unplayable stutter once the world starts generating RLCraft's structures
   and mobs.

## 4. Add the server

1. Launch RLCraft once through the CurseForge app (first launch takes a
   while — it's compiling/caching assets, this is normal).
2. In the Minecraft main menu, go to **Multiplayer** → **Add Server**.
3. Server address: `mc.campfire.pub:25565`
4. Save, then double-click the server entry to connect.

## 5. Whitelist status (read this before asking why you can't join)

As of Phase 1, the server has **no whitelist** — access is open to anyone
who can reach `mc.campfire.pub:25565` with the correct client version. You
do not need your nickname added by the operator to join right now.

This is a deliberate, temporary decision (see `01-01-SUMMARY.md`, D-09
override) and it will not last — Phase 2 replaces open access with proper
token-based authentication tied to the future launcher. If a future
whitelist is ever turned back on, a non-whitelisted nickname will be
refused at login with an on-screen message before you can move or interact
in the world — if you ever see that message, ask the operator to add your
nickname.

## 6. If something fails

The client log lives at:

- **Windows:** `%APPDATA%\CurseForge\Instances\RLCraft\logs\latest.log`
- **macOS:** `~/Library/Application Support/CurseForge/Instances/RLCraft/logs/latest.log`
  (exact path depends on where the CurseForge app installed the instance —
  check the instance's "Open Folder" button in the app if this doesn't
  exist)

Common failure: a mod-version mismatch between your client and the server
shows up as a red disconnect screen listing which mods differ — this
almost always means you installed the wrong RLCraft version. Re-check step
2.

## Before Phase 4 ships

Everything above — the CurseForge app, the manual version pin, the manual
RAM slider, typing the server address by hand — goes away once Phase 4's
launcher exists. Don't build muscle memory around this; it's a bridge, not
the final experience.
