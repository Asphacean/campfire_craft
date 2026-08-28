# Phase 3: Modpack Distribution - Context

**Gathered:** 2026-08-28
**Status:** Ready for planning

<domain>
## Phase Boundary

Make the exact client pack fetchable over HTTPS from mc.campfire.pub with per-file sha256, expose the auth API and a status endpoint over the same HTTPS front, and give the operator a one-command publish. Covers DIST-01…DIST-04. No launcher (Phase 4); the manifest contract is what Phase 4 consumes.

</domain>

<decisions>
## Implementation Decisions

### HTTPS front (Caddy)
- Port 443 is taken by sing-box and :80 by pbwiki's LAN-only Caddy container; do NOT touch either. Our HTTPS lives on **TCP 8444** on `mc.campfire.pub` (same A record). Router: forward TCP 8444 → Pi (operator action, checkpoint)
- TLS = **own private CA** (operator decision, option B): `ca/campfire-ca.pem` (public, committed) + `ca/campfire-ca-key.pem` (mode 600, gitignored, in backups); server cert for `mc.campfire.pub` issued by that CA, 10-year CA / ~2-year leaf with a `scripts/renew-cert.sh`. The launcher (Phase 4) pins this CA; browsers will warn — acceptable, friends only use the launcher
- Caddy installed on the HOST from the official Caddy apt repo (`caddy.service`), Caddyfile at `caddy/Caddyfile` in the repo, installed by `scripts/install-caddy.sh`; `auto_https off`, explicit `tls` with our cert
- Routes on `mc.campfire.pub:8444`: `/api/register`, `/api/login`, `/status` → reverse_proxy 127.0.0.1:8081 (strip `/api` prefix or mirror paths — planner decides, document in auth-service/README); `/api/validate` is NOT proxied (loopback-only, mod-side); `/manifest.json` and `/pack/*` → `file_server` rooted at `~/rlcraft/pack/`, GET/HEAD only, no directory browsing, no dotfiles
- Rate limiting for registration stays in the auth service (Phase 2)

### Client pack & manifest
- The client pack ≠ server pack. Base = official **RLCraft 2.9.3 client zip** from CurseForge (its `manifest.json` lists mods by projectID/fileID + `overrides/`); on top: our `server/config/` (single source of truth for configs) and the `campfire-auth-*.jar` from `server/mods/`
- **All files self-hosted** (operator decision, overrides the license-audit recommendation): every mod jar, config, script, resource is served from `/pack/`. NO CurseForge API key, no per-mod license audit. Risk (redistribution of non-redistributable mods to a closed friend group) accepted by operator. DIST-03 is therefore weakened to: *Minecraft client jar, libraries and assets are never served from our host — launcher fetches them from Mojang; everything else is self-hosted*
- `manifest.json` shape: `{ "pack_version", "mc": "1.12.2", "forge": "14.23.5.2860", "java": 8, "files": [ { "path", "sha256", "size", "url" } ], "delete": [ ... ] }`; `url` relative to `/pack/`; managed dirs = `mods/`, `config/`, `scripts/`, `resources/`, `structures/` (+ whatever the client zip's overrides contain); never `saves/`, `options.txt`, `servers.dat`, `screenshots/`, `logs/`
- Staging dir `~/rlcraft/pack/` (gitignored — hundreds of MB, reproducible). `scripts/publish-pack.sh` = one command: unpack/refresh client base (cached zip, sha-pinned like Phase 1's fetch-pack), rsync `server/config/` + campfire-auth jar over it, generate manifest atomically (tmp → mv), compute `delete[]` as diff vs previous manifest. Manual run after any mod/config change (DIST-02)
- Mods listed in the client manifest by projectID/fileID are downloaded once from CurseForge CDN into `pack/mods/` by the publish script (same unauthenticated forgecdn path Phase 1 used for the server pack); if a file's distribution is blocked, the script reports it — operator resolves manually

### Status endpoint
- `GET /status` implemented in the auth service (already stubbed in Phase 2): performs a Minecraft **Server List Ping** against 127.0.0.1:25565 (standard protocol, no password), 10 s cache, returns `{ online, players, max, motd }`; offline → `{ online:false }` with HTTP 200

### Operations & verification
- `pack/` is not backed up (reproducible); `ca/` key + `caddy/Caddyfile` are added to `scripts/backup.sh`
- Success criterion "a client assembled from the manifest connects and plays": `scripts/assemble-client.py` builds a client dir from the manifest (download + verify hashes) on the Pi as an automated proof of manifest completeness/hash correctness; the actual play test is a human check deferred with the other UAT items until the launcher exists (operator decision)

### Claude's Discretion
- Caddy version pin, cert tooling (openssl vs caddy's internal PKI vs mkcert), exact `/api` path mapping, manifest generator language (python3 preferred — already used for join-probe.py), hashing parallelism, whether `pack_version` is a timestamp or a counter

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `scripts/fetch-pack.sh` — sha-pinned CurseForge zip download pattern (reuse for the client zip)
- `auth-service/` (axum) — add real `/status` (Server List Ping) replacing the stub; `README.md` holds the API contract
- `scripts/install-units.sh`, `systemd/` — unit install pattern; `scripts/backup.sh` — flock + archive list
- `server.env` — add `HTTPS_PORT=8444`, `PACK_DIR`, `CA_DIR`; mirror in `server.env.example`
- Phase 1 `docs/CLIENT-SETUP.md` — update with manifest/HTTPS info for hand-install users

### Established Patterns
- Bash `set -euo pipefail`, idempotent installers, `bash -n`; Python 3 for protocol/tooling scripts; everything on the Pi; never reboot; game server restarts only when announced and 0 players
- Secrets mode 600 + gitignored; services loopback-first

### Integration Points
- Phase 4 launcher: `GET https://mc.campfire.pub:8444/manifest.json`, `/pack/<url>`, `POST /api/register|login`, `GET /status`; pins `ca/campfire-ca.pem`; passes `-Dcampfire.nick/-Dcampfire.token`
- Phase 5: `ca/campfire-ca.pem` must be embedded in the launcher build

</code_context>

<specifics>
## Specific Ideas

- Do not modify `~/pbwiki` or sing-box in any way
- Keep the HTTPS surface minimal: no directory listing, no PUT, no auth-service admin routes exposed

</specifics>

<deferred>
## Deferred Ideas

- Public CA (Let's Encrypt) — possible later via DNS-01 or by freeing :80; not needed while only the launcher talks to the host
- Per-mod license audit via CurseForge API — rejected by operator for now
- Launcher self-update feed (`/launcher/latest.json`) — belongs to Phase 4/5, but the file server can host it

</deferred>
