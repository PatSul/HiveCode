//! Live Specification System — self-maintaining specs inspired by Augment Intent.
//!
//! Provides a `SpecManager` that tracks specifications with sections, entries,
//! versioning, and auto-update support. Agents can update specs as they work,
//! keeping the living document in sync with progress.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Spec Status
// ---------------------------------------------------------------------------

/// Lifecycle status of a specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecStatus {
    Draft,
    Active,
    Completed,
    Archived,
}

// ---------------------------------------------------------------------------
// Spec Section
// ---------------------------------------------------------------------------

/// Logical section within a specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecSection {
    Requirements,
    Plan,
    Progress,
    Notes,
}

impl SpecSection {
    pub const ALL: [SpecSection; 4] = [
        Self::Requirements,
        Self::Plan,
        Self::Progress,
        Self::Notes,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Requirements => "Requirements",
            Self::Plan => "Plan",
            Self::Progress => "Progress",
            Self::Notes => "Notes",
        }
    }
}

// ---------------------------------------------------------------------------
// Spec Entry
// ---------------------------------------------------------------------------

/// A single entry within a spec section (e.g. a requirement or a plan step).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecEntry {
    pub id: String,
    pub title: String,
    pub content: String,
    pub status: SpecStatus,
    pub checked: bool,
}

impl SpecEntry {
    pub fn new(id: impl Into<String>, title: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            content: content.into(),
            status: SpecStatus::Draft,
            checked: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Spec
// ---------------------------------------------------------------------------

/// A complete specification document with sections, versioning, and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: SpecStatus,
    pub sections: HashMap<SpecSection, Vec<SpecEntry>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub version: u32,
    pub auto_update: bool,
}

impl Spec {
    /// Create a new spec with empty sections and version 1.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let now = chrono::Utc::now();
        let mut sections = HashMap::new();
        for section in SpecSection::ALL {
            sections.insert(section, Vec::new());
        }
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            status: SpecStatus::Draft,
            sections,
            created_at: now,
            updated_at: now,
            version: 1,
            auto_update: true,
        }
    }

    /// Total number of entries across all sections.
    pub fn entry_count(&self) -> usize {
        self.sections.values().map(|v| v.len()).sum()
    }

    /// Number of checked entries across all sections.
    pub fn checked_count(&self) -> usize {
        self.sections
            .values()
            .flat_map(|v| v.iter())
            .filter(|e| e.checked)
            .count()
    }

    /// Completion percentage (0.0..=1.0). Returns 0.0 if no entries exist.
    pub fn completion_pct(&self) -> f32 {
        let total = self.entry_count();
        if total == 0 {
            return 0.0;
        }
        self.checked_count() as f32 / total as f32
    }

    /// Bump version and update the `updated_at` timestamp.
    fn bump_version(&mut self) {
        self.version += 1;
        self.updated_at = chrono::Utc::now();
    }
}

// ---------------------------------------------------------------------------
// Spec Manager
// ---------------------------------------------------------------------------

/// Manages a collection of specifications with CRUD, versioning, and
/// agent-driven auto-update support.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpecManager {
    pub specs: HashMap<String, Spec>,
}

