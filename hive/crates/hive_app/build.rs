use std::process::Command;

fn main() {
    // -- Version from Cargo.toml (bumped by auto-release workflow) --------
    //
    // HIVE_VERSION = CARGO_PKG_VERSION from Cargo.toml (e.g. "0.2.1").
    // This matches the git tags created by auto-release.yml, so the
    // in-app updater can compare versions correctly.
    //
    // HIVE_GIT_HASH = short commit hash for traceability.
    // HIVE_BUILD_NUMBER = total commit count (informational).

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());

    let commit_count = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "0".to_string());

    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=HIVE_BUILD_NUMBER={commit_count}");
    println!("cargo:rustc-env=HIVE_GIT_HASH={git_hash}");
    println!("cargo:rustc-env=HIVE_VERSION={version}");

    // Rebuild when git state changes or Cargo.toml version changes.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads/");
    println!("cargo:rerun-if-changed=Cargo.toml");

    // -- Windows icon embedding -------------------------------------------
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("hive_bee.ico");
        if let Err(e) = res.compile() {
            eprintln!("cargo:warning=Failed to embed icon: {e}");
        }
    }
}
