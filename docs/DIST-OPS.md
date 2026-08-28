# Distribution Operations Runbook

Everything the operator needs to publish, rotate, and reason about the
public HTTPS surface, without reading any other file. `auth-service/README.md`
holds the authoritative API/route table contract; this page is the
operational half for the modpack file server (DIST-01..DIST-03).

## What `pack/` is, and why it isn't in git or the backups

`pack/` (`PACK_DIR` in `server.env`, absolute path
`/home/asphacean/rlcraft/pack`) is the entire RLCraft 2.9.3 **client** tree —
every mod jar and resource pack the client's own CurseForge manifest
references, everything its `overrides/` carries, `server/config/` overlaid on
top (single source of truth for configs), and the `campfire-auth` jar. It is
several hundred megabytes and entirely reproducible: `scripts/publish-pack.sh`
rebuilds it from CurseForge, `server/config/`, and `server/mods/` on every
run. Reproducible content that large has no business in git history or in a
six-hourly backup archive — `.gitignore` excludes `pack/` outright, and
`scripts/backup.sh` (Phase 3 plan 01) deliberately does not tar it. If the
Pi's disk is lost, `pack/` is rebuilt with one command; the CA, the accounts
database, and the world are what actually need restoring from backup.

## Publishing after a mod or config change

One command, from the repo root:

```
bash scripts/publish-pack.sh
```

This is DIST-02 in full: it re-fetches the client base zip only if its pin
in `server.env` (`CLIENT_PACK_SHA256`/`CLIENT_PACK_ZIP`) is missing or the
file on disk no longer matches it, re-fetches every CurseForge mod/resource
pack the client manifest references (skipping any already present on disk),
re-extracts `overrides/`, overlays `server/config/` and the
`campfire-auth-*.jar`, and finishes by regenerating `pack/manifest.json`
atomically via `scripts/gen-manifest.py`. Nothing else — no Caddy restart,
no manual step.

**The operator only edited a config or the auth jar changed version.** Use
the fast path — it never touches CurseForge:

```
bash scripts/publish-pack.sh --skip-fetch
```

This re-runs the overlay (`server/config/` → `pack/config/` with `--delete`,
so a config the operator removed actually disappears from the client) and
regenerates the manifest from the pack tree already on disk. It is the
common case and should complete in seconds.

**Confirm the new manifest is live:**

```
curl -s --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/manifest.json | jq '.pack_version, (.files|length), (.delete|length)'
```

A fresh `pack_version` timestamp confirms the publish landed.

## When a CurseForge file is refused

`publish-pack.sh` never publishes an incomplete pack silently. If any
`{projectID, fileID}` in the client manifest returns a non-200 response, an
unsafe filename, or an extension outside `.jar`/`.zip`, the fetch phase logs
it individually (`REFUSED [n/total] project=... file=... reason=...`),
**continues attempting every remaining file**, and only after the whole
fetch phase completes does it print the full list of refusals and exit
non-zero (exit code 4) — the manifest step is never reached, so a partial
pack is never advertised as complete (D-10).

To resolve:

1. Read the printed list — each line names the `projectID`, `fileID`, and
   the reason (an HTTP status, an unsafe filename, or an unexpected
   extension).
2. If it's a transient CDN hiccup (a single bad HTTP code), just re-run
   `bash scripts/publish-pack.sh` — already-downloaded files are skipped, so
   only the refused ones are retried.
3. If a file is persistently refused (CurseForge has pulled it, or it needs
   authentication this project deliberately doesn't carry — see
   `server.env`'s `CF_API_KEY`), download it manually and place it by hand
   under `pack/mods/` or `pack/resourcepacks/` (by its actual extension),
   then run `bash scripts/publish-pack.sh --skip-fetch` to fold it into the
   manifest without re-attempting the CurseForge fetch.
4. If CurseForge's unauthenticated redirect starts returning 403/429 across
   many files at once (bulk rate-limiting, not a single bad file), stop and
   wait — do not loop retries; the endpoint has historically resumed within
   minutes to hours per RESEARCH.md's Open Question 2.

## Certificate rotation

The CA (`ca/campfire-ca.pem`, 10-year validity) is generated once, ever. The
leaf certificate for `mc.campfire.pub` (`ca/mc.campfire.pub-cert.pem`,
~2-year validity) is what needs periodic rotation:

```
bash scripts/renew-cert.sh
sudo systemctl restart caddy
```

This reissues only the leaf, signed by the same CA — `ca/campfire-ca.pem`
itself is byte-identical before and after (verified live in 03-01). Caddy
has no admin API (`admin off`), so a config or certificate change always
requires a restart, never a live reload. Restarting Caddy does not touch
`rlcraft.service` — the game server is unaffected.

**If the CA private key (`ca/campfire-ca-key.pem`) is ever lost or leaked:**
there is no way to reissue a leaf under the old CA. Generate a brand new CA
and leaf pair, redistribute the new `ca/campfire-ca.pem` to every launcher
(Phase 4/5 embed it at build time), and every existing launcher install
must update before it can reach the HTTPS front again. This is the one
piece of key material in this project with no in-place recovery path — treat
`ca/campfire-ca-key.pem`'s backup copy (it rides the six-hourly world
archive, mode 600, gitignored) as the thing standing between "rotate the
leaf" and "redistribute a new CA to everyone."

