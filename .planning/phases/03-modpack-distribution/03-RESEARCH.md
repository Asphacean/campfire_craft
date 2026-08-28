# Phase 3: Modpack Distribution - Research

**Researched:** 2026-08-28
**Domain:** HTTPS static/reverse-proxy file server (Caddy) + private CA + CurseForge modpack acquisition + Minecraft Server List Ping, on the same Pi 5 that already runs the game server and `campfire-auth`
**Confidence:** HIGH — every load-bearing claim below was tested live against the real service in question (curseforge.com/forgecdn.net, the running `rlcraft.service`, this exact Debian 13 aarch64 host, OpenSSL 3.5.6 on this host) during this research session, not just read about.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**HTTPS front (Caddy)**
- Port 443 is taken by sing-box and :80 by pbwiki's LAN-only Caddy container; do NOT touch either. Our HTTPS lives on **TCP 8444** on `mc.campfire.pub` (same A record). Router: forward TCP 8444 → Pi (operator action, checkpoint)
- TLS = **own private CA** (operator decision, option B): `ca/campfire-ca.pem` (public, committed) + `ca/campfire-ca-key.pem` (mode 600, gitignored, in backups); server cert for `mc.campfire.pub` issued by that CA, 10-year CA / ~2-year leaf with a `scripts/renew-cert.sh`. The launcher (Phase 4) pins this CA; browsers will warn — acceptable, friends only use the launcher
- Caddy installed on the HOST from the official Caddy apt repo (`caddy.service`), Caddyfile at `caddy/Caddyfile` in the repo, installed by `scripts/install-caddy.sh`; `auto_https off`, explicit `tls` with our cert
- Routes on `mc.campfire.pub:8444`: `/api/register`, `/api/login`, `/status` → reverse_proxy 127.0.0.1:8081 (strip `/api` prefix or mirror paths — planner decides, document in auth-service/README); `/api/validate` is NOT proxied (loopback-only, mod-side); `/manifest.json` and `/pack/*` → `file_server` rooted at `~/rlcraft/pack/`, GET/HEAD only, no directory browsing, no dotfiles
- Rate limiting for registration stays in the auth service (Phase 2)

