//! Auto-commit on Task Completion — automatic git commits after agent work.
//!
//! Hooks into agent task completion to stage changes and create descriptive
//! git commits. All shell commands are validated through `SecurityGateway`
//! before execution.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

use hive_core::SecurityGateway;

// ---------------------------------------------------------------------------
// Auto-commit Config
// ---------------------------------------------------------------------------

/// Configuration for the auto-commit service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoCommitConfig {
    /// Whether auto-commit is enabled.
    pub enabled: bool,
    /// Prefix for commit messages (e.g. "[hive]").
    pub commit_prefix: String,
    /// Whether to include the spec ID in commit messages.
    pub include_spec_id: bool,
    /// Whether to create a dedicated branch per spec.
    pub branch_per_spec: bool,
}

impl Default for AutoCommitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            commit_prefix: "[hive]".into(),
            include_spec_id: true,
            branch_per_spec: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Commit Result
// ---------------------------------------------------------------------------

/// Result of an auto-commit operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitResult {
    pub success: bool,
    pub commit_hash: Option<String>,
    pub message: String,
    pub files_changed: Vec<String>,
}

// ---------------------------------------------------------------------------
// Auto-commit Service
// ---------------------------------------------------------------------------

/// Service that creates git commits after agent task completion.
///
/// All git commands are validated through `SecurityGateway` before execution
/// to prevent command injection.
pub struct AutoCommitService {
    pub config: AutoCommitConfig,
    gateway: SecurityGateway,
    work_dir: PathBuf,
}

impl AutoCommitService {
    /// Create a new auto-commit service with the given configuration and
    /// working directory.
    pub fn new(config: AutoCommitConfig, work_dir: PathBuf) -> Self {
        Self {
            config,
            gateway: SecurityGateway::new(),
            work_dir,
        }
    }

    /// Get the list of changed files in the working directory.
    pub fn get_changed_files(&self) -> Result<Vec<String>, String> {
        let cmd = "git status --porcelain";
        self.gateway
            .check_command(cmd)
            .map_err(|e| format!("Security check failed: {e}"))?;

        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.work_dir)
            .output()
            .map_err(|e| format!("Failed to run git status: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git status failed: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let files: Vec<String> = stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                // git status --porcelain format: "XY filename"
                let trimmed = line.trim();
                if trimmed.len() > 3 {
                    trimmed[3..].to_string()
                } else {
                    trimmed.to_string()
                }
            })
            .collect();

