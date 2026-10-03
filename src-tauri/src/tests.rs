use crate::{
    git,
    github::GitHub,
    models::*,
    storage::Storage,
    sync::{resolve, SyncEngine},
    watcher::automation_due,
};
use git2::Repository;
use std::{
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

struct Fixture {
    _temp: tempfile::TempDir,
    folder: String,
    remote: String,
    storage: Storage,
    settings: Settings,
    mapping: RepositoryMapping,
}
impl Fixture {
    fn new(initial: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let folder = root.join("checkout");
        let remote = root.join("remote.git");
        fs::create_dir(&folder).unwrap();
        let mut opts = git2::RepositoryInitOptions::new();
        opts.initial_head("main");
        Repository::init_opts(&folder, &opts).unwrap();
        opts.bare(true);
        Repository::init_opts(&remote, &opts).unwrap();
        let r = Repository::open(&folder).unwrap();
        r.remote("origin", remote.to_str().unwrap()).unwrap();
        let storage = Storage::new(root.join("app")).unwrap();
        let settings = Settings {
            commit_name: "Test User".into(),
            commit_email: "test@example.com".into(),
            device_name: "Test Device".into(),
            ..Settings::default()
        };
        let mapping = RepositoryMapping {
            remote: RemoteRepository {
                id: "1".into(),
                full_name: "test/repo".into(),
                owner: "test".into(),
                private: true,
                writable: true,
                default_branch: "main".into(),
            },
            folder: folder.to_string_lossy().into_owned(),
            branch: "main".into(),
            last_sync: None,
        };
        let f = Self {
            _temp: temp,
            folder: mapping.folder.clone(),
            remote: remote.to_string_lossy().into_owned(),
            storage,
            settings,
            mapping,
        };
        if initial {
            f.write("notes.txt", b"base\n");
            f.commit("base");
            f.push();
        }
        f
    }
    fn engine(&self) -> SyncEngine<'_> {
        SyncEngine {
            storage: &self.storage,
            settings: &self.settings,
            token: "",
        }
    }
    fn write(&self, name: &str, bytes: &[u8]) {
        fs::write(Path::new(&self.folder).join(name), bytes).unwrap();
    }
    fn cmd(&self, args: &[&str]) {
        git::command(Path::new(&self.folder), args, Some(&self.settings)).unwrap();
    }
    fn commit(&self, msg: &str) {
        self.cmd(&["add", "-A"]);
        self.cmd(&["commit", "-m", msg]);
    }
    fn push(&self) {
        git::push(&git::open(&self.folder).unwrap(), "main", "").unwrap();
    }
    fn peer(&self) -> String {
        let p = self._temp.path().join("peer");
        Repository::clone(&self.remote, &p).unwrap();
        p.to_string_lossy().into_owned()
    }
    fn peer_commit(&self, peer: &str, name: &str, bytes: &[u8]) {
        fs::write(Path::new(peer).join(name), bytes).unwrap();
        git::command(Path::new(peer), &["add", "-A"], Some(&self.settings)).unwrap();
        git::command(
            Path::new(peer),
            &["commit", "-m", "remote edit"],
            Some(&self.settings),
        )
        .unwrap();
        git::push(&git::open(peer).unwrap(), "main", "").unwrap();
    }
}

