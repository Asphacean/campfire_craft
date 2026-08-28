---
phase: 01-playable-server-on-the-pi
reviewed: 2026-08-28T00:00:00Z
depth: standard
files_reviewed: 20
files_reviewed_list:
  - .gitignore
  - docs/CLIENT-SETUP.md
  - scripts/backup.sh
  - scripts/cgnat-check.sh
  - scripts/fetch-pack.sh
  - scripts/harden-rcon.sh
  - scripts/install-units.sh
  - scripts/install.sh
  - scripts/preflight.sh
  - scripts/reachability.sh
  - scripts/restore.sh
  - scripts/start-server.sh
  - scripts/tps-log.sh
  - server.env.example
  - server/server.properties.template
  - systemd/rlcraft-backup.service
  - systemd/rlcraft-backup.timer
  - systemd/rlcraft-nft.service
  - systemd/rlcraft-restart.service
  - systemd/rlcraft-restart.timer
  - systemd/rlcraft.service
findings:
  critical: 2
  warning: 8
  info: 6
  total: 16
status: issues_found
---

# Phase 01: Code Review Report

**Reviewed:** 2026-08-28
**Depth:** standard
**Files Reviewed:** 20
**Status:** issues_found

## Summary

Reviewed the Phase 1 ops scripts and systemd units for an RLCraft (Forge 1.12.2,
Java 8) server on a Raspberry Pi 5. `bash -n` passes clean on every script;
`shellcheck` was not available in this environment so findings below come from
manual read-through and tracing of `server.env` sourcing, `set -e`/`pipefail`
behavior, and cross-file call chains (systemd units → scripts → `rcon-cli`).

The overall design is careful — the backup/restore EXIT-trap discipline, the
zstd-archive validation gate, the pack integrity gate in `fetch-pack.sh`, and
the deliberately scoped nftables table are all sound. However, two real
security defects surfaced: the RCON password is passed on the command line in
four separate places (visible to any local user via `/proc/<pid>/cmdline` or
`ps`), and `online-mode=false` combined with an open (no-whitelist) server
enables username/UUID impersonation, including impersonation of any operator
account keyed by name. Several warnings cover race conditions (no locking
between backup/restore, no boot-ordering guarantee between the nftables unit
and the game unit) and silently-swallowed validation errors.

## Critical Issues

### CR-01: RCON password passed on the command line — visible to any local user via `ps`/`/proc/<pid>/cmdline`

**File:** `scripts/backup.sh:35`, `scripts/restore.sh:59`, `scripts/tps-log.sh:32`, `systemd/rlcraft.service:20`

**Issue:** Every RCON invocation in this codebase passes the password as a
plain CLI argument:

```bash
rcon() {
  rcon-cli --host "$RCON_HOST" --port "$RCON_PORT" --password "$RCON_PASSWORD" "$@"
}
```

and, more severely, the systemd unit does the same thing at every service
stop/restart, with the password expanded by systemd itself from
`EnvironmentFile`:

```
ExecStop=-/usr/local/bin/rcon-cli --host ${RCON_HOST} --port ${RCON_PORT} --password ${RCON_PASSWORD} stop
```

On Linux, process argv is world-readable via `ps aux`/`ps -ef` and
`/proc/<pid>/cmdline` unless `hidepid=2` is set on `/proc` (not configured
anywhere in this repo). Any other local account on the Pi (or any process
that can read `/proc`) can recover the RCON password simply by watching for
`rcon-cli` invocations — which happen automatically every 6 hours via
`rlcraft-backup.timer`, and on every `systemctl stop/restart rlcraft`. This is
exactly the failure mode `harden-rcon.sh`'s network-level drop rule (D-08) was
built to compensate for on the wire, but it does nothing against local
process-table exposure.

**Fix:** Use whatever non-argv mechanism `rcon-cli` supports for
credentials — itzg/rcon-cli reads `RCON_HOST`/`RCON_PORT`/`RCON_PASSWORD`
(or similarly named) environment variables as an alternative to flags; since
every caller here already has these as environment variables (`server.env`
sourced, or systemd `EnvironmentFile`), simply drop the `--password` flag and
let `rcon-cli` inherit the environment instead of receiving it as argv:

