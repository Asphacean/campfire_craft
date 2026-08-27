#!/usr/bin/env bash
# systemd rlcraft.service ExecStart wrapper. Sources server.env (D-15), execs
# Temurin 8 directly (exec, so the JVM itself becomes the unit's MainPID) with
# Aikar's G1GC flags on the command line.
#
# Java 8 predates @argument-file support (that's a Forge-1.17+/Java-9+
# mechanism) — flags MUST stay on the command line, never in a
# user_jvm_args.txt @file passed to this JVM.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"

: "${JAVA8_BIN:?JAVA8_BIN not set in server.env}"
: "${SERVER_JAR:?SERVER_JAR not set in server.env — run scripts/install.sh first}"
: "${HEAP:?HEAP not set in server.env}"

if [[ ! -x "$JAVA8_BIN" ]]; then
  echo "FATAL: JAVA8_BIN ($JAVA8_BIN) is not executable" >&2
  exit 1
fi

JAVA_VERSION_STRING=$("$JAVA8_BIN" -version 2>&1 | head -1)
if [[ "$JAVA_VERSION_STRING" != *"1.8.0"* ]]; then
  echo "FATAL: JAVA8_BIN does not report 1.8.0 (got: $JAVA_VERSION_STRING) — refusing to start Forge 1.12.2 on the wrong JVM" >&2
  exit 1
fi

cd "$ROOT_DIR/server"

if [[ ! -f "$SERVER_JAR" ]]; then
  echo "FATAL: SERVER_JAR not found at server/$SERVER_JAR — run scripts/install.sh first" >&2
  exit 1
fi

# Aikar's flags (G1GC), sized for $HEAP — RESEARCH.md "Code Examples".
# -XX:+UnlockExperimentalVMOptions stays ahead of every experimental G1 flag.
exec "$JAVA8_BIN" \
  -Xms"${HEAP}" -Xmx"${HEAP}" \
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
  -XX:MaxTenuringThreshold=1 \
  -jar "$SERVER_JAR" nogui
