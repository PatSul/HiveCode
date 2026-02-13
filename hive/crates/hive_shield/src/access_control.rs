use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// How sensitive the data is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DataClassification {
    Public,
    Internal,
    Confidential,
    Restricted,
}

impl std::fmt::Display for DataClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataClassification::Public => write!(f, "public"),
            DataClassification::Internal => write!(f, "internal"),
            DataClassification::Confidential => write!(f, "confidential"),
            DataClassification::Restricted => write!(f, "restricted"),
        }
    }
}

/// How much we trust the AI provider receiving our data.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProviderTrust {
    /// Fully trusted local model (e.g. ollama, LM Studio).
    Local,
    /// Cloud provider with a signed data-processing agreement.
    Trusted,
    /// Standard cloud API with default terms of service.
    Standard,
    /// Unknown or unreviewed provider.
    Untrusted,
}

impl std::fmt::Display for ProviderTrust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderTrust::Local => write!(f, "local"),
            ProviderTrust::Trusted => write!(f, "trusted"),
            ProviderTrust::Standard => write!(f, "standard"),
            ProviderTrust::Untrusted => write!(f, "untrusted"),
        }
    }
}

/// Policy governing what data may be sent to a given provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessPolicy {
    /// Trust level of the provider this policy applies to.
    pub provider_trust: ProviderTrust,
    /// The highest data classification allowed to be sent to this provider.
    pub max_classification: DataClassification,
    /// Whether PII must be cloaked before sending to this provider.
    pub require_pii_cloaking: bool,
    /// Optional allowlist of data types (e.g. "code", "logs"). Empty means
    /// all types are allowed.
    pub allowed_data_types: Vec<String>,
    /// Regex patterns that must NOT appear in outgoing data.
    pub blocked_patterns: Vec<String>,
}

/// The result of an access-control check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessDecision {
    pub allowed: bool,
    pub reason: String,
    /// Actions that must be taken before sending (e.g. "cloak PII").
    pub required_actions: Vec<String>,
}

// ---------------------------------------------------------------------------
// PolicyEngine
// ---------------------------------------------------------------------------

