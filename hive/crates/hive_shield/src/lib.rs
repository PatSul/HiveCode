pub mod access_control;
pub mod pii;
pub mod secrets;
pub mod shield;
pub mod vulnerability;

// Re-export core types at crate root for convenience.
pub use access_control::{
    AccessDecision, AccessPolicy, DataClassification, PolicyEngine, ProviderTrust,
};
pub use pii::{CloakFormat, CloakedText, PiiConfig, PiiDetector, PiiMatch, PiiReport, PiiType};
pub use secrets::{RiskLevel, ScanResult, SecretMatch, SecretScanner, SecretType};
pub use shield::{HiveShield, ShieldAction, ShieldConfig, ShieldResult};
pub use vulnerability::{
    Assessment, DetectedThreat, PromptThreat, ThreatLevel, VulnerabilityAssessor,
};
