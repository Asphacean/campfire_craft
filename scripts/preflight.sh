#!/usr/bin/env bash
# scripts/preflight.sh
#
# Idempotent host bootstrap for the RLCraft server (Phase 1, Plan 01, Task 1):
#   1. Materialize server.env from server.env.example (never overwrites an existing one)
#   2. Add the Adoptium apt repo and install Temurin 8 JDK arm64 (or fall back to the
#      official tarball if the apt package is missing) — system Java 25 stays untouched
#   3. Install zstd, unzip, curl, jq
#   4. Install itzg/rcon-cli (aarch64 static binary, checksum-verified)
#   5. Detect and stand down any running instance of the old ~/mcserver Paper server
#
# A second run must exit 0 and change nothing (idempotent).
#
# Exit codes:
#   0 = bootstrap complete (or already complete)
#   1 = JAVA8_BIN did not resolve to a working Java 8 runtime
#   2 = temurin-8-jdk apt candidate absent AND tarball fallback failed
#   3 = rcon-cli asset/checksum verification failed

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

ENV_FILE="$REPO_ROOT/server.env"
ENV_EXAMPLE="$REPO_ROOT/server.env.example"
OLD_SERVER_DIR="$HOME/mcserver"

log() { echo "[preflight] $*"; }

# ---------------------------------------------------------------------------
# Step 1: server.env
# ---------------------------------------------------------------------------
if [ ! -f "$ENV_FILE" ]; then
  log "Creating server.env from server.env.example"
  cp "$ENV_EXAMPLE" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
  RCON_PW="$(openssl rand -hex 24)"
  sed -i "s|^RCON_PASSWORD=.*|RCON_PASSWORD=${RCON_PW}|" "$ENV_FILE"
else
  log "server.env already exists — not overwriting"
  chmod 600 "$ENV_FILE"
fi

# helper: persist KEY=VALUE into server.env (create the key if absent).
# Quotes the value — an unquoted path with spaces would break `. ./server.env`.
set_env_var() {
  local key="$1" val="$2"
  local escaped="${val//\"/\\\"}"
  if grep -q "^${key}=" "$ENV_FILE"; then
    sed -i "s|^${key}=.*|${key}=\"${escaped}\"|" "$ENV_FILE"
  else
    printf '%s="%s"\n' "$key" "$escaped" >>"$ENV_FILE"
  fi
}

# shellcheck disable=SC1090
. "$ENV_FILE"

# ---------------------------------------------------------------------------
# Step 2-4: Temurin 8 JDK
# ---------------------------------------------------------------------------
java8_works() {
  [ -n "${JAVA8_BIN:-}" ] && [ -x "$JAVA8_BIN" ] && "$JAVA8_BIN" -version 2>&1 | grep -q '1\.8\.0'
}

if java8_works; then
  log "Java 8 already resolved and working: $JAVA8_BIN"
