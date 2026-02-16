//! Git Worktree Manager — swarm team isolation via git worktrees.
//!
//! Each swarm team gets its own worktree and branch, allowing parallel
//! development without conflicts. Teams work on `swarm/{run_id}/{team_id}`
//! branches and their changes can be merged back to a target branch when done.
//!
//! All worktrees are created under `.hive-worktrees/` in the repository root,
//! which should be added to `.gitignore`.

use git2::{BranchType, Repository};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A worktree assigned to a specific swarm team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamWorktree {
    /// Unique identifier for the team.
    pub team_id: String,
    /// The branch name this worktree tracks (e.g. `swarm/run-1/team-alpha`).
    pub branch_name: String,
    /// Filesystem path to the worktree directory.
    pub worktree_path: PathBuf,
}

/// Result of merging a team's branch into a target branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeBranchResult {
    /// Whether the merge completed without conflicts.
    pub success: bool,
    /// List of file paths that had conflicts (empty on success).
    pub conflicts: Vec<String>,
    /// The resulting commit hash if the merge succeeded.
    pub commit_hash: Option<String>,
}

// ---------------------------------------------------------------------------
// WorktreeManager
// ---------------------------------------------------------------------------

/// Manages git worktrees for swarm team isolation.
///
/// Each team gets a dedicated worktree under `.hive-worktrees/` with its own
/// branch. This allows multiple teams to work on the same repository in
/// parallel without stepping on each other's changes.
pub struct WorktreeManager {
    repo_path: PathBuf,
}

impl WorktreeManager {
    /// Create a new worktree manager for the repository at `repo_path`.
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    /// Base directory for all swarm worktrees.
    fn worktrees_dir(&self) -> PathBuf {
        self.repo_path.join(".hive-worktrees")
    }

    /// Validate that a path is safely under the worktrees directory.
    /// Returns the canonicalized path on success.
    fn validate_worktree_path(&self, path: &Path) -> Result<PathBuf, String> {
        let worktrees_dir = self.worktrees_dir();

        // Ensure the worktrees base directory exists for canonicalization.
        if !worktrees_dir.exists() {
            std::fs::create_dir_all(&worktrees_dir)
                .map_err(|e| format!("Failed to create worktrees dir: {e}"))?;
        }

        let canonical_base = worktrees_dir
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize worktrees dir: {e}"))?;

        // If the target path exists, canonicalize it directly.
        // Otherwise, canonicalize the parent and append the final component.
        let canonical_target = if path.exists() {
            path.canonicalize()
                .map_err(|e| format!("Failed to canonicalize path: {e}"))?
        } else {
            let parent = path
                .parent()
                .ok_or_else(|| "Path has no parent".to_string())?;
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent dir: {e}"))?;
            }
            let canonical_parent = parent
                .canonicalize()
                .map_err(|e| format!("Failed to canonicalize parent: {e}"))?;
            let file_name = path
                .file_name()
                .ok_or_else(|| "Path has no file name".to_string())?;
            canonical_parent.join(file_name)
        };

        if !canonical_target.starts_with(&canonical_base) {
            return Err(format!(
                "Path escapes worktrees directory: {} is not under {}",
                canonical_target.display(),
                canonical_base.display()
            ));
        }

