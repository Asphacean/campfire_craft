//! HTTP handlers: `/register`, `/login`, `/validate`, `/status`. Wired into
//! the `axum::Router` in `main.rs`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth;
use crate::db::{Db, InsertUserResult};
use crate::ratelimit::RateLimiter;

pub struct AppState {
    pub db: Db,
    /// D-04: 5 registrations/hour/peer, counting every attempt.
    pub register_limiter: RateLimiter,
    /// 10 *failed* login attempts/hour/peer — successes never count, so
    /// normal launcher use and testing are never throttled.
    pub login_limiter: RateLimiter,
    // Peer address comes from `ConnectInfo`, which is the direct TCP peer.
    // Once Phase 3 puts Caddy in front of this service every request will
    // arrive from 127.0.0.1 — Phase 3 must either keep these endpoints
    // direct or teach this limiter to read a forwarded-for header from a
    // trusted proxy. Repeated in auth-service/README.md for Phase 3/4.
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

#[derive(Serialize)]
pub struct StatusResponse {
    pub online: bool,
    pub players: Option<u32>,
}

pub async fn status() -> Json<StatusResponse> {
    // RESEARCH.md Open Question 2: ship a fixed placeholder for this phase,
    // no RCON call — a real player count is added only once Phase 3/4
    // actually need it.
    Json(StatusResponse {
        online: true,
        players: None,
    })
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    body: Result<Json<RegisterRequest>, JsonRejection>,
) -> Result<StatusCode, ApiError> {
    // D-04: 5 registrations/hour/peer, counting every attempt — checked
    // before validation or DB work so a flood cannot spend CPU past this
    // point.
    if !state.register_limiter.check(peer.ip()) {
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
    body: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Json<LoginResponse>, ApiError> {
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
    if !state.login_limiter.check(peer.ip()) {
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
    state.login_limiter.refund(peer.ip());

    let token = auth::generate_token();
    let token_hash = auth::hash_secret(&token).map_err(|_| ApiError::Internal)?;
    let expires = crate::db::now_unix() + TOKEN_TTL_SECS;
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
    // game server on loopback; throttling it would throttle joins.
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