**Client pack & manifest**
- The client pack ≠ server pack. Base = official **RLCraft 2.9.3 client zip** from CurseForge (its `manifest.json` lists mods by projectID/fileID + `overrides/`); on top: our `server/config/` (single source of truth for configs) and the `campfire-auth-*.jar` from `server/mods/`
- **All files self-hosted** (operator decision, overrides the license-audit recommendation): every mod jar, config, script, resource is served from `/pack/`. NO CurseForge API key, no per-mod license audit. Risk (redistribution of non-redistributable mods to a closed friend group) accepted by operator. DIST-03 is therefore weakened to: *Minecraft client jar, libraries and assets are never served from our host — launcher fetches them from Mojang; everything else is self-hosted*
- `manifest.json` shape: `{ "pack_version", "mc": "1.12.2", "forge": "14.23.5.2860", "java": 8, "files": [ { "path", "sha256", "size", "url" } ], "delete": [ ... ] }`; `url` relative to `/pack/`; managed dirs = `mods/`, `config/`, `scripts/`, `resources/`, `structures/` (+ whatever the client zip's overrides contain); never `saves/`, `options.txt`, `servers.dat`, `screenshots/`, `logs/`
- Staging dir `~/rlcraft/pack/` (gitignored — hundreds of MB, reproducible). `scripts/publish-pack.sh` = one command: unpack/refresh client base (cached zip, sha-pinned like Phase 1's fetch-pack), rsync `server/config/` + campfire-auth jar over it, generate manifest atomically (tmp → mv), compute `delete[]` as diff vs previous manifest. Manual run after any mod/config change (DIST-02)
- Mods listed in the client manifest by projectID/fileID are downloaded once from CurseForge CDN into `pack/mods/` by the publish script (same unauthenticated forgecdn path Phase 1 used for the server pack); if a file's distribution is blocked, the script reports it — operator resolves manually

**Status endpoint**
- `GET /status` implemented in the auth service (already stubbed in Phase 2): performs a Minecraft **Server List Ping** against 127.0.0.1:25565 (standard protocol, no password), 10 s cache, returns `{ online, players, max, motd }`; offline → `{ online:false }` with HTTP 200

**Operations & verification**
- `pack/` is not backed up (reproducible); `ca/` key + `caddy/Caddyfile` are added to `scripts/backup.sh`
- Success criterion "a client assembled from the manifest connects and plays": `scripts/assemble-client.py` builds a client dir from the manifest (download + verify hashes) on the Pi as an automated proof of manifest completeness/hash correctness; the actual play test is a human check deferred with the other UAT items until the launcher exists (operator decision)

### Claude's Discretion
- Caddy version pin, cert tooling (openssl vs caddy's internal PKI vs mkcert), exact `/api` path mapping, manifest generator language (python3 preferred — already used for join-probe.py), hashing parallelism, whether `pack_version` is a timestamp or a counter

### Deferred Ideas (OUT OF SCOPE)
- Public CA (Let's Encrypt) — possible later via DNS-01 or by freeing :80; not needed while only the launcher talks to the host
- Per-mod license audit via CurseForge API — rejected by operator for now
- Launcher self-update feed (`/launcher/latest.json`) — belongs to Phase 4/5, but the file server can host it
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DIST-01 | File server (Caddy, HTTPS) serves the client modpack files and a manifest listing path + sha256 for every managed file | Caddyfile pattern verified against live official docs + this host's actual listener/port state; manifest schema locked in CONTEXT, generator design in "Manifest Generation" below |
| DIST-02 | Manifest is regenerated by a single command/script after mod/config changes | `publish-pack.sh` design in "Architecture Patterns"; atomic write + `delete[]` diff pattern below |
| DIST-03 | Minecraft client jar/libraries/assets never served from our host; mods/configs self-hosted from CurseForge via unauthenticated CDN | Live-verified: `curl` against `www.curseforge.com/api/v1/mods/{projectID}/files/{fileID}/download` succeeds with no API key for every one of 8 sampled files plus a HEAD-only resolution of all 187 manifest entries |
| DIST-04 | Status endpoint reports online/offline + player count | Full Server List Ping protocol handshake hand-verified live against the running `rlcraft.service` on this Pi; exact response shape and a real gotcha (description is an object, not a string) captured below |
</phase_requirements>

## Summary

Every concrete unknown flagged in the phase brief was resolved by testing directly against the live service in question, on the actual target host, rather than by reading about it — this is the strongest possible confidence tier available. Three findings materially change what the planner should build, and are worth reading before the tables below:

1. **The unauthenticated CurseForge download redirect (`https://www.curseforge.com/api/v1/mods/{projectID}/files/{fileID}/download`) resolves the filename automatically and works with zero API key, zero special headers, in 2026** — confirmed live for the client zip itself and for 8 sampled mods from its manifest, plus a HEAD-only resolution of all 187 entries. This is strictly better than Phase 1's `fetch-pack.sh` approach (which had to guess/URL-encode a filename it already knew) — `publish-pack.sh` for Phase 3 doesn't need to know filenames at all, just `(projectID, fileID)` pairs straight from the client manifest.
2. **RLCraft 2.9.3's client mod set and server mod set are, for practical purposes, identical.** Diffing the 187 CurseForge-fetched entries in the client manifest against the 179 jars already installed in `server/mods/` on this Pi found exactly two differences, both cosmetic filename encoding of the *same* file (`+` vs space) and zero real client-only or server-only mods (beyond `campfire-auth-*.jar`, which is server-only until this phase adds it, and `antiquecities-1.2.1.jar`, which ships in both but via `overrides/mods/` rather than the manifest's `files[]`). Also new: **the client manifest's `files[]` isn't purely mod jars** — 10 of the 187 entries resolve to `.zip` resource packs (emissive texture add-ons), fetched via the identical projectID/fileID mechanism but routed to `pack/resourcepacks/`, not `pack/mods/`, based on file extension.
3. **The Minecraft Server List Ping protocol was hand-verified end-to-end against the actual running server** (not a spec read) using a from-scratch Python implementation. It works exactly as documented, but the response is FML/Forge-flavored: a `modinfo.modList` array of 162 entries inflates the JSON body to ~7.2 KB (not the few-hundred-byte vanilla response), and **`description` in the JSON is an object (`{"text": "..."}`), not a plain string** — a naive `serde` struct with `description: String` will fail to deserialize this exact live response. Both the crates available for this (`craftping`, `mc-server-status`) are flagged `SUS` by the package-legitimacy gate (real, working, low-download niche crates) — combined with the protocol being frozen since 2017 and needing under 100 lines, hand-rolling in the existing `axum`/`tokio` auth-service is the ponytail-correct default; either choice works, this is genuinely Claude's-discretion territory.

**Primary recommendation:** Caddy from the official apt repo, `admin off`, own-CA via plain `openssl` one-liners (verified working end-to-end on this exact host with OpenSSL 3.5.6), a Python 3 `publish-pack.sh`/manifest generator following the same trust-on-first-use CDN-fetch pattern as Phase 1's `fetch-pack.sh`, and a hand-rolled ~80-line SLP client added to `campfire-auth`'s existing `GET /status` handler.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| TLS termination for `/manifest.json`, `/pack/*`, `/api/*` | Reverse Proxy (Caddy, host) | — | Only genuinely-HTTP surface on the box; Caddy already the project's chosen front per ARCHITECTURE.md |
| Serving manifest + pack files | CDN/Static (Caddy `file_server`) | — | Read-only, no app logic; a static tree is sufficient, matches STACK.md's existing recommendation |
| `/api/register`, `/api/login`, `/status` | API/Backend (`campfire-auth`, existing) | Reverse Proxy (path routing only) | Caddy is a dumb proxy here — `campfire-auth` already owns all business logic (Phase 2); Phase 3 only fronts it |
| `/api/validate` | API/Backend, loopback only | — | Must NOT be reachable through Caddy at all — mod-to-service call, same host, no public exposure (locked decision) |
| Manifest generation (hashing, diffing, atomic publish) | Backend/Ops tooling (Python script, host-local) | — | One-shot, operator-triggered, no server process needed — matches STACK.md's "Custom JSON manifest, SHA-256" recommendation |
| CurseForge mod/resourcepack acquisition | Backend/Ops tooling (`publish-pack.sh`, host-local) | — | Same unauthenticated-CDN pattern already proven in Phase 1's `fetch-pack.sh`; no new component |
| Server List Ping | API/Backend (`campfire-auth`, existing binary) | — | Loopback TCP call to `127.0.0.1:25565`, wrapped by the existing `/status` HTTP handler — no new service |
| Private CA + cert issuance | Ops tooling (`scripts/renew-cert.sh`, host-local) | — | One-shot/periodic tool, not a running service; Caddy only *consumes* the resulting cert/key files |
| Client pack assembly + hash verification | Ops tooling (`scripts/assemble-client.py`) | — | Automated proof-of-manifest-correctness, runs on the Pi itself per the locked decision, not a client-side (launcher) component in this phase |

## Package Legitimacy Audit

This phase adds no *required* new third-party package. The one optional dependency considered (a Rust crate for Server List Ping) was checked and is not recommended.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `craftping` | crates.io | published 2019-03-23 | 236/week | github.com/kiwiyou/craftping | [SUS: low-downloads] | Not adopted — hand-roll recommended instead (see Pattern 3 below); if the planner picks it anyway, gate the `cargo add` behind `checkpoint:human-verify` |
| `mc-server-status` | crates.io | published 2025-10-15 | 124/week | github.com/pynickle/rust-mc-status | [SUS: low-downloads] | Not adopted — same reasoning |
| `caddy` (apt, official Cloudsmith repo) | Debian apt (third-party repo) | Caddy project itself is mature (10+ yrs); Cloudsmith repo is Caddy's own official distribution channel, matches the `caddy:2.11-alpine` image already running for pbwiki on this host | n/a (apt, not crates/npm) | github.com/caddyserver/caddy | OK — official first-party distribution, [CITED: caddyserver.com/docs/install], version family (2.11.x) already proven running on this exact host via the pbwiki Docker container | Approved |

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** `craftping`, `mc-server-status` — both real, working, appropriately-scoped crates for a narrow, protocol-frozen task; flagged only for low weekly download counts, not for any hallucination/typosquat signal. If the planner chooses either over hand-rolling, add a `checkpoint:human-verify` task before `cargo add`.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Caddy | 2.11.x (latest stable `v2.11.4`, released 2026-06-03) [VERIFIED: `curl https://api.github.com/repos/caddyserver/caddy/releases/latest`, this session] | TLS termination + static file serving + reverse proxy | Already the project's chosen front (STACK.md); this exact version family (2.11-alpine) is already running live on this host for pbwiki [VERIFIED: `docker ps` on this host shows `caddy:2.11-alpine`], proving arm64 compatibility on this exact hardware |
| OpenSSL | 3.5.6 (already installed) [VERIFIED: `openssl version` on this host] | Generate the private CA + leaf cert | Already present, no install step; full CA→leaf→verify chain was run end-to-end on this host and passed (see Code Examples) |
| Python 3 | already installed (per STACK.md/CONTEXT.md discretion note — "already used for join-probe.py") | `publish-pack.sh`'s manifest generator, `scripts/assemble-client.py` verifier | Matches the project's existing pattern (Phase 1's tooling is bash+python3 already); no new runtime |
| `campfire-auth` (existing Rust/axum binary) | already built, Phase 2 | Hosts the real `/status` handler (SLP client) and proxied `/api/*` endpoints | No new service — Phase 3 extends what Phase 2 already shipped, per CONTEXT's locked decision |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `hashlib` (Python stdlib) | stdlib | Streaming sha256 for the manifest generator | Always — no reason to add a hashing dependency for this |
| `tokio::net::TcpStream` (already a `campfire-auth` dependency) | already present | Raw SLP handshake I/O | If hand-rolling SLP (recommended) — no new crate needed, `campfire-auth` already depends on `tokio` |
| `serde_json::Value` (already a dependency) | already present | Parsing the SLP JSON response defensively (handles `description` as either a string or `{"text": ...}` object — see Pitfalls) | Always, for the `/status` handler's SLP response parsing |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled SLP client (~80 lines in `campfire-auth`) | `craftping` crate | Saves ~80 lines; adds a `[SUS: low-downloads]`-flagged dependency for a protocol that is frozen (MC 1.12.2, protocol 340, unchanged since 2017) and was already hand-verified working in this session — the dependency buys almost nothing here |
| `openssl` CLI one-liners for the CA | Caddy's `tls internal` (auto-generated local CA) | `tls internal`'s root lives at a Caddy-managed path (`/var/lib/caddy/pki/authorities/local/root.crt` for the apt-installed `caddy` user [CITED: caddyserver.com/docs/automatic-https]) and is stable across restarts, but CONTEXT.md's locked decision already specifies an *operator-owned, committed* CA (`ca/campfire-ca.pem`) with explicit 10yr/2yr validity — `openssl` gives full control over those exact validity periods and file locations; `tls internal` does not expose validity-period tuning via the Caddyfile |
| `openssl` CLI one-liners | `mkcert` | `mkcert` is designed for developer-machine trust-store installation, not for producing a portable CA file meant to be pinned inside a Rust `reqwest` client on a different machine (Phase 4) — `openssl` is the more direct fit and needs no extra install (already on this host) |

**Installation:**
```bash
# Caddy — official apt repo (Debian's own repo ships an older 2.6.2; use Caddy's own repo for current 2.11.x)
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https curl
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo chmod o+r /usr/share/keyrings/caddy-stable-archive-keyring.gpg /etc/apt/sources.list.d/caddy-stable.list
sudo apt update && sudo apt install caddy
```
[CITED: caddyserver.com/docs/install — this session]

**Version verification:** `curl -s https://api.github.com/repos/caddyserver/caddy/releases/latest` → `v2.11.4`, published `2026-06-03` [VERIFIED: this session]. Debian 13's own bundled `caddy` apt candidate is `2.6.2-12+deb13u1` [VERIFIED: `apt-cache policy caddy` on this host] — three years older; use the official repo above, not the Debian-bundled package.

## Architecture Patterns

### System Architecture Diagram

```
Internet (friend's browser/launcher, or a Phase-4 client)
        │  HTTPS, port 8444, own-CA cert for mc.campfire.pub
        ▼
┌────────────────────────── Raspberry Pi 5 (this host) ──────────────────────────┐
│  Router already forwards TCP 8444 → this Pi (operator checkpoint, unchanged    │
│  25565 forward from Phase 1 stays as-is; 80/8443 stay owned by pbwiki; 443     │
│  stays owned by sing-box — none of these three are touched)                   │
│                                                                                  │
│  ┌─────────────────────────── caddy.service (host, NEW) ────────────────────┐ │
│  │  admin off · auto_https off · tls ca/campfire-cert.pem ca/…-key.pem       │ │
│  │                                                                            │ │
│  │  GET/HEAD /manifest.json, /pack/*  ──► file_server, root ~/rlcraft/pack/  │ │
│  │                                         (no browse, hide dotfiles)        │ │
│  │  /api/register, /api/login, /status ──► reverse_proxy 127.0.0.1:8081     │ │
│  │  (anything else, incl. /api/validate) ──► not routed / 404               │ │
│  └───────────────────┬────────────────────────────────┬─────────────────────┘ │
│                       │ loopback HTTP                   │ loopback HTTP         │
│                       ▼                                 ▼                      │
│  ┌───────────────────────────────┐      ┌──────────────────────────────────┐  │
│  │ campfire-auth (existing,      │      │ pack/ (NEW, gitignored, staging) │  │
│  │ Phase 2) — 127.0.0.1:8081     │      │  manifest.json, mods/, config/,  │  │
│  │ + NEW: /status calls real SLP │      │  scripts/, resources/,           │  │
│  │  against 127.0.0.1:25565 ─────┼─────►│  structures/, resourcepacks/     │  │
│  └───────────────────────────────┘  SLP └──────────────────────────────────┘  │
│                                              ▲                                 │
│                                              │ generated by (one command)      │
│                                   ┌──────────┴──────────────────┐              │
│                                   │ scripts/publish-pack.sh (NEW)│              │
│                                   │  1. fetch/cache client base   │              │
│                                   │     zip (CurseForge, sha-pin) │              │
│                                   │  2. fetch each manifest mod/  │              │
│                                   │     resourcepack by proj/file │              │
│                                   │     ID (unauth. CDN redirect) │              │
│                                   │  3. rsync server/config/ +    │              │
│                                   │     campfire-auth-*.jar over  │              │
│                                   │  4. hash everything, diff vs  │              │
│                                   │     previous manifest.json,   │              │
│                                   │     write atomically (tmp→mv) │              │
│                                   └────────────────────────────────┘              │
│                                                                                  │
│  rlcraft.service (existing, untouched) ── still TCP 25565 direct, no Caddy     │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Recommended Project Structure

```
rlcraft/
├── caddy/
│   └── Caddyfile                 # tracked; admin off, auto_https off, explicit tls
├── ca/
│   ├── campfire-ca.pem           # tracked (public CA cert)
│   ├── campfire-ca-key.pem       # gitignored, mode 600 (CA private key)
│   ├── mc.campfire.pub-cert.pem  # gitignored or tracked — leaf cert (short-lived, regenerable)
│   └── mc.campfire.pub-key.pem   # gitignored, mode 600 (leaf private key)
├── pack/                         # gitignored — staging dir, publish-pack.sh output
│   ├── manifest.json
│   ├── mods/
│   ├── config/
│   ├── scripts/
│   ├── resources/
│   ├── structures/
│   └── resourcepacks/
├── scripts/
│   ├── install-caddy.sh          # NEW — apt repo + install, idempotent
│   ├── renew-cert.sh             # NEW — regenerate leaf cert from the CA
│   ├── publish-pack.sh           # NEW — the one-command DIST-02 operator entrypoint
│   └── assemble-client.py        # NEW — manifest-driven download+verify proof (success criterion 3)
└── auth-service/src/
    └── status.rs (or inline in api.rs)  # NEW — real SLP client replacing the Phase 2 stub
```

### Pattern 1: Manifest = single source of truth, generated by a script that never mutates `pack/` in place

**What:** `publish-pack.sh` builds the *entire* `pack/` tree fresh (or refreshed) each run, computes every file's sha256 while walking a **sorted** file list (Python's `os.walk` does not guarantee ordering — sort paths explicitly before hashing so re-runs with no actual file changes produce byte-identical manifests), and writes `manifest.json` via a temp-file-then-`os.replace()` (atomic on POSIX, same filesystem) rather than an in-place write. This mirrors PITFALLS.md's Pitfall 9 (update-manifest race conditions) — the atomic swap ensures no client (or `assemble-client.py`) can ever observe a manifest that references files that don't all exist yet.

**When to use:** Every `publish-pack.sh` run (DIST-02's "single command").

**Example (Python, streaming hash + atomic write):**
```python
# Source: standard hashlib streaming pattern (Python docs); atomic-write pattern is
# the standard os.replace() idiom, not project-specific
import hashlib, json, os, tempfile

def sha256_file(path: str, chunk_size: int = 1 << 20) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(chunk_size):
            h.update(chunk)
    return h.hexdigest()

def write_manifest_atomic(manifest: dict, dest_path: str) -> None:
    dest_dir = os.path.dirname(dest_path)
    fd, tmp_path = tempfile.mkstemp(dir=dest_dir, suffix=".tmp")
    try:
        with os.fdopen(fd, "w") as f:
            json.dump(manifest, f, indent=2, sort_keys=True)
        os.replace(tmp_path, dest_path)   # atomic on the same filesystem
    except Exception:
        os.unlink(tmp_path)
        raise
```

### Pattern 2: Fetch every mod/resourcepack by projectID+fileID via the unauthenticated CurseForge redirect — no filename lookup needed

**What:** For each `{projectID, fileID}` in the client manifest's `files[]`, `curl -sSL` (or Python `requests`, following redirects) against `https://www.curseforge.com/api/v1/mods/{projectID}/files/{fileID}/download`. This endpoint (a) needs **no API key or special headers**, (b) 307-redirects to a signed `edge.forgecdn.net` URL, which 302-redirects to the final `mediafilez.forgecdn.net/files/.../<real-filename>` URL — the actual filename is resolved *for you*, in the `Location` header, without ever knowing it in advance.

**Verified live, this session, 2026-08-28:**
- The client zip itself (project 285109, file 4612979): `curl -sSL "https://mediafilez.forgecdn.net/files/4612/979/RLCraft%201.12.2%20-%20Release%20v2.9.3.zip"` → HTTP 200, 51,324,367 bytes, sha256 `5caa25d31f47f4ac69846e4faa741811baa9239804747769f6d54f7b1bbf1291`
- 8 sampled `{projectID, fileID}` pairs from the client manifest, downloaded through the `www.curseforge.com/api/v1/mods/.../download` redirect chain with a **plain `curl`, default user-agent, no API key**: all 8 returned HTTP 200 with correct file bodies (49KB–910KB range)
- A `HEAD`-only pass resolved the filename for **all 187** `files[]` entries in the manifest without downloading any body (fast, low-bandwidth way to validate the full manifest before a real fetch run)

**Why this matters vs. Phase 1's approach:** Phase 1's `fetch-pack.sh` had to *already know* the exact filename and URL-encode it to build a guessed `mediafilez.forgecdn.net/files/{id/1000}/{id%1000}/{filename}` URL (documented there as "LOW confidence" — it happened to work). This phase's mod-by-mod fetch doesn't need that guess at all — the `www.curseforge.com/api/v1/mods/.../download` route does the filename resolution as part of the redirect, which is both simpler and higher-confidence than what Phase 1 needed to do.

**Example (bash, used inside `publish-pack.sh`):**
```bash
# Source: live-verified against curseforge.com/forgecdn.net, this session
fetch_cf_file() {
  local project_id="$1" file_id="$2" dest_dir="$3"
  local url="https://www.curseforge.com/api/v1/mods/${project_id}/files/${file_id}/download"
  # -w '%{url_effective}' after -L reports the FINAL resolved URL (which ends in the real filename)
  local final_url
  final_url=$(curl -sSL -o /dev/null -w '%{url_effective}' "$url" --max-time 30)
  local filename
  filename=$(basename "${final_url%%\?*}")
  curl -sSL -o "${dest_dir}/${filename}" "$url" --max-time 60
  # route by extension: .jar -> mods/, .zip -> resourcepacks/ (see Pitfall below)
}
```

### Pattern 3: Hand-rolled Server List Ping, not a crate

**What:** ~80 lines of raw TCP protocol (VarInt-prefixed packets) added to `campfire-auth`, reusing its existing `tokio` dependency. Live-verified this session with a from-scratch Python client against the actual running `rlcraft.service`:

```python
# Source: hand-verified live against 127.0.0.1:25565 on this host, this session.
# Protocol reference matches wiki.vg's documented Server List Ping for protocol 340 (MC 1.12.2).
import socket, struct, json

def varint(n: int) -> bytes:
    out = b''
    while True:
        b = n & 0x7F
        n >>= 7
        out += bytes([b | 0x80]) if n else bytes([b])
        if not n:
            break
    return out

def read_varint(sock: socket.socket) -> int:
    num, shift = 0, 0
    while True:
        byte = sock.recv(1)[0]
        num |= (byte & 0x7F) << shift
        if not (byte & 0x80):
            return num
        shift += 7

def recvall(sock: socket.socket, n: int) -> bytes:
    buf = b''
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise EOFError("short read")
        buf += chunk
    return buf

def slp(host: str, port: int, protocol: int = 340, timeout: float = 5.0) -> dict:
    s = socket.create_connection((host, port), timeout=timeout)
    addr = host.encode()
    handshake = b'\x00' + varint(protocol) + varint(len(addr)) + addr + struct.pack('>H', port) + varint(1)
    s.sendall(varint(len(handshake)) + handshake)
    s.sendall(varint(1) + b'\x00')          # Status Request: packet id 0x00, empty body
    read_varint(s)                          # total response length (unused, we read exactly str_len below)
    read_varint(s)                          # response packet id (0x00)
    str_len = read_varint(s)
    return json.loads(recvall(s, str_len))
```

**Verified live response shape (against the real RLCraft/Forge server on this Pi):**
```json
{
  "description": { "text": "campfire.pub" },
  "players": { "max": 10, "online": 0 },
  "version": { "name": "1.12.2", "protocol": 340 },
  "modinfo": { "type": "FML", "modList": [ /* 162 entries */ ] }
}
```
Total JSON body: **7,199 bytes** (not the few-hundred-byte vanilla response — Forge's `modinfo.modList` dominates the payload). No `favicon` field was present.

**Rust implementation notes for `campfire-auth`'s real `/status` handler (currently a stub returning `{"online": true, "players": null}` per `auth-service/README.md`):**
- Deserialize the JSON body loosely (`serde_json::Value`, or an untagged enum for `description`) — **do not** assume `description` is a plain string. This live server returns `{"text": "campfire.pub"}`, an object, which will fail to deserialize into a `String` field and is exactly the kind of pitfall that only shows up against a real server, not the spec.
- Discard `modinfo` entirely — CONTEXT.md's target shape is `{ online, players, max, motd }`, not the raw Forge payload; forwarding 7KB+ of mod list data to the launcher is wasted bandwidth and an unnecessary internal-detail leak.
- Use a real read loop (`recvall`-style), not a single `.read()` call — the ~7KB response will typically span more than one TCP segment.
- 10s cache (locked decision) avoids re-pinging on every launcher poll; on ping failure/timeout, return `{"online": false}` with HTTP 200 (locked decision), never propagate a 5xx for "server is off."

### Anti-Patterns to Avoid

- **Proxying `/api/validate` through Caddy:** `auth-service/README.md`'s own constraints section (Phase 2) explicitly forbids this — it's loopback-only, unauthenticated beyond the token itself, and has no rate limit by design. Only `/api/register`, `/api/login`, `/status` go through Caddy.
- **Trusting `os.walk`'s file ordering for manifest determinism:** it is not guaranteed sorted — sort the path list explicitly before hashing, or two runs with identical file contents can still produce a manifest with different `files[]` array ordering (harmless for correctness, but makes diffs/reviews noisy and could mask the `delete[]` diff logic).
- **Assuming the client manifest's `files[]` is only mod jars:** 10 of 187 entries in the live 2.9.3 client manifest resolve to `.zip` resource packs. A publish script that blindly extracts every fetched file into `pack/mods/` will put resource-pack zips in the wrong directory.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| TLS termination, HTTP routing, static file serving | A custom Rust/Python HTTP server | Caddy | Already the project's chosen front (STACK.md); `file_server`'s directory-listing-off and dotfile-`hide` behaviors are built-in, not code to write |
| CurseForge mod/resourcepack fetching | A CurseForge API client wrapper | Plain `curl`/`requests` against the verified unauthenticated redirect (Pattern 2) | No API key, no client library needed — a single HTTP GET with redirect-following does the whole job |
| SHA-256 streaming hash | A custom chunked-read hasher | Python's `hashlib` (stdlib) | Zero reason to hand-roll — this is exactly what the stdlib module is for |
| Atomic file writes | Manual lockfile/rename dance | `tempfile.mkstemp()` + `os.replace()` | Standard, already atomic on POSIX same-filesystem renames, no library needed |

**Key insight:** Every "don't hand-roll" item above is already solved by something already on this host (Caddy, Python stdlib, `curl`) — this phase adds no new dependency for any of them. The one place hand-rolling actually beats reaching for a library (SLP) is because the two available libraries are both `[SUS]`-flagged, low-adoption, and solve a problem (protocol-340 SLP) that's frozen and small enough that the "library" would be roughly the same line count as the crate's own source.

## Common Pitfalls

### Pitfall 1: `description` in the SLP JSON response is an object, not a string

**What goes wrong:** A Rust `#[derive(Deserialize)] struct StatusResponse { description: String, ... }` will fail to parse the *real* response from this exact server (`{"text": "campfire.pub"}`), even though many blog-post examples show `description` as a plain string.

**Why it happens:** Minecraft's protocol allows `description` to be either a raw string or a full chat-component object; which one a given server sends depends on how its MOTD was configured. This server (and likely any Forge 1.12.2 server using the default `motd=` rendering) sends the object form.

**How to avoid:** Deserialize `description` as `serde_json::Value` and extract `.get("text")` (falling back to treating the whole value as a string if it's a JSON string), or use an untagged enum. Verified this session against the live server — this is not a hypothetical edge case for this project, it is the actual response shape.

**Warning signs:** A `serde_json` deserialize error the first time `/status` calls the real server (as opposed to the current Phase 2 stub, which never hit this because it never actually pinged).

### Pitfall 2: Manifest `files[]` mixes mod jars and resource-pack zips — route by extension, not by assuming everything is a mod

**What goes wrong:** A publish script that unconditionally drops every CurseForge-fetched file from the manifest into `pack/mods/` will misplace the 10 resource-pack zips (e.g. `RLHats.zip`, several `AnimEmis_*.zip` emissive texture packs) that the client manifest also references by `{projectID, fileID}`.

**Why it happens:** CurseForge's modpack manifest format doesn't distinguish "mod" from "resource pack" in the `files[]` array itself — both are just `{projectID, fileID, required}` triples; the distinction only becomes visible once you actually resolve and look at the downloaded file (extension, or the project's CurseForge category, which isn't available without an API key).

**How to avoid:** Route by resolved file extension after fetching: `.jar` → `pack/mods/`, `.zip` → `pack/resourcepacks/`. Verified live this session: of 187 manifest entries, 177 resolve to `.jar`, 10 to `.zip`.

### Pitfall 3: The client zip's `overrides/` also ships one mod jar and a "memory_repo" Maven-style dir that aren't in `files[]` at all

**What goes wrong:** A publish script that only processes `manifest.json`'s `files[]` array and ignores `overrides/mods/` will miss `overrides/mods/antiquecities-1.2.1.jar`, which the RLCraft author bundled directly rather than via CurseForge project reference (confirmed: this same file already exists in the live `server/mods/` — it isn't a client-only extra, it's simply distributed this way).

**Why it happens:** Modpack authors sometimes bundle a mod jar directly in `overrides/` instead of referencing it by CurseForge project — usually because the mod isn't itself hosted as a normal CurseForge file (a fork, a manually-compiled build, etc.).

**How to avoid:** The publish script must extract **all** of `overrides/` (config, mods, scripts, resources, structures, resourcepacks — all six top-level dirs confirmed present in the live 2.9.3 client zip) in addition to resolving `files[]` — not one or the other. `overrides/server.properties`, the changelog `.txt` files, and the "FOR SERVERS ONLY" `.txt` file in `overrides/` should be excluded (server-only/irrelevant content shipped in the same zip).

### Pitfall 4: `options.txt` shipped in the client zip's `overrides/` has nowhere to go under the locked manifest schema

**What goes wrong:** The live client zip includes a default `overrides/options.txt` (4,757 bytes — RLCraft's tuned keybindings/graphics defaults) and `overrides/optionsof.txt`. CONTEXT.md's locked decision explicitly places `options.txt` in the "never managed, never in `delete[]`" list, and the locked manifest schema has no "seed once, never overwrite" flag. This means Phase 3's manifest, as specified, cannot deliver this default file to a fresh install at all.

**Why it happens:** The manifest schema was locked before this specific detail (that the base pack itself ships a default `options.txt`) was confirmed by unzipping the real file this session.

**How to avoid — not a Phase 3 blocker, flagged for the planner/Phase 4:** This is a genuine gap between what the base pack provides and what the locked manifest schema can deliver, but it does not block Phase 3 — the manifest generator should simply skip `overrides/options.txt` and `overrides/optionsof.txt` (matching the locked "never" list), and the gap (new players get vanilla defaults instead of RLCraft's tuned ones) is recorded as an Open Question below for Phase 4's launcher to solve independently (e.g., the launcher ships its own bundled default template, applied only when no local `options.txt` exists — entirely outside the manifest system).

## Runtime State Inventory

> Not applicable — this phase adds new capability (a file server, a manifest, a real `/status` handler) rather than renaming/refactoring existing state. No existing stored data, service config, OS registrations, secrets, or build artifacts reference strings that this phase changes.

**Nothing found in any category** — verified by inspecting `server.env`/`server.env.example` (no keys this phase renames), `.gitignore` (no path this phase moves), and the existing `systemd/` units (no unit this phase renames) — this phase only *adds* `caddy.service`, `ca/`, `pack/`, and new script/handler code; nothing pre-existing is touched by name.

## Code Examples

### Caddyfile skeleton (DIST-01 routes, admin off, own-CA TLS)

```caddyfile
# Source: caddyserver.com/docs/caddyfile/options, .../directives/reverse_proxy,
# .../directives/file_server, .../automatic-https — all fetched and confirmed this session
{
	admin off
	auto_https off
}

mc.campfire.pub:8444 {
	tls /home/asphacean/rlcraft/ca/mc.campfire.pub-cert.pem /home/asphacean/rlcraft/ca/mc.campfire.pub-key.pem

	encode zstd gzip

	handle /api/register {
		reverse_proxy 127.0.0.1:8081
	}
	handle /api/login {
		reverse_proxy 127.0.0.1:8081
	}
	handle /status {
		reverse_proxy 127.0.0.1:8081
	}
	# /api/validate is intentionally NOT routed here — loopback-only (locked decision)

	handle /manifest.json {
		root * /home/asphacean/rlcraft/pack
		file_server {
			hide .*
		}
	}
	handle /pack/* {
		root * /home/asphacean/rlcraft/pack
		file_server {
			hide .*
		}
	}
}
```
**Confirmed this session:** `file_server` disables directory listing by default (no config needed — `browse` must be *explicitly* added to enable it, so the locked "no directory browsing" requirement needs zero extra directive) [CITED: caddyserver.com/docs/caddyfile/directives/file_server]. `admin off` means `caddy reload` (live config reload via the admin API) will no longer work — config changes require `sudo systemctl restart caddy` instead; acceptable for a Caddyfile that changes rarely (cert renewal, route changes), and consistent with CONTEXT's stated goal of a minimal HTTPS surface with no admin API reachable at all [CITED: caddyserver.com/docs/caddyfile/options].

### Own-CA generation, verified end-to-end on this host (OpenSSL 3.5.6)

```bash
# Source: run and verified live on this exact host (Debian 13 aarch64, OpenSSL 3.5.6), this session.
# ECDSA P-256 keys — small, fast, plenty secure for a private CA serving one leaf cert.

# CA: 10-year self-signed root
openssl ecparam -name prime256v1 -genkey -noout -out ca/campfire-ca-key.pem
openssl req -x509 -new -key ca/campfire-ca-key.pem -sha256 -days 3650 \
  -subj "/CN=campfire.pub Root CA" -out ca/campfire-ca.pem \
  -addext "basicConstraints=critical,CA:true" \
  -addext "keyUsage=critical,keyCertSign,cRLSign"

# Leaf: ~2-year cert for mc.campfire.pub, signed by the CA above
openssl ecparam -name prime256v1 -genkey -noout -out ca/mc.campfire.pub-key.pem
openssl req -new -key ca/mc.campfire.pub-key.pem -subj "/CN=mc.campfire.pub" -out /tmp/leaf.csr
cat > /tmp/leaf-ext.cnf <<'EOF'
subjectAltName=DNS:mc.campfire.pub
extendedKeyUsage=serverAuth
basicConstraints=CA:false
keyUsage=digitalSignature,keyEncipherment
EOF
openssl x509 -req -in /tmp/leaf.csr -CA ca/campfire-ca.pem -CAkey ca/campfire-ca-key.pem -CAcreateserial \
  -days 730 -sha256 -extfile /tmp/leaf-ext.cnf -out ca/mc.campfire.pub-cert.pem

# Verified this session: chain checks out
openssl verify -CAfile ca/campfire-ca.pem ca/mc.campfire.pub-cert.pem
# → mc.campfire.pub-cert.pem: OK
```
`scripts/renew-cert.sh` is exactly the "Leaf:" block above, re-run periodically (before the 730-day leaf expiry) — the CA block runs once, ever.

### `/manifest.json` shape (locked schema, shown filled in)

```json
{
  "pack_version": "2026-08-28T00:00:00Z",
  "mc": "1.12.2",
  "forge": "14.23.5.2860",
  "java": 8,
  "files": [
    { "path": "mods/SpawnerControl-1.6.3b.jar", "sha256": "…", "size": 49537, "url": "pack/mods/SpawnerControl-1.6.3b.jar" },
    { "path": "resourcepacks/RLHats.zip", "sha256": "…", "size": 412317, "url": "pack/resourcepacks/RLHats.zip" }
  ],
  "delete": []
}
```

### Validation commands (DIST-01/02 verification)

```bash
# Caddyfile syntax check before restart
caddy validate --config caddy/Caddyfile

# Manifest served with a valid (CA-pinned) cert over HTTPS
curl --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/manifest.json | python3 -m json.tool

# A single pack file, hash-verified against the manifest
curl --cacert ca/campfire-ca.pem -o /tmp/f.jar https://mc.campfire.pub:8444/pack/mods/SpawnerControl-1.6.3b.jar
sha256sum /tmp/f.jar   # compare against manifest's "sha256" for that path

# Status endpoint
curl --cacert ca/campfire-ca.pem https://mc.campfire.pub:8444/status
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Phase 1's guessed-filename `mediafilez.forgecdn.net/files/{id/1000}/{id%1000}/{filename}` URL (needs the filename in advance) | `www.curseforge.com/api/v1/mods/{projectID}/files/{fileID}/download` — resolves the filename via redirect, no API key | Confirmed working, this session, 2026-08-28 | `publish-pack.sh` never needs to know a mod's filename in advance — only the `{projectID, fileID}` pairs already present in the client manifest |

**Deprecated/outdated:** None specific to this phase — RLCraft itself has been frozen since 2022 (per STACK.md), so there is no "current vs. old RLCraft version" concern; the CurseForge-fetch mechanics were the only moving target and are now verified current.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The unauthenticated CurseForge download redirect will remain available/unrate-limited for a full 187-file publish run (only 8 were sampled with full-body downloads in this session, plus 187 HEAD-only resolutions — no full 187-file bulk-download run was performed) | Architecture Patterns, Pattern 2 | If CurseForge/Cloudflare rate-limits bulk sequential fetches, `publish-pack.sh` needs a retry/backoff loop; low risk given Phase 1 already relies on the same CDN family successfully, but the *volume* here (187 files) is untested |
| A2 | `reqwest`'s exact API for custom-CA pinning (`Certificate::from_pem` + `.add_root_certificate()`, `.tls_built_in_root_certs(false)`) — not verified this session, this is a Phase 4 integration detail only referenced here for forward context | Additional context (not a RESEARCH.md section directly — informational only) | Low — Phase 4's own research will verify this when that phase is planned; not load-bearing for Phase 3's deliverables |
| A3 | Whether CurseForge's per-mod redistribution terms are actually violated by self-hosting — operator has already explicitly accepted this risk (CONTEXT.md, locked) so this is not a decision Phase 3 needs to make, only implement | Client pack & manifest (User Constraints) | None — this is a locked, already-accepted operator decision, not an open research question |

**If this table is empty:** N/A — see A1–A3 above; none of these block planning, they are forward-looking notes for the executor/Phase 4.

## Open Questions

1. **Should `options.txt`/`optionsof.txt` defaults ever reach a fresh client, given the locked manifest schema excludes them entirely?**
   - What we know: The base RLCraft 2.9.3 client zip ships tuned defaults for both files; CONTEXT.md's locked decision excludes `options.txt` from all manifest management.
   - What's unclear: Whether "new players get vanilla Minecraft option defaults instead of RLCraft's" is an acceptable UX gap, or whether Phase 4's launcher should independently seed a bundled default template on first install (outside the manifest system).
   - Recommendation: Not a Phase 3 blocker — record as a Phase 4 planning input. Phase 3 simply omits both files from the manifest per the already-locked "never manage options.txt" rule.

2. **Exact rate/volume behavior of CurseForge's unauthenticated download redirect at the full 187-file scale.**
   - What we know: 8 sequential fetches with a 0.3s gap all succeeded; a HEAD-only pass over all 187 succeeded.
   - What's unclear: Whether a full bulk run (187 GETs, some multi-hundred-KB) triggers any Cloudflare bot-mitigation the smaller samples didn't hit.
   - Recommendation: `publish-pack.sh` should log and skip-with-warning (not hard-fail) any single file that returns non-200, matching Phase 1's `fetch-pack.sh` philosophy of a hard integrity gate per file rather than an all-or-nothing bulk operation.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | Indirectly | Not this phase's concern directly — `/api/register`/`/api/login` are proxied unchanged from Phase 2's already-hardened `campfire-auth` |
| V3 Session Management | Indirectly | Same — Caddy is a transparent proxy for these routes, adds no session logic of its own |
| V4 Access Control | Yes | `/api/validate` must never be routed through Caddy (locked decision) — the Caddyfile must have no `handle`/route block matching that path at all, not merely "not documented" |
| V5 Input Validation | Yes | `manifest.json`'s `files[]` paths must be validated to stay inside `pack/` when the manifest generator writes `url` fields — a path-traversal-shaped `path` value (e.g. containing `../`) must never be trusted verbatim from a CurseForge-sourced filename |
| V6 Cryptography | Yes | TLS handled entirely by Caddy + the own-CA leaf cert (ECDSA P-256, SHA-256 signature) — never hand-roll TLS; cert/key generation uses `openssl`, a standard, audited tool, not custom crypto code |
| V9 Communications | Yes | HTTPS only for the manifest/pack/api surface (locked); the game protocol itself (TCP 25565) stays unencrypted by design, matching ARCHITECTURE.md's already-documented anti-pattern ("don't proxy Minecraft's port through Caddy") |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Directory listing / dotfile exposure via `file_server` | Information Disclosure | Confirmed this session: browse is off by default (no directive needed); `hide .*` explicitly added for dotfiles — both should be asserted in a verification step, not assumed |
| `/api/validate` accidentally exposed via a future Caddyfile edit | Elevation of Privilege | No route/handle block for `/api/validate` in the Caddyfile at all — a missing route, not a denied one, is the correct posture (denying it via a 404 handler is defense-in-depth but the primary control is simply never writing the route) |
| Path traversal in manifest `path`/`url` fields | Tampering | Manifest generator must reject/sanitize any resolved filename containing `..`, absolute paths, or characters outside a safe filename set before writing it into `manifest.json` — a maliciously-named CurseForge file (unlikely but not impossible) should not be able to write outside `pack/` |
| Non-GET/HEAD methods against `/manifest.json` or `/pack/*` | Tampering | Caddy's `file_server` does not implement PUT/DELETE — unhandled methods fall through to Caddy's default (404/405); no additional Caddyfile directive is required beyond not adding a handler for other methods, but this should be asserted in verification (`curl -X PUT` should not succeed) |
| Server List Ping response used for anything beyond `{online, players, max, motd}` | Information Disclosure | The real response includes a 162-entry Forge mod list (7.2KB) — `/status`'s handler must discard `modinfo` before returning to the launcher, not proxy the raw SLP JSON |

## Sources

### Primary (HIGH confidence — live-verified this session against the authoritative source itself)
- `curl` against `www.curseforge.com/api/v1/mods/{projectID}/files/{fileID}/download` and `mediafilez.forgecdn.net` — client zip download, 8 sampled mod downloads, 187 HEAD-only filename resolutions, all this session
- Hand-written Python Server List Ping client, run against the live `rlcraft.service` on this Pi (`127.0.0.1:25565`) — full protocol handshake, response shape, size, and the `description`-is-an-object finding
- `openssl` CA + leaf cert generation and chain verification, run end-to-end on this host (OpenSSL 3.5.6)
- `docker ps`, `ss -tlnp`, `sudo iptables -L`, `apt-cache policy caddy`, `getent passwd caddy` — this host's actual port/service state (pbwiki's `caddy:2.11-alpine`, sing-box on 443, docker-proxy on 80/8443, nothing on 8444/2019, no `caddy` user yet)
- `curl https://api.github.com/repos/caddyserver/caddy/releases/latest` — current Caddy version
- `curl -A "<UA>" https://crates.io/api/v1/crates/{craftping,mc-server-status,mcping}` — crate download counts/repo URLs
- `node gsd-tools.cjs query package-legitimacy check --ecosystem crates craftping mc-server-status` — SUS verdicts on both SLP crates

### Secondary (MEDIUM confidence — official docs, fetched and cross-checked this session)
- [caddyserver.com/docs/install](https://caddyserver.com/docs/install#debian-ubuntu-raspbian) — official apt repo setup commands
- [caddyserver.com/docs/caddyfile/directives/reverse_proxy](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy) — `handle_path` vs `handle`, path-scoped `reverse_proxy`
- [caddyserver.com/docs/caddyfile/directives/file_server](https://caddyserver.com/docs/caddyfile/directives/file_server) — browse-off-by-default, `hide` directive, `root`
- [caddyserver.com/docs/caddyfile/options](https://caddyserver.com/docs/caddyfile/options) — `admin off`, admin address binding, global options block syntax
- [caddyserver.com/docs/automatic-https](https://caddyserver.com/docs/automatic-https#local-https) — `tls internal` local CA storage path and stability (rejected alternative, documented for comparison)

### Tertiary (LOW confidence — WebSearch only, used for cross-reference/file-ID discovery, then independently confirmed via direct download)
- WebSearch results identifying CurseForge file ID 4612979 for the RLCraft 2.9.3 client zip — independently confirmed by successfully downloading and hashing the file this session, so the *outcome* is HIGH confidence even though the initial lead was a plain search result

## Metadata

**Confidence breakdown:**
- Standard stack (Caddy, OpenSSL, Python): HIGH — versions and behavior confirmed live on this exact host
- CurseForge fetch mechanics: HIGH — live-tested against the real service for the exact files this phase needs
- Server List Ping: HIGH — hand-verified against the actual running server, including a real protocol gotcha caught only by testing
- Manifest generation patterns (hashing, atomic write, diffing): MEDIUM-HIGH — standard, well-known Python idioms, not project-specific, low risk
- Security domain / ASVS mapping: MEDIUM — reasoned from locked decisions and Caddy's documented defaults, not a formal threat-model exercise

**Research date:** 2026-08-28
**Valid until:** ~60 days for the Caddy/CurseForge-mechanics findings (stable, slow-moving); RLCraft itself is frozen (no re-verification needed for mod-list contents unless the pack is re-released)
