pub mod pii;
pub mod secrets;
pub mod vulnerability;
pub mod access_control;
pub mod shield;

// Re-export core types at crate root for convenience.
pub use pii::{CloakedText, CloakFormat, PiiConfig, PiiDetector, PiiMatch, PiiReport, PiiType};
pub use secrets::{RiskLevel, ScanResult, SecretMatch, SecretScanner, SecretType};
pub use vulnerability::{Assessment, DetectedThreat, PromptThreat, ThreatLevel, VulnerabilityAssessor};
pub use access_control::{AccessDecision, AccessPolicy, DataClassification, PolicyEngine, ProviderTrust};
pub use shield::{HiveShield, ShieldAction, ShieldConfig, ShieldResult};
