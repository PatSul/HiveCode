//! Auto-update service — checks GitHub releases for newer versions and
//! performs in-place binary replacement on all platforms.
//!
//! The update strategy varies by platform:
//! - **macOS (Homebrew)**: `brew upgrade hive` (preferred) or binary swap
//! - **Linux**: Downloads the tarball and replaces the binary in-place
//! - **Windows**: Downloads the zip, extracts, and replaces the exe

use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use anyhow::{Context, Result, bail};
use parking_lot::RwLock;
use tracing::{info, warn};

/// Repository owner/name on GitHub.
const GITHUB_REPO: &str = "PatSul/Hive";

/// Information about an available update.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    /// The new version string (e.g. "0.3.0").
    pub version: String,
    /// URL to the release page on GitHub.
    pub release_url: String,
    /// URL to the platform-appropriate asset for direct download.
    pub asset_url: String,
    /// Release notes / body text (markdown).
    pub release_notes: String,
}

/// Shared state for the updater, safe to clone across threads.
#[derive(Clone)]
pub struct UpdateService {
    inner: Arc<UpdateServiceInner>,
}

struct UpdateServiceInner {
    current_version: String,
    update_info: RwLock<Option<UpdateInfo>>,
    checking: AtomicBool,
    updating: AtomicBool,
}

