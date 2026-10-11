use std::path::{Path, PathBuf};

fn main() {
    if std::env::var_os("CARGO_FEATURE_STEAM").is_some() {
        steam_api_beside_the_binary();
    }
    if std::env::var_os("CARGO_FEATURE_DISCORD").is_some() {
        discord_social_sdk();
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
    beside_the_binary(&Path::new("steam/redistributable/win64").join("steam_api64.dll"));
}

/// Where discord/README.md says to unpack Discord's Social SDK.
const DISCORD_SDK: &str = "discord/social_sdk";

/// Discord's Social SDK: src/discord/shim.cpp compiled against its C++
/// header, and its library linked, and beside the binary as Steam's is,
/// which the app does not start without either.
fn discord_social_sdk() {
    let sdk = Path::new(DISCORD_SDK);
    let header = sdk.join("include/discordpp.h");
    if !header.exists() {
        panic!(
            "\n\nDiscord's Social SDK is not in {DISCORD_SDK}/ (no {}).\n\
             Download it from the Discord Developer Portal and unpack it there, as \
             discord/README.md says, or build without it: \
             `cargo build --no-default-features --features steam`.\n\n",
            header.display()
        );
    }
    println!("cargo:rerun-if-changed=src/discord/shim.cpp");
    println!("cargo:rerun-if-changed={}", header.display());

    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .include(sdk.join("include"))
        .file("src/discord/shim.cpp")
        .compile("oeee_discord");

    let target = std::env::var("TARGET").unwrap();
    let (windows, apple) = (target.contains("windows"), target.contains("apple"));
    // Windows on ARM has its own, in an `arm64` folder beside x64's, and
    // macOS's one library is for both. Linux has x64's alone, which an ARM
    // build is not to be linked against for want of its own: one is looked
    // for where Windows's is, should a later SDK bring one, and its absence
    // said plainly rather than left to the linker.
    let arch = if target.starts_with("aarch64") && !apple {
        "arm64"
    } else {
        ""
    };
    let release = sdk.join("lib/release").join(arch);
    if !windows && !apple {
        let library = release.join("libdiscord_partner_sdk.so");
        if !target.starts_with(if arch.is_empty() { "x86_64" } else { "aarch64" })
            || !library.exists()
        {
            panic!(
                "\n\nDiscord's Social SDK has no library for {target} (no {}).\n\
                 Build without it: \
                 `cargo build --no-default-features --features steam`.\n\n",
                library.display()
            );
        }
    }
    println!("cargo:rustc-link-search=native={}", release.display());
    println!("cargo:rustc-link-lib=dylib=discord_partner_sdk");
    if windows {
        beside_the_binary(
            &sdk.join("bin/release")
                .join(arch)
                .join("discord_partner_sdk.dll"),
        );
    } else if apple {
        // A developer's build, run by cargo from target/<profile>/.
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
        beside_the_binary(&release.join("libdiscord_partner_sdk.dylib"));
    } else {
        // Linux looks for a library where the system keeps them, and not
        // beside the binary unless told to: `$ORIGIN` is the binary's own
        // folder, wherever it is installed, which the library ships in.
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        beside_the_binary(&release.join("libdiscord_partner_sdk.so"));
    }
}

/// Copies `source` into `target/<profile>/`, beside the binary, and into
/// `deps/` beside the tests.
fn beside_the_binary(source: &Path) {
    println!("cargo:rerun-if-changed={}", source.display());
    let file = source.file_name().unwrap();

    // OUT_DIR is target/<profile>/build/<crate>-<hash>/out.
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let Some(profile_dir) = out.ancestors().nth(3) else {
        return;
    };
    for dir in [profile_dir.to_path_buf(), profile_dir.join("deps")] {
        std::fs::copy(source, dir.join(file))
            .unwrap_or_else(|e| panic!("copying {} to {}: {e}", source.display(), dir.display()));
    }
}
