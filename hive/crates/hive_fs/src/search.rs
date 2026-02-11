use anyhow::{Context, Result};
use ignore::WalkBuilder;
use regex::RegexBuilder;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Options controlling how a search is performed.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub max_results: usize,
    pub include_hidden: bool,
    pub file_pattern: Option<String>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: true,
            max_results: 100,
            include_hidden: false,
            file_pattern: None,
        }
    }
}

/// A single search match with location and context.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: PathBuf,
    pub line_number: usize,
    pub line_content: String,
    pub match_start: usize,
    pub match_end: usize,
}

/// File content search service using regex and gitignore-aware traversal.
pub struct SearchService;

impl SearchService {
    /// Search for a regex pattern across files under `root`.
    ///
    /// Respects `.gitignore` rules and supports glob-based file filtering.
    pub fn search(root: &Path, pattern: &str, options: SearchOptions) -> Result<Vec<SearchResult>> {
        let regex = RegexBuilder::new(pattern)
            .case_insensitive(!options.case_sensitive)
            .build()
            .with_context(|| format!("Invalid search pattern: {pattern}"))?;

        let glob_matcher = match &options.file_pattern {
            Some(glob) => Some(
                glob::Pattern::new(glob)
                    .with_context(|| format!("Invalid file glob pattern: {glob}"))?,
            ),
            None => None,
        };

        let walker = WalkBuilder::new(root)
            .hidden(!options.include_hidden)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        let mut results = Vec::new();

        debug!("Searching for '{}' in {}", pattern, root.display());

        for entry in walker {
            if results.len() >= options.max_results {
                break;
            }

            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Skip directories
            let entry_path = entry.path();
            if entry_path.is_dir() {
                continue;
            }

            // Apply file glob filter
            if let Some(ref glob) = glob_matcher {
                let file_name = match entry_path.file_name() {
                    Some(name) => name.to_string_lossy(),
                    None => continue,
                };
                if !glob.matches(&file_name) {
                    continue;
                }
            }

            // Skip binary files by checking the first 512 bytes
            if is_likely_binary(entry_path) {
                continue;
            }

            let content = match fs::read_to_string(entry_path) {
                Ok(c) => c,
                Err(_) => continue, // Skip files that can't be read as UTF-8
            };

            for (line_idx, line) in content.lines().enumerate() {
                if results.len() >= options.max_results {
                    break;
                }

                if let Some(m) = regex.find(line) {
                    results.push(SearchResult {
                        path: entry_path.to_path_buf(),
                        line_number: line_idx + 1,
                        line_content: line.to_string(),
                        match_start: m.start(),
                        match_end: m.end(),
                    });
                }
            }
        }

        debug!("Found {} results", results.len());
        Ok(results)
    }
}

/// Heuristic: read the first 512 bytes and check for null bytes.
fn is_likely_binary(path: &Path) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    use std::io::Read;
    let mut buf = [0u8; 512];
    let mut reader = std::io::BufReader::new(file);
    let Ok(n) = reader.read(&mut buf) else {
        return false;
    };
    buf[..n].contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_search_dir() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hello.rs"), "fn main() {\n    println!(\"Hello\");\n}\n")
            .unwrap();
        fs::write(
            dir.path().join("world.rs"),
            "fn greet() {\n    println!(\"World\");\n}\n",
        )
        .unwrap();
        fs::write(dir.path().join("notes.txt"), "This is a note\nWith multiple lines\n").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(
            dir.path().join("sub").join("deep.rs"),
            "fn deep() { /* deep */ }\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn test_basic_search() {
        let dir = setup_search_dir();
        let results = SearchService::search(dir.path(), "println", SearchOptions::default()).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_case_insensitive_search() {
        let dir = setup_search_dir();
        let opts = SearchOptions {
            case_sensitive: false,
            ..Default::default()
        };
        let results = SearchService::search(dir.path(), "hello", opts).unwrap();
        assert!(results.len() >= 1);
    }

    #[test]
    fn test_file_pattern_filter() {
        let dir = setup_search_dir();
        let opts = SearchOptions {
            file_pattern: Some("*.txt".to_string()),
            ..Default::default()
        };
        let results = SearchService::search(dir.path(), "note", opts).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].path.to_string_lossy().ends_with("notes.txt"));
    }

    #[test]
    fn test_max_results() {
        let dir = setup_search_dir();
        let opts = SearchOptions {
            max_results: 1,
            ..Default::default()
        };
        let results = SearchService::search(dir.path(), "fn", opts).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_result_positions() {
        let dir = setup_search_dir();
        let results = SearchService::search(dir.path(), "main", SearchOptions::default()).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].match_start < results[0].match_end);
        assert_eq!(results[0].line_number, 1);
    }

    #[test]
    fn test_invalid_regex() {
        let dir = setup_search_dir();
        let result = SearchService::search(dir.path(), "[invalid", SearchOptions::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_search_subdirectories() {
        let dir = setup_search_dir();
        let results = SearchService::search(dir.path(), "deep", SearchOptions::default()).unwrap();
        assert!(results.len() >= 1);
        assert!(results.iter().any(|r| r.path.to_string_lossy().contains("deep.rs")));
    }
}
