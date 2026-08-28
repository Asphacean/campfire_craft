//! HTTP handlers: `/register`, `/login`, `/validate`, `/status`. Wired into
//! the `axum::Router` in `main.rs`.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth;
use crate::db::{Db, InsertUserResult};
use crate::ratelimit::RateLimiter;

/// D-11: how long a `/status` result is reused before pinging again.
const STATUS_CACHE_TTL: Duration = Duration::from_secs(10);

pub struct AppState {
    pub db: Db,
    /// D-04: 5 registrations/hour/peer, counting every attempt.
    pub register_limiter: RateLimiter,
    /// 10 *failed* login attempts/hour/peer — successes never count, so
    /// normal launcher use and testing are never throttled.
    pub login_limiter: RateLimiter,
    /// WR-04: a much looser 60/hour/peer limiter counting *successful*
    /// logins — purely a circuit breaker against a credential holder
    /// hammering /login for no reason (each call is a real argon2id hash),
    /// not a security control against brute force (that's `login_limiter`
    /// above).
    pub login_success_limiter: RateLimiter,
    /// D-11: SLP ping target (`SLP_ADDR`, default 127.0.0.1:25565).
    pub slp_addr: String,
    /// D-11: the last `/status` result and when it was computed, reused for
    /// `STATUS_CACHE_TTL` instead of re-pinging on every launcher poll.
    pub status_cache: Mutex<Option<(Instant, StatusResponse)>>,
}

