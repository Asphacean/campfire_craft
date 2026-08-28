# Auth Operations Runbook

Everything the operator needs at 2am, without reading any other file. The
API contract itself lives in `auth-service/README.md` — this page is only
the operational half: mint, reset, roll back, emergency access, and what
happened when enforcement went live.

## Minting a token

```
campfire-auth login <nick>
```

Prints exactly one line: the token, good for exactly **one** join (single-use,
12-hour TTL). Mint a fresh one for every join — reusing a token fails with
`invalid_token`. Exits non-zero for an unknown nick.

## Resetting a password

```
campfire-auth reset <nick>
```

Reads the new password from stdin, applies the same 8-character minimum as
registration. Exits non-zero for an unknown nick or a too-short password.
This is also the fix for the "my stuff is gone" support case below.

## Rollback — turning enforcement off

Enforcement is undone with one file deletion and one restart:

```
rm server/mods/campfire-auth-0.1.1.jar
sudo systemctl restart rlcraft
```

This returns the server to its Phase-1 open state (no whitelist, anyone can
join). The accounts database and every player's world data are untouched
either way — this is deliberately reversible. Take the same care as the
original enforcement restart: check `rcon-cli list` first and announce if
anyone is online.

## No bypass account, by design

There is no operator bypass list (D-10). The operator joins the game the
same way any friend does — `campfire-auth login <own-nick>` and the two
`-D` flags. If the gate itself is misbehaving and refuses everyone
including the operator, **RCON is the emergency channel**, not a special
account: `rcon-cli` reaches the server regardless of whether the auth gate
lets players in, because RCON never goes through `ServerAuthHandler`.

## Stopping `campfire-auth.service` is never the first move

Once the mod is installed, the game server calls `POST /validate` on every
join. If `campfire-auth.service` is down, every join fails closed
(`result=kick reason=service_error`) — **stopping it locks every player
out**, it does not open the gate. During an incident, check
`systemctl status campfire-auth` and `curl http://127.0.0.1:8081/status`
first; restart the unit if it's wedged, don't stop it to "get around" a
problem.

## Enforcement went live: 2026-08-28

- **Pre-enforcement backup:** `world-20260828-112834.tar.zst`
  (2026-08-28T11:28:34Z / 14:28:34 local), taken via `scripts/backup.sh`
  immediately before the restart. Verified to contain both `world/level.dat`
  and `auth/campfire.db`.
- **Players online at restart time:** 0 (`rcon-cli list` and the full
  day's server log show no player joins before the restart) — no
  announcement was sent, per the plan's own "if nobody is online, restart
  straight away" branch.
- **The one restart:** `sudo systemctl restart rlcraft` at 14:29:03 local —
  a single `Stopping` → `Stopped` → `Started` sequence in the systemd
  journal, confirmed exactly once. The Pi itself was never rebooted
  (`uptime -s` unchanged across the whole plan).
- **Mod loaded cleanly:** `server/logs/latest.log` shows `campfireauth` in
  the FML mod list with no exception attributable to it (the log's other
  stack traces are pre-existing RLCraft recipe-parsing warnings from
  unrelated mods, present on every start).
- **Live probe result:** `python3 scripts/join-probe.py 127.0.0.1 25565
  ProbeNick` (a registered nick) returned a disconnect, but **not** our
  gate's bilingual message — it was Forge's own mod-list handshake refusal:
  `"This server has mods that require FML/Forge to be installed on the
  client. Contact your server admin for more details."` This is expected
  and explained in the plan: unlike the throwaway single-mod devserver used
  in 02-02, the live server carries the full RLCraft mod list, and Forge's
  own FML handshake turns away a raw-protocol connection (no FML marker,
  missing ~200 mods) before our gate ever runs. It still proves a vanilla
  client cannot join, but it is **not** proof of our gate specifically —
  that requires a real, fully-modded RLCraft client, which is the human
  check below.
- **`campfire-auth.service`:** confirmed `active` throughout, before and
  after the restart.

### Nick inventory (existing player data)

`server/usercache.json` is `[]` and `server/world/playerdata/` is empty —
**nobody has ever joined this server**. Phase 1 ran open with no
whitelist, but no friend actually connected before enforcement went live,
so there is no existing progress at risk of being claimed by someone
registering the same nick first (D-14's residual risk this inventory
exists to catch). No nicks need to be told to register promptly; this
section stays as a record that the check was made, not a to-do list.

One account exists in the auth database: `ProbeNick`, created during this
plan's own live-probe testing — not a player account, and not part of the
Phase-1 nick inventory.

## Client verification

Pending — this project runs `human_verify_mode: end-of-phase`, so Test
A (valid token joins, can move/break/chat), Test B (no token is kicked
with the bilingual message), and Test C (optional plain-vanilla client) are
harvested into `02-UAT.md` from a real operator PC, not recorded here. See
`02-03-PLAN.md` Task 1's `<verify><human-check>` for the exact steps.
Until that check is run and reported, the "a valid token lets you in" path
is unproven — if it fails, the rollback above is one file deletion and one
restart away.

## Support answers

**"A friend's token doesn't work."** Tokens are single-use and expire in
12 hours — mint a fresh one: `campfire-auth login <their-nick>`. A token
that already worked once, or one older than 12 hours, will always fail
with `invalid_token`; that's expected, not a bug.

**"A friend says their stuff is gone."** Almost always a nick-casing
mismatch: the game derives the offline-mode player ID from the *exact*
letters of the nick, so `Steve` and `steve` are two different players with
two different inventories. Check the capitalisation the friend connected
with (via `-Dcampfire.nick=`) against the exact casing they registered
with. The fix is to reconnect using the exact registered casing. If the
dispute is genuine (two people registered variants of the same name and
one wants the other's account), the remedy is `campfire-auth reset
<nick>` on the account that should be kept, and the loser re-registers
under a different nick.

## See also

- `auth-service/README.md` — the full API contract (`/register`,
  `/login`, `/validate`, `/status`, error codes, CLI reference). This page
  does not duplicate it.
- `docs/CLIENT-SETUP.md` — the friend-facing hand-install path, including
  the token flow.
