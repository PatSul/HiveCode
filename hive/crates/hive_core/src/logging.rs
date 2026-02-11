use anyhow::Result;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::config::HiveConfig;

/// Initializes the logging system with file + console output.
/// Returns a guard that must be kept alive for the duration of the app.
pub fn init_logging() -> Result<WorkerGuard> {
    let logs_dir = HiveConfig::logs_dir()?;
    std::fs::create_dir_all(&logs_dir)?;

    // File appender: daily rotation
    let file_appender = tracing_appender::rolling::daily(&logs_dir, "hive");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info,hive_app=debug,hive_ui=debug,hive_core=debug,hive_ai=debug")
    });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(false)
                .with_writer(non_blocking),
        )
        .with(
            fmt::layer()
                .with_target(false)
                .with_thread_ids(false)
                .compact(),
        )
        .init();

    Ok(guard)
}

/// Initialize logging to a custom directory with a custom filter.
/// Useful for tests or embedded scenarios where `~/.hive/logs` is not desired.
pub fn init_logging_to_dir(logs_dir: &std::path::Path, filter: &str) -> Result<WorkerGuard> {
    std::fs::create_dir_all(logs_dir)?;

    let file_appender = tracing_appender::rolling::daily(logs_dir, "hive");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(false)
                .with_ansi(false)
                .with_writer(non_blocking),
        )
        .try_init()
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {e}"))?;

    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_init_logging_to_dir_creates_directory() {
        let tmp = tempfile::tempdir().expect("Failed to create tempdir");
        let logs_dir = tmp.path().join("nested").join("logs");
        assert!(!logs_dir.exists());

        // init_logging_to_dir should create the directory tree.
        // Note: we cannot call .init() on the global subscriber more than once
        // per process, so we just verify the directory creation and guard.
        let guard = init_logging_to_dir(&logs_dir, "warn");
        // The directory should now exist regardless of whether the subscriber
        // was actually installed (it may fail if another test already set it).
        assert!(logs_dir.exists());

        // If the guard was created successfully, it is valid.
        if let Ok(_guard) = guard {
            // Guard exists and is holding the non-blocking writer.
        }
    }

    #[test]
    fn test_init_logging_to_dir_existing_directory() {
        let tmp = tempfile::tempdir().expect("Failed to create tempdir");
        let logs_dir = tmp.path().join("logs");
        fs::create_dir_all(&logs_dir).unwrap();

        // Should not fail when directory already exists.
        let result = init_logging_to_dir(&logs_dir, "info");
        // Even if the global subscriber is already set, directory ops succeed.
        assert!(logs_dir.exists());
        // We mainly care that it did not panic.
        drop(result);
    }

    #[test]
    fn test_init_logging_to_dir_returns_guard() {
        let tmp = tempfile::tempdir().expect("Failed to create tempdir");
        let logs_dir = tmp.path().join("guard_test");

        let result = init_logging_to_dir(&logs_dir, "debug");
        assert!(logs_dir.exists());

        // If the subscriber was installed successfully, we get a guard.
        // If another test already set the global subscriber, we get an error
        // but the file appender and directory were still created.
        match result {
            Ok(guard) => {
                // Guard is alive; dropping it flushes pending writes.
                drop(guard);
            }
            Err(e) => {
                // Expected when another test already initialized the
                // global subscriber in this process.
                let msg = e.to_string();
                assert!(
                    msg.contains("logging") || msg.contains("subscriber"),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    #[test]
    fn test_env_filter_fallback() {
        // Verify EnvFilter construction does not panic with various inputs.
        let filters = ["info", "debug", "warn", "trace", "hive_core=debug,warn"];
        for f in &filters {
            let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(f));
            // Filter was created without panicking — it is valid.
            drop(filter);
        }
    }
}