impl UpdateService {
    /// Create a new update service with the current running version.
    pub fn new(current_version: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(UpdateServiceInner {
                current_version: current_version.into(),
                update_info: RwLock::new(None),
                checking: AtomicBool::new(false),
                updating: AtomicBool::new(false),
            }),
        }
    }

    /// Returns the current running version.
    pub fn current_version(&self) -> &str {
        &self.inner.current_version
    }

    /// Returns the cached update info if a newer version is available.
    pub fn available_update(&self) -> Option<UpdateInfo> {
        self.inner.update_info.read().clone()
    }

    /// Whether a check is currently in progress.
    pub fn is_checking(&self) -> bool {
        self.inner.checking.load(Ordering::Relaxed)
    }

    /// Whether an update download/install is in progress.
    pub fn is_updating(&self) -> bool {
        self.inner.updating.load(Ordering::Relaxed)
    }

    /// Check GitHub releases for a newer version.
    /// This is blocking — call from a background thread.
    pub fn check_for_updates(&self) -> Result<Option<UpdateInfo>> {
        if self.inner.checking.swap(true, Ordering::SeqCst) {
            bail!("Update check already in progress");
        }

        let result = self.do_check();

        self.inner.checking.store(false, Ordering::SeqCst);

        match &result {
            Ok(Some(info)) => {
                info!("Update available: {} -> {}", self.inner.current_version, info.version);
                *self.inner.update_info.write() = Some(info.clone());
            }
            Ok(None) => {
                info!("No update available (current: {})", self.inner.current_version);
                *self.inner.update_info.write() = None;
            }
            Err(e) => {
                warn!("Update check failed: {e}");
            }
        }

        result
    }

    fn do_check(&self) -> Result<Option<UpdateInfo>> {
        let url = format!(
            "https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
        );

        // Use a blocking reqwest client (we're on a background thread).
        let client = reqwest::blocking::Client::builder()
            .user_agent("hive-updater/1.0")
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        let resp = client.get(&url).send().context("Failed to reach GitHub")?;

        if !resp.status().is_success() {
            bail!("GitHub API returned status {}", resp.status());
        }

        let body: serde_json::Value = resp.json().context("Failed to parse response")?;

        let tag = body["tag_name"]
            .as_str()
            .context("No tag_name in response")?;

        let remote_version = tag.strip_prefix('v').unwrap_or(tag);

        if !is_newer(remote_version, &self.inner.current_version) {
            return Ok(None);
        }

        // Find the right asset for this platform.
        let asset_name = platform_asset_name();
        let assets = body["assets"].as_array().context("No assets array")?;

        let asset_url = assets
            .iter()
            .find(|a| a["name"].as_str() == Some(asset_name))
            .and_then(|a| a["browser_download_url"].as_str())
            .map(String::from)
            .context(format!("Asset {asset_name} not found in release"))?;

        let release_url = body["html_url"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let release_notes = body["body"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(Some(UpdateInfo {
            version: remote_version.to_string(),
            release_url,
            asset_url,
            release_notes,
        }))
    }

    /// Download and install the update. Blocking — call from a background thread.
    /// Returns the path to the new binary (caller may need to restart).
    pub fn install_update(&self) -> Result<PathBuf> {
        let info = self.available_update()
            .context("No update available to install")?;

        if self.inner.updating.swap(true, Ordering::SeqCst) {
            bail!("Update installation already in progress");
        }

        let result = self.do_install(&info);

        self.inner.updating.store(false, Ordering::SeqCst);

        result
    }

    fn do_install(&self, info: &UpdateInfo) -> Result<PathBuf> {
        info!("Downloading update v{}...", info.version);

        let client = reqwest::blocking::Client::builder()
            .user_agent("hive-updater/1.0")
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .context("Failed to build HTTP client")?;

        let resp = client.get(&info.asset_url)
            .send()
            .context("Failed to download update")?;

        if !resp.status().is_success() {
            bail!("Download failed with status {}", resp.status());
        }

        let bytes = resp.bytes().context("Failed to read response body")?;

        let current_exe = std::env::current_exe()
            .context("Cannot determine current executable path")?;

        let tmp_dir = tempfile::tempdir()
            .context("Failed to create temp directory")?;

        // Platform-specific extraction and installation.
        #[cfg(target_os = "macos")]
        {
            self.install_macos(&bytes, &current_exe, tmp_dir.path())?;
        }

        #[cfg(target_os = "linux")]
        {
            self.install_linux(&bytes, &current_exe, tmp_dir.path())?;
        }

        #[cfg(target_os = "windows")]
        {
            self.install_windows(&bytes, &current_exe, tmp_dir.path())?;
        }

        info!("Update v{} installed successfully", info.version);
        Ok(current_exe)
    }

    #[cfg(target_os = "macos")]
    fn install_macos(&self, data: &[u8], current_exe: &std::path::Path, tmp: &std::path::Path) -> Result<()> {
        // First try Homebrew upgrade if available.
        if let Ok(output) = std::process::Command::new("brew")
            .args(["upgrade", "PatSul/tap/hive"])
            .output()
        {
            if output.status.success() {
                info!("Updated via Homebrew");
                return Ok(());
            }
            warn!("Homebrew upgrade failed, falling back to binary swap");
        }

        // Fallback: extract tar.gz and replace binary.
        let archive_path = tmp.join("update.tar.gz");
        std::fs::write(&archive_path, data).context("Failed to write archive")?;

        let status = std::process::Command::new("tar")
            .args(["xzf", &archive_path.to_string_lossy()])
            .current_dir(tmp)
            .status()
            .context("Failed to run tar")?;

        if !status.success() {
            bail!("tar extraction failed");
        }

        let new_binary = tmp.join("hive");
        if !new_binary.exists() {
            bail!("Extracted binary not found");
        }

        // Swap the binary: rename old, copy new, delete old.
        let backup = current_exe.with_extension("old");
        std::fs::rename(current_exe, &backup)
            .context("Failed to back up current binary")?;

        if let Err(e) = std::fs::copy(&new_binary, current_exe) {
            // Restore backup on failure.
            let _ = std::fs::rename(&backup, current_exe);
            bail!("Failed to install new binary: {e}");
        }

        // Remove quarantine attribute.
        let _ = std::process::Command::new("xattr")
            .args(["-dr", "com.apple.quarantine", &current_exe.to_string_lossy()])
            .output();

        let _ = std::fs::remove_file(&backup);
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn install_linux(&self, data: &[u8], current_exe: &std::path::Path, tmp: &std::path::Path) -> Result<()> {
        let archive_path = tmp.join("update.tar.gz");
        std::fs::write(&archive_path, data).context("Failed to write archive")?;

        let status = std::process::Command::new("tar")
            .args(["xzf", &archive_path.to_string_lossy()])
            .current_dir(tmp)
            .status()
            .context("Failed to run tar")?;

        if !status.success() {
            bail!("tar extraction failed");
        }

        let new_binary = tmp.join("hive");
        if !new_binary.exists() {
            bail!("Extracted binary not found");
        }

        let backup = current_exe.with_extension("old");
        std::fs::rename(current_exe, &backup)
            .context("Failed to back up current binary")?;

        if let Err(e) = std::fs::copy(&new_binary, current_exe) {
            let _ = std::fs::rename(&backup, current_exe);
            bail!("Failed to install new binary: {e}");
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(current_exe, std::fs::Permissions::from_mode(0o755));
        }

        let _ = std::fs::remove_file(&backup);
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn install_windows(&self, data: &[u8], current_exe: &std::path::Path, tmp: &std::path::Path) -> Result<()> {
        let archive_path = tmp.join("update.zip");
        std::fs::write(&archive_path, data).context("Failed to write archive")?;

        // Use PowerShell to extract on Windows.
        let status = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    archive_path.display(),
                    tmp.display()
                ),
            ])
            .status()
            .context("Failed to run PowerShell")?;

        if !status.success() {
            bail!("zip extraction failed");
        }

        let new_binary = tmp.join("hive.exe");
        if !new_binary.exists() {
            bail!("Extracted binary not found");
        }

        // On Windows, we can't replace a running exe directly.
        // Rename current to .old, copy new, schedule .old for deletion on reboot.
        let backup = current_exe.with_extension("exe.old");
        std::fs::rename(current_exe, &backup)
            .context("Failed to back up current binary")?;

        if let Err(e) = std::fs::copy(&new_binary, current_exe) {
            let _ = std::fs::rename(&backup, current_exe);
            bail!("Failed to install new binary: {e}");
        }

        // Try to clean up the old binary (may fail if still in use).
        let _ = std::fs::remove_file(&backup);
        Ok(())
    }
}

/// Compare two semver-like version strings, return true if `remote > local`.
fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |v: &str| -> (u32, u32, u32) {
        let parts: Vec<&str> = v.split('.').collect();
        let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        (major, minor, patch)
    };

    let r = parse(remote);
    let l = parse(local);

    r > l
}

/// The expected asset filename for the current platform.
fn platform_asset_name() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    { "hive-macos-arm64.tar.gz" }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { "hive-linux-x64.tar.gz" }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { "hive-windows-x64.zip" }

    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    { "hive-unknown-platform" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        assert!(is_newer("0.3.0", "0.2.1"));
        assert!(is_newer("0.2.2", "0.2.1"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.2.1", "0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.1"));
        assert!(!is_newer("0.1.0", "0.2.1"));
    }

    #[test]
    fn test_platform_asset_name() {
        let name = platform_asset_name();
        assert!(!name.is_empty());
        assert!(name.contains("hive"));
    }
}
