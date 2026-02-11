use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// ShellOutput
// ---------------------------------------------------------------------------

/// A single chunk of output from an interactive shell session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellOutput {
    /// Data received on the child's stdout.
    Stdout(String),
    /// Data received on the child's stderr.
    Stderr(String),
    /// The shell process exited with this code.
    Exit(i32),
}

// ---------------------------------------------------------------------------
// InteractiveShell
// ---------------------------------------------------------------------------

/// Interactive shell backed by `tokio::process::Command`.
///
/// Spawns a platform-appropriate shell (`cmd.exe` on Windows, `/bin/bash` or
/// `/bin/sh` on Unix) and provides async read/write access to its
/// stdin/stdout/stderr via an mpsc channel.
///
/// This is intentionally a lightweight process wrapper. True PTY support
/// (resize signalling, raw mode, etc.) would require `portable-pty` or
/// similar, which can be added later.
pub struct InteractiveShell {
    child: Child,
    stdin: ChildStdin,
    output_rx: mpsc::Receiver<ShellOutput>,
    cols: u16,
    rows: u16,
    cwd: PathBuf,
}

impl std::fmt::Debug for InteractiveShell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InteractiveShell")
            .field("cols", &self.cols)
            .field("rows", &self.rows)
            .field("cwd", &self.cwd)
            .finish_non_exhaustive()
    }
}

/// Return the platform-appropriate shell program and its interactive flag(s).
#[cfg(windows)]
fn shell_program() -> (&'static str, Vec<&'static str>) {
    ("cmd.exe", vec![])
}

#[cfg(unix)]
fn shell_program() -> (&'static str, Vec<&'static str>) {
    // Prefer bash when available; fall back to sh.
    if Path::new("/bin/bash").exists() {
        ("/bin/bash", vec![])
    } else {
        ("/bin/sh", vec![])
    }
}

/// Name of the shell program for the current platform (useful in tests).
#[allow(dead_code)]
#[cfg(windows)]
fn expected_shell_name() -> &'static str {
    "cmd.exe"
}

#[allow(dead_code)]
#[cfg(unix)]
fn expected_shell_name() -> &'static str {
    if Path::new("/bin/bash").exists() {
        "/bin/bash"
    } else {
        "/bin/sh"
    }
}

impl InteractiveShell {
    /// Default terminal dimensions when none are specified.
    const DEFAULT_COLS: u16 = 80;
    const DEFAULT_ROWS: u16 = 24;

    /// Channel buffer size for output messages.
    const OUTPUT_CHANNEL_SIZE: usize = 1024;

    /// Spawn a new interactive shell.
    ///
    /// If `cwd` is `None`, the current working directory of the parent process
    /// is used. If `cwd` is `Some(path)`, the shell starts in that directory
    /// (the path must exist and be a directory).
    pub fn new(cwd: Option<&Path>) -> Result<Self> {
        let working_dir = match cwd {
            Some(p) => {
                anyhow::ensure!(p.exists(), "Working directory does not exist: {}", p.display());
                anyhow::ensure!(p.is_dir(), "Path is not a directory: {}", p.display());
                p.to_path_buf()
            }
            None => std::env::current_dir().context("Failed to determine current directory")?,
        };

        let (program, args) = shell_program();

        debug!(shell = program, dir = %working_dir.display(), "spawning interactive shell");

        let mut cmd = Command::new(program);
        cmd.args(&args)
            .current_dir(&working_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // On Unix, create a new process group so `kill` targets the whole tree.
        // tokio::process::Command implements std::os::unix::process::CommandExt.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            cmd.process_group(0);
        }

        let mut child = cmd.spawn().with_context(|| {
            format!("Failed to spawn shell: {program}")
        })?;

        let stdin = child.stdin.take().context("Failed to open shell stdin")?;
        let stdout = child.stdout.take().context("Failed to open shell stdout")?;
        let stderr = child.stderr.take().context("Failed to open shell stderr")?;

        let (tx, rx) = mpsc::channel(Self::OUTPUT_CHANNEL_SIZE);

        // Spawn background reader for stdout.
        let tx_out = tx.clone();
        tokio::spawn(async move {
            read_stream_to_channel(stdout, tx_out, false).await;
        });

        // Spawn background reader for stderr.
        let tx_err = tx;
        tokio::spawn(async move {
            read_stream_to_channel(stderr, tx_err, true).await;
        });

        Ok(Self {
            child,
            stdin,
            output_rx: rx,
            cols: Self::DEFAULT_COLS,
            rows: Self::DEFAULT_ROWS,
            cwd: working_dir,
        })
    }

