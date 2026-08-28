#!/usr/bin/env bash
# Standing check for SRV-04 (default mode) and DIST-01 (--https mode): can
# this server actually be reached from outside the home network?
#
# A probe run from THIS Pi cannot answer that question on its own — it sits
# behind the same NAT as the server, so a local connect-back can succeed via
# hairpin NAT even when the outside world gets nothing, and this Pi's own
# /etc/hosts maps mc.campfire.pub to 127.0.0.1 (Phase 3 plan 01), which would
# make any check that trusts local name resolution pass regardless of
# whether the router forwards anything at all.
#
# Default mode (no args) — SRV-04: DNS convergence, then a third-party API
# (api.mcsrvstat.us) confirms TCP 25565 is reachable from outside. Exits 0
# (VERDICT: PASS) or 1 (VERDICT: FAIL). Unchanged from the original script.
#
# --https mode (DIST-01): three-way honest check for public HTTPS
# reachability on HTTPS_PORT. Checks the local path first (so a local fault
# is never misreported as a router fault), then forces the public path to
# the resolved public address with `curl --resolve` — this is what makes
# /etc/hosts unable to fake a pass, because --resolve overrides name
# resolution for that one request regardless of what /etc/hosts says.
#   exit 0  VERDICT: PASS         — public path returned a real manifest;
#                                   the port is forwarded and this router
#                                   does hairpin NAT.
#   exit 1  VERDICT: FAIL         — something on our side is wrong (local
#                                   front broken, TLS/certificate failure,
#                                   or a non-200 status once connected).
#   exit 2  VERDICT: INCONCLUSIVE — connection refused, reset, or timed out.
#                                   This Pi cannot tell whether that means
#                                   "not forwarded" or "this router doesn't
#                                   hairpin" — a human off the home network
#                                   must settle it (steps printed below).
#
# api.mcsrvstat.us caches responses for a few minutes, so a retry
# immediately after a router/DNS change can return a stale "offline"
# answer — the retry loop below tolerates that instead of failing hard on
# the first attempt.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  echo "Usage: $(basename "$0") [--https]" >&2
  echo "  (no args)  SRV-04 check: DNS convergence + third-party (api.mcsrvstat.us) TCP 25565 reachability" >&2
  echo "  --https    DIST-01 check: three-way public HTTPS reachability on HTTPS_PORT — PASS (0) / FAIL (1) / INCONCLUSIVE (2)" >&2
}

MODE="srv04"
case "${1-}" in
  "") MODE="srv04" ;;
  --https) MODE="https" ;;
  *) usage; exit 1 ;;
esac

# The --https acceptance test overrides HTTPS_PORT via the environment
# (`HTTPS_PORT=8555 bash scripts/reachability.sh --https`) to prove the
# public check cannot be fooled — but server.env's own unconditional
# `HTTPS_PORT=8444` assignment would otherwise silently clobber that
# override the instant it's sourced. Preserve a caller-supplied value.
_HTTPS_PORT_OVERRIDE="${HTTPS_PORT-}"
# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"
if [[ -n "$_HTTPS_PORT_OVERRIDE" ]]; then
  HTTPS_PORT="$_HTTPS_PORT_OVERRIDE"
fi

: "${DOMAIN:?DOMAIN not set in server.env}"
if [[ "$DOMAIN" == "mc.example.com" ]]; then
  echo "FATAL: DOMAIN is still the placeholder mc.example.com — set the real domain in server.env first" >&2
  exit 1
fi

# WR-06: fail fast with a clear message rather than letting a missing `dig`
# masquerade as "DNS did not converge" after the full retry budget.
if ! command -v dig >/dev/null 2>&1; then
  echo "FATAL: dig not found — install dnsutils (scripts/preflight.sh does this)" >&2
  exit 1
fi

RETRY_BUDGET_SEC=300
POLL_INTERVAL=15

resolve_domain() {
  dig +short "$DOMAIN" | tail -1
}

public_ip() {
  curl -s --max-time 10 ifconfig.me
}

