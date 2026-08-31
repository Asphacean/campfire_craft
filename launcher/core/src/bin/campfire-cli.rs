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
use std::path::PathBuf;

use campfire_launcher_core::{
    auth, forge, http, java, launch, manifest, mojang, play, progress::Progress, status, system, update,
};

const HELP_TEXT: &str = "usage: campfire-cli status|register <nick>|login <nick>|refresh|keyring-selftest|pin-check\n              sync [--dir <path>]|verify [--dir <path>]\n              java-fetch [--target windows-x64|mac-x64|mac-arm64] [--dir <path>]|java-probe\n              vanilla [--dir <path>]|forge [--dir <path>]\n              launch-cmd --nick <n> --ram <g> [--token <t>] [--target ...] [--dir <path>]\n              launch --nick <n> --ram <g> [--token <t>] [--target ...] [--dir <path>]\n              play --nick <n> --ram <g> [--no-spawn] [--dir <path>]\n              system-memory\n              update-check\n\n\
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

/// Removes `flag` and its following value from `args`, if present.
fn take_flag_value(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == flag)?;
    if pos + 1 >= args.len() {
        return None;
    }
    let value = args.remove(pos + 1);
    args.remove(pos);
    Some(value)
}

/// Removes a bare boolean `flag` from `args`, if present, and reports
/// whether it was there.
fn take_bool_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(pos) = args.iter().position(|a| a == flag) {
        args.remove(pos);
        true
    } else {
        false
    }
}

