//! D-18: the whole Play sequence as one function — refresh the stored
//! session, sync the pack, provision Java, fetch Mojang's files, install
//! Forge, build the launch command, spawn it — reporting through one
//! [`ProgressSink`] and mapping every failure down to one of the handful
//! of stable codes the UI-SPEC has a sentence for. Both the `play` Tauri
//! command and `campfire-cli play` call this one function: the Tauri
//! command adapts the sink to a `tauri::ipc::Channel`, the CLI adapts it
//! to stdout — neither reimplements the sequence.

use crate::auth::{self, AuthError, Session};
use crate::forge::{self, ForgeError};
use crate::java::{self, JavaError, Target};
use crate::launch::{self, LaunchError};
use crate::log;
use crate::manifest::{self, SyncError};
use crate::mojang::{self, MojangError};
use crate::progress::{Progress, ProgressSink};

const TOTAL_STEPS: u32 = 6;

/// A stable, machine-readable outcome for every error the sequence can
/// produce — never a formatted sentence. `strings.rs` owns the sentence
/// each [`PlayError::code`] maps to; this type only names *which* one.
#[derive(Debug)]
pub enum PlayError {
    Auth(AuthError),
    Sync(SyncError),
    Java(JavaError),
    Vanilla(MojangError),
    Forge(ForgeError),
    Launch(LaunchError),
    /// The final `std::process::Command::spawn()` call itself failed —
    /// distinct from every step before it, which build the command but
    /// never run it.
    Spawn(String),
}

impl PlayError {
    /// The one stable code every UI lookup and `<verify>` grep keys off.
    /// Only the five sentences the UI-SPEC actually names get their own
    /// code (T-04-04-06's "every failure crosses the boundary as a stable
    /// code mapped to one of the contract's sentences") — everything else
    /// is `"generic"`, which still names the log.
    pub fn code(&self) -> &'static str {
        match self {
            PlayError::Auth(AuthError::InvalidCredentials) => "wrong_password",
            PlayError::Auth(AuthError::InvalidToken) | PlayError::Auth(AuthError::NoStoredSession) => {
                "session_expired"
            }
            PlayError::Auth(AuthError::Network) => "server_unreachable",
            PlayError::Auth(_) => "generic",
            PlayError::Sync(SyncError::Network(_)) | PlayError::Sync(SyncError::ManifestRejected(_)) => {
                "server_unreachable"
            }
            PlayError::Sync(SyncError::DiskFull) => "disk_full",
            PlayError::Sync(_) => "generic",
            // Every Java failure — including the Apple Silicon
            // translated-process case — is "Couldn't set up Java.": the
            // one sentence the UI-SPEC gives this whole category, not five
            // different technical causes.
            PlayError::Java(_) => "java_error",
            PlayError::Vanilla(_) => "generic",
            PlayError::Forge(_) => "generic",
            PlayError::Launch(_) => "generic",
            PlayError::Spawn(_) => "generic",
        }
    }

    /// D-18: an expired/revoked refresh token tells the frontend to reopen
    /// the auth form with the nick pre-filled, not merely show a banner.
    pub fn reopen_form(&self) -> bool {
        matches!(
            self,
            PlayError::Auth(AuthError::InvalidToken) | PlayError::Auth(AuthError::NoStoredSession)
        )
    }
}

/// What a successful [`play`] call resolved: the fresh session and the
/// complete `java` argv it built — returned so a headless caller
/// (`campfire-cli play`) can print and assert against the real command
/// line, the way `campfire-cli launch-cmd` already does for the
/// non-orchestrated builder.
pub struct PlayOutcome {
    pub session: Session,
    pub argv: Vec<String>,
}

/// Resolves the shipped target for the current host, falling back to
/// `WindowsX64` when `detect_target()` has no entry for it (this Pi has no
/// shipped Linux target) — the same fallback `campfire-cli launch-cmd`
/// already uses, so the headless proof harness exercises the identical
/// production `ensure_java()` path a real Windows machine's own
/// `detect_target()` would take. A no-op on every real shipped platform,
/// where `detect_target()` never fails.
fn resolve_target() -> Target {
    java::detect_target().unwrap_or(Target::WindowsX64)
}

