#!/usr/bin/env bash
# Self-contained API assertion suite for campfire-auth (D-01..D-05). Starts an
# ephemeral instance against a temp database on AUTH_BIND=127.0.0.1:8099, runs
# a fixed set of named PASS assertions, then tears everything down via an
# EXIT trap — repeatable an unlimited number of times, never touches the
# production database.
#
# Per-behavior tests that hit /register are spread across distinct loopback
# source addresses (127.0.0.2, .3, ... — the whole 127.0.0.0/8 block routes
# over `lo`, so `curl --interface 127.0.0.N` is a real distinct peer address
# as far as the rate limiter's `ConnectInfo` is concerned) so that
# functional-behavior assertions don't burn the same 5/hour quota the
# dedicated flood test (127.0.0.9) needs to exercise 429 cleanly.
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

# Poll /status (never rate limited, no side effects) for the port to answer,
# up to 10s.
READY=0
for _ in $(seq 1 50); do
  if curl -s -o /dev/null --max-time 1 "$BASE_URL/status"; then
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

# req IP PATH JSON_BODY — sets REQ_CODE and REQ_BODY.
req() {
  local resp
  resp=$(curl -s --interface "$1" -w '\n%{http_code}' -X POST -H 'content-type: application/json' -d "$3" "$BASE_URL$2")
  REQ_CODE="${resp##*$'\n'}"
  REQ_BODY="${resp%$'\n'*}"
}

extract_json_string() { # $1=body $2=field
  echo "$1" | grep -oE "\"$2\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" | sed -E 's/.*:[[:space:]]*"([^"]*)"$/\1/'
}
extract_json_number() { # $1=body $2=field
  echo "$1" | grep -oE "\"$2\"[[:space:]]*:[[:space:]]*[0-9]+" | grep -oE '[0-9]+$'
}

# ============================================================
# Happy path (Task 1): register, log in, spend the token once
# ============================================================
NICK="SmokeNick$$"
PASSWORD="smokepassword1"

req 127.0.0.1 /register "{\"nick\":\"$NICK\",\"password\":\"$PASSWORD\"}"
[[ "$REQ_CODE" == "201" ]] && pass "register a fresh nick returns 201" \
  || fail "register a fresh nick returns 201" "201" "$REQ_CODE"

