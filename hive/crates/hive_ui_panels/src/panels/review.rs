use std::path::Path;

use chrono::{DateTime, Utc};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{Icon, IconName};
use tracing::warn;

use hive_fs::git::{FileStatusType, GitService};

use hive_ui_core::HiveTheme;
use hive_ui_core::{
    ReviewAiCommitMessage, ReviewBranchCreate, ReviewBranchDeleteNamed, ReviewBranchSwitch,
    ReviewCommitWithMessage, ReviewDiscardAll, ReviewGitflowFinishNamed, ReviewGitflowInit,
    ReviewGitflowStart, ReviewLfsPull, ReviewLfsPush, ReviewLfsTrack, ReviewPrAiGenerate,
    ReviewPrCreate, ReviewPush, ReviewPushSetUpstream, ReviewStageAll, ReviewSwitchTab,
    ReviewUnstageAll,
};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Git file status classification (mirrors hive_fs::git::FileStatusType).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl ReviewFileStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Untracked => "?",
        }
    }
}

impl From<FileStatusType> for ReviewFileStatus {
    fn from(fst: FileStatusType) -> Self {
        match fst {
            FileStatusType::Modified => Self::Modified,
            FileStatusType::Added => Self::Added,
            FileStatusType::Deleted => Self::Deleted,
            FileStatusType::Renamed => Self::Renamed,
            FileStatusType::Untracked => Self::Untracked,
        }
    }
}

/// A file entry in the code review panel.
pub struct ReviewFileEntry {
    pub path: String,
    pub status: ReviewFileStatus,
    pub additions: usize,
    pub deletions: usize,
    pub is_staged: bool,
}

/// A single diff line for the inline diff viewer.
pub struct DiffLine {
    pub line_num_old: Option<usize>,
    pub line_num_new: Option<usize>,
    pub kind: DiffLineKind,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
    Hunk,
}

/// A commit summary for the recent commits section.
pub struct CommitEntry {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub time_ago: String,
}

// ---------------------------------------------------------------------------
// Comment / Annotation data model
// ---------------------------------------------------------------------------

/// The type of review action (overall verdict).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    /// No verdict yet.
    Pending,
    /// Approved -- changes look good.
    Approved,
    /// Request changes before merging.
    RequestChanges,
    /// General comment without explicit approval/rejection.
    Comment,
}

impl ReviewVerdict {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Approved => "Approved",
            Self::RequestChanges => "Changes Requested",
            Self::Comment => "Commented",
        }
    }
}

// ---------------------------------------------------------------------------
// Git Ops tab system
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitOpsTab {
    Changes,
    Push,
    PullRequests,
    Branches,
    Lfs,
    Gitflow,
}

impl GitOpsTab {
    pub const ALL: [GitOpsTab; 6] = [
        GitOpsTab::Changes,
        GitOpsTab::Push,
        GitOpsTab::PullRequests,
        GitOpsTab::Branches,
        GitOpsTab::Lfs,
        GitOpsTab::Gitflow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Changes => "Changes",
            Self::Push => "Push",
            Self::PullRequests => "Pull Requests",
            Self::Branches => "Branches",
            Self::Lfs => "LFS",
            Self::Gitflow => "Gitflow",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "changes" => Self::Changes,
            "push" => Self::Push,
            "pull_requests" => Self::PullRequests,
            "branches" => Self::Branches,
            "lfs" => Self::Lfs,
            "gitflow" => Self::Gitflow,
            _ => Self::Changes,
        }
    }

    pub fn to_str(self) -> &'static str {
        match self {
            Self::Changes => "changes",
            Self::Push => "push",
            Self::PullRequests => "pull_requests",
            Self::Branches => "branches",
            Self::Lfs => "lfs",
            Self::Gitflow => "gitflow",
        }
    }
}

// ---------------------------------------------------------------------------
// Sub-data for each Git Ops tab
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[derive(Default)]
pub struct AiCommitState {
    pub generating: bool,
    pub generated_message: Option<String>,
    pub user_edited_message: String,
}


#[derive(Debug, Clone)]
pub struct PushData {
    pub remote_name: String,
    pub remote_url: String,
    pub tracking_branch: Option<String>,
    pub ahead_count: usize,
    pub behind_count: usize,
    pub push_in_progress: bool,
    pub last_push_result: Option<Result<String, String>>,
}

