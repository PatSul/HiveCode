use std::path::Path;

use hive_ui::panels::review::*;
use hive_fs::git::FileStatusType;

#[test]
fn empty_data_is_not_repo() {
    let data = ReviewData::empty();
    assert!(!data.is_repo);
    assert!(data.files.is_empty());
    assert!(data.recent_commits.is_empty());
    assert!(data.diff_lines.is_empty());
    assert!(data.branch.is_empty());
    assert!(data.comments.is_empty());
    assert_eq!(data.verdict, ReviewVerdict::Pending);
}

#[test]
fn status_labels() {
    assert_eq!(ReviewFileStatus::Modified.label(), "M");
    assert_eq!(ReviewFileStatus::Added.label(), "A");
    assert_eq!(ReviewFileStatus::Deleted.label(), "D");
    assert_eq!(ReviewFileStatus::Renamed.label(), "R");
    assert_eq!(ReviewFileStatus::Untracked.label(), "?");
}

#[test]
fn from_converts_file_status_type() {
    assert_eq!(
        ReviewFileStatus::from(FileStatusType::Modified),
        ReviewFileStatus::Modified
    );
    assert_eq!(
        ReviewFileStatus::from(FileStatusType::Added),
        ReviewFileStatus::Added
    );
    assert_eq!(
        ReviewFileStatus::from(FileStatusType::Deleted),
        ReviewFileStatus::Deleted
    );
    assert_eq!(
        ReviewFileStatus::from(FileStatusType::Renamed),
        ReviewFileStatus::Renamed
    );
    assert_eq!(
        ReviewFileStatus::from(FileStatusType::Untracked),
        ReviewFileStatus::Untracked
    );
}

#[test]
fn parse_diff_handles_hunk_header() {
    let diff = "@@ -10,3 +10,5 @@ fn example()\n context\n-removed\n+added\n+also added\n context2";
    let lines = ReviewData::parse_diff(diff);
    assert_eq!(lines.len(), 6);
    assert_eq!(lines[0].kind, DiffLineKind::Hunk);
    assert_eq!(lines[1].kind, DiffLineKind::Context);
    assert_eq!(lines[1].line_num_old, Some(10));
    assert_eq!(lines[1].line_num_new, Some(10));
    assert_eq!(lines[2].kind, DiffLineKind::Deletion);
    assert_eq!(lines[2].line_num_old, Some(11));
    assert_eq!(lines[3].kind, DiffLineKind::Addition);
    assert_eq!(lines[3].line_num_new, Some(11));
    assert_eq!(lines[4].kind, DiffLineKind::Addition);
    assert_eq!(lines[4].line_num_new, Some(12));
    assert_eq!(lines[5].kind, DiffLineKind::Context);
    assert_eq!(lines[5].line_num_old, Some(12));
    assert_eq!(lines[5].line_num_new, Some(13));
}

#[test]
fn parse_hunk_header_extracts_line_numbers() {
    assert_eq!(
        ReviewData::parse_hunk_header("@@ -1,5 +1,7 @@ fn test()"),
        Some((1, 1))
    );
    assert_eq!(
        ReviewData::parse_hunk_header("@@ -42,10 +50,12 @@"),
        Some((42, 50))
    );
    assert_eq!(ReviewData::parse_hunk_header("not a hunk"), None);
}

#[test]
fn parse_diff_empty() {
    let lines = ReviewData::parse_diff("");
    assert!(lines.is_empty());
}

#[test]
fn parse_diff_skips_metadata() {
    let diff = "diff --git a/foo b/foo\nindex abc..def 100644\n--- a/foo\n+++ b/foo\n@@ -1,2 +1,3 @@\n context\n+added";
    let lines = ReviewData::parse_diff(diff);
    // Only the hunk header, context, and addition should appear
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].kind, DiffLineKind::Hunk);
    assert_eq!(lines[1].kind, DiffLineKind::Context);
    assert_eq!(lines[2].kind, DiffLineKind::Addition);
}

