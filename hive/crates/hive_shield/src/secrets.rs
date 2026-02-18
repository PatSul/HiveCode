use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Categories of secrets/credentials that can be detected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SecretType {
    ApiKey,
    AwsAccessKey,
    AwsSecretKey,
    GithubToken,
    GitlabToken,
    SlackToken,
    PrivateKey,
    Password,
    JwtToken,
    GenericSecret,
    DatabaseUrl,
    Custom(String),
}

impl std::fmt::Display for SecretType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretType::ApiKey => write!(f, "API_KEY"),
            SecretType::AwsAccessKey => write!(f, "AWS_ACCESS_KEY"),
            SecretType::AwsSecretKey => write!(f, "AWS_SECRET_KEY"),
            SecretType::GithubToken => write!(f, "GITHUB_TOKEN"),
            SecretType::GitlabToken => write!(f, "GITLAB_TOKEN"),
            SecretType::SlackToken => write!(f, "SLACK_TOKEN"),
            SecretType::PrivateKey => write!(f, "PRIVATE_KEY"),
            SecretType::Password => write!(f, "PASSWORD"),
            SecretType::JwtToken => write!(f, "JWT_TOKEN"),
            SecretType::GenericSecret => write!(f, "GENERIC_SECRET"),
            SecretType::DatabaseUrl => write!(f, "DATABASE_URL"),
            SecretType::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// A single secret found in scanned text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMatch {
    pub secret_type: SecretType,
    /// The masked value (first 4 chars visible, rest replaced with `****`).
    pub value: String,
    /// Filename or context label describing where the match was found.
    pub location: String,
    /// 1-based line number of the match.
    pub line: usize,
    /// Confidence of the match (0.0 - 1.0).
    pub confidence: f64,
}

/// Aggregate result of scanning text for secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub matches: Vec<SecretMatch>,
    pub files_scanned: usize,
    pub risk_level: RiskLevel,
}

/// Overall risk level derived from the set of detected secrets.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    None,
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::None => write!(f, "none"),
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
            RiskLevel::Critical => write!(f, "critical"),
        }
    }
}

// ---------------------------------------------------------------------------
// Compiled regex patterns
// ---------------------------------------------------------------------------

struct SecretPattern {
    secret_type: SecretType,
    regex: Regex,
    confidence: f64,
    risk: RiskLevel,
}

static SECRET_PATTERNS: Lazy<Vec<SecretPattern>> = Lazy::new(|| {
    vec![
        SecretPattern {
            secret_type: SecretType::AwsAccessKey,
            regex: Regex::new(r"AKIA[0-9A-Z]{16}").expect("valid regex: AWS access key"),
            confidence: 0.95,
            risk: RiskLevel::Critical,
        },
        SecretPattern {
            secret_type: SecretType::GithubToken,
            regex: Regex::new(r"gh[pousr]_[A-Za-z0-9_]{36,}").expect("valid regex: GitHub token"),
            confidence: 0.95,
            risk: RiskLevel::Critical,
        },
        SecretPattern {
            secret_type: SecretType::GitlabToken,
            regex: Regex::new(r"glpat-[A-Za-z0-9\-]{20,}").expect("valid regex: GitLab token"),
            confidence: 0.95,
            risk: RiskLevel::Critical,
        },
        SecretPattern {
            secret_type: SecretType::SlackToken,
            regex: Regex::new(r"xox[baprs]-[A-Za-z0-9\-]+").expect("valid regex: Slack token"),
            confidence: 0.90,
            risk: RiskLevel::High,
        },
        SecretPattern {
            secret_type: SecretType::JwtToken,
            regex: Regex::new(r"eyJ[A-Za-z0-9_\-]+\.eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+").expect("valid regex: JWT token"),
            confidence: 0.85,
            risk: RiskLevel::High,
        },
        SecretPattern {
            secret_type: SecretType::PrivateKey,
            regex: Regex::new(r"-----BEGIN (RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----").expect("valid regex: private key"),
            confidence: 0.99,
            risk: RiskLevel::Critical,
        },
        SecretPattern {
            secret_type: SecretType::DatabaseUrl,
            regex: Regex::new(r"(?i)(postgres|mysql|mongodb|redis)://[^\s]+").expect("valid regex: database URL"),
            confidence: 0.90,
            risk: RiskLevel::High,
        },
        SecretPattern {
            secret_type: SecretType::GenericSecret,
            regex: Regex::new(
                r"(?i)(api[_\-]?key|apikey|api_secret|access_token)\s*[=:]\s*['\x22]?([a-zA-Z0-9_\-]{20,})['\x22]?"
            ).expect("valid regex: generic API key"),
            confidence: 0.70,
            risk: RiskLevel::Medium,
        },
    ]
});

// ---------------------------------------------------------------------------
// SecretScanner
// ---------------------------------------------------------------------------

/// Scans text for leaked secrets, credentials, and tokens.
pub struct SecretScanner;

impl SecretScanner {
    pub fn new() -> Self {
        Self
    }

    /// Scan a block of text and return all detected secret matches.
    pub fn scan_text(&self, text: &str) -> Vec<SecretMatch> {
        self.scan_text_with_context(text, "<inline>")
    }