# Shared by both modes: establishes that DOMAIN resolves to this
# connection's current public address, tolerating slow DDNS propagation.
# Sets RESOLVED/PUBLIC as side effects; returns 0 on convergence, 1 on
# timeout.
dns_converge() {
  echo "Checking DNS: does $DOMAIN resolve to this connection's current public IP?" >&2
  local deadline=$(( $(date +%s) + RETRY_BUDGET_SEC ))
  RESOLVED=""
  PUBLIC=""
  while :; do
    RESOLVED=$(resolve_domain)
    PUBLIC=$(public_ip)
    if [[ -n "$RESOLVED" && -n "$PUBLIC" && "$RESOLVED" == "$PUBLIC" ]]; then
      return 0
    fi
    if [[ "$(date +%s)" -ge "$deadline" ]]; then
      return 1
    fi
    echo "  waiting for DNS to converge (resolved='$RESOLVED', public='$PUBLIC') — retry in ${POLL_INTERVAL}s" >&2
    sleep "$POLL_INTERVAL"
  done
}

if [[ "$MODE" == "srv04" ]]; then
  if ! dns_converge; then
    echo "VERDICT: FAIL — DNS did not converge within ${RETRY_BUDGET_SEC}s (resolved='$RESOLVED' public='$PUBLIC')"
    exit 1
  fi
  echo "  DNS OK: $DOMAIN -> $RESOLVED" >&2

  echo "Checking outside-in reachability via api.mcsrvstat.us (third-party vantage point)..." >&2
  DEADLINE2=$(( $(date +%s) + RETRY_BUDGET_SEC ))
  ONLINE="false"
  VERSION="unknown"
  PLAYERS_ONLINE="0"
  while :; do
    API_JSON=$(curl -s --max-time 10 "https://api.mcsrvstat.us/2/${DOMAIN}")
    ONLINE=$(printf '%s' "$API_JSON" | jq -r '.online // false' 2>/dev/null || echo "false")
    if [[ "$ONLINE" == "true" ]]; then
      VERSION=$(printf '%s' "$API_JSON" | jq -r '.version // "unknown"')
      PLAYERS_ONLINE=$(printf '%s' "$API_JSON" | jq -r '.players.online // 0')
      break
    fi
    if [[ "$(date +%s)" -ge "$DEADLINE2" ]]; then
      echo "VERDICT: FAIL — api.mcsrvstat.us never reported online=true within ${RETRY_BUDGET_SEC}s (last response: $API_JSON)"
      exit 1
    fi
    echo "  api.mcsrvstat.us reports offline (possibly a stale cached answer) — retry in ${POLL_INTERVAL}s" >&2
    sleep "$POLL_INTERVAL"
  done

  echo "VERDICT: PASS — $DOMAIN resolves to $RESOLVED (matches public IP), api.mcsrvstat.us confirms online (version=$VERSION, players=$PLAYERS_ONLINE)"
  exit 0
fi

# --https mode below.

: "${CA_DIR:?CA_DIR not set in server.env}"
CACERT="$CA_DIR/campfire-ca.pem"
if [[ ! -f "$CACERT" ]]; then
  echo "VERDICT: FAIL — trust anchor not found at $CACERT"
  exit 1
fi

if ! dns_converge; then
  echo "VERDICT: FAIL — DNS did not converge within ${RETRY_BUDGET_SEC}s (resolved='$RESOLVED' public='$PUBLIC') — cannot determine the public address to test against"
  exit 1
fi
echo "  DNS OK: $DOMAIN -> $RESOLVED" >&2

# Local path first: this Pi's own /etc/hosts maps $DOMAIN to 127.0.0.1, so
# this request never leaves the box and never depends on the router. A
# failure here means Caddy or the manifest is broken — the public check is
# not attempted at all, so a local fault is never misreported as a router
# fault.
echo "Checking the local path (via /etc/hosts -> 127.0.0.1) before checking publicly..." >&2
MANIFEST_URL="https://${DOMAIN}:${HTTPS_PORT}/manifest.json"
LOCAL_OUT="$(mktemp)"
LOCAL_HTTP=$(curl -s --max-time 8 --cacert "$CACERT" -o "$LOCAL_OUT" -w '%{http_code}' "$MANIFEST_URL" 2>/dev/null)
LOCAL_RC=$?
if [[ "$LOCAL_RC" -ne 0 || "$LOCAL_HTTP" != "200" ]]; then
  echo "VERDICT: FAIL — local manifest request to $MANIFEST_URL failed (curl exit=$LOCAL_RC, http=${LOCAL_HTTP:-none}) — this is Caddy or the manifest, not the router; the public path was not checked"
  rm -f "$LOCAL_OUT"
  exit 1
