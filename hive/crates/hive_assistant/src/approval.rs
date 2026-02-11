use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::AssistantStorage;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Severity level of an approval request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Current status of an approval request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

/// A request that requires human approval before proceeding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub action: String,
    pub resource: String,
    pub level: ApprovalLevel,
    pub requested_by: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// ApprovalService
// ---------------------------------------------------------------------------

/// Service for managing approval workflows.
///
/// Actions that require human sign-off (e.g. deploying to production,
/// deleting resources, sending emails above a certain sensitivity) are
/// submitted as `ApprovalRequest`s and must be explicitly approved or
/// rejected before proceeding.
pub struct ApprovalService {
    storage: Arc<AssistantStorage>,
}

impl ApprovalService {
    pub fn new(storage: Arc<AssistantStorage>) -> Self {
        Self { storage }
    }

    /// Submit a new approval request.
    pub fn submit(
        &self,
        action: &str,
        resource: &str,
        level: ApprovalLevel,
        requested_by: &str,
    ) -> Result<ApprovalRequest, String> {
        let request = ApprovalRequest {
            id: Uuid::new_v4().to_string(),
            action: action.to_string(),
            resource: resource.to_string(),
            level,
            requested_by: requested_by.to_string(),
            created_at: Utc::now().to_rfc3339(),
        };
        self.storage.insert_approval(&request)?;
        Ok(request)
    }

    /// Approve a pending request.
    pub fn approve(&self, id: &str, decided_by: &str) -> Result<(), String> {
        let exists = self.storage.get_approval(id)?;
        if exists.is_none() {
            return Err(format!("Approval request '{id}' not found"));
        }
        self.storage.update_approval_decision(
            id,
            "approved",
            decided_by,
            &Utc::now().to_rfc3339(),
        )?;
        Ok(())
    }

    /// Reject a pending request.
    pub fn reject(&self, id: &str, decided_by: &str) -> Result<(), String> {
        let exists = self.storage.get_approval(id)?;
        if exists.is_none() {
            return Err(format!("Approval request '{id}' not found"));
        }
        self.storage.update_approval_decision(
            id,
            "rejected",
            decided_by,
            &Utc::now().to_rfc3339(),
        )?;
        Ok(())
    }

    /// List all pending approval requests.
    pub fn list_pending(&self) -> Result<Vec<ApprovalRequest>, String> {
        self.storage.list_approvals_by_status("pending")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::approval::{ApprovalLevel, ApprovalService};
    use crate::storage::AssistantStorage;

    fn make_service() -> ApprovalService {
        let storage = Arc::new(AssistantStorage::in_memory().unwrap());
        ApprovalService::new(storage)
    }

    #[test]
    fn test_submit_and_list_pending() {
        let service = make_service();

        service
            .submit("deploy", "prod-server", ApprovalLevel::High, "bot")
            .unwrap();
        service
            .submit("delete", "temp-files", ApprovalLevel::Low, "bot")
            .unwrap();

        let pending = service.list_pending().unwrap();
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_approve_removes_from_pending() {
        let service = make_service();

        let request = service
            .submit("deploy", "staging", ApprovalLevel::Medium, "bot")
            .unwrap();

        service.approve(&request.id, "admin").unwrap();

        let pending = service.list_pending().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_reject_removes_from_pending() {
        let service = make_service();

        let request = service
            .submit("delete", "database", ApprovalLevel::Critical, "bot")
            .unwrap();

        service.reject(&request.id, "admin").unwrap();

        let pending = service.list_pending().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_approve_nonexistent_errors() {
        let service = make_service();
        let result = service.approve("nonexistent", "admin");
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_nonexistent_errors() {
        let service = make_service();
        let result = service.reject("nonexistent", "admin");
        assert!(result.is_err());
    }

    #[test]
    fn test_submit_returns_populated_request() {
        let service = make_service();

        let request = service
            .submit("send_email", "user@example.com", ApprovalLevel::Medium, "email_agent")
            .unwrap();

        assert!(!request.id.is_empty());
        assert_eq!(request.action, "send_email");
        assert_eq!(request.resource, "user@example.com");
        assert!(matches!(request.level, ApprovalLevel::Medium));
        assert_eq!(request.requested_by, "email_agent");
        assert!(!request.created_at.is_empty());
    }

    #[test]
    fn test_multiple_approve_reject_mixed() {
        let service = make_service();

        let r1 = service
            .submit("action1", "res1", ApprovalLevel::Low, "bot")
            .unwrap();
        let r2 = service
            .submit("action2", "res2", ApprovalLevel::High, "bot")
            .unwrap();
        let _r3 = service
            .submit("action3", "res3", ApprovalLevel::Medium, "bot")
            .unwrap();

        service.approve(&r1.id, "admin").unwrap();
        service.reject(&r2.id, "admin").unwrap();

        let pending = service.list_pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].action, "action3");
    }
}
