//! `campfire-auth` — binary entry point. Subcommand dispatch (`serve` today;
//! `login`/`reset` are Task 2's operator CLI), the loopback-bind guard, and
//! the axum router wiring.

mod api;
mod auth;
mod db;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::post;
use axum::Router;

use api::AppState;
use db::Db;

const DEFAULT_BIND: &str = "127.0.0.1:8081";

fn usage() -> ! {
    eprintln!("usage: campfire-auth serve");
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args();
    let _argv0 = args.next();
    match args.next().as_deref() {
        Some("serve") => serve().await,
        _ => usage(),
    }
}

async fn serve() {
    let bind = std::env::var("AUTH_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_string());
    let db_path = std::env::var("AUTH_DB")
        .unwrap_or_else(|_| {
            eprintln!("FATAL: AUTH_DB is not set (no default — this is the accounts database path)");
            std::process::exit(1);
        });

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

    let db = Db::open(&db_path).unwrap_or_else(|e| {
        eprintln!("FATAL: could not open accounts database at '{db_path}': {e}");
        std::process::exit(1);
    });
    let state = Arc::new(AppState { db });

    let app = Router::new()
        .route("/register", post(api::register))
        .route("/login", post(api::login))
        .route("/validate", post(api::validate))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
        eprintln!("FATAL: could not bind '{addr}': {e}");
        std::process::exit(1);
    });

    eprintln!("campfire-auth listening on {addr}");
    // into_make_service_with_connect_info: Task 2's per-IP rate limiter
    // needs the peer address; wiring it in now avoids reworking the serve
    // call later.
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