fi
rm -f "$LOCAL_OUT"
echo "  Local OK: $MANIFEST_URL returned 200" >&2

# Public path: force the connection to the resolved public address with
# --resolve. This is the whole point of the mode — --resolve overrides name
# resolution for this one request, so this Pi's own /etc/hosts entry cannot
# turn a closed port into a passing check.
echo "Checking the public path: forcing the connection to the resolved public address ($RESOLVED) via curl --resolve..." >&2
PUBLIC_OUT="$(mktemp)"
PUBLIC_ERR="$(mktemp)"
PUBLIC_HTTP=$(curl -s --max-time 8 --resolve "${DOMAIN}:${HTTPS_PORT}:${RESOLVED}" --cacert "$CACERT" -o "$PUBLIC_OUT" -w '%{http_code}' "$MANIFEST_URL" 2>"$PUBLIC_ERR")
PUBLIC_RC=$?
PUBLIC_ERR_TEXT=$(cat "$PUBLIC_ERR")

LAN_ADDR=$(ip route get 1.1.1.1 2>/dev/null | grep -oP '(?<=src )\S+' | head -1)
PHONE_URL="$MANIFEST_URL"

if [[ "$PUBLIC_RC" -eq 0 && "$PUBLIC_HTTP" == "200" ]] && jq -e '.pack_version' "$PUBLIC_OUT" >/dev/null 2>&1; then
  echo "VERDICT: PASS — $MANIFEST_URL is reachable from this Pi via --resolve $RESOLVED — the port is forwarded and this router does hairpin NAT, which is proof of public reachability from the Pi itself"
  RC=0
elif [[ "$PUBLIC_RC" -eq 7 || "$PUBLIC_RC" -eq 28 || "$PUBLIC_RC" -eq 52 || "$PUBLIC_RC" -eq 56 ]]; then
  cat >&2 <<EOF
  Connection refused, reset, or timed out (curl exit=$PUBLIC_RC) reaching
  $MANIFEST_URL via --resolve $RESOLVED. This is NOT a failure verdict:
  this Pi sits behind the same NAT as the server, and many consumer routers
  simply do not hairpin a connection back to itself — a local result here
  cannot answer whether the forward exists. Settle it from a different
  network:
    1. On a phone, turn Wi-Fi OFF (use mobile data).
    2. Open this URL in the phone's browser: $PHONE_URL
    3. You will get a certificate warning first. That is CORRECT and
       expected — the CA is private and only the Phase 4 launcher pins it,
       so a warning followed by JSON is a PASS, not a problem. Tap through
       it.
    4. Expect a wall of JSON starting with "pack_version" after the
       warning.
  Or from any machine outside the home network:
    openssl s_client -connect ${DOMAIN}:${HTTPS_PORT} -servername ${DOMAIN} </dev/null
  and expect a certificate whose subject alternative name is ${DOMAIN}.
EOF
  echo "VERDICT: INCONCLUSIVE — connection refused/reset/timeout (curl exit=$PUBLIC_RC); this Pi cannot tell whether the forward is missing or this router just doesn't hairpin — see the phone-check steps above"
  RC=2
else
  echo "VERDICT: FAIL — the public path answered but something on our side is wrong (curl exit=$PUBLIC_RC, http=${PUBLIC_HTTP:-none}, curl error: ${PUBLIC_ERR_TEXT:-none}) — the connection got through, so this is a TLS/certificate/response problem, not a missing router forward"
  RC=1
fi
rm -f "$PUBLIC_OUT" "$PUBLIC_ERR"

cat >&2 <<EOF

============================================================
 For the operator — everything needed to forward the port
============================================================
 Pi LAN address (eth0):  ${LAN_ADDR:-unknown}
 Resolved public address: $RESOLVED
 Port to forward:         $HTTPS_PORT
 Protocol:                TCP
 Phone-check URL:         $PHONE_URL
============================================================
EOF

exit "$RC"