        Ok(canonical_target)
    }

    /// Sanitize a string for use in a branch name: keep alphanumeric, dash,
    /// underscore, and forward slash.
    fn sanitize_branch_component(s: &str) -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect()
    }

    /// Create a worktree for a team with its own branch.
    ///
    /// The branch is named `swarm/{run_id}/{team_id}` and the worktree is
    /// placed at `.hive-worktrees/{team_id}/`.
    pub fn create_worktree(&self, run_id: &str, team_id: &str) -> Result<TeamWorktree, String> {
        let safe_run_id = Self::sanitize_branch_component(run_id);
        let safe_team_id = Self::sanitize_branch_component(team_id);

        if safe_run_id.is_empty() || safe_team_id.is_empty() {
            return Err("run_id and team_id must contain valid characters".into());
        }

        let branch_name = format!("swarm/{safe_run_id}/{safe_team_id}");
        let worktree_path = self.worktrees_dir().join(&safe_team_id);

        // Validate that the worktree path stays under .hive-worktrees/.
        let validated_path = self.validate_worktree_path(&worktree_path)?;

        info!(
            branch = %branch_name,
            path = %validated_path.display(),
            "Creating worktree for team"
        );

        // Open the repository.
        let repo = Repository::open(&self.repo_path)
            .map_err(|e| format!("Failed to open repository: {e}"))?;

        // Get the HEAD commit to branch from.
        let head = repo
            .head()
            .map_err(|e| format!("Failed to get HEAD: {e}"))?;
        let head_commit = head
            .peel_to_commit()
            .map_err(|e| format!("Failed to peel HEAD to commit: {e}"))?;

        // Create the branch from HEAD.
        repo.branch(&branch_name, &head_commit, false)
            .map_err(|e| format!("Failed to create branch '{branch_name}': {e}"))?;

        info!(branch = %branch_name, "Created branch from HEAD");

        // Create the worktree directory if it doesn't exist.
        if !validated_path.exists() {
            std::fs::create_dir_all(&validated_path)
                .map_err(|e| format!("Failed to create worktree directory: {e}"))?;
        }

        // Add the worktree via git2. The worktree is linked to the new branch.
        let branch_ref = format!("refs/heads/{branch_name}");
        let reference = repo
            .find_reference(&branch_ref)
            .map_err(|e| format!("Failed to find branch reference: {e}"))?;

        // Remove the directory first — git2 worktree_add expects it not to exist.
        if validated_path.exists() {
            std::fs::remove_dir_all(&validated_path)
                .map_err(|e| format!("Failed to clean worktree directory: {e}"))?;
        }

        repo.worktree(
            &safe_team_id,
            &validated_path,
            Some(git2::WorktreeAddOptions::new().reference(Some(&reference))),
        )
        .map_err(|e| format!("Failed to add worktree: {e}"))?;

        info!(
            team_id = %safe_team_id,
            path = %validated_path.display(),
            "Worktree created successfully"
        );

        Ok(TeamWorktree {
            team_id: safe_team_id,
            branch_name,
            worktree_path: validated_path,
        })
    }

    /// Merge a team's branch into a target branch.
    ///
    /// Performs merge analysis and attempts a fast-forward or normal merge.
    /// If conflicts are detected, returns `MergeBranchResult` with
    /// `success = false` and the list of conflicting paths.
    pub fn merge_team_branch(
        &self,
        team_branch: &str,
        target_branch: &str,
    ) -> Result<MergeBranchResult, String> {
        // Never allow merging into or deleting protected branches by accident.
        if team_branch == "main" || team_branch == "master" {
            return Err("Cannot use main/master as a team branch".into());
        }

        info!(
            from = %team_branch,
            into = %target_branch,
            "Merging team branch"
        );

        let repo = Repository::open(&self.repo_path)
            .map_err(|e| format!("Failed to open repository: {e}"))?;

        // Find the source branch (team branch).
        let source_branch = repo
            .find_branch(team_branch, BranchType::Local)
            .map_err(|e| format!("Failed to find team branch '{team_branch}': {e}"))?;

        let source_ref = source_branch.into_reference();
        let source_commit_oid = source_ref
            .target()
            .ok_or_else(|| format!("Team branch '{team_branch}' has no target"))?;

        let source_annotated = repo
            .find_annotated_commit(source_commit_oid)
            .map_err(|e| format!("Failed to find annotated commit: {e}"))?;

        // Find the target branch.
        let target_branch_ref = repo
            .find_branch(target_branch, BranchType::Local)
            .map_err(|e| format!("Failed to find target branch '{target_branch}': {e}"))?;

        let target_ref = target_branch_ref.into_reference();
        let target_commit_oid = target_ref
            .target()
            .ok_or_else(|| format!("Target branch '{target_branch}' has no target"))?;

        // Perform merge analysis.
        let (analysis, _preference) = repo
            .merge_analysis(&[&source_annotated])
            .map_err(|e| format!("Merge analysis failed: {e}"))?;

        if analysis.is_up_to_date() {
            info!("Target branch is already up to date");
            let hash = format!("{target_commit_oid}");
            return Ok(MergeBranchResult {
                success: true,
                conflicts: vec![],
                commit_hash: Some(hash),
            });
        }

        if analysis.is_fast_forward() {
            info!("Performing fast-forward merge");

            // Fast-forward: just move the target branch ref to the source commit.
            let refname = format!("refs/heads/{target_branch}");
            repo.find_reference(&refname)
                .map_err(|e| format!("Failed to find target ref: {e}"))?
                .set_target(
                    source_commit_oid,
                    &format!("hive: fast-forward merge {team_branch} into {target_branch}"),
                )
                .map_err(|e| format!("Failed to fast-forward: {e}"))?;

            let hash = format!("{source_commit_oid}");
            info!(commit = %hash, "Fast-forward merge complete");

            return Ok(MergeBranchResult {
                success: true,
                conflicts: vec![],
                commit_hash: Some(hash),
            });
        }

        if analysis.is_normal() {
            info!("Performing normal merge");

            // Get the two commit trees for merging.
            let source_commit = repo
                .find_commit(source_commit_oid)
                .map_err(|e| format!("Failed to find source commit: {e}"))?;
            let target_commit = repo
                .find_commit(target_commit_oid)
                .map_err(|e| format!("Failed to find target commit: {e}"))?;

            let ancestor = repo
                .find_commit(
                    repo.merge_base(source_commit_oid, target_commit_oid)
                        .map_err(|e| format!("Failed to find merge base: {e}"))?,
                )
                .map_err(|e| format!("Failed to find ancestor commit: {e}"))?;

            let mut index = repo
                .merge_trees(
                    &ancestor.tree().map_err(|e| format!("ancestor tree: {e}"))?,
                    &target_commit
                        .tree()
                        .map_err(|e| format!("target tree: {e}"))?,
                    &source_commit
                        .tree()
                        .map_err(|e| format!("source tree: {e}"))?,
                    None,
                )
                .map_err(|e| format!("Merge failed: {e}"))?;

            // Check for conflicts.
            if index.has_conflicts() {
                let conflicts: Vec<String> = index
                    .conflicts()
                    .map_err(|e| format!("Failed to read conflicts: {e}"))?
                    .filter_map(|entry| {
                        let entry = entry.ok()?;
                        // Use whichever side has a path.
                        let path = entry
                            .our
                            .as_ref()
                            .or(entry.their.as_ref())
                            .or(entry.ancestor.as_ref())?;
                        String::from_utf8(path.path.clone()).ok()
                    })
                    .collect();

                warn!(count = conflicts.len(), "Merge has conflicts");

                return Ok(MergeBranchResult {
                    success: false,
                    conflicts,
                    commit_hash: None,
                });
            }

            // Write the merged tree and create a merge commit.
            let tree_oid = index
                .write_tree_to(&repo)
                .map_err(|e| format!("Failed to write merged tree: {e}"))?;
            let merged_tree = repo
                .find_tree(tree_oid)
                .map_err(|e| format!("Failed to find merged tree: {e}"))?;

            let sig = repo
                .signature()
                .unwrap_or_else(|_| git2::Signature::now("Hive Swarm", "hive@localhost")
                    .expect("static signature should never fail"));

            let message = format!("hive: merge {team_branch} into {target_branch}");

            let merge_oid = repo
                .commit(
                    Some(&format!("refs/heads/{target_branch}")),
                    &sig,
                    &sig,
                    &message,
                    &merged_tree,
                    &[&target_commit, &source_commit],
                )
                .map_err(|e| format!("Failed to create merge commit: {e}"))?;

            let hash = format!("{merge_oid}");
            info!(commit = %hash, "Normal merge complete");

            return Ok(MergeBranchResult {
                success: true,
                conflicts: vec![],
                commit_hash: Some(hash),
            });
        }

        Err("Merge analysis returned unhandled state".into())
    }

    /// Remove a team's worktree and optionally delete its branch.
    ///
    /// This removes the worktree directory from disk, prunes stale worktree
    /// references, and optionally deletes the associated branch.
    pub fn cleanup_worktree(&self, team_id: &str, delete_branch: bool) -> Result<(), String> {
        let safe_team_id = Self::sanitize_branch_component(team_id);
        if safe_team_id.is_empty() {
            return Err("Invalid team_id".into());
        }

        let worktree_path = self.worktrees_dir().join(&safe_team_id);

        info!(
            team_id = %safe_team_id,
            path = %worktree_path.display(),
            delete_branch,
            "Cleaning up worktree"
        );

        // Remove the worktree directory if it exists.
        if worktree_path.exists() {
            // Validate path before removal.
            self.validate_worktree_path(&worktree_path)?;

            std::fs::remove_dir_all(&worktree_path)
                .map_err(|e| format!("Failed to remove worktree directory: {e}"))?;

            info!(path = %worktree_path.display(), "Removed worktree directory");
        }

        // Open repo and prune worktrees.
        let repo = Repository::open(&self.repo_path)
            .map_err(|e| format!("Failed to open repository: {e}"))?;

        // Prune the worktree reference by looking it up and pruning if valid.
        if let Ok(wt) = repo.find_worktree(&safe_team_id) {
            // Prune with flags to handle locked/valid worktrees.
            let mut flags = git2::WorktreePruneOptions::new();
            flags.working_tree(true);
            flags.valid(true);

            if let Err(e) = wt.prune(Some(&mut flags)) {
                warn!(error = %e, "Failed to prune worktree (may already be removed)");
            }
        }

        // Delete the associated branch if requested.
        if delete_branch {
            self.delete_team_branch(&repo, &safe_team_id)?;
        }

        Ok(())
    }

    /// Find and delete a branch associated with a team.
    /// Searches for branches matching `swarm/*/team_id`.
    /// Never deletes main or master.
    fn delete_team_branch(&self, repo: &Repository, team_id: &str) -> Result<(), String> {
        let branches = repo
            .branches(Some(BranchType::Local))
            .map_err(|e| format!("Failed to list branches: {e}"))?;

        for branch_result in branches {
            let (mut branch, _) =
                branch_result.map_err(|e| format!("Failed to read branch: {e}"))?;

            let name = match branch.name() {
                Ok(Some(n)) => n.to_string(),
                _ => continue,
            };

            // Match branches like swarm/{run_id}/{team_id}.
            if name.ends_with(&format!("/{team_id}")) && name.starts_with("swarm/") {
                // Never delete main/master.
                if name == "main" || name == "master" {
                    warn!(branch = %name, "Refusing to delete protected branch");
                    continue;
                }

                info!(branch = %name, "Deleting team branch");
                branch
                    .delete()
                    .map_err(|e| format!("Failed to delete branch '{name}': {e}"))?;
            }
        }

        Ok(())
    }

    /// Clean up all worktrees for a completed swarm run.
    ///
    /// Finds all branches matching `swarm/{run_id}/*`, cleans up their
    /// worktrees and directories, and deletes the branches.
    /// Returns the number of worktrees cleaned.
    pub fn cleanup_swarm(&self, run_id: &str) -> Result<usize, String> {
        let safe_run_id = Self::sanitize_branch_component(run_id);
        if safe_run_id.is_empty() {
            return Err("Invalid run_id".into());
        }

        info!(run_id = %safe_run_id, "Cleaning up swarm worktrees");

        let repo = Repository::open(&self.repo_path)
            .map_err(|e| format!("Failed to open repository: {e}"))?;

        let prefix = format!("swarm/{safe_run_id}/");
        let mut cleaned = 0;

        // Collect branch names first to avoid borrow issues.
        let branch_names: Vec<String> = {
            let branches = repo
                .branches(Some(BranchType::Local))
                .map_err(|e| format!("Failed to list branches: {e}"))?;

            branches
                .filter_map(|b| {
                    let (branch, _) = b.ok()?;
                    let name = branch.name().ok()??.to_string();
                    if name.starts_with(&prefix) {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect()
        };

        for branch_name in &branch_names {
            // Extract team_id from branch name: swarm/{run_id}/{team_id}
            let team_id = match branch_name.strip_prefix(&prefix) {
                Some(tid) => tid.to_string(),
                None => continue,
            };

            // Clean up the worktree directory.
            let worktree_path = self.worktrees_dir().join(&team_id);
            if worktree_path.exists()
                && let Ok(_validated) = self.validate_worktree_path(&worktree_path)
                    && let Err(e) = std::fs::remove_dir_all(&worktree_path) {
                        warn!(
                            path = %worktree_path.display(),
                            error = %e,
                            "Failed to remove worktree directory"
                        );
                    }

            // Prune the worktree reference.
            if let Ok(wt) = repo.find_worktree(&team_id) {
                let mut flags = git2::WorktreePruneOptions::new();
                flags.working_tree(true);
                flags.valid(true);
                let _ = wt.prune(Some(&mut flags));
            }

            // Delete the branch (never main/master).
            if branch_name != "main" && branch_name != "master"
                && let Ok(mut branch) = repo.find_branch(branch_name, BranchType::Local) {
                    if let Err(e) = branch.delete() {
                        warn!(
                            branch = %branch_name,
                            error = %e,
                            "Failed to delete branch"
                        );
                    } else {
                        info!(branch = %branch_name, "Deleted swarm branch");
                    }
                }

            cleaned += 1;
        }

        // Try to remove the worktrees base directory if empty.
        let worktrees_dir = self.worktrees_dir();
        if worktrees_dir.exists() {
            // Only remove if the directory is empty.
            if let Ok(mut entries) = std::fs::read_dir(&worktrees_dir)
                && entries.next().is_none() {
                    let _ = std::fs::remove_dir(&worktrees_dir);
                }
        }

        info!(count = cleaned, run_id = %safe_run_id, "Swarm cleanup complete");
        Ok(cleaned)
    }

    /// List all active team worktrees.
    ///
    /// Reads the `.hive-worktrees/` directory and matches entries with
    /// existing git branches to build the list.
    pub fn list_worktrees(&self) -> Result<Vec<TeamWorktree>, String> {
        let worktrees_dir = self.worktrees_dir();

        if !worktrees_dir.exists() {
            return Ok(vec![]);
        }

        let repo = Repository::open(&self.repo_path)
            .map_err(|e| format!("Failed to open repository: {e}"))?;

        let entries = std::fs::read_dir(&worktrees_dir)
            .map_err(|e| format!("Failed to read worktrees directory: {e}"))?;

        let mut worktrees = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let team_id = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            // Find a matching branch (swarm/*/{team_id}).
            let branch_name = self.find_branch_for_team(&repo, &team_id);

            if let Some(branch_name) = branch_name {
                worktrees.push(TeamWorktree {
                    team_id,
                    branch_name,
                    worktree_path: path,
                });
            }
        }

        Ok(worktrees)
    }

    /// Find a branch matching `swarm/*/{team_id}` pattern.
    fn find_branch_for_team(&self, repo: &Repository, team_id: &str) -> Option<String> {
        let branches = repo.branches(Some(BranchType::Local)).ok()?;
        let suffix = format!("/{team_id}");

        for branch_result in branches {
            if let Ok((branch, _)) = branch_result
                && let Ok(Some(name)) = branch.name()
                    && name.starts_with("swarm/") && name.ends_with(&suffix) {
                        return Some(name.to_string());
                    }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a temporary git repository with an initial commit.
    fn setup_test_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        // Create initial commit so HEAD exists.
        {
            let sig = repo
                .signature()
                .unwrap_or_else(|_| git2::Signature::now("Test", "test@test.com").unwrap());
            let tree_id = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .unwrap();
        }

        (dir, repo)
    }

    #[test]
    fn create_worktree_creates_branch_and_directory() {
        let (dir, _repo) = setup_test_repo();
        let manager = WorktreeManager::new(dir.path());

        let result = manager.create_worktree("run-1", "team-alpha");
        assert!(result.is_ok(), "create_worktree failed: {:?}", result.err());

        let wt = result.unwrap();
        assert_eq!(wt.team_id, "team-alpha");
        assert_eq!(wt.branch_name, "swarm/run-1/team-alpha");
        assert!(wt.worktree_path.exists(), "Worktree directory should exist");

        // Verify the branch exists in the repo.
        let repo = Repository::open(dir.path()).unwrap();
        let branch = repo.find_branch("swarm/run-1/team-alpha", BranchType::Local);
        assert!(branch.is_ok(), "Branch should exist in repository");
    }

    #[test]
    fn cleanup_worktree_removes_directory() {
        let (dir, _repo) = setup_test_repo();
        let manager = WorktreeManager::new(dir.path());

        // Create a worktree first.
        let wt = manager.create_worktree("run-2", "team-beta").unwrap();
        assert!(wt.worktree_path.exists());

        // Clean it up.
        let result = manager.cleanup_worktree("team-beta", true);
        assert!(
            result.is_ok(),
            "cleanup_worktree failed: {:?}",
            result.err()
        );

        // Directory should be gone.
        assert!(
            !wt.worktree_path.exists(),
            "Worktree directory should be removed"
        );

        // Branch should be deleted.
        let repo = Repository::open(dir.path()).unwrap();
        let branch = repo.find_branch("swarm/run-2/team-beta", BranchType::Local);
        assert!(
            branch.is_err(),
            "Branch should be deleted after cleanup with delete_branch=true"
        );
    }

    #[test]
    fn cleanup_worktree_keeps_branch_when_not_requested() {
        let (dir, _repo) = setup_test_repo();
        let manager = WorktreeManager::new(dir.path());

        manager.create_worktree("run-3", "team-gamma").unwrap();

        // Clean up without deleting the branch.
        manager.cleanup_worktree("team-gamma", false).unwrap();

        // Branch should still exist.
        let repo = Repository::open(dir.path()).unwrap();
        let branch = repo.find_branch("swarm/run-3/team-gamma", BranchType::Local);
        assert!(
            branch.is_ok(),
            "Branch should remain when delete_branch=false"
        );
    }

    #[test]
    fn merge_team_branch_fast_forward() {
        let (dir, repo) = setup_test_repo();
        let manager = WorktreeManager::new(dir.path());

        // Create a team worktree.
        let wt = manager.create_worktree("run-4", "team-delta").unwrap();

        // Add a commit to the team branch by writing a file in the worktree.
        let test_file = wt.worktree_path.join("team_file.txt");
        fs::write(&test_file, "team delta work").unwrap();

        // Open the worktree repo and commit.
        let wt_repo = Repository::open(&wt.worktree_path).unwrap();
        let mut index = wt_repo.index().unwrap();
        index.add_path(Path::new("team_file.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = wt_repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();
        let parent = wt_repo.head().unwrap().peel_to_commit().unwrap();
        wt_repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "Team delta work",
                &tree,
                &[&parent],
            )
            .unwrap();

        // Get the current HEAD branch name from the main repo.
        let head_ref = repo.head().unwrap();
        let target_branch = head_ref.shorthand().unwrap_or("master").to_string();

        // Merge team branch into the main branch (should fast-forward since
        // the main branch hasn't moved).
        let result = manager.merge_team_branch("swarm/run-4/team-delta", &target_branch);
        assert!(result.is_ok(), "merge failed: {:?}", result.err());

        let merge_result = result.unwrap();
        assert!(
            merge_result.success,
            "Merge should succeed: conflicts={:?}",
            merge_result.conflicts
        );
        assert!(merge_result.commit_hash.is_some());
        assert!(merge_result.conflicts.is_empty());
    }

    #[test]
    fn list_worktrees_returns_created_worktrees() {
        let (dir, _repo) = setup_test_repo();
        let manager = WorktreeManager::new(dir.path());

        // Initially no worktrees.
        let list = manager.list_worktrees().unwrap();
        assert!(list.is_empty(), "Should start with no worktrees");

        // Create two worktrees.
        manager.create_worktree("run-5", "team-one").unwrap();
        manager.create_worktree("run-5", "team-two").unwrap();

        let list = manager.list_worktrees().unwrap();
        assert_eq!(list.len(), 2, "Should list 2 worktrees");

        let team_ids: Vec<&str> = list.iter().map(|w| w.team_id.as_str()).collect();
        assert!(team_ids.contains(&"team-one"));
        assert!(team_ids.contains(&"team-two"));
    }

    #[test]
    fn path_validation_prevents_escape() {
        let (dir, _repo) = setup_test_repo();
        let manager = WorktreeManager::new(dir.path());

        // Ensure the worktrees dir exists for validation.
        fs::create_dir_all(manager.worktrees_dir()).unwrap();

        // Attempt to use a path that escapes the worktrees directory.
        let escape_path = manager.worktrees_dir().join("..").join("..").join("evil");
        let result = manager.validate_worktree_path(&escape_path);
        assert!(
            result.is_err(),
            "Path traversal should be rejected: {:?}",
            result.ok()
        );
    }

    #[test]
    fn create_worktree_rejects_empty_ids() {
        let (dir, _repo) = setup_test_repo();
        let manager = WorktreeManager::new(dir.path());

        let result = manager.create_worktree("", "team");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("valid characters"));

        let result = manager.create_worktree("run", "!@#$");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("valid characters"));
    }

    #[test]
    fn sanitize_branch_component_strips_special_chars() {
        assert_eq!(
            WorktreeManager::sanitize_branch_component("hello-world_123"),
            "hello-world_123"
        );
        assert_eq!(
            WorktreeManager::sanitize_branch_component("bad!@#chars"),
            "badchars"
        );
        assert_eq!(
            WorktreeManager::sanitize_branch_component("../../escape"),
            "escape"
        );
    }

    #[test]
    fn merge_rejects_main_as_team_branch() {
        let (dir, _repo) = setup_test_repo();
        let manager = WorktreeManager::new(dir.path());

        let result = manager.merge_team_branch("main", "some-branch");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("main/master"));

        let result = manager.merge_team_branch("master", "some-branch");
        assert!(result.is_err());
    }

    #[test]
    fn cleanup_swarm_removes_all_run_worktrees() {
        let (dir, _repo) = setup_test_repo();
        let manager = WorktreeManager::new(dir.path());

        // Create worktrees for the same run.
        manager.create_worktree("run-6", "team-a").unwrap();
        manager.create_worktree("run-6", "team-b").unwrap();

        // Also create one for a different run.
        manager.create_worktree("run-7", "team-c").unwrap();

        // Clean up run-6.
        let cleaned = manager.cleanup_swarm("run-6").unwrap();
        assert_eq!(cleaned, 2, "Should clean 2 worktrees for run-6");

        // run-7 worktree should still exist.
        let list = manager.list_worktrees().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].team_id, "team-c");
    }

    #[test]
    fn team_worktree_serialization() {
        let wt = TeamWorktree {
            team_id: "team-test".into(),
            branch_name: "swarm/run-1/team-test".into(),
            worktree_path: std::env::temp_dir().join("test"),
        };

        let json = serde_json::to_string(&wt).unwrap();
        let deserialized: TeamWorktree = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.team_id, "team-test");
        assert_eq!(deserialized.branch_name, "swarm/run-1/team-test");
    }

    #[test]
    fn merge_branch_result_serialization() {
        let result = MergeBranchResult {
            success: false,
            conflicts: vec!["src/main.rs".into(), "Cargo.toml".into()],
            commit_hash: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: MergeBranchResult = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.success);
        assert_eq!(deserialized.conflicts.len(), 2);
        assert!(deserialized.commit_hash.is_none());
    }
}
