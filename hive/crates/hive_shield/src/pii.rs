use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Categories of personally identifiable information.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PiiType {
    Email,
    Phone,
    SSN,
    CreditCard,
    IpAddress,
    Name,
    Address,
    DateOfBirth,
    Passport,
    DriversLicense,
    BankAccount,
    Custom(String),
}

impl std::fmt::Display for PiiType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PiiType::Email => write!(f, "EMAIL"),
            PiiType::Phone => write!(f, "PHONE"),
            PiiType::SSN => write!(f, "SSN"),
            PiiType::CreditCard => write!(f, "CREDIT_CARD"),
            PiiType::IpAddress => write!(f, "IP_ADDRESS"),
            PiiType::Name => write!(f, "NAME"),
            PiiType::Address => write!(f, "ADDRESS"),
            PiiType::DateOfBirth => write!(f, "DOB"),
            PiiType::Passport => write!(f, "PASSPORT"),
            PiiType::DriversLicense => write!(f, "DRIVERS_LICENSE"),
            PiiType::BankAccount => write!(f, "BANK_ACCOUNT"),
            PiiType::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// How to replace detected PII in the output text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloakFormat {
    /// Replace with `[TYPE_N]` (e.g. `[EMAIL_1]`).
    Placeholder,
    /// Replace with a truncated SHA-256 hex prefix.
    Hash,
    /// Replace with `****` asterisks.
    Redact,
}

/// Configuration for PII detection and cloaking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiConfig {
    /// Which PII types to scan for. Empty means scan for all built-in types.
    pub types_to_detect: Vec<PiiType>,
    /// How to replace detected PII values.
    pub cloaking_format: CloakFormat,
    /// Whether to preserve the character-length of the original value when
    /// using `CloakFormat::Redact`.
    pub preserve_format: bool,
}

impl Default for PiiConfig {
    fn default() -> Self {
        Self {
            types_to_detect: Vec::new(),
            cloaking_format: CloakFormat::Placeholder,
            preserve_format: false,
        }
    }
}

/// A single PII match found in the input text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiMatch {
    pub pii_type: PiiType,
    pub original: String,
    pub replacement: String,
    pub start: usize,
    pub end: usize,
    pub confidence: f64,
}

/// The result of cloaking a piece of text: the transformed text plus the
/// mapping needed to restore the originals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloakedText {
    pub text: String,
    pub matches: Vec<PiiMatch>,
    /// Maps replacement tokens back to original values.
    pub cloak_map: HashMap<String, String>,
}

/// Aggregate report of PII found in a piece of text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiReport {
    pub total_found: usize,
    pub by_type: HashMap<String, usize>,
    pub risk_level: String,
}

// ---------------------------------------------------------------------------
// Compiled regex patterns (built once, reused)
// ---------------------------------------------------------------------------

struct PiiPattern {
    pii_type: PiiType,
    regex: Regex,
    confidence: f64,
}