    /// Scan text with a filename/context label attached to each match.
    pub fn scan_text_with_context(&self, text: &str, filename: &str) -> Vec<SecretMatch> {
        let mut results = Vec::new();

        for pattern in SECRET_PATTERNS.iter() {
            for m in pattern.regex.find_iter(text) {
                let line = line_number_of(text, m.start());
                results.push(SecretMatch {
                    secret_type: pattern.secret_type.clone(),
                    value: mask_secret(m.as_str()),
                    location: filename.to_string(),
                    line,
                    confidence: pattern.confidence,
                });
            }
        }

        results.sort_by_key(|m| m.line);
        results
    }

    /// Compute an aggregate risk level from a set of matches.
    pub fn risk_level(matches: &[SecretMatch]) -> RiskLevel {
        if matches.is_empty() {
            return RiskLevel::None;
        }

        // The aggregate risk is the highest individual risk among patterns
        // that matched, boosted if there are many matches.
        let mut worst = RiskLevel::Low;
        for m in matches {
            let pattern_risk = SECRET_PATTERNS
                .iter()
                .find(|p| p.secret_type == m.secret_type)
                .map(|p| p.risk.clone())
                .unwrap_or(RiskLevel::Low);
            if pattern_risk > worst {
                worst = pattern_risk;
            }
        }

        // Escalate if many secrets found.
        if matches.len() >= 5 && worst < RiskLevel::Critical {
            RiskLevel::Critical
        } else if matches.len() >= 3 && worst < RiskLevel::High {
            RiskLevel::High
        } else {
            worst
        }
    }

    /// Build a complete `ScanResult` from scanning a single text.
    pub fn scan(&self, text: &str) -> ScanResult {
        let matches = self.scan_text(text);
        let risk_level = Self::risk_level(&matches);
        ScanResult {
            matches,
            files_scanned: 1,
            risk_level,
        }
    }
}

impl Default for SecretScanner {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Mask a secret value: show at most the first 4 characters followed by `****`.
pub fn mask_secret(secret: &str) -> String {
    if secret.len() <= 4 {
        return "****".to_string();
    }
    format!("{}****", &secret[..4])
}

/// Return the 1-based line number containing byte offset `pos`.
fn line_number_of(text: &str, pos: usize) -> usize {
    text[..pos].matches('\n').count() + 1
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn scanner() -> SecretScanner {
        SecretScanner::new()
    }

    #[test]
    fn detect_aws_access_key() {
        let s = scanner();
        let fake_key = format!("AKIA{}", "IOSFODNN7EXAMPLE");
        let matches = s.scan_text(&format!("key = {fake_key}"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].secret_type, SecretType::AwsAccessKey);
    }

    #[test]
    fn detect_github_token() {
        let s = scanner();
        let token = format!("ghp_{}", "A".repeat(40));
        let matches = s.scan_text(&format!("token: {token}"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].secret_type, SecretType::GithubToken);
    }

    #[test]
    fn detect_gitlab_token() {
        let s = scanner();
        let matches = s.scan_text("token = glpat-abcdefghijklmnopqrst01234");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].secret_type, SecretType::GitlabToken);
    }

    #[test]
    fn detect_slack_token() {
        let s = scanner();
        let matches = s.scan_text("SLACK_TOKEN=xoxb-123456789-abcdef");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].secret_type, SecretType::SlackToken);
    }

    #[test]
    fn detect_jwt() {
        let s = scanner();
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abc123_DEF-456";
        let matches = s.scan_text(&format!("Authorization: Bearer {jwt}"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].secret_type, SecretType::JwtToken);
    }

    #[test]
    fn detect_private_key_header() {
        let s = scanner();
        let matches = s.scan_text("-----BEGIN RSA PRIVATE KEY-----\nMIIE...");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].secret_type, SecretType::PrivateKey);
    }

    #[test]
    fn detect_database_url() {
        let s = scanner();
        let matches = s.scan_text("DATABASE_URL=postgres://user:pass@host:5432/db");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].secret_type, SecretType::DatabaseUrl);
    }

    #[test]
    fn detect_generic_api_key() {
        let s = scanner();
        let matches = s.scan_text("api_key = 'abcdefghijklmnopqrstuvwx'");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].secret_type, SecretType::GenericSecret);
    }

    #[test]
    fn mask_secret_short() {
        assert_eq!(mask_secret("abc"), "****");
        assert_eq!(mask_secret("abcde"), "abcd****");
    }

    #[test]
    fn no_secrets_in_clean_text() {
        let s = scanner();
        let matches = s.scan_text("This is a normal sentence with nothing secret.");
        assert!(matches.is_empty());
    }

    #[test]
    fn risk_level_none_for_empty() {
        assert_eq!(SecretScanner::risk_level(&[]), RiskLevel::None);
    }

    #[test]
    fn scan_result_aggregation() {
        let s = scanner();
        let fake_key = format!("AKIA{}", "IOSFODNN7EXAMPLE");
        let text = format!("{fake_key}\npostgres://u:p@h:5432/db");
        let result = s.scan(text);
        assert_eq!(result.files_scanned, 1);
        assert!(result.matches.len() >= 2);
        assert!(result.risk_level >= RiskLevel::High);
    }

    #[test]
    fn line_numbers_are_correct() {
        let s = scanner();
        let fake_key = format!("AKIA{}", "IOSFODNN7EXAMPLE");
        let text = format!("line1\nline2\n{fake_key}\nline4");
        let matches = s.scan_text(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line, 3);
    }
}
