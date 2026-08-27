#!/usr/bin/env bash
# Standing check for SRV-04: does DOMAIN resolve to the current public IP,
# and does an outside-the-LAN vantage point confirm TCP 25565 is actually
# reachable?
#
# A probe run from THIS Pi cannot answer that second question on its own —
# it sits behind the same NAT as the server, so a local connect-back can
# succeed via hairpin NAT even when the outside world gets nothing. The
# authority for "reachable from the internet" here is a third-party API
# (api.mcsrvstat.us), not a local connect.
#
# api.mcsrvstat.us caches responses for a few minutes, so a retry
# immediately after a router/DNS change can return a stale "offline"
# answer — the retry loop below tolerates that instead of failing hard on
# the first attempt.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"

: "${DOMAIN:?DOMAIN not set in server.env}"
if [[ "$DOMAIN" == "mc.example.com" ]]; then
  echo "FATAL: DOMAIN is still the placeholder mc.example.com — set the real domain in server.env first" >&2
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

echo "Checking DNS: does $DOMAIN resolve to this connection's current public IP?" >&2
DEADLINE=$(( $(date +%s) + RETRY_BUDGET_SEC ))
RESOLVED=""
PUBLIC=""
while :; do
  RESOLVED=$(resolve_domain)
  PUBLIC=$(public_ip)
  if [[ -n "$RESOLVED" && -n "$PUBLIC" && "$RESOLVED" == "$PUBLIC" ]]; then
    break
  fi
  if [[ "$(date +%s)" -ge "$DEADLINE" ]]; then
    echo "VERDICT: FAIL — DNS did not converge within ${RETRY_BUDGET_SEC}s (resolved='$RESOLVED' public='$PUBLIC')"
    exit 1
  fi
  echo "  waiting for DNS to converge (resolved='$RESOLVED', public='$PUBLIC') — retry in ${POLL_INTERVAL}s" >&2
  sleep "$POLL_INTERVAL"
done
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
