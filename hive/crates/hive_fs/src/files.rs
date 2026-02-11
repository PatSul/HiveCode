use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::debug;

/// Maximum file size allowed for reads (10 MB).
const MAX_READ_SIZE: u64 = 10 * 1024 * 1024;

/// Sensitive directory segments that are blocked from access.
const BLOCKED_SEGMENTS: &[&str] = &[".ssh", ".aws", ".gnupg", ".config/gcloud"];

/// Represents a single directory entry with metadata.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// File metadata stats.
#[derive(Debug, Clone)]
pub struct FileStats {
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
}

/// File operations service with built-in path validation.
pub struct FileService;

impl FileService {
    /// Read a file's contents as a UTF-8 string.
    ///
    /// Validates the path and enforces a 10 MB size limit.
    pub fn read_file(path: &Path) -> Result<String> {
        validate_path(path)?;
        let canonical = path
            .canonicalize()
            .with_context(|| format!("Cannot resolve path: {}", path.display()))?;
        validate_canonical(&canonical)?;

        let metadata = std::fs::metadata(&canonical)
            .with_context(|| format!("Cannot stat file: {}", canonical.display()))?;

        if metadata.len() > MAX_READ_SIZE {
            bail!(
                "File too large ({} bytes, max {} bytes): {}",
                metadata.len(),
                MAX_READ_SIZE,
                canonical.display()
            );
        }

        debug!("Reading file: {}", canonical.display());
        std::fs::read_to_string(&canonical)
            .with_context(|| format!("Failed to read file: {}", canonical.display()))
    }

    /// Write content to a file, creating parent directories as needed.
    pub fn write_file(path: &Path, content: &str) -> Result<()> {
        validate_path(path)?;

        // For writes, also block the hive config file
        let path_str = normalize_path_str(path);
        if path_str.contains(".hive/config.json") || path_str.contains(".hive\\config.json") {
            bail!("Writing to .hive/config.json is blocked for safety");
        }

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }
        }

        // If the path already exists, validate its canonical form too
        if path.exists() {
            let canonical = path.canonicalize()?;
            validate_canonical(&canonical)?;
        }

        debug!("Writing file: {}", path.display());
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write file: {}", path.display()))
    }

    /// Delete a file.
    pub fn delete_file(path: &Path) -> Result<()> {
        validate_path(path)?;
        let canonical = path
            .canonicalize()
            .with_context(|| format!("Cannot resolve path: {}", path.display()))?;
        validate_canonical(&canonical)?;

        debug!("Deleting file: {}", canonical.display());
        std::fs::remove_file(&canonical)
            .with_context(|| format!("Failed to delete file: {}", canonical.display()))
    }

    /// List the contents of a directory.
    pub fn list_dir(path: &Path) -> Result<Vec<DirEntry>> {
        validate_path(path)?;
        let canonical = path
            .canonicalize()
            .with_context(|| format!("Cannot resolve path: {}", path.display()))?;
        validate_canonical(&canonical)?;

        let mut entries = Vec::new();
        let read_dir = std::fs::read_dir(&canonical)
            .with_context(|| format!("Failed to read directory: {}", canonical.display()))?;

        for entry in read_dir {
            let entry = entry?;
            let metadata = entry.metadata()?;
            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
                modified: metadata.modified().ok(),
            });
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    /// Rename or move a file/directory.
    pub fn rename(from: &Path, to: &Path) -> Result<()> {
        validate_path(from)?;
        validate_path(to)?;
        let canonical_from = from
            .canonicalize()
            .with_context(|| format!("Cannot resolve source: {}", from.display()))?;
        validate_canonical(&canonical_from)?;

        debug!("Renaming {} -> {}", from.display(), to.display());
        std::fs::rename(&canonical_from, to)
            .with_context(|| format!("Failed to rename {} -> {}", from.display(), to.display()))
    }

    /// Get metadata/stats for a file or directory.
    pub fn file_stats(path: &Path) -> Result<FileStats> {
        validate_path(path)?;
        let canonical = path
            .canonicalize()
            .with_context(|| format!("Cannot resolve path: {}", path.display()))?;
        validate_canonical(&canonical)?;

        let metadata = std::fs::symlink_metadata(&canonical)
            .with_context(|| format!("Cannot stat: {}", canonical.display()))?;

        Ok(FileStats {
            size: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            is_symlink: metadata.file_type().is_symlink(),
        })
    }

    /// Check whether a path is a directory.
    pub fn is_directory(path: &Path) -> bool {
        path.is_dir()
    }

    /// Read multiple files in one call. Returns a result per path.
    pub fn read_multiple(paths: &[PathBuf]) -> Vec<Result<(PathBuf, String)>> {
        paths
            .iter()
            .map(|path| {
                let content = Self::read_file(path)?;
                Ok((path.clone(), content))
            })
            .collect()
    }
}

