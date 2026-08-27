#!/usr/bin/env bash
# scripts/cgnat-check.sh
#
# Two-stage CGNAT detection (D-07, D-16). Run before anything else in the
# phase — a CGNAT verdict of "detected" invalidates SRV-04 (no port forward
# can ever expose the server).
#
#   Stage 1 (always runs, no argument needed): is the public IP itself inside
#     the RFC 6598 shared address space (100.64.0.0/10)? If so, CGNAT is
#     definitively present.
#   Stage 2 (only when a router WAN IP is supplied as $1): does that IP fall
#     in the same range, or differ from the public IP the internet sees?
#
# Prints exactly one line: "CGNAT: detected|absent|unknown-needs-router-ip"
# Persists PUBLIC_IP_AT_SETUP and CGNAT_VERDICT into server.env.
# Never writes the public IP anywhere else (no hardcoding into configs).
#
# Exit codes: 0 = absent, 1 = detected, 2 = unknown-needs-router-ip

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$REPO_ROOT/server.env"
ROUTER_WAN_IP="${1:-}"

is_cgnat_range() {
  # RFC 6598 shared address space: 100.64.0.0/10 (100.64.0.0 - 100.127.255.255)
  [[ "$1" =~ ^100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\. ]]
}

set_env_var() {
  local key="$1" val="$2"
  local escaped="${val//\"/\\\"}"
  if [ -f "$ENV_FILE" ] && grep -q "^${key}=" "$ENV_FILE"; then
    sed -i "s|^${key}=.*|${key}=\"${escaped}\"|" "$ENV_FILE"
  elif [ -f "$ENV_FILE" ]; then
    printf '%s="%s"\n' "$key" "$escaped" >>"$ENV_FILE"
  fi
}

PUBLIC_IP="$(curl -s --max-time 10 ifconfig.me || true)"

VERDICT="unknown-needs-router-ip"
EXIT_CODE=2

if [ -n "$PUBLIC_IP" ] && is_cgnat_range "$PUBLIC_IP"; then
  VERDICT="detected"
  EXIT_CODE=1
elif [ -n "$ROUTER_WAN_IP" ]; then
  if is_cgnat_range "$ROUTER_WAN_IP" || [ "$ROUTER_WAN_IP" != "$PUBLIC_IP" ]; then
    VERDICT="detected"
    EXIT_CODE=1
  else
    VERDICT="absent"
    EXIT_CODE=0
  fi
fi

set_env_var PUBLIC_IP_AT_SETUP "$PUBLIC_IP"
set_env_var CGNAT_VERDICT "$VERDICT"

echo "CGNAT: $VERDICT"
exit $EXIT_CODE