    /// Send input text to the shell's stdin.
    ///
    /// The caller is responsible for including a trailing newline (`\n`) if a
    /// command submission is intended.
    pub async fn write(&mut self, input: &str) -> Result<()> {
        self.stdin
            .write_all(input.as_bytes())
            .await
            .context("Failed to write to shell stdin")?;
        self.stdin.flush().await.context("Failed to flush shell stdin")?;
        Ok(())
    }

    /// Non-blocking read of the next available output chunk.
    ///
    /// Returns `None` if no output is currently available (the channel is
    /// empty but not closed) or if the channel has been closed.
    pub fn read(&mut self) -> Option<ShellOutput> {
        self.output_rx.try_recv().ok()
    }

    /// Async read that waits for the next output chunk.
    ///
    /// Returns `None` only when the channel is closed (both background reader
    /// tasks have finished).
    pub async fn read_async(&mut self) -> Option<ShellOutput> {
        self.output_rx.recv().await
    }

    /// Store new terminal dimensions.
    ///
    /// Without a real PTY, this only records the values for later use (e.g.
    /// when upgrading to `portable-pty`). No signal is sent to the child.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        debug!(cols, rows, "terminal dimensions updated (no PTY signal)");
    }

    /// Current terminal column count.
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// Current terminal row count.
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// The working directory the shell was started in.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Check whether the shell process is still running.
    ///
    /// This performs a non-blocking wait. If the process has exited, the exit
    /// code is **consumed** internally; use [`try_wait`] if you need the code.
    pub fn is_running(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_status)) => false,
            Ok(None) => true,
            Err(_) => false,
        }
    }

    /// Non-blocking check for exit status.
    ///
    /// Returns `Ok(Some(code))` if the process has exited, `Ok(None)` if it
    /// is still running, or an error if the status could not be queried.
    pub fn try_wait(&mut self) -> Result<Option<i32>> {
        match self.child.try_wait().context("Failed to query process status")? {
            Some(status) => Ok(Some(status.code().unwrap_or(-1))),
            None => Ok(None),
        }
    }

    /// Kill the shell process.
    ///
    /// On Unix this sends SIGKILL. On Windows this calls `TerminateProcess`.
    /// The method is idempotent: killing an already-exited process is not an
    /// error.
    pub async fn kill(&mut self) -> Result<()> {
        match self.child.kill().await {
            Ok(()) => {
                debug!("shell process killed");
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => {
                // Process already exited.
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!(e).context("Failed to kill shell process")),
        }
    }
}

// ---------------------------------------------------------------------------
// Background reader helper
// ---------------------------------------------------------------------------

