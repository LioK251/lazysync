use crate::models::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
pub struct FolderWatcher {
    _watcher: RecommendedWatcher,
    events: Receiver<notify::Result<notify::Event>>,
    pub folder: String,
}
impl FolderWatcher {
    pub fn new(folder: &str) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let mut w = notify::recommended_watcher(move |e| {
            let _ = tx.send(e);
        })
        .map_err(|_| {
            AppError::new(
                "watcher",
                "Cannot watch this folder.",
                "Check folder access.",
            )
        })?;
        w.watch(Path::new(folder), RecursiveMode::Recursive)
            .map_err(|_| {
                AppError::new(
                    "watcher",
                    "Cannot watch this folder.",
                    "Check folder access.",
                )
            })?;
        Ok(Self {
            _watcher: w,
            events: rx,
            folder: folder.into(),
        })
    }
    pub fn changed(&self) -> bool {
        let mut changed = false;
        for e in self.events.try_iter().flatten() {
            if !matches!(e.kind, notify::EventKind::Access(_))
                && e.paths
                    .iter()
                    .any(|p| !p.components().any(|c| c.as_os_str() == ".git"))
            {
                changed = true;
            }
        }
        changed
    }
    pub fn drain(&self) {
        for _ in self.events.try_iter() {}
    }
}
pub fn automation_due(mode: &Automation, last_change: Option<Instant>, now: Instant) -> bool {
    last_change.is_some_and(|last| {
        now.duration_since(last)
            >= match mode {
                Automation::AfterChanges => Duration::from_secs(2),
                Automation::FolderIdle => Duration::from_secs(30),
                Automation::Off => Duration::MAX,
            }
    })
}
