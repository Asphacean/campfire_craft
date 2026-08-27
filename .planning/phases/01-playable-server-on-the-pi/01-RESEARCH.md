# Phase 1: Playable Server on the Pi - Research

**Researched:** 2026-08-27
**Domain:** Forge 1.12.2 dedicated server hosting on Raspberry Pi 5 (aarch64/Debian 13) — Java provisioning, systemd process supervision, RCON-driven ops, backups, home-network exposure (DDNS/CGNAT)
**Confidence:** MEDIUM-HIGH — server/Java/systemd patterns well-corroborated across multiple sources; two items (CurseForge scripted-download policy, exact G1GC tuning for a 6G/4-core ARM box) are MEDIUM/LOW and flagged below for a checkpoint or spike during execution.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Server Runtime**
- Server lives in `~/rlcraft/server/` (inside the project repo; world/, logs/, backups gitignored)
- Java 8 = Eclipse Temurin 8 JDK arm64 installed via the official Adoptium apt repository (Debian 13 ships no openjdk-8). System Java 25 stays untouched
- `install.sh` downloads the official RLCraft Server Pack 2.9.3 zip from CurseForge CDN by file ID, verifies checksum, unpacks, runs the Forge 1.12.2-14.23.5.2860 installer `--installServer`. Reproducible, no manual downloads
- Runs as systemd system unit `rlcraft.service` under user `asphacean`, `Restart=on-failure`, `RestartSec=15`, `ExecStop` = RCON `stop` (mcrcon) with `TimeoutStopSec=90` so the world saves cleanly
- JVM: `-Xms6G -Xmx6G` + Aikar flags for Java 8 (G1GC). Leaves ~9 GB for other Pi services. Bump to 8G only if load test demands

**Network**
- Server addressed by a domain/subdomain the operator provides (placeholder `mc.example.com` in configs until given). A-record → home public IP
- First task: CGNAT check (router WAN IP vs `curl ifconfig.me`). If IP is dynamic → ddclient / registrar-API update script; if static → nothing
- Only TCP 25565 forwarded to the Pi. RCON bound to 127.0.0.1 only
- Interim access control until Phase 2: `white-list=true`, `enforce-whitelist=true`, friends' nicks added by operator. Phase 2 replaces this with token auth

**Ops**
- Backups: systemd timer every 6 h → RCON `save-off` + `save-all` → `tar --zstd` of world dirs → `save-on`; keep 14 archives in `~/rlcraft-backups/`; `restore.sh` script. Restore tested once (SRV-03)
- Scheduled restart daily 05:00 via RCON `stop` (systemd restarts it) — Forge 1.12.2 leaks memory over long uptimes
- `server.properties`: RLCraft pack defaults except `online-mode=false`, `max-players=10`, `view-distance=8`, `difficulty=hard`, `enable-rcon=true` (localhost, random password in file)
- Load test (SRV-05): 3 real friends online for 20 min, `/forge tps` recorded in SUMMARY. If <15 TPS: try heap 8G, view-distance 6, then RLCraft-safe tweaks

### Claude's Discretion
- Exact Aikar flag set, mcrcon vs alternative RCON client, backup script language (bash preferred), whether old `~/mcserver` (Paper 1.21.6) is stopped to free RAM — recommend stopping if it is running

### Deferred Ideas (OUT OF SCOPE)
- Offsite backup copy (rclone to cloud) — nice-to-have, not in v1
- Discord webhook on server crash/restart — later
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SRV-01 | RLCraft Server Pack 2.9.3 (Forge 1.12.2-14.23.5.2860) runs on the Pi 5 under Java 8 (Temurin aarch64) in offline-mode | Adoptium apt repo confirmed for `trixie`; Forge installer URL confirmed; CurseForge file ID confirmed; **install.sh download step flagged as needing a CurseForge API key + distribution-toggle check (see Pitfall 1)** |
| SRV-02 | Server starts on boot and restarts on crash (`Restart=on-failure`) | systemd unit pattern with RCON `ExecStop` documented, `KillMode`/`TimeoutStopSec` gotchas covered |
| SRV-03 | World backed up on schedule with rotation; restore tested once | `save-off`/`save-all`/`save-on` sequence, world directory layout (incl. `DIM-1`/`DIM1`), `tar --zstd` availability confirmed on Debian 13 |
| SRV-04 | Reachable from internet on TCP 25565 via domain (port forward + DDNS/static; CGNAT verified absent) | CGNAT detection method (WAN IP vs public IP, plus fast CGNAT-range check), ddclient/cloudflare-ddns package availability on `trixie` |
| SRV-05 | Holds ≥15 TPS with 3 players after tuning (heap ≤8 GB) | Aikar's flags baseline, `/forge tps` output format for measurement, project-level Pitfall 1 (CPU-bound on ARM) reinforced |
</phase_requirements>

## Summary

This phase is almost entirely "known playbook, ARM-specific gotchas" rather than novel design work. The RLCraft Server Pack, Forge 1.12.2, Temurin 8, systemd+RCON, and `save-off`/`save-all` backup patterns are all extremely well-trodden for x86 Minecraft hosting — the only genuinely new territory is (a) confirming each of those tools actually ships an aarch64 build/repo suite for the Pi 5, and (b) the fact that CurseForge now gates *all* scripted/API downloads behind a free-but-required API key (since July 2024) and a per-project distribution toggle the pack author controls — this directly threatens the "reproducible `install.sh`, no manual downloads" locked decision and needs a checkpoint before being trusted blindly.

