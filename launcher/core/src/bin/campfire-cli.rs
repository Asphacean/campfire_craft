//! Headless proof harness for `campfire-launcher-core` — every capability
//! exercisable on this Pi with no display. Passwords are always read from
//! stdin, never argv, so they never land in the process table or shell
//! history.
//!
//! Usage:
//!   campfire-cli status
//!   campfire-cli register <nick>     (password read from stdin)
//!   campfire-cli login <nick>        (password read from stdin)
//!   campfire-cli refresh             (refresh token read from the credential store)
//!   campfire-cli keyring-selftest
//!   campfire-cli pin-check

use std::io::Read;

use campfire_launcher_core::{auth, http, manifest, progress::Progress, status};

const HELP_TEXT: &str = "usage: campfire-cli status|register <nick>|login <nick>|refresh|keyring-selftest|pin-check\n              sync [--dir <path>]|verify [--dir <path>]\n\n\
Passwords are always read from stdin (never a command-line argument),\n\
so they never appear in the process table or shell history.";

/// `--dir <path>` (shared by every subcommand below that touches the
/// filesystem) overrides the install root for this process the same way
/// `CAMPFIRE_HOME` does — `paths.rs` re-reads the environment on every
/// call, so setting it once here is enough for the whole subcommand.
fn take_dir_override(args: &mut Vec<String>) {
    if let Some(pos) = args.iter().position(|a| a == "--dir") {
        if pos + 1 < args.len() {
            let dir = args.remove(pos + 1);
            args.remove(pos);
            // SAFETY: single-threaded at this point in `main`, before any
            // spawned work reads the environment.
            unsafe {
                std::env::set_var("CAMPFIRE_HOME", dir);
            }
        }
    }
}

fn print_progress(p: Progress) {
    match p {
        Progress::Step { name, current, total } => {
            println!("[{name}] {current}/{total}");
        }
        Progress::Bytes { downloaded, total, per_sec } => {
            println!("[bytes] {downloaded}/{total} · {per_sec} B/s");
        }
        Progress::Done => println!("[done]"),
        Progress::Failed { code } => println!("[failed] {code}"),
    }
}

fn usage() -> ! {
    eprintln!("{HELP_TEXT}");
    std::process::exit(1);
}

fn read_stdin_trimmed() -> String {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .expect("could not read from stdin");
    buf.trim_end_matches(['\n', '\r']).to_string()
}

#[tokio::main]
async fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    args.remove(0); // argv0
    if args.is_empty() {
        usage();
    }
    let subcommand = args.remove(0);
    take_dir_override(&mut args);

    match subcommand.as_str() {
        "--help" | "-h" => println!("{HELP_TEXT}"),
        "status" => cmd_status().await,
        "register" => {
            let nick = args.first().cloned().unwrap_or_else(|| usage());
            cmd_register(&nick).await;
        }
        "login" => {
            let nick = args.first().cloned().unwrap_or_else(|| usage());
            cmd_login(&nick).await;
        }
        "refresh" => cmd_refresh().await,
        "keyring-selftest" => cmd_keyring_selftest(),
        "pin-check" => cmd_pin_check().await,
        "sync" => cmd_sync().await,
        "verify" => cmd_verify().await,
        _ => usage(),
    }
}

async fn cmd_status() {
    let s = status::fetch_status().await;
    println!(
        "online={} players={:?} max={:?} motd={:?}",
        s.online, s.players, s.max, s.motd
    );
}

async fn cmd_register(nick: &str) {
    let password = read_stdin_trimmed();
    match auth::register(nick, &password).await {
        Ok(()) => println!("registered: {nick}"),
        Err(e) => {
            eprintln!("FATAL: register failed: {e:?}");
            std::process::exit(1);
        }
    }
}

async fn cmd_login(nick: &str) {
    let password = read_stdin_trimmed();
    match auth::login(nick, &password).await {
        Ok((session, refresh_token)) => {
            if let Err(e) = auth::store_refresh(&session.nick, &refresh_token) {
                eprintln!("FATAL: could not store refresh token: {e:?}");
                std::process::exit(1);
            }
            // The game token is short-lived (12h), single-use for /validate,
            // and exists precisely so the caller can pass it on (this
            // mirrors campfire-auth's own `login <nick>` CLI, which also
            // prints its minted token to stdout for the same reason). Only
            // the refresh token and password are protected secrets here —
            // the refresh token above never appears anywhere but the
            // credential store.
            println!(
                "nick={} token={} expires={}",
                session.nick, session.token, session.expires
            );
        }
        Err(e) => {
            eprintln!("FATAL: login failed: {e:?}");
            std::process::exit(1);
        }
    }
}