/// Phase 3 (T-03-01-07/T-03-01-08): resolves the address to charge against
/// the rate limiters. The service binds loopback only, so trusting a
/// forwarded-for header unconditionally would be a spoofing hole for any
/// caller that can reach it directly — this only trusts the header when the
/// direct TCP peer is loopback (i.e. genuinely came through Caddy on this
/// same host), and takes the *last* comma-separated element, which is
/// correct whether the edge replaced the header (current Caddyfile) or
/// appended to it (if that's ever changed later).
fn client_ip(peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
    if peer.ip().is_loopback() {
        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(candidate) = xff.rsplit(',').next() {
                if let Ok(ip) = candidate.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }
    peer.ip()
}

/// 12 hours, in seconds (D-03).
const TOKEN_TTL_SECS: i64 = 12 * 60 * 60;

/// D-04: nick pattern, checked as a character-class + length test rather
/// than pulling in a regex crate for one shape.
fn valid_nick(nick: &str) -> bool {
    let len = nick.chars().count();
    (3..=16).contains(&len) && nick.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// D-04: password minimum, counted as characters, not bytes.
fn valid_password(password: &str) -> bool {
    password.chars().count() >= 8
}

/// Stable, machine-readable error codes — part of the API contract Phase
/// 4's launcher reads to show a human message (`auth-service/README.md`).
#[derive(Clone, Copy)]
pub enum ApiError {
    NickTaken,
    InvalidNick,
    WeakPassword,
    BadJson,
    InvalidCredentials,
    InvalidToken,
    RateLimited,
    Internal,
}

impl ApiError {
    fn status(self) -> StatusCode {
        match self {
            ApiError::NickTaken => StatusCode::CONFLICT,
            ApiError::InvalidNick | ApiError::WeakPassword | ApiError::BadJson => {
                StatusCode::BAD_REQUEST
            }
            ApiError::InvalidCredentials | ApiError::InvalidToken => StatusCode::UNAUTHORIZED,
            ApiError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            ApiError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(self) -> &'static str {
        match self {
            ApiError::NickTaken => "nick_taken",
            ApiError::InvalidNick => "invalid_nick",
            ApiError::WeakPassword => "weak_password",
            ApiError::BadJson => "bad_json",
            ApiError::InvalidCredentials => "invalid_credentials",
            ApiError::InvalidToken => "invalid_token",
            ApiError::RateLimited => "rate_limited",
            ApiError::Internal => "internal_error",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status(), Json(json!({ "error": self.code() }))).into_response()
    }
}

impl From<JsonRejection> for ApiError {
    // axum's own JSON extractor rejection defaults vary by failure shape
    // (a syntax error is 400, but a missing/mistyped field is 422, and a
    // missing content-type header is 415) — confirmed by reading axum
    // 0.8.9's own json.rs tests directly, not assumed. This contract says
    // "malformed JSON and a body missing a field both return 400", so every
    // rejection variant is mapped to the same explicit 400 `bad_json`
    // rather than passed through.
    fn from(_: JsonRejection) -> Self {
        ApiError::BadJson
    }
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub nick: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub nick: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires: i64,
}

#[derive(Deserialize)]
pub struct ValidateRequest {
    pub nick: String,
    pub token: String,
}

#[derive(Serialize)]
pub struct ValidateResponse {
    pub nick: String,
}

#[derive(Serialize, Clone)]
pub struct StatusResponse {
    pub online: bool,
    pub players: Option<u32>,
    pub max: Option<u32>,
    pub motd: Option<String>,
}

/// D-11: real Server List Ping against `SLP_ADDR`, 10s cache. An
/// unreachable server, a timeout, a malformed body, or any other failure
/// all produce `online: false` with the other three fields null, returned
/// with HTTP 200 — never a 5xx, because "the game is off" is a normal
/// answer to this question and the launcher must be able to display it.
pub async fn status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    {
        let cache = state.status_cache.lock().expect("status cache mutex poisoned");
        if let Some((fetched_at, cached)) = cache.as_ref() {
            if fetched_at.elapsed() < STATUS_CACHE_TTL {
                return Json(cached.clone());
            }
        }
    }

    let fresh = match crate::slp::ping(&state.slp_addr).await {
        Some(result) => StatusResponse {
            online: true,
            players: Some(result.players_online),
            max: Some(result.players_max),
            motd: Some(result.motd),
        },
        None => StatusResponse {
            online: false,
            players: None,
            max: None,
            motd: None,
        },
    };

    let mut cache = state.status_cache.lock().expect("status cache mutex poisoned");
    *cache = Some((Instant::now(), fresh.clone()));
    Json(fresh)
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Result<Json<RegisterRequest>, JsonRejection>,
) -> Result<StatusCode, ApiError> {
    let limit_ip = client_ip(peer, &headers);

    // D-04: 5 registrations/hour/peer, counting every attempt — checked
    // before validation or DB work so a flood cannot spend CPU past this
    // point.
    if !state.register_limiter.check(limit_ip) {
        return Err(ApiError::RateLimited);
    }

    let Json(req) = body?;

    if !valid_nick(&req.nick) {
        return Err(ApiError::InvalidNick);
    }
    if !valid_password(&req.password) {
        return Err(ApiError::WeakPassword);
    }

    let nick_lower = req.nick.to_lowercase();
    let pw_hash = auth::hash_secret(&req.password).map_err(|_| ApiError::Internal)?;
    match state
        .db
        .insert_user(&req.nick, &nick_lower, &pw_hash)
        .map_err(|_| ApiError::Internal)?
    {
        InsertUserResult::Created => Ok(StatusCode::CREATED),
        InsertUserResult::NickTaken => Err(ApiError::NickTaken),
    }
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Json<LoginResponse>, ApiError> {
    let limit_ip = client_ip(peer, &headers);

    let Json(req) = body?;
    let nick_lower = req.nick.to_lowercase();

    // WR-01: reserve the failure-limiter slot atomically, in the same
    // single-critical-section `check()` `/register` uses, right before the
    // password check — refunded below on success. A separate peek
    // (would_allow) + record-after-the-fact (record_failure) split left a
    // check-then-record race: concurrent failed logins could all peek
    // "under limit" before any of them recorded. Reserving up front closes
    // that race; refunding on success is what still keeps a successful
    // login from counting against it.
    if !state.login_limiter.check(limit_ip) {
        return Err(ApiError::RateLimited);
    }

    let user = state
        .db
        .find_user_by_nick_lower(&nick_lower)
        .map_err(|_| ApiError::Internal)?;

    // Wrong password and unknown nick must cost the same and answer the
    // same way (T-02-01-01): verify against a fixed dummy hash for an
    // unknown nick rather than short-circuiting before argon2 runs, which
    // would make the two cases distinguishable by timing.
    let ok = match &user {
        Some(u) => auth::verify_secret(&req.password, &u.pw_hash),
        None => {
            auth::verify_secret(&req.password, auth::dummy_hash());
            false
        }
    };

    if !ok {
        return Err(ApiError::InvalidCredentials);
    }
    let user = user.expect("ok implies user was found");
    state.login_limiter.refund(limit_ip);

    // WR-04: a much looser limiter purely as a circuit breaker against
    // runaway automation from a caller who already knows a valid password
    // — this is separate from, and does not touch, `login_limiter` above.
    if !state.login_success_limiter.check(limit_ip) {
        return Err(ApiError::RateLimited);
    }

    let now = crate::db::now_unix();
    // WR-04: bound `tokens` table growth (and the /validate candidate-loop
    // cost that grows with it) opportunistically on each successful login,
    // rather than standing up a background task for a table this small.
    state.db.prune_tokens(now).map_err(|_| ApiError::Internal)?;

    let token = auth::generate_token();
    let token_hash = auth::hash_secret(&token).map_err(|_| ApiError::Internal)?;
    let expires = now + TOKEN_TTL_SECS;
    state
        .db
        .insert_token(user.id, &token_hash, expires)
        .map_err(|_| ApiError::Internal)?;

    Ok(Json(LoginResponse { token, expires }))
}

pub async fn validate(
    State(state): State<Arc<AppState>>,
    body: Result<Json<ValidateRequest>, JsonRejection>,
) -> Result<Json<ValidateResponse>, ApiError> {
    // Never rate limited — this is the join path and the caller is the
    // game server on loopback; throttling it would throttle joins. This
    // handler deliberately does not resolve/use `client_ip`: it is never
    // rate limited and never proxied through Caddy (D-04), so there is
    // nothing here for a forwarded-for header to matter to.
    let Json(req) = body?;
    let nick_lower = req.nick.to_lowercase();

    let user = state
        .db
        .find_user_by_nick_lower(&nick_lower)
        .map_err(|_| ApiError::Internal)?
        .ok_or(ApiError::InvalidToken)?;

    let now = crate::db::now_unix();
    let candidates = state
        .db
        .candidate_tokens(user.id, now)
        .map_err(|_| ApiError::Internal)?;

    for candidate in candidates {
        if auth::verify_secret(&req.token, &candidate.token_hash) {
            // Atomic compare-and-swap: `consumed_at IS NULL` in the WHERE
            // clause is the single-use enforcement point (T-02-01-04) — a
            // select-then-update without it is the replay hole this design
            // closes. If another request already consumed this exact row
            // (raced us), fall through rather than declaring victory on a
            // match we didn't actually win.
            let consumed = state
                .db
                .consume_token(candidate.id, now)
                .map_err(|_| ApiError::Internal)?;
            if consumed {
                return Ok(Json(ValidateResponse { nick: user.nick }));
            }
        }
    }

    Err(ApiError::InvalidToken)
}
