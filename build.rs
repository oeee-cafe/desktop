use std::path::{Path, PathBuf};

fn main() {
    if std::env::var_os("CARGO_FEATURE_STEAM").is_some() {
        steam_api_beside_the_binary();
    }
    tauri_build::build()
}

/// Steam's library, where the binary looks for it: beside itself. The app
/// links it dynamically and does not start without it -- even when Steam is
/// not running, which the app itself takes in its stride -- so `cargo run` and
/// `cargo test` need it in `target/<profile>/`, as Steam's depot has it
/// beside the `.exe` (README.md).
///
/// Only Windows ships. A macOS build is a developer's, run by cargo, which
/// finds the `steamworks` crate's own copy of the library where it built it.
fn steam_api_beside_the_binary() {
    if !std::env::var("TARGET").unwrap().contains("windows") {
        return;
    }
    let file = "steam_api64.dll";
    let source = Path::new("steam/redistributable/win64").join(file);
    println!("cargo:rerun-if-changed={}", source.display());

    // OUT_DIR is target/<profile>/build/<crate>-<hash>/out.
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let Some(profile_dir) = out.ancestors().nth(3) else {
        return;
    };
    for dir in [profile_dir.to_path_buf(), profile_dir.join("deps")] {
        std::fs::copy(&source, dir.join(file))
            .unwrap_or_else(|e| panic!("copying {} to {}: {e}", source.display(), dir.display()));
    }
}
