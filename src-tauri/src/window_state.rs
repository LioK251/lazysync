//! Window preferences are independent of the repository worker and its recovery journal.
use crate::{models::Result, storage::atomic_write};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Dimensions {
    pub width: f64,
    pub height: f64,
}
impl Dimensions {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
    fn valid(self) -> bool {
        self.width.is_finite()
            && self.height.is_finite()
            && (1.0..=32768.0).contains(&self.width)
            && (1.0..=32768.0).contains(&self.height)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub version: u32,
    pub compact: Dimensions,
    pub expanded: Dimensions,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            compact: Dimensions::new(340.0, 460.0),
            expanded: Dimensions::new(1120.0, 640.0),
        }
    }
}

#[derive(Default)]
struct Session {
    preferences: Preferences,
    expanded: bool,
    // Physical size expected from startup, monitor fitting, or a mode transition.
    // Intermediate resize events must not become user preferences for the new mode.
    target: Option<(u32, u32)>,
    last: Option<((u32, u32), f64)>,
    area: Option<WorkArea>,
    dirty: bool,
}
impl Session {
    fn observe(&mut self, physical: (u32, u32), scale: f64) -> bool {
        if let Some(target) = self.target {
            if physical == target {
                self.target = None;
                self.last = Some((physical, scale));
            }
            return false;
        }
        if self.last == Some((physical, scale)) {
            return false;
        }
        self.remember(physical, scale)
    }
    fn remember(&mut self, physical: (u32, u32), scale: f64) -> bool {
        if physical.0 == 0 || physical.1 == 0 || !scale.is_finite() || scale <= 0.0 {
            return false;
        }
        let size = Dimensions::new(physical.0 as f64 / scale, physical.1 as f64 / scale);
        if !size.valid() {
            return false;
        }
        self.last = Some((physical, scale));
        let saved = if self.expanded {
            &mut self.preferences.expanded
        } else {
            &mut self.preferences.compact
        };
        if *saved == size {
            return false;
        }
        *saved = size;
        self.dirty = true;
        true
    }
}

pub struct WindowState {
    session: Arc<Mutex<Session>>,
    path: PathBuf,
    changes: mpsc::SyncSender<()>,
}
impl WindowState {
    pub fn load(path: PathBuf, on_error: impl Fn() + Send + 'static) -> Self {
        let preferences = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Preferences>(&bytes).ok())
            .filter(|p| p.version == 1 && p.compact.valid() && p.expanded.valid())
            .unwrap_or_default();
        let session = Arc::new(Mutex::new(Session {
            preferences,
            ..Session::default()
        }));
        let (changes, rx) = mpsc::sync_channel(1);
        let state = session.clone();
        let file = path.clone();
        std::thread::spawn(move || {
            while rx.recv().is_ok() {
                loop {
                    match rx.recv_timeout(Duration::from_millis(300)) {
                        Ok(()) => continue,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if Self::save(&state, &file).is_err() {
                                on_error();
                            }
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            let _ = Self::save(&state, &file);
                            return;
                        }
                    }
                }
            }
        });
        Self {
            session,
            path,
            changes,
        }
    }
    fn save(session: &Mutex<Session>, path: &std::path::Path) -> Result<()> {
        // Serialize all writes and capture the newest sizes, including exit flushes.
        let mut s = session.lock().unwrap();
        if s.dirty {
            atomic_write(path, &serde_json::to_vec_pretty(&s.preferences)?)?;
            s.dirty = false;
        }
        Ok(())
    }
    pub fn flush(&self) -> Result<()> {
        Self::save(&self.session, &self.path)
    }
    pub fn expanded(&self) -> bool {
        self.session.lock().unwrap().expanded
    }
    pub fn desired(&self, expanded: bool) -> Dimensions {
        let s = self.session.lock().unwrap();
        if expanded {
            s.preferences.expanded
        } else {
            s.preferences.compact
        }
    }
    pub fn transition(&self, expanded: bool, target: (u32, u32), current: (u32, u32), scale: f64) {
        let mut s = self.session.lock().unwrap();
        s.expanded = expanded;
        s.target = (target != current).then_some(target);
        s.last = Some((target, scale));
    }
    pub fn scale_changed(&self, scale: f64) -> bool {
        self.session
            .lock()
            .unwrap()
            .last
            .is_some_and(|(_, previous)| previous != scale)
    }
    pub fn area_changed(&self, area: WorkArea) -> bool {
        self.session.lock().unwrap().area != Some(area)
    }
    pub fn set_area(&self, area: WorkArea) {
        self.session.lock().unwrap().area = Some(area);
    }
    pub fn observe(&self, physical: (u32, u32), scale: f64) {
        if self.session.lock().unwrap().observe(physical, scale) {
            let _ = self.changes.try_send(());
        }
    }
    pub fn remember_user_resize(&self, physical: (u32, u32), scale: f64) {
        if self.session.lock().unwrap().remember(physical, scale) {
            let _ = self.changes.try_send(());
        }
    }
}

