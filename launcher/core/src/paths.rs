//! D-13's on-disk layout, resolved once. `directories` gives the platform
//! config/data root; `CAMPFIRE_HOME` overrides it entirely, which is what
//! lets every headless test in this phase run against a scratch directory
//! instead of a real profile.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use directories::ProjectDirs;

/// A collision-proof suffix for temp files. `std::process::id()` alone is
/// identical for every concurrent async task inside one process — two
/// tasks racing to write the same destination (e.g. `ensure_vanilla` and
/// `ensure_java` both build their own tmp path from bare pid) can share the
/// same tmp path; whichever renames first leaves the other's later
/// `rename`/`remove_file` call hitting a bare ENOENT with no indication
/// which file or op it was, since `io::Error`'s `Display` never names
/// either (gap-closure #4 audit). One process-wide counter, appended to
/// pid, closes the race outright rather than merely narrowing it.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn unique_tmp_suffix() -> String {
    format!("{}-{}", std::process::id(), TMP_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Serializes every test in this crate that mutates the process-wide
/// `CAMPFIRE_HOME` env var. `cargo test` runs unit tests from multiple
/// threads inside one process by default; two tests each doing their own
/// unguarded `set_var`/`remove_var("CAMPFIRE_HOME")` race, and a slower
/// thread can read the *other* test's value between the fast thread's
/// `set_var` and its own assertion (gap-closure #4 — this crate's own test
/// suite hit exactly this once a second `CAMPFIRE_HOME`-mutating test
/// module landed). `pub(crate)` so `java.rs`'s equivalent test shares it.
#[cfg(test)]
pub(crate) static CAMPFIRE_HOME_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Wraps a bare `io::Error` with the operation and path it failed on.
/// `io::Error`'s own `Display`/`Debug` never include either — a Mojang or
/// Forge bootstrap failure logged as `Io("No such file or directory (os
/// error 2)")` gives an operator nothing to act on. Every fs/process call
/// in the play pipeline that turns an `io::Error` into this crate's own
/// error types goes through here (gap-closure #4).
pub fn io_ctx(op: &str, path: &Path, e: std::io::Error) -> String {
    format!("{op} {}: {e}", path.display())
}

/// `%APPDATA%\campfire\` on Windows, `~/Library/Application Support/campfire/`
/// on macOS (D-13). Honors `CAMPFIRE_HOME` first so tests and `campfire-cli`
/// runs on this Pi never touch a real profile.
pub fn install_root() -> PathBuf {
    if let Ok(home) = std::env::var("CAMPFIRE_HOME") {
        return PathBuf::from(home);
    }
    // "campfire", no qualifier/organization component — D-13 names the
    // directory literally `campfire`, not a reverse-DNS-style path.
    let dirs = ProjectDirs::from("", "", "campfire").expect("no home directory for this user");
    dirs.data_dir().to_path_buf()
}

fn ensure_dir(path: PathBuf) -> PathBuf {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::create_dir_all(&path);
    path
}

/// The Minecraft instance directory: mods, config, saves, etc.
pub fn game_dir() -> PathBuf {
    ensure_dir(install_root().join("game"))
}

/// Provisioned Java 8 runtime(s).
pub fn runtime_dir() -> PathBuf {
    ensure_dir(install_root().join("runtime"))
}

/// Version JSONs (vanilla + Forge-merged), one directory per version id.
pub fn versions_dir() -> PathBuf {
    ensure_dir(install_root().join("versions"))
}

/// Mojang + Forge library jars, laid out by their Maven-style path.
pub fn libraries_dir() -> PathBuf {
    ensure_dir(install_root().join("libraries"))
}

/// Mojang asset objects, indexed by asset index.
pub fn assets_dir() -> PathBuf {
    ensure_dir(install_root().join("assets"))
}

/// `launcher.log`, directly under the install root (D-13).
pub fn log_path() -> PathBuf {
    let root = install_root();
    let _ = std::fs::create_dir_all(&root);
    root.join("launcher.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campfire_home_override_is_honored() {
        // Holds `CAMPFIRE_HOME_TEST_LOCK` for the test's whole body — see
        // the lock's own doc comment for why this is load-bearing, not
        // decorative.
        let _guard = CAMPFIRE_HOME_TEST_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("campfire-paths-test-{}", std::process::id()));
        // SAFETY: test-only; `_guard` above serializes every test in this
        // crate that touches CAMPFIRE_HOME, so no other thread reads it
        // mid-mutation.
        unsafe {
            std::env::set_var("CAMPFIRE_HOME", &tmp);
        }
        assert_eq!(install_root(), tmp);
        assert_eq!(game_dir(), tmp.join("game"));
        assert_eq!(log_path(), tmp.join("launcher.log"));
        unsafe {
            std::env::remove_var("CAMPFIRE_HOME");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Every dir helper must resolve correctly under a `CAMPFIRE_HOME`
    /// containing a space — the real macOS default install root
    /// (`~/Library/Application Support/campfire`) always has one, and
    /// gap-closure #4's audit needed a Pi-reproducible stand-in for it.
    #[test]
    fn every_dir_helper_resolves_under_a_space_containing_home() {
        let _guard = CAMPFIRE_HOME_TEST_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("campfire paths test {}", std::process::id()));
        unsafe {
            std::env::set_var("CAMPFIRE_HOME", &tmp);
        }
        assert_eq!(game_dir(), tmp.join("game"));
        assert!(game_dir().is_dir());
        assert_eq!(runtime_dir(), tmp.join("runtime"));
        assert!(runtime_dir().is_dir());
        assert_eq!(versions_dir(), tmp.join("versions"));
        assert!(versions_dir().is_dir());
        assert_eq!(libraries_dir(), tmp.join("libraries"));
        assert!(libraries_dir().is_dir());
        assert_eq!(assets_dir(), tmp.join("assets"));
        assert!(assets_dir().is_dir());
        unsafe {
            std::env::remove_var("CAMPFIRE_HOME");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unique_tmp_suffix_never_repeats_within_one_process() {
        let a = unique_tmp_suffix();
        let b = unique_tmp_suffix();
        assert_ne!(a, b, "two calls in the same process must not collide — this is the whole point of the counter");
        // Both still carry the pid prefix, so a stray leftover tmp file is
        // still attributable to the process that made it.
        let pid_prefix = format!("{}-", std::process::id());
        assert!(a.starts_with(&pid_prefix));
        assert!(b.starts_with(&pid_prefix));
    }

    #[test]
    fn io_ctx_names_both_the_operation_and_the_path() {
        let path = Path::new("/some/example/path.txt");
        let underlying = std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory (os error 2)");
        let msg = io_ctx("rename", path, underlying);
        assert!(msg.contains("rename"), "message must name the operation: {msg}");
        assert!(msg.contains("/some/example/path.txt"), "message must name the path: {msg}");
        assert!(msg.contains("No such file or directory"), "message must still carry the OS error: {msg}");
    }
}
