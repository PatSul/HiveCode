use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Top-level application error type.
#[derive(Error, Debug)]
pub enum HiveError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("AI provider error: {0}")]
    AiProvider(String),

    #[error("Security violation: {0}")]
    Security(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("File system error: {0}")]
    FileSystem(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Budget exceeded: {0}")]
    BudgetExceeded(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Classification of errors for logging and user display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// Error caused by user action (e.g., exceeding budget).
    UserError,
    /// Error from an AI provider (e.g., model unavailable).
    ProviderError,
    /// Network connectivity or timeout issue.
    NetworkError,
    /// Security or authentication failure.
    SecurityError,
    /// Internal system error (storage, file I/O, etc.).
    SystemError,
    /// Invalid or missing configuration.
    ConfigError,
}

impl HiveError {
    /// Returns the broad error category for routing and display purposes.
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::Config(_) => ErrorCategory::ConfigError,
            Self::AiProvider(_) => ErrorCategory::ProviderError,
            Self::Security(_) => ErrorCategory::SecurityError,
            Self::Storage(_) => ErrorCategory::SystemError,
            Self::Network(_) => ErrorCategory::NetworkError,
            Self::FileSystem(_) => ErrorCategory::SystemError,
            Self::Auth(_) => ErrorCategory::SecurityError,
            Self::RateLimited { .. } => ErrorCategory::ProviderError,
            Self::BudgetExceeded(_) => ErrorCategory::UserError,
            Self::Internal(_) => ErrorCategory::SystemError,
        }
    }

    /// Returns a user-friendly message (hides internal details).
    pub fn user_message(&self) -> String {
        match self {
            Self::Config(msg) => format!("Configuration issue: {msg}"),
            Self::AiProvider(msg) => format!("AI service error: {msg}"),
            Self::Security(msg) => format!("Security: {msg}"),
            Self::Storage(_) => "Storage error. Check disk space and permissions.".into(),
            Self::Network(_) => "Network error. Check your connection.".into(),
            Self::FileSystem(msg) => format!("File error: {msg}"),
            Self::Auth(_) => "Authentication failed. Check your API keys.".into(),
            Self::RateLimited { retry_after_secs } => {
                format!("Rate limited. Retrying in {retry_after_secs}s.")
            }
            Self::BudgetExceeded(msg) => format!("Budget exceeded: {msg}"),
            Self::Internal(_) => "An unexpected error occurred.".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Error classification for anyhow::Error (message-pattern based)
// ---------------------------------------------------------------------------

/// Error severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    /// Non-critical, log only.
    Low,
    /// Show to user, recoverable.
    Medium,
    /// Operation failed.
    High,
    /// App may be unstable.
    Critical,
}

/// Fine-grained error category derived from message patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassifiedCategory {
    Network,
    Authentication,
    RateLimit,
    Configuration,
    FileSystem,
    Database,
    Provider,
    Security,
    Internal,
}

/// Classified error with context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedError {
    pub severity: ErrorSeverity,
    pub category: ClassifiedCategory,
    pub message: String,
    pub user_message: String,
    pub recoverable: bool,
}