impl Default for PushData {
    fn default() -> Self {
        Self {
            remote_name: "origin".to_string(),
            remote_url: String::new(),
            tracking_branch: None,
            ahead_count: 0,
            behind_count: 0,
            push_in_progress: false,
            last_push_result: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrSummary {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head: String,
    pub base: String,
    pub state: String,
    pub created_at: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct PrForm {
    pub title: String,
    pub body: String,
    pub base_branch: String,
    pub ai_generating: bool,
}

impl Default for PrForm {
    fn default() -> Self {
        Self {
            title: String::new(),
            body: String::new(),
            base_branch: "main".to_string(),
            ai_generating: false,
        }
    }
}

#[derive(Debug, Clone)]
#[derive(Default)]
pub struct PullRequestsData {
    pub open_prs: Vec<PrSummary>,
    pub pr_form: PrForm,
    pub loading: bool,
    pub github_connected: bool,
}


#[derive(Debug, Clone)]
pub struct BranchEntry {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub last_commit_msg: String,
    pub last_commit_time: String,
}

#[derive(Debug, Clone)]
#[derive(Default)]
pub struct BranchesData {
    pub branches: Vec<BranchEntry>,
    pub current_branch: String,
    pub new_branch_name: String,
}


#[derive(Debug, Clone)]
pub struct LfsFileEntry {
    pub path: String,
    pub size: String,
    pub oid: String,
    pub is_pointer: bool,
}

#[derive(Debug, Clone)]
#[derive(Default)]
pub struct LfsData {
    pub is_lfs_installed: bool,
    pub tracked_patterns: Vec<String>,
    pub lfs_files: Vec<LfsFileEntry>,
    pub new_pattern: String,
    pub lfs_pull_in_progress: bool,
    pub lfs_push_in_progress: bool,
}


#[derive(Debug, Clone)]
pub struct GitflowData {
    pub initialized: bool,
    pub develop_branch: String,
    pub main_branch: String,
    pub feature_prefix: String,
    pub release_prefix: String,
    pub hotfix_prefix: String,
    pub active_features: Vec<String>,
    pub active_releases: Vec<String>,
    pub active_hotfixes: Vec<String>,
    pub new_name: String,
}

impl Default for GitflowData {
    fn default() -> Self {
        Self {
            initialized: false,
            develop_branch: "develop".to_string(),
            main_branch: "main".to_string(),
            feature_prefix: "feature/".to_string(),
            release_prefix: "release/".to_string(),
            hotfix_prefix: "hotfix/".to_string(),
            active_features: Vec::new(),
            active_releases: Vec::new(),
            active_hotfixes: Vec::new(),
            new_name: String::new(),
        }
    }
}

/// A single inline comment attached to a file (and optionally a line number).
#[derive(Debug, Clone)]
pub struct ReviewComment {
    /// Unique identifier for this comment.
    pub id: String,
    /// File path this comment is attached to.
    pub file_path: String,
    /// Line number in the new file (if line-level comment). `None` for file-level.
    pub line_number: Option<usize>,
    /// Author of the comment.
    pub author: String,
    /// Comment body text.
    pub body: String,
    /// Timestamp of creation.
    pub created_at: DateTime<Utc>,
    /// Whether this comment has been resolved.
    pub resolved: bool,
}

impl ReviewComment {
    pub fn new(
        id: impl Into<String>,
        file_path: impl Into<String>,
        line_number: Option<usize>,
        author: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            file_path: file_path.into(),
            line_number,
            author: author.into(),
            body: body.into(),
            created_at: Utc::now(),
            resolved: false,
        }
    }

    /// Mark this comment as resolved.
    pub fn resolve(&mut self) {
        self.resolved = true;
    }
}

// ---------------------------------------------------------------------------
// ReviewData
// ---------------------------------------------------------------------------

/// All data needed to render the code review panel.
pub struct ReviewData {
    pub branch: String,
    pub last_commit_hash: String,
    pub last_commit_msg: String,
    pub modified_count: usize,
    pub staged_count: usize,
    pub untracked_count: usize,
    pub files: Vec<ReviewFileEntry>,
    pub diff_lines: Vec<DiffLine>,
    pub selected_file: Option<String>,
    pub recent_commits: Vec<CommitEntry>,
    /// True when this data represents a valid git repo.
    pub is_repo: bool,
    /// Inline review comments / annotations.
    pub comments: Vec<ReviewComment>,
    /// Current review verdict.
    pub verdict: ReviewVerdict,
    // --- Git Ops tab system ---
    pub active_tab: GitOpsTab,
    pub ai_commit: AiCommitState,
    pub push_data: PushData,
    pub pr_data: PullRequestsData,
    pub branches_data: BranchesData,
    pub lfs_data: LfsData,
    pub gitflow_data: GitflowData,
}

impl ReviewData {
    /// Load real git data from a repository path.
    ///
    /// Reads branch name, file statuses (with staged detection), diff output
    /// parsed into structured lines, and recent commit history.
    pub fn from_git(repo_path: &Path) -> Self {
        let git = match GitService::open(repo_path) {
            Ok(g) => g,
            Err(e) => {
                warn!(
                    "ReviewData: could not open git repo at {}: {e}",
                    repo_path.display()
                );
                return Self::empty();
            }
        };

        // -- Branch name --
        let branch = git.current_branch().unwrap_or_else(|_| "unknown".into());

        // -- File statuses with staged detection --
        let (files, modified_count, staged_count, untracked_count) = Self::load_file_statuses(&git);

        // -- Diff (full working-tree diff parsed into structured lines) --
        let diff_raw = git.diff().unwrap_or_default();
        let diff_lines = Self::parse_diff(&diff_raw);

        // -- Selected file: first changed file if any --
        let selected_file = files.first().map(|f| f.path.clone());

        // -- Recent commits --
        let log_entries = git.log(10).unwrap_or_default();
        let recent_commits: Vec<CommitEntry> = log_entries
            .iter()
            .map(|entry| CommitEntry {
                hash: entry.hash[..7.min(entry.hash.len())].to_string(),
                message: entry.message.clone(),
                author: entry.author.clone(),
                time_ago: Self::format_time_ago(entry.timestamp),
            })
            .collect();

        // -- Last commit --
        let (last_commit_hash, last_commit_msg) = recent_commits
            .first()
            .map(|c| (c.hash.clone(), c.message.clone()))
            .unwrap_or_else(|| ("none".into(), "No commits yet".into()));

        Self {
            branch,
            last_commit_hash,
            last_commit_msg,
            modified_count,
            staged_count,
            untracked_count,
            files,
            diff_lines,
            selected_file,
            recent_commits,
            is_repo: true,
            comments: Vec::new(),
            verdict: ReviewVerdict::Pending,
            active_tab: GitOpsTab::Changes,
            ai_commit: AiCommitState::default(),
            push_data: PushData::default(),
            pr_data: PullRequestsData::default(),
            branches_data: BranchesData::default(),
            lfs_data: LfsData::default(),
            gitflow_data: GitflowData::default(),
        }
    }

    /// Empty state returned when the path is not a git repository.
    pub fn empty() -> Self {
        Self {
            branch: String::new(),
            last_commit_hash: String::new(),
            last_commit_msg: String::new(),
            modified_count: 0,
            staged_count: 0,
            untracked_count: 0,
            files: Vec::new(),
            diff_lines: Vec::new(),
            selected_file: None,
            recent_commits: Vec::new(),
            is_repo: false,
            comments: Vec::new(),
            verdict: ReviewVerdict::Pending,
            active_tab: GitOpsTab::Changes,
            ai_commit: AiCommitState::default(),
            push_data: PushData::default(),
            pr_data: PullRequestsData::default(),
            branches_data: BranchesData::default(),
            lfs_data: LfsData::default(),
            gitflow_data: GitflowData::default(),
        }
    }

    /// Load from the current working directory. Falls back to `empty()` on failure.
    pub fn from_cwd() -> Self {
        match std::env::current_dir() {
            Ok(cwd) => Self::from_git(&cwd),
            Err(_) => Self::empty(),
        }
    }

    // -- Comment management --

    /// Add an inline comment to the review.
    pub fn add_comment(&mut self, comment: ReviewComment) {
        self.comments.push(comment);
    }

    /// Get all comments for a specific file.
    pub fn comments_for_file(&self, file_path: &str) -> Vec<&ReviewComment> {
        self.comments
            .iter()
            .filter(|c| c.file_path == file_path)
            .collect()
    }

    /// Get all unresolved comments.
    pub fn unresolved_comments(&self) -> Vec<&ReviewComment> {
        self.comments.iter().filter(|c| !c.resolved).collect()
    }

    /// Resolve a comment by ID. Returns true if found and resolved.
    pub fn resolve_comment(&mut self, id: &str) -> bool {
        if let Some(comment) = self.comments.iter_mut().find(|c| c.id == id) {
            comment.resolve();
            true
        } else {
            false
        }
    }

    /// Set the review verdict.
    pub fn set_verdict(&mut self, verdict: ReviewVerdict) {
        self.verdict = verdict;
    }

    // -- Private helpers --

    /// Query git status and build file entries with staged detection.
    ///
    /// Uses `git2::Status` flags via `GitService::status()`. We re-open the
    /// raw status to detect index vs worktree flags for staged detection.
    fn load_file_statuses(git: &GitService) -> (Vec<ReviewFileEntry>, usize, usize, usize) {
        let statuses = match git.status() {
            Ok(s) => s,
            Err(_) => return (Vec::new(), 0, 0, 0),
        };

        let mut modified_count: usize = 0;
        let mut staged_count: usize = 0;
        let mut untracked_count: usize = 0;

        let files: Vec<ReviewFileEntry> = statuses
            .iter()
            .map(|fs| {
                let review_status = ReviewFileStatus::from(fs.status);

                // Detect staged: index_* flags mean the file is staged.
                // GitService only returns FileStatusType, but we can infer
                // staged based on whether it was an index operation.
                // For now, Added files are typically staged (they came from
                // `git add`), and others we mark as unstaged unless we can
                // detect further. We'll use a heuristic: if status is Added
                // and not Untracked, it's staged.
                let is_staged = matches!(review_status, ReviewFileStatus::Added);

                match review_status {
                    ReviewFileStatus::Modified => modified_count += 1,
                    ReviewFileStatus::Untracked => untracked_count += 1,
                    _ => {}
                }
                if is_staged {
                    staged_count += 1;
                }

                ReviewFileEntry {
                    path: fs.path.to_string_lossy().to_string(),
                    status: review_status,
                    additions: 0,
                    deletions: 0,
                    is_staged,
                }
            })
            .collect();

        (files, modified_count, staged_count, untracked_count)
    }

    /// Parse a unified diff string into structured `DiffLine` entries.
    ///
    /// Handles hunk headers (`@@`), additions (`+`), deletions (`-`), and
    /// context lines (` `). Tracks old/new line numbers through the diff.
    ///
    /// Skips file-level metadata lines (`diff --git`, `index`, `---`, `+++`)
    /// that appear before hunk headers. Only processes `+`/`-`/` ` lines
    /// that appear after we have seen at least one hunk header.
    pub fn parse_diff(raw: &str) -> Vec<DiffLine> {
        let mut lines = Vec::new();
        let mut old_line: usize = 0;
        let mut new_line: usize = 0;
        let mut in_hunk = false;

        for text in raw.lines() {
            if text.starts_with("@@") {
                // Parse hunk header: @@ -old,count +new,count @@
                if let Some((o, n)) = Self::parse_hunk_header(text) {
                    old_line = o;
                    new_line = n;
                }
                in_hunk = true;
                lines.push(DiffLine {
                    line_num_old: None,
                    line_num_new: None,
                    kind: DiffLineKind::Hunk,
                    content: text.to_string(),
                });
            } else if text.starts_with("diff ")
                || text.starts_with("index ")
                || text.starts_with("--- ")
                || text.starts_with("+++ ")
            {
                // File-level metadata: skip and reset hunk state.
                in_hunk = false;
            } else if !in_hunk {
                // Outside a hunk, skip unknown lines.
                continue;
            } else if let Some(content) = text.strip_prefix('+') {
                lines.push(DiffLine {
                    line_num_old: None,
                    line_num_new: Some(new_line),
                    kind: DiffLineKind::Addition,
                    content: content.to_string(),
                });
                new_line += 1;
            } else if let Some(content) = text.strip_prefix('-') {
                lines.push(DiffLine {
                    line_num_old: Some(old_line),
                    line_num_new: None,
                    kind: DiffLineKind::Deletion,
                    content: content.to_string(),
                });
                old_line += 1;
            } else if let Some(content) = text.strip_prefix(' ') {
                lines.push(DiffLine {
                    line_num_old: Some(old_line),
                    line_num_new: Some(new_line),
                    kind: DiffLineKind::Context,
                    content: content.to_string(),
                });
                old_line += 1;
                new_line += 1;
            }
        }

        lines
    }

    /// Extract old and new starting line numbers from a hunk header.
    /// Format: `@@ -old_start[,count] +new_start[,count] @@`
    pub fn parse_hunk_header(header: &str) -> Option<(usize, usize)> {
        // Find the ranges between @@ markers
        let inner = header.strip_prefix("@@")?.split("@@").next()?.trim();

        let mut parts = inner.split_whitespace();

        let old_part = parts.next()?.strip_prefix('-')?;
        let old_start: usize = old_part.split(',').next()?.parse().ok()?;

        let new_part = parts.next()?.strip_prefix('+')?;
        let new_start: usize = new_part.split(',').next()?.parse().ok()?;

        Some((old_start, new_start))
    }

    /// Format a Unix timestamp into a human-readable "X ago" string.
    pub fn format_time_ago(timestamp: i64) -> String {
        let now = chrono::Utc::now().timestamp();
        let delta = now - timestamp;

        if delta < 0 {
            return "just now".into();
        }

        let seconds = delta as u64;
        let minutes = seconds / 60;
        let hours = minutes / 60;
        let days = hours / 24;
        let weeks = days / 7;
        let months = days / 30;

        if seconds < 60 {
            "just now".into()
        } else if minutes < 60 {
            format!("{minutes} min ago")
        } else if hours < 24 {
            if hours == 1 {
                "1 hour ago".into()
            } else {
                format!("{hours} hours ago")
            }
        } else if days < 7 {
            if days == 1 {
                "1 day ago".into()
            } else {
                format!("{days} days ago")
            }
        } else if weeks < 5 {
            if weeks == 1 {
                "1 week ago".into()
            } else {
                format!("{weeks} weeks ago")
            }
        } else if months == 1 {
            "1 month ago".into()
        } else {
            format!("{months} months ago")
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Code review panel: git status, changed files list, diff viewer,
/// inline comments, and review verdict actions.
pub struct ReviewPanel;

impl ReviewPanel {
    pub fn render(data: &ReviewData, theme: &HiveTheme) -> impl IntoElement {
        if !data.is_repo {
            return Self::empty_state(theme).into_any_element();
        }

        div()
            .id("review-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(Self::header(data, theme))
            .child(Self::render_tab_bar(data, theme))
            .when(data.active_tab == GitOpsTab::Changes, |el| {
                el.child(Self::branch_card(data, theme))
                    .child(Self::changed_files_card(data, theme))
                    .child(Self::diff_viewer_card(data, theme))
                    .child(Self::comments_card(data, theme))
                    .child(Self::recent_commits_card(data, theme))
                    .child(Self::review_actions(data, theme))
                    .child(Self::git_actions(data, theme))
            })
            .when(data.active_tab == GitOpsTab::Push, |el| {
                el.child(Self::render_push_tab(data, theme))
            })
            .when(data.active_tab == GitOpsTab::PullRequests, |el| {
                el.child(Self::render_pr_tab(data, theme))
            })
            .when(data.active_tab == GitOpsTab::Branches, |el| {
                el.child(Self::render_branches_tab(data, theme))
            })
            .when(data.active_tab == GitOpsTab::Lfs, |el| {
                el.child(Self::render_lfs_tab(data, theme))
            })
            .when(data.active_tab == GitOpsTab::Gitflow, |el| {
                el.child(Self::render_gitflow_tab(data, theme))
            })
            .into_any_element()
    }

    // ------------------------------------------------------------------
    // Empty state (not a git repo)
    // ------------------------------------------------------------------

    fn empty_state(theme: &HiveTheme) -> impl IntoElement {
        div()
            .id("review-panel-empty")
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap(theme.space_4)
            .child(Icon::new(IconName::Eye).size_4())
            .child(
                div()
                    .text_size(theme.font_size_xl)
                    .text_color(theme.text_secondary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("No Git Repository"),
            )
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .max_w(px(360.0))
                    .child(
                        "The current directory is not a git repository. \
                         Open a project with git initialized to view changes.",
                    ),
            )
    }

    // ------------------------------------------------------------------
    // Header
    // ------------------------------------------------------------------

    fn header(data: &ReviewData, theme: &HiveTheme) -> impl IntoElement {
        let verdict_color = match data.verdict {
            ReviewVerdict::Approved => theme.accent_green,
            ReviewVerdict::RequestChanges => theme.accent_red,
            ReviewVerdict::Comment => theme.accent_yellow,
            ReviewVerdict::Pending => theme.text_muted,
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .child(
                div()
                    .text_size(theme.font_size_2xl)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::BOLD)
                    .child("Code Review"),
            )
            .child(div().flex_1())
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_xs)
                    .text_color(verdict_color)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(data.verdict.label().to_string()),
            )
            .child(
                div()
                    .px(theme.space_3)
                    .py(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child("Git"),
            )
    }

    // ------------------------------------------------------------------
    // Branch / repository status card
    // ------------------------------------------------------------------

    fn branch_card(data: &ReviewData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(
                div()
                    .text_size(theme.font_size_lg)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Repository Status"),
            )
            .child(Self::info_row(
                "\u{1F33F}",
                "Branch",
                &data.branch,
                theme.accent_cyan,
                theme,
            ))
            .child(Self::info_row(
                "\u{1F4DD}",
                "Last Commit",
                &format!("{} {}", data.last_commit_hash, data.last_commit_msg),
                theme.text_primary,
                theme,
            ))
            .child(Self::info_row(
                "\u{1F4CA}",
                "Changes",
                &format!(
                    "{} modified, {} staged, {} untracked",
                    data.modified_count, data.staged_count, data.untracked_count
                ),
                theme.text_primary,
                theme,
            ))
    }

    fn info_row(
        icon: &str,
        label: &str,
        value: &str,
        value_color: Hsla,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .child(
                div()
                    .w(px(20.0))
                    .text_size(theme.font_size_sm)
                    .child(icon.to_string()),
            )
            .child(
                div()
                    .w(px(100.0))
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_sm)
                    .text_color(value_color)
                    .child(value.to_string()),
            )
    }

    // ------------------------------------------------------------------
    // Changed files list
    // ------------------------------------------------------------------

    fn changed_files_card(data: &ReviewData, theme: &HiveTheme) -> impl IntoElement {
        let mut container = div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(Self::changed_files_header(data, theme));

        if data.files.is_empty() {
            container = container.child(Self::centered_message("No changes detected", theme));
        } else {
            // Column header
            container = container.child(Self::file_list_header(theme));
            for entry in &data.files {
                let is_selected = data
                    .selected_file
                    .as_ref() == Some(&entry.path);
                let comment_count = data.comments_for_file(&entry.path).len();
                container =
                    container.child(Self::file_row(entry, is_selected, comment_count, theme));
            }
        }

        container
    }

    fn changed_files_header(data: &ReviewData, theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_lg)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Changed Files"),
            )
            .child(
                div()
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .rounded(theme.radius_full)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_secondary)
                    .child(format!("{}", data.files.len())),
            )
    }

    fn centered_message(msg: &str, theme: &HiveTheme) -> Div {
        div()
            .py(theme.space_4)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .child(msg.to_string()),
            )
    }

    fn file_list_header(theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .pb(theme.space_1)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .w(px(28.0))
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("St"),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("File"),
            )
            .child(
                div()
                    .w(px(80.0))
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Changes"),
            )
            .child(
                div()
                    .w(px(56.0))
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Staged"),
            )
    }