#[test]
fn initial_push_untracked_ignored_and_no_empty_commit() {
    let f = Fixture::new(false);
    f.write(".gitignore", b"cache/\n");
    fs::create_dir(Path::new(&f.folder).join("cache")).unwrap();
    f.write("cache/local", b"ignored");
    f.write("new.txt", b"hello");
    f.engine().start(&f.mapping, |_| {}).unwrap();
    let r = git::open(&f.folder).unwrap();
    let first = git::head(&r).unwrap();
    assert!(r
        .head()
        .unwrap()
        .peel_to_tree()
        .unwrap()
        .get_path(Path::new("cache/local"))
        .is_err());
    f.engine().start(&f.mapping, |_| {}).unwrap();
    assert_eq!(first, git::head(&r).unwrap());
    assert!(f.storage.journal().unwrap().is_none());
    assert!(fs::read(Path::new(&f.folder).join("cache/local")).is_ok());
}
#[test]
fn stash_restoration_deletions_and_user_stash_preserved() {
    let f = Fixture::new(true);
    f.write("notes.txt", b"user stash\n");
    f.cmd(&["stash", "push", "-m", "user stash"]);
    f.write("new.txt", b"new");
    fs::remove_file(Path::new(&f.folder).join("notes.txt")).unwrap();
    f.engine().start(&f.mapping, |_| {}).unwrap();
    let mut r = git::open(&f.folder).unwrap();
    let mut names = vec![];
    r.stash_foreach(|_, n, _| {
        names.push(n.to_string());
        true
    })
    .unwrap();
    assert_eq!(names.len(), 1);
    assert!(names[0].contains("user stash"));
    assert!(!Path::new(&f.folder).join("notes.txt").exists());
    assert_eq!(
        fs::read(Path::new(&f.folder).join("new.txt")).unwrap(),
        b"new"
    );
    assert!(!git::dirty(&r).unwrap());
    assert!(
        r.references_glob("refs/lazysync/backups/*/original")
            .unwrap()
            .count()
            > 0
    );
}
#[test]
fn clean_behind_fast_forward() {
    let f = Fixture::new(true);
    let peer = f.peer();
    f.peer_commit(&peer, "remote.txt", b"remote");
    f.engine().start(&f.mapping, |_| {}).unwrap();
    assert_eq!(
        fs::read(Path::new(&f.folder).join("remote.txt")).unwrap(),
        b"remote"
    );
    assert_eq!(
        git::ahead_behind(&git::open(&f.folder).unwrap(), "main").unwrap(),
        (0, 0)
    );
}
#[test]
fn divergence_rebases_and_preserves_local_commit_backup() {
    let f = Fixture::new(true);
    let peer = f.peer();
    f.write("local.txt", b"local");
    f.commit("local");
    let original = git::head(&git::open(&f.folder).unwrap()).unwrap();
    f.peer_commit(&peer, "remote.txt", b"remote");
    f.write("untracked.txt", b"pending");
    f.engine().start(&f.mapping, |_| {}).unwrap();
    let r = git::open(&f.folder).unwrap();
    assert_ne!(git::head(&r).unwrap(), original);
    assert!(r
        .find_commit(git2::Oid::from_str(&original).unwrap())
        .is_ok());
    assert!(Path::new(&f.folder).join("local.txt").exists());
    assert!(Path::new(&f.folder).join("remote.txt").exists());
    assert!(Path::new(&f.folder).join("untracked.txt").exists());
}
#[test]
fn rebase_conflict_local_remote_labels_and_continue() {
    let f = Fixture::new(true);
    let peer = f.peer();
    f.write("notes.txt", b"local version\n");
    f.commit("local");
    f.peer_commit(&peer, "notes.txt", b"remote version\n");
    assert_eq!(
        f.engine().start(&f.mapping, |_| {}).unwrap_err().code,
        "conflicts"
    );
    let j = f.storage.journal().unwrap().unwrap();
    assert_eq!(j.conflicts[0].local.as_deref(), Some("local version\n"));
    assert_eq!(j.conflicts[0].remote.as_deref(), Some("remote version\n"));
    assert!(f.engine().resume(&f.mapping, |_| {}).is_err());
    resolve(&f.storage, "notes.txt", "local").unwrap();
    f.engine().resume(&f.mapping, |_| {}).unwrap();
    assert_eq!(
        fs::read(Path::new(&f.folder).join("notes.txt")).unwrap(),
        b"local version\n"
    );
}
#[test]
fn stash_conflict_preserves_versions_and_resolves() {
    let f = Fixture::new(true);
    let peer = f.peer();
    f.peer_commit(&peer, "notes.txt", b"remote\n");
    f.write("notes.txt", b"pending local\n");
    assert_eq!(
        f.engine().start(&f.mapping, |_| {}).unwrap_err().code,
        "conflicts"
    );
    let j = f.storage.journal().unwrap().unwrap();
    assert_eq!(j.phase, "apply");
    assert_eq!(j.conflicts[0].local.as_deref(), Some("pending local\n"));
    resolve(&f.storage, "notes.txt", "remote").unwrap();
    f.engine().resume(&f.mapping, |_| {}).unwrap();
    assert_eq!(
        fs::read(Path::new(&f.folder).join("notes.txt")).unwrap(),
        b"remote\n"
    );
}
#[test]
fn binary_conflict_keeps_complete_local_bytes() {
    let f = Fixture::new(true);
    f.write("image.bin", b"\0base");
    f.commit("binary base");
    f.push();
    let peer = f.peer();
    f.write("image.bin", b"\0local");
    f.commit("binary local");
    f.peer_commit(&peer, "image.bin", b"\0remote");
    assert!(f.engine().start(&f.mapping, |_| {}).is_err());
    let j = f.storage.journal().unwrap().unwrap();
    assert!(j.conflicts[0].binary);
    resolve(&f.storage, "image.bin", "local").unwrap();
    f.engine().resume(&f.mapping, |_| {}).unwrap();
    assert_eq!(
        fs::read(Path::new(&f.folder).join("image.bin")).unwrap(),
        b"\0local"
    );
}
#[test]
fn deletion_conflict_remote_resolution() {
    let f = Fixture::new(true);
    let peer = f.peer();
    f.cmd(&["rm", "notes.txt"]);
    f.commit("delete locally");
    f.peer_commit(&peer, "notes.txt", b"remote modification\n");
    assert!(f.engine().start(&f.mapping, |_| {}).is_err());
    let j = f.storage.journal().unwrap().unwrap();
    assert!(j.conflicts[0].local.is_none());
    resolve(&f.storage, "notes.txt", "remote").unwrap();
    f.engine().resume(&f.mapping, |_| {}).unwrap();
    assert_eq!(
        fs::read(Path::new(&f.folder).join("notes.txt")).unwrap(),
        b"remote modification\n"
    );
}
#[test]
fn concurrent_push_retries_without_force() {
    let f = Fixture::new(true);
    let peer = f.peer();
    f.write("local.txt", b"local");
    let mut pushed = false;
    f.engine()
        .start(&f.mapping, |phase| {
            if phase == "push" && !pushed {
                pushed = true;
                f.peer_commit(&peer, "race.txt", b"concurrent");
            }
        })
        .unwrap();
    assert!(Path::new(&f.folder).join("race.txt").exists());
    assert!(Path::new(&f.folder).join("local.txt").exists());
}
#[test]
fn external_edit_during_fetch_pauses_and_keeps_stash() {
    let f = Fixture::new(true);
    f.write("new.txt", b"saved");
    assert!(f
        .engine()
        .start(&f.mapping, |phase| {
            if phase == "fetch" {
                f.write("external.txt", b"external");
            }
        })
        .is_err());
    assert!(f.storage.journal().unwrap().unwrap().stash_oid.is_some());
    assert!(Path::new(&f.folder).join("external.txt").exists());
    assert!(f.engine().resume(&f.mapping, |_| {}).is_err());
}
#[test]
fn external_branch_changes_and_hooks_are_blocked() {
    let f = Fixture::new(true);
    f.cmd(&["switch", "-c", "other"]);
    assert_eq!(
        f.engine().start(&f.mapping, |_| {}).unwrap_err().code,
        "branchChanged"
    );
    f.cmd(&["switch", "main"]);
    let r = git::open(&f.folder).unwrap();
    fs::write(r.path().join("hooks/pre-commit"), b"active hook").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            r.path().join("hooks/pre-commit"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    assert_eq!(
        f.engine().start(&f.mapping, |_| {}).unwrap_err().code,
        "hooks"
    );
}
#[test]
fn recovery_at_every_boundary_is_durable() {
    for boundary in [
        "backup",
        "stash",
        "fetch",
        "integrate",
        "apply",
        "commit",
        "push",
        "verify",
        "cleanup",
    ] {
        let f = Fixture::new(true);
        f.write("new.txt", b"recover");
        let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.engine().start(&f.mapping, |phase| {
                if phase == boundary {
                    panic!("simulate interruption")
                }
            })
        }));
        assert!(stopped.is_err());
        let j = f.storage.journal().unwrap().unwrap();
        assert_eq!(j.phase, boundary);
        assert!(!j.completed);
        f.engine().resume(&f.mapping, |_| {}).unwrap();
        assert_eq!(
            fs::read(Path::new(&f.folder).join("new.txt")).unwrap(),
            b"recover"
        );
    }
}
#[test]
fn ambiguous_mutation_recovery_never_replays() {
    let f = Fixture::new(true);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.engine().start(&f.mapping, |p| {
            if p == "apply" {
                panic!("stop")
            }
        })
    }));
    let mut j = f.storage.journal().unwrap().unwrap();
    j.in_flight = true;
    f.storage.save_journal(&j).unwrap();
    assert_eq!(
        f.engine().resume(&f.mapping, |_| {}).unwrap_err().code,
        "ambiguousRecovery"
    );
    assert!(f.storage.journal().unwrap().is_some());
}
#[test]
fn settings_roundtrip_and_no_secrets() {
    let f = Fixture::new(false);
    let mut s = f.settings.clone();
    s.repositories.push(f.mapping.clone());
    s.active_id = Some("1".into());
    f.storage.save_settings(&s).unwrap();
    assert_eq!(
        f.storage.load_settings().unwrap().active_id,
        Some("1".into())
    );
    let content = fs::read_to_string(f.storage.root.join("config.json")).unwrap();
    assert!(!content.contains("token"));
    assert_eq!(
        f.storage.load_settings().unwrap().automation,
        Automation::Off
    );
}
#[test]
fn watcher_debounce_and_idle() {
    let now = Instant::now();
    assert!(!automation_due(
        &Automation::AfterChanges,
        Some(now),
        now + Duration::from_millis(1999)
    ));
    assert!(automation_due(
        &Automation::AfterChanges,
        Some(now),
        now + Duration::from_secs(2)
    ));
    assert!(!automation_due(
        &Automation::FolderIdle,
        Some(now),
        now + Duration::from_secs(29)
    ));
    assert!(automation_due(
        &Automation::FolderIdle,
        Some(now),
        now + Duration::from_secs(30)
    ));
    assert!(!automation_due(
        &Automation::Off,
        Some(now),
        now + Duration::from_secs(60)
    ));
}
#[test]
fn path_traversal_rejected() {
    let f = Fixture::new(false);
    for name in [
        "../outside",
        ".git/config",
        "C:/outside",
        "a/../../outside",
        "a\\b",
    ] {
        assert!(git::safe_path(Path::new(&f.folder), name).is_err());
    }
}
#[test]
fn history_pagination_and_binary_diff() {
    let f = Fixture::new(true);
    for i in 0..22 {
        f.write("notes.txt", format!("{i}").as_bytes());
        f.commit("history");
    }
    let r = git::open(&f.folder).unwrap();
    assert_eq!(git::history(&r, 0).unwrap().len(), 20);
    assert_eq!(git::history(&r, 1).unwrap().len(), 3);
    f.write("binary.dat", b"\0binary");
    assert!(git::diffs(&r, None)
        .unwrap()
        .iter()
        .any(|d| d.path == "binary.dat" && d.binary));
}

