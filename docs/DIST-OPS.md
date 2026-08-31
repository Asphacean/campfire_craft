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

## Phase 4 integration contract

The single document Phase 4's planner and executor read instead of
re-deriving the HTTPS surface. Router forward (TCP 8444 → the Pi) is in
place as of 2026-08-28; the router does NAT hairpin (confirmed both from the
Pi itself and from independent external vantage points — see "Router
forward result" below).

### Base URL

`https://mc.campfire.pub:8444`. The port is **8444, not 443**, because :443
already belongs to sing-box and :80/:8443 already belong to the pbwiki
containers on this same host (D-14) — neither is ever touched by this
project, so the HTTPS front for the modpack lives on its own dedicated port
for the life of this host.

### The full route table

`auth-service/README.md`'s "Public route table" section is the authoritative
copy for the API half; this table restates the complete published surface
so Phase 4 never has to open a second document mid-implementation.

| Public route | Method | Reaches | Returns |
|---|---|---|---|
| `/manifest.json` | GET, HEAD | `pack/manifest.json` via Caddy's `file_server` | The pack contract (schema below) |
| `/pack/<url>` | GET, HEAD | `pack/<url>` via Caddy's `file_server` | A managed file; `<url>` is a `files[].url` value verbatim |
| `/api/register` | POST | `campfire-auth` `/register` over loopback (127.0.0.1:8081) | 201, or 400/409/429 with a stable `{"error":"<code>"}` |
| `/api/login` | POST | `campfire-auth` `/login` over loopback | `{"token","expires","refresh"}`, or 400/401/429 |
| `/api/refresh` | POST | `campfire-auth` `/refresh` over loopback | `{"token","expires","refresh"}` (rotated), or 400/401/429 — see `auth-service/README.md`'s `POST /refresh` |
| `/status` | GET | `campfire-auth` `/status` over loopback | `{"online","players","max","motd"}`; offline is HTTP 200 with `online:false`, never a 5xx |
| `/launcher/<file>` | GET, HEAD | `file_server` at `launcher-dist/<file>` (Phase 4, outside `PACK_DIR`) | The self-update feed's static tree |
| anything else | any | — | 404 from Caddy's terminal handler |
| non-GET/HEAD on `/manifest.json`, `/pack/*` or `/launcher/*` | — | — | 405 |

**`/api/validate` has no public route and must never be given one.** It is
unauthenticated beyond the token itself and deliberately never rate
limited, because it is the join path — its only legitimate caller is the
auth-gate Forge mod over loopback on this same host. There is no `/api/*`
wildcard in `caddy/Caddyfile`; adding one would republish it. Phase 4's
launcher never calls this endpoint directly, under any circumstance.

### The manifest schema

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
  `/pack/` HTTPS route (currently identical to `path`).
- `pack_version` is an ISO-8601 UTC timestamp, regenerated on every publish.
- **Managed** (appear in `files` and can appear in `delete`): `mods/`,
  `config/`, `scripts/`, `resources/`, `structures/`, `resourcepacks/`, and
  anything else the client zip's `overrides/` carries.
- **Never managed** (never in `files`, never in `delete`, the launcher must
  never touch these): `saves/`, `screenshots/`, `logs/`, `crash-reports/`,
  `options.txt`, `optionsof.txt`, `servers.dat`.
- `delete` is **cumulative**: a removed file's path stays in `delete` until
  it reappears in `files`, so a launcher install that is several publishes
  behind still learns about a removal from two publishes ago, not just the
  most recent one — the launcher must not assume `delete` only ever
  contains the previous publish's removals.

### The trust anchor — pinning is the only defense, not the hashes

**The launcher must pin `ca/campfire-ca.pem` and must disable the built-in
root certificate store, so that our CA is the only certificate authority it
will accept for this hostname.** This is a requirement, not a suggestion.

Why: the manifest's per-file hashes are served by the same host that serves
the files. An attacker who can impersonate `mc.campfire.pub` — DNS
hijack, a compromised CDN, a MITM on the friend's network — serves both a
forged file and a matching forged hash computed over that same forged file.
Hash verification alone proves internal consistency between the manifest
and the download; it proves nothing about who served either of them.
**TLS pinning to our own private CA is the only trust anchor in this
design.** A launcher that verified every hash correctly but accepted any
publicly-trusted certificate for this hostname would have no security at
all against that attack.

`scripts/assemble-client.py` is the reference implementation of exactly
this: it trusts only `ca/campfire-ca.pem`, with no system-trust-store
fallback, and Phase 4's launcher must mirror that behaviour exactly.
Phase 5 must embed the CA certificate in the shipped binaries so a fresh
install has it before ever making its first request.

### Nick casing — the one contract item that silently destroys progress

The launcher must always pass through the exact casing `/api/validate`'s
response returns, **never** a player-retyped variant, and never the
lowercased uniqueness key used for registration matching. Minecraft's
offline-mode UUID is derived from the exact username byte string
(`UUID.nameUUIDFromBytes("OfflinePlayer:" + nick)`), so a differently-cased
connection computes a different UUID and the player silently loses their
inventory and progress — with no error, no warning, just an empty world
that looks like data loss. See `auth-service/README.md`'s "Nick casing"
note for the full mechanism.

### Two known gaps this phase deliberately leaves for Phase 4

1. **`options.txt` / `optionsof.txt` seeding.** The base pack ships tuned
   defaults for both files, but the locked manifest schema above explicitly
   never manages either of them (they are player state, correctly excluded
   from `delete` so a player's tuned settings are never clobbered by a
   publish). This means the manifest alone cannot deliver the pack's
   intended tuned defaults to a brand-new install. The launcher must seed
   its own default template for both files on first install, and must never
   overwrite either file on any subsequent sync once it exists.
2. **The file server is not a manual-download path.** A friend's browser
   will show a certificate warning when visiting `/manifest.json` or
   `/pack/*` directly, because the CA is private and no browser trusts it.
   That is fine — friends are expected to use the launcher, which pins the
   CA and never warns — but it means this HTTPS front cannot double as a
   "click here to download the pack" page for a non-technical person. The
   CurseForge hand-install path in `docs/CLIENT-SETUP.md` remains the only
   supported manual route until the launcher ships.

### The launcher self-update feed (Phase 4 plan 04, LNCH-08)

`/launcher/latest.json` is published in two places, on purpose, because the
two consumers use different HTTP clients with different trust roots:

- **`campfire_launcher_core::update::check`** (the silent startup banner
  check, LNCH-08) fetches `/launcher/latest.json` from this Pi over the
  same pinned CA/client as every other campfire.pub request
  (`http::campfire_client()`). This is our own host, so a private CA is
  fine here.
- **`tauri-plugin-updater`** (the actual signed download-and-install behind
  "Update Now") uses its own `reqwest` client, built with the plugin's
  default trust store — public webpki roots only, no way to hand it our
  private CA. Pointed at `https://mc.campfire.pub:8444`, this client's TLS
  handshake fails outright (**the v0.1.5 Mac UAT bug**: the modal correctly
  offered 0.1.6, but "Update Now" always failed with nothing useful logged,
  because `install_update` had no error logging of its own either — fixed
  alongside the endpoint change below). `tauri.conf.json`'s
  `plugins.updater.endpoints` therefore points at
  `https://github.com/Asphacean/campfire_craft/releases/latest/download/latest.json`
  instead — a public host the plugin's default trust store already
  handles, serving the exact same file. The minisign `pubkey` embedded in
  the binary is unchanged; trust still comes from the per-platform Ed25519
  signature inside the feed (produced by the same pi-only key either way),
  not from which host served the JSON — GitHub is transport only.
  `releases/latest/download/<name>` always resolves to the current
  release's own asset, so this needs no per-tag URL.

The schema (`version`, `notes`, `pub_date`,
`platforms.{windows-x86_64,darwin-x86_64,darwin-aarch64}.{url,signature}`)
is `tauri-plugin-updater`'s own either way — the actual signed
download-and-install goes through the plugin's own `Updater`/`Update`
types in `src-tauri`, not this crate.

**Back-compat gap.** Any launcher already installed at 0.1.5 or 0.1.6 has
the old `:8444` endpoint baked into its own binary's `tauri.conf.json` at
build time — a config value, not something a running app can update in
place. Those installs' "Update Now" will keep failing (now with a real
log line explaining why) until the person reinstalls from
`releases/latest` by hand once; see `docs/FRIENDS.md`. As of this fix,
only the operator's own 0.1.5/0.1.6 test installs are affected — no friend
has installed yet.

