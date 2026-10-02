use crate::{
    git,
    models::*,
    storage::{atomic_write, Storage},
};
use git2::{Oid, Repository};
use std::{fs, path::Path};

pub struct SyncEngine<'a> {
    pub storage: &'a Storage,
    pub settings: &'a Settings,
    pub token: &'a str,
}
impl SyncEngine<'_> {
    pub fn start(&self, m: &RepositoryMapping, mut emit: impl FnMut(&str)) -> Result<()> {
        if self.storage.journal()?.is_some() {
            return Err(paused("An interrupted sync needs recovery."));
        }
        let r = git::open(&m.folder)?;
        git::preflight(&r, m, false)?;
        git::validate_upload_files(&r)?;
        self.identity()?;
        let id = uuid::Uuid::new_v4().to_string();
        let j = Recovery {
            version: 1,
            operation_id: id.clone(),
            repository_id: m.remote.id.clone(),
            folder: m.folder.clone(),
            branch: m.branch.clone(),
            phase: "backup".into(),
            original_head: git::head(&r),
            backup_ref: format!("refs/lazysync/backups/{id}/original"),
            stash_oid: None,
            expected: git::fingerprint(&r)?,
            in_flight: false,
            retries: 0,
            conflicts: vec![],
            completed: false,
        };
        self.storage.save_journal(&j)?;
        self.run(m, j, &mut emit)
    }
    pub fn resume(&self, m: &RepositoryMapping, mut emit: impl FnMut(&str)) -> Result<()> {
        let mut j = self
            .storage
            .journal()?
            .ok_or_else(|| paused("No recovery operation exists."))?;
        if j.repository_id != m.remote.id || j.folder != m.folder || j.branch != m.branch {
            return Err(paused("Recovery belongs to a different checkout."));
        }
        let r = git::open(&m.folder)?;
        git::preflight(
            &r,
            m,
            r.path().join("rebase-merge").exists() || r.path().join("rebase-apply").exists(),
        )?;
        if j.conflicts.iter().any(|c| !c.resolved) || r.index()?.has_conflicts() {
            return Err(paused("Resolve every file before continuing."));
        }
        self.identity()?;
        if j.in_flight {
            // A stash can be reconciled by its unique operation marker without guessing.
            if j.phase == "stash" {
                let mut r = git::open(&m.folder)?;
                let mut found = None;
                r.stash_foreach(|_, message, oid| {
                    if message.contains(&j.operation_id) {
                        found = Some(oid.to_string());
                    }
                    true
                })?;
                if let Some(oid) = found {
                    if git::dirty(&r)? || git::head(&r) != j.original_head {
                        return Err(paused("The checkout changed after the saved stash. Review it in your Git client before finishing recovery."));
                    }
                    j.stash_oid = Some(oid);
                    j.phase = "fetch".into();
                    j.expected = git::fingerprint(&r)?;
                    j.in_flight = false;
                    self.storage.save_journal(&j)?;
                }
            }
            if j.in_flight
                && (j.phase == "integrate" || j.phase == "retryIntegrate")
                && !j.conflicts.is_empty()
            {
                self.check(&r, &j)?;
                git::command(
                    Path::new(&j.folder),
                    &["rebase", "--continue"],
                    Some(self.settings),
                )
                .or_else(|e| self.capture_error(&r, &mut j, e))?;
                let next = if j.phase == "retryIntegrate" {
                    "push"
                } else {
                    "apply"
                };
                j.conflicts.clear();
                self.finish_step(&r, &mut j, next)?;
            } else if j.in_flight && j.phase == "apply" && !j.conflicts.is_empty() {
                self.check(&r, &j)?;
                j.conflicts.clear();
                self.finish_step(&r, &mut j, "commit")?;
            } else if j.in_flight
                && ["fetch", "push", "verify", "cleanup", "backup"].contains(&j.phase.as_str())
            {
                // These steps are safe to repeat; push acceptance is independently verified.
                j.in_flight = false;
                self.storage.save_journal(&j)?;
            } else if j.in_flight {
                return Err(AppError::new("ambiguousRecovery", "The app stopped during a local Git change. Automatic replay is paused.", "Inspect the journal, backup references, and saved stash in your Git client. Use Finish Recovery after preserving and reconciling your files."));
            }
        }
        self.run(m, j, &mut emit)
    }
    fn identity(&self) -> Result<()> {
        if self.settings.commit_name.trim().is_empty()
            || !self.settings.commit_email.contains('@')
            || self.settings.device_name.trim().is_empty()
        {
            return Err(AppError::new(
                "identity",
                "Set a device name and commit identity.",
                "Complete Settings before syncing.",
            ));
        }
        Ok(())
    }
    fn check(&self, r: &Repository, j: &Recovery) -> Result<()> {
        if git::fingerprint(r)? != j.expected {
            return Err(AppError::new("externalChange", "Files, the index, or HEAD changed outside this sync.", "Review the changes in your Git client and finish recovery. Automation remains paused."));
        }
        Ok(())
    }
    fn begin_step(&self, r: &Repository, j: &mut Recovery) -> Result<()> {
        self.check(r, j)?;
        j.in_flight = true;
        self.storage.save_journal(j)
    }
    fn finish_step(&self, r: &Repository, j: &mut Recovery, next: &str) -> Result<()> {
        j.expected = git::fingerprint(r)?;
        j.phase = next.into();
        j.in_flight = false;
        self.storage.save_journal(j)
    }
    fn run(
        &self,
        m: &RepositoryMapping,
        mut j: Recovery,
        emit: &mut impl FnMut(&str),
    ) -> Result<()> {
        let mut r = git::open(&m.folder)?;
        loop {
            emit(&j.phase);
            self.begin_step(&r, &mut j)?;
            match j.phase.as_str() {
                "backup" => {
                    if let Some(oid) = &j.original_head {
                        r.reference(
                            &j.backup_ref,
                            Oid::from_str(oid)?,
                            false,
                            "lazysync recovery backup",
                        )
                        .or_else(|e| {
                            if e.code() == git2::ErrorCode::Exists {
                                r.find_reference(&j.backup_ref)
                            } else {
                                Err(e)
                            }
                        })?;
                    }
                    // Preserve raw pending files even on an unborn branch before any Git mutation.
                    let folder = self.storage.conflict_folder(&j)?.join("original-files");
                    let paths = git::command(
                        Path::new(&m.folder),
                        &[
                            "ls-files",
                            "--cached",
                            "--others",
                            "--exclude-standard",
                            "-z",
                        ],
                        None,
                    )?;
                    for p in paths.split(|b| *b == 0).filter(|p| !p.is_empty()) {
                        let name = String::from_utf8_lossy(p);
                        let from = git::safe_path(Path::new(&m.folder), &name)?;
                        if from.is_file() {
                            let to = folder.join(name.as_ref());
                            if !to.exists() {
                                atomic_write(&to, &fs::read(from)?)?;
                            }
                        }
                    }
                    self.finish_step(&r, &mut j, "stash")?;
                }
                "stash" => {
                    if git::head(&r).is_some() && git::dirty(&r)? {
                        let sig = git2::Signature::now(
                            &self.settings.commit_name,
                            &self.settings.commit_email,
                        )?;
                        let oid = r.stash_save(
                            &sig,
                            &format!("lazysync {}", j.operation_id),
                            Some(git2::StashFlags::INCLUDE_UNTRACKED),
                        )?;
                        j.stash_oid = Some(oid.to_string());
                    }
                    self.finish_step(&r, &mut j, "fetch")?;
                }
                "fetch" => {
                    git::fetch_with_progress(&r, &m.branch, self.token, &mut *emit)?;
                    self.check(&r, &j)?;
                    self.finish_step(&r, &mut j, "integrate")?;
                }
                "integrate" | "retryIntegrate" => {
                    let retry = j.phase == "retryIntegrate";
                    if let Ok(up) = r.refname_to_id(&format!("refs/remotes/origin/{}", m.branch)) {
                        validate_incoming(&r, up)?;
                        if git::head(&r).is_none() && git::dirty(&r)? {
                            j.in_flight = false;
                            self.storage.save_journal(&j)?;
                            return Err(paused("An unborn checkout has files and the remote already has history. Clone the remote into an empty folder and copy your files there."));
                        }
                        if let Some(h) = git::head(&r) {
                            let h = Oid::from_str(&h)?;
                            if h != up {
                                let (ahead, behind) = r.graph_ahead_behind(h, up)?;
                                if behind > 0 {
                                    r.reference(
                                        &format!(
                                            "refs/lazysync/backups/{}/before-rebase-{}",
                                            j.operation_id, j.retries
                                        ),
                                        h,
                                        false,
                                        "preserve commits before rebase",
                                    )
                                    .or_else(|e| {
                                        if e.code() == git2::ErrorCode::Exists {
                                            r.find_reference(&format!(
                                                "refs/lazysync/backups/{}/before-rebase-{}",
                                                j.operation_id, j.retries
                                            ))
                                        } else {
                                            Err(e)
                                        }
                                    })?;
                                    let args = if ahead == 0 {
                                        vec!["merge", "--ff-only", "--no-edit", "--"]
                                    } else {
                                        vec!["rebase"]
                                    };
                                    let target = up.to_string();
                                    let mut args = args;
                                    args.push(&target);
                                    if let Err(e) = git::command(
                                        Path::new(&m.folder),
                                        &args,
                                        Some(self.settings),
                                    ) {
                                        return self.capture_error(&r, &mut j, e);
                                    }
                                }
                            }
                        } else {
                            git::command(
                                Path::new(&m.folder),
                                &["merge", "--ff-only", "--no-edit", &up.to_string()],
                                Some(self.settings),
                            )?;
                        }
                    }
                    self.finish_step(&r, &mut j, if retry { "push" } else { "apply" })?;
                }
                "apply" => {
                    if let Some(oid) = &j.stash_oid {
                        stash_index(&mut r,oid)?.ok_or_else(|| paused("The saved app stash is missing. Restore it from the recovery backup before continuing."))?;
                        if let Err(e) =
                            git::command(Path::new(&m.folder), &["stash", "apply", oid], None)
                        {
                            return self.capture_error(&r, &mut j, e);
                        }
                    }
                    self.finish_step(&r, &mut j, "commit")?;
                }
                "commit" => {
                    if git::dirty(&r)? {
                        let before = git::worktree_fingerprint(&r)?;
                        git::command(Path::new(&m.folder), &["add", "-A", "--", "."], None)?;
                        if git::worktree_fingerprint(&r)? != before {
                            return Err(paused(
                                "Files changed while staging. Review recovery before continuing.",
                            ));
                        }
                        let mut index = r.index()?;
                        index.read(true)?;
                        let tree_oid = index.write_tree()?;
                        let tree = r.find_tree(tree_oid)?;
                        let parent = r.head().ok().and_then(|h| h.peel_to_commit().ok());
                        if parent.as_ref().is_none_or(|p| p.tree_id() != tree_oid) {
                            let time = chrono::Local::now();
                            let sig = git2::Signature::now(
                                &self.settings.commit_name,
                                &self.settings.commit_email,
                            )?;
                            r.commit(
                                Some("HEAD"),
                                &sig,
                                &sig,
                                &format!(
                                    "Sync from {} - {}\n\nTimezone: {}\nOperation: {}\n",
                                    self.settings.device_name,
                                    time.format("%Y-%m-%d %H:%M"),
                                    time.format("%:z"),
                                    j.operation_id
                                ),
                                &tree,
                                &parent.iter().collect::<Vec<_>>(),
                            )?;
                        }
                        if git::worktree_fingerprint(&r)? != before {
                            return Err(paused("Files changed while committing. The local commit and recovery backups are preserved."));
                        }
                    }
                    self.finish_step(&r, &mut j, "push")?;
                }
                "push" => {
                    if git::head(&r).is_some() {
                        if let Err(e) =
                            git::push_with_progress(&r, &m.branch, self.token, &mut *emit)
                        {
                            if e.code == "pushRejected" && j.retries < 2 {
                                self.check(&r, &j)?;
                                git::fetch_with_progress(&r, &m.branch, self.token, &mut *emit)?;
                                self.check(&r, &j)?;
                                j.retries += 1;
                                self.finish_step(&r, &mut j, "retryIntegrate")?;
                                continue;
                            }
                            return Err(e);
                        }
                    }
                    self.check(&r, &j)?;
                    self.finish_step(&r, &mut j, "verify")?;
                }
                "verify" => {
                    if let Some(head) = git::head(&r) {
                        git::fetch_with_progress(&r, &m.branch, self.token, &mut *emit)?;
                        self.check(&r, &j)?;
                        let upstream =
                            r.refname_to_id(&format!("refs/remotes/origin/{}", m.branch))?;
                        let h = Oid::from_str(&head)?;
                        if upstream != h && !r.graph_descendant_of(upstream, h)? {
                            return Err(paused("Push acceptance could not be verified. Local commits and backups are preserved."));
                        }
                    }
                    self.finish_step(&r, &mut j, "cleanup")?;
                }
                "cleanup" => {
                    if let Some(oid) = &j.stash_oid {
                        if let Some(idx) = stash_index(&mut r, oid)? {
                            r.stash_drop(idx)?;
                        }
                    }
                    j.completed = true;
                    j.in_flight = false;
                    self.storage.save_journal(&j)?;
                    emit("idle");
                    return Ok(());
                }
                _ => return Err(paused("Unrecognized recovery phase.")),
            }
        }
    }
    fn capture_error<T>(&self, r: &Repository, j: &mut Recovery, error: AppError) -> Result<T> {
        let mut captured = vec![];
        let folder = self.storage.conflict_folder(j)?;
        let mut index = r.index()?;
        index.read(true)?;
        for conflict in index.conflicts()? {
            let c = conflict?;
            let name = c
                .our
                .as_ref()
                .or(c.their.as_ref())
                .or(c.ancestor.as_ref())
                .map(|e| String::from_utf8_lossy(&e.path).into_owned())
                .ok_or_else(|| paused("Conflict has no path."))?;
            // During rebase and stash apply, stage 3 contains our saved local version.
            let local = c.their.as_ref().map(|e| r.find_blob(e.id)).transpose()?;
            let remote = c.our.as_ref().map(|e| r.find_blob(e.id)).transpose()?;
            let stem = format!("{:x}", sha2::Sha256::digest(name.as_bytes()));
            atomic_write(
                &folder.join(format!("{stem}.protected")),
                git::fingerprint_except(r, &name)?.as_bytes(),
            )?;
            for (label, entry) in [("local", c.their.as_ref()), ("remote", c.our.as_ref())] {
                if let Some(entry) = entry {
                    atomic_write(
                        &folder.join(format!("{stem}.{label}.mode")),
                        entry.mode.to_string().as_bytes(),
                    )?;
                }
            }
            for (label, blob) in [("local", local.as_ref()), ("remote", remote.as_ref())] {
                if let Some(b) = blob {
                    atomic_write(&folder.join(format!("{stem}.{label}")), b.content())?;
                }
            }
            let binary = local
                .as_ref()
                .is_some_and(|b| b.is_binary() || std::str::from_utf8(b.content()).is_err())
                || remote
                    .as_ref()
                    .is_some_and(|b| b.is_binary() || std::str::from_utf8(b.content()).is_err());
            let preview = |blob: Option<&git2::Blob>| {
                blob.map(|b| {
                    if binary {
                        "Binary file — preview unavailable".into()
                    } else {
                        String::from_utf8_lossy(&b.content()[..b.size().min(65536)]).into_owned()
                    }
                })
            };
            captured.push(Conflict {
                path: name,
                local: preview(local.as_ref()),
                remote: preview(remote.as_ref()),
                binary,
                truncated: local.as_ref().is_some_and(|b| b.size() > 65536)
                    || remote.as_ref().is_some_and(|b| b.size() > 65536),
                resolved: false,
            });
        }
        j.conflicts = captured;
        j.expected = git::fingerprint(r)?;
        self.storage.save_journal(j)?;
        if !j.conflicts.is_empty() {
            Err(AppError::new(
                "conflicts",
                "Local and remote changes overlap.",
                "Resolve each file, then select Continue Sync.",
            ))
        } else {
            Err(error)
        }
    }
}
use sha2::Digest;
pub fn resolve(storage: &Storage, path: &str, choice: &str) -> Result<()> {
    let mut j = storage
        .journal()?
        .ok_or_else(|| paused("No interrupted sync exists."))?;
    let r = git::open(&j.folder)?;
    if git::fingerprint(&r)? != j.expected && choice != "edited" {
        return Err(paused(
            "The checkout changed while resolving conflicts. Review it in your Git client.",
        ));
    }
    let c = j
        .conflicts
        .iter()
        .find(|c| c.path == path && !c.resolved)
        .cloned()
        .ok_or_else(|| paused("Conflict not found."))?;
    if choice == "edited" {
        let stem = format!("{:x}", sha2::Sha256::digest(path.as_bytes()));
        let protected = fs::read_to_string(
            storage
                .conflict_folder(&j)?
                .join(format!("{stem}.protected")),
        )?;
        if protected != git::fingerprint_except(&r, path)? {
            return Err(paused("Other files, HEAD, or the index changed while editing this conflict. Review recovery in your Git client."));
        }
        let p = git::safe_path(Path::new(&j.folder), path)?;
        if p.is_file()
            && fs::read_to_string(&p).is_ok_and(|s| {
                s.lines()
                    .any(|l| l.starts_with("<<<<<<< ") || l.starts_with(">>>>>>> "))
            })
        {
            return Err(paused("Conflict markers remain in this file."));
        }
        git::command(Path::new(&j.folder), &["add", "-A", "--", path], None)?;
    } else {
        let local = match choice {
            "local" => true,
            "remote" => false,
            _ => return Err(paused("Invalid resolution choice.")),
        };
        let exists = if local {
            c.local.is_some()
        } else {
            c.remote.is_some()
        };
        let stem = format!("{:x}", sha2::Sha256::digest(path.as_bytes()));
        let bytes = if exists {
            Some(fs::read(storage.conflict_folder(&j)?.join(format!(
                "{stem}.{}",
                if local { "local" } else { "remote" }
            )))?)
        } else {
            None
        };
        let label = if local { "local" } else { "remote" };
        let mode = if bytes.is_some() {
            fs::read_to_string(
                storage
                    .conflict_folder(&j)?
                    .join(format!("{stem}.{label}.mode")),
            )?
            .parse::<u32>()
            .unwrap_or(0o100644)
        } else {
            0
        };
        if mode == 0o120000 {
            return Err(paused(
                "Resolve symbolic-link conflicts in your Git client.",
            ));
        }
        git::write_resolution(&r, path, bytes.as_deref())?;
        #[cfg(unix)]
        if bytes.is_some() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                git::safe_path(Path::new(&j.folder), path)?,
                fs::Permissions::from_mode(if mode & 0o111 != 0 { 0o755 } else { 0o644 }),
            )?;
            git::command(Path::new(&j.folder), &["add", "--", path], None)?;
        }
    }
    j.conflicts
        .iter_mut()
        .find(|c| c.path == path)
        .unwrap()
        .resolved = true;
    j.expected = git::fingerprint(&r)?;
    for conflict in j.conflicts.iter().filter(|c| !c.resolved) {
        let stem = format!("{:x}", sha2::Sha256::digest(conflict.path.as_bytes()));
        atomic_write(
            &storage
                .conflict_folder(&j)?
                .join(format!("{stem}.protected")),
            git::fingerprint_except(&r, &conflict.path)?.as_bytes(),
        )?;
    }
    storage.save_journal(&j)
}
pub fn finish_manual_recovery(storage: &Storage) -> Result<()> {
    let mut j = storage
        .journal()?
        .ok_or_else(|| paused("No interrupted sync exists."))?;
    let r = git::open(&j.folder)?;
    if r.state() != git2::RepositoryState::Clean || r.index()?.has_conflicts() {
        return Err(paused(
            "Finish the Git operation and resolve all conflicts in your Git client first.",
        ));
    }
    // This explicitly acknowledges manual recovery; never removes a stash or backup reference.
    j.completed = true;
    j.in_flight = false;
    storage.save_journal(&j)
}
fn stash_index(r: &mut Repository, oid: &str) -> Result<Option<usize>> {
    let mut idx = None;
    r.stash_foreach(|i, _, o| {
        if o.to_string() == oid {
            idx = Some(i);
        }
        true
    })?;
    Ok(idx)
}
fn paused(message: &str) -> AppError {
    AppError::new(
        "recovery",
        message,
        "Review recovery and continue only after your files are preserved.",
    )
}
fn validate_incoming(r: &Repository, oid: Oid) -> Result<()> {
    let tree = r.find_commit(oid)?.tree()?;
    let mut unsupported = false;
    tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
        if entry.filemode() == 0o160000 || entry.name() == Some(".gitmodules") {
            unsupported = true;
        }
        if entry.name() == Some(".gitattributes") {
            if let Ok(blob) = r.find_blob(entry.id()) {
                if String::from_utf8_lossy(blob.content()).contains("filter=") {
                    unsupported = true;
                }
            }
        }
        git2::TreeWalkResult::Ok
    })?;
    if unsupported {
        return Err(AppError::new(
            "unsupportedRemote",
            "The incoming branch uses LFS/content filters or submodules.",
            "Use your Git client for this repository; recovery backups remain intact.",
        ));
    }
    Ok(())
}
