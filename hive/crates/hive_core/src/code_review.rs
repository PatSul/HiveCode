use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewStatus {
    Draft,
    Pending,
    Approved,
    ChangesRequested,
    Merged,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommentStatus {
    Pending,
    Resolved,
    WontFix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    pub id: String,
    pub file_path: String,
    pub line_number: Option<u32>,
    pub author: String,
    pub content: String,
    pub status: CommentStatus,
    pub created_at: DateTime<Utc>,
    pub replies: Vec<ReviewComment>,
}

impl ReviewComment {
    pub fn new(
        file_path: impl Into<String>,
        line_number: Option<u32>,
        author: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            file_path: file_path.into(),
            line_number,
            author: author.into(),
            content: content.into(),
            status: CommentStatus::Pending,
            created_at: Utc::now(),
            replies: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub status: ChangeType,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeReview {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub branch: String,
    pub base_branch: String,
    pub status: ReviewStatus,
    pub author: String,
    pub reviewers: Vec<String>,
    pub files: Vec<FileChange>,
    pub comments: Vec<ReviewComment>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Aggregated statistics for a single code review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewStats {
    pub total_comments: usize,
    pub resolved_comments: usize,
    pub pending_comments: usize,
    pub files_changed: usize,
    pub additions: u32,
    pub deletions: u32,
}

// ---------------------------------------------------------------------------
// CodeReviewStore — in-memory store
// ---------------------------------------------------------------------------

/// In-memory code review store following the same pattern as `NotificationStore`.
pub struct CodeReviewStore {
    reviews: Vec<CodeReview>,
}

impl CodeReviewStore {
    pub fn new() -> Self {
        Self {
            reviews: Vec::new(),
        }
    }

    /// Creates a new code review in `Draft` status and returns a clone of it.
    pub fn create_review(
        &mut self,
        title: impl Into<String>,
        branch: impl Into<String>,
        base_branch: impl Into<String>,
        author: impl Into<String>,
    ) -> CodeReview {
        let now = Utc::now();
        let review = CodeReview {
            id: Uuid::new_v4().to_string(),
            title: title.into(),
            description: None,
            branch: branch.into(),
            base_branch: base_branch.into(),
            status: ReviewStatus::Draft,
            author: author.into(),
            reviewers: Vec::new(),
            files: Vec::new(),
            comments: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        let clone = review.clone();
        self.reviews.push(review);
        clone
    }

    /// Returns a reference to a review by ID, or `None` if not found.
    pub fn get_review(&self, id: &str) -> Option<&CodeReview> {
        self.reviews.iter().find(|r| r.id == id)
    }

    /// Returns a mutable reference to a review by ID, or `None` if not found.
    fn get_review_mut(&mut self, id: &str) -> Option<&mut CodeReview> {
        self.reviews.iter_mut().find(|r| r.id == id)
    }

    /// Updates the status of a review. Returns an error if the review is not found.
    pub fn update_status(&mut self, id: &str, status: ReviewStatus) -> Result<()> {
        let review = self
            .get_review_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Review not found: {id}"))?;
        review.status = status;
        review.updated_at = Utc::now();
        Ok(())
    }

    /// Adds a comment to a review. Returns an error if the review is not found.
    pub fn add_comment(&mut self, review_id: &str, comment: ReviewComment) -> Result<()> {
        let review = self
            .get_review_mut(review_id)
            .ok_or_else(|| anyhow::anyhow!("Review not found: {review_id}"))?;
        review.comments.push(comment);
        review.updated_at = Utc::now();
        Ok(())
    }

    /// Resolves a comment (sets its status to `Resolved`). Searches top-level
    /// comments and their replies recursively. Returns an error if the review
    /// or comment is not found.
    pub fn resolve_comment(&mut self, review_id: &str, comment_id: &str) -> Result<()> {
        let review = self
            .get_review_mut(review_id)
            .ok_or_else(|| anyhow::anyhow!("Review not found: {review_id}"))?;

        if resolve_comment_recursive(&mut review.comments, comment_id) {
            review.updated_at = Utc::now();
            Ok(())
        } else {
            bail!("Comment not found: {comment_id}")
        }
    }

    /// Adds a reviewer to a review. Duplicates are silently ignored.
    /// Returns an error if the review is not found.
    pub fn add_reviewer(&mut self, review_id: &str, reviewer: impl Into<String>) -> Result<()> {
        let review = self
            .get_review_mut(review_id)
            .ok_or_else(|| anyhow::anyhow!("Review not found: {review_id}"))?;
        let reviewer = reviewer.into();
        if !review.reviewers.contains(&reviewer) {
            review.reviewers.push(reviewer);
            review.updated_at = Utc::now();
        }
        Ok(())
    }

    /// Returns a slice of all reviews.
    pub fn list_reviews(&self) -> &[CodeReview] {
        &self.reviews
    }

    /// Adds a file change to a review. Returns an error if the review is not found.
    pub fn add_file_change(&mut self, review_id: &str, file_change: FileChange) -> Result<()> {
        let review = self
            .get_review_mut(review_id)
            .ok_or_else(|| anyhow::anyhow!("Review not found: {review_id}"))?;
        review.files.push(file_change);
        review.updated_at = Utc::now();
        Ok(())
    }

    /// Computes aggregated statistics for a review. Returns an error if the
    /// review is not found.
    pub fn get_review_stats(&self, id: &str) -> Result<ReviewStats> {
        let review = self
            .get_review(id)
            .ok_or_else(|| anyhow::anyhow!("Review not found: {id}"))?;

        let (total, resolved, pending) = count_comments_recursive(&review.comments);

        let additions: u32 = review.files.iter().map(|f| f.additions).sum();
        let deletions: u32 = review.files.iter().map(|f| f.deletions).sum();

        Ok(ReviewStats {
            total_comments: total,
            resolved_comments: resolved,
            pending_comments: pending,
            files_changed: review.files.len(),
            additions,
            deletions,
        })
    }
}

impl Default for CodeReviewStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Recursively searches comments (and their replies) to resolve one by ID.
fn resolve_comment_recursive(comments: &mut [ReviewComment], comment_id: &str) -> bool {
    for comment in comments.iter_mut() {
        if comment.id == comment_id {
            comment.status = CommentStatus::Resolved;
            return true;
        }
        if resolve_comment_recursive(&mut comment.replies, comment_id) {
            return true;
        }
    }
    false
}

/// Recursively counts total, resolved, and pending comments.
fn count_comments_recursive(comments: &[ReviewComment]) -> (usize, usize, usize) {
    let mut total = 0;
    let mut resolved = 0;
    let mut pending = 0;

    for comment in comments {
        total += 1;
        match comment.status {
            CommentStatus::Resolved => resolved += 1,
            CommentStatus::Pending => pending += 1,
            CommentStatus::WontFix => {} // counted in total only
        }
        let (t, r, p) = count_comments_recursive(&comment.replies);
        total += t;
        resolved += r;
        pending += p;
    }

    (total, resolved, pending)
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

    fn make_store_with_review() -> (CodeReviewStore, String) {
        let mut store = CodeReviewStore::new();
        let review = store.create_review(
            "Add login feature",
            "feature/login",
            "main",
            "alice",
        );
        (store, review.id)
    }

    // -----------------------------------------------------------------------
    // 1. create_review
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_review() {
        let mut store = CodeReviewStore::new();
        let review = store.create_review("My PR", "feat/x", "main", "bob");

        assert_eq!(review.title, "My PR");
        assert_eq!(review.branch, "feat/x");
        assert_eq!(review.base_branch, "main");
        assert_eq!(review.author, "bob");
        assert_eq!(review.status, ReviewStatus::Draft);
        assert!(review.reviewers.is_empty());
        assert!(review.files.is_empty());
        assert!(review.comments.is_empty());
        assert!(!review.id.is_empty());
        assert_eq!(store.list_reviews().len(), 1);
    }

    // -----------------------------------------------------------------------
    // 2. get_review
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_review_found_and_not_found() {
        let (store, id) = make_store_with_review();

        assert!(store.get_review(&id).is_some());
        assert_eq!(store.get_review(&id).unwrap().title, "Add login feature");
        assert!(store.get_review("nonexistent").is_none());
    }

    // -----------------------------------------------------------------------
    // 3. update_status
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_status() {
        let (mut store, id) = make_store_with_review();

        store.update_status(&id, ReviewStatus::Pending).unwrap();
        assert_eq!(store.get_review(&id).unwrap().status, ReviewStatus::Pending);

        store.update_status(&id, ReviewStatus::Approved).unwrap();
        assert_eq!(store.get_review(&id).unwrap().status, ReviewStatus::Approved);
    }

    // -----------------------------------------------------------------------
    // 4. status transitions (full lifecycle)
    // -----------------------------------------------------------------------

    #[test]
    fn test_status_lifecycle() {
        let (mut store, id) = make_store_with_review();

        // Draft -> Pending -> ChangesRequested -> Pending -> Approved -> Merged
        let transitions = [
            ReviewStatus::Pending,
            ReviewStatus::ChangesRequested,
            ReviewStatus::Pending,
            ReviewStatus::Approved,
            ReviewStatus::Merged,
        ];

        for status in transitions {
            store.update_status(&id, status).unwrap();
            assert_eq!(store.get_review(&id).unwrap().status, status);
        }
    }

    // -----------------------------------------------------------------------
    // 5. update_status with nonexistent ID
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_status_not_found() {
        let mut store = CodeReviewStore::new();
        let result = store.update_status("bad-id", ReviewStatus::Pending);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // 6. add_comment
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_comment() {
        let (mut store, id) = make_store_with_review();

        let comment = ReviewComment::new("src/main.rs", Some(42), "bob", "Needs refactor");
        store.add_comment(&id, comment).unwrap();

        let review = store.get_review(&id).unwrap();
        assert_eq!(review.comments.len(), 1);
        assert_eq!(review.comments[0].file_path, "src/main.rs");
        assert_eq!(review.comments[0].line_number, Some(42));
        assert_eq!(review.comments[0].author, "bob");
        assert_eq!(review.comments[0].content, "Needs refactor");
        assert_eq!(review.comments[0].status, CommentStatus::Pending);
    }

    // -----------------------------------------------------------------------
    // 7. comment threading (replies)
    // -----------------------------------------------------------------------

    #[test]
    fn test_comment_threading() {
        let (mut store, id) = make_store_with_review();

        let mut parent = ReviewComment::new("lib.rs", Some(10), "alice", "Why this approach?");
        let reply = ReviewComment::new("lib.rs", Some(10), "bob", "Performance reasons");
        parent.replies.push(reply);

        store.add_comment(&id, parent).unwrap();

        let review = store.get_review(&id).unwrap();
        assert_eq!(review.comments.len(), 1);
        assert_eq!(review.comments[0].replies.len(), 1);
        assert_eq!(review.comments[0].replies[0].content, "Performance reasons");
    }

    // -----------------------------------------------------------------------
    // 8. resolve_comment (top-level)
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_comment() {
        let (mut store, id) = make_store_with_review();

        let comment = ReviewComment::new("main.rs", None, "carol", "Fix this");
        let comment_id = comment.id.clone();
        store.add_comment(&id, comment).unwrap();

        assert_eq!(
            store.get_review(&id).unwrap().comments[0].status,
            CommentStatus::Pending
        );

        store.resolve_comment(&id, &comment_id).unwrap();

        assert_eq!(
            store.get_review(&id).unwrap().comments[0].status,
            CommentStatus::Resolved
        );
    }

    // -----------------------------------------------------------------------
    // 9. resolve_comment in nested reply
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_nested_comment() {
        let (mut store, id) = make_store_with_review();

        let mut parent = ReviewComment::new("lib.rs", Some(5), "alice", "Parent");
        let nested_reply = ReviewComment::new("lib.rs", Some(5), "bob", "Nested reply");
        let nested_id = nested_reply.id.clone();
        parent.replies.push(nested_reply);

        store.add_comment(&id, parent).unwrap();
        store.resolve_comment(&id, &nested_id).unwrap();

        let review = store.get_review(&id).unwrap();
        assert_eq!(review.comments[0].status, CommentStatus::Pending);
        assert_eq!(
            review.comments[0].replies[0].status,
            CommentStatus::Resolved
        );
    }

    // -----------------------------------------------------------------------
    // 10. resolve_comment not found
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_comment_not_found() {
        let (mut store, id) = make_store_with_review();
        let result = store.resolve_comment(&id, "nonexistent-comment");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // 11. add_reviewer (including dedup)
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_reviewer_with_dedup() {
        let (mut store, id) = make_store_with_review();

        store.add_reviewer(&id, "bob").unwrap();
        store.add_reviewer(&id, "carol").unwrap();
        store.add_reviewer(&id, "bob").unwrap(); // duplicate

        let review = store.get_review(&id).unwrap();
        assert_eq!(review.reviewers, vec!["bob", "carol"]);
    }

    // -----------------------------------------------------------------------
    // 12. add_file_change
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_file_change() {
        let (mut store, id) = make_store_with_review();

        store
            .add_file_change(
                &id,
                FileChange {
                    path: "src/main.rs".into(),
                    status: ChangeType::Modified,
                    additions: 10,
                    deletions: 3,
                },
            )
            .unwrap();

        store
            .add_file_change(
                &id,
                FileChange {
                    path: "src/new.rs".into(),
                    status: ChangeType::Added,
                    additions: 50,
                    deletions: 0,
                },
            )
            .unwrap();

        let review = store.get_review(&id).unwrap();
        assert_eq!(review.files.len(), 2);
        assert_eq!(review.files[0].path, "src/main.rs");
        assert_eq!(review.files[0].status, ChangeType::Modified);
        assert_eq!(review.files[1].path, "src/new.rs");
        assert_eq!(review.files[1].status, ChangeType::Added);
    }

    // -----------------------------------------------------------------------
    // 13. get_review_stats
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_review_stats() {
        let (mut store, id) = make_store_with_review();

        // Add file changes
        store
            .add_file_change(
                &id,
                FileChange {
                    path: "a.rs".into(),
                    status: ChangeType::Modified,
                    additions: 20,
                    deletions: 5,
                },
            )
            .unwrap();
        store
            .add_file_change(
                &id,
                FileChange {
                    path: "b.rs".into(),
                    status: ChangeType::Added,
                    additions: 100,
                    deletions: 0,
                },
            )
            .unwrap();
        store
            .add_file_change(
                &id,
                FileChange {
                    path: "c.rs".into(),
                    status: ChangeType::Deleted,
                    additions: 0,
                    deletions: 30,
                },
            )
            .unwrap();

        // Add comments (including a threaded reply)
        let comment1 = ReviewComment::new("a.rs", Some(1), "bob", "Comment 1");
        let c1_id = comment1.id.clone();
        store.add_comment(&id, comment1).unwrap();

        let mut comment2 = ReviewComment::new("b.rs", Some(10), "carol", "Comment 2");
        let reply = ReviewComment::new("b.rs", Some(10), "alice", "Reply to comment 2");
        comment2.replies.push(reply);
        store.add_comment(&id, comment2).unwrap();

        // Resolve the first comment
        store.resolve_comment(&id, &c1_id).unwrap();

        let stats = store.get_review_stats(&id).unwrap();

        assert_eq!(stats.total_comments, 3); // 2 top-level + 1 reply
        assert_eq!(stats.resolved_comments, 1);
        assert_eq!(stats.pending_comments, 2);
        assert_eq!(stats.files_changed, 3);
        assert_eq!(stats.additions, 120);
        assert_eq!(stats.deletions, 35);
    }

    // -----------------------------------------------------------------------
    // 14. get_review_stats not found
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_review_stats_not_found() {
        let store = CodeReviewStore::new();
        assert!(store.get_review_stats("nonexistent").is_err());
    }

    // -----------------------------------------------------------------------
    // 15. list_reviews
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_reviews() {
        let mut store = CodeReviewStore::new();
        assert!(store.list_reviews().is_empty());

        store.create_review("PR 1", "feat/a", "main", "alice");
        store.create_review("PR 2", "feat/b", "main", "bob");
        store.create_review("PR 3", "fix/c", "develop", "carol");

        assert_eq!(store.list_reviews().len(), 3);
        assert_eq!(store.list_reviews()[0].title, "PR 1");
        assert_eq!(store.list_reviews()[2].title, "PR 3");
    }

    // -----------------------------------------------------------------------
    // 16. serde roundtrip for CodeReview
    // -----------------------------------------------------------------------

    #[test]
    fn test_serde_roundtrip() {
        let mut store = CodeReviewStore::new();
        let review = store.create_review("Serde test", "branch", "main", "alice");
        let id = review.id.clone();

        store.add_reviewer(&id, "bob").unwrap();
        store
            .add_file_change(
                &id,
                FileChange {
                    path: "test.rs".into(),
                    status: ChangeType::Modified,
                    additions: 5,
                    deletions: 2,
                },
            )
            .unwrap();
        let comment = ReviewComment::new("test.rs", Some(1), "bob", "Looks good");
        store.add_comment(&id, comment).unwrap();

        let review = store.get_review(&id).unwrap();
        let json = serde_json::to_string_pretty(review).unwrap();
        let parsed: CodeReview = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, review.id);
        assert_eq!(parsed.title, "Serde test");
        assert_eq!(parsed.status, ReviewStatus::Draft);
        assert_eq!(parsed.reviewers, vec!["bob"]);
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.comments.len(), 1);
        assert_eq!(parsed.comments[0].content, "Looks good");
    }

    // -----------------------------------------------------------------------
    // 17. updated_at changes on mutations
    // -----------------------------------------------------------------------

    #[test]
    fn test_updated_at_changes() {
        let (mut store, id) = make_store_with_review();

        let initial_updated = store.get_review(&id).unwrap().updated_at;

        // Sleep briefly to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        store.update_status(&id, ReviewStatus::Pending).unwrap();
        let after_status = store.get_review(&id).unwrap().updated_at;
        assert!(after_status >= initial_updated);
    }

    // -----------------------------------------------------------------------
    // 18. default trait
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_store() {
        let store = CodeReviewStore::default();
        assert!(store.list_reviews().is_empty());
    }
}
