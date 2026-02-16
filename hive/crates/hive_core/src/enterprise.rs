use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::debug;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Permission level of a team member within an enterprise team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeamRole {
    /// Full control over the team; cannot be removed or demoted.
    Owner,
    /// Can manage members and team settings.
    Admin,
    /// Standard team member with read/write access.
    Member,
    /// Read-only access to team resources.
    Viewer,
}

/// A member of an enterprise team with role and join timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub id: String,
    pub name: String,
    pub email: String,
    pub role: TeamRole,
    pub joined_at: DateTime<Utc>,
}

/// An enterprise team with members and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub description: String,
    pub members: Vec<TeamMember>,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

/// Auditable action types recorded in the enterprise audit log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditAction {
    /// User signed in.
    Login,
    /// User signed out.
    Logout,
    /// A new team was created.
    CreateTeam,
    /// Team settings were modified.
    UpdateTeam,
    /// A team was deleted.
    DeleteTeam,
    /// A member was added to a team.
    AddMember,
    /// A member was removed from a team.
    RemoveMember,
    /// A team member's role was changed.
    ChangeRole,
    /// Application configuration was modified.
    ConfigChange,
    /// An API key was accessed or rotated.
    ApiKeyAccess,
    /// Data was exported from the system.
    DataExport,
    /// A security-related event occurred.
    SecurityEvent,
}

/// A single entry in the enterprise audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub user_id: String,
    pub user_name: String,
    pub action: AuditAction,
    pub resource_type: String,
    pub resource_id: String,
    pub details: Option<String>,
    pub ip_address: Option<String>,
}

/// A recorded usage metric tracking token consumption and cost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetric {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub user_id: String,
    pub action_type: String,
    pub tokens_used: u64,
    pub cost_usd: f64,
    pub model: String,
}

// ---------------------------------------------------------------------------
// EnterpriseService — in-memory store
// ---------------------------------------------------------------------------

/// In-memory enterprise service managing teams, audit logs, and usage metrics.
#[derive(Serialize, Deserialize)]
pub struct EnterpriseService {
    teams: Vec<Team>,
    audit_log: Vec<AuditEntry>,
    usage_metrics: Vec<UsageMetric>,
}

