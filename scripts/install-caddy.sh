#!/usr/bin/env bash
# scripts/install-caddy.sh
#
# Idempotent host install of Caddy from the official apt repo (RESEARCH.md's
# Installation block — Debian's own bundled 2.6.2 is three years old; this
# host wants the current 2.11.x family). Deploys caddy/Caddyfile to
# /etc/caddy/Caddyfile and grants the `caddy` system user exactly the
# filesystem access it needs: traversal into /home/asphacean, read on the
# leaf key and the pack/ tree, nothing else (D-14). Never touches ~/pbwiki,
# any Docker container, sing-box, any nftables ruleset, or rlcraft.service.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CADDYFILE="$ROOT_DIR/caddy/Caddyfile"

log() { echo "[install-caddy] $*"; }

# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"
: "${HTTPS_PORT:?HTTPS_PORT not set in server.env}"
: "${PACK_DIR:?PACK_DIR not set in server.env}"
: "${CA_DIR:?CA_DIR not set in server.env}"
: "${DOMAIN:?DOMAIN not set in server.env}"

# --- drift guard: caddy/Caddyfile is read by a service that does not source
# server.env, so it carries HTTPS_PORT/PACK_DIR as absolute literals. Assert
# they still agree before doing anything privileged. ---
if ! grep -qF "$PACK_DIR" "$CADDYFILE"; then
  echo "FATAL: $CADDYFILE does not contain PACK_DIR ($PACK_DIR) — the Caddyfile has drifted from server.env" >&2
  exit 1
fi
if ! grep -qF ":$HTTPS_PORT" "$CADDYFILE"; then
  echo "FATAL: $CADDYFILE does not contain ':$HTTPS_PORT' — the Caddyfile has drifted from server.env" >&2
  exit 1
fi

# --- port pre-flight: never bind a port another service already owns
# (D-14, T-03-01-13). Skipped when caddy.service itself already owns the
# port — that is the expected steady state on every re-run after the first
# install, and this script must stay idempotent. The check still fires on a
# genuinely fresh install (or if some other process has grabbed the port
# while caddy.service is inactive), which is what T-03-01-13 cares about. ---
if ! systemctl is-active --quiet caddy && ss -ltn "sport = :$HTTPS_PORT" | grep -q LISTEN; then
  echo "FATAL: something is already listening on :$HTTPS_PORT and it is not caddy.service — refusing to install/restart Caddy" >&2
  exit 1
fi

# --- install the official Caddy apt repo (skip repo setup if already done) ---
KEYRING=/usr/share/keyrings/caddy-stable-archive-keyring.gpg
SOURCES_LIST=/etc/apt/sources.list.d/caddy-stable.list
if [[ -f "$KEYRING" ]]; then
  log "Caddy apt repo keyring already present — skipping repo setup"
else
  log "installing the official Caddy apt repo"
  sudo apt-get install -y debian-keyring debian-archive-keyring apt-transport-https curl
  curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o "$KEYRING"
  curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee "$SOURCES_LIST" >/dev/null
  sudo chmod o+r "$KEYRING" "$SOURCES_LIST"
  sudo apt-get update
fi
sudo apt-get install -y caddy

# --- grant the caddy system user exactly the access it needs, nothing more ---
if ! command -v setfacl >/dev/null 2>&1; then
  sudo apt-get install -y acl || true
fi
if command -v setfacl >/dev/null 2>&1 && setfacl -m u:caddy:--x /home/asphacean 2>/dev/null; then
  log "granted caddy a traversal-only ACL on /home/asphacean"
else
  log "WARNING: setfacl unavailable or failed — falling back to chmod 711 on /home/asphacean"
  chmod 711 /home/asphacean
fi

sudo chgrp caddy "$CA_DIR/$DOMAIN-key.pem"
chmod 640 "$CA_DIR/$DOMAIN-key.pem"
chmod 755 "$CA_DIR"
mkdir -p -m 755 "$PACK_DIR"
chmod -R a+rX "$PACK_DIR"

# --- /etc/hosts: lets this Pi reach its own Caddy under the certificate's
# name. Outside-in checks (plan 03-03) must use `curl --resolve` against the
# public IP instead — this entry only applies on this host. ---
if grep -qF "127.0.0.1 $DOMAIN" /etc/hosts; then
  log "/etc/hosts already maps $DOMAIN to 127.0.0.1"
else
  printf '127.0.0.1 %s # campfire.pub Phase 3: local resolution so this Pi reaches its own Caddy under the cert name; outside-in checks use curl --resolve instead\n' "$DOMAIN" | sudo tee -a /etc/hosts >/dev/null
  log "added /etc/hosts entry for $DOMAIN"
fi

# --- validate, deploy, (re)start ---
caddy validate --config "$CADDYFILE"

sudo install -m 644 "$CADDYFILE" /etc/caddy/Caddyfile

if systemctl is-active --quiet caddy; then
  sudo systemctl restart caddy
  log "caddy.service restarted (admin API is off — no live reload available)"
else
  sudo systemctl enable --now caddy
  log "caddy.service enabled and started"
fi

log "install-caddy.sh complete"