```bash
rcon() {
  RCON_HOST="$RCON_HOST" RCON_PORT="$RCON_PORT" RCON_PASSWORD="$RCON_PASSWORD" \
    rcon-cli "$@"
}
```

and for the systemd unit, export the same variables and drop them from the
`ExecStop=` command line entirely (systemd `Environment=`/`EnvironmentFile=`
already sets the process environment for `ExecStop`, no explicit
interpolation needed). If `rcon-cli` genuinely has no non-argv credential
path, write the credentials to a mode-600 temp file it can read via a
`--config`/`@file` mechanism, or restrict `/proc` visibility with
`hidepid=2` as defense-in-depth.

### CR-02: `online-mode=false` + open access allows username/UUID impersonation, including of operator accounts

**File:** `server/server.properties.template:40`

**Issue:**

```
online-mode=false
```

With `online-mode=false`, the server never verifies a connecting client's
username against Mojang's session servers — any player can connect with any
username, entirely unauthenticated. Combined with the current Phase 1 state
(`docs/CLIENT-SETUP.md`: "the server has no whitelist — access is open to
anyone"), this means:

1. Anyone can impersonate any other player's in-game nickname (griefing,
   social engineering, chat impersonation).
2. Vanilla 1.12.2 derives an offline-mode UUID deterministically from the
   username (`UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`). If
   `server/ops.json` is ever populated (op-permission-level=4 is already set
   in this same template), any player can reconnect with the operator's
   exact nickname and receive that op's UUID — and therefore that op's
   permissions — with no credential check at all. This is a full
   authentication bypass / privilege-escalation path, not merely
   impersonation.

This is inherited from the RLCraft Server Pack's bundled defaults (per this
file's own header comment) rather than a value Phase 1 deliberately set, but
it ships as-is in the reviewed template and will be the live configuration.

**Fix:** At minimum, before any operator account is ever granted OP, either
flip `online-mode=true` (requires a paid-account Mojang login for every
player — a real tradeoff against Phase 1's "friends join easily" goal, worth
an explicit decision rather than a silent inherited default) or restrict OP
grants to a whitelist-gated, name-locked identity once Phase 2's token auth
lands. At the very least, document this as a known, accepted risk next to
the whitelist decision (D-09) rather than leaving it as an unexamined pack
default.

## Warnings

### WR-01: No mutual exclusion between `backup.sh` and `restore.sh` (or two overlapping `backup.sh` runs)

**File:** `scripts/backup.sh` (whole file), `scripts/restore.sh` (whole file)

**Issue:** Both scripts independently call `rcon save-off` / `rcon save-all`
/ `sleep 5` / tar the `world/` tree / `rcon save-on`, with no `flock` or other
lock file preventing two instances from running concurrently. `backup.sh`
fires every 6 hours via `rlcraft-backup.timer`; if an operator runs
`restore.sh` while a scheduled backup is mid-flight (or a previous backup
run has hung), the two runs interleave their save-off/save-on toggling and
can race on `restore.sh`'s `mv server/world server/world.pre-restore-<ts>`
step — `backup.sh`'s `tar -C server world` would then fail against a
directory that has just been moved out from under it, or capture a
partially-moved state.

**Fix:** Add a `flock` guard at the top of both scripts, e.g.:

```bash
exec 9>"$ROOT_DIR/.backup.lock"
flock -n 9 || { echo "FATAL: another backup/restore run is in progress" >&2; exit 1; }
```

using the same lock file path in both scripts.

### WR-02: No boot-time ordering guarantee between `rlcraft-nft.service` and `rlcraft.service`

**File:** `systemd/rlcraft-nft.service:7`, `systemd/rlcraft.service:1-4`

**Issue:** `rlcraft-nft.service` only declares `After=network.target`, and
`rlcraft.service` only declares `After=network-online.target` — neither unit
orders itself relative to the other. Both are `WantedBy=multi-user.target`,
so systemd is free to start them in parallel at boot. This reopens the exact
window `harden-rcon.sh` exists to close: for a brief period after boot, the
game server's RCON listener can be up before the nftables drop rule is
loaded, exposing RCON to the network (password-protected, but unprotected by
the intended network-layer control) until `rlcraft-nft.service` finishes.

