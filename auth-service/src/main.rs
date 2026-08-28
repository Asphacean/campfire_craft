//! `campfire-auth` — binary entry point. Subcommand dispatch (`serve` /
//! `login` / `reset`), the loopback-bind guard, and the axum router wiring.

mod api;
mod auth;
mod db;
mod ratelimit;

use std::io::Read;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::routing::{get, post};
use axum::Router;

use api::AppState;
use db::Db;
use ratelimit::RateLimiter;

const DEFAULT_BIND: &str = "127.0.0.1:8081";

/// D-04: 5 registrations/hour/peer.
const REGISTER_LIMIT: usize = 5;
/// 10 *failed* login attempts/hour/peer.
const LOGIN_FAIL_LIMIT: usize = 10;
const RATE_WINDOW: Duration = Duration::from_secs(3600);
/// Same TTL `/login` uses (D-03) — `campfire-auth login` mints through the
/// same code path.
const TOKEN_TTL_SECS: i64 = 12 * 60 * 60;
/// Same rule `/register` enforces — `campfire-auth reset` must not be able
/// to set a weaker password than self-registration would accept.
const MIN_PASSWORD_LEN: usize = 8;

fn usage() -> ! {
    eprintln!("usage: campfire-auth serve|login <nick>|reset <nick>");
    std::process::exit(1);
}

fn open_db_from_env() -> Db {
    let db_path = std::env::var("AUTH_DB").unwrap_or_else(|_| {
        eprintln!("FATAL: AUTH_DB is not set (no default — this is the accounts database path)");
        std::process::exit(1);
    });
    Db::open(&db_path).unwrap_or_else(|e| {
        eprintln!("FATAL: could not open accounts database at '{db_path}': {e}");
        std::process::exit(1);
    })
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args();
    let _argv0 = args.next();
    match args.next().as_deref() {
        Some("serve") => serve().await,
        Some("login") => {
            let nick = args.next().unwrap_or_else(|| usage());
            cli_login(&nick);
        }
        Some("reset") => {
            let nick = args.next().unwrap_or_else(|| usage());
            cli_reset(&nick);
        }
        _ => usage(),
    }
}

/// `campfire-auth login <nick>`: mints and prints a token for that nick,
/// through the same issuance path `/login` uses after its password check,
/// and prints nothing else so the output pastes straight into a JVM flag.
///
/// This asks for no password on purpose, and that is not an authentication
/// bypass: it can only ever run for someone who can already open the
/// mode-600 database file (D-13) — which is strictly more privilege than
/// knowing an account's password — so a password prompt here would be
/// theatre, not a security control (D-05).
fn cli_login(nick: &str) {
    let db = open_db_from_env();
    let nick_lower = nick.to_lowercase();
    let user = db
        .find_user_by_nick_lower(&nick_lower)
        .unwrap_or_else(|e| {
            eprintln!("FATAL: database error: {e}");
            std::process::exit(1);
        })
        .unwrap_or_else(|| {
            eprintln!("FATAL: no such nick: {nick}");
            std::process::exit(1);
        });

    let token = auth::generate_token();
    let token_hash = auth::hash_secret(&token).unwrap_or_else(|e| {
        eprintln!("FATAL: could not hash token: {e}");
        std::process::exit(1);
    });
    let expires = db::now_unix() + TOKEN_TTL_SECS;
    db.insert_token(user.id, &token_hash, expires)
        .unwrap_or_else(|e| {
            eprintln!("FATAL: could not store token: {e}");
            std::process::exit(1);
        });

    println!("{token}");
}

/// `campfire-auth reset <nick>`: reads a new password from stdin, applies
/// the same length rule as registration, and replaces the stored hash.
fn cli_reset(nick: &str) {
    let mut password = String::new();
    std::io::stdin()
        .read_to_string(&mut password)
        .unwrap_or_else(|e| {
            eprintln!("FATAL: could not read new password from stdin: {e}");
            std::process::exit(1);
        });
    let password = password.trim_end_matches(['\n', '\r']);

    if password.chars().count() < MIN_PASSWORD_LEN {
        eprintln!("FATAL: new password must be at least {MIN_PASSWORD_LEN} characters");
        std::process::exit(1);
    }

    let db = open_db_from_env();
    let nick_lower = nick.to_lowercase();
    let pw_hash = auth::hash_secret(password).unwrap_or_else(|e| {
        eprintln!("FATAL: could not hash password: {e}");
        std::process::exit(1);
    });

    let updated = db.update_pw_hash(&nick_lower, &pw_hash).unwrap_or_else(|e| {
        eprintln!("FATAL: database error: {e}");
        std::process::exit(1);
    });
    if !updated {
        eprintln!("FATAL: no such nick: {nick}");
        std::process::exit(1);
    }

    println!("Password reset for {nick}");
}

async fn serve() {
    let bind = std::env::var("AUTH_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_string());
    let db = open_db_from_env();

    let addr: SocketAddr = bind.parse().unwrap_or_else(|e| {
        eprintln!("FATAL: AUTH_BIND '{bind}' is not a valid address: {e}");
        std::process::exit(1);
    });

    // D-16 / T-02-01-07: nothing new is exposed to the internet this phase.
    // A config typo in server.env must not be able to violate that, so the
    // guard lives in the binary, not (only) in the unit file.
    if !addr.ip().is_loopback() {
        eprintln!(
            "FATAL: refusing to bind non-loopback address '{addr}' — AUTH_BIND must be a 127.0.0.1/::1 address"
        );
        std::process::exit(1);
    }

    let state = Arc::new(AppState {
        db,
        register_limiter: RateLimiter::new(RATE_WINDOW, REGISTER_LIMIT),
        login_limiter: RateLimiter::new(RATE_WINDOW, LOGIN_FAIL_LIMIT),
    });

    let app = Router::new()
        .route("/register", post(api::register))
        .route("/login", post(api::login))
        .route("/validate", post(api::validate))
        .route("/status", get(api::status))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
        eprintln!("FATAL: could not bind '{addr}': {e}");
        std::process::exit(1);
    });

    eprintln!("campfire-auth listening on {addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("FATAL: server error: {e}");
        std::process::exit(1);
    });
}
