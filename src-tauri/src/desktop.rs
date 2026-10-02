use crate::{
    credentials::NativeVault,
    git,
    github::GitHub,
    models::*,
    runtime::{Service, Worker},
    storage::Storage,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State,
};
use tauri_plugin_notification::NotificationExt;

struct DialogGuard(AtomicBool);
async fn work<T: Send + 'static>(
    worker: &Worker,
    mutate: bool,
    f: impl FnOnce(&mut Service) -> Result<T> + Send + 'static,
) -> Result<T> {
    let w = worker.clone();
    tauri::async_runtime::spawn_blocking(move || w.call(mutate, f))
        .await
        .map_err(|_| AppError::new("worker", "Repository worker failed.", "Restart lazysync."))?
}
#[tauri::command]
async fn get_settings(w: State<'_, Worker>) -> Result<Settings> {
    Ok(w.settings())
}
#[tauri::command]
async fn get_status(w: State<'_, Worker>) -> Result<StatusSnapshot> {
    Ok(w.status())
}
#[tauri::command]
async fn authenticate(w: State<'_, Worker>, token: String) -> Result<Identity> {
    work(&w, true, move |s| s.authenticate(token)).await
}
#[tauri::command]
async fn save_settings(
    w: State<'_, Worker>,
    device_name: String,
    commit_name: String,
    commit_email: String,
    automation: Automation,
) -> Result<Settings> {
    work(&w, true, move |s| {
        s.save_settings(device_name, commit_name, commit_email, automation)
    })
    .await
}
#[tauri::command]
async fn list_repositories(w: State<'_, Worker>, page: u32) -> Result<Vec<RemoteRepository>> {
    work(&w, false, move |s| s.list(page)).await
}
#[tauri::command]
async fn connect_repository(
    w: State<'_, Worker>,
    remote: RemoteRepository,
    folder: String,
) -> Result<Settings> {
    work(&w, true, move |s| s.connect(remote, folder, false)).await
}
#[tauri::command]
async fn clone_repository(
    w: State<'_, Worker>,
    remote: RemoteRepository,
    folder: String,
) -> Result<Settings> {
    work(&w, true, move |s| s.connect(remote, folder, true)).await
}
#[tauri::command]
async fn select_repository(w: State<'_, Worker>, id: String) -> Result<Settings> {
    work(&w, true, move |s| s.select(id)).await
}
#[tauri::command]
async fn create_repository(
    w: State<'_, Worker>,
    name: String,
    description: String,
    folder: String,
    ignore_patterns: Vec<String>,
) -> Result<Settings> {
    work(&w, true, move |s| {
        s.create_with_ignore(name, description, folder, ignore_patterns)
    })
    .await
}
#[tauri::command]
async fn get_comparison_files(w: State<'_, Worker>) -> Result<ComparisonList> {
    work(&w, true, |s| s.comparison()).await
}
#[tauri::command]
async fn get_file_comparison(
    w: State<'_, Worker>,
    path: String,
    cloud_oid: Option<String>,
) -> Result<FileComparison> {
    work(&w, false, move |s| {
        git::file_comparison(
            &git::open(&s.active()?.folder)?,
            &path,
            cloud_oid.as_deref(),
        )
    })
    .await
}
#[tauri::command]
async fn get_ignore_settings(
    w: State<'_, Worker>,
    folder: Option<String>,
) -> Result<IgnoreSettings> {
    work(&w, false, move |s| {
        let folder = folder.unwrap_or(s.active().map(|m| m.folder).unwrap_or_default());
        crate::ignore::settings(&PathBuf::from(folder))
    })
    .await
}
#[tauri::command]
async fn get_folder_entries(
    w: State<'_, Worker>,
    folder: Option<String>,
    directory: String,
) -> Result<Vec<FolderEntry>> {
    work(&w, false, move |s| {
        let folder = folder.unwrap_or(s.active().map(|m| m.folder).unwrap_or_default());
        crate::ignore::entries(&PathBuf::from(folder), &directory)
    })
    .await
}
#[tauri::command]
async fn save_ignore_settings(
    w: State<'_, Worker>,
    patterns: Vec<String>,
    revision: String,
) -> Result<IgnoreSettings> {
    work(&w, true, move |s| s.save_ignores(patterns, revision)).await
}
#[tauri::command]
fn set_comparison_open(window: tauri::WebviewWindow, open: bool) -> Result<()> {
    let monitor = window.current_monitor()?.ok_or_else(|| {
        AppError::new(
            "monitor",
            "No screen is available.",
            "Move the app to an active screen.",
        )
    })?;
    let scale = monitor.scale_factor();
    let origin = monitor.position();
    let size = monitor.size();
    let old = window.outer_position()?;
    let old_width = window.inner_size()?.width as i32;
    let width = ((if open { 1120.0 } else { 340.0 }) * scale)
        .min(size.width as f64 - 16.0)
        .max(1.0) as u32;
    let height = ((if open { 640.0 } else { 460.0 }) * scale)
        .min(size.height as f64 - 64.0)
        .max(1.0) as u32;
    let x = (old.x + old_width - width as i32).clamp(
        origin.x + 8,
        origin.x + size.width as i32 - width as i32 - 8,
    );
    let y = old.y.clamp(
        origin.y + 8,
        origin.y + size.height as i32 - height as i32 - 8,
    );
    window.set_size(tauri::PhysicalSize::new(width, height))?;
    window.set_position(tauri::PhysicalPosition::new(x, y))?;
    Ok(())
}
#[tauri::command]
async fn abandon_setup(w: State<'_, Worker>) -> Result<Settings> {
    work(&w, true, |s| s.abandon_setup()).await
}
#[tauri::command]
async fn reconfirm_branch(w: State<'_, Worker>) -> Result<Settings> {
    work(&w, true, |s| s.reconfirm_branch()).await
}
#[tauri::command]
async fn sync_now(w: State<'_, Worker>) -> Result<StatusSnapshot> {
    work(&w, true, |s| s.sync(false)).await
}
#[tauri::command]
async fn continue_sync(w: State<'_, Worker>) -> Result<StatusSnapshot> {
    work(&w, true, |s| s.sync(true)).await
}
#[tauri::command]
async fn get_history(w: State<'_, Worker>, page: u32) -> Result<Vec<Commit>> {
    work(&w, false, move |s| {
        git::history(&git::open(&s.active()?.folder)?, page)
    })
    .await
}
#[tauri::command]
async fn get_diffs(w: State<'_, Worker>, oid: Option<String>) -> Result<Vec<FileDiff>> {
    work(&w, false, move |s| {
        git::diffs(&git::open(&s.active()?.folder)?, oid.as_deref())
    })
    .await
}
#[tauri::command]
async fn get_conflicts(w: State<'_, Worker>) -> Result<Vec<Conflict>> {
    work(&w, false, |s| s.conflicts()).await
}
#[tauri::command]
async fn resolve_conflict(
    w: State<'_, Worker>,
    path: String,
    choice: String,
) -> Result<Vec<Conflict>> {
    work(&w, true, move |s| s.resolve(path, choice)).await
}
#[tauri::command]
async fn finish_recovery(w: State<'_, Worker>) -> Result<StatusSnapshot> {
    work(&w, true, |s| s.finish_recovery()).await
}
#[tauri::command]
fn set_dialog_open(guard: State<'_, DialogGuard>, open: bool) {
    guard.0.store(open, Ordering::SeqCst);
}
#[tauri::command]
fn quit(app: tauri::AppHandle) {
    app.exit(0);
}
#[tauri::command]
async fn open_conflict_editor(w: State<'_, Worker>, path: String) -> Result<()> {
    let folder = work(&w, false, |s| Ok(s.active()?.folder)).await?;
    let p = git::safe_path(&PathBuf::from(folder), &path)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut c = std::process::Command::new(if cfg!(windows) { "notepad.exe" } else { "open" });
        #[cfg(target_os = "macos")]
        c.arg("-t");
        c.arg(p);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x08000000);
        }
        c.spawn().map_err(|_| {
            AppError::new(
                "editor",
                "Could not open the native text editor.",
                "Open this file from your checkout folder.",
            )
        })?;
        Ok(())
    })
    .await
    .map_err(|_| AppError::new("editor", "Editor task failed.", "Open the file manually."))?
}
fn tray_icon(color: [u8; 3]) -> Image<'static> {
    fn near_segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> bool {
        let vx = b.0 - a.0;
        let vy = b.1 - a.1;
        let t = (((x - a.0) * vx + (y - a.1) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
        ((x - a.0 - t * vx).powi(2) + (y - a.1 - t * vy).powi(2)).sqrt() < 0.85
    }
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let dx = x as f32 - 16.0;
            let dy = y as f32 - 16.0;
            let radius = (dx * dx + dy * dy).sqrt();
            let angle = dy.atan2(dx).to_degrees();
            let arc = (radius - 9.33).abs() < 0.85
                && ((-135.0..=-22.0).contains(&angle) || (45.0..=158.0).contains(&angle));
            let segments = [
                ((8.9, 9.1), (6.0, 12.0)),
                ((6.0, 6.0), (6.0, 12.0)),
                ((6.0, 12.0), (12.0, 12.0)),
                ((23.1, 22.9), (26.0, 20.0)),
                ((26.0, 26.0), (26.0, 20.0)),
                ((26.0, 20.0), (20.0, 20.0)),
            ];
            if arc
                || segments
                    .iter()
                    .any(|(a, b)| near_segment(x as f32, y as f32, *a, *b))
            {
                let i = (y * 32 + x) * 4;
                rgba[i..i + 3].copy_from_slice(&color);
                rgba[i + 3] = 255;
            }
        }
    }
    Image::new_owned(rgba, 32, 32)
}
fn show(app: &tauri::AppHandle, position: Option<tauri::PhysicalPosition<f64>>) {
    if let Some(w) = app.get_webview_window("main") {
        if let Some(p) = position {
            if let Ok(monitors) = w.available_monitors() {
                for m in monitors {
                    let origin = m.position();
                    let size = m.size();
                    if p.x >= origin.x as f64
                        && p.x < origin.x as f64 + size.width as f64
                        && p.y >= origin.y as f64
                        && p.y < origin.y as f64 + size.height as f64
                    {
                        let scale = m.scale_factor();
                        let current = w.inner_size().ok();
                        let width = current
                            .map_or((340.0 * scale) as i32, |s| s.width as i32)
                            .min(size.width as i32 - 16);
                        let height = current
                            .map_or((460.0 * scale) as i32, |s| s.height as i32)
                            .min(size.height as i32 - 16);
                        let x = (p.x as i32 - width / 2)
                            .clamp(origin.x + 8, origin.x + size.width as i32 - width - 8);
                        let y = (if p.y < origin.y as f64 + size.height as f64 / 2.0 {
                            p.y as i32 + 24
                        } else {
                            p.y as i32 - height - 24
                        })
                        .clamp(origin.y + 8, origin.y + size.height as i32 - height - 8);
                        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
                        break;
                    }
                }
            }
        }
        let _ = w.show();
        let _ = w.set_focus();
    }
}
pub fn run() {
    git::configure_network_timeouts().expect("initialize Git network timeouts");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app,_,_|show(app,None)))
        .plugin(tauri_plugin_dialog::init()).plugin(tauri_plugin_notification::init())
        .manage(DialogGuard(AtomicBool::new(false)))
        .setup(|app| {
            let handle=app.handle().clone();
            let emit=Arc::new(move|status:StatusSnapshot| {
                let _=handle.emit("lazysync:status:v1",&status);
                if let Some(tray)=handle.tray_by_id("lazysync") {
                    let color=if status.error.is_some() || status.recovery && status.phase=="paused" { [160,160,160] } else { [232,232,232] };
                    let _=tray.set_icon(Some(tray_icon(color))); let _=tray.set_tooltip(Some(format!("lazysync · {}",status.label)));
                }
                if status.error.as_ref().is_some_and(|e|e.code=="conflicts") { let _=handle.notification().builder().title("lazysync needs your attention").body("Open lazysync from the tray to resolve conflicts and continue syncing.").show(); }
            });
            let service=Service::new(Storage::new(app.path().app_config_dir()?)?,Arc::new(NativeVault),GitHub::new()?,emit)?;
            app.manage(Worker::new(service));
            let open=MenuItem::with_id(app,"open","Open lazysync",true,None::<&str>)?;
            let quit=MenuItem::with_id(app,"quit","Quit",true,None::<&str>)?;
            let menu=Menu::with_items(app,&[&open,&quit])?;
            TrayIconBuilder::with_id("lazysync").icon(tray_icon([232,232,232])).tooltip("lazysync").menu(&menu).show_menu_on_left_click(false)
                .on_menu_event(|app,event|match event.id.as_ref() { "quit"=>app.exit(0),"open"=>show(app,None),_=>{} })
                .on_tray_icon_event(|tray,event| { if let TrayIconEvent::Click { button:MouseButton::Left,button_state:MouseButtonState::Up,position,.. }=event { show(tray.app_handle(),Some(position)); } }).build(app)?;
            Ok(())
        })
        .on_window_event(|window,event| {
            if let tauri::WindowEvent::CloseRequested { api,.. } = event { api.prevent_close(); let _=window.hide(); }
        })
        .invoke_handler(tauri::generate_handler![get_settings,get_status,authenticate,save_settings,list_repositories,connect_repository,clone_repository,select_repository,create_repository,abandon_setup,reconfirm_branch,sync_now,continue_sync,get_history,get_diffs,get_conflicts,resolve_conflict,finish_recovery,set_dialog_open,quit,open_conflict_editor,get_comparison_files,get_file_comparison,get_ignore_settings,get_folder_entries,save_ignore_settings,set_comparison_open])
        .run(tauri::generate_context!()).expect("lazysync desktop runtime");
}
