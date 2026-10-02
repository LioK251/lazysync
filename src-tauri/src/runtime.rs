use crate::{
    credentials::CredentialStore,
    git,
    github::GitHub,
    models::*,
    storage::Storage,
    sync::{self, SyncEngine},
    watcher::{automation_due, FolderWatcher},
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, RwLock,
    },
    time::{Duration, Instant},
};

pub struct Service {
    pub storage: Storage,
    pub settings: Settings,
    pub github: GitHub,
    pub vault: Arc<dyn CredentialStore>,
    pub status: StatusSnapshot,
    emit: Arc<dyn Fn(StatusSnapshot) + Send + Sync>,
    watcher: Option<FolderWatcher>,
    observed: String,
    last_change: Option<Instant>,
    last_scan: Instant,
    last_fetch: Instant,
    last_watch_event: Option<Instant>,
    auto_paused: bool,
}
impl Service {
    pub fn new(
        storage: Storage,
        vault: Arc<dyn CredentialStore>,
        github: GitHub,
        emit: Arc<dyn Fn(StatusSnapshot) + Send + Sync>,
    ) -> Result<Self> {
        let settings = storage.load_settings()?;
        let recovery = storage.journal()?.is_some();
        let mut s = Self {
            storage,
            settings,
            github,
            vault,
            status: StatusSnapshot::default(),
            emit,
            watcher: None,
            observed: String::new(),
            last_change: None,
            last_scan: Instant::now() - Duration::from_secs(10),
            last_fetch: Instant::now() - Duration::from_secs(60),
            last_watch_event: None,
            auto_paused: recovery,
        };
        if let Err(e) = s.scan() {
            s.status.error = Some(e);
            s.status.phase = "paused".into();
            s.status.label = "Repository unavailable".into();
            s.auto_paused = true;
            s.publish();
        }
        Ok(s)
    }
    pub fn active(&self) -> Result<RepositoryMapping> {
        self.settings
            .repositories
            .iter()
            .find(|m| Some(&m.remote.id) == self.settings.active_id.as_ref())
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    "selection",
                    "Choose a repository first.",
                    "Open the repository picker.",
                )
            })
    }
    pub fn publish(&mut self) {
        self.status.sequence = self.status.sequence.wrapping_add(1);
        (self.emit)(self.status.clone());
    }
    pub fn scan(&mut self) -> Result<()> {
        let recovery = self.storage.journal()?;
        self.status.recovery = recovery.is_some();
        if let Ok(m) = self.active() {
            let r = git::open(&m.folder)?;
            self.status.repository_id = Some(m.remote.id.clone());
            self.status.last_sync = m.last_sync.clone();
            self.status.changes = git::counts(&r)?;
            let (ahead, behind) = git::ahead_behind(&r, &m.branch)?;
            self.status.ahead = ahead;
            self.status.behind = behind;
            if recovery.is_some() {
                self.status.phase = "paused".into();
                self.status.label = "Recovery needs attention".into();
            } else if let Err(e) = git::preflight(&r, &m, false) {
                self.auto_paused = true;
                self.status.phase = "paused".into();
                self.status.label = "Sync paused".into();
                self.status.error = Some(e);
            } else if self.status.error.is_none() {
                self.status.phase = "idle".into();
                self.status.label = if self.status.changes.added
                    + self.status.changes.modified
                    + self.status.changes.deleted
                    + ahead
                    + behind
                    == 0
                {
                    "All changes synced"
                } else {
                    "Ready to sync"
                }
                .into();
            }
        }
        self.last_scan = Instant::now();
        self.publish();
        Ok(())
    }
    fn mutable(&self) -> Result<()> {
        if self.storage.journal()?.is_some() {
            return Err(AppError::new(
                "recovery",
                "Finish recovery before changing repositories or settings.",
                "Review Recovery.",
            ));
        }
        Ok(())
    }
    pub fn authenticate(&mut self, token: String) -> Result<Identity> {
        let token = zeroize::Zeroizing::new(token);
        let identity = self.github.identity(token.trim())?;
        self.vault.set(token.trim())?;
        if self.settings.commit_name.is_empty() {
            self.settings.commit_name = identity.name.clone();
        }
        if self.settings.commit_email.is_empty() {
            self.settings.commit_email = identity.email.clone();
        }
        self.storage.save_settings(&self.settings)?;
        self.status.error = None;
        self.status.connectivity = "online".into();
        self.publish();
        Ok(identity)
    }
    pub fn save_settings(
        &mut self,
        device_name: String,
        commit_name: String,
        commit_email: String,
        automation: Automation,
    ) -> Result<Settings> {
        self.mutable()?;
        if device_name.trim().is_empty()
            || commit_name.trim().is_empty()
            || !commit_email.contains('@')
            || [
                device_name.as_str(),
                commit_name.as_str(),
                commit_email.as_str(),
            ]
            .iter()
            .any(|s| s.len() > 200 || s.contains(['\n', '\r', '\0']))
        {
            return Err(AppError::new(
                "identity",
                "Enter a valid device name, name, and email.",
                "Check Settings.",
            ));
        }
        let mut next = self.settings.clone();
        next.device_name = device_name.trim().into();
        next.commit_name = commit_name.trim().into();
        next.commit_email = commit_email.trim().into();
        next.automation = automation;
        self.storage.save_settings(&next)?;
        self.settings = next;
        self.auto_paused = false;
        self.status.error = None;
        self.last_change = Some(Instant::now());
        self.scan()?;
        Ok(self.settings.clone())
    }
    pub fn list(&self, page: u32) -> Result<Vec<RemoteRepository>> {
        self.github.repositories(&self.vault.get()?, page)
    }
    fn verified(&self, selected: &RemoteRepository) -> Result<RemoteRepository> {
        let r = self
            .github
            .repository(&self.vault.get()?, &selected.full_name)?;
        if r.id != selected.id || !r.writable {
            return Err(AppError::new(
                "permission",
                "This token does not have write access to the selected repository.",
                "Update repository access and Contents read/write permissions.",
            ));
        }
        Ok(r)
    }
    fn remember(&mut self, m: RepositoryMapping) -> Result<Settings> {
        let mut next = self.settings.clone();
        next.repositories.retain(|old| old.remote.id != m.remote.id);
        next.active_id = Some(m.remote.id.clone());
        next.repositories.push(m);
        next.pending_setup = None;
        self.storage.save_settings(&next)?;
        self.settings = next;
        self.watcher = None;
        self.observed.clear();
        self.last_change = None;
        self.last_watch_event = None;
        self.last_fetch = Instant::now() - Duration::from_secs(60);
        self.auto_paused = false;
        self.status.error = None;
        self.scan()?;
        Ok(self.settings.clone())
    }
    pub fn connect(
        &mut self,
        remote: RemoteRepository,
        folder: String,
        clone: bool,
    ) -> Result<Settings> {
        self.mutable()?;
        let remote = self.verified(&remote)?;
        self.check_folder(&folder)?;
        let m = if clone {
            git::clone_repo(&remote, &folder, &self.vault.get()?)?
        } else {
            git::attach(&remote, &folder, false)?
        };
        self.remember(m)
    }
    fn check_folder(&self, folder: &str) -> Result<()> {
        if self
            .settings
            .repositories
            .iter()
            .any(|m| m.folder == folder)
        {
            return Err(AppError::new(
                "folderMapped",
                "This folder is already connected.",
                "Select its remembered repository instead.",
            ));
        }
        Ok(())
    }
    pub fn select(&mut self, id: String) -> Result<Settings> {
        self.mutable()?;
        if !self.settings.repositories.iter().any(|m| m.remote.id == id) {
            return Err(AppError::new(
                "selection",
                "Unknown remembered repository.",
                "Connect it first.",
            ));
        }
        let mut next = self.settings.clone();
        next.active_id = Some(id);
        self.storage.save_settings(&next)?;
        self.settings = next;
        self.watcher = None;
        self.observed.clear();
        self.last_change = None;
        self.last_watch_event = None;
        self.last_fetch = Instant::now() - Duration::from_secs(60);
        self.status.error = None;
        self.auto_paused = false;
        self.scan()?;
        Ok(self.settings.clone())
    }
    pub fn create(
        &mut self,
        name: String,
        description: String,
        folder: String,
    ) -> Result<Settings> {
        self.create_with_ignore(name, description, folder, vec![])
    }
    pub fn create_with_ignore(
        &mut self,
        name: String,
        description: String,
        folder: String,
        ignore_patterns: Vec<String>,
    ) -> Result<Settings> {
        self.mutable()?;
        self.check_folder(&folder)?;
        crate::ignore::validate(&ignore_patterns)?;
        let token = self.vault.get()?;
        match self.settings.pending_setup.clone() {
            None => {
                self.settings.pending_setup = Some(PendingSetup {
                    name: name.clone(),
                    description: description.clone(),
                    folder: folder.clone(),
                    remote: None,
                    ignore_patterns,
                });
                self.storage.save_settings(&self.settings)?;
                let remote = self.github.create_private(&token, &name, &description)?;
                self.settings.pending_setup.as_mut().unwrap().remote = Some(remote);
                self.storage.save_settings(&self.settings)?;
            }
            Some(p) => {
                if p.name != name || p.folder != folder {
                    return Err(AppError::new(
                        "pendingSetup",
                        "A previous private repository setup is unfinished.",
                        "Resume its setup from the Create Repository dialog.",
                    ));
                }
                // Reconcile a possibly accepted POST using GET; never repeat an uncertain creation.
                if p.remote.is_none() {
                    let login = self.github.identity(&token)?.login;
                    let remote = self.github.repository(&token, &format!("{login}/{name}"))?;
                    if !remote.private {
                        return Err(AppError::new(
                            "pendingSetup",
                            "The matching repository is public.",
                            "Review it on GitHub; connect deliberately through the picker.",
                        ));
                    }
                    self.settings.pending_setup.as_mut().unwrap().remote = Some(remote);
                    self.storage.save_settings(&self.settings)?;
                }
            }
        }
        let remote = self
            .settings
            .pending_setup
            .as_ref()
            .unwrap()
            .remote
            .clone()
            .unwrap();
        let remote = self.verified(&remote)?;
        let mapping = git::attach(&remote, &folder, true)?;
        let patterns = self
            .settings
            .pending_setup
            .as_ref()
            .unwrap()
            .ignore_patterns
            .clone();
        if !patterns.is_empty() {
            let root = std::path::Path::new(&folder);
            let previous = crate::ignore::settings(root)?;
            let mut combined = previous.patterns;
            for pattern in patterns {
                if !combined.contains(&pattern) {
                    combined.push(pattern);
                }
            }
            crate::ignore::save(root, combined, &previous.revision)?;
        }
        self.remember(mapping)
    }
    pub fn comparison(&mut self) -> Result<ComparisonList> {
        self.mutable()?;
        let mapping = self.active()?;
        let r = git::open(&mapping.folder)?;
        git::preflight(&r, &mapping, false)?;
        git::fetch(&r, &mapping.branch, &self.vault.get()?)?;
        self.last_fetch = Instant::now();
        self.status.connectivity = "online".into();
        self.scan()?;
        git::comparison_files(&r, &mapping.branch)
    }
    pub fn save_ignores(
        &mut self,
        patterns: Vec<String>,
        revision: String,
    ) -> Result<IgnoreSettings> {
        self.mutable()?;
        let mapping = self.active()?;
        let result =
            crate::ignore::save(std::path::Path::new(&mapping.folder), patterns, &revision)?;
        if self
            .status
            .error
            .as_ref()
            .is_some_and(|e| e.code == "largeFiles")
        {
            self.status.error = None;
            self.auto_paused = false;
        }
        self.scan()?;
        Ok(result)
    }
    pub fn abandon_setup(&mut self) -> Result<Settings> {
        self.mutable()?;
        self.settings.pending_setup = None;
        self.storage.save_settings(&self.settings)?;
        Ok(self.settings.clone())
    }
    pub fn reconfirm_branch(&mut self) -> Result<Settings> {
        self.mutable()?;
        let m = self.active()?;
        let r = git::open(&m.folder)?;
        let b = git::branch(&r)?;
        let mut updated = m;
        updated.branch = b;
        git::preflight(&r, &updated, false)?;
        self.remember(updated)
    }
    pub fn sync(&mut self, resume: bool) -> Result<StatusSnapshot> {
        self.status.phase = "validating".into();
        self.status.label = "Checking repository access".into();
        self.publish();
        let result = self.sync_inner(resume);
        if let Err(e) = &result {
            self.auto_paused = true;
            self.status.error = Some(e.clone());
            self.status.phase = "paused".into();
            self.status.label = "Sync needs attention".into();
            if ["network", "git", "authentication"].contains(&e.code.as_str()) {
                self.status.connectivity = "unavailable".into();
            }
            if self.scan().is_err() {
                self.publish();
            }
        }
        result
    }
    fn sync_inner(&mut self, resume: bool) -> Result<StatusSnapshot> {
        let m = self.active()?;
        let token = self.vault.get()?;
        // Validate remote access before preserving or modifying local files.
        self.verified(&m.remote)?;
        let engine = SyncEngine {
            storage: &self.storage,
            settings: &self.settings,
            token: &token,
        };
        let mut status = self.status.clone();
        let emit = self.emit.clone();
        let mut progress = |phase: &str| {
            status.sequence = status.sequence.wrapping_add(1);
            let (stage, detail) = phase
                .split_once(':')
                .map_or((phase, None), |(stage, detail)| (stage, Some(detail)));
            status.phase = stage.into();
            status.label = detail
                .unwrap_or(match stage {
                    "idle" => "All changes synced",
                    "backup" => "Backing up local files",
                    "stash" => "Preserving local changes",
                    "fetch" | "retryFetch" => "Fetching cloud changes",
                    "integrate" | "retryIntegrate" => "Integrating cloud changes",
                    "apply" => "Restoring local changes",
                    "commit" => "Committing local changes",
                    "push" => "Preparing upload",
                    "verify" => "Verifying upload",
                    "cleanup" => "Finishing sync",
                    _ => "Syncing",
                })
                .into();
            status.recovery = phase != "idle";
            (emit)(status.clone());
        };
        let result = if resume {
            engine.resume(&m, &mut progress)
        } else {
            engine.start(&m, &mut progress)
        };
        self.status.sequence = status.sequence;
        if let Some(w) = &self.watcher {
            w.drain();
        }
        self.last_change = None;
        self.observed = git::open(&m.folder)
            .and_then(|r| git::observation_fingerprint(&r))
            .unwrap_or_default();
        match result {
            Ok(()) => {
                let timestamp = chrono::Utc::now().to_rfc3339();
                if let Some(repo) = self
                    .settings
                    .repositories
                    .iter_mut()
                    .find(|r| r.remote.id == m.remote.id)
                {
                    repo.last_sync = Some(timestamp);
                }
                self.storage.save_settings(&self.settings)?;
                self.status.error = None;
                self.status.connectivity = "online".into();
                self.auto_paused = false;
            }
            Err(e) => {
                return Err(e);
            }
        }
        self.scan()?;
        Ok(self.status.clone())
    }
    pub fn tick(&mut self) -> Result<()> {
        self.tick_with_priority(&AtomicBool::new(false))
    }
    fn tick_with_priority(&mut self, manual_pending: &AtomicBool) -> Result<()> {
        let Ok(m) = self.active() else {
            return Ok(());
        };
        if self.watcher.as_ref().is_none_or(|w| w.folder != m.folder) {
            self.watcher = Some(FolderWatcher::new(&m.folder)?);
        }
        let event = self.watcher.as_ref().is_some_and(|w| w.changed());
        let now = Instant::now();
        if event {
            self.last_watch_event = Some(now);
        }
        let due = now.duration_since(self.last_scan)
            >= Duration::from_secs(self.settings.status_interval_secs.max(1) as u64);
        let debounced = self
            .last_watch_event
            .is_some_and(|last| now.duration_since(last) >= Duration::from_secs(2));
        if debounced || due {
            let r = git::open(&m.folder)?;
            if self.settings.automation != Automation::Off {
                let fp = git::observation_fingerprint(&r)?;
                if fp != self.observed {
                    self.observed = fp;
                    if git::dirty(&r)? {
                        self.last_change = Some(self.last_watch_event.unwrap_or(now));
                    }
                }
            }
            self.last_watch_event = None;
            self.scan()?;
        }
        if self.storage.journal()?.is_some() || self.auto_paused {
            return Ok(());
        }
        if manual_pending.load(Ordering::SeqCst) {
            return Ok(());
        }
        if now.duration_since(self.last_fetch)
            >= Duration::from_secs(self.settings.fetch_interval_secs.max(10) as u64)
        {
            self.last_fetch = now;
            let r = git::open(&m.folder)?;
            let result = self.vault.get().and_then(|t| git::fetch(&r, &m.branch, &t));
            match result {
                Ok(()) => {
                    self.status.connectivity = "online".into();
                    self.status.error = None;
                }
                Err(e) => {
                    self.status.connectivity = "unavailable".into();
                    self.status.error = Some(e);
                }
            }
            self.scan()?;
        }
        if !manual_pending.load(Ordering::SeqCst)
            && automation_due(&self.settings.automation, self.last_change, now)
        {
            self.sync(false)?;
        }
        Ok(())
    }
    pub fn conflicts(&self) -> Result<Vec<Conflict>> {
        Ok(self
            .storage
            .journal()?
            .map(|j| j.conflicts)
            .unwrap_or_default())
    }
    pub fn resolve(&mut self, path: String, choice: String) -> Result<Vec<Conflict>> {
        sync::resolve(&self.storage, &path, &choice)?;
        self.conflicts()
    }
    pub fn finish_recovery(&mut self) -> Result<StatusSnapshot> {
        sync::finish_manual_recovery(&self.storage)?;
        self.auto_paused = true;
        self.settings.automation = Automation::Off;
        self.storage.save_settings(&self.settings)?;
        self.status.error = None;
        self.scan()?;
        Ok(self.status.clone())
    }
    fn worker_panic(&mut self) {
        self.auto_paused = true;
        self.status.phase = "paused".into();
        self.status.label = "Operation interrupted".into();
        self.status.recovery = self.storage.journal().ok().flatten().is_some();
        self.status.error = Some(AppError::new(
            "workerPanic",
            "The repository operation was interrupted.",
            "Review recovery before retrying. Your files and backups are preserved.",
        ));
        self.publish();
    }
}