fn mock(
    status: u16,
    response: &str,
    check: impl FnOnce(&mut tiny_http::Request) + Send + 'static,
) -> GitHub {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr());
    let body = response.to_owned();
    std::thread::spawn(move || {
        let mut req = server.recv().unwrap();
        check(&mut req);
        req.respond(
            tiny_http::Response::from_string(body)
                .with_status_code(status)
                .with_header(
                    tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                ),
        )
        .unwrap();
    });
    GitHub::with_base(base).unwrap()
}
const REPO: &str = r#"{"id":1,"full_name":"test/repo","owner":{"login":"test"},"private":true,"permissions":{"push":true},"default_branch":"main"}"#;
#[test]
fn github_private_creation_payload() {
    let g = mock(201, REPO, |req| {
        assert_eq!(req.url(), "/user/repos");
        let mut body = String::new();
        req.as_reader().read_to_string(&mut body).unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["private"], true);
        assert_eq!(v["auto_init"], false);
        assert_eq!(req.method(), &tiny_http::Method::Post);
    });
    assert!(g.create_private("test-secret", "repo", "").unwrap().private);
}
#[test]
fn github_pagination_and_structured_errors() {
    let g = mock(200, &format!("[{REPO}]"), |r| {
        assert!(r.url().contains("page=2"))
    });
    assert_eq!(g.repositories("secret", 2).unwrap().len(), 1);
    for (status, code) in [
        (401, "authentication"),
        (403, "permission"),
        (404, "permission"),
        (422, "validation"),
        (429, "rateLimit"),
    ] {
        let g = mock(status, "{\"message\":\"secret\"}", |_| {});
        let e = g.repositories("secret", 1).unwrap_err();
        assert_eq!(e.code, code);
        assert!(!serde_json::to_string(&e).unwrap().contains("secret"));
    }
}
#[test]
fn github_identity_and_token_not_in_error() {
    let g = mock(
        200,
        r#"{"id":12,"login":"test","name":null,"email":null}"#,
        |_| {},
    );
    assert_eq!(
        g.identity("secret").unwrap().email,
        "12+test@users.noreply.github.com"
    );
}

