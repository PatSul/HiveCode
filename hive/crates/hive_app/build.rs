use std::process::Command;

fn main() {
    // -- Auto-increment build version from git ----------------------------
    //
    // HIVE_BUILD_NUMBER = total commit count (monotonically increasing).
    // HIVE_GIT_HASH     = short commit hash for traceability.
    // HIVE_VERSION       = "0.1.<commit_count>" — a semver-compatible string.
    //
    // These are available at compile time via `env!("HIVE_VERSION")`, etc.

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

    let version = format!("0.1.{commit_count}");

    println!("cargo:rustc-env=HIVE_BUILD_NUMBER={commit_count}");
    println!("cargo:rustc-env=HIVE_GIT_HASH={git_hash}");
    println!("cargo:rustc-env=HIVE_VERSION={version}");

    // Rebuild when git state changes (new commits).
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads/");

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