type Job = Box<dyn FnOnce(&mut Service) + Send>;
#[derive(Clone)]
pub struct Worker {
    tx: mpsc::Sender<Job>,
    pub busy: Arc<AtomicBool>,
    status: Arc<RwLock<StatusSnapshot>>,
    settings: Arc<RwLock<Settings>>,
}
struct BusyGuard(Arc<AtomicBool>);
impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
impl Worker {
    pub fn new(mut service: Service) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let busy = Arc::new(AtomicBool::new(false));
        let status = Arc::new(RwLock::new(service.status.clone()));
        let settings = Arc::new(RwLock::new(service.settings.clone()));
        let snapshot = status.clone();
        let emit = service.emit.clone();
        service.emit = Arc::new(move |next| {
            *snapshot.write().unwrap_or_else(|p| p.into_inner()) = next.clone();
            emit(next);
        });
        let worker_settings = settings.clone();
        std::thread::Builder::new()
            .name("lazysync-repository".into())
            .spawn(move || {
                let mut service = service;
                while let Ok(job) = rx.recv() {
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&mut service)))
                        .is_err()
                    {
                        service.worker_panic();
                    }
                    *worker_settings.write().unwrap_or_else(|p| p.into_inner()) =
                        service.settings.clone();
                }
            })
            .expect("repository worker");
        let tick_tx = tx.clone();
        let tick_busy = busy.clone();
        let tick_pending = Arc::new(AtomicBool::new(false));
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            if !tick_busy.load(Ordering::SeqCst)
                && tick_pending
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            {
                let guard = BusyGuard(tick_pending.clone());
                let manual = tick_busy.clone();
                if tick_tx
                    .send(Box::new(move |s| {
                        let _guard = guard;
                        if !manual.load(Ordering::SeqCst) {
                            if let Err(e) = s.tick_with_priority(&manual) {
                                s.status.error = Some(e);
                                s.publish();
                            }
                        }
                    }))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            tx,
            busy,
            status,
            settings,
        }
    }
    pub fn status(&self) -> StatusSnapshot {
        self.status
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
    pub fn settings(&self) -> Settings {
        self.settings
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
    pub fn call<T: Send + 'static>(
        &self,
        mutate: bool,
        f: impl FnOnce(&mut Service) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        if mutate
            && self
                .busy
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            return Err(AppError::new(
                "busy",
                "A repository operation is in progress.",
                "Wait until it completes.",
            ));
        }
        let (tx, rx) = mpsc::channel();
        let guard = mutate.then(|| BusyGuard(self.busy.clone()));
        let settings = self.settings.clone();
        self.tx
            .send(Box::new(move |s| {
                let guard = guard;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(s)))
                    .unwrap_or_else(|_| {
                        s.worker_panic();
                        Err(s.status.error.clone().unwrap())
                    });
                *settings.write().unwrap_or_else(|p| p.into_inner()) = s.settings.clone();
                drop(guard);
                let _ = tx.send(result);
            }))
            .map_err(|_| {
                AppError::new("worker", "Repository worker stopped.", "Restart lazysync.")
            })?;
        rx.recv().map_err(|_| {
            AppError::new("worker", "Repository worker stopped.", "Restart lazysync.")
        })?
    }
}