**Fix:** Add `After=rlcraft-nft.service` and `Requires=rlcraft-nft.service`
to `rlcraft.service`, or equivalently `Before=rlcraft.service` on
`rlcraft-nft.service`, so the firewall rule is guaranteed to load before the
listener starts.

### WR-03: Temurin tarball fallback has no integrity check, unlike every other download path in this codebase

**File:** `scripts/preflight.sh:84-90`

**Issue:** The apt-package path for Temurin 8 is trusted implicitly (apt's
own signing), but the tarball fallback path downloads and extracts —
**as root**, via `sudo tar`— with zero verification:

```bash
curl -fsSL "https://api.adoptium.net/v3/binary/latest/8/ga/linux/aarch64/jdk/hotspot/normal/eclipse" -o "$TARBALL"
sudo tar -xzf "$TARBALL" -C /opt/temurin-8 --strip-components=0
```

Every other acquisition path in this repo (`fetch-pack.sh`'s pack zip,
`preflight.sh`'s own `rcon-cli` install a few lines later) verifies a
sha256 checksum before trusting the artifact. This one silently doesn't,
despite feeding directly into a root-privileged extraction.

**Fix:** Adoptium's API exposes a checksum alongside the binary
(`.../eclipse?project=jdk` metadata endpoint, or the `-o` response headers'
content digest); verify it the same way `rcon-cli`'s checksum step does
before the `sudo tar` runs.

### WR-04: Predictable, fixed temp-file paths in `/tmp` instead of `mktemp`

**File:** `scripts/preflight.sh:85`, `scripts/fetch-pack.sh:143`

**Issue:** Both scripts write to a hardcoded, predictable path under the
shared `/tmp`:

```bash
TARBALL="/tmp/temurin-8-jdk-aarch64.tar.gz"                 # preflight.sh:85
... >/tmp/fetch-pack-unzip-test.log 2>&1                    # fetch-pack.sh:143
```

`preflight.sh`'s case is the more serious of the two since the file is then
consumed by `sudo tar`. A fixed, guessable name in world-writable `/tmp` is
the classic setup for a symlink/TOCTOU race on a multi-user box; even on a
single-operator Pi this is a cheap, well-known fix.

**Fix:** Use `mktemp` for both:

```bash
TARBALL="$(mktemp /tmp/temurin-8-jdk-aarch64.XXXXXX.tar.gz)"
```

### WR-05: `tps-log.sh` swallows argument-parsing failures instead of aborting

**File:** `scripts/tps-log.sh:51-52`

**Issue:**

```bash
DURATION_SEC=$(parse_duration_secs "$DURATION_ARG")
INTERVAL_SEC=$(parse_duration_secs "$INTERVAL_ARG")
```

`parse_duration_secs` calls `exit 1` on an unparseable argument, but that
`exit` only terminates the command-substitution subshell — the assignment
still "succeeds" with an **empty** string, and because this script
deliberately runs under `set -uo pipefail` (no `-e`, by design, so a single
bad RCON sample doesn't abort a 20-minute run), the empty value is never
checked. `END=$(( START + DURATION_SEC ))` then silently treats the empty
value as `0` in bash arithmetic, so the sampling loop takes exactly one
sample and exits — a malformed `duration`/`interval` argument produces a
near-instant, silently truncated run instead of the documented usage error.

**Fix:** Check the exit status explicitly:

```bash
DURATION_SEC=$(parse_duration_secs "$DURATION_ARG") || exit 1
INTERVAL_SEC=$(parse_duration_secs "$INTERVAL_ARG") || exit 1
```

### WR-06: `reachability.sh` depends on `dig`, which `preflight.sh` never installs

**File:** `scripts/reachability.sh:33`, `scripts/preflight.sh:115-116`

**Issue:** `resolve_domain()` shells out to `dig +short "$DOMAIN"`, but
`preflight.sh`'s dependency-install step only ensures `zstd unzip curl jq`
are present — `dnsutils`/`bind9-dnsutils` (which provides `dig`) is never
installed. On a fresh Debian 13 minimal image this command will fail with
`dig: command not found`; because the script runs under `set -uo pipefail`
(no `-e`) and captures `dig`'s (nonexistent) output via `| tail -1`, the
symptom is a silently-empty `$RESOLVED` that retries for the full 300s
budget and then reports `VERDICT: FAIL — DNS did not converge`, masking the
real root cause (missing binary) behind a misleading "DNS" verdict.

**Fix:** Either add `dnsutils` to `preflight.sh`'s package install list, or
have `reachability.sh` check `command -v dig` up front and fail fast with a
clear "dig not found, install dnsutils" message.

### WR-07: `restore.sh` never verifies/repairs file ownership after extraction

**File:** `scripts/restore.sh:112-122`

**Issue:** `rlcraft.service` runs as `User=asphacean`. `restore.sh` itself
is not run under `sudo` as a whole (only the `systemctl stop`/`start` calls
inside it are individually elevated), so extraction normally inherits the
invoking user. But nothing in the script checks or enforces this — if an
operator runs `sudo scripts/restore.sh` (a very natural thing to do given
the script also needs `sudo systemctl stop/start`), the `mv`/`tar -x` steps
at lines 113–121 run as root, leaving `server/world/*` root-owned. The next
`rlcraft.service` start (as `asphacean`) would then fail or corrupt saves on
write, and this failure mode would only surface *after* the restore already
reported success.

**Fix:** Guard at the top of the script:

```bash
if [[ "$(id -un)" != "asphacean" ]]; then
  echo "FATAL: run this as asphacean, not root/sudo (systemctl calls inside will prompt for sudo as needed)" >&2
  exit 1
fi
```

or `chown -R asphacean:asphacean server/world` after extraction, before
starting the service.

### WR-08: `cgnat-check.sh`'s `set_env_var` silently drops writes if `server.env` doesn't exist yet

**File:** `scripts/cgnat-check.sh:31-39`

**Issue:**

```bash
set_env_var() {
  local key="$1" val="$2"
  local escaped="${val//\"/\\\"}"
  if [ -f "$ENV_FILE" ] && grep -q "^${key}=" "$ENV_FILE"; then
    sed -i "s|^${key}=.*|${key}=\"${escaped}\"|" "$ENV_FILE"
  elif [ -f "$ENV_FILE" ]; then
    printf '%s="%s"\n' "$key" "$escaped" >>"$ENV_FILE"
  fi
}
```

Both branches require `$ENV_FILE` to already exist; there is no `else`
branch. The script's own header says "Run before anything else in the
phase" — if an operator follows that literally and runs `cgnat-check.sh`
before `preflight.sh` (which is what creates `server.env`), both
`set_env_var PUBLIC_IP_AT_SETUP` and `set_env_var CGNAT_VERDICT` silently
no-op. The script still prints its `CGNAT: $VERDICT` line and exits with the
correct code, so the run *looks* successful while the persisted verdict is
simply lost, with no warning at all.

**Fix:** Print a warning (or create the file, mirroring `preflight.sh`'s own
`cp server.env.example server.env` bootstrap) when `$ENV_FILE` is missing:

```bash
if [ ! -f "$ENV_FILE" ]; then
  echo "WARNING: $ENV_FILE does not exist — CGNAT verdict not persisted, run preflight.sh first" >&2
fi
```

## Info

### IN-01: `set_env_var` helper duplicated four times, and doesn't escape sed-special characters

**File:** `scripts/cgnat-check.sh:31-39`, `scripts/fetch-pack.sh:36-46`, `scripts/install.sh:28-36`, `scripts/preflight.sh:47-55`

**Issue:** The identical `set_env_var` implementation is copy-pasted across
four scripts. It also only escapes double quotes in the value
(`${val//\"/\\\"}"`) — a value containing `&` (sed's "whole match"
backreference) or the `|` delimiter used by the `s|...|...|` command would
corrupt the resulting `server.env` line or break the `sed` invocation
outright. None of the current call sites pass attacker-controlled input
through this path today, so this is low-severity, but the duplication makes
it four places to fix instead of one when it does bite.

**Fix:** Factor into a single `scripts/lib/env.sh` sourced by all four, and
escape `&`, `\`, and the sed delimiter, not just `"`.

### IN-02: `install.sh`'s top-level-directory detection miscounts stray top-level files

**File:** `scripts/install.sh:83`

**Issue:**

```bash
UNIQUE_TOP_COUNT=$(unzip -Z1 "$PACK_ZIP" | sed -E 's#^([^/]+)/.*#\1#' | sort -u | wc -l)
```

The `sed` pattern requires a `/` to strip a path down to its top-level
directory name; a zip entry that happens to sit directly at the archive
root with no `/` (e.g. a stray top-level `readme.txt`) passes through
unchanged and is counted as its own distinct "top-level directory," which
can inflate `UNIQUE_TOP_COUNT` past 1 and skip the wrapping-directory-strip
logic even when the archive is otherwise single-rooted. Not currently
triggered by the pinned RLCraft pack layout, but the detection heuristic
itself is imprecise.

**Fix:** Exclude entries with no `/` from the count, or `grep '/'` before
the `sed`/`sort -u`.

### IN-03: `discover_server_jar`'s `head -1` is non-deterministic if more than one candidate jar exists

**File:** `scripts/install.sh:110-117`

**Issue:** Both the `-universal.jar` and fallback `forge-*.jar` lookups pipe
`find` through `head -1`. `find`'s output order is filesystem-dependent, not
alphabetical or otherwise stable — if the Forge installer (or a future
version of it) ever produces two matching candidates, which jar gets
persisted as `SERVER_JAR` would vary run to run.

**Fix:** Add `| sort` before `head -1` for a deterministic (if still
arbitrary) choice, or fail loudly if more than one candidate matches.

### IN-04: Several network calls have no timeout, risking an indefinite hang

**File:** `scripts/preflight.sh:72,86,125,138-139`, `scripts/fetch-pack.sh:75-76,80,100`

**Issue:** Unlike `cgnat-check.sh`/`reachability.sh`'s `curl -s --max-time
10`, most `curl` invocations in `preflight.sh` and `fetch-pack.sh` (GPG key
fetch, apt repo setup, Forge/rcon-cli/CDN downloads) have no `--max-time` or
`--connect-timeout`. A stalled connection on any of these hangs the
otherwise-idempotent bootstrap script indefinitely with no operator
feedback.

**Fix:** Add `--max-time <n> --connect-timeout <n>` consistently to every
`curl` call in these scripts.

### IN-05: Unquoted `kill $OLD_PID` relies on undocumented word-splitting

**File:** `scripts/preflight.sh:167`

**Issue:** `kill $OLD_PID || true` is deliberately unquoted so that multiple
space-separated PIDs from `pgrep -f` each get killed, but nothing documents
that this is intentional — it reads identically to the common SC2086
mistake, and a future editor "fixing" it by adding quotes would silently
break the multi-PID case.

**Fix:** Either quote it and loop explicitly (`for pid in $OLD_PID; do kill
"$pid"; done`), or leave it unquoted with a `# shellcheck disable=SC2086 —
intentional: multiple space-separated PIDs` comment.

### IN-06: Adoptium GPG key trusted on first fetch with no fingerprint pin

**File:** `scripts/preflight.sh:72`

**Issue:**

```bash
curl -fsSL https://packages.adoptium.net/artifactory/api/gpg/key/public | sudo tee /etc/apt/keyrings/adoptium.asc >/dev/null
```

The Adoptium signing key is trusted purely on the basis of HTTPS to
`packages.adoptium.net`, with no independent fingerprint check. A
compromised CDN/DNS/CA at that moment silently roots a malicious apt repo. A
common, low-effort pattern industry-wide, but worth noting since every other
artifact in this codebase (pack zip, rcon-cli, Forge jar via Maven's HTTPS)
gets an explicit integrity gate and this one doesn't.

**Fix:** Pin and verify the key's known fingerprint before trusting it, if a
stable published fingerprint is available from Adoptium.

---

_Reviewed: 2026-08-28_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