/// Normalize a path to a forward-slash string for consistent checks.
fn normalize_path_str(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Validate a path before any I/O. Checks for system roots and sensitive dirs.
fn validate_path(path: &Path) -> Result<()> {
    let path_str = normalize_path_str(path);

    // Block system roots
    if is_system_root(&path_str) {
        bail!("Access to system root is blocked: {}", path.display());
    }

    // Block sensitive directories
    for segment in BLOCKED_SEGMENTS {
        if path_str.contains(segment) {
            bail!("Access to sensitive path blocked: {segment}");
        }
    }

    Ok(())
}

/// Validate a canonicalized path against the same rules (catches traversal).
fn validate_canonical(path: &Path) -> Result<()> {
    let path_str = normalize_path_str(path);

    if is_system_root(&path_str) {
        bail!(
            "Path resolves to system root: {}",
            path.display()
        );
    }

    for segment in BLOCKED_SEGMENTS {
        if path_str.contains(segment) {
            bail!(
                "Path traversal to sensitive directory blocked: {segment}"
            );
        }
    }

    Ok(())
}

/// Check whether a normalized path string is a system root.
fn is_system_root(path_str: &str) -> bool {
    let trimmed = path_str.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return true;
    }
    // Match Windows drive roots like "C:" or "C:/"
    if trimmed.len() <= 2 && trimmed.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic()) {
        if trimmed.len() == 2 && trimmed.ends_with(':') {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_write_and_read_file() {
        let dir = setup();
        let file = dir.path().join("hello.txt");
        FileService::write_file(&file, "hello world").unwrap();
        let content = FileService::read_file(&file).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_delete_file() {
        let dir = setup();
        let file = dir.path().join("deleteme.txt");
        fs::write(&file, "gone").unwrap();
        assert!(file.exists());
        FileService::delete_file(&file).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn test_list_dir() {
        let dir = setup();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::write(dir.path().join("b.txt"), "b").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();

        let entries = FileService::list_dir(dir.path()).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "a.txt");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[2].name, "sub");
        assert!(entries[2].is_dir);
    }

    #[test]
    fn test_rename() {
        let dir = setup();
        let from = dir.path().join("old.txt");
        let to = dir.path().join("new.txt");
        fs::write(&from, "data").unwrap();
        FileService::rename(&from, &to).unwrap();
        assert!(!from.exists());
        assert!(to.exists());
        assert_eq!(fs::read_to_string(&to).unwrap(), "data");
    }

    #[test]
    fn test_file_stats() {
        let dir = setup();
        let file = dir.path().join("stats.txt");
        fs::write(&file, "content").unwrap();

        let stats = FileService::file_stats(&file).unwrap();
        assert!(stats.is_file);
        assert!(!stats.is_dir);
        assert_eq!(stats.size, 7);
    }

    #[test]
    fn test_is_directory() {
        let dir = setup();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        assert!(FileService::is_directory(&sub));
        assert!(!FileService::is_directory(&dir.path().join("nope")));
    }

    #[test]
    fn test_read_multiple() {
        let dir = setup();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        let c = dir.path().join("missing.txt");
        fs::write(&a, "aaa").unwrap();
        fs::write(&b, "bbb").unwrap();

        let results = FileService::read_multiple(&[a.clone(), b.clone(), c]);
        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());
        assert!(results[2].is_err());
    }

    #[test]
    fn test_block_system_root() {
        let result = FileService::read_file(Path::new("/"));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("system root"));
    }

    #[test]
    fn test_block_sensitive_path() {
        let result = FileService::read_file(Path::new("/home/user/.ssh/id_rsa"));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains(".ssh"));
    }

    #[test]
    fn test_write_creates_parent_dirs() {
        let dir = setup();
        let nested = dir.path().join("a").join("b").join("c.txt");
        FileService::write_file(&nested, "deep").unwrap();
        assert_eq!(fs::read_to_string(&nested).unwrap(), "deep");
    }

    #[test]
    fn test_is_system_root_checks() {
        assert!(is_system_root("/"));
        assert!(is_system_root("C:"));
        assert!(!is_system_root("/home/user"));
        assert!(!is_system_root("C:/Users"));
    }
}