/// Evaluates access policies to decide whether data may be sent to a provider.
pub struct PolicyEngine {
    policies: HashMap<String, AccessPolicy>,
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self {
            policies: HashMap::new(),
        }
    }

    /// Register a policy for a named provider.
    pub fn add_policy(&mut self, provider: &str, policy: AccessPolicy) {
        self.policies.insert(provider.to_string(), policy);
    }

    /// Check whether data with the given classification may be sent to
    /// `provider`, considering whether the data contains PII.
    pub fn check_access(
        &self,
        provider: &str,
        data_classification: DataClassification,
        contains_pii: bool,
    ) -> AccessDecision {
        let policy = match self.policies.get(provider) {
            Some(p) => p,
            None => {
                return self.check_against_policy(
                    &Self::default_policy(),
                    data_classification,
                    contains_pii,
                );
            }
        };
        self.check_against_policy(policy, data_classification, contains_pii)
    }

    /// A sensible default policy: standard trust, up to internal data,
    /// PII cloaking required.
    pub fn default_policy() -> AccessPolicy {
        AccessPolicy {
            provider_trust: ProviderTrust::Standard,
            max_classification: DataClassification::Internal,
            require_pii_cloaking: true,
            allowed_data_types: Vec::new(),
            blocked_patterns: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn check_against_policy(
        &self,
        policy: &AccessPolicy,
        data_classification: DataClassification,
        contains_pii: bool,
    ) -> AccessDecision {
        let mut required_actions = Vec::new();

        // Untrusted providers are blocked outright for anything above Public.
        if policy.provider_trust == ProviderTrust::Untrusted
            && data_classification > DataClassification::Public
        {
            return AccessDecision {
                allowed: false,
                reason: "Untrusted providers may only receive public data".to_string(),
                required_actions: Vec::new(),
            };
        }

        // Classification check: data must not exceed the policy ceiling.
        if data_classification > policy.max_classification {
            return AccessDecision {
                allowed: false,
                reason: format!(
                    "Data classification '{}' exceeds maximum allowed '{}' for this provider",
                    data_classification, policy.max_classification
                ),
                required_actions: Vec::new(),
            };
        }

        // PII check.
        if contains_pii && policy.require_pii_cloaking {
            required_actions.push("cloak_pii".to_string());
        }

        AccessDecision {
            allowed: true,
            reason: "Access granted".to_string(),
            required_actions,
        }
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_with_policies() -> PolicyEngine {
        let mut engine = PolicyEngine::new();

        engine.add_policy(
            "ollama",
            AccessPolicy {
                provider_trust: ProviderTrust::Local,
                max_classification: DataClassification::Restricted,
                require_pii_cloaking: false,
                allowed_data_types: Vec::new(),
                blocked_patterns: Vec::new(),
            },
        );

        engine.add_policy(
            "openai",
            AccessPolicy {
                provider_trust: ProviderTrust::Trusted,
                max_classification: DataClassification::Confidential,
                require_pii_cloaking: true,
                allowed_data_types: Vec::new(),
                blocked_patterns: Vec::new(),
            },
        );

        engine.add_policy(
            "shady-api",
            AccessPolicy {
                provider_trust: ProviderTrust::Untrusted,
                max_classification: DataClassification::Public,
                require_pii_cloaking: true,
                allowed_data_types: Vec::new(),
                blocked_patterns: Vec::new(),
            },
        );

        engine
    }

    #[test]
    fn local_provider_allows_restricted() {
        let engine = engine_with_policies();
        let decision = engine.check_access("ollama", DataClassification::Restricted, false);
        assert!(decision.allowed);
    }

    #[test]
    fn local_provider_no_pii_cloaking_needed() {
        let engine = engine_with_policies();
        let decision = engine.check_access("ollama", DataClassification::Internal, true);
        assert!(decision.allowed);
        assert!(decision.required_actions.is_empty());
    }

    #[test]
    fn trusted_provider_requires_pii_cloaking() {
        let engine = engine_with_policies();
        let decision = engine.check_access("openai", DataClassification::Internal, true);
        assert!(decision.allowed);
        assert!(decision.required_actions.contains(&"cloak_pii".to_string()));
    }

    #[test]
    fn trusted_provider_blocks_restricted() {
        let engine = engine_with_policies();
        let decision = engine.check_access("openai", DataClassification::Restricted, false);
        assert!(!decision.allowed);
        assert!(decision.reason.contains("exceeds"));
    }

    #[test]
    fn untrusted_provider_blocks_internal() {
        let engine = engine_with_policies();
        let decision = engine.check_access("shady-api", DataClassification::Internal, false);
        assert!(!decision.allowed);
        assert!(decision.reason.contains("Untrusted"));
    }

    #[test]
    fn untrusted_provider_allows_public() {
        let engine = engine_with_policies();
        let decision = engine.check_access("shady-api", DataClassification::Public, false);
        assert!(decision.allowed);
    }

    #[test]
    fn unknown_provider_gets_default_policy() {
        let engine = engine_with_policies();
        let decision = engine.check_access("unknown-llm", DataClassification::Internal, false);
        // Default policy allows up to Internal.
        assert!(decision.allowed);
    }

    #[test]
    fn unknown_provider_blocks_confidential() {
        let engine = engine_with_policies();
        let decision = engine.check_access("unknown-llm", DataClassification::Confidential, false);
        assert!(!decision.allowed);
    }

    #[test]
    fn default_policy_requires_pii_cloaking() {
        let policy = PolicyEngine::default_policy();
        assert!(policy.require_pii_cloaking);
        assert_eq!(policy.max_classification, DataClassification::Internal);
    }

    #[test]
    fn access_decision_display() {
        let decision = AccessDecision {
            allowed: true,
            reason: "Access granted".to_string(),
            required_actions: vec!["cloak_pii".to_string()],
        };
        assert_eq!(decision.reason, "Access granted");
        assert_eq!(decision.required_actions.len(), 1);
    }
}
