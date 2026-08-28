#!/usr/bin/env bash
# The only supported way to invoke this project's Gradle build. Pins
# JAVA_HOME to the Temurin 8 install this repo already uses for the game
# server (server.env's JAVA8_BIN) and refuses to run on any other JVM —
# Gradle 4.10.3 (paired with ForgeGradle 2.3) cannot start its daemon on the
# host's system Java 25 (RESEARCH.md Pitfall 2).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

# shellcheck source=/dev/null
source "$ROOT_DIR/server.env"

: "${JAVA8_BIN:?JAVA8_BIN not set in server.env}"

if [[ ! -x "$JAVA8_BIN" ]]; then
  echo "FATAL: JAVA8_BIN ($JAVA8_BIN) is not executable" >&2
  exit 1
fi

JAVA_VERSION_STRING=$("$JAVA8_BIN" -version 2>&1 | head -1)
if [[ "$JAVA_VERSION_STRING" != *"1.8.0"* ]]; then
  echo "FATAL: JAVA8_BIN does not report 1.8.0 (got: $JAVA_VERSION_STRING) — refusing to run Gradle 4.10.3/ForgeGradle 2.3 on the wrong JVM" >&2
  exit 1
fi

export JAVA_HOME
JAVA_HOME="$(dirname "$(dirname "$JAVA8_BIN")")"

cd "$SCRIPT_DIR"
exec ./gradlew "$@"
