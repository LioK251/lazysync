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
        mpsc, Arc,
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
        self.mutable()?;
        self.check_folder(&folder)?;
        let token = self.vault.get()?;
        match self.settings.pending_setup.clone() {
            None => {
                self.settings.pending_setup = Some(PendingSetup {
                    name: name.clone(),
                    description: description.clone(),
                    folder: folder.clone(),
                    remote: None,
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
        self.remember(git::attach(&remote, &folder, true)?)
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
            status.phase = phase.into();
            status.label = if phase == "idle" {
                "All changes synced".into()
            } else {
                format!("Syncing · {phase}")
            };
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
            .and_then(|r| git::fingerprint(&r))
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
                self.auto_paused = true;
                self.status.error = Some(e.clone());
                self.status.phase = "paused".into();
                self.status.label = "Sync needs attention".into();
                if ["network", "git", "authentication"].contains(&e.code.as_str()) {
                    self.status.connectivity = "unavailable".into();
                }
                let _ = self.scan();
                return Err(e);
            }
        }
        self.scan()?;
        Ok(self.status.clone())
    }
    pub fn tick(&mut self) -> Result<()> {
        let Ok(m) = self.active() else {
            return Ok(());
        };
        if self.watcher.as_ref().is_none_or(|w| w.folder != m.folder) {
            self.watcher = Some(FolderWatcher::new(&m.folder)?);
        }
        let event = self.watcher.as_ref().is_some_and(|w| w.changed());
        let now = Instant::now();
        let due = now.duration_since(self.last_scan)
            >= Duration::from_secs(self.settings.status_interval_secs.max(1) as u64);
        if event || due {
            let r = git::open(&m.folder)?;
            let fp = git::fingerprint(&r)?;
            if fp != self.observed {
                self.observed = fp;
                if git::dirty(&r)? {
                    self.last_change = Some(now);
                }
            } // ignored and sync-generated changes do not restart idle timer
            if due || event {
                self.scan()?;
            }
        }
        if self.storage.journal()?.is_some() || self.auto_paused {
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
        if automation_due(&self.settings.automation, self.last_change, now) {
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
}

type Job = Box<dyn FnOnce(&mut Service) + Send>;
#[derive(Clone)]
pub struct Worker {
    tx: mpsc::Sender<Job>,
    pub busy: Arc<AtomicBool>,
}
impl Worker {
    pub fn new(service: Service) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let busy = Arc::new(AtomicBool::new(false));
        std::thread::Builder::new()
            .name("lazysync-repository".into())
            .spawn(move || {
                let mut service = service;
                while let Ok(job) = rx.recv() {
                    job(&mut service);
                }
            })
            .expect("repository worker");
        let tick_tx = tx.clone();
        let tick_busy = busy.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            if tick_busy
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                let flag = tick_busy.clone();
                if tick_tx
                    .send(Box::new(move |s| {
                        if let Err(e) = s.tick() {
                            s.status.error = Some(e);
                            s.publish();
                        }
                        flag.store(false, Ordering::SeqCst);
                    }))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self { tx, busy }
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
        let busy = self.busy.clone();
        self.tx
            .send(Box::new(move |s| {
                let result = f(s);
                if mutate {
                    busy.store(false, Ordering::SeqCst);
                }
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