**Artifact naming** (Tauri's own updater-artifact convention;
`scripts/publish-launcher.sh` reads the platform straight off the
filename, refusing anything it can't parse):

| Filename shape | Platform key |
|---|---|
| `<name>_<version>_x64-setup.exe` | `windows-x86_64` |
| `<name>_<version>_x64_en-US.msi` | `windows-x86_64` |
| `<name>_<version>_x64.app.tar.gz` | `darwin-x86_64` |
| `<name>_<version>_aarch64.app.tar.gz` | `darwin-aarch64` |

**Publishing a new launcher build.** Run `scripts/publish-launcher.sh
--version <X.Y.Z> [--notes <text>] <artifact> [<artifact> ...]` from the
repository root, once per release, with every platform's build output
built per `docs/LAUNCHER-BUILD.md`. The script copies each artifact into
`launcher-dist/` (served at `/launcher/*` above, outside `PACK_DIR`),
signs it with the operator's own minisign key, and writes
`latest.json` atomically (temp file + same-directory rename) — one
re-runnable step, same shape as `scripts/publish-pack.sh`. Confirm with
the `curl --cacert ca/campfire-ca.pem` command the script prints at the
end. `release.yml`'s `publish` job runs this script and then, as its own
next step, uploads the freshly written `launcher-dist/latest.json` as a
`latest.json` release asset on the same tag (deleting any prior asset of
that name first, so re-running a tag is idempotent) — that upload is what
`tauri-plugin-updater`'s GitHub-hosted endpoint actually serves. A manual,
by-hand run of this script (e.g. to republish an old version to the Pi
feed) does **not** touch the GitHub release asset; only the CI job does.

**Key custody.** The signing keypair was generated once with `cargo tauri
signer generate`. The **private key lives at `~/.tauri/campfire.key` on
this Pi only** (mode 600) — the checkpoint's "pi-only" choice — and is
deliberately **not** added to the `BACKUP_DIR` backup set above; its
password lives in `server.env`'s `LAUNCHER_SIGNING_KEY_PASSWORD`
(gitignored, same file as `RCON_PASSWORD`), documented empty in
`server.env.example`. The public half is compiled into every launcher
binary via `tauri.conf.json`'s `plugins.updater.pubkey`.

**The consequence of losing it.** This host's system disk is an SSD
(not an SD card, which softens the disk-death risk a Pi normally carries),
but if `~/.tauri/campfire.key` or its password is ever lost anyway — disk
failure, accidental deletion, no note taken of the password — **every
already-installed launcher's self-update permanently stops working**: the
public key baked into those binaries has no matching private key left to
sign anything they'll accept, and minisign has no password-reset path.
Recovery is not "restore the key" — it means generating a brand-new
keypair, shipping a new launcher build that embeds the new public key via
Phase 5's release process, and asking every friend to reinstall by hand
once. This is an accepted, explicit consequence of the checkpoint's
choice, not an oversight.

### Router forward result (2026-08-28)

One rule was added: **TCP 8444 → the Pi's LAN address, external port 8444**
— no range, no DMZ. The existing TCP 25565 rule from Phase 1 is untouched.
Confirmed by three independent lines of evidence, because the operator's
phone was not available for the in-hand check this plan originally
specified:

- **From the Pi itself**, `bash scripts/reachability.sh --https` (which
  forces the connection to the resolved public IP via `curl --resolve`, so
  the Pi's own `/etc/hosts` entry cannot fake a pass) returned
  `VERDICT: PASS` with exit 0 — proof this router does NAT hairpin.
- **From three external networks**, a check-host.net raw TCP probe against
  `91.193.195.130:8444` succeeded from Hong Kong, Sweden, and Miami (USA)
  nodes, each with a clean connect time and no error.
- **From three more external networks**, a check-host.net HTTP probe
  against `https://mc.campfire.pub:8444/manifest.json` returned `200 OK`
  from Bulgaria, Iran, and Ukraine nodes.
- As a negative control, the same probe against `91.193.195.130:22`
  (SSH, never forwarded) returned "Connection refused" from two external
  nodes — confirming the router forward is scoped to exactly 8444, not
  wider.

Six independent external successes against zero external successes for a
deliberately-unforwarded port is stronger evidence than the phone check
this plan specified, and both proofs answer the identical question: is
`mc.campfire.pub:8444` reachable from outside the operator's home network.
The phone-in-hand check was not performed; it is not needed given the
above, but remains available to the operator at any time as a spot-check.

## See also

- `auth-service/README.md` — the full public route table and the
  `/register`/`/login`/`/status` API contract. This page does not
  duplicate it.
- `docs/AUTH-OPS.md` — the accounts/auth-gate operational runbook
- `docs/LAUNCHER-BUILD.md` — build-from-source instructions for Windows
  x64 and Apple Silicon, the headless proof-harness subcommand list, and
  the full Phase 4 operator QA matrix
  (tokens, rollback, support answers).