impl SpecManager {
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
        }
    }

    /// Create a new spec and store it. Returns the spec id.
    pub fn create_spec(
        &mut self,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let spec = Spec::new(id.clone(), title, description);
        self.specs.insert(id.clone(), spec);
        id
    }

    /// Get an immutable reference to a spec by id.
    pub fn get_spec(&self, id: &str) -> Option<&Spec> {
        self.specs.get(id)
    }

    /// Get a mutable reference to a spec by id.
    pub fn get_spec_mut(&mut self, id: &str) -> Option<&mut Spec> {
        self.specs.get_mut(id)
    }

    /// Update a spec's title and description. Bumps the version.
    pub fn update_spec(
        &mut self,
        id: &str,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<(), String> {
        let spec = self.specs.get_mut(id).ok_or_else(|| format!("Spec not found: {id}"))?;
        spec.title = title.into();
        spec.description = description.into();
        spec.bump_version();
        Ok(())
    }

    /// Add an entry to a specific section of a spec.
    pub fn add_entry(
        &mut self,
        spec_id: &str,
        section: SpecSection,
        entry: SpecEntry,
    ) -> Result<(), String> {
        let spec = self
            .specs
            .get_mut(spec_id)
            .ok_or_else(|| format!("Spec not found: {spec_id}"))?;
        spec.sections
            .entry(section)
            .or_default()
            .push(entry);
        spec.bump_version();
        Ok(())
    }

    /// Mark an entry as checked (completed). Returns an error if the entry
    /// or spec is not found.
    pub fn check_entry(
        &mut self,
        spec_id: &str,
        entry_id: &str,
        checked: bool,
    ) -> Result<(), String> {
        let spec = self
            .specs
            .get_mut(spec_id)
            .ok_or_else(|| format!("Spec not found: {spec_id}"))?;

        for entries in spec.sections.values_mut() {
            if let Some(entry) = entries.iter_mut().find(|e| e.id == entry_id) {
                entry.checked = checked;
                spec.bump_version();
                return Ok(());
            }
        }

        Err(format!("Entry not found: {entry_id}"))
    }

    /// Apply an agent's output to auto-update matching spec sections.
    ///
    /// Scans the agent output for section keywords and appends progress
    /// notes. Returns the list of sections that were updated.
    pub fn apply_agent_update(
        &mut self,
        spec_id: &str,
        agent_output: &str,
    ) -> Result<Vec<SpecSection>, String> {
        let spec = self
            .specs
            .get_mut(spec_id)
            .ok_or_else(|| format!("Spec not found: {spec_id}"))?;

        if !spec.auto_update {
            return Ok(Vec::new());
        }

        let lower = agent_output.to_lowercase();
        let mut updated_sections = Vec::new();

        // Match keywords to sections and add progress entries.
        let section_keywords: &[(SpecSection, &[&str])] = &[
            (SpecSection::Requirements, &["requirement", "must", "shall", "need"]),
            (SpecSection::Plan, &["plan", "step", "phase", "approach"]),
            (SpecSection::Progress, &["completed", "done", "implemented", "fixed", "progress"]),
            (SpecSection::Notes, &["note", "caveat", "warning", "observation"]),
        ];

        for (section, keywords) in section_keywords {
            if keywords.iter().any(|kw| lower.contains(kw)) {
                let entry = SpecEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: format!("Agent update — {}", section.label()),
                    content: truncate_output(agent_output, 500),
                    status: SpecStatus::Active,
                    checked: false,
                };
                spec.sections.entry(*section).or_default().push(entry);
                updated_sections.push(*section);
            }
        }

        if !updated_sections.is_empty() {
            spec.bump_version();
        }

        Ok(updated_sections)
    }

    /// Return all specs with `Active` status.
    pub fn get_active_specs(&self) -> Vec<&Spec> {
        self.specs
            .values()
            .filter(|s| s.status == SpecStatus::Active)
            .collect()
    }

    /// Archive a spec by setting its status to `Archived`.
    pub fn archive_spec(&mut self, id: &str) -> Result<(), String> {
        let spec = self.specs.get_mut(id).ok_or_else(|| format!("Spec not found: {id}"))?;
        spec.status = SpecStatus::Archived;
        spec.bump_version();
        Ok(())
    }

    /// Export a spec as a Markdown document.
    pub fn export_markdown(&self, id: &str) -> Result<String, String> {
        let spec = self.specs.get(id).ok_or_else(|| format!("Spec not found: {id}"))?;

        let mut md = String::new();
        md.push_str(&format!("# {}\n\n", spec.title));
        md.push_str(&format!("**Status:** {:?} | **Version:** {} | **Auto-update:** {}\n\n", spec.status, spec.version, spec.auto_update));
        md.push_str(&format!("{}\n\n", spec.description));

        for section in SpecSection::ALL {
            let entries = spec.sections.get(&section).cloned().unwrap_or_default();
            md.push_str(&format!("## {}\n\n", section.label()));

            if entries.is_empty() {
                md.push_str("_No entries._\n\n");
            } else {
                for entry in &entries {
                    let check = if entry.checked { "x" } else { " " };
                    md.push_str(&format!("- [{}] **{}** — {}\n", check, entry.title, entry.content));
                }
                md.push('\n');
            }
        }

        Ok(md)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Truncate a string to at most `max_len` characters, appending "..." if truncated.
fn truncate_output(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager_with_spec() -> (SpecManager, String) {
        let mut mgr = SpecManager::new();
        let id = mgr.create_spec("Test Spec", "A test specification");
        (mgr, id)
    }

    #[test]
    fn create_spec_returns_unique_id() {
        let mut mgr = SpecManager::new();
        let id1 = mgr.create_spec("Spec A", "First");
        let id2 = mgr.create_spec("Spec B", "Second");
        assert_ne!(id1, id2);
        assert_eq!(mgr.specs.len(), 2);
    }

    #[test]
    fn get_spec_returns_correct_data() {
        let (mgr, id) = make_manager_with_spec();
        let spec = mgr.get_spec(&id).unwrap();
        assert_eq!(spec.title, "Test Spec");
        assert_eq!(spec.description, "A test specification");
        assert_eq!(spec.status, SpecStatus::Draft);
        assert_eq!(spec.version, 1);
        assert!(spec.auto_update);
    }

    #[test]
    fn get_spec_not_found_returns_none() {
        let mgr = SpecManager::new();
        assert!(mgr.get_spec("nonexistent").is_none());
    }

    #[test]
    fn update_spec_changes_title_and_bumps_version() {
        let (mut mgr, id) = make_manager_with_spec();
        mgr.update_spec(&id, "Updated Title", "Updated Desc").unwrap();
        let spec = mgr.get_spec(&id).unwrap();
        assert_eq!(spec.title, "Updated Title");
        assert_eq!(spec.description, "Updated Desc");
        assert_eq!(spec.version, 2);
    }

    #[test]
    fn update_spec_not_found_returns_error() {
        let mut mgr = SpecManager::new();
        let result = mgr.update_spec("bad-id", "Title", "Desc");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn add_entry_and_check_entry() {
        let (mut mgr, id) = make_manager_with_spec();

        let entry = SpecEntry::new("e1", "Requirement 1", "Must support X");
        mgr.add_entry(&id, SpecSection::Requirements, entry).unwrap();

        let spec = mgr.get_spec(&id).unwrap();
        assert_eq!(spec.entry_count(), 1);
        assert_eq!(spec.checked_count(), 0);
        assert_eq!(spec.completion_pct(), 0.0);

        mgr.check_entry(&id, "e1", true).unwrap();
        let spec = mgr.get_spec(&id).unwrap();
        assert_eq!(spec.checked_count(), 1);
        assert!((spec.completion_pct() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn check_entry_not_found_returns_error() {
        let (mut mgr, id) = make_manager_with_spec();
        let result = mgr.check_entry(&id, "nonexistent", true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Entry not found"));
    }

    #[test]
    fn apply_agent_update_adds_progress_entries() {
        let (mut mgr, id) = make_manager_with_spec();

        // Activate auto_update (it's on by default).
        let updated = mgr
            .apply_agent_update(&id, "I have completed the implementation and fixed the bug.")
            .unwrap();

        // Should match Progress section ("completed").
        assert!(updated.contains(&SpecSection::Progress));

        let spec = mgr.get_spec(&id).unwrap();
        let progress = spec.sections.get(&SpecSection::Progress).unwrap();
        assert!(!progress.is_empty());
    }

    #[test]
    fn apply_agent_update_respects_auto_update_flag() {
        let (mut mgr, id) = make_manager_with_spec();
        mgr.get_spec_mut(&id).unwrap().auto_update = false;

        let updated = mgr
            .apply_agent_update(&id, "I have completed the implementation.")
            .unwrap();

        assert!(updated.is_empty());
    }

    #[test]
    fn apply_agent_update_matches_multiple_sections() {
        let (mut mgr, id) = make_manager_with_spec();

        let output = "The plan is to implement the requirement. Note: this is completed.";
        let updated = mgr.apply_agent_update(&id, output).unwrap();

        // Should match Requirements ("requirement"), Plan ("plan"),
        // Progress ("completed"), Notes ("note").
        assert!(updated.len() >= 3);
    }

    #[test]
    fn get_active_specs_filters_correctly() {
        let mut mgr = SpecManager::new();
        let id1 = mgr.create_spec("Active Spec", "desc");
        let _id2 = mgr.create_spec("Draft Spec", "desc");

        mgr.get_spec_mut(&id1).unwrap().status = SpecStatus::Active;

        let active = mgr.get_active_specs();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].title, "Active Spec");
    }

    #[test]
    fn archive_spec_sets_status() {
        let (mut mgr, id) = make_manager_with_spec();
        mgr.archive_spec(&id).unwrap();
        let spec = mgr.get_spec(&id).unwrap();
        assert_eq!(spec.status, SpecStatus::Archived);
    }

    #[test]
    fn archive_spec_not_found_returns_error() {
        let mut mgr = SpecManager::new();
        let result = mgr.archive_spec("missing");
        assert!(result.is_err());
    }

    #[test]
    fn export_markdown_contains_all_sections() {
        let (mut mgr, id) = make_manager_with_spec();
        let entry = SpecEntry::new("r1", "Auth", "Must support OAuth");
        mgr.add_entry(&id, SpecSection::Requirements, entry).unwrap();

        let md = mgr.export_markdown(&id).unwrap();
        assert!(md.contains("# Test Spec"));
        assert!(md.contains("## Requirements"));
        assert!(md.contains("## Plan"));
        assert!(md.contains("## Progress"));
        assert!(md.contains("## Notes"));
        assert!(md.contains("Auth"));
        assert!(md.contains("Must support OAuth"));
    }

    #[test]
    fn export_markdown_shows_checked_state() {
        let (mut mgr, id) = make_manager_with_spec();
        let entry = SpecEntry::new("r1", "Done item", "content");
        mgr.add_entry(&id, SpecSection::Requirements, entry).unwrap();
        mgr.check_entry(&id, "r1", true).unwrap();

        let md = mgr.export_markdown(&id).unwrap();
        assert!(md.contains("[x]"));
    }

    #[test]
    fn spec_entry_new_defaults() {
        let entry = SpecEntry::new("id-1", "Title", "Content");
        assert_eq!(entry.status, SpecStatus::Draft);
        assert!(!entry.checked);
        assert_eq!(entry.id, "id-1");
    }

    #[test]
    fn spec_section_labels_are_nonempty() {
        for section in SpecSection::ALL {
            assert!(!section.label().is_empty());
        }
    }

    #[test]
    fn spec_completion_pct_empty() {
        let spec = Spec::new("id", "Title", "Desc");
        assert_eq!(spec.completion_pct(), 0.0);
    }

    #[test]
    fn truncate_output_short_string_unchanged() {
        assert_eq!(truncate_output("hello", 10), "hello");
    }

    #[test]
    fn truncate_output_long_string_truncated() {
        let result = truncate_output("abcdefghij", 5);
        assert_eq!(result, "abcde...");
    }

    #[test]
    fn serialization_roundtrip() {
        let (mut mgr, id) = make_manager_with_spec();
        let entry = SpecEntry::new("e1", "T", "C");
        mgr.add_entry(&id, SpecSection::Plan, entry).unwrap();

        let json = serde_json::to_string(&mgr).unwrap();
        let deserialized: SpecManager = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.specs.len(), 1);

        let spec = deserialized.get_spec(&id).unwrap();
        assert_eq!(spec.title, "Test Spec");
        let plan = spec.sections.get(&SpecSection::Plan).unwrap();
        assert_eq!(plan.len(), 1);
    }
}
