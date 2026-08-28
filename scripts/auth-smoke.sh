#!/usr/bin/env bash
# Self-contained API assertion suite for campfire-auth (D-01..D-05). Starts an
# ephemeral instance against a temp database on AUTH_BIND=127.0.0.1:8099, runs
# a fixed set of named PASS assertions, then tears everything down via an
# EXIT trap — repeatable an unlimited number of times, never touches the
# production database.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN="$ROOT_DIR/auth-service/target/release/campfire-auth"
SMOKE_BIND="127.0.0.1:8099"
BASE_URL="http://$SMOKE_BIND"

CHECKS=0
pass() {
  CHECKS=$((CHECKS + 1))
  echo "PASS: $1"
}
fail() {
  echo "FAIL: $1" >&2
  echo "  expected: $2" >&2
  echo "  actual:   $3" >&2
  exit 1
}

TMP_DIR=""
SERVER_PID=""
on_exit() {
  local ec=$?
  [[ -n "$SERVER_PID" ]] && kill "$SERVER_PID" >/dev/null 2>&1 || true
  [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]] && rm -rf "$TMP_DIR"
  exit "$ec"
}
trap on_exit EXIT

if [[ ! -x "$BIN" ]]; then
  echo "FATAL: $BIN not found or not executable — build first: cargo build --release --manifest-path auth-service/Cargo.toml" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
DB_PATH="$TMP_DIR/campfire.db"

AUTH_BIND="$SMOKE_BIND" AUTH_DB="$DB_PATH" "$BIN" serve >"$TMP_DIR/server.log" 2>&1 &
SERVER_PID=$!

# Poll for the port to answer, up to 10s.
READY=0
for _ in $(seq 1 50); do
  if curl -s -o /dev/null --max-time 1 "$BASE_URL/register" -X POST -H 'content-type: application/json' -d '{}'; then
    READY=1
    break
  fi
  sleep 0.2
done
if [[ "$READY" -ne 1 ]]; then
  echo "FATAL: campfire-auth did not come up on $SMOKE_BIND within 10s" >&2
  cat "$TMP_DIR/server.log" >&2 || true
  exit 1
fi

NICK="SmokeNick$$"
PASSWORD="smokepassword1"

# --- Register ---
CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' \
  -d "{\"nick\":\"$NICK\",\"password\":\"$PASSWORD\"}" "$BASE_URL/register")
[[ "$CODE" == "201" ]] && pass "register a fresh nick returns 201" \
  || fail "register a fresh nick returns 201" "201" "$CODE"

# --- Login ---
LOGIN_BODY=$(curl -s -X POST -H 'content-type: application/json' \
  -d "{\"nick\":\"$NICK\",\"password\":\"$PASSWORD\"}" "$BASE_URL/login")
TOKEN=$(echo "$LOGIN_BODY" | grep -oE '"token"[[:space:]]*:[[:space:]]*"[^"]+"' | sed -E 's/.*"([^"]+)"$/\1/')
EXPIRES=$(echo "$LOGIN_BODY" | grep -oE '"expires"[[:space:]]*:[[:space:]]*[0-9]+' | grep -oE '[0-9]+$')
TOKEN_LEN=${#TOKEN}
NOW=$(date +%s)
MIN_EXPIRES=$((NOW + 11 * 3600))

[[ "$TOKEN_LEN" -ge 40 ]] && pass "login with the right password returns a token at least 40 chars long" \
  || fail "login token length >= 40" "true" "len=$TOKEN_LEN body=$LOGIN_BODY"

if [[ "$TOKEN" == *"+"* || "$TOKEN" == *"/"* || "$TOKEN" == *"="* ]]; then
  fail "login token is base64url without padding" "no +, / or =" "$TOKEN"
fi
pass "login token contains no +, / or = (base64url without padding)"

[[ -n "$EXPIRES" && "$EXPIRES" -ge "$MIN_EXPIRES" ]] && pass "login expires is at least 11 hours in the future" \
  || fail "login expires >= now+11h" ">= $MIN_EXPIRES" "$EXPIRES"

# --- Validate ---
CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' \
  -d "{\"nick\":\"$NICK\",\"token\":\"$TOKEN\"}" "$BASE_URL/validate")
[[ "$CODE" == "200" ]] && pass "validate that nick with that token returns 200" \
  || fail "validate returns 200" "200" "$CODE"

echo "SMOKE OK ($CHECKS checks)"
