use anyhow::{Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Simplified file system event.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

/// Watches a directory for file system changes and invokes a callback.
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    /// Create a new watcher on the given path.
    ///
    /// The callback receives `WatchEvent` variants for creates, modifications,
    /// deletions, and renames. The watcher stays active as long as this struct
    /// is alive.
    pub fn new(path: &Path, callback: impl Fn(WatchEvent) + Send + 'static) -> Result<Self> {
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    for watch_event in translate_event(&event) {
                        callback(watch_event);
                    }
                }
                Err(e) => {
                    warn!("File watcher error: {e}");
                }
            }
        })
        .context("Failed to create file watcher")?;

        watcher
            .watch(path, RecursiveMode::Recursive)
            .with_context(|| format!("Failed to watch path: {}", path.display()))?;

        debug!("Watching for changes: {}", path.display());
        Ok(Self { _watcher: watcher })
    }
}

/// Translate a raw `notify::Event` into zero or more `WatchEvent`s.
fn translate_event(event: &Event) -> Vec<WatchEvent> {
    let paths = &event.paths;

    match &event.kind {
        EventKind::Create(_) => paths
            .iter()
            .map(|p| WatchEvent::Created(p.clone()))
            .collect(),

        EventKind::Modify(modify_kind) => {
            use notify::event::ModifyKind;
            match modify_kind {
                ModifyKind::Name(_) if paths.len() >= 2 => {
                    vec![WatchEvent::Renamed {
                        from: paths[0].clone(),
                        to: paths[1].clone(),
                    }]
                }
                _ => paths
                    .iter()
                    .map(|p| WatchEvent::Modified(p.clone()))
                    .collect(),
            }
        }

        EventKind::Remove(_) => paths
            .iter()
            .map(|p| WatchEvent::Deleted(p.clone()))
            .collect(),

        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn test_watcher_detects_creation() {
        let dir = tempfile::tempdir().unwrap();
        let events: Arc<Mutex<Vec<WatchEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let _watcher = FileWatcher::new(dir.path(), move |event| {
            events_clone.lock().unwrap().push(event);
        })
        .unwrap();

        // Give the watcher time to initialize
        std::thread::sleep(Duration::from_millis(100));

        fs::write(dir.path().join("test.txt"), "hello").unwrap();

        // Wait for events to propagate
        std::thread::sleep(Duration::from_millis(500));

        let captured = events.lock().unwrap();
        assert!(
            !captured.is_empty(),
            "Expected at least one event from file creation"
        );
    }

    #[test]
    fn test_translate_create_event() {
        let event = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/new.txt")],
            attrs: Default::default(),
        };
        let translated = translate_event(&event);
        assert_eq!(translated.len(), 1);
        assert!(matches!(&translated[0], WatchEvent::Created(p) if p == Path::new("/tmp/new.txt")));
    }

    #[test]
    fn test_translate_delete_event() {
        let event = Event {
            kind: EventKind::Remove(notify::event::RemoveKind::File),
            paths: vec![PathBuf::from("/tmp/gone.txt")],
            attrs: Default::default(),
        };
        let translated = translate_event(&event);
        assert_eq!(translated.len(), 1);
        assert!(matches!(&translated[0], WatchEvent::Deleted(_)));
    }

    #[test]
    fn test_translate_rename_event() {
        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Name(
                notify::event::RenameMode::Both,
            )),
            paths: vec![
                PathBuf::from("/tmp/old.txt"),
                PathBuf::from("/tmp/new.txt"),
            ],
            attrs: Default::default(),
        };
        let translated = translate_event(&event);
        assert_eq!(translated.len(), 1);
        assert!(matches!(&translated[0], WatchEvent::Renamed { from, to }
            if from == Path::new("/tmp/old.txt") && to == Path::new("/tmp/new.txt")
        ));
    }
}