else
  log "Adding Adoptium apt repository"
  sudo mkdir -p /etc/apt/keyrings
  curl -fsSL https://packages.adoptium.net/artifactory/api/gpg/key/public | sudo tee /etc/apt/keyrings/adoptium.asc >/dev/null
  CODENAME="$(awk -F= '/^VERSION_CODENAME/{print $2}' /etc/os-release)"
  echo "deb [signed-by=/etc/apt/keyrings/adoptium.asc] https://packages.adoptium.net/artifactory/deb ${CODENAME} main" \
    | sudo tee /etc/apt/sources.list.d/adoptium.list >/dev/null
  sudo apt-get update -qq

  if apt-cache policy temurin-8-jdk 2>/dev/null | grep -q 'Candidate:.*[0-9]'; then
    log "temurin-8-jdk apt candidate found — installing"
    sudo apt-get install -y -qq temurin-8-jdk
    RESOLVED_JAVA8_BIN="$(compgen -G '/usr/lib/jvm/temurin-8-jdk*/bin/java' | head -1 || true)"
  else
    log "temurin-8-jdk NOT FOUND in the Adoptium apt repo for this suite (Pitfall 3) — falling back to the official tarball"
    sudo mkdir -p /opt/temurin-8
    # WR-04: mktemp instead of a fixed, guessable /tmp path — this file is fed
    # into a root-privileged `sudo tar`, so a predictable name in shared /tmp
    # is a symlink/TOCTOU setup.
    TARBALL="$(mktemp /tmp/temurin-8-jdk-aarch64.XXXXXX.tar.gz)"
    # WR-03: unlike every other download path in this repo, the tarball
    # fallback fed straight into a root-owned `sudo tar` extraction with no
    # integrity check. The Adoptium assets API publishes a sha256 alongside
    # the direct download link for the same binary the old /v3/binary/latest
    # redirect would have served — fetch that metadata first and verify
    # before trusting the artifact. (jq isn't installed yet at this point in
    # the script — Step 5 below — so this is parsed with grep, matching the
    # rest of this script's pre-jq dependency handling.)
    ASSET_JSON="$(curl -fsSL "https://api.adoptium.net/v3/assets/latest/8/hotspot?architecture=aarch64&image_type=jdk&os=linux&vendor=eclipse")"
    TARBALL_URL="$(printf '%s' "$ASSET_JSON" | grep -oP '"link"\s*:\s*"\K[^"]+' | head -1)"
    TARBALL_SHA256="$(printf '%s' "$ASSET_JSON" | grep -oP '"checksum"\s*:\s*"\K[a-f0-9]+' | head -1)"
    if [ -z "$TARBALL_URL" ] || [ -z "$TARBALL_SHA256" ]; then
      log "FATAL: Adoptium assets API did not return a download URL/checksum for the Temurin 8 aarch64 tarball"
      exit 2
    fi
    curl -fsSL "$TARBALL_URL" -o "$TARBALL"
    if ! echo "${TARBALL_SHA256}  ${TARBALL}" | sha256sum -c - >/dev/null 2>&1; then
      log "FATAL: downloaded Temurin 8 tarball failed sha256 verification against Adoptium's published checksum"
      rm -f "$TARBALL"
      exit 2
    fi
    sudo tar -xzf "$TARBALL" -C /opt/temurin-8 --strip-components=0
    rm -f "$TARBALL"
    RESOLVED_JAVA8_BIN="$(compgen -G '/opt/temurin-8/*/bin/java' | head -1 || true)"
  fi

  if [ -z "${RESOLVED_JAVA8_BIN:-}" ] || [ ! -x "$RESOLVED_JAVA8_BIN" ]; then
    log "FATAL: could not resolve a Java 8 binary on disk after install"
    exit 2
  fi

  set_env_var JAVA8_BIN "$RESOLVED_JAVA8_BIN"
  JAVA8_BIN="$RESOLVED_JAVA8_BIN"

  if ! java8_works; then
    log "FATAL: resolved JAVA8_BIN ($JAVA8_BIN) does not report a 1.8.0 build"
    exit 1
  fi
  log "Java 8 resolved: $JAVA8_BIN ($("$JAVA8_BIN" -version 2>&1 | head -1))"
fi

# System Java 25 must stay the default — verify, never modify java-alternatives/JAVA_HOME here.
if ! java -version 2>&1 | grep -q 'version "25'; then
  log "WARNING: system 'java' no longer reports version 25 — this script never changed it, investigate separately"
fi

# ---------------------------------------------------------------------------
# Step 5: ops tooling (zstd, unzip, curl, jq)
# ---------------------------------------------------------------------------
log "Ensuring zstd, unzip, curl, jq, dnsutils are installed"
# WR-06: dnsutils provides `dig`, which scripts/reachability.sh depends on but
# this script never installed — on a fresh minimal image that made
# reachability.sh's DNS check fail with a misleading "DNS did not converge"
# verdict instead of the real "dig: command not found".
sudo apt-get install -y -qq zstd unzip curl jq dnsutils

# ---------------------------------------------------------------------------
# Step 6: rcon-cli (itzg/rcon-cli, aarch64 static binary)
# ---------------------------------------------------------------------------
if [ -x /usr/local/bin/rcon-cli ] && /usr/local/bin/rcon-cli --help >/dev/null 2>&1; then
  log "rcon-cli already installed: $(/usr/local/bin/rcon-cli --help 2>&1 | head -2 | tail -1 || true)"
else
  log "Installing rcon-cli"
  RELEASE_JSON="$(curl -fsSL https://api.github.com/repos/itzg/rcon-cli/releases/latest)"
  TAG="$(echo "$RELEASE_JSON" | jq -r '.tag_name')"
  ASSET_NAME="rcon-cli_${TAG}_linux_arm64.tar.gz"
  CHECKSUMS_NAME="rcon-cli_${TAG}_checksums.txt"
  ASSET_URL="$(echo "$RELEASE_JSON" | jq -r --arg n "$ASSET_NAME" '.assets[] | select(.name == $n) | .browser_download_url')"
  CHECKSUMS_URL="$(echo "$RELEASE_JSON" | jq -r --arg n "$CHECKSUMS_NAME" '.assets[] | select(.name == $n) | .browser_download_url')"

  if [ -z "$ASSET_URL" ] || [ "$ASSET_URL" = "null" ]; then
    log "FATAL: rcon-cli release $TAG has no asset named $ASSET_NAME — arm64 asset naming has drifted (Assumption A5), refusing to guess"
    exit 3
  fi

  WORKDIR="$(mktemp -d)"
  curl -fsSL "$ASSET_URL" -o "$WORKDIR/$ASSET_NAME"
  curl -fsSL "$CHECKSUMS_URL" -o "$WORKDIR/$CHECKSUMS_NAME"

  (cd "$WORKDIR" && grep "  ${ASSET_NAME}\$" "$CHECKSUMS_NAME" | sha256sum -c -) || {
    log "FATAL: rcon-cli asset failed checksum verification against $CHECKSUMS_NAME"
    rm -rf "$WORKDIR"
    exit 3
  }

  tar -xzf "$WORKDIR/$ASSET_NAME" -C "$WORKDIR" rcon-cli
  sudo install -m 0755 "$WORKDIR/rcon-cli" /usr/local/bin/rcon-cli
  rm -rf "$WORKDIR"

  if ! file /usr/local/bin/rcon-cli | grep -qi 'aarch64\|ARM aarch64'; then
    log "FATAL: installed rcon-cli binary is not an aarch64 executable"
    exit 3
  fi
  log "rcon-cli installed: $TAG"
fi

# ---------------------------------------------------------------------------
# Step 7: old Paper server standdown
# ---------------------------------------------------------------------------
OLD_PAPER_LINE=""

# a) running java process pointed at the old server dir
OLD_PID="$(pgrep -f "java.*${OLD_SERVER_DIR}" || true)"
if [ -n "$OLD_PID" ]; then
  log "Found running old Paper server process(es): $OLD_PID — sending SIGTERM"
  kill $OLD_PID || true
  sleep 5
  OLD_PAPER_LINE="OLD_PAPER: stopped process (kill)"
fi

# b) a systemd unit
if [ -z "$OLD_PAPER_LINE" ]; then
  OLD_UNIT="$(systemctl list-units --type=service --all --no-legend 2>/dev/null | awk '{print $1}' | grep -iE 'mcserver|paper' || true)"
  if [ -n "$OLD_UNIT" ] && systemctl is-active --quiet "$OLD_UNIT" 2>/dev/null; then
    log "Found active systemd unit for old server: $OLD_UNIT — stopping and disabling"
    sudo systemctl stop "$OLD_UNIT" || true
    sudo systemctl disable "$OLD_UNIT" || true
    OLD_PAPER_LINE="OLD_PAPER: stopped systemd unit"
  fi
fi

# c) a pm2 entry
if [ -z "$OLD_PAPER_LINE" ] && command -v pm2 >/dev/null 2>&1; then
  OLD_PM2="$(pm2 jlist 2>/dev/null | jq -r '.[] | select(.name | test("mcserver|paper"; "i")) | .name' || true)"
  if [ -n "$OLD_PM2" ]; then
    log "Found pm2 entry for old server: $OLD_PM2 — stopping"
    pm2 stop "$OLD_PM2" || true
    OLD_PAPER_LINE="OLD_PAPER: stopped pm2"
  fi
fi

if [ -z "$OLD_PAPER_LINE" ]; then
  OLD_PAPER_LINE="OLD_PAPER: absent"
fi

# ---------------------------------------------------------------------------
# Step 8: summary (OLD_PAPER: line printed exactly once, here, as the
# canonical marker line the acceptance criteria greps for)
# ---------------------------------------------------------------------------
log "Summary:"
log "  JAVA8_BIN=$JAVA8_BIN"
log "  $("$JAVA8_BIN" -version 2>&1 | head -1)"
log "  rcon-cli: installed at /usr/local/bin/rcon-cli ($(file -b /usr/local/bin/rcon-cli | cut -d, -f1-2))"
log "  zstd: $(zstd --version 2>&1 | head -1)"
echo "$OLD_PAPER_LINE"

exit 0
