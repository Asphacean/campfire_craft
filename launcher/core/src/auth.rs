//! The session: register/login/refresh against the live `campfire-auth`
//! service, plus OS-credential-store round-tripping of the refresh token.
//! **The password is a parameter and a request body and nothing else** — no
//! struct field here ever holds one past the single call that needs it, it
//! is never logged, and it is never sent anywhere except `/register` and
//! `/login`.

use serde::{Deserialize, Serialize};

use crate::http::{campfire_client, CAMPFIRE_BASE_URL};
use crate::log;

/// The credential-store service name every `keyring::Entry` in this
/// launcher uses — matches `tauri.conf.json`'s `identifier`.
const KEYRING_SERVICE: &str = "pub.campfire.launcher";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Exact registration casing, never player-retyped input and never the
    /// lowercased uniqueness key — DIST-OPS.md's "Nick casing" contract:
    /// the offline UUID is derived from these exact bytes.
    pub nick: String,
    pub token: String,
    pub expires: i64,
}

/// Lines up with the service's stable error codes
/// (`auth-service/README.md`), plus two client-only variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    NickTaken,
    InvalidNick,
    WeakPassword,
    InvalidCredentials,
    InvalidToken,
    RateLimited,
    /// Any transport-level failure: timeout, DNS, TLS, connection refused.
    Network,
    /// No refresh token found in the credential store (cold start, never
    /// logged in, or already logged out).
    NoStoredSession,
}

impl AuthError {
    fn from_code(code: &str) -> Self {
        match code {
            "nick_taken" => AuthError::NickTaken,
            "invalid_nick" => AuthError::InvalidNick,
            "weak_password" => AuthError::WeakPassword,
            "invalid_credentials" => AuthError::InvalidCredentials,
            "invalid_token" => AuthError::InvalidToken,
            "rate_limited" => AuthError::RateLimited,
            _ => AuthError::Network,
        }
    }
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

#[derive(Deserialize)]
struct LoginResponseBody {
    token: String,
    expires: i64,
    refresh: String,
}

/// `POST /api/register`. Maps 201/400/409/429 per `auth-service/README.md`.
pub async fn register(nick: &str, password: &str) -> Result<(), AuthError> {
    log::info(&format!("register: nick={nick}"));
    let resp = campfire_client()
        .post(format!("{CAMPFIRE_BASE_URL}/api/register"))
        .json(&serde_json::json!({ "nick": nick, "password": password }))
        .send()
        .await
        .map_err(|_| AuthError::Network)?;

    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body: ErrorBody = resp.json().await.map_err(|_| AuthError::Network)?;
    log::info(&format!("register failed: status={status} error={}", body.error));
    Err(AuthError::from_code(&body.error))
}

/// `POST /api/login`. Returns the session and the raw refresh token — the
/// caller decides whether/how to persist the refresh value; this function
/// never touches the credential store itself.
pub async fn login(nick: &str, password: &str) -> Result<(Session, String), AuthError> {
    log::info(&format!("login: nick={nick}"));
    let resp = campfire_client()
        .post(format!("{CAMPFIRE_BASE_URL}/api/login"))
        .json(&serde_json::json!({ "nick": nick, "password": password }))
        .send()
        .await
        .map_err(|_| AuthError::Network)?;

    if resp.status().is_success() {
        let body: LoginResponseBody = resp.json().await.map_err(|_| AuthError::Network)?;
        log::info(&format!(
            "login succeeded: nick={nick} token={} refresh={}",
            log::redact(&body.token),
            log::redact(&body.refresh)
        ));
        return Ok((
            Session {
                nick: nick.to_string(),
                token: body.token,
                expires: body.expires,
            },
            body.refresh,
        ));
    }
    let status = resp.status();
    let body: ErrorBody = resp.json().await.map_err(|_| AuthError::Network)?;
    log::info(&format!("login failed: status={status} error={}", body.error));
    Err(AuthError::from_code(&body.error))
}

/// `POST /api/refresh`. Exchanges a stored refresh token for a fresh game
/// token and a rotated refresh token (D-17/D-18). The caller is
/// responsible for writing the rotated value back to the credential store
/// immediately — the old value dies the instant this call succeeds.
pub async fn refresh(nick: &str, refresh_token: &str) -> Result<(Session, String), AuthError> {
    log::info(&format!("refresh: nick={nick}"));
    let resp = campfire_client()
        .post(format!("{CAMPFIRE_BASE_URL}/api/refresh"))
        .json(&serde_json::json!({ "nick": nick, "refresh": refresh_token }))
        .send()
        .await
        .map_err(|_| AuthError::Network)?;

    if resp.status().is_success() {
        let body: LoginResponseBody = resp.json().await.map_err(|_| AuthError::Network)?;
        log::info(&format!(
            "refresh succeeded: nick={nick} token={} refresh={}",
            log::redact(&body.token),
            log::redact(&body.refresh)
        ));
        return Ok((
            Session {
                nick: nick.to_string(),
                token: body.token,
                expires: body.expires,
            },
            body.refresh,
        ));
    }
    let status = resp.status();
    let body: ErrorBody = resp.json().await.map_err(|_| AuthError::Network)?;
    log::info(&format!("refresh failed: status={status} error={}", body.error));
    Err(AuthError::from_code(&body.error))
}

/// Clears local state only — no network call. There is nothing server-side
/// to revoke synchronously; the refresh token is simply forgotten locally.
pub fn logout(nick: &str) {
    log::info(&format!("logout: nick={nick}"));
    clear_refresh(nick);
}

fn nick_file_path() -> std::path::PathBuf {
    crate::paths::install_root().join(".last-nick")
}

/// Stores the refresh token in the OS credential store, and the nick in a
/// tiny plaintext sidecar file (not a secret — just the lookup key the
/// launcher needs on a cold start to know which credential-store entry to
/// ask for).
pub fn store_refresh(nick: &str, refresh_token: &str) -> Result<(), AuthError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, nick).map_err(|_| AuthError::Network)?;
    entry.set_password(refresh_token).map_err(|_| AuthError::Network)?;
    let _ = std::fs::create_dir_all(crate::paths::install_root());
    let _ = std::fs::write(nick_file_path(), nick);
    Ok(())
}

/// Reads the last-known nick from the sidecar file, then reads that nick's
/// refresh token from the credential store. `None` on a cold start (no
/// sidecar file yet) or if the credential store has nothing for that nick.
pub fn load_refresh() -> Option<(String, String)> {
    let nick = std::fs::read_to_string(nick_file_path()).ok()?;
    let nick = nick.trim().to_string();
    if nick.is_empty() {
        return None;
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, &nick).ok()?;
    let refresh_token = entry.get_password().ok()?;
    Some((nick, refresh_token))
}

/// Removes the stored refresh token for `nick` (log out). Never errors on
/// "nothing was stored" — that's not a failure, it's the already-logged-out
/// state.
pub fn clear_refresh(nick: &str) {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, nick) {
        let _ = entry.delete_credential();
    }
    let _ = std::fs::remove_file(nick_file_path());
}
