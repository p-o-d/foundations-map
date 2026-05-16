//! File-system watcher for the X4 save directory.
//!
//! Debounces rapid modify events (X4 may write the .xml.gz over a few hundred ms);
//! reports each settled change as one message on a sender channel.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use notify::{Config, Event, EventKind, RecursiveMode, Watcher};

pub use notify::RecommendedWatcher;

const DEBOUNCE: Duration = Duration::from_millis(1000);

pub enum WatcherEvent {
    SaveChanged(PathBuf),
}

/// Spawn a watcher on `dir`. Returns the watcher handle (drop = stop) plus a thread
/// handle. Send `WatcherEvent`s on `tx`. Filters to `.xml.gz` files; debounces by
/// `DEBOUNCE`; emits the most recently modified `.xml.gz` in the directory after
/// the quiet window.
pub fn watch_save_dir(dir: &Path, tx: Sender<WatcherEvent>) -> notify::Result<RecommendedWatcher> {
    let dir_owned = dir.to_path_buf();
    // notify::recommended_watcher takes a closure that runs on the watcher's thread.
    // Forward events into a second channel; a debounce thread consumes them.
    let (raw_tx, raw_rx) = std::sync::mpsc::channel::<notify::Result<Event>>();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = raw_tx.send(res);
    })?;
    watcher.configure(Config::default())?;
    watcher.watch(dir, RecursiveMode::NonRecursive)?;

    std::thread::spawn(move || {
        let mut pending_until: Option<Instant> = None;
        loop {
            // Block with a timeout so we can fire the debounced event when quiet.
            let timeout = pending_until
                .map(|t| t.saturating_duration_since(Instant::now()))
                .unwrap_or(Duration::from_secs(60));
            match raw_rx.recv_timeout(timeout) {
                Ok(Ok(event)) => {
                    if relevant(&event) {
                        pending_until = Some(Instant::now() + DEBOUNCE);
                    }
                }
                Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if pending_until.take().is_some() {
                        if let Some(path) = latest_xml_gz(&dir_owned) {
                            let _ = tx.send(WatcherEvent::SaveChanged(path));
                        }
                    }
                }
            }
        }
    });

    Ok(watcher)
}

fn relevant(e: &Event) -> bool {
    matches!(
        e.kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    ) && e
        .paths
        .iter()
        .any(|p| p.extension().map(|x| x == "gz").unwrap_or(false))
}

fn latest_xml_gz(dir: &Path) -> Option<PathBuf> {
    let mut latest: Option<(std::time::SystemTime, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()) {
        let p = e.path();
        let name = p.file_name()?.to_str()?.to_string();
        if !name.ends_with(".xml.gz") {
            continue;
        }
        let mtime = e
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::UNIX_EPOCH);
        if latest.as_ref().map_or(true, |(t, _)| mtime > *t) {
            latest = Some((mtime, p));
        }
    }
    latest.map(|(_, p)| p)
}
