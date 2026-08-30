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
OFFLINE_PID=""
on_exit() {
  local ec=$?
  [[ -n "$SERVER_PID" ]] && kill "$SERVER_PID" >/dev/null 2>&1 || true
  [[ -n "$OFFLINE_PID" ]] && kill "$OFFLINE_PID" >/dev/null 2>&1 || true
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

# --- /status: real Server List Ping (D-11) ---
# The primary smoke instance was started with no SLP_ADDR override, so it
# defaults to 127.0.0.1:25565 — the live rlcraft.service on this host — and
# exercises the online branch. Read-only SLP query; never stops/restarts it.
STATUS_BODY=$(curl -s "$BASE_URL/status")
STATUS_CODE=$(curl -s -o /dev/null -w '%{http_code}' "$BASE_URL/status")
[[ "$STATUS_CODE" == "200" ]] && pass "GET /status against the live game server returns 200" \
  || fail "GET /status live returns 200" "200" "$STATUS_CODE"

echo "$STATUS_BODY" | jq -e '.online==true and (.players|type=="number") and (.max|type=="number") and (.motd|type=="string") and (keys|length==4)' >/dev/null \
  && pass "live /status has exactly 4 keys: online true, numeric players/max, string motd (no Forge mod list)" \
  || fail "live /status shape (4 keys, online true, numeric players/max, string motd)" "true" "$STATUS_BODY"

STATUS_LEN=$(printf '%s' "$STATUS_BODY" | wc -c)
[[ "$STATUS_LEN" -lt 512 ]] && pass "live /status body is under 512 bytes" \
  || fail "live /status body under 512 bytes" "<512" "$STATUS_LEN"

STATUS_A=$(curl -s "$BASE_URL/status")
STATUS_B=$(curl -s "$BASE_URL/status")
[[ "$STATUS_A" == "$STATUS_B" ]] && pass "two /status calls in quick succession are byte-identical (10s cache)" \
  || fail "two quick /status calls are byte-identical" "$STATUS_A" "$STATUS_B"

# --- /status offline branch: a second ephemeral instance with SLP_ADDR
# pointed at a port nothing listens on, sharing the same temp database. ---
OFFLINE_BIND="127.0.0.1:8098"
OFFLINE_BASE="http://$OFFLINE_BIND"
AUTH_BIND="$OFFLINE_BIND" AUTH_DB="$DB_PATH" SLP_ADDR="127.0.0.1:25599" "$BIN" serve >"$TMP_DIR/server-offline.log" 2>&1 &
OFFLINE_PID=$!

OFFLINE_READY=0
for _ in $(seq 1 50); do
  if curl -s -o /dev/null --max-time 1 "$OFFLINE_BASE/status"; then
    OFFLINE_READY=1
    break
  fi
  sleep 0.2
done
if [[ "$OFFLINE_READY" -ne 1 ]]; then
  echo "FATAL: offline-SLP campfire-auth instance did not come up on $OFFLINE_BIND within 10s" >&2
  cat "$TMP_DIR/server-offline.log" >&2 || true
  exit 1
fi

OFFLINE_STATUS=$(curl -s "$OFFLINE_BASE/status")
OFFLINE_CODE=$(curl -s -o /dev/null -w '%{http_code}' "$OFFLINE_BASE/status")
[[ "$OFFLINE_CODE" == "200" ]] && pass "GET /status with SLP_ADDR pointed at a dead port still returns 200" \
  || fail "offline /status returns 200" "200" "$OFFLINE_CODE"

echo "$OFFLINE_STATUS" | jq -e '.online==false and .players==null and .max==null and .motd==null' >/dev/null \
  && pass "offline /status: online false, players/max/motd all null" \
  || fail "offline /status shape (online false, players/max/motd null)" "true" "$OFFLINE_STATUS"

kill "$OFFLINE_PID" >/dev/null 2>&1 || true
wait "$OFFLINE_PID" 2>/dev/null || true
OFFLINE_PID=""

# --- Rate limiter forwarded-for handling (T-03-01-07/T-03-01-08). The
# service trusts X-Forwarded-For only when the direct TCP peer is loopback
# (client_ip() in api.rs) — which every peer is in this smoke suite, same as
# every request Caddy forwards in production. This proves the service's own
# half of the contract directly; the full Caddy-fronted proof (Caddy SETS
# rather than appends the header at the edge, so a client-supplied value
# never reaches here at all) is a separate acceptance check against the live
# deployment, not reproducible without Caddy in front. ---
for i in 1 2 3 4 5; do
  req 127.0.0.7 /register "{\"nick\":\"XffBucket$i\",\"password\":\"xffpassword1\"}"
done
[[ "$REQ_CODE" == "201" ]] || fail "XFF-bucket fixture: 5th registration from 127.0.0.7 (within quota) returns 201" "201" "$REQ_CODE"
XFF_EXHAUSTED_CODE=$(curl -s --interface 127.0.0.7 -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' -d '{"nick":"XffBucket6","password":"xffpassword1"}' "$BASE_URL/register")
[[ "$XFF_EXHAUSTED_CODE" == "429" ]] || fail "127.0.0.7's own bucket is exhausted after 5 registrations" "429" "$XFF_EXHAUSTED_CODE"

# A fresh, never-before-used peer (127.0.0.10) presenting an
# X-Forwarded-For naming the already-exhausted 127.0.0.7 is limited under
# that named address, not its own untouched one.
SPOOFED_CODE=$(curl -s --interface 127.0.0.10 -H 'X-Forwarded-For: 127.0.0.7' -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' -d '{"nick":"XffBucket7","password":"xffpassword1"}' "$BASE_URL/register")
[[ "$SPOOFED_CODE" == "429" ]] && pass "a loopback peer's request is rate-limited under its forwarded-for header's address, not its own untouched peer address" \
  || fail "request keyed on forwarded-for header value" "429" "$SPOOFED_CODE"

# The same fresh peer without the header uses its own untouched budget.
REAL_CODE=$(curl -s --interface 127.0.0.10 -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' -d '{"nick":"XffBucket8","password":"xffpassword1"}' "$BASE_URL/register")
[[ "$REAL_CODE" == "201" ]] && pass "the same peer without a forwarded-for header uses its own untouched budget" \
  || fail "peer without XFF header has its own budget" "201" "$REAL_CODE"

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

# ============================================================
# Phase 4 (D-17/AUTH-03): refresh tokens — 127.0.0.11..14, distinct
# loopback sources so these don't spend the flood test's quota.
# ============================================================

# --- happy path: login returns a refresh, /refresh rotates it — 127.0.0.11 ---
req 127.0.0.11 /login "{\"nick\":\"$NICK\",\"password\":\"$PASSWORD\"}"
REFRESH1=$(extract_json_string "$REQ_BODY" refresh)
REFRESH1_LEN=${#REFRESH1}
[[ "$REFRESH1_LEN" -ge 40 ]] && pass "login also returns a refresh token at least 40 chars long" \
  || fail "login refresh length >= 40" "true" "len=$REFRESH1_LEN body=$REQ_BODY"

req 127.0.0.11 /refresh "{\"nick\":\"$NICK\",\"refresh\":\"$REFRESH1\"}"
REFRESH_GAME_TOKEN=$(extract_json_string "$REQ_BODY" token)
REFRESH2=$(extract_json_string "$REQ_BODY" refresh)
[[ "$REQ_CODE" == "200" ]] && pass "refresh with a live refresh token returns 200" \
  || fail "refresh returns 200" "200" "$REQ_CODE"
[[ -n "$REFRESH_GAME_TOKEN" && ${#REFRESH_GAME_TOKEN} -ge 40 ]] && pass "refresh returns a fresh game token at least 40 chars long" \
  || fail "refresh game token length >= 40" "true" "$REQ_BODY"
[[ -n "$REFRESH2" && "$REFRESH2" != "$REFRESH1" ]] && pass "refresh rotates: the new refresh value differs from the presented one" \
  || fail "refresh rotates to a new value" "different from $REFRESH1" "$REFRESH2"

# --- rotation is real: replaying the original refresh token dies — 127.0.0.11 ---
req 127.0.0.11 /refresh "{\"nick\":\"$NICK\",\"refresh\":\"$REFRESH1\"}"
[[ "$REQ_CODE" == "401" ]] && pass "replaying the original (now-rotated) refresh token returns 401" \
  || fail "replayed refresh token returns 401" "401" "$REQ_CODE"
echo "$REQ_BODY" | grep -q '"invalid_token"' && pass "replayed refresh token error code is invalid_token" \
  || fail "replayed refresh error code" "invalid_token" "$REQ_BODY"

# --- the game token refresh minted is single-use, same as any other — 127.0.0.11 ---
req 127.0.0.11 /validate "{\"nick\":\"$NICK\",\"token\":\"$REFRESH_GAME_TOKEN\"}"
[[ "$REQ_CODE" == "200" ]] && pass "the game token minted by refresh validates once" \
  || fail "refresh-minted game token validates" "200" "$REQ_CODE"
req 127.0.0.11 /validate "{\"nick\":\"$NICK\",\"token\":\"$REFRESH_GAME_TOKEN\"}"
[[ "$REQ_CODE" == "401" ]] && pass "the game token minted by refresh is single-use, same as any other" \
  || fail "refresh-minted game token replay returns 401" "401" "$REQ_CODE"

# --- foreign-nick refresh (401) — 127.0.0.12, reusing the ForeignA/B fixture ---
req 127.0.0.12 /login '{"nick":"ForeignA","password":"foreignpassword1"}'
FOREIGN_REFRESH_A=$(extract_json_string "$REQ_BODY" refresh)
req 127.0.0.12 /refresh "{\"nick\":\"ForeignB\",\"refresh\":\"$FOREIGN_REFRESH_A\"}"
[[ "$REQ_CODE" == "401" ]] && pass "refreshing with a token that belongs to a different nick returns 401" \
  || fail "foreign-nick refresh returns 401" "401" "$REQ_CODE"

# --- expired refresh token (401) — 127.0.0.13, expiry aged into the past ---
req 127.0.0.13 /register '{"nick":"ExpRefreshNick","password":"expiredrefresh1"}'
req 127.0.0.13 /login '{"nick":"ExpRefreshNick","password":"expiredrefresh1"}'
EXPIRED_REFRESH=$(extract_json_string "$REQ_BODY" refresh)
sqlite3 "$DB_PATH" "UPDATE refresh_tokens SET expires_at = 1 WHERE user_id = (SELECT id FROM users WHERE nick_lower = 'exprefreshnick');"
req 127.0.0.13 /refresh "{\"nick\":\"ExpRefreshNick\",\"refresh\":\"$EXPIRED_REFRESH\"}"
[[ "$REQ_CODE" == "401" ]] && pass "refreshing with a token whose row has aged past its expiry returns 401" \
  || fail "expired refresh token returns 401" "401" "$REQ_CODE"

# --- campfire-auth reset revokes remembered sessions — 127.0.0.14 ---
req 127.0.0.14 /register '{"nick":"ResetRefreshNick","password":"resetrefreshpw1"}'
req 127.0.0.14 /login '{"nick":"ResetRefreshNick","password":"resetrefreshpw1"}'
RESET_REFRESH=$(extract_json_string "$REQ_BODY" refresh)
printf 'newresetrefreshpw1' | AUTH_DB="$DB_PATH" "$BIN" reset "ResetRefreshNick" >/dev/null
req 127.0.0.14 /refresh "{\"nick\":\"ResetRefreshNick\",\"refresh\":\"$RESET_REFRESH\"}"
[[ "$REQ_CODE" == "401" ]] && pass "campfire-auth reset leaves an outstanding refresh token unusable" \
  || fail "refresh token dead after reset" "401" "$REQ_CODE"

# ============================================================
# WR-04: /logout revokes without reissuing — 127.0.0.15
# ============================================================
req 127.0.0.15 /login "{\"nick\":\"$NICK\",\"password\":\"$PASSWORD\"}"
LOGOUT_REFRESH=$(extract_json_string "$REQ_BODY" refresh)

req 127.0.0.15 /logout "{\"nick\":\"$NICK\",\"refresh\":\"$LOGOUT_REFRESH\"}"
[[ "$REQ_CODE" == "204" ]] && pass "logout with a live refresh token returns 204" \
  || fail "logout returns 204" "204" "$REQ_CODE"

req 127.0.0.15 /refresh "{\"nick\":\"$NICK\",\"refresh\":\"$LOGOUT_REFRESH\"}"
[[ "$REQ_CODE" == "401" ]] && pass "a refresh token revoked by /logout can no longer be used to /refresh" \
  || fail "logged-out refresh token dies for /refresh" "401" "$REQ_CODE"

req 127.0.0.15 /logout "{\"nick\":\"$NICK\",\"refresh\":\"$LOGOUT_REFRESH\"}"
[[ "$REQ_CODE" == "401" ]] && pass "logging out an already-revoked refresh token returns 401" \
  || fail "already-revoked logout returns 401" "401" "$REQ_CODE"

req 127.0.0.15 /logout '{"nick":"NoSuchLogoutNick","refresh":"whatever-not-a-real-token"}'
[[ "$REQ_CODE" == "401" ]] && pass "logout for an unknown nick returns 401" \
  || fail "unknown-nick logout returns 401" "401" "$REQ_CODE"

# --- At-rest secrecy: hashes only, never plaintext (T-02-01-02 / T-02-01-05) ---
BAD_HASH_COUNT=$(sqlite3 "$DB_PATH" "select count(*) from users where pw_hash NOT LIKE '\$argon2id\$%';")
[[ "$BAD_HASH_COUNT" == "0" ]] && pass "every row of users.pw_hash starts with \$argon2id\$" \
  || fail "all pw_hash rows are argon2id PHC strings" "0" "$BAD_HASH_COUNT"

PLAINTEXT_PW_COUNT=$(sqlite3 "$DB_PATH" 'select * from users;' | grep -cF -- "$PASSWORD" || true)
[[ "$PLAINTEXT_PW_COUNT" == "0" ]] && pass "the fixture password never appears in the users table" \
  || fail "fixture password absent from users table" "0" "$PLAINTEXT_PW_COUNT"

PLAINTEXT_TOKEN_COUNT=$(sqlite3 "$DB_PATH" 'select * from tokens;' | grep -cF -- "$TOKEN" || true)
[[ "$PLAINTEXT_TOKEN_COUNT" == "0" ]] && pass "the issued token never appears in the tokens table" \
  || fail "issued token absent from tokens table" "0" "$PLAINTEXT_TOKEN_COUNT"

BAD_REFRESH_HASH_COUNT=$(sqlite3 "$DB_PATH" "select count(*) from refresh_tokens where token_hash NOT LIKE '\$argon2id\$%';")
[[ "$BAD_REFRESH_HASH_COUNT" == "0" ]] && pass "every row of refresh_tokens.token_hash starts with \$argon2id\$" \
  || fail "all refresh_tokens rows are argon2id PHC strings" "0" "$BAD_REFRESH_HASH_COUNT"

PLAINTEXT_REFRESH_COUNT=$(sqlite3 "$DB_PATH" 'select * from refresh_tokens;' | grep -cF -- "$REFRESH1" || true)
[[ "$PLAINTEXT_REFRESH_COUNT" == "0" ]] && pass "the issued refresh token never appears in the refresh_tokens table" \
  || fail "issued refresh token absent from refresh_tokens table" "0" "$PLAINTEXT_REFRESH_COUNT"

echo "SMOKE OK ($CHECKS checks)"