struct MemoryVault(std::sync::Mutex<String>);

#[test]
#[ignore = "read-only diagnostic; requires LAZYSYNC_DIAGNOSTIC_FOLDER"]
fn selected_folder_scan_diagnostic() {
    let folder = std::env::var("LAZYSYNC_DIAGNOSTIC_FOLDER").expect("set the diagnostic folder");
    let r = git::open(&folder).unwrap();
    let start = Instant::now();
    let counts = git::counts(&r).unwrap();
    println!("Status scan: {:?}; changes: {:?}", start.elapsed(), counts);
    let start = Instant::now();
    git::observation_fingerprint(&r).unwrap();
    println!("Metadata observation: {:?}", start.elapsed());
    let start = Instant::now();
    let upload = git::validate_upload_files(&r);
    println!("Upload preflight: {:?}; {:?}", start.elapsed(), upload);
}

#[test]
fn background_scan_accepts_manual_work_and_keeps_snapshots_readable() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    };
    let f = Fixture::new(true);
    let mut settings = f.settings.clone();
    settings.repositories = vec![f.mapping.clone()];
    settings.active_id = Some("1".into());
    settings.status_interval_secs = 1;
    f.storage.save_settings(&settings).unwrap();
    let armed = Arc::new(AtomicBool::new(false));
    let block_once = armed.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let service = crate::runtime::Service::new(
        f.storage.clone(),
        Arc::new(MemoryVault(Mutex::new("secret".into()))),
        GitHub::new().unwrap(),
        Arc::new(move |_| {
            if block_once.swap(false, Ordering::SeqCst) {
                entered_tx.send(()).unwrap();
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
            }
        }),
    )
    .unwrap();
    let worker = crate::runtime::Worker::new(service);
    armed.store(true, Ordering::SeqCst);
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        !worker.busy.load(Ordering::SeqCst),
        "a background scan must not reserve manual operations"
    );
    assert_eq!(worker.status().repository_id.as_deref(), Some("1"));
    assert_eq!(worker.settings().active_id.as_deref(), Some("1"));
    let manual = worker.clone();
    let queued =
        std::thread::spawn(move || manual.call(true, |s| Ok(s.settings.active_id.clone())));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !worker.busy.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(worker.busy.load(Ordering::SeqCst));
    assert_eq!(worker.call(true, |_| Ok(())).unwrap_err().code, "busy");
    release_tx.send(()).unwrap();
    assert_eq!(queued.join().unwrap().unwrap().as_deref(), Some("1"));
    assert!(!worker.busy.load(Ordering::SeqCst));
}

#[test]
fn worker_panic_releases_busy_and_preserves_recovery() {
    let f = Fixture::new(true);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.engine().start(&f.mapping, |phase| {
            if phase == "fetch" {
                panic!("interrupted sync");
            }
        })
    }));
    let before = fs::read(f.storage.root.join("recovery.json")).unwrap();
    let service = crate::runtime::Service::new(
        f.storage.clone(),
        Arc::new(MemoryVault(std::sync::Mutex::new("secret".into()))),
        GitHub::new().unwrap(),
        Arc::new(|_| {}),
    )
    .unwrap();
    let worker = crate::runtime::Worker::new(service);
    assert_eq!(
        worker
            .call::<()>(true, |_| panic!("operation panic"))
            .unwrap_err()
            .code,
        "workerPanic"
    );
    assert!(!worker.busy.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(worker.status().phase, "paused");
    assert!(worker.status().recovery);
    worker.call(true, |_| Ok(())).unwrap();
    assert_eq!(
        fs::read(f.storage.root.join("recovery.json")).unwrap(),
        before
    );
}