## What a backup archive does and does not contain

Each six-hourly `world-*.tar.zst` (via `scripts/backup.sh`) contains:

- `world/` — the game world
- `auth/campfire.db` — the accounts database (Phase 2)
- `ca/` — including the CA private key, mode 600 (Phase 3 plan 01)
- `caddy/Caddyfile` — the route table (Phase 3 plan 01)

It deliberately does **not** contain `pack/` — see "What `pack/` is" above.
A full disaster recovery is: restore the archive for world/auth/ca/Caddyfile,
then run `bash scripts/publish-pack.sh` once to rebuild `pack/` from
CurseForge and the restored `server/config/`.

## The manifest schema

```json
{
  "pack_version": "2026-08-28T14:03:11Z",
  "mc": "1.12.2",
  "forge": "14.23.5.2860",
  "java": 8,
  "files": [
    { "path": "mods/SpawnerControl-1.6.3b.jar", "sha256": "<64 lowercase hex>", "size": 49537, "url": "mods/SpawnerControl-1.6.3b.jar" }
  ],
  "delete": [ "mods/SomethingRemoved.jar" ]
}
```

- `path` is relative to the client instance root; `url` is relative to the
  `/pack/` HTTPS route and is currently identical to `path` (the file server
  is rooted directly at `PACK_DIR`).
- `pack_version` is an ISO-8601 UTC timestamp, regenerated on every publish.
- Managed directories: `mods/`, `config/`, `scripts/`, `resources/`,
  `structures/`, `resourcepacks/`, and anything else the client zip's
  `overrides/` carries.
- Never managed, never in `files` and never in `delete`: `saves/`,
  `screenshots/`, `logs/`, `crash-reports/`, `options.txt`, `optionsof.txt`,
  `servers.dat`.
- `delete` is cumulative: an entry stays until the file reappears in
  `files`, so a client several publishes behind still learns about a
  removal from two publishes ago, not just the most recent one.
- The manifest is written atomically (`tempfile.mkstemp` + `os.replace`) —
  no reader ever observes it referencing files that are not all present.
- A forbidden-content gate refuses to publish if any collected path is
  `server.properties`, `ops.json`, `whitelist.json`, `usercache.json`,
  `server.env`, `eula.txt`, a `banned-*` file, a `*.db` file, or anything
  under a `saves/` component — aborting with the previous manifest
  untouched.

## The route table

Unchanged from Phase 3 plan 01. `auth-service/README.md`'s "Public route
table" section is the authoritative copy — `/manifest.json` and `/pack/*`
are the file server this document covers; `/api/register`, `/api/login`,
and `/status` are proxied to `campfire-auth`; `/api/validate` is
deliberately never routed through Caddy at all.

## The `/etc/hosts` entry, and why an outside check can't trust it

The Pi's own `/etc/hosts` maps `mc.campfire.pub` to `127.0.0.1` (added in
Phase 3 plan 01) so that every HTTPS check run **on the Pi itself** —
including everything in this runbook — reaches the local Caddy under the
certificate's own name without depending on DNS or the router's port
forward. This is a convenience for operator commands run locally; it tells
you nothing about whether a friend, connecting from outside the home
network, can actually reach the server. Any reachability check meant to
prove the outside path works must bypass this entry and resolve the real
public IP itself:

```
curl --resolve mc.campfire.pub:8444:<public-ip> --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/manifest.json
```

`--resolve` overrides normal DNS/hosts resolution for this one request,
so it proves the router's port forward and the public IP are both correct —
`/etc/hosts` on the Pi would otherwise mask a broken outside path by quietly
succeeding via loopback.

## Verifying a full client can be assembled

```
python3 scripts/assemble-client.py --dest ~/client-check
python3 scripts/assemble-client.py --dest ~/client-check --verify
```

The first run downloads and hash-verifies every manifest entry into a
directory outside the repository, trusting only `ca/campfire-ca.pem` (no
system trust store fallback — this script is the reference implementation
Phase 4's launcher mirrors). The second re-verifies without downloading
anything, and also reports any file present in a managed directory that the
manifest doesn't list. Run this after any publish you want to be sure is
actually complete and correct end-to-end, not just "the command exited 0".

## The accepted redistribution risk (D-07)

Every mod jar and resource pack this pack references is self-hosted from
`/pack/` — there is no per-mod CurseForge licence audit, and no CurseForge
API key is used anywhere in this pipeline. This is an explicit operator
decision (D-07 in `03-CONTEXT.md`), not an oversight: some mods'
distribution terms may not strictly permit third-party rehosting, and the
operator accepted that risk for redistribution to a small, closed group of
friends rather than gate the whole pipeline behind a licence-by-licence
review. What DIST-03 actually guarantees, and is enforced by
`scripts/assemble-client.py` as a hard check (not just documentation): the
Minecraft client jar, its libraries, and its assets are never served from
this host — those come from Mojang via the launcher, only mods/configs/
resource packs are self-hosted here.

## See also

- `auth-service/README.md` — the full public route table and the
  `/register`/`/login`/`/status` API contract. This page does not
  duplicate it.
- `docs/AUTH-OPS.md` — the accounts/auth-gate operational runbook
  (tokens, rollback, support answers).
