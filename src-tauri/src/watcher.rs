use crate::models::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};
pub struct FolderWatcher {
    _watcher: RecommendedWatcher,
    events: Receiver<bool>,
    pub folder: String,
}
impl FolderWatcher {
    pub fn new(folder: &str) -> Result<Self> {
        // One pending rescan is enough. Continuous filesystem traffic must not keep drain() running forever.
        let (tx, rx) = mpsc::sync_channel(1);
        let mut w = notify::recommended_watcher(move |e: notify::Result<notify::Event>| {
            enqueue_event(e, &tx);
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
        self.events.try_recv().is_ok()
    }
    pub fn drain(&self) {
        let _ = self.events.try_recv();
    }
}
fn enqueue_event(event: notify::Result<notify::Event>, tx: &SyncSender<bool>) {
    let relevant = event.map_or(true, |e| {
        !matches!(e.kind, notify::EventKind::Access(_))
            && e.paths.iter().any(|p| {
                !p.components()
                    .any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case(".git"))
            })
    });
    if relevant {
        let _ = tx.try_send(true);
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn watcher_storm_coalesces_without_blocking_and_filters_git_and_reads() {
        let (tx, rx) = mpsc::sync_channel(1);
        let edit = notify::Event::new(notify::EventKind::Any).add_path("checkout/file.txt".into());
        for _ in 0..100_000 {
            enqueue_event(Ok(edit.clone()), &tx);
        }
        assert!(rx.try_recv().unwrap());
        assert!(rx.try_recv().is_err());
        enqueue_event(
            Ok(notify::Event::new(notify::EventKind::Any).add_path("checkout/.GiT/index".into())),
            &tx,
        );
        enqueue_event(
            Ok(
                notify::Event::new(notify::EventKind::Access(notify::event::AccessKind::Read))
                    .add_path("checkout/file.txt".into()),
            ),
            &tx,
        );
        assert!(rx.try_recv().is_err());
        enqueue_event(Ok(edit), &tx);
        assert!(rx.try_recv().unwrap());
        enqueue_event(Err(notify::Error::generic("watcher needs rescan")), &tx);
        assert!(rx.try_recv().unwrap());
    }
}
