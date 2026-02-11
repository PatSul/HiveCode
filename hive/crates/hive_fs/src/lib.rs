// Phase 3: File operations, search, git

pub mod files;
pub mod git;
pub mod search;
pub mod watcher;

pub use files::{DirEntry, FileService, FileStats};
pub use git::{FileStatusType, GitFileStatus, GitLogEntry, GitService};
pub use search::{SearchOptions, SearchResult, SearchService};
pub use watcher::{FileWatcher, WatchEvent};
