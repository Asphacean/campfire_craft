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

## 5. Registration and tokens (Phase 2 — read this before asking why you can't join)

As of Phase 2, the server has **no whitelist**, but it does require a
valid, freshly-minted token on every join — a client that connects without
one is turned away before it can move, break a block, or chat, with this
message:

```
Зайди через лаунчер campfire.pub / Join via the campfire.pub launcher
```

If you see that message, it means the token step below wasn't done (or was
skipped, or has expired) — it is not a whitelist rejection.

This hand-install token flow is a **stopgap** until the Phase-4 launcher
does it automatically for you. Right now, you are the launcher:

1. **Add the mod.** Put `campfire-auth-0.1.1.jar` into your RLCraft
   instance's `mods/` folder, alongside the rest of the RLCraft mods (ask
   the operator for the jar — it's the same one running on the server).
2. **Register once.** Ask the operator to create your account (nick +
   password), or do it yourself if self-registration is open — use the
   **exact capitalisation** you intend to always play under. The game
   derives your player ID from the exact letters of your nick, so `Steve`
   and `steve` are two different players with two different inventories.
   Getting the casing wrong looks like your world/inventory going missing.
3. **Get a token before every join.** The operator mints one with
   `campfire-auth login <YourNick>` (one CLI call on the Pi) and gives you
   the printed value. A token is **single-use** and expires after 12
   hours — you need a fresh one for every single connection, not just the
   first one.
4. **Add two JVM arguments** to your launcher profile before connecting:
   `-Dcampfire.nick=<YourNick>` and `-Dcampfire.token=<the token>`. In the
   CurseForge app this is under the profile's **Options** → advanced/JVM
   arguments field, alongside the memory settings from step 3 above.
5. **Connect as normal.** You may float in place, unable to act, for up to
   a few seconds while the server checks your token — that's expected.
   Then you're in.

A friend who has not registered yet cannot join at all, no matter what
JVM flags they set — registration has to happen first, and only the
operator can do it right now (or you, if self-registration is open; ask).

This is a deliberate, temporary decision (see `01-01-SUMMARY.md`, D-09
override, and `02-CONTEXT.md`) and it will not last — Phase 4's launcher
handles all of the above (registration, login, minting a token per join)
automatically, with no manual JVM flags.

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

## 7. The file server (for the curious, not a download page)

As of Phase 3, the server also publishes the exact modpack it runs over
HTTPS, at `https://mc.campfire.pub:8444/manifest.json` — a listing of every
managed mod/config file with its hash, kept in sync every time the operator
publishes a change. **This is not a manual download page.** Visiting it in a
browser will show a certificate warning first — that's expected, the
certificate is issued by our own private CA, which no browser trusts by
default — and tapping through it just shows a wall of JSON, not a
zip you can download and drop into CurseForge.

This file server exists for Phase 4's launcher, which pins that same
private CA and reads the JSON automatically to know exactly which files to
fetch. Until the launcher ships, the CurseForge-app route in steps 1–5
above remains the supported way to get the pack onto your machine.

## Before Phase 4 ships

Everything above — the CurseForge app, the manual version pin, the manual
RAM slider, typing the server address by hand, and the manual token you
paste in before every join — goes away once Phase 4's launcher exists.
Don't build muscle memory around this; it's a bridge, not the final
experience.

As of the end of Phase 3: the HTTPS file server, the manifest, and the
status endpoint the launcher will use are all live and reachable from the
internet (`docs/DIST-OPS.md`'s "Phase 4 integration contract" has the full
detail a launcher implementer needs). What's still missing is the launcher
itself — nothing about how you join changes until it ships.