impl EnterpriseService {
    /// Creates a new, empty enterprise service.
    pub fn new() -> Self {
        Self {
            teams: Vec::new(),
            audit_log: Vec::new(),
            usage_metrics: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Team management
    // -----------------------------------------------------------------------

    /// Creates a new team with the creator as Owner. Returns a clone of the
    /// created team.
    pub fn create_team(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        creator_name: impl Into<String>,
        creator_email: impl Into<String>,
    ) -> Team {
        let now = Utc::now();
        let creator_name = creator_name.into();

        let owner = TeamMember {
            id: Uuid::new_v4().to_string(),
            name: creator_name.clone(),
            email: creator_email.into(),
            role: TeamRole::Owner,
            joined_at: now,
        };

        let team = Team {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: description.into(),
            members: vec![owner],
            created_at: now,
            created_by: creator_name,
        };

        debug!("Created team: {} ({})", team.name, team.id);

        let clone = team.clone();
        self.teams.push(team);
        clone
    }

    /// Returns a reference to a team by ID, or `None` if not found.
    pub fn get_team(&self, id: &str) -> Option<&Team> {
        self.teams.iter().find(|t| t.id == id)
    }

    /// Returns a slice of all teams.
    pub fn list_teams(&self) -> &[Team] {
        &self.teams
    }

    /// Returns the number of teams.
    pub fn team_count(&self) -> usize {
        self.teams.len()
    }

    /// Adds a member to a team. Returns an error if the team is not found or
    /// a member with the same email already exists.
    pub fn add_member(
        &mut self,
        team_id: &str,
        name: impl Into<String>,
        email: impl Into<String>,
        role: TeamRole,
    ) -> Result<TeamMember> {
        let email = email.into();
        let team = self
            .teams
            .iter_mut()
            .find(|t| t.id == team_id)
            .ok_or_else(|| anyhow::anyhow!("Team not found: {team_id}"))
            .context("Failed to add member")?;

        // Check for duplicate email
        if team.members.iter().any(|m| m.email == email) {
            bail!(
                "A member with email '{}' already exists in this team",
                email
            );
        }

        let member = TeamMember {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            email,
            role,
            joined_at: Utc::now(),
        };

        debug!("Added member {} to team {}", member.name, team.name);

        let clone = member.clone();
        team.members.push(member);
        Ok(clone)
    }

    /// Removes a member from a team. Returns an error if the team or member
    /// is not found, or if the member is the Owner.
    pub fn remove_member(&mut self, team_id: &str, member_id: &str) -> Result<()> {
        let team = self
            .teams
            .iter_mut()
            .find(|t| t.id == team_id)
            .ok_or_else(|| anyhow::anyhow!("Team not found: {team_id}"))?;

        let idx = team
            .members
            .iter()
            .position(|m| m.id == member_id)
            .ok_or_else(|| anyhow::anyhow!("Member not found: {member_id}"))?;

        if team.members[idx].role == TeamRole::Owner {
            bail!("Cannot remove the team Owner");
        }

        debug!(
            "Removed member {} from team {}",
            team.members[idx].name, team.name
        );
        team.members.remove(idx);
        Ok(())
    }

    /// Changes the role of a team member. Returns an error if the team or
    /// member is not found, or if the member is the Owner.
    pub fn change_role(
        &mut self,
        team_id: &str,
        member_id: &str,
        new_role: TeamRole,
    ) -> Result<()> {
        let team = self
            .teams
            .iter_mut()
            .find(|t| t.id == team_id)
            .ok_or_else(|| anyhow::anyhow!("Team not found: {team_id}"))?;

        let member = team
            .members
            .iter_mut()
            .find(|m| m.id == member_id)
            .ok_or_else(|| anyhow::anyhow!("Member not found: {member_id}"))?;

        if member.role == TeamRole::Owner {
            bail!("Cannot change the role of the team Owner");
        }

        debug!(
            "Changed role of {} from {:?} to {:?} in team {}",
            member.name, member.role, new_role, team.name
        );
        member.role = new_role;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Audit logging
    // -----------------------------------------------------------------------

    /// Creates an audit log entry and returns a clone of it.
    #[allow(clippy::too_many_arguments)]
    pub fn log_audit(
        &mut self,
        user_id: impl Into<String>,
        user_name: impl Into<String>,
        action: AuditAction,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        details: Option<String>,
        ip_address: Option<String>,
    ) -> AuditEntry {
        let entry = AuditEntry {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            user_id: user_id.into(),
            user_name: user_name.into(),
            action,
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            details,
            ip_address,
        };

        debug!(
            "Audit: {:?} by {} on {}",
            entry.action, entry.user_name, entry.resource_id
        );

        let clone = entry.clone();
        self.audit_log.push(entry);
        clone
    }

    /// Returns the most recent audit entries, up to `limit`.
    pub fn get_audit_log(&self, limit: usize) -> Vec<&AuditEntry> {
        self.audit_log.iter().rev().take(limit).collect()
    }

    /// Returns audit entries for a specific user, up to `limit`.
    pub fn get_audit_by_user(&self, user_id: &str, limit: usize) -> Vec<&AuditEntry> {
        self.audit_log
            .iter()
            .rev()
            .filter(|e| e.user_id == user_id)
            .take(limit)
            .collect()
    }

    /// Returns the total number of audit entries.
    pub fn audit_count(&self) -> usize {
        self.audit_log.len()
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    /// Persist the enterprise service to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load an enterprise service from a JSON file. Returns an empty service
    /// if the file does not exist.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let json = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
    }

    // -----------------------------------------------------------------------
    // Usage tracking
    // -----------------------------------------------------------------------

    /// Records a usage metric and returns a clone of it.
    pub fn record_usage(
        &mut self,
        user_id: impl Into<String>,
        action_type: impl Into<String>,
        tokens_used: u64,
        cost_usd: f64,
        model: impl Into<String>,
    ) -> UsageMetric {
        let metric = UsageMetric {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            user_id: user_id.into(),
            action_type: action_type.into(),
            tokens_used,
            cost_usd,
            model: model.into(),
        };

        debug!(
            "Usage: {} tokens, ${:.4} for {}",
            metric.tokens_used, metric.cost_usd, metric.user_id
        );

        let clone = metric.clone();
        self.usage_metrics.push(metric);
        clone
    }

    /// Returns a tuple of `(total_tokens, total_cost)` for a specific user.
    pub fn get_usage_summary(&self, user_id: &str) -> (u64, f64) {
        let total_tokens: u64 = self
            .usage_metrics
            .iter()
            .filter(|m| m.user_id == user_id)
            .map(|m| m.tokens_used)
            .sum();

        let total_cost: f64 = self
            .usage_metrics
            .iter()
            .filter(|m| m.user_id == user_id)
            .map(|m| m.cost_usd)
            .sum();

        (total_tokens, total_cost)
    }
}

impl Default for EnterpriseService {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper
    // -----------------------------------------------------------------------

    fn make_service_with_team() -> (EnterpriseService, String) {
        let mut svc = EnterpriseService::new();
        let team = svc.create_team("Acme Corp", "Main team", "Alice", "alice@acme.com");
        (svc, team.id)
    }

    // -----------------------------------------------------------------------
    // 1. create_team
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_team() {
        let mut svc = EnterpriseService::new();
        let team = svc.create_team("Acme Corp", "Main team", "Alice", "alice@acme.com");

        assert_eq!(team.name, "Acme Corp");
        assert_eq!(team.description, "Main team");
        assert_eq!(team.created_by, "Alice");
        assert_eq!(team.members.len(), 1);
        assert_eq!(team.members[0].name, "Alice");
        assert_eq!(team.members[0].email, "alice@acme.com");
        assert_eq!(team.members[0].role, TeamRole::Owner);
        assert!(!team.id.is_empty());
        assert_eq!(svc.team_count(), 1);
    }

    // -----------------------------------------------------------------------
    // 2. get_team
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_team_found_and_not_found() {
        let (svc, id) = make_service_with_team();

        assert!(svc.get_team(&id).is_some());
        assert_eq!(svc.get_team(&id).unwrap().name, "Acme Corp");
        assert!(svc.get_team("nonexistent").is_none());
    }

    // -----------------------------------------------------------------------
    // 3. list_teams
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_teams() {
        let mut svc = EnterpriseService::new();
        assert!(svc.list_teams().is_empty());

        svc.create_team("Team A", "First", "Alice", "a@a.com");
        svc.create_team("Team B", "Second", "Bob", "b@b.com");
        svc.create_team("Team C", "Third", "Carol", "c@c.com");

        assert_eq!(svc.list_teams().len(), 3);
        assert_eq!(svc.team_count(), 3);
        assert_eq!(svc.list_teams()[0].name, "Team A");
        assert_eq!(svc.list_teams()[2].name, "Team C");
    }

    // -----------------------------------------------------------------------
    // 4. add_member
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_member() {
        let (mut svc, team_id) = make_service_with_team();

        let member = svc
            .add_member(&team_id, "Bob", "bob@acme.com", TeamRole::Member)
            .unwrap();

        assert_eq!(member.name, "Bob");
        assert_eq!(member.email, "bob@acme.com");
        assert_eq!(member.role, TeamRole::Member);

        let team = svc.get_team(&team_id).unwrap();
        assert_eq!(team.members.len(), 2);
    }

    // -----------------------------------------------------------------------
    // 5. add_member duplicate email
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_member_duplicate_email() {
        let (mut svc, team_id) = make_service_with_team();

        // alice@acme.com already exists (Owner)
        let result = svc.add_member(&team_id, "Alice Clone", "alice@acme.com", TeamRole::Member);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    // -----------------------------------------------------------------------
    // 6. add_member to nonexistent team
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_member_team_not_found() {
        let mut svc = EnterpriseService::new();
        let result = svc.add_member("bad-id", "Bob", "bob@b.com", TeamRole::Viewer);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // 7. remove_member
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_member() {
        let (mut svc, team_id) = make_service_with_team();

        let member = svc
            .add_member(&team_id, "Bob", "bob@acme.com", TeamRole::Member)
            .unwrap();

        assert_eq!(svc.get_team(&team_id).unwrap().members.len(), 2);
        svc.remove_member(&team_id, &member.id).unwrap();
        assert_eq!(svc.get_team(&team_id).unwrap().members.len(), 1);
    }

    // -----------------------------------------------------------------------
    // 8. remove_member — cannot remove Owner
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_owner_fails() {
        let (mut svc, team_id) = make_service_with_team();

        let owner_id = svc.get_team(&team_id).unwrap().members[0].id.clone();
        let result = svc.remove_member(&team_id, &owner_id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Owner"));
    }

    // -----------------------------------------------------------------------
    // 9. remove_member — member not found
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_member_not_found() {
        let (mut svc, team_id) = make_service_with_team();
        let result = svc.remove_member(&team_id, "nonexistent");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // 10. change_role
    // -----------------------------------------------------------------------

    #[test]
    fn test_change_role() {
        let (mut svc, team_id) = make_service_with_team();

        let member = svc
            .add_member(&team_id, "Bob", "bob@acme.com", TeamRole::Member)
            .unwrap();

        svc.change_role(&team_id, &member.id, TeamRole::Admin)
            .unwrap();

        let team = svc.get_team(&team_id).unwrap();
        let bob = team.members.iter().find(|m| m.id == member.id).unwrap();
        assert_eq!(bob.role, TeamRole::Admin);
    }

    // -----------------------------------------------------------------------
    // 11. change_role — cannot change Owner
    // -----------------------------------------------------------------------

    #[test]
    fn test_change_owner_role_fails() {
        let (mut svc, team_id) = make_service_with_team();

        let owner_id = svc.get_team(&team_id).unwrap().members[0].id.clone();
        let result = svc.change_role(&team_id, &owner_id, TeamRole::Member);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Owner"));
    }

    // -----------------------------------------------------------------------
    // 12. log_audit and get_audit_log
    // -----------------------------------------------------------------------

    #[test]
    fn test_log_audit_and_get_log() {
        let mut svc = EnterpriseService::new();

        svc.log_audit(
            "user-1",
            "Alice",
            AuditAction::Login,
            "session",
            "sess-001",
            None,
            Some("192.168.1.1".into()),
        );
        svc.log_audit(
            "user-1",
            "Alice",
            AuditAction::CreateTeam,
            "team",
            "team-001",
            Some("Created Acme team".into()),
            Some("192.168.1.1".into()),
        );
        svc.log_audit(
            "user-2",
            "Bob",
            AuditAction::Login,
            "session",
            "sess-002",
            None,
            None,
        );

        assert_eq!(svc.audit_count(), 3);

        let log = svc.get_audit_log(10);
        assert_eq!(log.len(), 3);
        // Most recent first
        assert_eq!(log[0].user_name, "Bob");
        assert_eq!(log[1].action, AuditAction::CreateTeam);
        assert_eq!(log[2].action, AuditAction::Login);
    }

    // -----------------------------------------------------------------------
    // 13. get_audit_log with limit
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_audit_log_limit() {
        let mut svc = EnterpriseService::new();

        for i in 0..5 {
            svc.log_audit(
                "user-1",
                "Alice",
                AuditAction::ConfigChange,
                "config",
                format!("cfg-{i}"),
                None,
                None,
            );
        }

        let log = svc.get_audit_log(3);
        assert_eq!(log.len(), 3);
        // Most recent entries
        assert_eq!(log[0].resource_id, "cfg-4");
        assert_eq!(log[1].resource_id, "cfg-3");
        assert_eq!(log[2].resource_id, "cfg-2");
    }

    // -----------------------------------------------------------------------
    // 14. get_audit_by_user
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_audit_by_user() {
        let mut svc = EnterpriseService::new();

        svc.log_audit(
            "user-1",
            "Alice",
            AuditAction::Login,
            "session",
            "s1",
            None,
            None,
        );
        svc.log_audit(
            "user-2",
            "Bob",
            AuditAction::Login,
            "session",
            "s2",
            None,
            None,
        );
        svc.log_audit(
            "user-1",
            "Alice",
            AuditAction::Logout,
            "session",
            "s1",
            None,
            None,
        );
        svc.log_audit(
            "user-2",
            "Bob",
            AuditAction::DataExport,
            "report",
            "r1",
            None,
            None,
        );
        svc.log_audit(
            "user-1",
            "Alice",
            AuditAction::ApiKeyAccess,
            "key",
            "k1",
            None,
            None,
        );

        let alice_log = svc.get_audit_by_user("user-1", 10);
        assert_eq!(alice_log.len(), 3);
        // Most recent first
        assert_eq!(alice_log[0].action, AuditAction::ApiKeyAccess);
        assert_eq!(alice_log[1].action, AuditAction::Logout);
        assert_eq!(alice_log[2].action, AuditAction::Login);

        let bob_log = svc.get_audit_by_user("user-2", 1);
        assert_eq!(bob_log.len(), 1);
        assert_eq!(bob_log[0].action, AuditAction::DataExport);
    }

    // -----------------------------------------------------------------------
    // 15. record_usage and get_usage_summary
    // -----------------------------------------------------------------------

    #[test]
    fn test_record_usage_and_summary() {
        let mut svc = EnterpriseService::new();

        svc.record_usage("user-1", "chat", 1000, 0.03, "gpt-4");
        svc.record_usage("user-1", "completion", 500, 0.01, "gpt-4");
        svc.record_usage("user-2", "chat", 2000, 0.06, "claude");

        let (tokens, cost) = svc.get_usage_summary("user-1");
        assert_eq!(tokens, 1500);
        assert!((cost - 0.04).abs() < f64::EPSILON);

        let (tokens2, cost2) = svc.get_usage_summary("user-2");
        assert_eq!(tokens2, 2000);
        assert!((cost2 - 0.06).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // 16. get_usage_summary for unknown user
    // -----------------------------------------------------------------------

    #[test]
    fn test_usage_summary_unknown_user() {
        let svc = EnterpriseService::new();
        let (tokens, cost) = svc.get_usage_summary("nobody");
        assert_eq!(tokens, 0);
        assert!((cost - 0.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // 17. serde roundtrip for Team
    // -----------------------------------------------------------------------

    #[test]
    fn test_team_serde_roundtrip() {
        let (mut svc, team_id) = make_service_with_team();
        svc.add_member(&team_id, "Bob", "bob@acme.com", TeamRole::Admin)
            .unwrap();

        let team = svc.get_team(&team_id).unwrap();
        let json = serde_json::to_string_pretty(team).unwrap();
        let parsed: Team = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, team.id);
        assert_eq!(parsed.name, "Acme Corp");
        assert_eq!(parsed.members.len(), 2);
        assert_eq!(parsed.members[0].role, TeamRole::Owner);
        assert_eq!(parsed.members[1].role, TeamRole::Admin);
    }

    // -----------------------------------------------------------------------
    // 18. serde roundtrip for AuditEntry
    // -----------------------------------------------------------------------

    #[test]
    fn test_audit_entry_serde_roundtrip() {
        let mut svc = EnterpriseService::new();
        let entry = svc.log_audit(
            "user-1",
            "Alice",
            AuditAction::SecurityEvent,
            "system",
            "sys-001",
            Some("Suspicious login attempt".into()),
            Some("10.0.0.1".into()),
        );

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: AuditEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, entry.id);
        assert_eq!(parsed.action, AuditAction::SecurityEvent);
        assert_eq!(parsed.details.as_deref(), Some("Suspicious login attempt"));
        assert_eq!(parsed.ip_address.as_deref(), Some("10.0.0.1"));
    }

    // -----------------------------------------------------------------------
    // 19. serde roundtrip for UsageMetric
    // -----------------------------------------------------------------------

    #[test]
    fn test_usage_metric_serde_roundtrip() {
        let mut svc = EnterpriseService::new();
        let metric = svc.record_usage("user-1", "embed", 750, 0.005, "text-embedding-3");

        let json = serde_json::to_string(&metric).unwrap();
        let parsed: UsageMetric = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, metric.id);
        assert_eq!(parsed.tokens_used, 750);
        assert!((parsed.cost_usd - 0.005).abs() < f64::EPSILON);
        assert_eq!(parsed.model, "text-embedding-3");
    }

    // -----------------------------------------------------------------------
    // 20. default trait
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_service() {
        let svc = EnterpriseService::default();
        assert!(svc.list_teams().is_empty());
        assert_eq!(svc.audit_count(), 0);
        assert_eq!(svc.team_count(), 0);
    }

    // -----------------------------------------------------------------------
    // 21. all AuditAction variants
    // -----------------------------------------------------------------------

    #[test]
    fn test_all_audit_action_variants() {
        let actions = [
            AuditAction::Login,
            AuditAction::Logout,
            AuditAction::CreateTeam,
            AuditAction::UpdateTeam,
            AuditAction::DeleteTeam,
            AuditAction::AddMember,
            AuditAction::RemoveMember,
            AuditAction::ChangeRole,
            AuditAction::ConfigChange,
            AuditAction::ApiKeyAccess,
            AuditAction::DataExport,
            AuditAction::SecurityEvent,
        ];

        let mut svc = EnterpriseService::new();
        for (i, action) in actions.iter().enumerate() {
            let entry = svc.log_audit(
                format!("user-{i}"),
                format!("User {i}"),
                *action,
                "test",
                format!("res-{i}"),
                None,
                None,
            );
            assert_eq!(entry.action, *action);
        }
        assert_eq!(svc.audit_count(), 12);
    }

    // -----------------------------------------------------------------------
    // 22. all TeamRole variants
    // -----------------------------------------------------------------------

    #[test]
    fn test_all_team_role_variants() {
        let roles = [
            TeamRole::Owner,
            TeamRole::Admin,
            TeamRole::Member,
            TeamRole::Viewer,
        ];
        for role in roles {
            let json = serde_json::to_string(&role).unwrap();
            let parsed: TeamRole = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, role);
        }
    }

    // -----------------------------------------------------------------------
    // 23. audit entry with no optional fields
    // -----------------------------------------------------------------------

    #[test]
    fn test_audit_entry_no_optional_fields() {
        let mut svc = EnterpriseService::new();
        let entry = svc.log_audit(
            "user-1",
            "Alice",
            AuditAction::Login,
            "session",
            "s1",
            None,
            None,
        );
        assert!(entry.details.is_none());
        assert!(entry.ip_address.is_none());
    }
}
