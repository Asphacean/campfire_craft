#!/usr/bin/env bash
# RCON forge-tps sampler for SRV-05. Samples `forge tps` and `list` every
# interval for the given duration, appends one CSV row per sample, and
# prints a min/median/mean Overall TPS summary with a PASS/FAIL verdict
# against the 15 TPS threshold (D-13).
#
# Usage: scripts/tps-log.sh [duration] [interval]
#   duration: total sampling window, <n>m or <n>s (default 20m)
#   interval: seconds between samples, <n>m or <n>s (default 30s)
#
# A single unparseable `forge tps` reply must not abort a 20-minute run —
# this script does NOT use `set -e`; a bad sample just logs an empty field.
#
# NOTE on this server's actual `forge tps` output: it does NOT match the
# newline-per-dimension format some docs assume — this Forge 1.12.2-2860
# build's RCON reply has no newline between the last `Dim` line and the
# `Overall` line (confirmed live: "...Mean TPS: 20.000Overall : Mean tick
# time: ... Mean TPS: 20.000"). The parser below matches `Overall` anywhere
# in the reply rather than assuming it starts a line.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"

: "${RCON_HOST:?RCON_HOST not set in server.env}"
: "${RCON_PORT:?RCON_PORT not set in server.env}"
: "${RCON_PASSWORD:?RCON_PASSWORD not set in server.env}"

rcon() {
  rcon-cli --host "$RCON_HOST" --port "$RCON_PORT" --password "$RCON_PASSWORD" "$@"
}

parse_duration_secs() {
  local v="$1"
  if [[ "$v" =~ ^([0-9]+)m$ ]]; then
    echo $(( ${BASH_REMATCH[1]} * 60 ))
  elif [[ "$v" =~ ^([0-9]+)s$ ]]; then
    echo "${BASH_REMATCH[1]}"
  elif [[ "$v" =~ ^[0-9]+$ ]]; then
    echo "$v"
  else
    echo "FATAL: cannot parse duration '$v' (expected <n>m or <n>s)" >&2
    exit 1
  fi
}

DURATION_ARG="${1:-20m}"
INTERVAL_ARG="${2:-30s}"
DURATION_SEC=$(parse_duration_secs "$DURATION_ARG")
INTERVAL_SEC=$(parse_duration_secs "$INTERVAL_ARG")

LOG_DIR="$ROOT_DIR/server/logs"
mkdir -p "$LOG_DIR"
CSV_PATH="$LOG_DIR/tps-$(date -u +%Y-%m-%d).csv"
[[ -f "$CSV_PATH" ]] || echo "timestamp,players,overall_tps" > "$CSV_PATH"

echo "Sampling forge tps every ${INTERVAL_SEC}s for ${DURATION_SEC}s -> $CSV_PATH" >&2

START=$(date +%s)
END=$(( START + DURATION_SEC ))
SAMPLES=0
MAX_PLAYERS=0
TPS_VALUES=()

while :; do
  # No 'Z' suffix on purpose — the timestamp field is grepped downstream
  # against a [0-9T:+-]+ charset with no 'Z'.
  NOW_TS="$(date -u +%Y-%m-%dT%H:%M:%S)+00:00"

  TPS_RAW=$(rcon "forge tps" 2>/dev/null || true)
  LIST_RAW=$(rcon "list" 2>/dev/null || true)

  OVERALL_TPS=$(printf '%s' "$TPS_RAW" | grep -oP 'Overall\s*:\s*Mean tick time:\s*[0-9.]+\s*ms\.\s*Mean TPS:\s*\K[0-9.]+' || true)
  PLAYERS=$(printf '%s' "$LIST_RAW" | grep -oP 'There are \K[0-9]+' || true)

  if [[ -n "$PLAYERS" && "$PLAYERS" -gt "$MAX_PLAYERS" ]]; then
    MAX_PLAYERS="$PLAYERS"
  fi

  echo "${NOW_TS},${PLAYERS},${OVERALL_TPS}" >> "$CSV_PATH"
  SAMPLES=$(( SAMPLES + 1 ))
  [[ -n "$OVERALL_TPS" ]] && TPS_VALUES+=("$OVERALL_TPS")

  NOW=$(date +%s)
  [[ "$NOW" -ge "$END" ]] && break
  REMAIN=$(( END - NOW ))
  SLEEP_FOR=$(( REMAIN < INTERVAL_SEC ? REMAIN : INTERVAL_SEC ))
  [[ "$SLEEP_FOR" -gt 0 ]] && sleep "$SLEEP_FOR"
done

echo "Sample count: $SAMPLES"
echo "Max players observed: $MAX_PLAYERS"

if [[ "${#TPS_VALUES[@]}" -eq 0 ]]; then
  echo "FAIL — no Overall TPS sample parsed successfully out of $SAMPLES samples (threshold: 15)"
  exit 1
fi

STATS=$(python3 - "${TPS_VALUES[@]}" <<'PYEOF'
import sys, statistics
vals = [float(v) for v in sys.argv[1:]]
print(min(vals), statistics.median(vals), sum(vals) / len(vals))
PYEOF
)
read -r MIN_TPS MEDIAN_TPS MEAN_TPS <<<"$STATS"

echo "Min Overall TPS: $MIN_TPS"
echo "Median Overall TPS: $MEDIAN_TPS"
echo "Mean Overall TPS: $MEAN_TPS"

THRESHOLD=15
if python3 -c "import sys; sys.exit(0 if float('$MEDIAN_TPS') >= $THRESHOLD else 1)"; then
  PASS_FAIL="PASS"
else
  PASS_FAIL="FAIL"
fi

EVIDENCE_NOTE=""
if [[ "$MAX_PLAYERS" -le 1 ]]; then
  EVIDENCE_NOTE=" (max players observed <= 1 — NOT evidence for SRV-05; a real 3-player run is required)"
fi

echo "${PASS_FAIL} — median Overall TPS ${MEDIAN_TPS} vs 15 TPS threshold, max players observed: ${MAX_PLAYERS}${EVIDENCE_NOTE}"

# PASS/FAIL is a measurement result, not a script error — the script itself
# exits 0 whenever it produced a verdict from at least one valid sample.
exit 0
