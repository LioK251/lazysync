use crate::{
    models::{AppError, Result},
    window_state::{Dimensions, WindowState, WorkArea},
};
use tauri::{Manager, PhysicalPosition, PhysicalSize, Window};

fn area(monitor: &tauri::Monitor) -> WorkArea {
    let bounds = monitor.work_area();
    WorkArea {
        x: bounds.position.x,
        y: bounds.position.y,
        width: bounds.size.width,
        height: bounds.size.height,
        scale: monitor.scale_factor(),
    }
}
fn monitor(window: &Window) -> Result<tauri::Monitor> {
    window
        .current_monitor()?
        .or(window.primary_monitor()?)
        .ok_or_else(|| {
            AppError::new(
                "monitor",
                "No screen is available.",
                "Move the app to an active screen.",
            )
        })
}
fn pair(size: PhysicalSize<u32>) -> (u32, u32) {
    (size.width, size.height)
}

pub fn capture(window: &Window, state: &WindowState) -> Result<()> {
    state.observe(pair(window.inner_size()?), window.scale_factor()?);
    Ok(())
}
fn apply(
    window: &Window,
    state: &WindowState,
    monitor: &tauri::Monitor,
    expanded: bool,
    desired: Dimensions,
    anchor: Option<PhysicalPosition<f64>>,
) -> Result<()> {
    let area = area(monitor);
    let old = window.outer_position()?;
    let current = pair(window.inner_size()?);
    let target = area.fit(desired, expanded);
    let (x, y) = if let Some(p) = anchor {
        area.position(
            p.x as i32 - target.0 as i32 / 2,
            if p.y < area.y as f64 + area.height as f64 / 2.0 {
                p.y as i32 + 24
            } else {
                p.y as i32 - target.1 as i32 - 24
            },
            target,
        )
    } else {
        // Keep the right edge fixed as the checker expands to the left.
        area.position(old.x + current.0 as i32 - target.0 as i32, old.y, target)
    };
    state.transition(expanded, target, current, area.scale);
    state.set_area(area);
    let (min, max) = area.limits(expanded);
    // Remove the other mode's constraints before applying this mode's bounds.
    window.set_min_size(None::<tauri::LogicalSize<f64>>)?;
    window.set_max_size(None::<tauri::LogicalSize<f64>>)?;
    window.set_min_size(Some(PhysicalSize::new(
        (min.width * area.scale).round() as u32,
        (min.height * area.scale).round() as u32,
    )))?;
    window.set_max_size(Some(PhysicalSize::new(
        (max.width * area.scale).round() as u32,
        (max.height * area.scale).round() as u32,
    )))?;
    window.set_size(PhysicalSize::new(target.0, target.1))?;
    window.set_position(PhysicalPosition::new(x, y))?;
    // Settle using the native result before returning to input handling. A queued
    // target event may be skipped if the user begins dragging immediately.
    let settled = pair(window.inner_size()?);
    state.transition(expanded, settled, settled, window.scale_factor()?);
    Ok(())
}
pub fn set_expanded(window: &Window, state: &WindowState, expanded: bool) -> Result<()> {
    if state.expanded() == expanded {
        return Ok(());
    }
    capture(window, state)?;
    state.flush()?;
    apply(
        window,
        state,
        &monitor(window)?,
        expanded,
        state.desired(expanded),
        None,
    )
}
pub fn resize_by(window: &Window, state: &WindowState, width: f64, height: f64) -> Result<()> {
    if !width.is_finite() || !height.is_finite() || width.abs() > 64.0 || height.abs() > 64.0 {
        return Err(AppError::new(
            "windowSize",
            "Invalid resize step.",
            "Use the resize grip or arrow keys.",
        ));
    }
    let scale = window.scale_factor()?;
    let current = window.inner_size()?;
    let expanded = state.expanded();
    let monitor = monitor(window)?;
    let target = area(&monitor).fit(
        Dimensions::new(
            current.width as f64 / scale + width,
            current.height as f64 / scale + height,
        ),
        expanded,
    );
    if target == pair(current) {
        return Ok(());
    }
    // Keyboard resizing is a user preference, even though set_size is programmatic.
    state.transition(expanded, target, pair(current), monitor.scale_factor());
    window.set_size(PhysicalSize::new(target.0, target.1))?;
    state.remember_user_resize(target, monitor.scale_factor());
    let settled = pair(window.inner_size()?);
    state.transition(expanded, settled, settled, window.scale_factor()?);
    clamp_position(window)?;
    Ok(())
}
pub fn clamp_position(window: &Window) -> Result<()> {
    let monitor = monitor(window)?;
    let old = window.outer_position()?;
    let current = pair(window.inner_size()?);
    let (x, y) = area(&monitor).position(old.x, old.y, current);
    if old.x != x || old.y != y {
        window.set_position(PhysicalPosition::new(x, y))?;
    }
    Ok(())
}
pub fn refit_if_monitor_changed(window: &Window, state: &WindowState) -> Result<()> {
    if state.area_changed(area(&monitor(window)?)) {
        fit(window, state, None)?;
    }
    Ok(())
}
pub fn fit(
    window: &Window,
    state: &WindowState,
    anchor: Option<PhysicalPosition<f64>>,
) -> Result<()> {
    let selected = anchor.and_then(|p| {
        window.available_monitors().ok().and_then(|monitors| {
            monitors.into_iter().find(|m| {
                let o = m.position();
                let s = m.size();
                p.x >= o.x as f64
                    && p.y >= o.y as f64
                    && p.x < o.x as f64 + s.width as f64
                    && p.y < o.y as f64 + s.height as f64
            })
        })
    });
    let monitor = selected.map_or_else(|| monitor(window), Ok)?;
    let expanded = state.expanded();
    apply(
        window,
        state,
        &monitor,
        expanded,
        state.desired(expanded),
        anchor,
    )
}
pub fn flush(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<WindowState>() {
        if let Some(window) = app.get_webview_window("main") {
            let _ = capture(&window.as_ref().window(), &state);
        }
        if state.flush().is_err() {
            use tauri_plugin_notification::NotificationExt;
            let _ = app
                .notification()
                .builder()
                .title("lazysync window size could not be saved")
                .body("Check app configuration folder permissions and disk space.")
                .show();
        }
    }
}