The two decisions flagged `Claude's Discretion` in CONTEXT.md are resolved by this research: use **Aikar's flags** (standard G1GC baseline, values below) sized for a 6 GB heap, and use **itzg/rcon-cli** (Go, single static binary, ships official `linux_arm64` releases) instead of `mcrcon` — `mcrcon`'s upstream GitHub releases only publish x86 Linux/Windows binaries, so on the Pi's aarch64 it would require a build toolchain (gcc/make) for zero benefit over a drop-in static binary.

**Primary recommendation:** Build `install.sh` around the confirmed URLs (Adoptium `trixie` apt repo, Forge Maven installer jar, CurseForge file 4612990) but gate the CurseForge download step behind a `checkpoint:human-verify` for API-key acquisition and a live test of the distribution toggle — everything else in this phase can be scripted with high confidence.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Game server process (Forge/JVM) | OS Process (systemd) | — | Bare-metal systemd unit owns lifecycle; no container/orchestration tier in this phase (Docker explicitly rejected) |
| Interim access control (whitelist) | Game server config | — | `whitelist.json` + `server.properties` flags are server-tier state; no separate auth service exists until Phase 2 |
| Network reachability (port forward, DDNS) | Network / Router+DNS | OS (ddclient/cloudflare-ddns daemon) | Router does the actual forwarding; a Pi-resident daemon keeps the DNS record in sync — both tiers required, neither alone is sufficient |
| Backup & restore | Storage (filesystem + RCON control channel) | OS Process (systemd timer) | RCON commands pause world writes (a server-process concern); the timer and `tar` step are OS/cron-tier |
| TPS / load verification | Game server (in-process `/forge tps`) | Ops (manual load test) | No external monitoring tier in this phase — verification is a manual, in-game command run during a real 3-player session |

## Standard Stack

