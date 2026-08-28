//! HTTP handlers: `/register`, `/login`, `/validate`. Wired into the
//! `axum::Router` in `main.rs`.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::db::{Db, InsertUserResult};

pub struct AppState {
    pub db: Db,
}

/// 12 hours, in seconds (D-03).
const TOKEN_TTL_SECS: i64 = 12 * 60 * 60;

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

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> StatusCode {
    let nick_lower = req.nick.to_lowercase();
    let pw_hash = match auth::hash_secret(&req.password) {
        Ok(h) => h,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };
    match state.db.insert_user(&req.nick, &nick_lower, &pw_hash) {
        Ok(InsertUserResult::Created) => StatusCode::CREATED,
        Ok(InsertUserResult::NickTaken) => StatusCode::CONFLICT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let nick_lower = req.nick.to_lowercase();
    let user = state
        .db
        .find_user_by_nick_lower(&nick_lower)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !auth::verify_secret(&req.password, &user.pw_hash) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let token = auth::generate_token();
    let token_hash = auth::hash_secret(&token).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let expires = crate::db::now_unix() + TOKEN_TTL_SECS;
    state
        .db
        .insert_token(user.id, &token_hash, expires)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(LoginResponse { token, expires }))
}

pub async fn validate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, StatusCode> {
    let nick_lower = req.nick.to_lowercase();
    let user = state
        .db
        .find_user_by_nick_lower(&nick_lower)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let now = crate::db::now_unix();
    let candidates = state
        .db
        .candidate_tokens(user.id, now)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for candidate in candidates {
        if auth::verify_secret(&req.token, &candidate.token_hash) {
            // Atomic compare-and-swap: `consumed_at IS NULL` in the WHERE
            // clause makes this the single-use enforcement point
            // (T-02-01-04) — a select-then-update without it is the replay
            // hole this design closes. If another request already consumed
            // this exact row (raced us), fall through to the next candidate
            // rather than declaring victory on a match we didn't actually
            // win.
            let consumed = state
                .db
                .consume_token(candidate.id, now)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if consumed {
                return Ok(Json(ValidateResponse { nick: user.nick }));
            }
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}