    fn file_row(
        entry: &ReviewFileEntry,
        is_selected: bool,
        comment_count: usize,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        let status_color = match entry.status {
            ReviewFileStatus::Modified => theme.accent_yellow,
            ReviewFileStatus::Added => theme.accent_green,
            ReviewFileStatus::Deleted => theme.accent_red,
            ReviewFileStatus::Renamed => theme.accent_cyan,
            ReviewFileStatus::Untracked => theme.text_muted,
        };

        let bg = if is_selected {
            theme.bg_tertiary
        } else {
            Hsla::transparent_black()
        };

        let changes_text = match (entry.additions, entry.deletions) {
            (0, 0) => "untracked".to_string(),
            (a, 0) => format!("+{a}"),
            (0, d) => format!("-{d}"),
            (a, d) => format!("+{a} -{d}"),
        };

        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .py(theme.space_1)
            .px(theme.space_1)
            .rounded(theme.radius_sm)
            .bg(bg)
            .child(Self::file_status_badge(entry, status_color, theme))
            .child(Self::file_path_cell(entry, theme))
            .child(Self::file_changes_cell(entry, &changes_text, theme))
            .child(Self::file_staged_cell(entry, theme));

        // Show comment count badge if there are comments on this file
        if comment_count > 0 {
            row = row.child(
                div()
                    .px(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.accent_cyan)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_on_accent)
                    .font_weight(FontWeight::BOLD)
                    .child(format!("{comment_count}")),
            );
        }

