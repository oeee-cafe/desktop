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
/// `cargo test` need it in `target/<profile>/` just as a bundle does. Bundles
/// get it from `tauri.conf.json` (macOS, Linux) and the depot (Windows).
fn steam_api_beside_the_binary() {
    let target = std::env::var("TARGET").unwrap();
    let (dir, file) = if target.contains("windows") {
        ("win64", "steam_api64.dll")
    } else if target.contains("apple") {
        ("osx", "libsteam_api.dylib")
    } else {
        ("linux64", "libsteam_api.so")
    };
    let source = Path::new("steam/redistributable").join(dir).join(file);
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