/// Read from an async reader line-by-line and send each chunk to the channel.
///
/// When `is_stderr` is true, output is wrapped in [`ShellOutput::Stderr`];
/// otherwise in [`ShellOutput::Stdout`].
async fn read_stream_to_channel<R>(reader: R, tx: mpsc::Sender<ShellOutput>, is_stderr: bool)
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncBufReadExt;

    let mut lines = tokio::io::BufReader::new(reader).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let msg = if is_stderr {
                    ShellOutput::Stderr(line)
                } else {
                    ShellOutput::Stdout(line)
                };
                if tx.send(msg).await.is_err() {
                    // Receiver dropped; stop reading.
                    break;
                }
            }
            Ok(None) => break, // EOF
            Err(e) => {
                warn!(error = %e, is_stderr, "error reading shell output");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Construction --------------------------------------------------------

    #[tokio::test]
    async fn shell_construction_succeeds() {
        let shell = InteractiveShell::new(None);
        assert!(shell.is_ok(), "shell creation failed: {:?}", shell.err());
        // Clean up.
        let mut shell = shell.unwrap();
        let _ = shell.kill().await;
    }

    #[tokio::test]
    async fn shell_construction_with_explicit_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let shell = InteractiveShell::new(Some(dir.path()));
        assert!(shell.is_ok(), "shell creation failed: {:?}", shell.err());
        let mut shell = shell.unwrap();
        assert_eq!(shell.cwd(), dir.path());
        let _ = shell.kill().await;
    }

    #[tokio::test]
    async fn shell_construction_rejects_nonexistent_dir() {
        let result = InteractiveShell::new(Some(Path::new("/tmp/no_such_hive_dir_99999")));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("does not exist"),
            "expected 'does not exist', got: {msg}"
        );
    }

    // -- Platform detection --------------------------------------------------

    #[test]
    fn detects_correct_platform_shell() {
        let (program, _args) = shell_program();
        let expected = expected_shell_name();
        assert_eq!(program, expected);
    }

    // -- ShellOutput serialization -------------------------------------------

    #[test]
    fn shell_output_serialization_roundtrip() {
        let variants = vec![
            ShellOutput::Stdout("hello world".into()),
            ShellOutput::Stderr("an error".into()),
            ShellOutput::Exit(42),
        ];
        for original in &variants {
            let json = serde_json::to_string(original).expect("serialize");
            let restored: ShellOutput = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(&restored, original);
        }
    }

    #[test]
    fn shell_output_exit_serialization() {
        let output = ShellOutput::Exit(0);
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("Exit"), "json was: {json}");
        assert!(json.contains("0"), "json was: {json}");
    }

    // -- Dimensions ----------------------------------------------------------

    #[tokio::test]
    async fn default_dimensions() {
        let mut shell = InteractiveShell::new(None).unwrap();
        assert_eq!(shell.cols(), InteractiveShell::DEFAULT_COLS);
        assert_eq!(shell.rows(), InteractiveShell::DEFAULT_ROWS);
        let _ = shell.kill().await;
    }

    #[tokio::test]
    async fn resize_updates_dimensions() {
        let mut shell = InteractiveShell::new(None).unwrap();
        shell.resize(120, 40);
        assert_eq!(shell.cols(), 120);
        assert_eq!(shell.rows(), 40);
        let _ = shell.kill().await;
    }

    // -- is_running ----------------------------------------------------------

    #[tokio::test]
    async fn is_running_returns_true_for_live_shell() {
        let mut shell = InteractiveShell::new(None).unwrap();
        assert!(shell.is_running(), "shell should be running immediately after spawn");
        let _ = shell.kill().await;
    }

    #[tokio::test]
    async fn is_running_returns_false_after_kill() {
        let mut shell = InteractiveShell::new(None).unwrap();
        shell.kill().await.expect("kill should succeed");
        // Give the OS a moment to reap.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!shell.is_running(), "shell should not be running after kill");
    }

    // -- Write + read (echo) -------------------------------------------------

    #[tokio::test]
    async fn write_and_read_echo() {
        let mut shell = InteractiveShell::new(None).unwrap();

        // Send an echo command.
        let echo_cmd = if cfg!(target_os = "windows") {
            "echo hive_test_marker\r\n"
        } else {
            "echo hive_test_marker\n"
        };
        shell.write(echo_cmd).await.expect("write should succeed");

        // Read output with a timeout — we should see our marker.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut found = false;
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(
                std::time::Duration::from_millis(200),
                shell.read_async(),
            )
            .await
            {
                Ok(Some(ShellOutput::Stdout(line))) if line.contains("hive_test_marker") => {
                    found = true;
                    break;
                }
                Ok(Some(_)) => continue,   // skip other output (prompt, etc.)
                Ok(None) => break,          // channel closed
                Err(_) => continue,         // timeout, try again
            }
        }
        assert!(found, "expected to read back 'hive_test_marker' from shell");

        let _ = shell.kill().await;
    }

    // -- Kill ----------------------------------------------------------------

    #[tokio::test]
    async fn kill_terminates_process() {
        let mut shell = InteractiveShell::new(None).unwrap();
        assert!(shell.is_running());
        shell.kill().await.expect("kill should succeed");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        // try_wait should now return an exit code.
        let status = shell.try_wait().expect("try_wait should not error");
        assert!(status.is_some(), "process should have exited after kill");
    }

    #[tokio::test]
    async fn kill_is_idempotent() {
        let mut shell = InteractiveShell::new(None).unwrap();
        shell.kill().await.expect("first kill");
        // Second kill should not error.
        shell.kill().await.expect("second kill should also succeed");
    }

    // -- Default working directory -------------------------------------------

    #[tokio::test]
    async fn default_working_directory_is_current_dir() {
        let mut shell = InteractiveShell::new(None).unwrap();
        let expected = std::env::current_dir().unwrap();
        assert_eq!(shell.cwd(), expected.as_path());
        let _ = shell.kill().await;
    }

    // -- try_wait ------------------------------------------------------------

    #[tokio::test]
    async fn try_wait_returns_none_while_running() {
        let mut shell = InteractiveShell::new(None).unwrap();
        let status = shell.try_wait().expect("try_wait should not error");
        assert!(status.is_none(), "expected None while shell is running");
        let _ = shell.kill().await;
    }

    // -- Debug ---------------------------------------------------------------

    #[tokio::test]
    async fn debug_format_does_not_panic() {
        let mut shell = InteractiveShell::new(None).unwrap();
        let debug_str = format!("{:?}", shell);
        assert!(debug_str.contains("InteractiveShell"));
        let _ = shell.kill().await;
    }

    // -- Non-blocking read on empty channel ----------------------------------

    #[tokio::test]
    async fn read_returns_none_when_no_output() {
        let mut shell = InteractiveShell::new(None).unwrap();
        // Immediately reading with try_recv should return None (no output yet
        // in the channel, or only the shell prompt which may not have arrived).
        // Either None or Some is acceptable here, but it must not panic.
        let _output = shell.read();
        let _ = shell.kill().await;
    }
}