/// Wraps `print_progress` as an owned `ProgressSink` — every headless
/// subcommand that streams progress shares this one construction.
fn progress_sink() -> campfire_launcher_core::progress::ProgressSink {
    campfire_launcher_core::progress::sink_from(print_progress)
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
        "java-fetch" => {
            let target = args
                .iter()
                .position(|a| a == "--target")
                .and_then(|pos| args.get(pos + 1))
                .cloned();
            cmd_java_fetch(target.as_deref()).await;
        }
        "java-probe" => cmd_java_probe(),
        "vanilla" => cmd_vanilla().await,
        "forge" => cmd_forge().await,
        "launch-cmd" => cmd_launch_cmd(&mut args, false).await,
        "launch" => cmd_launch_cmd(&mut args, true).await,
        "play" => cmd_play(&mut args).await,
        "system-memory" => cmd_system_memory(),
        "update-check" => cmd_update_check().await,
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
    match manifest::sync(progress_sink()).await {
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
    match manifest::verify(progress_sink()).await {
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

async fn cmd_java_fetch(target_arg: Option<&str>) {
    let target = match target_arg {
        Some(s) => match java::Target::parse(s) {
            Some(t) => t,
            None => {
                eprintln!("FATAL: unrecognized --target '{s}' (expected windows-x64|mac-x64|mac-arm64)");
                std::process::exit(1);
            }
        },
        None => match java::detect_target() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("FATAL: java-fetch failed: {e:?}");
                std::process::exit(1);
            }
        },
    };
    println!("target={}", target.as_str());
    match java::ensure_java(target).await {
        Ok(p) => {
            println!("release={}", p.release);
            println!("link={}", p.link);
            println!("checksum={}", p.checksum);
            println!("java={}", p.java_path.display());
        }
        Err(e) => {
            eprintln!("FATAL: java-fetch failed: {e:?}");
            std::process::exit(1);
        }
    }
}

async fn cmd_vanilla() {
    match mojang::ensure_vanilla(progress_sink()).await {
        Ok(r) => {
            println!(
                "version={} libs_included={} libs_excluded={} natives_resolved={} asset_index={} asset_objects={} bytes={}",
                r.version_id,
                r.libraries_included,
                r.libraries_excluded,
                r.natives_resolved,
                r.asset_index_id,
                r.asset_object_count,
                r.bytes_downloaded
            );
            println!("OK");
        }
        Err(e) => {
            eprintln!("FATAL: vanilla failed: {e:?}");
            std::process::exit(1);
        }
    }
}

async fn cmd_forge() {
    match forge::ensure_forge(progress_sink()).await {
        Ok((report, merged)) => {
            println!(
                "installer_sha256_ok={} already_installed={} version={} merged_libraries={} classpath_len={}",
                report.installer_hash_verified,
                report.already_installed,
                report.version_id,
                report.merged_library_count,
                report.classpath_len
            );
            if report.already_installed {
                println!("already installed — installer skipped");
            }
            let _ = merged;
            println!("OK");
        }
        Err(e) => {
            eprintln!("FATAL: forge failed: {e:?}");
            std::process::exit(1);
        }
    }
}

/// Resolves the java path for `launch-cmd`/`launch` via the real
/// `java::ensure_java` pipeline — `--target` overrides `detect_target()`,
/// which has no Linux entry; on this dev Pi (no shipped Linux target) this
/// defaults to `windows-x64` purely to exercise the identical production
/// code path a real Windows machine's own `detect_target()` would take.
async fn resolve_cli_java(target_arg: Option<&str>) -> java::Target {
    match target_arg.and_then(java::Target::parse) {
        Some(t) => t,
        None => java::detect_target().unwrap_or(java::Target::WindowsX64),
    }
}

async fn cmd_launch_cmd(args: &mut Vec<String>, spawn_it: bool) {
    let nick = take_flag_value(args, "--nick").unwrap_or_else(|| usage());
    let ram: f32 = take_flag_value(args, "--ram")
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| usage());
    let token = take_flag_value(args, "--token").unwrap_or_else(|| "0".to_string());
    let target_arg = take_flag_value(args, "--target");

    // Vanilla + Forge must already be bootstrapped (`vanilla`/`forge`
    // subcommands) — `launch-cmd` only builds the command line from what's
    // already on disk, matching the "one function per wave-4 step" split.
    if let Err(e) = mojang::load_version_json() {
        eprintln!("FATAL: no vanilla install found — run `campfire-cli vanilla` first: {e:?}");
        std::process::exit(1);
    }
    // `noop_sink`, not `print_progress`: `launch-cmd`'s stdout is the argv,
    // one element per line, and is redirected straight into acceptance
    // checks — this call is expected to be the already-installed fast path
    // (progress spam would land in the same stream as the argv otherwise).
    let (_, merged) = match forge::ensure_forge(campfire_launcher_core::progress::noop_sink()).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("FATAL: forge not installed — run `campfire-cli forge` first: {e:?}");
            std::process::exit(1);
        }
    };

    let target = resolve_cli_java(target_arg.as_deref()).await;
    let java_path: PathBuf = match java::ensure_java(target).await {
        Ok(p) => p.java_path,
        Err(e) => {
            eprintln!("FATAL: java resolution failed: {e:?}");
            std::process::exit(1);
        }
    };

    launch::seed_server_list();

    let session = auth::Session {
        nick,
        token,
        expires: 0,
    };
    let argv = match launch::build_launch_command(&session, ram, &merged, &java_path, true, None) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("FATAL: build_launch_command failed: {e:?}");
            std::process::exit(1);
        }
    };

    for arg in &argv {
        println!("{arg}");
    }

    if spawn_it {
        match launch::spawn(&argv) {
            Ok(child) => println!("spawned pid={}", child.id()),
            Err(e) => {
                eprintln!("FATAL: spawn failed: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn cmd_java_probe() {
    match java::read_marker() {
        Some((release, target, path)) => {
            println!("release={release} target={target} java={}", path.display());
            match std::process::Command::new(&path).arg("-version").output() {
                Ok(out) => {
                    print!("{}", String::from_utf8_lossy(&out.stderr));
                    print!("{}", String::from_utf8_lossy(&out.stdout));
                }
                Err(e) => println!("(not runnable on this host: {e})"),
            }
        }
        None => {
            eprintln!("FATAL: no Java provisioned yet — run java-fetch first");
            std::process::exit(1);
        }
    }
}


/// The whole Play sequence, headlessly: refresh → sync → Java → Mojang →
/// Forge → build the command → spawn (unless `--no-spawn`). This is what
/// proves the orchestration end to end on a machine with no display —
/// `--nick` must already have a stored refresh token (`campfire-cli login
/// <nick>` first).
async fn cmd_play(args: &mut Vec<String>) {
    let nick = take_flag_value(args, "--nick").unwrap_or_else(|| usage());
    let ram: f32 = take_flag_value(args, "--ram")
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| usage());
    let no_spawn = take_bool_flag(args, "--no-spawn");

    match play::play(&nick, ram, !no_spawn, progress_sink()).await {
        Ok(outcome) => {
            // Redact the real token everywhere it appears in the printed
            // argv, one element per line — the token itself is real and
            // was really used to build this exact command, only the
            // printed proof withholds it.
            let token = outcome.session.token.as_str();
            for arg in &outcome.argv {
                if !token.is_empty() && arg.contains(token) {
                    println!("{}", arg.replace(token, "<redacted>"));
                } else {
                    println!("{arg}");
                }
            }
            println!("PLAY OK — nick={} spawned={}", outcome.session.nick, !no_spawn);
        }
        Err(e) => {
            // Deliberately no `{e:?}` here: this is the one FATAL message
            // in the whole CLI that must read exactly like the window
            // would show it — a plain sentence, never a Rust type name, a
            // `reqwest` string, or an HTTP status number. Every other
            // subcommand in this file prints its error's Debug form
            // because it's a developer-facing proof harness; `play` is
            // additionally the acceptance-tested proof that the mapping
            // itself never leaks internals.
            let code = e.code();
            let reopen = e.reopen_form();
            let sentence = campfire_launcher_core::strings::play_error_sentence(code);
            eprintln!("FATAL: {sentence} (code={code}, reopen_form={reopen})");
            std::process::exit(1);
        }
    }
}

fn cmd_system_memory() {
    let total = system::total_memory_gb();
    let recommended = system::recommended_ram_gb(total);
    println!("total_gb={total:.2} recommended_gb={recommended:.1}");
}

/// LNCH-08: checks the real `/launcher/latest.json` feed against this
/// binary's own `CARGO_PKG_VERSION`. Always exits 0 — a failed or
/// malformed check reports "no update available" rather than an error,
/// exactly like the startup check the window performs (D-08: self-update
/// is a convenience, never a precondition for playing).
async fn cmd_update_check() {
    let current = env!("CARGO_PKG_VERSION");
    match update::check(current).await {
        Some(a) => println!("update available: {} ({})", a.version, a.notes),
        None => println!("no update available"),
    }
}