async fn cmd_refresh() {
    let Some((nick, refresh_token)) = auth::load_refresh() else {
        eprintln!("FATAL: no stored session — log in first");
        std::process::exit(1);
    };
    match auth::refresh(&nick, &refresh_token).await {
        Ok((session, new_refresh)) => {
            if let Err(e) = auth::store_refresh(&session.nick, &new_refresh) {
                eprintln!("FATAL: could not store rotated refresh token: {e:?}");
                std::process::exit(1);
            }
            println!(
                "nick={} token={} expires={}",
                session.nick, session.token, session.expires
            );
        }
        Err(e) => {
            eprintln!("FATAL: refresh failed: {e:?}");
            std::process::exit(1);
        }
    }
}

fn cmd_keyring_selftest() {
    let entry = match keyring::Entry::new("pub.campfire.launcher", "keyring-selftest") {
        Ok(e) => e,
        Err(e) => {
            println!("FAIL: could not create a credential-store entry: {e}");
            std::process::exit(1);
        }
    };
    let probe_value = format!("selftest-{}", std::process::id());
    if let Err(e) = entry.set_password(&probe_value) {
        println!("FAIL: could not write to the credential store: {e}");
        std::process::exit(1);
    }
    let read_back = entry.get_password();
    let _ = entry.delete_credential();
    match read_back {
        Ok(value) if value == probe_value => {
            println!("PASS: keyring round-trip succeeded (linux keyutils backend)");
        }
        Ok(other) => {
            println!("FAIL: read back a different value than was written: {other}");
            std::process::exit(1);
        }
        Err(e) => {
            println!("FAIL: could not read back from the credential store: {e}");
            std::process::exit(1);
        }
    }
}

async fn cmd_pin_check() {
    let client = http::campfire_client();

    match client.get(format!("{}/status", http::CAMPFIRE_BASE_URL)).send().await {
        Ok(resp) if resp.status().is_success() => {
            println!("PASS: pinned client reached mc.campfire.pub over our own CA");
        }
        Ok(resp) => {
            println!("FAIL: mc.campfire.pub responded with {}", resp.status());
            std::process::exit(1);
        }
        Err(e) => {
            println!("FAIL: pinned client could not reach mc.campfire.pub: {e}");
            std::process::exit(1);
        }
    }

    // The actual proof that built-in roots are disabled, not merely
    // unused: the same pinned client must FAIL against a public-CA host.
    match client
        .get("https://api.adoptium.net/v3/info/available_releases")
        .send()
        .await
    {
        Ok(resp) => {
            println!(
                "FAIL: pinned client reached a public-CA host (status {}) — built-in roots are NOT disabled",
                resp.status()
            );
            std::process::exit(1);
        }
        Err(e) if e.is_connect() || e.to_string().contains("certificate") || e.to_string().contains("UnknownIssuer") => {
            println!("PASS: pinned client could not reach a public-CA host (certificate error, as expected)");
        }
        Err(e) => {
            println!("FAIL: pinned client failed against the public-CA host, but not with a certificate error: {e}");
            std::process::exit(1);
        }
    }
}

async fn cmd_sync() {
    match manifest::sync(&print_progress).await {
        Ok(report) => {
            println!(
                "SYNC OK — checked={} downloaded={} deleted={} seeded={} bytes={}",
                report.checked, report.downloaded, report.deleted, report.seeded, report.bytes_downloaded
            );
        }
        Err(e) => {
            eprintln!("FATAL: sync failed: {e:?}");
            std::process::exit(1);
        }
    }
}

async fn cmd_verify() {
    match manifest::verify(&print_progress).await {
        Ok(report) => {
            println!(
                "VERIFY OK — checked={} repaired={}",
                report.checked, report.repaired
            );
        }
        Err(e) => {
            eprintln!("FATAL: verify failed: {e:?}");
            std::process::exit(1);
        }
    }
}

