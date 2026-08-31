//! Gap-closure #4 regression: the Mac UAT report ("fetching Minecraft files
//! failed: Io(\"No such file or directory (os error 2)\")") happened on a
//! real macOS install root that always contains a space
//! (`~/Library/Application Support/campfire`). This test drives the exact
//! same vanilla-bootstrap + Forge-install pipeline `play.rs` orchestrates
//! into a Pi-reproducible stand-in — a scratch `CAMPFIRE_HOME` that also
//! contains a space — proving path-building through every `PathBuf::join`
//! in `mojang.rs`/`forge.rs`/`java.rs` never breaks on one.
//!
//! Real network, real Mojang/Forge downloads (~180MB), several minutes on
//! a Pi — `#[ignore]`d so `cargo test --workspace` stays offline and fast
//! by default. Pi-runnable on demand:
//!
//! ```sh
//! CAMPFIRE_FORGE_JAVA=/path/to/a/real/java8 \
//!   cargo test --release -p campfire-launcher-core --test vanilla_forge_space_dir -- --ignored --nocapture
//! ```
//!
//! `CAMPFIRE_FORGE_JAVA` is the same test-only escape hatch
//! `forge::resolve_forge_java` already documents: this Pi has no shipped
//! Java 8 to run the Forge installer with, so the proof points at any real
//! Java 8 already on the machine (mirrors the Phase 1 Temurin 8 used for
//! the game server).

use campfire_launcher_core::{forge, mojang, progress};

fn scratch_home_with_a_space() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("campfire vanilla forge space test {}", std::process::id()));
    assert!(
        dir.to_string_lossy().contains(' '),
        "this test only proves anything if the scratch home actually has a space in it"
    );
    dir
}

#[tokio::test]
#[ignore = "live network + real Mojang/Forge downloads; Pi-runnable on demand, see module docs"]
async fn vanilla_and_forge_bootstrap_into_a_space_containing_home() {
    let home = scratch_home_with_a_space();
    let _ = std::fs::remove_dir_all(&home);
    // SAFETY: test-only, single-threaded within this test's lifetime; this
    // binary runs no other test concurrently (see `launch_command.rs`'s
    // identical comment for the same constraint in this crate).
    unsafe {
        std::env::set_var("CAMPFIRE_HOME", &home);
    }

    let sink = progress::sink_from(|_| {});
    let vanilla = mojang::ensure_vanilla(sink.clone())
        .await
        .expect("vanilla bootstrap must succeed against a space-containing CAMPFIRE_HOME");
    assert!(vanilla.libraries_included > 0);
    assert!(vanilla.asset_object_count > 0);

    let (report, merged) = forge::ensure_forge(sink)
        .await
        .expect("forge install must succeed against a space-containing CAMPFIRE_HOME — set CAMPFIRE_FORGE_JAVA to a real Java 8 binary");
    assert_eq!(report.version_id, forge::FORGE_ID);
    assert!(merged.libraries.len() > vanilla.libraries_included as usize);

    unsafe {
        std::env::remove_var("CAMPFIRE_HOME");
    }
    let _ = std::fs::remove_dir_all(&home);
}
