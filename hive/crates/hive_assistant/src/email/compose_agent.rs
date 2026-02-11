use serde::{Deserialize, Serialize};

use crate::email::UnifiedEmail;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A drafted email ready for review before sending.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftedEmail {
    pub to: String,
    pub subject: String,
    pub body: String,
    /// How confident the agent is in this draft (0.0 - 1.0).
    pub confidence: f64,
}

// ---------------------------------------------------------------------------
// ComposeAgent
// ---------------------------------------------------------------------------

/// Agent that drafts emails from natural-language instructions or in reply
/// to existing emails.
pub struct ComposeAgent;

impl ComposeAgent {
    pub fn new() -> Self {
        Self
    }

    /// Draft an email from a natural-language instruction.
    ///
    /// For example: "Send a follow-up to Alice about the Q1 report."
    ///
    /// TODO: implement with actual AI generation
    pub fn draft_from_instruction(&self, instruction: &str) -> Result<DraftedEmail, String> {
        // TODO: implement with actual AI generation
        Ok(DraftedEmail {
            to: String::new(),
            subject: format!("Re: {instruction}"),
            body: format!("Draft based on instruction: {instruction}"),
            confidence: 0.0,
        })
    }

    /// Draft a reply to an existing email.
    ///
    /// TODO: implement with actual AI generation
    pub fn draft_reply(
        &self,
        original: &UnifiedEmail,
        instruction: &str,
    ) -> Result<DraftedEmail, String> {
        // TODO: implement with actual AI generation
        Ok(DraftedEmail {
            to: original.from.clone(),
            subject: format!("Re: {}", original.subject),
            body: format!(
                "Reply to '{}' based on instruction: {instruction}",
                original.subject
            ),
            confidence: 0.0,
        })
    }
}

impl Default for ComposeAgent {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::email::compose_agent::{ComposeAgent, DraftedEmail};
    use crate::email::{EmailProvider, UnifiedEmail};

    fn make_original_email() -> UnifiedEmail {
        UnifiedEmail {
            id: "orig-1".to_string(),
            from: "alice@example.com".to_string(),
            to: "me@example.com".to_string(),
            subject: "Q1 Report".to_string(),
            body: "Please review the attached Q1 report.".to_string(),
            timestamp: "2026-02-10T10:00:00Z".to_string(),
            provider: EmailProvider::Gmail,
            read: true,
            important: true,
        }
    }

    #[test]
    fn test_draft_from_instruction() {
        let agent = ComposeAgent::new();
        let draft = agent
            .draft_from_instruction("Follow up with Bob about the meeting")
            .unwrap();

        assert!(draft.subject.contains("Follow up with Bob"));
        assert!(draft.body.contains("Follow up with Bob"));
        assert!((draft.confidence - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_draft_reply() {
        let agent = ComposeAgent::new();
        let original = make_original_email();

        let draft = agent
            .draft_reply(&original, "Acknowledge and confirm review by Friday")
            .unwrap();

        assert_eq!(draft.to, "alice@example.com");
        assert_eq!(draft.subject, "Re: Q1 Report");
        assert!(draft.body.contains("Q1 Report"));
        assert!(draft.body.contains("Acknowledge"));
    }

    #[test]
    fn test_draft_reply_preserves_sender() {
        let agent = ComposeAgent::new();
        let original = make_original_email();

        let draft = agent.draft_reply(&original, "Thanks!").unwrap();
        assert_eq!(draft.to, original.from);
    }

    #[test]
    fn test_drafted_email_serialization() {
        let draft = DraftedEmail {
            to: "test@example.com".to_string(),
            subject: "Test".to_string(),
            body: "Body".to_string(),
            confidence: 0.85,
        };
        let json = serde_json::to_string(&draft).unwrap();
        let deserialized: DraftedEmail = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.to, "test@example.com");
        assert!((deserialized.confidence - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_compose_agent() {
        let agent = ComposeAgent::default();
        let draft = agent.draft_from_instruction("test").unwrap();
        assert!(draft.subject.contains("test"));
    }
}