/// Classify an `anyhow::Error` into severity, category, and a user-friendly message
/// by inspecting the error message for known patterns.
pub fn classify_error(error: &anyhow::Error) -> ClassifiedError {
    let msg = error.to_string().to_lowercase();

    let (category, severity, user_msg) = if msg.contains("rate limit") || msg.contains("429") {
        (
            ClassifiedCategory::RateLimit,
            ErrorSeverity::Medium,
            "Rate limited. Will retry with a different provider.",
        )
    } else if msg.contains("unauthorized") || msg.contains("401") || msg.contains("invalid key") {
        (
            ClassifiedCategory::Authentication,
            ErrorSeverity::High,
            "Invalid API key. Check your settings.",
        )
    } else if msg.contains("timeout") || msg.contains("connection") || msg.contains("dns") {
        (
            ClassifiedCategory::Network,
            ErrorSeverity::Medium,
            "Network error. Check your connection.",
        )
    } else if (msg.contains("not found") && msg.contains("file")) || msg.contains("no such file") {
        (
            ClassifiedCategory::FileSystem,
            ErrorSeverity::Medium,
            "File not found.",
        )
    } else if msg.contains("permission denied") {
        (
            ClassifiedCategory::Security,
            ErrorSeverity::High,
            "Permission denied.",
        )
    } else if msg.contains("database") || msg.contains("sqlite") {
        (
            ClassifiedCategory::Database,
            ErrorSeverity::High,
            "Database error. Your data is safe.",
        )
    } else if msg.contains("config") {
        (
            ClassifiedCategory::Configuration,
            ErrorSeverity::Medium,
            "Configuration error. Check settings.",
        )
    } else {
        (
            ClassifiedCategory::Internal,
            ErrorSeverity::Medium,
            "An unexpected error occurred.",
        )
    };

    ClassifiedError {
        severity,
        category,
        message: error.to_string(),
        user_message: user_msg.to_string(),
        recoverable: severity != ErrorSeverity::Critical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    // -----------------------------------------------------------------------
    // HiveError — category mapping
    // -----------------------------------------------------------------------

    #[test]
    fn test_hive_error_category_config() {
        let err = HiveError::Config("bad value".into());
        assert_eq!(err.category(), ErrorCategory::ConfigError);
    }

    #[test]
    fn test_hive_error_category_provider() {
        let err = HiveError::AiProvider("model not found".into());
        assert_eq!(err.category(), ErrorCategory::ProviderError);
    }

    #[test]
    fn test_hive_error_user_message_hides_internals() {
        let err = HiveError::Internal("segfault at 0xdeadbeef".into());
        assert_eq!(err.user_message(), "An unexpected error occurred.");
    }

    // -----------------------------------------------------------------------
    // classify_error — one test per category
    // -----------------------------------------------------------------------

    #[test]
    fn test_classify_rate_limit_keyword() {
        let err = anyhow!("rate limit exceeded, please retry");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::RateLimit);
        assert_eq!(classified.severity, ErrorSeverity::Medium);
        assert!(classified.recoverable);
        assert_eq!(
            classified.user_message,
            "Rate limited. Will retry with a different provider."
        );
    }

    #[test]
    fn test_classify_rate_limit_429() {
        let err = anyhow!("HTTP 429 Too Many Requests");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::RateLimit);
    }

    #[test]
    fn test_classify_authentication_unauthorized() {
        let err = anyhow!("Unauthorized: bad credentials");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::Authentication);
        assert_eq!(classified.severity, ErrorSeverity::High);
        assert_eq!(
            classified.user_message,
            "Invalid API key. Check your settings."
        );
    }

    #[test]
    fn test_classify_authentication_401() {
        let err = anyhow!("server returned 401");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::Authentication);
    }

    #[test]
    fn test_classify_authentication_invalid_key() {
        let err = anyhow!("invalid key provided");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::Authentication);
    }

    #[test]
    fn test_classify_network_timeout() {
        let err = anyhow!("request timeout after 30s");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::Network);
        assert_eq!(classified.severity, ErrorSeverity::Medium);
        assert_eq!(
            classified.user_message,
            "Network error. Check your connection."
        );
    }

    #[test]
    fn test_classify_network_connection() {
        let err = anyhow!("connection refused on port 443");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::Network);
    }

    #[test]
    fn test_classify_network_dns() {
        let err = anyhow!("dns resolution failed for api.example.com");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::Network);
    }

    #[test]
    fn test_classify_filesystem_not_found() {
        let err = anyhow!("file not found: /tmp/missing.txt");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::FileSystem);
        assert_eq!(classified.severity, ErrorSeverity::Medium);
        assert_eq!(classified.user_message, "File not found.");
    }

    #[test]
    fn test_classify_filesystem_no_such_file() {
        let err = anyhow!("No such file or directory (os error 2)");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::FileSystem);
    }

    #[test]
    fn test_classify_security_permission_denied() {
        let err = anyhow!("permission denied: /etc/shadow");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::Security);
        assert_eq!(classified.severity, ErrorSeverity::High);
        assert_eq!(classified.user_message, "Permission denied.");
    }

    #[test]
    fn test_classify_database_sqlite() {
        let err = anyhow!("sqlite error: disk I/O error");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::Database);
        assert_eq!(classified.severity, ErrorSeverity::High);
        assert_eq!(
            classified.user_message,
            "Database error. Your data is safe."
        );
    }

    #[test]
    fn test_classify_database_keyword() {
        let err = anyhow!("database locked by another process");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::Database);
    }

    #[test]
    fn test_classify_configuration() {
        let err = anyhow!("config file is malformed");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::Configuration);
        assert_eq!(classified.severity, ErrorSeverity::Medium);
        assert_eq!(
            classified.user_message,
            "Configuration error. Check settings."
        );
    }

    #[test]
    fn test_classify_internal_fallback() {
        let err = anyhow!("something totally unexpected happened");
        let classified = classify_error(&err);
        assert_eq!(classified.category, ClassifiedCategory::Internal);
        assert_eq!(classified.severity, ErrorSeverity::Medium);
        assert_eq!(classified.user_message, "An unexpected error occurred.");
    }

    #[test]
    fn test_classify_preserves_original_message() {
        let original = "connection reset by peer";
        let err = anyhow!("{}", original);
        let classified = classify_error(&err);
        assert_eq!(classified.message, original);
    }

    #[test]
    fn test_classify_recoverable_for_non_critical() {
        // All current categories map to Medium or High, never Critical,
        // so all should be recoverable.
        let errors = [
            anyhow!("rate limit hit"),
            anyhow!("unauthorized"),
            anyhow!("timeout"),
            anyhow!("file not found"),
            anyhow!("permission denied"),
            anyhow!("database error"),
            anyhow!("config missing"),
            anyhow!("unknown"),
        ];
        for err in &errors {
            let classified = classify_error(err);
            assert!(
                classified.recoverable,
                "expected recoverable for: {}",
                classified.message
            );
        }
    }

    // -----------------------------------------------------------------------
    // ErrorSeverity — equality and serialization round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_severity_serde_round_trip() {
        let severities = [
            ErrorSeverity::Low,
            ErrorSeverity::Medium,
            ErrorSeverity::High,
            ErrorSeverity::Critical,
        ];
        for severity in &severities {
            let json = serde_json::to_string(severity).unwrap();
            let deserialized: ErrorSeverity = serde_json::from_str(&json).unwrap();
            assert_eq!(*severity, deserialized);
        }
    }

    #[test]
    fn test_classified_error_serde_round_trip() {
        let classified = ClassifiedError {
            severity: ErrorSeverity::High,
            category: ClassifiedCategory::Authentication,
            message: "unauthorized".into(),
            user_message: "Invalid API key. Check your settings.".into(),
            recoverable: true,
        };
        let json = serde_json::to_string(&classified).unwrap();
        let deserialized: ClassifiedError = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.severity, ErrorSeverity::High);
        assert_eq!(deserialized.category, ClassifiedCategory::Authentication);
        assert_eq!(deserialized.message, "unauthorized");
        assert!(deserialized.recoverable);
    }
}