#[test]
fn format_time_ago_recent() {
    let now = chrono::Utc::now().timestamp();
    assert_eq!(ReviewData::format_time_ago(now), "just now");
    assert_eq!(ReviewData::format_time_ago(now - 120), "2 min ago");
    assert_eq!(ReviewData::format_time_ago(now - 3600), "1 hour ago");
    assert_eq!(ReviewData::format_time_ago(now - 7200), "2 hours ago");
    assert_eq!(ReviewData::format_time_ago(now - 86400), "1 day ago");
    assert_eq!(ReviewData::format_time_ago(now - 86400 * 3), "3 days ago");
    assert_eq!(ReviewData::format_time_ago(now - 86400 * 7), "1 week ago");
    assert_eq!(ReviewData::format_time_ago(now - 86400 * 14), "2 weeks ago");
}

#[test]
fn non_repo_path_returns_empty() {
    // A path that definitely is not a git repo.
    let data = ReviewData::from_git(Path::new("/tmp/nonexistent_git_repo_12345"));
    assert!(!data.is_repo);
    assert!(data.files.is_empty());
}

// -- New comment & verdict tests --

#[test]
fn add_comment_and_query() {
    let mut data = ReviewData::empty();
    data.is_repo = true;

    let comment = ReviewComment::new("c1", "src/main.rs", Some(42), "alice", "Looks good!");
    data.add_comment(comment);

    assert_eq!(data.comments.len(), 1);
    assert_eq!(data.comments[0].id, "c1");
    assert_eq!(data.comments[0].file_path, "src/main.rs");
    assert_eq!(data.comments[0].line_number, Some(42));
    assert!(!data.comments[0].resolved);
}

#[test]
fn comments_for_file_filters_correctly() {
    let mut data = ReviewData::empty();
    data.add_comment(ReviewComment::new("c1", "a.rs", Some(1), "alice", "Comment on a"));
    data.add_comment(ReviewComment::new("c2", "b.rs", Some(2), "bob", "Comment on b"));
    data.add_comment(ReviewComment::new("c3", "a.rs", None, "alice", "Another on a"));

    let a_comments = data.comments_for_file("a.rs");
    assert_eq!(a_comments.len(), 2);

    let b_comments = data.comments_for_file("b.rs");
    assert_eq!(b_comments.len(), 1);

    let c_comments = data.comments_for_file("c.rs");
    assert!(c_comments.is_empty());
}

#[test]
fn resolve_comment_works() {
    let mut data = ReviewData::empty();
    data.add_comment(ReviewComment::new("c1", "a.rs", Some(1), "alice", "Fix this"));
    data.add_comment(ReviewComment::new("c2", "a.rs", Some(5), "bob", "And this"));

    assert_eq!(data.unresolved_comments().len(), 2);

    let resolved = data.resolve_comment("c1");
    assert!(resolved);
    assert_eq!(data.unresolved_comments().len(), 1);
    assert!(data.comments[0].resolved);
    assert!(!data.comments[1].resolved);
}

#[test]
fn resolve_nonexistent_comment_returns_false() {
    let mut data = ReviewData::empty();
    assert!(!data.resolve_comment("nonexistent"));
}

#[test]
fn verdict_labels() {
    assert_eq!(ReviewVerdict::Pending.label(), "Pending");
    assert_eq!(ReviewVerdict::Approved.label(), "Approved");
    assert_eq!(ReviewVerdict::RequestChanges.label(), "Changes Requested");
    assert_eq!(ReviewVerdict::Comment.label(), "Commented");
}

#[test]
fn set_verdict_changes_state() {
    let mut data = ReviewData::empty();
    assert_eq!(data.verdict, ReviewVerdict::Pending);

    data.set_verdict(ReviewVerdict::Approved);
    assert_eq!(data.verdict, ReviewVerdict::Approved);

    data.set_verdict(ReviewVerdict::RequestChanges);
    assert_eq!(data.verdict, ReviewVerdict::RequestChanges);
}

#[test]
fn review_comment_new_sets_defaults() {
    let comment = ReviewComment::new("id1", "path.rs", None, "tester", "body text");
    assert_eq!(comment.id, "id1");
    assert_eq!(comment.file_path, "path.rs");
    assert_eq!(comment.line_number, None);
    assert_eq!(comment.author, "tester");
    assert_eq!(comment.body, "body text");
    assert!(!comment.resolved);
}

#[test]
fn review_comment_resolve() {
    let mut comment = ReviewComment::new("id1", "path.rs", None, "tester", "body");
    assert!(!comment.resolved);
    comment.resolve();
    assert!(comment.resolved);
}
