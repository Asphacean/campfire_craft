#!/usr/bin/env bash
# Disposable, loopback-only Forge 1.12.2 server carrying only campfire-auth,
# for proving the auth gate without ever touching rlcraft.service or its
# world. `devserver/` is fully gitignored and safe to delete at any time.
#
# start: assembles devserver/ (symlinks to the existing server/libraries and
# the two jars — those are hundreds of megabytes and this directory is
# disposable), drops in the freshly built campfire-auth jar as the only
# mod, writes a loopback-only server.properties (D-16), launches Temurin 8
# with a 1GB heap, and waits for the startup-complete line.
# stop: kills the recorded PID and waits for the port to close.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DEV_DIR="$ROOT_DIR/devserver"
PID_FILE="$DEV_DIR/server.pid"
LOG_FILE="$DEV_DIR/server.log"
PORT=25566
STARTUP_TIMEOUT_SECONDS=180

# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"

: "${JAVA8_BIN:?JAVA8_BIN not set in server.env}"
: "${SERVER_JAR:?SERVER_JAR not set in server.env}"

usage() {
  echo "Usage: $0 {start|stop}" >&2
  exit 1
}

cmd_start() {
  if [[ -f "$PID_FILE" ]] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
    echo "FATAL: devserver already running (pid $(cat "$PID_FILE"))" >&2
    exit 1
  fi

  local mod_jar="$ROOT_DIR/mods-src/campfire-auth/build/libs/campfire-auth-0.1.0.jar"
  if [[ ! -f "$mod_jar" ]]; then
    echo "FATAL: $mod_jar not found — run mods-src/campfire-auth/build.sh build first" >&2
    exit 1
  fi

  rm -rf "$DEV_DIR"
  mkdir -p "$DEV_DIR/mods"

  ln -s "$ROOT_DIR/server/libraries" "$DEV_DIR/libraries"
  ln -s "$ROOT_DIR/server/minecraft_server.1.12.2.jar" "$DEV_DIR/minecraft_server.1.12.2.jar"
  ln -s "$ROOT_DIR/server/$SERVER_JAR" "$DEV_DIR/$SERVER_JAR"
  cp "$mod_jar" "$DEV_DIR/mods/campfire-auth-0.1.0.jar"

  cat > "$DEV_DIR/eula.txt" <<EOF
eula=true
EOF

  cat > "$DEV_DIR/server.properties" <<EOF
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
network-compression-threshold=-1
level-type=FLAT
spawn-protection=0
max-players=4
motd=campfire-auth devserver (throwaway)
EOF

  (
    cd "$DEV_DIR"
    exec "$JAVA8_BIN" -Xms1G -Xmx1G -jar "$SERVER_JAR" nogui
  ) > "$LOG_FILE" 2>&1 &
  local pid=$!
  echo "$pid" > "$PID_FILE"
  echo "devserver starting (pid $pid), waiting up to ${STARTUP_TIMEOUT_SECONDS}s for startup..."

  local waited=0
  while (( waited < STARTUP_TIMEOUT_SECONDS )); do
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "FATAL: devserver process exited during startup — see $LOG_FILE" >&2
      tail -n 40 "$LOG_FILE" >&2 || true
      rm -f "$PID_FILE"
      exit 1
    fi
    if grep -qE '\]: Done \(' "$LOG_FILE" 2>/dev/null; then
      echo "devserver up on 127.0.0.1:$PORT (pid $pid)"
      return 0
    fi
    sleep 2
    waited=$((waited + 2))
  done

  echo "FATAL: devserver did not report startup-complete within ${STARTUP_TIMEOUT_SECONDS}s — see $LOG_FILE" >&2
  exit 1
}

cmd_stop() {
  if [[ ! -f "$PID_FILE" ]]; then
    echo "devserver not running (no pid file)"
    return 0
  fi
  local pid
  pid="$(cat "$PID_FILE")"
  if kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    local waited=0
    while kill -0 "$pid" 2>/dev/null && (( waited < 30 )); do
      sleep 1
      waited=$((waited + 1))
    done
    if kill -0 "$pid" 2>/dev/null; then
      echo "devserver did not stop gracefully, sending SIGKILL" >&2
      kill -9 "$pid" 2>/dev/null || true
      sleep 1
    fi
  fi
  rm -f "$PID_FILE"

  local port_wait=0
  while ss -ltn 2>/dev/null | grep -q "127.0.0.1:$PORT" && (( port_wait < 15 )); do
    sleep 1
    port_wait=$((port_wait + 1))
  done
  echo "devserver stopped"
}

case "${1:-}" in
  start) cmd_start ;;
  stop) cmd_stop ;;
  *) usage ;;
esac