static PII_PATTERNS: Lazy<Vec<PiiPattern>> = Lazy::new(|| {
    vec![
        PiiPattern {
            pii_type: PiiType::Email,
            regex: Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").expect("valid regex: email"),
            confidence: 0.95,
        },
        PiiPattern {
            pii_type: PiiType::SSN,
            regex: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").expect("valid regex: SSN"),
            confidence: 0.90,
        },
        PiiPattern {
            pii_type: PiiType::CreditCard,
            regex: Regex::new(r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b").expect("valid regex: credit card"),
            confidence: 0.85,
        },
        PiiPattern {
            pii_type: PiiType::Phone,
            regex: Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").expect("valid regex: phone"),
            confidence: 0.80,
        },
        PiiPattern {
            pii_type: PiiType::IpAddress,
            regex: Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").expect("valid regex: IP address"),
            confidence: 0.75,
        },
    ]
});

// ---------------------------------------------------------------------------
// PiiDetector
// ---------------------------------------------------------------------------

/// Detects and cloaks personally identifiable information in text.
pub struct PiiDetector {
    config: PiiConfig,
}

impl PiiDetector {
    pub fn new(config: PiiConfig) -> Self {
        Self { config }
    }

    /// Scan `text` and return all PII matches (without modifying the text).
    pub fn detect(&self, text: &str) -> Vec<PiiMatch> {
        let mut matches = Vec::new();

        for pattern in PII_PATTERNS.iter() {
            if !self.should_detect(&pattern.pii_type) {
                continue;
            }
            for m in pattern.regex.find_iter(text) {
                matches.push(PiiMatch {
                    pii_type: pattern.pii_type.clone(),
                    original: m.as_str().to_string(),
                    replacement: String::new(), // filled during cloaking
                    start: m.start(),
                    end: m.end(),
                    confidence: pattern.confidence,
                });
            }
        }

        // Sort by start position so cloaking works left-to-right.
        matches.sort_by_key(|m| m.start);

        // Deduplicate overlapping matches (keep the one with higher confidence).
        deduplicate_overlapping(&mut matches);

        matches
    }

    /// Replace all detected PII with cloaked tokens, returning the cloaked
    /// text and a mapping to restore the originals.
    pub fn cloak(&self, text: &str) -> CloakedText {
        let mut matches = self.detect(text);
        let mut cloak_map: HashMap<String, String> = HashMap::new();
        let mut type_counters: HashMap<String, usize> = HashMap::new();

        // Assign replacements.
        for m in &mut matches {
            let type_label = m.pii_type.to_string();
            let counter = type_counters.entry(type_label.clone()).or_insert(0);
            *counter += 1;

            m.replacement = match &self.config.cloaking_format {
                CloakFormat::Placeholder => format!("[{type_label}_{counter}]"),
                CloakFormat::Hash => {
                    let mut hasher = Sha256::new();
                    hasher.update(m.original.as_bytes());
                    let hash = hasher.finalize();
                    format!(
                        "[{type_label}_{:x}]",
                        &hash[..4].iter().fold(0u32, |acc, &b| (acc << 8) | b as u32)
                    )
                }
                CloakFormat::Redact => {
                    if self.config.preserve_format {
                        "*".repeat(m.original.len())
                    } else {
                        "****".to_string()
                    }
                }
            };

            cloak_map.insert(m.replacement.clone(), m.original.clone());
        }

        // Build the cloaked text by replacing from right to left so that
        // earlier indices remain valid.
        let mut result = text.to_string();
        for m in matches.iter().rev() {
            result.replace_range(m.start..m.end, &m.replacement);
        }

        CloakedText {
            text: result,
            matches,
            cloak_map,
        }
    }

    /// Restore original PII values in a cloaked text.
    pub fn uncloak(cloaked: &CloakedText) -> String {
        let mut text = cloaked.text.clone();
        for (replacement, original) in &cloaked.cloak_map {
            text = text.replace(replacement, original);
        }
        text
    }

    /// Produce an aggregate report of PII found in `text`.
    pub fn detect_and_report(&self, text: &str) -> PiiReport {
        let matches = self.detect(text);
        let total_found = matches.len();

        let mut by_type: HashMap<String, usize> = HashMap::new();
        for m in &matches {
            *by_type.entry(m.pii_type.to_string()).or_insert(0) += 1;
        }

        let risk_level = match total_found {
            0 => "none",
            1..=2 => "low",
            3..=5 => "medium",
            _ => "high",
        };

        PiiReport {
            total_found,
            by_type,
            risk_level: risk_level.to_string(),
        }
    }

    /// Returns `true` if `pii_type` should be scanned according to the
    /// current config (empty `types_to_detect` means scan everything).
    fn should_detect(&self, pii_type: &PiiType) -> bool {
        self.config.types_to_detect.is_empty() || self.config.types_to_detect.contains(pii_type)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Remove overlapping matches, preferring higher-confidence entries.
fn deduplicate_overlapping(matches: &mut Vec<PiiMatch>) {
    if matches.len() < 2 {
        return;
    }
    let mut keep = vec![true; matches.len()];
    for i in 0..matches.len() {
        if !keep[i] {
            continue;
        }
        for j in (i + 1)..matches.len() {
            if !keep[j] {
                continue;
            }
            // Check for overlap.
            if matches[j].start < matches[i].end {
                // Discard the one with lower confidence.
                if matches[j].confidence > matches[i].confidence {
                    keep[i] = false;
                    break;
                } else {
                    keep[j] = false;
                }
            }
        }
    }
    let mut idx = 0;
    matches.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_detector() -> PiiDetector {
        PiiDetector::new(PiiConfig::default())
    }

    #[test]
    fn detect_email() {
        let det = default_detector();
        let matches = det.detect("Contact us at alice@example.com for info.");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pii_type, PiiType::Email);
        assert_eq!(matches[0].original, "alice@example.com");
    }

    #[test]
    fn detect_phone() {
        let det = default_detector();
        let matches = det.detect("Call 555-123-4567 today.");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pii_type, PiiType::Phone);
        assert_eq!(matches[0].original, "555-123-4567");
    }

    #[test]
    fn detect_ssn() {
        let det = default_detector();
        let matches = det.detect("SSN: 123-45-6789");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pii_type, PiiType::SSN);
    }

    #[test]
    fn detect_credit_card() {
        let det = default_detector();
        let matches = det.detect("Card: 4111-1111-1111-1111");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pii_type, PiiType::CreditCard);
    }

    #[test]
    fn detect_ip_address() {
        let det = default_detector();
        let matches = det.detect("Server at 192.168.1.100");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pii_type, PiiType::IpAddress);
    }

    #[test]
    fn detect_multiple_pii() {
        let det = default_detector();
        let text = "Email: bob@test.org, SSN: 987-65-4321, IP: 10.0.0.1";
        let matches = det.detect(text);
        assert!(
            matches.len() >= 3,
            "expected at least 3 matches, got {}",
            matches.len()
        );
    }

    #[test]
    fn cloak_and_uncloak_placeholder() {
        let det = default_detector();
        let text = "Hi alice@example.com, your SSN is 123-45-6789.";
        let cloaked = det.cloak(text);

        assert!(!cloaked.text.contains("alice@example.com"));
        assert!(!cloaked.text.contains("123-45-6789"));
        assert!(cloaked.text.contains("[EMAIL_1]"));
        assert!(cloaked.text.contains("[SSN_1]"));

        let restored = PiiDetector::uncloak(&cloaked);
        assert_eq!(restored, text);
    }

    #[test]
    fn cloak_redact_preserve_format() {
        let config = PiiConfig {
            types_to_detect: vec![PiiType::Email],
            cloaking_format: CloakFormat::Redact,
            preserve_format: true,
        };
        let det = PiiDetector::new(config);
        let cloaked = det.cloak("email: a@b.co");
        assert!(
            cloaked.text.contains("******"),
            "cloaked text: {}",
            cloaked.text
        );
    }

    #[test]
    fn cloak_hash_format() {
        let config = PiiConfig {
            types_to_detect: vec![],
            cloaking_format: CloakFormat::Hash,
            preserve_format: false,
        };
        let det = PiiDetector::new(config);
        let cloaked = det.cloak("email: alice@example.com");
        assert!(
            cloaked.text.contains("[EMAIL_"),
            "cloaked: {}",
            cloaked.text
        );
        assert!(!cloaked.text.contains("alice@example.com"));
    }

    #[test]
    fn detect_and_report_risk_levels() {
        let det = default_detector();

        let report = det.detect_and_report("nothing here");
        assert_eq!(report.risk_level, "none");

        let report = det.detect_and_report("email: a@b.com");
        assert_eq!(report.risk_level, "low");

        let report =
            det.detect_and_report("a@b.com c@d.com e@f.com 123-45-6789 555.123.4567 10.0.0.1");
        assert_eq!(report.risk_level, "high");
    }

    #[test]
    fn filter_by_type() {
        let config = PiiConfig {
            types_to_detect: vec![PiiType::Email],
            cloaking_format: CloakFormat::Placeholder,
            preserve_format: false,
        };
        let det = PiiDetector::new(config);
        let matches = det.detect("Email: a@b.com SSN: 123-45-6789");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pii_type, PiiType::Email);
    }

    #[test]
    fn no_matches_in_clean_text() {
        let det = default_detector();
        let matches = det.detect("This is a perfectly clean sentence with no PII.");
        assert_eq!(matches.len(), 0);
    }
}