        row
    }

    fn file_status_badge(entry: &ReviewFileEntry, status_color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .w(px(24.0))
            .h(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radius_sm)
            .bg(theme.bg_tertiary)
            .text_size(theme.font_size_xs)
            .text_color(status_color)
            .font_weight(FontWeight::BOLD)
            .child(entry.status.label().to_string())
    }

    fn file_path_cell(entry: &ReviewFileEntry, theme: &HiveTheme) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_1)
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_primary)
                    .overflow_hidden()
                    .child(entry.path.clone()),
            )
    }

    fn file_changes_cell(entry: &ReviewFileEntry, changes_text: &str, theme: &HiveTheme) -> Div {
        div()
            .w(px(80.0))
            .text_size(theme.font_size_xs)
            .text_color(if entry.additions > 0 {
                theme.accent_green
            } else if entry.deletions > 0 {
                theme.accent_red
            } else {
                theme.text_muted
            })
            .child(changes_text.to_string())
    }

    fn file_staged_cell(entry: &ReviewFileEntry, theme: &HiveTheme) -> Div {
        div()
            .w(px(56.0))
            .text_size(theme.font_size_xs)
            .text_color(if entry.is_staged {
                theme.accent_green
            } else {
                theme.text_muted
            })
            .child(if entry.is_staged {
                "\u{2713} Yes"
            } else {
                "No"
            })
    }

    // ------------------------------------------------------------------
    // Diff viewer
    // ------------------------------------------------------------------

    fn diff_viewer_card(data: &ReviewData, theme: &HiveTheme) -> impl IntoElement {
        let mut container = div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(Self::diff_viewer_header(data, theme));

        if data.diff_lines.is_empty() {
            container = container.child(Self::centered_empty("No diff to display", theme));
        } else {
            // Diff container with dark background and monospace font
            let mut diff_box = div()
                .flex()
                .flex_col()
                .bg(theme.bg_primary)
                .rounded(theme.radius_sm)
                .p(theme.space_2);

            for line in &data.diff_lines {
                diff_box = diff_box.child(Self::render_diff_line(line, theme));
            }

            container = container.child(diff_box);
        }

        container
    }

    fn diff_viewer_header(data: &ReviewData, theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_lg)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Diff"),
            )
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.accent_cyan)
                    .child(
                        data.selected_file
                            .clone()
                            .unwrap_or_else(|| "No file selected".to_string()),
                    ),
            )
    }

    fn centered_empty(msg: &str, theme: &HiveTheme) -> Div {
        div()
            .py(theme.space_6)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_muted)
                    .child(msg.to_string()),
            )
    }

    fn render_diff_line(line: &DiffLine, theme: &HiveTheme) -> impl IntoElement {
        let (bg, text_color, prefix) = match line.kind {
            DiffLineKind::Addition => {
                (hsla(120.0 / 360.0, 0.4, 0.12, 0.4), theme.accent_green, "+")
            }
            DiffLineKind::Deletion => (hsla(0.0, 0.4, 0.12, 0.4), theme.accent_red, "-"),
            DiffLineKind::Hunk => (hsla(220.0 / 360.0, 0.3, 0.15, 0.3), theme.accent_cyan, "@"),
            DiffLineKind::Context => (Hsla::transparent_black(), theme.text_secondary, " "),
        };

        let old_num = line
            .line_num_old
            .map(|n| format!("{n:>4}"))
            .unwrap_or_else(|| "    ".to_string());
        let new_num = line
            .line_num_new
            .map(|n| format!("{n:>4}"))
            .unwrap_or_else(|| "    ".to_string());

        div()
            .flex()
            .flex_row()
            .items_center()
            .px(theme.space_1)
            .bg(bg)
            .child(Self::diff_line_num(old_num, theme))
            .child(Self::diff_line_num(new_num, theme))
            .child(Self::diff_prefix(prefix, text_color, theme))
            .child(Self::diff_content(&line.content, text_color, theme))
    }

    fn diff_line_num(num: String, theme: &HiveTheme) -> Div {
        div()
            .w(px(40.0))
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .font_family(theme.font_mono.clone())
            .child(num)
    }

    fn diff_prefix(prefix: &str, color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .w(px(16.0))
            .text_size(theme.font_size_sm)
            .text_color(color)
            .font_family(theme.font_mono.clone())
            .child(prefix.to_string())
    }

    fn diff_content(content: &str, color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .text_size(theme.font_size_sm)
            .text_color(color)
            .font_family(theme.font_mono.clone())
            .child(content.to_string())
    }

    // ------------------------------------------------------------------
    // Comments / Annotations
    // ------------------------------------------------------------------

    fn comments_card(data: &ReviewData, theme: &HiveTheme) -> impl IntoElement {
        let total = data.comments.len();
        let unresolved = data.unresolved_comments().len();

        let mut container = div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(Self::comments_header(total, unresolved, theme));

        if data.comments.is_empty() {
            container = container.child(Self::centered_message(
                "No comments yet. Click on a diff line to add a comment.",
                theme,
            ));
        } else {
            for comment in &data.comments {
                container = container.child(Self::comment_row(comment, theme));
            }
        }

        container
    }

    fn comments_header(total: usize, unresolved: usize, theme: &HiveTheme) -> Div {
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_lg)
                    .text_color(theme.text_primary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Comments"),
            )
            .child(Self::count_badge(&format!("{total}"), theme));

        if unresolved > 0 {
            header.child(Self::unresolved_badge(unresolved, theme))
        } else {
            header
        }
    }

    fn count_badge(text: &str, theme: &HiveTheme) -> Div {
        div()
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_full)
            .bg(theme.bg_tertiary)
            .text_size(theme.font_size_xs)
            .text_color(theme.text_secondary)
            .child(text.to_string())
    }

    fn unresolved_badge(unresolved: usize, theme: &HiveTheme) -> Div {
        div()
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_full)
            .bg(theme.accent_yellow)
            .text_size(theme.font_size_xs)
            .text_color(theme.text_on_accent)
            .child(format!("{unresolved} unresolved"))
    }

    fn comment_row(comment: &ReviewComment, theme: &HiveTheme) -> impl IntoElement {
        let location = if let Some(line) = comment.line_number {
            format!("{}:{}", comment.file_path, line)
        } else {
            comment.file_path.clone()
        };

        let mut row = div()
            .flex()
            .flex_col()
            .p(theme.space_3)
            .rounded(theme.radius_md)
            .bg(theme.bg_primary)
            .border_1()
            .border_color(theme.border)
            .gap(theme.space_1)
            .child(Self::comment_row_header(comment, &location, theme))
            .child(Self::comment_row_body(comment, theme));

        // Resolved badge
        if comment.resolved {
            row = row.child(Self::resolved_indicator(theme.accent_green, theme));
        }

        row
    }

    fn comment_row_header(comment: &ReviewComment, location: &str, theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.accent_cyan)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(comment.author.clone()),
            )
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .font_family(theme.font_mono.clone())
                    .child(location.to_string()),
            )
            .child(div().flex_1())
    }

    fn comment_row_body(comment: &ReviewComment, theme: &HiveTheme) -> Div {
        div()
            .text_size(theme.font_size_sm)
            .text_color(theme.text_primary)
            .child(comment.body.clone())
    }

    fn resolved_indicator(color: Hsla, theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_1)
            .child(div().w(px(6.0)).h(px(6.0)).rounded(px(9999.0)).bg(color))
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(color)
                    .child("Resolved"),
            )
    }

    // ------------------------------------------------------------------
    // Recent commits
    // ------------------------------------------------------------------

    fn recent_commits_card(data: &ReviewData, theme: &HiveTheme) -> impl IntoElement {
        let mut container = div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_2)
            .child(Self::section_title("Recent Commits", theme));

        if data.recent_commits.is_empty() {
            container = container.child(Self::centered_message("No commits yet", theme));
        } else {
            for commit in &data.recent_commits {
                container = container.child(Self::commit_row(commit, theme));
            }
        }

        container
    }

    fn commit_row(commit: &CommitEntry, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_2)
            .py(theme.space_1)
            // Hash
            .child(
                div()
                    .px(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.accent_aqua)
                    .font_family(theme.font_mono.clone())
                    .child(commit.hash.clone()),
            )
            // Message
            .child(
                div()
                    .flex_1()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_primary)
                    .overflow_hidden()
                    .child(commit.message.clone()),
            )
            // Author
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_secondary)
                    .child(commit.author.clone()),
            )
            // Time
            .child(
                div()
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_muted)
                    .child(commit.time_ago.clone()),
            )
    }

    // ------------------------------------------------------------------
    // Review action buttons (Approve / Request Changes / Comment)
    // ------------------------------------------------------------------

    fn review_actions(data: &ReviewData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius_md)
            .p(theme.space_4)
            .gap(theme.space_3)
            .child(Self::section_title("Review Decision", theme))
            .child(Self::review_verdict_summary(data, theme))
            .child(Self::review_buttons_row(data, theme))
    }

    fn section_title(text: &str, theme: &HiveTheme) -> Div {
        div()
            .text_size(theme.font_size_lg)
            .text_color(theme.text_primary)
            .font_weight(FontWeight::SEMIBOLD)
            .child(text.to_string())
    }

    fn review_verdict_summary(data: &ReviewData, theme: &HiveTheme) -> Div {
        div()
            .text_size(theme.font_size_sm)
            .text_color(theme.text_muted)
            .child(format!(
                "Current verdict: {}  |  {} comments ({} unresolved)",
                data.verdict.label(),
                data.comments.len(),
                data.unresolved_comments().len(),
            ))
    }

    fn review_buttons_row(data: &ReviewData, theme: &HiveTheme) -> Div {
        div()
            .flex()
            .flex_row()
            .gap(theme.space_3)
            .child(Self::review_btn(
                "\u{2713} Approve",
                theme.accent_green,
                data.verdict == ReviewVerdict::Approved,
                theme,
            ))
            .child(Self::review_btn(
                "\u{2717} Request Changes",
                theme.accent_red,
                data.verdict == ReviewVerdict::RequestChanges,
                theme,
            ))
            .child(Self::review_btn(
                "\u{1F4AC} Comment",
                theme.accent_yellow,
                data.verdict == ReviewVerdict::Comment,
                theme,
            ))
    }

    fn review_btn(
        label: &str,
        color: Hsla,
        is_active: bool,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        let (bg, text_color, border_color) = if is_active {
            (color, theme.text_on_accent, color)
        } else {
            (theme.bg_surface, color, theme.border)
        };

        div()
            .px(theme.space_4)
            .py(theme.space_2)
            .rounded(theme.radius_md)
            .bg(bg)
            .border_1()
            .border_color(border_color)
            .text_size(theme.font_size_sm)
            .text_color(text_color)
            .font_weight(FontWeight::SEMIBOLD)
            .child(label.to_string())
    }

    // ------------------------------------------------------------------
    // Git action buttons (Stage / Unstage / Commit / Discard)
    // ------------------------------------------------------------------

    fn git_actions(data: &ReviewData, theme: &HiveTheme) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(theme.space_3)
            // AI Commit message card
            .child(
                div()
                    .flex()
                    .flex_col()
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(theme.radius_md)
                    .p(theme.space_4)
                    .gap(theme.space_2)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_size(theme.font_size_lg)
                                    .text_color(theme.text_primary)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Commit Message"),
                            )
                            .child(
                                div()
                                    .id("ai-commit-msg-btn")
                                    .px(theme.space_3)
                                    .py(theme.space_1)
                                    .rounded(theme.radius_sm)
                                    .bg(theme.accent_cyan)
                                    .text_size(theme.font_size_xs)
                                    .text_color(theme.text_on_accent)
                                    .cursor_pointer()
                                    .when(data.ai_commit.generating, |el| el.opacity(0.5))
                                    .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                                        window.dispatch_action(
                                            Box::new(ReviewAiCommitMessage),
                                            cx,
                                        );
                                    })
                                    .child(if data.ai_commit.generating {
                                        "Generating..."
                                    } else {
                                        "AI Generate"
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .px(theme.space_3)
                            .py(theme.space_2)
                            .rounded(theme.radius_sm)
                            .bg(theme.bg_primary)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(theme.font_size_sm)
                            .text_color(if data.ai_commit.user_edited_message.is_empty() {
                                theme.text_muted
                            } else {
                                theme.text_primary
                            })
                            .min_h(px(60.0))
                            .child(if data.ai_commit.user_edited_message.is_empty() {
                                "(type or AI-generate a commit message)".to_string()
                            } else {
                                data.ai_commit.user_edited_message.clone()
                            }),
                    ),
            )
            // Action buttons row
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(theme.space_3)
                    .child(
                        Self::action_btn(
                            "review-stage-all",
                            "Stage All",
                            theme.accent_cyan,
                            theme,
                        )
                        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                            window.dispatch_action(Box::new(ReviewStageAll), cx);
                        }),
                    )
                    .child(
                        Self::action_btn(
                            "review-unstage-all",
                            "Unstage All",
                            theme.accent_yellow,
                            theme,
                        )
                        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                            window.dispatch_action(Box::new(ReviewUnstageAll), cx);
                        }),
                    )
                    .child(
                        Self::action_btn(
                            "review-commit",
                            "Commit",
                            theme.accent_green,
                            theme,
                        )
                        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                            window.dispatch_action(Box::new(ReviewCommitWithMessage), cx);
                        }),
                    )
                    .child(div().flex_1())
                    .child(
                        Self::action_btn(
                            "review-discard-all",
                            "Discard All",
                            theme.accent_red,
                            theme,
                        )
                        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                            window.dispatch_action(Box::new(ReviewDiscardAll), cx);
                        }),
                    ),
            )
    }

    // ------------------------------------------------------------------
    // Tab bar
    // ------------------------------------------------------------------

    fn render_tab_bar(data: &ReviewData, theme: &HiveTheme) -> Div {
        let mut bar = div()
            .flex()
            .flex_row()
            .gap_1()
            .border_b_1()
            .border_color(theme.border)
            .pb_2()
            .mb_4();

        for tab in GitOpsTab::ALL {
            let is_active = data.active_tab == tab;
            let tab_str = tab.to_str().to_string();
            bar = bar.child(
                div()
                    .id(SharedString::from(format!("gitops-tab-{}", tab.to_str())))
                    .px_3()
                    .py_2()
                    .rounded_t_md()
                    .bg(if is_active {
                        theme.bg_tertiary
                    } else {
                        Hsla::transparent_black()
                    })
                    .text_size(rems(0.8125))
                    .text_color(if is_active {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    })
                    .font_weight(if is_active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, {
                        move |_event, window, cx| {
                            window.dispatch_action(
                                Box::new(ReviewSwitchTab {
                                    tab: tab_str.clone(),
                                }),
                                cx,
                            );
                        }
                    })
                    .child(tab.label()),
            );
        }
        bar
    }

    // ------------------------------------------------------------------
    // Push tab
    // ------------------------------------------------------------------

    fn render_push_tab(data: &ReviewData, theme: &HiveTheme) -> Div {
        let push = &data.push_data;
        div()
            .flex()
            .flex_col()
            .gap_4()
            // Remote info card
            .child(
                div()
                    .p_4()
                    .rounded_md()
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(rems(0.875))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child("Remote"),
                    )
                    .child(
                        div()
                            .text_size(rems(0.8125))
                            .text_color(theme.text_muted)
                            .child(format!("{} — {}", push.remote_name, push.remote_url)),
                    )
                    .child(
                        div()
                            .text_size(rems(0.8125))
                            .text_color(theme.text_muted)
                            .child(format!(
                                "Tracking: {}",
                                push.tracking_branch.as_deref().unwrap_or("none")
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_4()
                            .child(
                                div()
                                    .text_size(rems(0.8125))
                                    .text_color(theme.accent_cyan)
                                    .child(format!("{} ahead", push.ahead_count)),
                            )
                            .child(
                                div()
                                    .text_size(rems(0.8125))
                                    .text_color(theme.accent_yellow)
                                    .child(format!("{} behind", push.behind_count)),
                            ),
                    ),
            )
            // Buttons
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        div()
                            .id("push-btn")
                            .px_4()
                            .py_2()
                            .rounded_md()
                            .bg(theme.accent_cyan)
                            .text_color(theme.text_on_accent)
                            .text_size(rems(0.8125))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .when(push.push_in_progress, |el| el.opacity(0.5))
                            .on_mouse_down(MouseButton::Left, |_e, w, cx| {
                                w.dispatch_action(Box::new(ReviewPush), cx);
                            })
                            .child(if push.push_in_progress {
                                "Pushing..."
                            } else {
                                "Push"
                            }),
                    )
                    .child(
                        div()
                            .id("push-upstream-btn")
                            .px_4()
                            .py_2()
                            .rounded_md()
                            .bg(theme.bg_tertiary)
                            .text_color(theme.text_primary)
                            .text_size(rems(0.8125))
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, |_e, w, cx| {
                                w.dispatch_action(Box::new(ReviewPushSetUpstream), cx);
                            })
                            .child("Push + Set Upstream"),
                    ),
            )
            // Last push result
            .when(push.last_push_result.is_some(), |el| {
                let result = push.last_push_result.as_ref().expect("guarded by is_some check");
                let (color, msg) = match result {
                    Ok(m) => (
                        theme.accent_green,
                        format!(
                            "Push successful{}",
                            if m.is_empty() {
                                String::new()
                            } else {
                                format!(": {m}")
                            }
                        ),
                    ),
                    Err(e) => (theme.accent_red, format!("Push failed: {e}")),
                };
                el.child(
                    div()
                        .p_3()
                        .rounded_md()
                        .bg(theme.bg_surface)
                        .border_1()
                        .border_color(color)
                        .text_size(rems(0.8125))
                        .text_color(color)
                        .child(msg),
                )
            })
    }

    // ------------------------------------------------------------------
    // Pull Requests tab
    // ------------------------------------------------------------------

    fn render_pr_tab(data: &ReviewData, theme: &HiveTheme) -> Div {
        let pr = &data.pr_data;
        let mut content = div().flex().flex_col().gap_4();

        if !pr.github_connected {
            return content.child(
                div()
                    .p_4()
                    .rounded_md()
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.accent_yellow)
                    .text_size(rems(0.8125))
                    .text_color(theme.accent_yellow)
                    .child("GitHub not connected. Connect via Settings > Connected Accounts."),
            );
        }

        // Create PR form
        content = content.child(
            div()
                .p_4()
                .rounded_md()
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .child(
                            div()
                                .text_size(rems(0.875))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_primary)
                                .child("Create Pull Request"),
                        )
                        .child(
                            div()
                                .id("pr-ai-generate-btn")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(theme.accent_cyan)
                                .text_color(theme.text_on_accent)
                                .text_size(rems(0.75))
                                .cursor_pointer()
                                .when(pr.pr_form.ai_generating, |el| el.opacity(0.5))
                                .on_mouse_down(MouseButton::Left, |_e, w, cx| {
                                    w.dispatch_action(Box::new(ReviewPrAiGenerate), cx);
                                })
                                .child(if pr.pr_form.ai_generating {
                                    "Generating..."
                                } else {
                                    "AI Generate"
                                }),
                        ),
                )
                // Title
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .text_color(theme.text_muted)
                                .child("Title"),
                        )
                        .child(
                            div()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.bg_primary)
                                .border_1()
                                .border_color(theme.border)
                                .text_size(rems(0.8125))
                                .text_color(theme.text_primary)
                                .child(if pr.pr_form.title.is_empty() {
                                    "(click AI Generate to fill)".to_string()
                                } else {
                                    pr.pr_form.title.clone()
                                }),
                        ),
                )
                // Body
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .text_color(theme.text_muted)
                                .child("Body / Release Notes"),
                        )
                        .child(
                            div()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.bg_primary)
                                .border_1()
                                .border_color(theme.border)
                                .text_size(rems(0.75))
                                .text_color(theme.text_secondary)
                                .min_h(px(80.))
                                .child(if pr.pr_form.body.is_empty() {
                                    "(AI will generate release notes here)".to_string()
                                } else {
                                    pr.pr_form.body.clone()
                                }),
                        ),
                )
                // Base branch + Create button
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_3()
                        .items_center()
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .text_color(theme.text_muted)
                                .child(format!("Base: {}", pr.pr_form.base_branch)),
                        )
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .text_color(theme.text_muted)
                                .child(format!("Head: {}", data.branch)),
                        )
                        .child(
                            div()
                                .id("pr-create-btn")
                                .px_4()
                                .py_2()
                                .rounded_md()
                                .bg(theme.accent_green)
                                .text_color(theme.text_on_accent)
                                .text_size(rems(0.8125))
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .when(pr.loading, |el| el.opacity(0.5))
                                .on_mouse_down(MouseButton::Left, |_e, w, cx| {
                                    w.dispatch_action(Box::new(ReviewPrCreate), cx);
                                })
                                .child(if pr.loading {
                                    "Creating..."
                                } else {
                                    "Create PR"
                                }),
                        ),
                ),
        );

        // Open PRs list
        if !pr.open_prs.is_empty() {
            let mut list = div()
                .p_4()
                .rounded_md()
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_size(rems(0.875))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child(format!("Open Pull Requests ({})", pr.open_prs.len())),
                );

            for p in &pr.open_prs {
                list = list.child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .py_1()
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .text_color(theme.accent_cyan)
                                .child(format!("#{}", p.number)),
                        )
                        .child(
                            div()
                                .text_size(rems(0.8125))
                                .text_color(theme.text_primary)
                                .flex_1()
                                .child(p.title.clone()),
                        )
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .text_color(theme.text_muted)
                                .child(p.author.clone()),
                        ),
                );
            }
            content = content.child(list);
        }

        content
    }

    // ------------------------------------------------------------------
    // Branches tab
    // ------------------------------------------------------------------

    fn render_branches_tab(data: &ReviewData, theme: &HiveTheme) -> Div {
        let bd = &data.branches_data;
        let mut content = div().flex().flex_col().gap_4();

        // Branch list
        let mut list = div()
            .p_4()
            .rounded_md()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(rems(0.875))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(format!("Branches ({})", bd.branches.len())),
            );

        for branch in &bd.branches {
            let branch_name = branch.name.clone();
            let branch_name2 = branch.name.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("branch-{}", branch.name)))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .px_2()
                    .rounded_md()
                    .when(branch.is_current, |el| el.bg(theme.bg_tertiary))
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, {
                        let name = branch_name.clone();
                        move |_e, w, cx| {
                            w.dispatch_action(
                                Box::new(ReviewBranchSwitch {
                                    branch_name: name.clone(),
                                }),
                                cx,
                            );
                        }
                    })
                    .child(
                        div()
                            .text_size(rems(0.8125))
                            .text_color(if branch.is_current {
                                theme.accent_cyan
                            } else {
                                theme.text_primary
                            })
                            .font_weight(if branch.is_current {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(format!(
                                "{}{}",
                                if branch.is_current { "* " } else { "  " },
                                branch.name
                            )),
                    )
                    .when(branch.is_remote, |el| {
                        el.child(
                            div()
                                .text_size(rems(0.625))
                                .text_color(theme.text_muted)
                                .child("remote"),
                        )
                    })
                    .child(
                        div()
                            .text_size(rems(0.75))
                            .text_color(theme.text_muted)
                            .flex_1()
                            .child(branch.last_commit_msg.clone()),
                    )
                    .when(!branch.is_current, |el| {
                        el.child(
                            div()
                                .id(SharedString::from(format!("del-branch-{}", branch_name2)))
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .text_size(rems(0.625))
                                .text_color(theme.accent_red)
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, {
                                    let name = branch_name2.clone();
                                    move |_e, w, cx| {
                                        w.dispatch_action(
                                            Box::new(ReviewBranchDeleteNamed {
                                                branch_name: name.clone(),
                                            }),
                                            cx,
                                        );
                                    }
                                })
                                .child("Delete"),
                        )
                    }),
            );
        }
        content = content.child(list);

        // New branch
        content = content.child(
            div()
                .p_4()
                .rounded_md()
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_row()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_size(rems(0.8125))
                        .text_color(theme.text_muted)
                        .child("New branch:"),
                )
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(theme.bg_primary)
                        .border_1()
                        .border_color(theme.border)
                        .text_size(rems(0.8125))
                        .text_color(theme.text_primary)
                        .min_w(px(200.))
                        .child(if bd.new_branch_name.is_empty() {
                            "(type branch name)".to_string()
                        } else {
                            bd.new_branch_name.clone()
                        }),
                )
                .child(
                    div()
                        .id("create-branch-btn")
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(theme.accent_cyan)
                        .text_color(theme.text_on_accent)
                        .text_size(rems(0.8125))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, |_e, w, cx| {
                            w.dispatch_action(Box::new(ReviewBranchCreate), cx);
                        })
                        .child("Create"),
                ),
        );

        content
    }

    // ------------------------------------------------------------------
    // LFS tab
    // ------------------------------------------------------------------

    fn render_lfs_tab(data: &ReviewData, theme: &HiveTheme) -> Div {
        let lfs = &data.lfs_data;
        let mut content = div().flex().flex_col().gap_4();

        if !lfs.is_lfs_installed {
            return content.child(
                div()
                    .p_4()
                    .rounded_md()
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.accent_yellow)
                    .text_size(rems(0.8125))
                    .text_color(theme.accent_yellow)
                    .child("Git LFS is not installed. Install it with: git lfs install"),
            );
        }

        // Tracked patterns
        let mut patterns_card = div()
            .p_4()
            .rounded_md()
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(rems(0.875))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child("Tracked Patterns"),
            );

        if lfs.tracked_patterns.is_empty() {
            patterns_card = patterns_card.child(
                div()
                    .text_size(rems(0.8125))
                    .text_color(theme.text_muted)
                    .child("No patterns tracked"),
            );
        } else {
            for pat in &lfs.tracked_patterns {
                patterns_card = patterns_card.child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .text_size(rems(0.8125))
                                .text_color(theme.text_primary)
                                .child(pat.clone()),
                        ),
                );
            }
        }

        // Add pattern input
        patterns_card = patterns_card.child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .mt_2()
                .items_center()
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(theme.bg_primary)
                        .border_1()
                        .border_color(theme.border)
                        .text_size(rems(0.8125))
                        .text_color(theme.text_primary)
                        .min_w(px(200.))
                        .child(if lfs.new_pattern.is_empty() {
                            "*.ext".to_string()
                        } else {
                            lfs.new_pattern.clone()
                        }),
                )
                .child(
                    div()
                        .id("lfs-track-btn")
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(theme.accent_cyan)
                        .text_color(theme.text_on_accent)
                        .text_size(rems(0.8125))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, |_e, w, cx| {
                            w.dispatch_action(Box::new(ReviewLfsTrack), cx);
                        })
                        .child("Track"),
                ),
        );
        content = content.child(patterns_card);

        // LFS files
        if !lfs.lfs_files.is_empty() {
            let mut files_card = div()
                .p_4()
                .rounded_md()
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(rems(0.875))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child(format!("LFS Files ({})", lfs.lfs_files.len())),
                );

            for f in &lfs.lfs_files {
                files_card = files_card.child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .py_1()
                        .child(
                            div()
                                .text_size(rems(0.8125))
                                .text_color(theme.text_primary)
                                .child(f.path.clone()),
                        )
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .text_color(theme.text_muted)
                                .child(f.size.clone()),
                        ),
                );
            }
            content = content.child(files_card);
        }

        // Pull/Push buttons
        content = content.child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(
                    div()
                        .id("lfs-pull-btn")
                        .px_4()
                        .py_2()
                        .rounded_md()
                        .bg(theme.accent_cyan)
                        .text_color(theme.text_on_accent)
                        .text_size(rems(0.8125))
                        .cursor_pointer()
                        .when(lfs.lfs_pull_in_progress, |el| el.opacity(0.5))
                        .on_mouse_down(MouseButton::Left, |_e, w, cx| {
                            w.dispatch_action(Box::new(ReviewLfsPull), cx);
                        })
                        .child(if lfs.lfs_pull_in_progress {
                            "Pulling..."
                        } else {
                            "LFS Pull"
                        }),
                )
                .child(
                    div()
                        .id("lfs-push-btn")
                        .px_4()
                        .py_2()
                        .rounded_md()
                        .bg(theme.bg_tertiary)
                        .text_color(theme.text_primary)
                        .text_size(rems(0.8125))
                        .cursor_pointer()
                        .when(lfs.lfs_push_in_progress, |el| el.opacity(0.5))
                        .on_mouse_down(MouseButton::Left, |_e, w, cx| {
                            w.dispatch_action(Box::new(ReviewLfsPush), cx);
                        })
                        .child(if lfs.lfs_push_in_progress {
                            "Pushing..."
                        } else {
                            "LFS Push"
                        }),
                ),
        );

        content
    }

    // ------------------------------------------------------------------
    // Gitflow tab
    // ------------------------------------------------------------------

    fn render_gitflow_tab(data: &ReviewData, theme: &HiveTheme) -> Div {
        let gf = &data.gitflow_data;
        let mut content = div().flex().flex_col().gap_4();

        if !gf.initialized {
            return content.child(
                div()
                    .p_4()
                    .rounded_md()
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_size(rems(0.875))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child("Gitflow not initialized"),
                    )
                    .child(
                        div()
                            .text_size(rems(0.8125))
                            .text_color(theme.text_muted)
                            .child(
                                "Initialize Gitflow to use feature/release/hotfix branching.",
                            ),
                    )
                    .child(
                        div()
                            .id("gitflow-init-btn")
                            .px_4()
                            .py_2()
                            .rounded_md()
                            .bg(theme.accent_cyan)
                            .text_color(theme.text_on_accent)
                            .text_size(rems(0.8125))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, |_e, w, cx| {
                                w.dispatch_action(Box::new(ReviewGitflowInit), cx);
                            })
                            .child("Initialize Gitflow"),
                    ),
            );
        }

        // Config
        content = content.child(
            div()
                .p_4()
                .rounded_md()
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(rems(0.875))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child("Gitflow Configuration"),
                )
                .child(
                    div()
                        .text_size(rems(0.8125))
                        .text_color(theme.text_muted)
                        .child(format!(
                            "Main: {}  |  Develop: {}",
                            gf.main_branch, gf.develop_branch
                        )),
                )
                .child(
                    div()
                        .text_size(rems(0.8125))
                        .text_color(theme.text_muted)
                        .child(format!(
                            "Prefixes: feature={}, release={}, hotfix={}",
                            gf.feature_prefix, gf.release_prefix, gf.hotfix_prefix
                        )),
                ),
        );

        // Active branches
        let sections = [
            ("Features", &gf.active_features, "feature"),
            ("Releases", &gf.active_releases, "release"),
            ("Hotfixes", &gf.active_hotfixes, "hotfix"),
        ];

        for (label, branches, kind) in sections {
            let mut section = div()
                .p_4()
                .rounded_md()
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(rems(0.875))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child(format!("Active {} ({})", label, branches.len())),
                );

            if branches.is_empty() {
                section = section.child(
                    div()
                        .text_size(rems(0.8125))
                        .text_color(theme.text_muted)
                        .child("(none)"),
                );
            } else {
                for name in branches {
                    let finish_kind = kind.to_string();
                    let finish_name = name.clone();
                    section = section.child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .items_center()
                            .py_1()
                            .child(
                                div()
                                    .text_size(rems(0.8125))
                                    .text_color(theme.text_primary)
                                    .child(name.clone()),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "finish-{kind}-{name}"
                                    )))
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(theme.accent_green)
                                    .text_color(theme.text_on_accent)
                                    .text_size(rems(0.75))
                                    .cursor_pointer()
                                    .on_mouse_down(MouseButton::Left, {
                                        move |_e, w, cx| {
                                            w.dispatch_action(
                                                Box::new(ReviewGitflowFinishNamed {
                                                    kind: finish_kind.clone(),
                                                    name: finish_name.clone(),
                                                }),
                                                cx,
                                            );
                                        }
                                    })
                                    .child("Finish"),
                            ),
                    );
                }
            }
            content = content.child(section);
        }

        // Start new
        content = content.child(
            div()
                .p_4()
                .rounded_md()
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_size(rems(0.875))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child("Start New"),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .text_size(rems(0.8125))
                                .text_color(theme.text_muted)
                                .child("Name:"),
                        )
                        .child(
                            div()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.bg_primary)
                                .border_1()
                                .border_color(theme.border)
                                .text_size(rems(0.8125))
                                .text_color(theme.text_primary)
                                .min_w(px(200.))
                                .child(if gf.new_name.is_empty() {
                                    "(enter name)".to_string()
                                } else {
                                    gf.new_name.clone()
                                }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(
                            div()
                                .id("gf-feature-btn")
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.accent_cyan)
                                .text_color(theme.text_on_accent)
                                .text_size(rems(0.8125))
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, {
                                    let name = gf.new_name.clone();
                                    move |_e, w, cx| {
                                        w.dispatch_action(
                                            Box::new(ReviewGitflowStart {
                                                kind: "feature".to_string(),
                                                name: name.clone(),
                                            }),
                                            cx,
                                        );
                                    }
                                })
                                .child("Feature"),
                        )
                        .child(
                            div()
                                .id("gf-release-btn")
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.accent_green)
                                .text_color(theme.text_on_accent)
                                .text_size(rems(0.8125))
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, {
                                    let name = gf.new_name.clone();
                                    move |_e, w, cx| {
                                        w.dispatch_action(
                                            Box::new(ReviewGitflowStart {
                                                kind: "release".to_string(),
                                                name: name.clone(),
                                            }),
                                            cx,
                                        );
                                    }
                                })
                                .child("Release"),
                        )
                        .child(
                            div()
                                .id("gf-hotfix-btn")
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.accent_red)
                                .text_color(theme.text_on_accent)
                                .text_size(rems(0.8125))
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, {
                                    let name = gf.new_name.clone();
                                    move |_e, w, cx| {
                                        w.dispatch_action(
                                            Box::new(ReviewGitflowStart {
                                                kind: "hotfix".to_string(),
                                                name: name.clone(),
                                            }),
                                            cx,
                                        );
                                    }
                                })
                                .child("Hotfix"),
                        ),
                ),
        );

        content
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn action_btn(id: &'static str, label: &str, color: Hsla, theme: &HiveTheme) -> Stateful<Div> {
        div()
            .id(id)
            .px(theme.space_3)
            .py(theme.space_2)
            .rounded(theme.radius_sm)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .text_size(theme.font_size_sm)
            .text_color(color)
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(theme.bg_tertiary))
            .child(label.to_string())
    }
}