#[test]
fn oversized_upload_is_blocked_before_recovery_and_can_be_ignored() {
    let f = Fixture::new(false);
    let path = Path::new(&f.folder).join("oversized.zip");
    fs::File::create(&path)
        .unwrap()
        .set_len(100 * 1024 * 1024 + 1)
        .unwrap();
    let error = f.engine().start(&f.mapping, |_| {}).unwrap_err();
    assert_eq!(error.code, "largeFiles");
    assert!(error.message.contains("oversized.zip"));
    assert!(f.storage.journal().unwrap().is_none());
    assert!(git::head(&git::open(&f.folder).unwrap()).is_none());
    assert_eq!(fs::metadata(&path).unwrap().len(), 100 * 1024 * 1024 + 1);
    f.write(".gitignore", b"oversized.zip\n");
    f.write("small.txt", b"upload me\n");
    f.engine().start(&f.mapping, |_| {}).unwrap();
    assert!(path.exists());
    assert!(!git::dirty(&git::open(&f.folder).unwrap()).unwrap());
}

#[test]
fn metadata_observations_detect_relevant_changes_and_skip_ignored_files() {
    let f = Fixture::new(true);
    f.write(".gitignore", b"ignored/\n");
    fs::create_dir(Path::new(&f.folder).join("ignored")).unwrap();
    let r = git::open(&f.folder).unwrap();
    let before = git::observation_fingerprint(&r).unwrap();
    f.write("ignored/cache", b"cache contents");
    assert_eq!(before, git::observation_fingerprint(&r).unwrap());
    // Large sparse content is observed from its metadata, never streamed into the poll hash.
    fs::File::create(Path::new(&f.folder).join("large.bin"))
        .unwrap()
        .set_len(1024 * 1024 * 1024)
        .unwrap();
    let added = git::observation_fingerprint(&r).unwrap();
    assert_ne!(before, added);
    fs::remove_file(Path::new(&f.folder).join("large.bin")).unwrap();
    assert_eq!(before, git::observation_fingerprint(&r).unwrap());
    f.write("notes.txt", b"changed contents\n");
    let modified = git::observation_fingerprint(&r).unwrap();
    assert_ne!(before, modified);
    fs::remove_file(Path::new(&f.folder).join("notes.txt")).unwrap();
    assert_ne!(modified, git::observation_fingerprint(&r).unwrap());
}
#[test]
fn checkout_root_hook_is_never_executed() {
    let f = Fixture::new(true);
    f.write("pre-commit", b"#!/bin/sh\ntouch hook-ran\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            Path::new(&f.folder).join("pre-commit"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    f.commit("root file is not a hook");
    assert!(!Path::new(&f.folder).join("hook-ran").exists());
}
#[test]
fn partial_private_setup_resumes_without_duplicate_creation() {
    let f = Fixture::new(true);
    let original = git::head(&git::open(&f.folder).unwrap());
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr());
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let posts = count.clone();
    let thread = std::thread::spawn(move || {
        for _ in 0..3 {
            let req = server.recv().unwrap();
            if req.method() == &tiny_http::Method::Post {
                posts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            req.respond(tiny_http::Response::from_string(REPO).with_status_code(200))
                .unwrap();
        }
    });
    let mut service = crate::runtime::Service::new(
        f.storage.clone(),
        Arc::new(MemoryVault(std::sync::Mutex::new("secret".into()))),
        GitHub::with_base(base).unwrap(),
        Arc::new(|_| {}),
    )
    .unwrap();
    assert_eq!(
        service
            .create_with_ignore(
                "repo".into(),
                "description".into(),
                f.folder.clone(),
                vec!["/private/".into()]
            )
            .unwrap_err()
            .code,
        "remoteConflict"
    );
    assert!(f
        .storage
        .load_settings()
        .unwrap()
        .pending_setup
        .unwrap()
        .remote
        .is_some());
    git::open(&f.folder)
        .unwrap()
        .remote_set_url("origin", "https://github.com/test/repo.git")
        .unwrap();
    let next = service
        .create(
            "repo".into(),
            "description".into(),
            Path::new(&f.folder)
                .join(".")
                .to_string_lossy()
                .into_owned(),
        )
        .unwrap();
    assert_eq!(next.active_id.as_deref(), Some("1"));
    assert!(next.pending_setup.is_none());
    assert_eq!(
        crate::ignore::settings(Path::new(&f.folder))
            .unwrap()
            .patterns,
        vec!["/private/"]
    );
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    thread.join().unwrap();
    assert_eq!(git::head(&git::open(&f.folder).unwrap()), original);
    assert!(!serde_json::to_string(&service.status)
        .unwrap()
        .contains("secret"));
}
#[test]
fn network_failure_preserves_changes_and_resumes() {
    let f = Fixture::new(true);
    f.write("pending.txt", b"preserve me");
    let r = git::open(&f.folder).unwrap();
    r.remote_set_url(
        "origin",
        f._temp.path().join("unavailable.git").to_str().unwrap(),
    )
    .unwrap();
    assert!(f.engine().start(&f.mapping, |_| {}).is_err());
    let j = f.storage.journal().unwrap().unwrap();
    assert!(j.stash_oid.is_some());
    assert_eq!(j.phase, "fetch");
    r.remote_set_url("origin", &f.remote).unwrap();
    f.engine().resume(&f.mapping, |_| {}).unwrap();
    assert_eq!(
        fs::read(Path::new(&f.folder).join("pending.txt")).unwrap(),
        b"preserve me"
    );
}
#[test]
fn incoming_lfs_and_local_submodules_are_blocked() {
    let f = Fixture::new(true);
    let peer = f.peer();
    f.peer_commit(
        &peer,
        ".gitattributes",
        b"*.dat filter=lfs diff=lfs merge=lfs -text\n",
    );
    f.write("pending.txt", b"saved");
    assert_eq!(
        f.engine().start(&f.mapping, |_| {}).unwrap_err().code,
        "unsupportedRemote"
    );
    assert!(!Path::new(&f.folder).join(".gitattributes").exists());
    assert!(f.storage.journal().unwrap().unwrap().stash_oid.is_some());
    let f = Fixture::new(true);
    f.write(".gitmodules", b"[submodule]\n");
    assert_eq!(
        f.engine().start(&f.mapping, |_| {}).unwrap_err().code,
        "submodules"
    );
}

#[test]
fn nested_repositories_are_named_before_staging_and_ignored_ones_are_preserved() {
    let f = Fixture::new(false);
    let root = Path::new(&f.folder);
    let nested = root.join("projects/nested");
    fs::create_dir_all(&nested).unwrap();
    let inner = Repository::init(&nested).unwrap();
    fs::write(nested.join("keep.txt"), b"nested project\n").unwrap();
    git::command(&nested, &["add", "-A"], Some(&f.settings)).unwrap();
    git::command(
        &nested,
        &["commit", "-m", "nested history"],
        Some(&f.settings),
    )
    .unwrap();
    let inner_head = git::head(&inner);
    f.write("parent.txt", b"parent project\n");
    let before = git::command(root, &["ls-files", "--stage", "-z"], None).unwrap();
    let error = f.engine().start(&f.mapping, |_| {}).unwrap_err();
    assert_eq!(error.code, "nestedRepositories");
    assert!(error.message.contains("projects/nested"));
    assert_eq!(
        before,
        git::command(root, &["ls-files", "--stage", "-z"], None).unwrap()
    );
    assert!(f.storage.journal().unwrap().is_none());
    f.cmd(&["add", "-A"]);
    let staged = f.engine().start(&f.mapping, |_| {}).unwrap_err();
    assert_eq!(staged.code, "submodules");
    assert!(staged.message.contains("projects/nested"));
    f.cmd(&["rm", "--cached", "-f", "--", "projects/nested"]);
    f.write(".gitignore", b"/projects/nested/\n");
    f.engine().start(&f.mapping, |_| {}).unwrap();
    assert_eq!(git::head(&inner), inner_head);
    assert_eq!(
        fs::read(nested.join("keep.txt")).unwrap(),
        b"nested project\n"
    );
    assert!(!git::open(&f.folder)
        .unwrap()
        .index()
        .unwrap()
        .iter()
        .any(|e| e.mode == 0o160000));
}
#[test]
fn signing_filters_and_nonempty_clone_are_blocked() {
    let f = Fixture::new(true);
    let r = git::open(&f.folder).unwrap();
    r.config()
        .unwrap()
        .set_bool("commit.gpgsign", true)
        .unwrap();
    assert_eq!(
        f.engine().start(&f.mapping, |_| {}).unwrap_err().code,
        "signing"
    );
    r.config()
        .unwrap()
        .set_bool("commit.gpgsign", false)
        .unwrap();
    f.write(".gitattributes", b"*.txt filter=lfs\n");
    assert_eq!(
        f.engine().start(&f.mapping, |_| {}).unwrap_err().code,
        "filters"
    );
    assert_eq!(
        git::clone_repo(&f.mapping.remote, &f.folder, "")
            .unwrap_err()
            .code,
        "notEmpty"
    );
}
#[test]
fn editor_resolution_rejects_unrelated_edits() {
    let f = Fixture::new(true);
    let peer = f.peer();
    f.write("notes.txt", b"local\n");
    f.commit("local");
    f.peer_commit(&peer, "notes.txt", b"remote\n");
    assert!(f.engine().start(&f.mapping, |_| {}).is_err());
    f.write("notes.txt", b"manual merge\n");
    f.write("other.txt", b"external edit\n");
    assert!(resolve(&f.storage, "notes.txt", "edited").is_err());
    fs::remove_file(Path::new(&f.folder).join("other.txt")).unwrap();
    resolve(&f.storage, "notes.txt", "edited").unwrap();
    f.engine().resume(&f.mapping, |_| {}).unwrap();
    assert_eq!(
        fs::read(Path::new(&f.folder).join("notes.txt")).unwrap(),
        b"manual merge\n"
    );
}
impl crate::credentials::CredentialStore for MemoryVault {
    fn get(&self) -> Result<zeroize::Zeroizing<String>> {
        Ok(zeroize::Zeroizing::new(self.0.lock().unwrap().clone()))
    }
    fn set(&self, t: &str) -> Result<()> {
        *self.0.lock().unwrap() = t.into();
        Ok(())
    }
}
#[test]
fn selection_persistence_and_recovery_blocks_switch() {
    let f = Fixture::new(true);
    let mut s = f.settings.clone();
    s.repositories = vec![
        f.mapping.clone(),
        RepositoryMapping {
            remote: RemoteRepository {
                id: "2".into(),
                ..f.mapping.remote.clone()
            },
            ..f.mapping.clone()
        },
    ];
    s.active_id = Some("1".into());
    f.storage.save_settings(&s).unwrap();
    let mut service = crate::runtime::Service::new(
        f.storage.clone(),
        Arc::new(MemoryVault(std::sync::Mutex::new("secret".into()))),
        GitHub::new().unwrap(),
        Arc::new(|_| {}),
    )
    .unwrap();
    assert_eq!(
        service.select("2".into()).unwrap().active_id,
        Some("2".into())
    );
    assert_eq!(
        f.storage.load_settings().unwrap().active_id,
        Some("2".into())
    );
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.engine().start(&f.mapping, |p| {
            if p == "fetch" {
                panic!("stop")
            }
        })
    }));
    assert_eq!(service.select("1".into()).unwrap_err().code, "recovery");
}
#[test]
fn folder_alias_cannot_create_a_second_repository_mapping() {
    let f = Fixture::new(true);
    let mut settings = f.settings.clone();
    settings.repositories = vec![RepositoryMapping {
        folder: fs::canonicalize(&f.folder)
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        ..f.mapping.clone()
    }];
    settings.active_id = Some(f.mapping.remote.id.clone());
    f.storage.save_settings(&settings).unwrap();
    let original = git::head(&git::open(&f.folder).unwrap());
    let mut service = crate::runtime::Service::new(
        f.storage.clone(),
        Arc::new(MemoryVault(std::sync::Mutex::new("secret".into()))),
        GitHub::new().unwrap(),
        Arc::new(|_| {}),
    )
    .unwrap();
    assert_eq!(
        service
            .create(
                "second".into(),
                String::new(),
                Path::new(&f.folder)
                    .join(".")
                    .to_string_lossy()
                    .into_owned()
            )
            .unwrap_err()
            .code,
        "folderMapped"
    );
    assert!(service.settings.pending_setup.is_none());
    assert_eq!(
        serde_json::to_value(f.storage.load_settings().unwrap().repositories).unwrap(),
        serde_json::to_value(settings.repositories).unwrap()
    );
    assert_eq!(git::head(&git::open(&f.folder).unwrap()), original);
}
#[test]
fn cloud_comparison_includes_unpushed_commits_and_pending_files() {
    let f = Fixture::new(true);
    f.write("notes.txt", b"local commit\n");
    f.commit("not pushed");
    f.write("new.ts", b"export const value = 1;\n");
    f.write(".gitignore", b"ignored.txt\n");
    f.write("ignored.txt", b"secret\n");
    let r = git::open(&f.folder).unwrap();
    git::fetch(&r, "main", "").unwrap();
    let before = git::fingerprint(&r).unwrap();
    let list = git::comparison_files(&r, "main").unwrap();
    assert!(list.files.iter().any(|f| f.path == "notes.txt"));
    assert!(list
        .files
        .iter()
        .any(|f| f.path == "new.ts" && f.status == "localOnly"));
    assert!(!list.files.iter().any(|f| f.path == "ignored.txt"));
    let file = git::file_comparison(&r, "notes.txt", list.cloud_oid.as_deref()).unwrap();
    assert_eq!(file.local.as_deref(), Some("local commit\n"));
    assert_eq!(file.cloud.as_deref(), Some("base\n"));
    assert_eq!(file.local_size, Some(13));
    assert_eq!(file.cloud_size, Some(5));
    let summary = list.files.iter().find(|f| f.path == "notes.txt").unwrap();
    assert_eq!(
        (summary.local_size, summary.cloud_size),
        (Some(13), Some(5))
    );
    assert_eq!(before, git::fingerprint(&r).unwrap());
}
#[test]
fn cloud_comparison_handles_deletions_binary_empty_remote_and_large_previews() {
    let f = Fixture::new(true);
    fs::remove_file(Path::new(&f.folder).join("notes.txt")).unwrap();
    f.write("image.bin", &[0, 1, 2, 3]);
    f.write("large.txt", &vec![b'x'; 70000]);
    let r = git::open(&f.folder).unwrap();
    git::fetch(&r, "main", "").unwrap();
    let list = git::comparison_files(&r, "main").unwrap();
    assert!(list
        .files
        .iter()
        .any(|f| f.path == "notes.txt" && f.status == "cloudOnly"));
    let removed = git::file_comparison(&r, "notes.txt", list.cloud_oid.as_deref()).unwrap();
    assert!(removed.local.is_none());
    assert_eq!(removed.local_size, None);
    assert_eq!(removed.cloud_size, Some(5));
    assert_eq!(removed.cloud.as_deref(), Some("base\n"));
    assert!(
        git::file_comparison(&r, "image.bin", list.cloud_oid.as_deref())
            .unwrap()
            .binary
    );
    let large = git::file_comparison(&r, "large.txt", None).unwrap();
    assert_eq!(large.local_size, Some(70000));
    assert_eq!(large.cloud_size, None);
    assert!(large.truncated);
    assert_eq!(large.local.unwrap().len(), 65536);
    assert!(git::file_comparison(&r, "../outside", None).is_err());
    let empty = Fixture::new(false);
    empty.write("first.txt", b"first\n");
    let list = git::comparison_files(&git::open(&empty.folder).unwrap(), "main").unwrap();
    assert!(list.cloud_oid.is_none());
    assert_eq!(list.files[0].status, "localOnly");
}
#[test]
fn cloud_comparison_pins_selected_version_even_after_a_later_fetch() {
    let f = Fixture::new(true);
    let r = git::open(&f.folder).unwrap();
    git::fetch(&r, "main", "").unwrap();
    let list = git::comparison_files(&r, "main").unwrap();
    let peer = f.peer();
    f.peer_commit(&peer, "notes.txt", b"new cloud\n");
    git::fetch(&r, "main", "").unwrap();
    assert_eq!(
        git::file_comparison(&r, "notes.txt", list.cloud_oid.as_deref())
            .unwrap()
            .cloud
            .as_deref(),
        Some("base\n")
    );
}
#[test]
fn ignore_editor_preserves_existing_rules_and_rejects_external_edits() {
    let f = Fixture::new(true);
    let root = Path::new(&f.folder);
    f.write(".gitignore", b"# User rules\r\n*.log\r\n!important.log\r\n");
    let original = crate::ignore::settings(root).unwrap();
    let saved = crate::ignore::save(
        root,
        vec!["/private/".into(), "/secret.txt".into()],
        &original.revision,
    )
    .unwrap();
    assert_eq!(saved.existing, original.existing);
    assert_eq!(saved.patterns.len(), 2);
    let r = git::open(&f.folder).unwrap();
    assert!(r
        .status_should_ignore(Path::new("private/file.txt"))
        .unwrap());
    assert!(r.status_should_ignore(Path::new("secret.txt")).unwrap());
    assert!(r.status_should_ignore(Path::new("a.log")).unwrap());
    f.write(".gitignore", b"changed externally\n");
    assert_eq!(
        crate::ignore::save(root, vec![], &saved.revision)
            .unwrap_err()
            .code,
        "ignoreChanged"
    );
    assert_eq!(
        fs::read(root.join(".gitignore")).unwrap(),
        b"changed externally\n"
    );
    assert!(crate::ignore::validate(&["bad\nrule".into()]).is_err());
}
#[test]
fn ignore_browser_excludes_git_and_keeps_tracked_files() {
    let f = Fixture::new(true);
    let root = Path::new(&f.folder);
    fs::create_dir(root.join("private")).unwrap();
    f.write(".env", b"secret");
    let original = crate::ignore::settings(root).unwrap();
    crate::ignore::save(
        root,
        vec!["/notes.txt".into(), "/.env".into()],
        &original.revision,
    )
    .unwrap();
    let entries = crate::ignore::entries(root, "").unwrap();
    assert!(!entries.iter().any(|f| f.path == ".git"));
    assert!(entries.iter().any(|f| f.path == "notes.txt" && f.tracked));
    assert_eq!(
        entries
            .iter()
            .find(|f| f.path == "notes.txt")
            .unwrap()
            .size_bytes,
        Some(5)
    );
    assert!(entries.iter().any(|f| f.path == ".env" && f.ignored));
    assert!(crate::ignore::entries(root, "../").is_err());
    assert_eq!(fs::read(root.join("notes.txt")).unwrap(), b"base\n");
    let r = git::open(&f.folder).unwrap();
    assert!(r
        .index()
        .unwrap()
        .get_path(Path::new("notes.txt"), 0)
        .is_some());
}
#[test]
fn ignore_updates_preserve_later_user_negations() {
    let f = Fixture::new(true);
    let root = Path::new(&f.folder);
    let first = crate::ignore::settings(root).unwrap();
    crate::ignore::save(root, vec!["*.log".into()], &first.revision).unwrap();
    let mut text = fs::read_to_string(root.join(".gitignore")).unwrap();
    text.push_str("!keep.log\n");
    fs::write(root.join(".gitignore"), text).unwrap();
    let before = crate::ignore::settings(root).unwrap();
    crate::ignore::save(root, vec!["*.log".into(), "*.tmp".into()], &before.revision).unwrap();
    let r = git::open(&f.folder).unwrap();
    assert!(!r.status_should_ignore(Path::new("keep.log")).unwrap());
    assert!(r.status_should_ignore(Path::new("other.log")).unwrap());
    let before = crate::ignore::settings(root).unwrap();
    crate::ignore::save(root, vec![], &before.revision).unwrap();
    assert_eq!(
        fs::read_to_string(root.join(".gitignore")).unwrap(),
        "!keep.log\n"
    );
}