/// Runs the whole sequence for `nick`, using that nick's already-stored
/// refresh token (`auth::load_refresh_for`) — the caller resolves *which*
/// nick from its own session state, this function only trusts the
/// credential store. `ram_gb` is the slider's raw half-gigabyte value; the
/// caller is responsible for clamping it to the slider's own 3..=10 range
/// before it ever reaches this function (T-04-04-07 — the clamp lives in
/// Rust, not merely in the slider element). `spawn_game` is `false` for
/// `--no-spawn`/the headless proof harness: every step through building
/// the command still runs for real either way, only the final process
/// spawn is skipped.
pub async fn play(
    nick: &str,
    ram_gb: f32,
    spawn_game: bool,
    sink: ProgressSink,
) -> Result<PlayOutcome, PlayError> {
    sink(Progress::Step {
        name: "Refreshing session".to_string(),
        current: 1,
        total: TOTAL_STEPS,
    });
    let refresh_token = auth::load_refresh_for(nick).ok_or(PlayError::Auth(AuthError::NoStoredSession))?;
    let (session, new_refresh) = auth::refresh(nick, &refresh_token).await.map_err(|e| {
        sink(Progress::Failed {
            code: PlayError::Auth(e).code().to_string(),
        });
        PlayError::Auth(e)
    })?;
    auth::store_refresh(&session.nick, &new_refresh).map_err(PlayError::Auth)?;

    sink(Progress::Step {
        name: "Syncing pack".to_string(),
        current: 2,
        total: TOTAL_STEPS,
    });
    manifest::sync(sink.clone()).await.map_err(PlayError::Sync)?;

    sink(Progress::Step {
        name: "Setting up Java".to_string(),
        current: 3,
        total: TOTAL_STEPS,
    });
    let target = resolve_target();
    let java_provision = java::ensure_java(target).await.map_err(PlayError::Java)?;

    sink(Progress::Step {
        name: "Fetching Minecraft files".to_string(),
        current: 4,
        total: TOTAL_STEPS,
    });
    mojang::ensure_vanilla(sink.clone()).await.map_err(PlayError::Vanilla)?;

    sink(Progress::Step {
        name: "Installing Forge".to_string(),
        current: 5,
        total: TOTAL_STEPS,
    });
    let (_report, merged) = forge::ensure_forge(sink.clone()).await.map_err(PlayError::Forge)?;

    sink(Progress::Step {
        name: "Launching".to_string(),
        current: 6,
        total: TOTAL_STEPS,
    });
    launch::seed_server_list();
    let argv = launch::build_launch_command(&session, ram_gb, &merged, &java_provision.java_path, true)
        .map_err(PlayError::Launch)?;
    log::info(&format!(
        "play: sequence complete for nick={} (spawn_game={spawn_game})",
        session.nick
    ));

    if spawn_game {
        launch::spawn(&argv).map_err(|e| PlayError::Spawn(e.to_string()))?;
    }

    sink(Progress::Done);
    Ok(PlayOutcome { session, argv })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-04-04-06: every error this function can produce maps to a
    /// non-empty code, and the five sentences the UI-SPEC actually names
    /// are five *distinct* codes from one another (everything else is
    /// free to collapse into "generic" — there is no dedicated sentence
    /// for a Forge/Mojang bootstrap failure).
    #[test]
    fn every_named_error_category_maps_to_its_own_distinct_code() {
        let wrong_password = PlayError::Auth(AuthError::InvalidCredentials).code();
        let session_expired = PlayError::Auth(AuthError::NoStoredSession).code();
        let server_unreachable = PlayError::Auth(AuthError::Network).code();
        let disk_full = PlayError::Sync(SyncError::DiskFull).code();
        let java_error = PlayError::Java(JavaError::ChecksumMismatch).code();

        for code in [wrong_password, session_expired, server_unreachable, disk_full, java_error] {
            assert!(!code.is_empty());
        }
        let all = [wrong_password, session_expired, server_unreachable, disk_full, java_error];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j], "codes at {i} and {j} collided: {}", all[i]);
            }
        }
    }

    #[test]
    fn every_unnamed_error_falls_back_to_generic() {
        assert_eq!(PlayError::Vanilla(MojangError::Network("x".into())).code(), "generic");
        assert_eq!(
            PlayError::Forge(ForgeError::InstallFailed("x".into())).code(),
            "generic"
        );
        assert_eq!(PlayError::Launch(LaunchError::JavaOutsideRuntime).code(), "generic");
        assert_eq!(PlayError::Spawn("x".into()).code(), "generic");
    }

    #[test]
    fn session_expired_is_the_only_category_that_reopens_the_form() {
        assert!(PlayError::Auth(AuthError::NoStoredSession).reopen_form());
        assert!(PlayError::Auth(AuthError::InvalidToken).reopen_form());
        assert!(!PlayError::Auth(AuthError::InvalidCredentials).reopen_form());
        assert!(!PlayError::Sync(SyncError::DiskFull).reopen_form());
    }
}
