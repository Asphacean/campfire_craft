//! D-13's on-disk layout, resolved once. `directories` gives the platform
//! config/data root; `CAMPFIRE_HOME` overrides it entirely, which is what
//! lets every headless test in this phase run against a scratch directory
//! instead of a real profile.

use std::path::PathBuf;

use directories::ProjectDirs;

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
        let tmp = std::env::temp_dir().join(format!("campfire-paths-test-{}", std::process::id()));
        // SAFETY: test-only, single-threaded within this test's lifetime;
        // no other test in this crate reads CAMPFIRE_HOME concurrently.
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
}
