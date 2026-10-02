use crate::{models::*, storage::atomic_write};
use git2::{FetchOptions, Oid, PushOptions, RemoteCallbacks, Repository, StatusOptions};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path},
    process::Command,
};

pub fn command(folder: &Path, args: &[&str], settings: Option<&Settings>) -> Result<Vec<u8>> {
    // An empty hooksPath can resolve to the checkout root. Use a real empty directory.
    let hooks = tempfile::tempdir()?;
    let disabled_hooks = format!("core.hooksPath={}", hooks.path().to_string_lossy());
    let mut c = Command::new("git");
    c.current_dir(folder)
        .args([
            "-c",
            &disabled_hooks,
            "-c",
            "commit.gpgSign=false",
            "-c",
            "rebase.autoStash=false",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
        ])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true");
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
    ] {
        c.env_remove(key);
    }
    if let Some(s) = settings {
        c.env("GIT_AUTHOR_NAME", &s.commit_name)
            .env("GIT_COMMITTER_NAME", &s.commit_name)
            .env("GIT_AUTHOR_EMAIL", &s.commit_email)
            .env("GIT_COMMITTER_EMAIL", &s.commit_email);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    let o = c.output().map_err(|_| {
        AppError::new(
            "gitMissing",
            "Git is not installed or is unavailable.",
            "Install Git and restart lazysync.",
        )
    })?;
    if !o.status.success() {
        return Err(AppError::new("gitCommand", "Git could not finish the operation.", "Review conflicts or open the repository in your Git client. Recovery backups are preserved."));
    }
    Ok(o.stdout)
}
pub fn open(folder: &str) -> Result<Repository> {
    let r = Repository::open_ext(folder, git2::RepositoryOpenFlags::NO_SEARCH, &[] as &[&str])?;
    if r.is_bare() || r.is_worktree() {
        return Err(AppError::new(
            "unsupported",
            "Bare repositories and linked worktrees are unsupported.",
            "Use a regular checkout.",
        ));
    }
    let root = r.workdir().ok_or_else(|| {
        AppError::new(
            "folder",
            "No checkout folder found.",
            "Choose a regular checkout.",
        )
    })?;
    if fs::canonicalize(root)? != fs::canonicalize(folder)? {
        return Err(AppError::new(
            "folder",
            "Choose the root of the checkout.",
            "Select the folder containing .git.",
        ));
    }
    Ok(r)
}
pub fn head(r: &Repository) -> Option<String> {
    r.head()
        .ok()
        .and_then(|h| h.target())
        .map(|o| o.to_string())
}
pub fn branch(r: &Repository) -> Result<String> {
    if r.head_detached().unwrap_or(false) {
        return Err(AppError::new(
            "branch",
            "The checkout has a detached HEAD.",
            "Check out a branch in your Git client.",
        ));
    }
    let h = r.find_reference("HEAD")?;
    Ok(h.symbolic_target()
        .and_then(|s| s.strip_prefix("refs/heads/"))
        .ok_or_else(|| {
            AppError::new(
                "branch",
                "Cannot determine current branch.",
                "Check out a branch.",
            )
        })?
        .to_string())
}
pub fn preflight(r: &Repository, mapping: &RepositoryMapping, allow_rebase: bool) -> Result<()> {
    if !allow_rebase && r.state() != git2::RepositoryState::Clean {
        return Err(AppError::new(
            "gitState",
            "Another Git operation is in progress.",
            "Finish it in your Git client first.",
        ));
    }
    if !allow_rebase && branch(r)? != mapping.branch {
        return Err(AppError::new(
            "branchChanged",
            "The active branch changed outside lazysync.",
            "Reconfirm this branch in Settings before syncing.",
        ));
    }
    let remote = r.find_remote("origin")?;
    #[cfg(not(test))]
    if remote.url() != Some(mapping.remote.url().as_str())
        || remote.pushurl().is_some_and(|u| u != mapping.remote.url())
    {
        return Err(AppError::new(
            "remoteChanged",
            "Origin no longer matches the selected GitHub repository.",
            "Reconnect the checkout after reviewing origin.",
        ));
    }
    #[cfg(test)]
    let _ = remote;
    let c = r.config()?;
    if c.get_bool("commit.gpgsign").unwrap_or(false) {
        return Err(AppError::new(
            "signing",
            "This repository requires signed commits.",
            "Use your Git client for this signing workflow.",
        ));
    }
    if !r.submodules()?.is_empty() || r.workdir().unwrap().join(".gitmodules").exists() {
        return Err(AppError::new(
            "submodules",
            "Submodules are unsupported.",
            "Synchronize this repository with your Git client.",
        ));
    }
    // Having Git LFS installed globally does not mean this checkout uses LFS.
    // Inspect effective attributes for each relevant path, including global/info attributes.
    let attrs = command(
        r.workdir().unwrap(),
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        None,
    )?;
    let mut active_filter = false;
    for path in attrs.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let name = String::from_utf8_lossy(path);
        let attr = r.get_attr_bytes(
            Path::new(name.as_ref()),
            "filter",
            git2::AttrCheckFlags::FILE_THEN_INDEX,
        )?;
        if !matches!(
            git2::AttrValue::from_bytes(attr),
            git2::AttrValue::Unspecified | git2::AttrValue::False
        ) {
            active_filter = true;
            break;
        }
    }
    if active_filter {
        return Err(AppError::new(
            "filters",
            "This checkout uses a Git content filter, including LFS.",
            "Use your Git client for filtered files.",
        ));
    }
    for path in attrs
        .split(|b| *b == 0)
        .filter(|p| p.ends_with(b".gitattributes"))
    {
        if let Ok(text) = fs::read_to_string(
            r.workdir()
                .unwrap()
                .join(String::from_utf8_lossy(path).as_ref()),
        ) {
            if text.contains("filter=") {
                return Err(AppError::new(
                    "lfs",
                    "Git attribute filters, including LFS, are unsupported.",
                    "Use your Git client for this repository.",
                ));
            }
        }
    }
    let configured = c.get_string("core.hooksPath").ok().map(|p| {
        let p = Path::new(&p).to_path_buf();
        if p.is_absolute() {
            p
        } else {
            r.workdir().unwrap().join(p)
        }
    });
    let hooks = configured.unwrap_or_else(|| r.path().join("hooks"));
    if hooks.exists() {
        for entry in fs::read_dir(hooks)? {
            let e = entry?;
            if e.file_type()?.is_file() && !e.file_name().to_string_lossy().ends_with(".sample") {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if e.metadata()?.permissions().mode() & 0o111 == 0 {
                        continue;
                    }
                }
                return Err(AppError::new(
                    "hooks",
                    "Active Git hooks are unsupported.",
                    "Use your Git client to run this repository's hooks.",
                ));
            }
        }
    }
    if r.is_shallow() {
        return Err(AppError::new(
            "shallow",
            "Shallow clones are unsupported.",
            "Fetch full history with your Git client.",
        ));
    }
    let upstream_remote = c
        .get_string(&format!("branch.{}.remote", mapping.branch))
        .ok();
    let upstream_merge = c
        .get_string(&format!("branch.{}.merge", mapping.branch))
        .ok();
    if upstream_remote.as_deref().is_some_and(|v| v != "origin")
        || upstream_merge
            .as_deref()
            .is_some_and(|v| v != format!("refs/heads/{}", mapping.branch))
    {
        return Err(AppError::new(
            "upstream",
            "Branch upstream differs from the selected branch on origin.",
            "Review and reconnect the branch.",
        ));
    }
    Ok(())
}
pub fn callbacks(token: &str) -> RemoteCallbacks<'_> {
    let mut cb = RemoteCallbacks::new();
    cb.credentials(move |url, _, _| {
        if url.starts_with("https://github.com/") {
            git2::Cred::userpass_plaintext("x-access-token", token)
        } else {
            Err(git2::Error::from_str(
                "Only GitHub HTTPS credentials are supported",
            ))
        }
    });
    cb
}
pub fn fetch(r: &Repository, b: &str, token: &str) -> Result<()> {
    let mut o = FetchOptions::new();
    o.remote_callbacks(callbacks(token));
    // Fetch all branches to distinguish an empty/missing target from a network failure.
    r.find_remote("origin")?
        .fetch(&["+refs/heads/*:refs/remotes/origin/*"], Some(&mut o), None)?;
    let mut c = r.config()?;
    c.set_str(&format!("branch.{b}.remote"), "origin")?;
    c.set_str(&format!("branch.{b}.merge"), &format!("refs/heads/{b}"))?;
    Ok(())
}
pub fn push(r: &Repository, b: &str, token: &str) -> Result<()> {
    let mut rejection = None;
    let mut cb = callbacks(token);
    cb.push_update_reference(|_, msg| {
        if let Some(m) = msg {
            rejection = Some(m.to_owned());
        }
        Ok(())
    });
    let mut opts = PushOptions::new();
    opts.remote_callbacks(cb);
    let result = r.find_remote("origin")?.push(
        &[&format!("refs/heads/{b}:refs/heads/{b}")],
        Some(&mut opts),
    );
    drop(opts);
    if rejection.is_some()
        || result
            .as_ref()
            .is_err_and(|e| e.code() == git2::ErrorCode::NotFastForward)
    {
        return Err(AppError::new(
            "pushRejected",
            "The remote rejected the push.",
            "Refetch and retry; never force push.",
        ));
    }
    result?;
    Ok(())
}
pub fn counts(r: &Repository) -> Result<DiffCounts> {
    let mut opt = StatusOptions::new();
    opt.include_untracked(true).recurse_untracked_dirs(true);
    let mut c = DiffCounts::default();
    for s in r.statuses(Some(&mut opt))?.iter() {
        let x = s.status();
        if x.intersects(git2::Status::WT_DELETED | git2::Status::INDEX_DELETED) {
            c.deleted += 1;
        } else if x.intersects(git2::Status::WT_NEW | git2::Status::INDEX_NEW) {
            c.added += 1;
        } else if x != git2::Status::CURRENT {
            c.modified += 1;
        }
    }
    Ok(c)
}
pub fn dirty(r: &Repository) -> Result<bool> {
    let c = counts(r)?;
    Ok(c.added + c.modified + c.deleted > 0)
}
pub fn ahead_behind(r: &Repository, b: &str) -> Result<(u32, u32)> {
    match (
        r.head().ok().and_then(|h| h.target()),
        r.refname_to_id(&format!("refs/remotes/origin/{b}")).ok(),
    ) {
        (Some(h), Some(u)) => {
            let (a, b) = r.graph_ahead_behind(h, u)?;
            Ok((a as u32, b as u32))
        }
        (Some(_), None) => Ok((1, 0)),
        (None, Some(_)) => Ok((0, 1)),
        _ => Ok((0, 0)),
    }
}
pub fn fingerprint(r: &Repository) -> Result<String> {
    fingerprint_parts(r, true, None)
}
pub fn worktree_fingerprint(r: &Repository) -> Result<String> {
    fingerprint_parts(r, false, None)
}
pub fn fingerprint_except(r: &Repository, except: &str) -> Result<String> {
    fingerprint_parts(r, true, Some(except))
}
fn fingerprint_parts(r: &Repository, include_git: bool, except: Option<&str>) -> Result<String> {
    let mut digest = Sha256::new();
    if include_git {
        digest.update(head(r).unwrap_or_default());
        digest.update(branch(r).unwrap_or_else(|_| "detached".into()));
        if let Ok(b) = fs::read(r.path().join("index")) {
            digest.update(b);
        }
    }
    let paths = command(
        r.workdir().unwrap(),
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        None,
    )?;
    let mut paths: Vec<_> = paths.split(|b| *b == 0).filter(|p| !p.is_empty()).collect();
    paths.sort();
    paths.dedup();
    for p in paths {
        if except.is_some_and(|name| name.as_bytes() == p) {
            continue;
        }
        let path = r
            .workdir()
            .unwrap()
            .join(String::from_utf8_lossy(p).as_ref());
        if !include_git && fs::symlink_metadata(&path).is_err() {
            continue;
        }
        digest.update(p);
        match fs::symlink_metadata(&path) {
            Ok(m) if m.file_type().is_symlink() => {
                digest.update(fs::read_link(path)?.to_string_lossy().as_bytes())
            }
            Ok(m) if m.is_file() => {
                use std::io::Read;
                let mut f = fs::File::open(path)?;
                let mut buf = [0u8; 65536];
                loop {
                    let n = f.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    digest.update(&buf[..n]);
                }
            }
            _ => digest.update(b"missing"),
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}
pub fn safe_path(root: &Path, name: &str) -> Result<std::path::PathBuf> {
    let p = Path::new(name);
    if p.components().any(|c| !matches!(c, Component::Normal(_)))
        || name.is_empty()
        || name.contains('\\')
        || p.components()
            .any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case(".git"))
    {
        return Err(AppError::new(
            "path",
            "Unsafe repository path.",
            "Resolve this file with your Git client.",
        ));
    }
    let mut current = root.to_path_buf();
    for c in p.components() {
        current.push(c);
        if fs::symlink_metadata(&current).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(AppError::new(
                "symlink",
                "Resolution through a symbolic link is unsupported.",
                "Resolve with your Git client.",
            ));
        }
    }
    Ok(root.join(p))
}
pub fn attach(
    remote: &RemoteRepository,
    folder: &str,
    initialize: bool,
) -> Result<RepositoryMapping> {
    fs::create_dir_all(folder)?;
    let r = match open(folder) {
        Ok(r) => r,
        Err(e) => {
            if !initialize
                || Path::new(folder).join(".git").exists()
                || Repository::discover(folder).is_ok()
            {
                return Err(e);
            }
            let mut opts = git2::RepositoryInitOptions::new();
            opts.initial_head("main");
            Repository::init_opts(folder, &opts)?
        }
    };
    match r.find_remote("origin") {
        Ok(o) => {
            if o.url() != Some(remote.url().as_str())
                || o.pushurl().is_some_and(|u| u != remote.url())
            {
                return Err(AppError::new(
                    "remoteConflict",
                    "Existing origin points to another repository.",
                    "Review origin in your Git client; lazysync will not replace it.",
                ));
            }
        }
        Err(_) if initialize => {
            r.remote("origin", &remote.url())?;
        }
        Err(e) => return Err(e.into()),
    }
    let b = branch(&r)?;
    let m = RepositoryMapping {
        remote: remote.clone(),
        folder: fs::canonicalize(folder)?.to_string_lossy().into_owned(),
        branch: b,
        last_sync: None,
    };
    preflight(&r, &m, false)?;
    Ok(m)
}
pub fn clone_repo(
    remote: &RemoteRepository,
    folder: &str,
    token: &str,
) -> Result<RepositoryMapping> {
    let p = Path::new(folder);
    if p.exists() && fs::read_dir(p)?.next().is_some() {
        return Err(AppError::new(
            "notEmpty",
            "Clone destination must be empty.",
            "Choose an empty folder.",
        ));
    }
    let mut opts = FetchOptions::new();
    opts.remote_callbacks(callbacks(token));
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(opts);
    builder.clone(&remote.url(), p)?;
    attach(remote, folder, false)
}
pub fn history(r: &Repository, page: u32) -> Result<Vec<Commit>> {
    if head(r).is_none() {
        return Ok(vec![]);
    }
    let mut w = r.revwalk()?;
    w.push_head()?;
    w.set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)?;
    w.skip(page.min(10000) as usize * 20)
        .take(20)
        .map(|o| {
            let c = r.find_commit(o?)?;
            let author = c.author().name().unwrap_or("Unknown").to_string();
            Ok(Commit {
                oid: c.id().to_string(),
                summary: c.summary().unwrap_or("Untitled commit").into(),
                author,
                timestamp: chrono::DateTime::from_timestamp(c.time().seconds(), 0)
                    .unwrap_or_default()
                    .to_rfc3339(),
            })
        })
        .collect()
}
pub fn diffs(r: &Repository, oid: Option<&str>) -> Result<Vec<FileDiff>> {
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .max_size(1024 * 1024);
    let diff = if let Some(oid) = oid {
        let c = r.find_commit(Oid::from_str(oid)?)?;
        let t = c.tree()?;
        let p = c.parent(0).ok().and_then(|c| c.tree().ok());
        r.diff_tree_to_tree(p.as_ref(), Some(&t), Some(&mut opts))?
    } else {
        let t = r.head().ok().and_then(|h| h.peel_to_tree().ok());
        r.diff_tree_to_workdir_with_index(t.as_ref(), Some(&mut opts))?
    };
    let mut result = vec![];
    for i in 0..diff.deltas().len().min(200) {
        let delta = diff.get_delta(i).unwrap();
        let mut patch = String::new();
        let mut binary = delta.flags().contains(git2::DiffFlags::BINARY);
        let mut truncated = false;
        if let Some(mut p) = git2::Patch::from_diff(&diff, i)? {
            let buf = p.to_buf()?;
            let b: &[u8] = &buf;
            binary |= b.contains(&0) || std::str::from_utf8(b).is_err();
            truncated = b.len() > 65536;
            if !binary {
                patch = String::from_utf8_lossy(&b[..b.len().min(65536)]).into_owned();
            }
        }
        binary |= delta.old_file().is_binary() || delta.new_file().is_binary();
        if patch.contains("Binary files") {
            binary = true;
        }
        if let Some(path) = delta.new_file().path() {
            let local = r.workdir().unwrap().join(path);
            if let Ok(mut file) = fs::File::open(local) {
                use std::io::Read;
                let mut buf = [0u8; 8192];
                if let Ok(n) = file.read(&mut buf) {
                    binary |= buf[..n].contains(&0);
                }
            }
        }
        result.push(FileDiff {
            path: delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .unwrap_or(Path::new("unknown"))
                .to_string_lossy()
                .into_owned(),
            binary,
            truncated,
            patch,
        });
    }
    Ok(result)
}
pub fn write_resolution(r: &Repository, name: &str, bytes: Option<&[u8]>) -> Result<()> {
    let p = safe_path(r.workdir().unwrap(), name)?;
    if let Some(bytes) = bytes {
        atomic_write(&p, bytes)?;
        command(r.workdir().unwrap(), &["add", "--", name], None)?;
    } else {
        if p.is_file() {
            fs::remove_file(p)?;
        }
        command(r.workdir().unwrap(), &["add", "-A", "--", name], None)?;
    }
    Ok(())
}