/// Physical monitor bounds, including taskbars and docks, may have negative origins.
#[derive(Clone, Copy, PartialEq)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}
impl WorkArea {
    pub fn limits(self, expanded: bool) -> (Dimensions, Dimensions) {
        let max = Dimensions::new(
            (self.width.saturating_sub(16)).max(1) as f64 / self.scale,
            (self.height.saturating_sub(16)).max(1) as f64 / self.scale,
        );
        let min = Dimensions::new(
            (if expanded { 900.0_f64 } else { 340.0_f64 }).min(max.width),
            460.0_f64.min(max.height),
        );
        (min, max)
    }
    pub fn fit(self, size: Dimensions, expanded: bool) -> (u32, u32) {
        let (min, max) = self.limits(expanded);
        (
            (size.width.clamp(min.width, max.width) * self.scale)
                .round()
                .max(1.0) as u32,
            (size.height.clamp(min.height, max.height) * self.scale)
                .round()
                .max(1.0) as u32,
        )
    }
    pub fn position(self, x: i32, y: i32, size: (u32, u32)) -> (i32, i32) {
        let left = self.x + 8.min(self.width.saturating_sub(size.0) as i32);
        let top = self.y + 8.min(self.height.saturating_sub(size.1) as i32);
        let right = (self.x as i64 + self.width as i64 - size.0 as i64 - 8).max(left as i64) as i32;
        let bottom =
            (self.y as i64 + self.height as i64 - size.1 as i64 - 8).max(top as i64) as i32;
        (x.clamp(left, right), y.clamp(top, bottom))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_default_corrupt_and_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("window-state.json");
        for bytes in [None, Some(b"broken".as_slice()), Some(br#"{"version":2,"compact":{"width":500,"height":600},"expanded":{"width":1200,"height":800}}"#.as_slice()), Some(br#"{"version":1,"compact":{"width":-1,"height":600},"expanded":{"width":1200,"height":800}}"#.as_slice())] {
            if let Some(bytes) = bytes { std::fs::write(&path, bytes).unwrap(); }
            let state = WindowState::load(path.clone(), || {});
            assert_eq!(state.desired(false), Preferences::default().compact);
            assert!(!state.expanded());
        }
    }
    #[test]
    fn separate_sizes_persist_without_repository_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("window-state.json");
        let s = WindowState::load(path.clone(), || {});
        s.observe((840, 1040), 2.0);
        s.transition(true, (2240, 1280), (840, 1040), 2.0);
        s.observe((840, 1040), 2.0); // late event from old mode
        s.observe((2240, 1280), 2.0); // automatic expansion
        s.observe((2600, 1600), 2.0); // user drag
        s.flush().unwrap();
        let saved = WindowState::load(path, || {});
        assert_eq!(saved.desired(false), Dimensions::new(420.0, 520.0));
        assert_eq!(saved.desired(true), Dimensions::new(1300.0, 800.0));
        assert!(!saved.expanded());
        assert!(!dir.path().join("config.json").exists());
        assert!(!dir.path().join("recovery.json").exists());
    }
    #[test]
    fn programmatic_and_late_events_preserve_both_modes() {
        let mut s = Session {
            target: Some((1120, 640)),
            expanded: true,
            ..Session::default()
        };
        for size in [(900, 460), (340, 460), (1120, 640)] {
            assert!(!s.observe(size, 1.0));
        }
        assert_eq!(s.preferences, Preferences::default());
        assert!(s.observe((1200, 700), 1.0));
        s.target = Some((340, 460));
        s.expanded = false;
        assert!(!s.observe((1200, 700), 1.0));
        assert!(!s.observe((340, 460), 1.0));
        assert!(!s.observe((340, 460), 1.0)); // a hide/exit capture after fitting
        assert_eq!(s.preferences.expanded, Dimensions::new(1200.0, 700.0));
        assert_eq!(s.preferences.compact, Preferences::default().compact);
    }
    #[test]
    fn clamp_to_work_area_at_multiple_scales_and_tiny_monitors() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for (width, height) in [(1920, 1040), (800, 450), (12, 12)] {
                let area = WorkArea {
                    x: -1920,
                    y: -40,
                    width,
                    height,
                    scale,
                };
                for expanded in [false, true] {
                    let size = area.fit(Dimensions::new(4000.0, 2000.0), expanded);
                    let (x, y) = area.position(5000, 5000, size);
                    assert!(size.0 <= width && size.1 <= height);
                    assert!(x >= area.x && y >= area.y);
                    assert!(x as i64 + size.0 as i64 <= area.x as i64 + width as i64);
                    assert!(y as i64 + size.1 as i64 <= area.y as i64 + height as i64);
                }
            }
        }
    }
    #[test]
    fn monitor_clamp_does_not_destroy_remembered_size() {
        let mut s = Session::default();
        assert!(s.observe((600, 800), 1.0));
        s.target = Some((340, 460));
        assert!(!s.observe((340, 460), 1.0));
        assert!(!s.observe((340, 460), 1.0));
        assert_eq!(s.preferences.compact, Dimensions::new(600.0, 800.0));
    }
    #[test]
    fn debounced_save_uses_latest_size_and_flush_is_immediate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("window-state.json");
        let state = WindowState::load(path.clone(), || {});
        for width in 400..450 {
            state.observe((width, 600), 1.0);
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !path.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let saved: Preferences = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.compact, Dimensions::new(449.0, 600.0));
        state.observe((500, 700), 1.0);
        state.flush().unwrap();
        let saved: Preferences = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.compact, Dimensions::new(500.0, 700.0));
    }
    #[test]
    fn keyboard_resize_is_saved_but_generated_events_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("window-state.json");
        let state = WindowState::load(path, || {});
        state.transition(false, (356, 460), (340, 460), 1.0);
        state.remember_user_resize((356, 460), 1.0);
        state.observe((340, 460), 1.0);
        state.observe((356, 460), 1.0);
        assert_eq!(state.desired(false), Dimensions::new(356.0, 460.0));
        state.flush().unwrap();
    }
    #[test]
    fn monitor_scale_and_same_size_transitions_keep_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let state = WindowState::load(dir.path().join("window-state.json"), || {});
        state.transition(false, (340, 460), (340, 460), 1.0);
        assert!(!state.scale_changed(1.0));
        assert!(state.scale_changed(1.5));
        state.transition(false, (510, 690), (510, 690), 1.5);
        state.observe((510, 690), 1.5);
        assert!(!state.scale_changed(1.5));
        assert_eq!(state.desired(false), Preferences::default().compact);
        state.transition(true, (1120, 640), (510, 690), 1.0);
        state.observe((1120, 640), 1.0);
        state.transition(false, (340, 460), (1120, 640), 1.0);
        state.observe((340, 460), 1.0);
        state.flush().unwrap();
        assert_eq!(state.desired(true), Preferences::default().expanded);
    }
}