        Ok(files)
    }

    /// Build a commit message from the task description and optional spec ID.
    fn build_commit_message(&self, task_desc: &str, spec_id: Option<&str>) -> String {
        let mut parts = Vec::new();

        parts.push(self.config.commit_prefix.clone());

        if self.config.include_spec_id
            && let Some(id) = spec_id {
                parts.push(format!("(spec:{id})"));
            }

        // Sanitize the task description: remove newlines and limit length.
        let sanitized = task_desc
            .chars()
            .filter(|c| *c != '\n' && *c != '\r')
            .take(200)
            .collect::<String>();
        parts.push(sanitized);

        parts.join(" ")
    }

    /// Stage all changes and create a commit after a task completes.
    ///
    /// Returns a `CommitResult` with the commit hash on success.
    pub fn commit_after_task(
        &self,
        task_desc: &str,
        spec_id: Option<&str>,
    ) -> Result<CommitResult, String> {
        if !self.config.enabled {
            return Ok(CommitResult {
                success: false,
                commit_hash: None,
                message: "Auto-commit is disabled".into(),
                files_changed: vec![],
            });
        }

        // Check for changed files first.
        let files = self.get_changed_files()?;
        if files.is_empty() {
            return Ok(CommitResult {
                success: false,
                commit_hash: None,
                message: "No changes to commit".into(),
                files_changed: vec![],
            });
        }

        // Stage all changes.
        let add_cmd = "git add -A";
        self.gateway
            .check_command(add_cmd)
            .map_err(|e| format!("Security check failed for git add: {e}"))?;

        let add_output = Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.work_dir)
            .output()
            .map_err(|e| format!("Failed to run git add: {e}"))?;

        if !add_output.status.success() {
            let stderr = String::from_utf8_lossy(&add_output.stderr);
            return Err(format!("git add failed: {stderr}"));
        }

        // Build and execute the commit.
        let message = self.build_commit_message(task_desc, spec_id);

        self.gateway
            .check_command("git commit")
            .map_err(|e| format!("Security check failed for git commit: {e}"))?;

        let commit_output = Command::new("git")
            .args(["commit", "-m", &message])
            .current_dir(&self.work_dir)
            .output()
            .map_err(|e| format!("Failed to run git commit: {e}"))?;

        if !commit_output.status.success() {
            let stderr = String::from_utf8_lossy(&commit_output.stderr);
            return Err(format!("git commit failed: {stderr}"));
        }

        // Extract commit hash.
        let commit_hash = self.get_head_hash()?;

        Ok(CommitResult {
            success: true,
            commit_hash: Some(commit_hash),
            message,
            files_changed: files,
        })
    }

    /// Create a new branch for a spec. Returns the branch name.
    pub fn create_spec_branch(&self, spec_id: &str) -> Result<String, String> {
        // Sanitize spec_id for use as a branch name: keep alphanumeric, dash, underscore.
        let safe_id: String = spec_id
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .take(50)
            .collect();

        if safe_id.is_empty() {
            return Err("Invalid spec ID for branch name".into());
        }

        let branch_name = format!("hive/spec-{safe_id}");

        let cmd = format!("git checkout -b {branch_name}");
        self.gateway
            .check_command(&cmd)
            .map_err(|e| format!("Security check failed: {e}"))?;

        let output = Command::new("git")
            .args(["checkout", "-b", &branch_name])
            .current_dir(&self.work_dir)
            .output()
            .map_err(|e| format!("Failed to create branch: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git checkout -b failed: {stderr}"));
        }

        Ok(branch_name)
    }

    /// Get the current HEAD commit hash.
    fn get_head_hash(&self) -> Result<String, String> {
        let cmd = "git rev-parse HEAD";
        self.gateway
            .check_command(cmd)
            .map_err(|e| format!("Security check failed: {e}"))?;

        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.work_dir)
            .output()
            .map_err(|e| format!("Failed to get HEAD hash: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git rev-parse HEAD failed: {stderr}"));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AutoCommitConfig {
        AutoCommitConfig {
            enabled: true,
            commit_prefix: "[test]".into(),
            include_spec_id: true,
            branch_per_spec: false,
        }
    }

    fn disabled_config() -> AutoCommitConfig {
        AutoCommitConfig {
            enabled: false,
            ..test_config()
        }
    }

    #[test]
    fn default_config_values() {
        let config = AutoCommitConfig::default();
        assert!(config.enabled);
        assert_eq!(config.commit_prefix, "[hive]");
        assert!(config.include_spec_id);
        assert!(!config.branch_per_spec);
    }

    #[test]
    fn build_commit_message_with_spec_id() {
        let svc = AutoCommitService::new(test_config(), PathBuf::from("."));
        let msg = svc.build_commit_message("Implement auth module", Some("spec-123"));
        assert!(msg.contains("[test]"));
        assert!(msg.contains("(spec:spec-123)"));
        assert!(msg.contains("Implement auth module"));
    }

    #[test]
    fn build_commit_message_without_spec_id() {
        let svc = AutoCommitService::new(test_config(), PathBuf::from("."));
        let msg = svc.build_commit_message("Fix bug", None);
        assert!(msg.contains("[test]"));
        assert!(msg.contains("Fix bug"));
        assert!(!msg.contains("spec:"));
    }

    #[test]
    fn build_commit_message_spec_id_disabled() {
        let config = AutoCommitConfig {
            include_spec_id: false,
            ..test_config()
        };
        let svc = AutoCommitService::new(config, PathBuf::from("."));
        let msg = svc.build_commit_message("Do something", Some("spec-abc"));
        assert!(!msg.contains("spec:"));
    }

    #[test]
    fn build_commit_message_sanitizes_newlines() {
        let svc = AutoCommitService::new(test_config(), PathBuf::from("."));
        let msg = svc.build_commit_message("Line one\nLine two\rLine three", None);
        assert!(!msg.contains('\n'));
        assert!(!msg.contains('\r'));
    }

    #[test]
    fn build_commit_message_truncates_long_descriptions() {
        let svc = AutoCommitService::new(test_config(), PathBuf::from("."));
        let long_desc = "a".repeat(500);
        let msg = svc.build_commit_message(&long_desc, None);
        // Prefix + space + 200 chars = well under 500.
        assert!(msg.len() < 220);
    }

    #[test]
    fn commit_after_task_disabled_returns_early() {
        let svc = AutoCommitService::new(disabled_config(), PathBuf::from("."));
        let result = svc.commit_after_task("test", None).unwrap();
        assert!(!result.success);
        assert!(result.message.contains("disabled"));
        assert!(result.commit_hash.is_none());
    }

    #[test]
    fn create_spec_branch_sanitizes_id() {
        let _svc = AutoCommitService::new(test_config(), PathBuf::from("."));
        // We can't actually create a branch without a git repo, so we test
        // the sanitization logic by checking that the SecurityGateway does
        // not reject the command.
        let gateway = SecurityGateway::new();
        let safe_id: String = "spec-123!@#$%"
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .take(50)
            .collect();
        let branch_name = format!("hive/spec-{safe_id}");
        let cmd = format!("git checkout -b {branch_name}");
        assert!(gateway.check_command(&cmd).is_ok());
    }

    #[test]
    fn create_spec_branch_empty_id_returns_error() {
        let svc = AutoCommitService::new(test_config(), PathBuf::from("."));
        let result = svc.create_spec_branch("!@#$%");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid spec ID"));
    }

    #[test]
    fn commit_result_serialization() {
        let result = CommitResult {
            success: true,
            commit_hash: Some("abc123def456".into()),
            message: "[hive] Test commit".into(),
            files_changed: vec!["src/main.rs".into(), "Cargo.toml".into()],
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: CommitResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.success, true);
        assert_eq!(deserialized.commit_hash, Some("abc123def456".into()));
        assert_eq!(deserialized.files_changed.len(), 2);
    }

    #[test]
    fn auto_commit_config_serialization() {
        let config = test_config();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AutoCommitConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.commit_prefix, "[test]");
        assert!(deserialized.enabled);
    }

    #[test]
    fn security_gateway_allows_git_commands() {
        let gateway = SecurityGateway::new();
        assert!(gateway.check_command("git status --porcelain").is_ok());
        assert!(gateway.check_command("git add -A").is_ok());
        assert!(gateway.check_command("git commit").is_ok());
        assert!(gateway.check_command("git rev-parse HEAD").is_ok());
    }

    #[test]
    fn get_changed_files_on_nonexistent_dir_returns_error() {
        let svc = AutoCommitService::new(
            test_config(),
            std::env::temp_dir().join("nonexistent-hive-test-dir-12345"),
        );
        let result = svc.get_changed_files();
        assert!(result.is_err());
    }

    #[test]
    fn commit_after_task_on_nonexistent_dir_returns_error() {
        let svc = AutoCommitService::new(
            test_config(),
            std::env::temp_dir().join("nonexistent-hive-test-dir-12345"),
        );
        let result = svc.commit_after_task("test task", None);
        assert!(result.is_err());
    }
}