req 127.0.0.1 /login "{\"nick\":\"$NICK\",\"password\":\"$PASSWORD\"}"
TOKEN=$(extract_json_string "$REQ_BODY" token)
EXPIRES=$(extract_json_number "$REQ_BODY" expires)
TOKEN_LEN=${#TOKEN}
NOW=$(date +%s)
MIN_EXPIRES=$((NOW + 11 * 3600))

[[ "$TOKEN_LEN" -ge 40 ]] && pass "login with the right password returns a token at least 40 chars long" \
  || fail "login token length >= 40" "true" "len=$TOKEN_LEN body=$REQ_BODY"

if [[ "$TOKEN" == *"+"* || "$TOKEN" == *"/"* || "$TOKEN" == *"="* ]]; then
  fail "login token is base64url without padding" "no +, / or =" "$TOKEN"
fi
pass "login token contains no +, / or = (base64url without padding)"

[[ -n "$EXPIRES" && "$EXPIRES" -ge "$MIN_EXPIRES" ]] && pass "login expires is at least 11 hours in the future" \
  || fail "login expires >= now+11h" ">= $MIN_EXPIRES" "$EXPIRES"

req 127.0.0.1 /validate "{\"nick\":\"$NICK\",\"token\":\"$TOKEN\"}"
[[ "$REQ_CODE" == "200" ]] && pass "validate that nick with that token returns 200" \
  || fail "validate returns 200" "200" "$REQ_CODE"

# ============================================================
# Task 2: rejections, limits, operator CLI
# ============================================================

# --- Duplicate nick, different case (409, hash unchanged) — 127.0.0.2 ---
DUPNICK="DupNick$$"
DUPNICK_LOWER=$(echo "$DUPNICK" | tr '[:upper:]' '[:lower:]')
req 127.0.0.2 /register "{\"nick\":\"$DUPNICK\",\"password\":\"duppassword1\"}"
[[ "$REQ_CODE" == "201" ]] || fail "duplicate-nick fixture registers cleanly" "201" "$REQ_CODE"
PW_HASH_BEFORE=$(sqlite3 "$DB_PATH" "select pw_hash from users where nick_lower='$DUPNICK_LOWER';")

# Same nick, all-lowercase (a different case than the mixed-case original),
# different password — must not overwrite anything.
req 127.0.0.2 /register "{\"nick\":\"$DUPNICK_LOWER\",\"password\":\"differentpassword\"}"
[[ "$REQ_CODE" == "409" ]] && pass "duplicate registration in a different case returns 409" \
  || fail "duplicate registration (case-insensitive) returns 409" "409" "$REQ_CODE"

PW_HASH_AFTER=$(sqlite3 "$DB_PATH" "select pw_hash from users where nick_lower='$DUPNICK_LOWER';")
[[ "$PW_HASH_BEFORE" == "$PW_HASH_AFTER" ]] && pass "the original account's stored hash is unchanged after a duplicate attempt" \
  || fail "pw_hash unchanged after duplicate registration" "$PW_HASH_BEFORE" "$PW_HASH_AFTER"

# --- Invalid nick pattern (400) — 127.0.0.2 ---
req 127.0.0.2 /register '{"nick":"xx","password":"validpassword1"}'
[[ "$REQ_CODE" == "400" ]] && pass "a nick failing the D-04 pattern returns 400" \
  || fail "invalid nick returns 400" "400" "$REQ_CODE"

# --- Weak password (400) — 127.0.0.2 ---
req 127.0.0.2 /register "{\"nick\":\"WeakPw$$\",\"password\":\"short1\"}"
[[ "$REQ_CODE" == "400" ]] && pass "a password shorter than 8 characters returns 400" \
  || fail "weak password returns 400" "400" "$REQ_CODE"

# --- Malformed JSON / missing field (400, not 500) — 127.0.0.3 ---
MALFORMED_CODE=$(curl -s --interface 127.0.0.3 -o /dev/null -w '%{http_code}' -X POST \
  -H 'content-type: application/json' -d '{not valid json' "$BASE_URL/register")
[[ "$MALFORMED_CODE" == "400" ]] && pass "a body that is not valid JSON returns 400" \
  || fail "malformed JSON returns 400" "400" "$MALFORMED_CODE"

req 127.0.0.3 /register '{"nick":"MissingField1"}'
[[ "$REQ_CODE" == "400" ]] && pass "a body missing a field returns 400" \
  || fail "missing field returns 400" "400" "$REQ_CODE"

# --- Wrong password / unknown nick (401, identical body, no token) — 127.0.0.1 ---
req 127.0.0.1 /login "{\"nick\":\"$NICK\",\"password\":\"wrongpassword\"}"
WRONG_PW_CODE="$REQ_CODE"
WRONG_PW_BODY="$REQ_BODY"
[[ "$WRONG_PW_CODE" == "401" ]] && pass "login with the wrong password returns 401" \
  || fail "wrong password returns 401" "401" "$WRONG_PW_CODE"
if echo "$WRONG_PW_BODY" | grep -q '"token"'; then
  fail "wrong-password response body contains no token field" "no token key" "$WRONG_PW_BODY"
fi
pass "wrong-password response body contains no token field"

req 127.0.0.1 /login "{\"nick\":\"NoSuchNick$$\",\"password\":\"whateverpassword\"}"
[[ "$REQ_CODE" == "401" ]] && pass "login for a never-registered nick returns 401" \
  || fail "unregistered nick login returns 401" "401" "$REQ_CODE"
[[ "$REQ_BODY" == "$WRONG_PW_BODY" ]] && pass "unknown-nick and wrong-password responses are indistinguishable" \
  || fail "unknown nick body == wrong password body" "$WRONG_PW_BODY" "$REQ_BODY"

# --- Token replay (401) — 127.0.0.1, reusing the already-consumed $TOKEN ---
req 127.0.0.1 /validate "{\"nick\":\"$NICK\",\"token\":\"$TOKEN\"}"
[[ "$REQ_CODE" == "401" ]] && pass "validating a token a second time returns 401" \
  || fail "second /validate of the same token returns 401" "401" "$REQ_CODE"

# --- Foreign-nick token (401) — 127.0.0.4 ---
req 127.0.0.4 /register '{"nick":"ForeignA","password":"foreignpassword1"}'
req 127.0.0.4 /register '{"nick":"ForeignB","password":"foreignpassword2"}'
req 127.0.0.4 /login '{"nick":"ForeignA","password":"foreignpassword1"}'
FOREIGN_TOKEN_A=$(extract_json_string "$REQ_BODY" token)
req 127.0.0.4 /validate "{\"nick\":\"ForeignB\",\"token\":\"$FOREIGN_TOKEN_A\"}"
[[ "$REQ_CODE" == "401" ]] && pass "validating a token that belongs to a different nick returns 401" \
  || fail "foreign-nick token returns 401" "401" "$REQ_CODE"

# --- Never-issued token (401) — 127.0.0.4 ---
req 127.0.0.4 /register '{"nick":"NeverIssued","password":"neverissuedpw1"}'
req 127.0.0.4 /validate '{"nick":"NeverIssued","token":"syntactically-valid-but-never-issued-token-value"}'
[[ "$REQ_CODE" == "401" ]] && pass "validating a syntactically valid but never-issued token returns 401" \
  || fail "never-issued token returns 401" "401" "$REQ_CODE"

# --- Expired token (401) — 127.0.0.5, expiry aged into the past via sqlite3 ---
req 127.0.0.5 /register '{"nick":"ExpiredNick","password":"expiredpassword1"}'
req 127.0.0.5 /login '{"nick":"ExpiredNick","password":"expiredpassword1"}'
EXPIRED_TOKEN=$(extract_json_string "$REQ_BODY" token)
sqlite3 "$DB_PATH" "UPDATE tokens SET expires_at = 1 WHERE user_id = (SELECT id FROM users WHERE nick_lower = 'expirednick');"
req 127.0.0.5 /validate "{\"nick\":\"ExpiredNick\",\"token\":\"$EXPIRED_TOKEN\"}"
[[ "$REQ_CODE" == "401" ]] && pass "validating a token whose row has aged past its expiry returns 401" \
  || fail "expired token returns 401" "401" "$REQ_CODE"

# --- Registration flood (429) vs. /validate never throttled — 127.0.0.9 ---
FLOOD_LAST_CODE=""
for i in 1 2 3 4 5 6; do
  req 127.0.0.9 /register "{\"nick\":\"FloodNick$i\",\"password\":\"floodpassword1\"}"
  FLOOD_LAST_CODE="$REQ_CODE"
  if [[ "$i" -le 5 ]]; then
    [[ "$REQ_CODE" == "201" ]] || fail "flood registration #$i (within quota) returns 201" "201" "$REQ_CODE"
  fi
done
[[ "$FLOOD_LAST_CODE" == "429" ]] && pass "a sixth registration attempt within the hour from the same peer returns 429" \
  || fail "sixth flood registration returns 429" "429" "$FLOOD_LAST_CODE"

req 127.0.0.9 /login '{"nick":"FloodNick1","password":"floodpassword1"}'
FLOOD_TOKEN=$(extract_json_string "$REQ_BODY" token)
req 127.0.0.9 /validate "{\"nick\":\"FloodNick1\",\"token\":\"$FLOOD_TOKEN\"}"
[[ "$REQ_CODE" == "200" ]] && pass "/validate for an already-registered account still returns 200 during a registration flood" \
  || fail "/validate unaffected by registration rate limit" "200" "$REQ_CODE"

# --- /status (200, online field) ---
STATUS_CODE=$(curl -s -o "$TMP_DIR/status.json" -w '%{http_code}' "$BASE_URL/status")
[[ "$STATUS_CODE" == "200" ]] && grep -q '"online"' "$TMP_DIR/status.json" && pass "GET /status returns 200 with an online field" \
  || fail "GET /status returns 200 with an online field" "200 + online" "$STATUS_CODE $(cat "$TMP_DIR/status.json")"

# --- Operator CLI: login (mint) and reset — direct DB access, no HTTP ---
CLI_TOKEN=$(AUTH_DB="$DB_PATH" "$BIN" login "$NICK")
req 127.0.0.1 /validate "{\"nick\":\"$NICK\",\"token\":\"$CLI_TOKEN\"}"
[[ "$REQ_CODE" == "200" ]] && pass "campfire-auth login <nick> prints a token /validate then accepts" \
  || fail "CLI-minted token validates" "200" "$REQ_CODE"

RESET_NICK="ResetNick$$"
req 127.0.0.6 /register "{\"nick\":\"$RESET_NICK\",\"password\":\"oldpassword1\"}"
printf 'newpassword1' | AUTH_DB="$DB_PATH" "$BIN" reset "$RESET_NICK" >/dev/null

req 127.0.0.6 /login "{\"nick\":\"$RESET_NICK\",\"password\":\"oldpassword1\"}"
[[ "$REQ_CODE" == "401" ]] && pass "after campfire-auth reset, the old password fails login with 401" \
  || fail "old password fails after reset" "401" "$REQ_CODE"

req 127.0.0.6 /login "{\"nick\":\"$RESET_NICK\",\"password\":\"newpassword1\"}"
[[ "$REQ_CODE" == "200" ]] && pass "after campfire-auth reset, the new password succeeds login with 200" \
  || fail "new password succeeds after reset" "200" "$REQ_CODE"

# --- At-rest secrecy: hashes only, never plaintext (T-02-01-02 / T-02-01-05) ---
BAD_HASH_COUNT=$(sqlite3 "$DB_PATH" "select count(*) from users where pw_hash NOT LIKE '\$argon2id\$%';")
[[ "$BAD_HASH_COUNT" == "0" ]] && pass "every row of users.pw_hash starts with \$argon2id\$" \
  || fail "all pw_hash rows are argon2id PHC strings" "0" "$BAD_HASH_COUNT"

PLAINTEXT_PW_COUNT=$(sqlite3 "$DB_PATH" 'select * from users;' | grep -cF "$PASSWORD" || true)
[[ "$PLAINTEXT_PW_COUNT" == "0" ]] && pass "the fixture password never appears in the users table" \
  || fail "fixture password absent from users table" "0" "$PLAINTEXT_PW_COUNT"

PLAINTEXT_TOKEN_COUNT=$(sqlite3 "$DB_PATH" 'select * from tokens;' | grep -cF "$TOKEN" || true)
[[ "$PLAINTEXT_TOKEN_COUNT" == "0" ]] && pass "the issued token never appears in the tokens table" \
  || fail "issued token absent from tokens table" "0" "$PLAINTEXT_TOKEN_COUNT"

echo "SMOKE OK ($CHECKS checks)"