### Core
| Component | Version | Purpose | Why Standard |
|-----------|---------|---------|---------------|
| Eclipse Temurin JDK 8 (aarch64) | latest 8u (Adoptium `trixie` apt repo) | Java runtime for Forge 1.12.2 | Only JVM Forge 1.12.2 runs on (Java 9+ breaks its classloader hack — project PITFALLS.md Pitfall 2). Adoptium's apt repo **does** publish a `trixie` suite `[CITED: packages.adoptium.net/artifactory/deb/dists/]` — no need to fall back to `bookworm` or a tarball. |
| Forge 1.12.2-14.23.5.2860 installer | 2860 | Materializes the server jar + libraries via `--installServer` | Exact build RLCraft pins to (already a locked decision). Installer jar confirmed reachable at `https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860-installer.jar` `[CITED: maven.minecraftforge.net]` |
| RLCraft Server Pack | 1.12.2 - Release v2.9.3 | The actual world/mod content | CurseForge project ID **285109**, file ID **4612990**, filename `RLCraft Server Pack 1.12.2 - Release v2.9.3.zip`, 318.9 MB, uploaded 2023-06-27 `[CITED: curseforge.com/minecraft/modpacks/rlcraft/files/4612990]` |
| systemd (system unit, not user unit) | Debian 13 default | Autostart + crash-restart (SRV-02) | Matches locked decision; see Architecture Patterns for the exact unit shape |
| itzg/rcon-cli | latest (check GitHub releases at execution time) | RCON client for `ExecStop`, backup `save-off`/`save-all`/`save-on`, ops scripts | Go, single static binary, **ships official `linux_arm64` release assets** `[CITED: github.com/itzg/rcon-cli release-asset naming via .goreleaser.yml]` — no build step needed on the Pi, unlike `mcrcon` (see Don't Hand-Roll / Pitfall 5) |
| zstd (Debian package `zstd`) | Debian 13 stable | `tar --zstd` compression for backups | `zstd` CLI package present in `trixie` `[CITED: packages.debian.org/trixie]`; GNU tar (any version ≥1.31, which `trixie`'s ships well past) supports `--zstd` natively once the `zstd` binary is on `PATH` — no separate tar plugin needed |
| ddclient **or** `cloudflare-ddns` (Debian package) | Debian 13 stable | Keep DNS A-record current if the WAN IP is dynamic | Both packages exist in `trixie` `[CITED: packages.debian.org/trixie/arm64/cloudflare-ddns, packages.debian.org/stable/ddclient]`. `cloudflare-ddns` is a purpose-built oneshot + systemd-timer tool specifically for Cloudflare-hosted zones (needs only an API token + zone ID); `ddclient` is the generic multi-provider client (supports Cloudflare API v4 since ddclient 3.8.3). **Recommendation:** if the operator's domain is on Cloudflare, use `cloudflare-ddns` — smaller surface, systemd-native, no Perl config wizard. |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `curl` | Debian stable | CGNAT check (`curl ifconfig.me`), CurseForge API calls | Already present on virtually every Debian install |
| `sha256sum` | coreutils | Verify RLCraft server-pack zip checksum before unpacking | Locked decision requires checksum verification before install |
| `git`, `build-essential` | Debian stable | **Only** needed if `rcon-cli`'s prebuilt `linux_arm64` binary is unavailable for some reason and `mcrcon` must be built from source as a fallback | See Pitfall 5 — not the primary path |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| itzg/rcon-cli | mcrcon (locked-decision default) | `mcrcon`'s GitHub releases (`v0.7.2`) ship **only** `linux-x86-32`/`linux-x86-64`/Windows binaries — no `arm64` asset `[CITED: github.com/Tiiffi/mcrcon/releases]`. Would require `git clone` + `make` on the Pi (trivial, but an unforced build dependency vs. a static binary download) |
| `cloudflare-ddns` | `ddclient` | `ddclient`'s config is an older Perl-wizard style and supports many providers generically; `cloudflare-ddns` is narrower but simpler if Cloudflare is confirmed as the DNS host |
| Direct CurseForge CDN URL scrape | Official CurseForge REST API (`api.curseforge.com`) with `CF_API_KEY` | Since 2024-07-16 the **official API requires a key** for every download `[CITED: blog.curseforge.com/introducing-api-key-authentication-for-curseforge-file-downloads]`. The historical trick of guessing the CDN path from the file ID (`https://mediafilez.forgecdn.net/files/{first4digits}/{rest}/{filename}`) `[CITED: github.com/PrismLauncher/PrismLauncher/issues/394]` may or may not still resolve without hitting the gated API path — **do not build `install.sh` around the guessed-URL trick without testing it live first** (see Pitfall 1) |

**Installation:**
```bash
# Java 8 (Temurin, aarch64) — Adoptium apt repo, trixie suite confirmed available
sudo mkdir -p /etc/apt/keyrings
wget -O - https://packages.adoptium.net/artifactory/api/gpg/key/public | sudo tee /etc/apt/keyrings/adoptium.asc
echo "deb [signed-by=/etc/apt/keyrings/adoptium.asc] https://packages.adoptium.net/artifactory/deb $(awk -F= '/^VERSION_CODENAME/{print$2}' /etc/os-release) main" | \
  sudo tee /etc/apt/sources.list.d/adoptium.list
sudo apt update
apt-cache search temurin-8   # VERIFY exact package name exists before `apt install` (see Assumptions Log A1)
sudo apt install temurin-8-jdk

# zstd for backups
sudo apt install zstd

# DDNS (pick one based on operator's DNS provider)
sudo apt install cloudflare-ddns   # if domain is on Cloudflare
# sudo apt install ddclient        # generic alternative

# RCON client (no build step, aarch64 static binary)
# Fetch the linux_arm64 asset from https://github.com/itzg/rcon-cli/releases — verify the exact
# current release filename at execution time (see Assumptions Log A5)
```

## Package Legitimacy Audit

> This phase installs OS-level apt packages and directly-downloaded official jars/zips (Java, Forge, RLCraft pack, `rcon-cli`) — **not** npm/pypi/crates ecosystem packages, so the automated `package-legitimacy check` seam (built for language package registries) does not apply. Legitimacy was instead verified by fetching each item's official source directly.

| Item | Source | Verified How | Verdict | Disposition |
|------|--------|---------------|---------|-------------|
| `temurin-8-jdk` (apt) | Adoptium official apt repo (`packages.adoptium.net`) | `[CITED]` fetched `dists/` listing directly, `trixie` present | OK | Approved |
| Forge 1.12.2-14.23.5.2860 installer | `maven.minecraftforge.net` (official Forge Maven) | `[CITED]` fetched URL directly, confirmed reachable | OK | Approved |
| RLCraft Server Pack 2.9.3 | CurseForge official project page (`curseforge.com/.../rlcraft`) | `[CITED]` fetched file page directly (project ID 285109, file ID 4612990) | OK — but see Pitfall 1 for the *download mechanism* risk, not the file's legitimacy | Approved, with checkpoint on the download step |
| `itzg/rcon-cli` | GitHub release assets, goreleaser-built | `[CITED]` release/build config confirmed `linux_arm64` target exists | OK | Approved |
| `mcrcon` (fallback only) | GitHub `Tiiffi/mcrcon` releases | `[CITED]` fetched releases page, no arm64 asset found | OK (source is legitimate; just no prebuilt arm64 binary) | Fallback only — build from source if `rcon-cli` is rejected |
| `cloudflare-ddns` / `ddclient` (apt) | Debian official archive (`packages.debian.org`) | `[CITED]` fetched package pages directly, both present in `trixie` | OK | Approved |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none — all items resolve to first-party vendor sources (Adoptium, Minecraft Forge, CurseForge official project, Debian archive, or the tool author's own GitHub releases).

## Architecture Patterns

### System Architecture Diagram

```
Friend's PC (RLCraft 2.9.3 client, hand-installed)
        │  TCP 25565 (Minecraft protocol)
        ▼
Home router  ──(port-forward 25565→Pi)──►  Raspberry Pi 5
        ▲                                        │
        │ WAN IP                                 │
   ISP / Internet                                 │
        ▲                                         │
        │ A-record lookup                         │
  mc.example.com (DNS) ◄── cloudflare-ddns/ddclient (systemd timer, runs on Pi,
        │                   pushes current WAN IP to DNS provider on change)
        │
   Friend's DNS resolver
                                                    │
                                       ┌────────────▼─────────────┐
                                       │  rlcraft.service (systemd)│
                                       │  Temurin 8 JVM (Forge)    │
                                       │  -Xms6G -Xmx6G + Aikar    │
                                       │  RCON: 127.0.0.1:25575    │
                                       └────────────┬─────────────┘
                                                     │ rcon-cli (localhost only)
                              ┌──────────────────────┼───────────────────────┐
                              ▼                       ▼                       ▼
                  systemd timer: backup      systemd timer: daily      operator: manual
                  (every 6h)                 05:00 restart             RCON ops (whitelist add)
                  save-off → save-all →
                  tar --zstd world/{...} →
                  save-on → rotate to 14
                  ~/rlcraft-backups/
```

### Recommended Project Structure
```
~/rlcraft/
├── server/                    # gitignored: world/, logs/, libraries/, mods/ (RLCraft pack contents)
│   ├── server.properties
│   ├── whitelist.json
│   ├── user_jvm_args.txt      # generated by Forge installer
│   ├── run.sh / forge-*.jar   # generated by Forge installer --installServer
│   └── mods/                  # operator-controlled; Phase 2's auth-gate mod will land here later
├── scripts/
│   ├── install.sh             # downloads Java repo config, Forge installer, RLCraft pack; verifies checksum
│   ├── backup.sh               # save-off/save-all → tar --zstd → save-on → rotate
│   ├── restore.sh
│   └── cgnat-check.sh
├── systemd/
│   ├── rlcraft.service
│   ├── rlcraft-backup.service
│   ├── rlcraft-backup.timer
│   ├── rlcraft-restart.service
│   └── rlcraft-restart.timer
├── server.env                  # domain, RCON password, backup retention count — single source read by all scripts
└── ~/rlcraft-backups/          # outside repo; 14 rotated tar.zst archives
```

### Pattern 1: systemd unit with RCON-based graceful stop
**What:** `ExecStop` runs an RCON `stop` command (not `SIGTERM`/`SIGKILL`) so Forge flushes chunks before the process exits; `TimeoutStopSec` gives it a wide window before systemd escalates to `SIGKILL`.
**When to use:** Any Minecraft server under systemd — SIGTERM alone is not guaranteed to trigger a clean world save in older Forge builds.
**Example:**
```ini
# Source: pattern corroborated across multiple community systemd+RCON writeups
# [CITED: teilgedanken.de/Blog/post/setting-up-a-minecraft-server-using-systemd/,
#         forum.level1techs.com/t/automating-and-protecting-minecraft-server-w-systemd-and-selinux]
[Unit]
Description=RLCraft dedicated server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=asphacean
WorkingDirectory=/home/asphacean/rlcraft/server
EnvironmentFile=/home/asphacean/rlcraft/server.env
ExecStart=/usr/lib/jvm/temurin-8-jdk-arm64/bin/java @user_jvm_args.txt -jar forge-1.12.2-14.23.5.2860.jar nogui
ExecStop=/usr/local/bin/rcon-cli --host 127.0.0.1 --port 25575 --password ${RCON_PASSWORD} stop
KillMode=none
TimeoutStopSec=90
Restart=on-failure
RestartSec=15

[Install]
WantedBy=multi-user.target
```
Note `KillMode=none` — without it, systemd's cgroup kill can race the graceful RCON stop and kill the JVM mid-save; this is a documented pitfall in the community writeups above, not just theoretical.

### Pattern 2: Backup sequence around live world writes
**What:** Pause world saving via RCON before copying files, resume after.
**When to use:** Every scheduled backup — never `tar`/`cp` a live-writing world directory directly (project PITFALLS.md Pitfall 11 already flags this).
**Example:**
```bash
# Source: standard Minecraft ops pattern, corroborated via Minecraft Wiki level-format docs
# [CITED: minecraft.wiki/w/Java_Edition_level_format for DIM-1/DIM1 directory semantics]
rcon-cli --host 127.0.0.1 --port 25575 --password "$RCON_PASSWORD" save-off
rcon-cli --host 127.0.0.1 --port 25575 --password "$RCON_PASSWORD" save-all
sleep 5   # let the save-all flush actually land on disk before copying
tar --zstd -cf "$BACKUP_DIR/world-$(date +%Y%m%d-%H%M%S).tar.zst" \
  -C /home/asphacean/rlcraft/server world world_nether world_the_end 2>/dev/null \
  -C /home/asphacean/rlcraft/server world   # RLCraft 1.12.2 keeps dimensions as world/DIM-1 (Nether) and
                                              # world/DIM1 (End) subfolders, NOT separate world_nether/ dirs —
                                              # a single `-C ... world` tar covers level.dat, region/, data/,
                                              # entities/, poi/, playerdata/, stats/, advancements/, DIM-1/, DIM1/
rcon-cli --host 127.0.0.1 --port 25575 --password "$RCON_PASSWORD" save-on
```
**Correction embedded above:** unlike Bukkit/Paper servers (which sometimes split `world_nether`/`world_the_end` as top-level folders depending on `level-type`/plugin config), vanilla/Forge 1.12.2 keeps all dimensions nested under the single `world/` directory as `world/DIM-1` (Nether) and `world/DIM1` (End) `[CITED: minecraft.wiki/w/Java_Edition_level_format]`. **Back up `world/` as one tree** — do not write a script that looks for separate `world_nether`/`world_the_end` top-level folders, they will not exist for this server.

### Anti-Patterns to Avoid
- **Backing up with a raw `tar`/`cp` while the server is running:** produces silently-corrupted region files (project PITFALLS.md Pitfall 11) — always `save-off`/`save-all` first.
- **Relying on `SIGTERM` (bare `systemctl stop` without RCON) for shutdown:** risks losing the last few minutes of world state; use the RCON `ExecStop` pattern above.
- **Hardcoding the WAN IP anywhere (launcher instructions, configs):** project PITFALLS.md Pitfall 10 — always resolve through the DDNS-managed domain, never a raw IP.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|--------------|-----|
| RCON protocol client | A custom Python/bash RCON socket implementation | `itzg/rcon-cli` (static Go binary, `linux_arm64` release) | The Minecraft RCON protocol has known quirks (packet fragmentation on large responses, auth packet ID matching) that existing clients already handle correctly |
| GC tuning | Ad-hoc `-Xmx`-only JVM flags | Aikar's flags (below) | Widely validated G1GC baseline for modded 1.12.2 servers; reinventing GC tuning from scratch risks the exact "oversized heap → longer pauses" trap already documented in project PITFALLS.md's Performance Traps table |
| DDNS update client | A custom cron script that curls a DNS provider's API | `cloudflare-ddns` or `ddclient` (both packaged in Debian `trixie`) | Handles IP-change detection, retry/backoff, and multiple provider APIs; a hand-rolled script would need to reinvent all of that for no benefit |
| World backup consistency | A raw `cp -r`/`zip` cron job | RCON `save-off`/`save-all`/`save-on` wrapper | The only way to guarantee a non-corrupted backup of a live world without a full server stop |

**Key insight:** Every piece of this phase's infrastructure (RCON client, GC flags, DDNS client, backup consistency pattern) has a mature, boring, already-solved answer. The only place custom code is genuinely needed is the small glue scripts (`install.sh`, `backup.sh`, `restore.sh`) that sequence these existing tools — keep those scripts thin.

## Common Pitfalls

### Pitfall 1: CurseForge now gates scripted downloads behind an API key — and a per-project toggle
**What goes wrong:** `install.sh`'s locked-decision plan is "download the RLCraft Server Pack zip from CurseForge CDN by file ID." As of **2024-07-16**, CurseForge's official API (`api.curseforge.com`) requires a valid `CF_API_KEY` for every download request `[CITED: blog.curseforge.com/introducing-api-key-authentication-for-curseforge-file-downloads]`. Separately, each modpack author can toggle "allow third-party distribution" off per-project `[CITED: support.curseforge.com/support/solutions/articles/9000207877-project-distribution-toggle]` — if RLCraft's author has this off, **no** API key makes the automated path work. Neither of these was confirmed live against the actual RLCraft project during this research session (would require an API key to test).
**Why it happens:** The old trick of guessing a direct CDN URL from the file ID (`mediafilez.forgecdn.net/files/{first4}/{rest}/{filename}`) predates the 2024 API-key requirement and predates increased enforcement against scraping; it may or may not still resolve for a fresh, un-authenticated request.
**How to avoid:**
- Before writing `install.sh`'s download step, do a **live spike**: (1) apply for a free CurseForge API key (self-service form, reviewed by Overwolf), (2) call `GET https://api.curseforge.com/v1/mods/285109/files/4612990/download-url` with the key, confirm it returns a working URL.
- If the API returns a 403/"distribution disabled" for this project, fall back to a manual one-time download (operator downloads via browser/CurseForge app, `scp`s the zip to the Pi) and have `install.sh` accept a pre-staged zip path as an alternative to fetching it itself.
- Either way, **checksum-verify** the zip against a hash captured once from a known-good download (locked decision already requires this) — this catches both corrupted downloads and any future file replacement by the author.
**Warning signs:** `install.sh`'s `curl`/`wget` step returns HTML (a login/error page) instead of a zip; `unzip` fails with "not a zip file."
**Phase to address:** Phase 1, first task — this blocks SRV-01 entirely if unresolved, so it should be verified before any other work in this phase, exactly as CONTEXT.md's "Specific Ideas" section already flags CGNAT as a first-task check. Add a `checkpoint:human-verify` here too.

### Pitfall 2: `mcrcon` has no prebuilt ARM64 Linux binary
**What goes wrong:** CONTEXT.md's locked decision names `mcrcon` for `ExecStop`. Its GitHub releases (`v0.7.2`) ship only `linux-x86-32`, `linux-x86-64`, and Windows assets — no `arm64`/`aarch64` binary `[CITED: github.com/Tiiffi/mcrcon/releases]`. On the Pi 5 this means either building from source (`git clone` + `make`, needs `build-essential`) or switching tools.
**Why it happens:** `mcrcon` is a small, mature C project whose release automation predates widespread ARM server adoption; it was never updated to cross-compile arm64 release assets.
**How to avoid:** Use `itzg/rcon-cli` instead (see Standard Stack) — it's goreleaser-built and does publish `linux_arm64` release assets, avoiding the build step entirely. This is explicitly the "Claude's Discretion" call CONTEXT.md left open.
**Warning signs:** `apt install mcrcon` fails (it's not a Debian package either — `[CITED: packages.debian.org search, zero results]`); a downloaded release binary reports "cannot execute binary file: Exec format error" on the Pi.
**Phase to address:** Phase 1, install/bootstrap task.

### Pitfall 3: Adoptium's apt package name for Java 8 must be verified, not assumed
**What goes wrong:** The Adoptium apt repo confirms a `trixie` suite exists, and the general Temurin package naming pattern is `temurin-<major>-jdk` (confirmed for other majors like `temurin-21-jdk`) — but this session did **not** get a direct listing of `trixie`'s package index confirming `temurin-8-jdk` specifically is published there (Java 8 is Adoptium's oldest actively maintained LTS line and occasionally lags newer-suite rollout for older majors).
**How to avoid:** `install.sh` should run `apt-cache search temurin-8` (or `apt list -a 'temurin-8-*'`) right after `apt update` and **fail loudly with a clear message** if the package isn't found, rather than silently trying to `apt install` a nonexistent package name. If it's genuinely missing from `trixie`, fall back to the Adoptium tarball (`api.adoptium.net/v3/binary/latest/8/ga/linux/aarch64/jdk/hotspot/normal/eclipse`) — same fallback already scoped in the phase's original task list.
**Warning signs:** `apt install temurin-8-jdk` returns "Unable to locate package."
**Phase to address:** Phase 1, first bootstrap task (adjacent to the CGNAT check).

### Pitfall 4: RLCraft's dimensions live *inside* `world/`, not as sibling folders
**What goes wrong:** A backup/restore script written from generic "Bukkit-server" habits (`world/`, `world_nether/`, `world_the_end/` as three top-level dirs) will silently back up an incomplete world on this Forge 1.12.2 server — `world_nether`/`world_the_end` don't exist here; the Nether and End are `world/DIM-1` and `world/DIM1` respectively.
**How to avoid:** `backup.sh`/`restore.sh` should archive the single `world/` directory tree wholesale — see Architecture Patterns Pattern 2 above for the corrected sequence.
**Warning signs:** A "successful" backup that's suspiciously small, or a restore that has an intact Overworld but players report the Nether portal leads to a fresh/empty dimension.
**Phase to address:** Phase 1, backup/restore task — this is exactly the kind of thing that only surfaces during the mandated test-restore (SRV-03), so make sure the test-restore actually visits the Nether/End, not just the Overworld spawn.

### Pitfall 5: RLCraft on ARM is CPU-bound, not RAM-bound (project-level, reinforced here)
Already documented in `.planning/research/PITFALLS.md` Pitfall 1 — repeated here because SRV-05 is this phase's hardest requirement. Key phase-specific action: budget real wall-clock time for a genuine 3-player, 20-minute session (not a solo smoke test) before considering SRV-05 done, and have the heap-8G / view-distance-6 fallback ladder from CONTEXT.md ready to execute immediately if the first test comes in under 15 TPS — don't treat the first load test as pass/fail with no remediation plan queued.

### Pitfall 6: Home-network exposure assumptions (project-level, reinforced here)
Already documented in `.planning/research/PITFALLS.md` Pitfall 10. Phase-specific addition: the **fast** CGNAT check (WAN IP in `100.64.0.0/10`) can be done without even logging into the router, purely from a machine on the LAN checking its own default-gateway-assigned IP range, and is worth running *before* the slower manual "log into router, compare to `curl ifconfig.me`" check `[CITED: docs.beammp.com/FAQ/How-to-check-for-CGNAT, oneuptime.com/blog/post/2026-03-20-detect-cgnat]`.

## Code Examples

### Aikar's flags, adapted for the locked 6 GB heap
```bash
# Source: standard Aikar's-flags baseline, corroborated across multiple hosting-provider docs
# [CITED: docs.berrybyte.net/games/minecraft/aikars-flags, winternode.com/help/games/minecraft-java/configuration/aikars-flags]
# Confidence: MEDIUM — this is the generic community baseline, not verified against RLCraft/ARM specifically;
# treat exact region-size/percent values as tunable, not gospel (see Assumptions Log A3).
JAVA_FLAGS="-Xms6G -Xmx6G \
  -XX:+UseG1GC \
  -XX:+ParallelRefProcEnabled \
  -XX:MaxGCPauseMillis=200 \
  -XX:+UnlockExperimentalVMOptions \
  -XX:+DisableExplicitGC \
  -XX:+AlwaysPreTouch \
  -XX:G1NewSizePercent=30 \
  -XX:G1MaxNewSizePercent=40 \
  -XX:G1HeapRegionSize=8M \
  -XX:G1ReservePercent=20 \
  -XX:G1HeapWastePercent=5 \
  -XX:G1MixedGCCountTarget=4 \
  -XX:InitiatingHeapOccupancyPercent=15 \
  -XX:G1MixedGCLiveThresholdPercent=90 \
  -XX:G1RSetUpdatingPauseTimePercent=5 \
  -XX:SurvivorRatio=32 \
  -XX:+PerfDisableSharedMem \
  -XX:MaxTenuringThreshold=1"
```
Note: `-XX:+AlwaysPreTouch` forces the JVM to touch (and thus commit) the full 6 GB heap at startup — fine on a dedicated Pi with 15 GB total RAM and nothing else heavy running, but confirm the old `~/mcserver` Paper instance is stopped first (CONTEXT.md's discretion item) since pre-touching 6 GB while another JVM also holds several GB could pressure the remaining ~9 GB budget.

### `/forge tps` output format (for SRV-05 measurement)
```
# Source: community-corroborated Forge 1.12.2 command output
# [CITED: github.com/micdoodle8/Galacticraft/issues/3123, discourse.cubecoders.com/t/.../2521]
Dim -1 (minecraft:the_nether): Mean tick time: 0.004 ms. Mean TPS: 20.000
Dim  0 (minecraft:overworld): Mean tick time: 0.300 ms. Mean TPS: 20.000
Dim  1 (minecraft:the_end): Mean tick time: 0.003 ms. Mean TPS: 20.000
Overall: Mean tick time: 0.354 ms. Mean TPS: 20.000
```
Run via RCON (`rcon-cli ... "forge tps"`) so it can be captured to a log file during the 20-minute load test without needing someone at the console.

### CGNAT check (fast range check + slow WAN-IP comparison)
```bash
#!/usr/bin/env bash
# scripts/cgnat-check.sh
# Fast check: is the LAN gateway's own WAN-facing IP in the CGNAT shared range (RFC 6598)?
ROUTER_WAN_IP="$1"   # operator supplies from router admin UI (no universal CLI way to fetch this from the Pi)
if [[ "$ROUTER_WAN_IP" =~ ^100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\. ]]; then
  echo "CGNAT detected: router WAN IP $ROUTER_WAN_IP is in the 100.64.0.0/10 shared address space."
  exit 1
fi
# Slower confirm: does it match what the internet sees?
PUBLIC_IP=$(curl -s ifconfig.me)
if [[ "$ROUTER_WAN_IP" != "$PUBLIC_IP" ]]; then
  echo "CGNAT likely: router WAN IP ($ROUTER_WAN_IP) != public IP seen by internet ($PUBLIC_IP)."
  exit 1
fi
echo "No CGNAT detected: router WAN IP matches public IP ($PUBLIC_IP). Port forwarding should work."
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|-------------------|---------------|--------|
| Guessing CurseForge CDN URLs from file IDs for scripted downloads | Official `api.curseforge.com` REST API with a required `CF_API_KEY` | 2024-07-16 `[CITED: blog.curseforge.com]` | Directly affects `install.sh` design — see Pitfall 1 |
| `mcrcon` as the default community RCON CLI | Either `mcrcon` (build from source on ARM) or `itzg/rcon-cli` (static multi-arch binary) | `rcon-cli` has published multi-arch releases for some years; not a recent shift, but worth noting Pi/ARM hosting specifically favors it | Simpler bootstrap, no build toolchain needed |

**Deprecated/outdated:** None directly deprecated in this domain — Forge 1.12.2 itself is a frozen, unmaintained-but-stable target (as already noted in project STACK.md), so "current approach" here mostly means "current tooling to interact with a fixed old target," not a moving spec.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|----------------|
| A1 | Adoptium's `trixie` apt suite publishes a `temurin-8-jdk` package specifically (not just newer majors) | Standard Stack, Pitfall 3 | `install.sh`'s Java step fails; mitigated by the `apt-cache search` guard + tarball fallback already written into the script recommendation |
| A2 | CurseForge's API will return a working download URL for RLCraft file 4612990 given a valid `CF_API_KEY` (distribution toggle assumed "on") | Pitfall 1, Standard Stack | SRV-01 blocked entirely; mitigated by the manual-download fallback path documented in Pitfall 1 |
| A3 | The generic Aikar's-flags values (region size 8M, `G1NewSizePercent=30`, etc.) are a good starting point for a 6 GB heap on a 4-core ARM Pi, not just x86 hosting-provider defaults | Code Examples | Suboptimal GC pauses; low risk since SRV-05's load test will surface this directly and CONTEXT.md already has a remediation ladder (heap 8G, view-distance 6) |
| A4 | Prism Launcher can hand-install RLCraft 2.9.3 as cleanly as the CurseForge app (only the CurseForge app path was directly verified this session) | Open Questions | The phase's client-install README (success criterion 1) might need to recommend only the CurseForge app path, or needs a second verification pass for Prism |
| A5 | `itzg/rcon-cli`'s current release still publishes a `linux_arm64` asset under a filename matching the `.goreleaser.yml` template at execution time | Standard Stack, Pitfall 2 | Low risk — even if the exact filename shifts, the releases page is easy to check manually during `install.sh` authoring |

## Open Questions

1. **Does RLCraft's CurseForge project currently allow third-party API distribution?**
   - What we know: The toggle exists project-wide and defaults to "on" for existing projects; RLCraft's specific setting was not directly observable without an API key.
   - What's unclear: Whether Shivaxi (RLCraft's author) has left it on.
   - Recommendation: First task of the plan — apply for a CF API key and test the download-url endpoint live; fall back to manual download+checksum if blocked. Gate with `checkpoint:human-verify`.

2. **Exact Prism Launcher steps for hand-installing RLCraft 2.9.3 (for the client README)**
   - What we know: The CurseForge app path is well-documented (search → Install → adjust RAM in Profile Options → Play).
   - What's unclear: Prism Launcher's exact modpack-import flow for a CurseForge-hosted pack (via .zip export vs. built-in CurseForge browse) wasn't independently verified this session.
   - Recommendation: Have the plan's README task verify one client-install path hands-on (CurseForge app, since it's the better-documented one) and treat Prism instructions as a nice-to-have unless a friend specifically needs it (e.g., Linux-only environment where the CurseForge app isn't offered).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|-------------|-----------|---------|----------|
| Java 25 (system) | Must NOT be used for RLCraft | ✓ (already installed) | 25 | N/A — explicitly must stay untouched, Temurin 8 installed alongside |
| Docker 29 | Not used (bare systemd decided) | ✓ (present, unused) | 29 | N/A |
| Temurin 8 JDK (aarch64) | SRV-01 | Not yet installed | — (to be installed via Adoptium apt) | Adoptium tarball via api.adoptium.net if apt package missing (Pitfall 3) |
| Forge 1.12.2-14.23.5.2860 | SRV-01 | Not yet installed | — | None needed — official Maven URL confirmed reachable |
| `zstd` (apt) | SRV-03 backups | Not yet confirmed installed on this Pi | Debian `trixie` stable version | None needed — package confirmed present in `trixie` |
| `cloudflare-ddns` or `ddclient` (apt) | SRV-04 (only if dynamic IP / no CGNAT) | Not yet installed | Debian `trixie` stable version | Manual IP updates if CGNAT-check shows a static IP (per locked decision, DDNS only needed if dynamic) |
| `itzg/rcon-cli` | SRV-02, SRV-03 ops scripts | Not yet installed | Check github.com/itzg/rcon-cli/releases at execution time | `mcrcon` built from source (`build-essential` + `make`) |
| Existing `~/mcserver` (Paper 1.21.6) | Frees ~RAM if stopped | Unknown — needs a live check (`systemctl status`/`pm2 list`) at plan-execution time | — | N/A — recommend stopping it per CONTEXT.md's discretion note if it's running |

**Missing dependencies with no fallback:** none — every dependency in this phase has either a confirmed apt package, a confirmed direct-download URL, or a documented fallback path.

**Missing dependencies with fallback:**
- Temurin 8 apt package (fallback: Adoptium tarball)
- `mcrcon`/`rcon-cli` (fallback: build `mcrcon` from source if no arm64 binary works)

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|----------------|---------|--------------------|
| V2 Authentication | Partial | This phase ships **no real authentication** — `white-list=true` + `enforce-whitelist=true` is a coarse allowlist by in-game nickname only, matching CONTEXT.md's explicit locked decision that Phase 2 replaces this with token auth. Document this as a known, accepted interim gap (already called out in project PITFALLS.md Pitfall 3), not a bug to fix in Phase 1. |
| V3 Session Management | N/A | No session concept exists at the Minecraft-protocol level in offline-mode; out of scope until Phase 2's auth-gate mod. |
| V4 Access Control | Yes | Whitelist enforcement (`enforce-whitelist=true`) is the only access-control mechanism this phase provides — verify it actually rejects non-whitelisted nicks during testing, not just that the config flag is set. |
| V5 Input Validation | N/A | No user-facing input surfaces are built in this phase (no web forms, no launcher) — Minecraft's own protocol handling is Mojang/Forge's concern, out of this project's control. |
| V6 Cryptography | Yes | RCON password: `server.properties` locked decision specifies "localhost, random password in file." Generate with a real CSPRNG (`openssl rand -hex 32` or similar), not a hand-typed or short/guessable string — even though RCON is bound to 127.0.0.1 only, defense-in-depth against any future misconfiguration that accidentally exposes the RCON port matters. |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|------------------------|
| Vanilla/modified client connects directly to the exposed game port, bypassing any launcher-side checks (none exist yet in Phase 1) | Spoofing | Locked decision already restricts this phase to whitelist-only gating — acceptable per project PITFALLS.md Pitfall 3's documented tradeoff for a small trusted friend group; **do not** treat this as "solved," it's explicitly deferred to Phase 2's server-side auth-gate mod. |
| RCON port accidentally exposed to the internet (e.g., a future config change or router UPnP auto-forward) | Elevation of Privilege | Locked decision already binds RCON to `127.0.0.1` only and forwards *only* TCP 25565 — verify with `ss -tlnp` after setup that RCON is not listening on `0.0.0.0`, and confirm the router has no UPnP rule auto-adding port 25575. |
| Backup archives (`~/rlcraft-backups/`) containing world data (incl. any player-identifying info like whitelisted nicknames) left world-readable on a shared Pi | Information Disclosure | Set backup directory permissions to the `asphacean` user only (`chmod 700 ~/rlcraft-backups`); not explicitly in CONTEXT.md's locked decisions but a one-line addition worth including in `backup.sh`. |
| CurseForge API key (once obtained, per Pitfall 1) checked into `install.sh` or committed to the repo | Information Disclosure | Store in `server.env` (already gitignored per the locked decision's "world/, logs/, backups gitignored" — extend that gitignore rule to cover `server.env` explicitly, or keep the key in a separate untracked file). |

## Sources

### Primary (HIGH confidence — official vendor/first-party sources, directly fetched)
- [packages.adoptium.net/artifactory/deb/dists/](https://packages.adoptium.net/artifactory/deb/dists/) — confirmed `trixie` suite present
- [maven.minecraftforge.net](https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860-installer.jar) — Forge 2860 installer reachable
- [curseforge.com/minecraft/modpacks/rlcraft/files/4612990](https://www.curseforge.com/minecraft/modpacks/rlcraft/files/4612990) — project ID 285109, file ID 4612990 confirmed directly
- [github.com/Tiiffi/mcrcon/releases](https://github.com/Tiiffi/mcrcon/releases) — confirmed no arm64 asset
- [packages.debian.org](https://packages.debian.org) (searched `mcrcon`, `zstd`, `ddclient`, `cloudflare-ddns` on `trixie`) — confirmed package presence/absence
- [blog.curseforge.com/introducing-api-key-authentication-for-curseforge-file-downloads](https://blog.curseforge.com/introducing-api-key-authentication-for-curseforge-file-downloads/) — official CurseForge policy announcement

### Secondary (MEDIUM confidence — community docs corroborated across multiple independent sources)
- Aikar's flags baseline: [docs.berrybyte.net](https://docs.berrybyte.net/games/minecraft/aikars-flags), [winternode.com](https://winternode.com/help/games/minecraft-java/configuration/aikars-flags)
- systemd+RCON unit pattern: [teilgedanken.de](https://teilgedanken.de/Blog/post/setting-up-a-minecraft-server-using-systemd/), [forum.level1techs.com](https://forum.level1techs.com/t/automating-and-protecting-minecraft-server-w-systemd-and-selinux/186022)
- RLCraft server pack contents/start script: [hub.tcno.co](https://hub.tcno.co/games/minecraft/mods/rlcraft/server/)
- `/forge tps` output format: [github.com/micdoodle8/Galacticraft#3123](https://github.com/micdoodle8/Galacticraft/issues/3123), [discourse.cubecoders.com](https://discourse.cubecoders.com/t/minecraft-forge-tps-output-not-showing-up-in-console/2521)
- World directory layout (`DIM-1`/`DIM1`): [minecraft.wiki/w/Java_Edition_level_format](https://minecraft.wiki/w/Java_Edition_level_format)
- CGNAT detection methods: [docs.beammp.com](https://docs.beammp.com/FAQ/How-to-check-for-CGNAT/), [oneuptime.com](https://oneuptime.com/blog/post/2026-03-20-detect-cgnat/view)
- CurseForge distribution toggle: [support.curseforge.com](https://support.curseforge.com/support/solutions/articles/9000207877-project-distribution-toggle)
- `itzg/rcon-cli` arm64 release assets: [github.com/itzg/rcon-cli](https://github.com/itzg/rcon-cli), `.goreleaser.yml` build config

### Tertiary (LOW confidence — single-source or not independently corroborated)
- CurseForge CDN URL guessing pattern (`mediafilez.forgecdn.net/files/{first4}/{rest}`): [github.com/PrismLauncher/PrismLauncher/issues/394](https://github.com/PrismLauncher/PrismLauncher/issues/394) — marked "wontfix" by maintainers, current reliability unverified live
- Prism Launcher's exact RLCraft import flow — not independently verified this session (see Open Questions #2)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every core tool's availability (apt package/repo suite, official download URL) was directly fetched and confirmed this session
- Architecture: HIGH — systemd+RCON and backup-consistency patterns are extremely well-established, low novelty
- Pitfalls: MEDIUM — the CurseForge API-key/distribution-toggle risk (Pitfall 1) is real but its exact live status for RLCraft specifically was not confirmed (would require obtaining an API key mid-research); everything else is HIGH confidence

**Research date:** 2026-08-27
**Valid until:** ~2026-09-26 (30 days) — except the CurseForge API-key policy, which should be re-verified at execution time regardless of research age, since it directly gates SRV-01
