use anyhow::{Context, Result};
use git2::{DiffOptions, Repository, StatusOptions};
use std::path::{Path, PathBuf};
use tracing::debug;

/// Status classification for a file in a git repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatusType {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

/// A file and its git status.
#[derive(Debug, Clone)]
pub struct GitFileStatus {
    pub path: PathBuf,
    pub status: FileStatusType,
}

/// A single entry from the git log.
#[derive(Debug, Clone)]
pub struct GitLogEntry {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub timestamp: i64,
}

/// Git operations service wrapping a `git2::Repository`.
pub struct GitService {
    repo: Repository,
}

impl GitService {
    /// Open an existing git repository at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::open(path)
            .with_context(|| format!("Failed to open git repo at {}", path.display()))?;
        debug!("Opened git repo at {}", path.display());
        Ok(Self { repo })
    }

    /// Initialize a new git repository at the given path.
    pub fn init(path: &Path) -> Result<Self> {
        let repo = Repository::init(path)
            .with_context(|| format!("Failed to init git repo at {}", path.display()))?;
        debug!("Initialized git repo at {}", path.display());
        Ok(Self { repo })
    }

    /// Get the status of all files in the working directory.
    pub fn status(&self) -> Result<Vec<GitFileStatus>> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);

        let statuses = self
            .repo
            .statuses(Some(&mut opts))
            .context("Failed to get git status")?;

        let mut result = Vec::new();
        for entry in statuses.iter() {
            let path = match entry.path() {
                Some(p) => PathBuf::from(p),
                None => continue,
            };
            let status = entry.status();

            let file_status = if status.is_index_new() || status.is_wt_new() {
                if status.is_wt_new() && !status.is_index_new() {
                    FileStatusType::Untracked
                } else {
                    FileStatusType::Added
                }
            } else if status.is_index_deleted() || status.is_wt_deleted() {
                FileStatusType::Deleted
            } else if status.is_index_renamed() || status.is_wt_renamed() {
                FileStatusType::Renamed
            } else if status.is_index_modified() || status.is_wt_modified() {
                FileStatusType::Modified
            } else {
                continue;
            };

            result.push(GitFileStatus {
                path,
                status: file_status,
            });
        }

        Ok(result)
    }

    /// Generate a diff of the working directory against HEAD.
    pub fn diff(&self) -> Result<String> {
        let head_tree = self
            .repo
            .head()
            .ok()
            .and_then(|head| head.peel_to_tree().ok());

        let mut diff_opts = DiffOptions::new();
        let diff = self
            .repo
            .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))
            .context("Failed to generate diff")?;

        let mut output = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let prefix = match line.origin() {
                '+' => "+",
                '-' => "-",
                ' ' => " ",
                _ => "",
            };
            let content = std::str::from_utf8(line.content()).unwrap_or("");
            output.push_str(prefix);
            output.push_str(content);
            true
        })
        .context("Failed to format diff")?;

        Ok(output)
    }

    /// Stage files by their paths (relative to the repo root).
    pub fn stage(&self, paths: &[&Path]) -> Result<()> {
        let mut index = self.repo.index().context("Failed to get index")?;

        for path in paths {
            index
                .add_path(path)
                .with_context(|| format!("Failed to stage: {}", path.display()))?;
        }

        index.write().context("Failed to write index")?;
        debug!("Staged {} file(s)", paths.len());
        Ok(())
    }

    /// Unstage files by resetting them to HEAD.
    ///
    /// Uses `reset_default` which is equivalent to `git reset HEAD -- <paths>`.
    pub fn unstage(&self, paths: &[&Path]) -> Result<()> {
        let head_obj = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.peel(git2::ObjectType::Commit).ok());

        let path_strings: Vec<&str> = paths
            .iter()
            .map(|p| p.to_str().unwrap_or(""))
            .collect();

        self.repo
            .reset_default(head_obj.as_ref(), path_strings)
            .context("Failed to unstage files")?;

        debug!("Unstaged {} file(s)", paths.len());
        Ok(())
    }

    /// Create a commit with the currently staged changes. Returns the commit hash.
    pub fn commit(&self, message: &str) -> Result<String> {
        let mut index = self.repo.index().context("Failed to get index")?;
        let tree_oid = index.write_tree().context("Failed to write tree")?;
        let tree = self
            .repo
            .find_tree(tree_oid)
            .context("Failed to find tree")?;

        let signature = self
            .repo
            .signature()
            .context("Failed to get git signature. Configure user.name and user.email.")?;

        let parent_commit = self.repo.head().ok().and_then(|h| h.peel_to_commit().ok());

        let parents: Vec<&git2::Commit<'_>> = match &parent_commit {
            Some(c) => vec![c],
            None => vec![],
        };

        let oid = self
            .repo
            .commit(Some("HEAD"), &signature, &signature, message, &tree, &parents)
            .context("Failed to create commit")?;

        let hash = oid.to_string();
        debug!("Created commit: {}", &hash[..8]);
        Ok(hash)
    }

    /// Get the name of the current branch.
    pub fn current_branch(&self) -> Result<String> {
        let head = self.repo.head().context("Failed to get HEAD")?;

        if head.is_branch() {
            let name = head.shorthand().unwrap_or("HEAD").to_string();
            Ok(name)
        } else {
            // Detached HEAD -- return the short OID
            let oid = head.target().context("HEAD has no target")?;
            Ok(format!("detached@{}", &oid.to_string()[..8]))
        }
    }

    /// Get the commit log, most recent first.
    pub fn log(&self, max_count: usize) -> Result<Vec<GitLogEntry>> {
        let mut revwalk = self.repo.revwalk().context("Failed to create revwalk")?;
        revwalk.push_head().context("Failed to push HEAD to revwalk")?;
        revwalk
            .set_sorting(git2::Sort::TIME)
            .context("Failed to set sort order")?;

        let mut entries = Vec::new();
        for oid_result in revwalk {
            if entries.len() >= max_count {
                break;
            }
            let oid = oid_result.context("Failed to iterate revwalk")?;
            let commit = self
                .repo
                .find_commit(oid)
                .with_context(|| format!("Failed to find commit {oid}"))?;

            entries.push(GitLogEntry {
                hash: oid.to_string(),
                message: commit.message().unwrap_or("").trim().to_string(),
                author: commit.author().name().unwrap_or("unknown").to_string(),
                timestamp: commit.time().seconds(),
            });
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_repo() -> (TempDir, GitService) {
        let dir = tempfile::tempdir().unwrap();
        let git = GitService::init(dir.path()).unwrap();

        // Configure user for commits
        let mut config = git.repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();

        (dir, git)
    }

    #[test]
    fn test_init_and_open() {
        let dir = tempfile::tempdir().unwrap();
        GitService::init(dir.path()).unwrap();
        GitService::open(dir.path()).unwrap();
    }

    #[test]
    fn test_status_untracked() {
        let (dir, git) = setup_repo();
        fs::write(dir.path().join("new.txt"), "new file").unwrap();

        let statuses = git.status().unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].status, FileStatusType::Untracked);
    }

    #[test]
    fn test_stage_and_commit() {
        let (dir, git) = setup_repo();
        fs::write(dir.path().join("file.txt"), "content").unwrap();

        git.stage(&[Path::new("file.txt")]).unwrap();
        let hash = git.commit("initial commit").unwrap();
        assert_eq!(hash.len(), 40); // SHA-1 hex
    }

    #[test]
    fn test_current_branch() {
        let (dir, git) = setup_repo();
        fs::write(dir.path().join("init.txt"), "init").unwrap();
        git.stage(&[Path::new("init.txt")]).unwrap();
        git.commit("init").unwrap();

        let branch = git.current_branch().unwrap();
        assert!(!branch.is_empty());
    }

    #[test]
    fn test_log() {
        let (dir, git) = setup_repo();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        git.stage(&[Path::new("a.txt")]).unwrap();
        git.commit("first").unwrap();

        fs::write(dir.path().join("b.txt"), "b").unwrap();
        git.stage(&[Path::new("b.txt")]).unwrap();
        git.commit("second").unwrap();

        let log = git.log(10).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].message, "second");
        assert_eq!(log[1].message, "first");
    }

    #[test]
    fn test_diff() {
        let (dir, git) = setup_repo();
        fs::write(dir.path().join("file.txt"), "line1\n").unwrap();
        git.stage(&[Path::new("file.txt")]).unwrap();
        git.commit("initial").unwrap();

        fs::write(dir.path().join("file.txt"), "line1\nline2\n").unwrap();
        let diff = git.diff().unwrap();
        assert!(diff.contains("+line2"));
    }

    #[test]
    fn test_status_modified() {
        let (dir, git) = setup_repo();
        fs::write(dir.path().join("file.txt"), "original").unwrap();
        git.stage(&[Path::new("file.txt")]).unwrap();
        git.commit("init").unwrap();

        fs::write(dir.path().join("file.txt"), "modified").unwrap();
        let statuses = git.status().unwrap();
        assert!(statuses.iter().any(|s| s.status == FileStatusType::Modified));
    }
}
