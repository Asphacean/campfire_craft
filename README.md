# Campfire Craft

A private RLCraft 2.9.3 server, running on a Raspberry Pi 5, with a
one-click launcher so friends don't have to hand-configure a modded
Minecraft client to play on it.

**Playing on this server?** Get the launcher and installation steps from
[docs/FRIENDS.md](docs/FRIENDS.md) — download the latest release at
<https://github.com/Asphacean/campfire_craft/releases/latest>.

**Building it yourself?** See [docs/LAUNCHER-BUILD.md](docs/LAUNCHER-BUILD.md)
for the launcher's build recipe and release procedure.

## For operators

- [docs/DIST-OPS.md](docs/DIST-OPS.md) — server, modpack distribution, and
  launcher update-feed operations
- [docs/AUTH-OPS.md](docs/AUTH-OPS.md) — account/token authentication
  operations
- [docs/CLIENT-SETUP.md](docs/CLIENT-SETUP.md) — the original hand-install
  client path, superseded by the launcher

No credential lives in this repository — every secret is kept outside the
tree or gitignored, and the full commit history was scanned for leaked
secrets before this repository was made public.
