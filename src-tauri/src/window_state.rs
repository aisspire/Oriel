use oriel_core::window_state::{self, WindowState, WorkArea};
use std::sync::Mutex;
use tauri::{LogicalSize, Manager, PhysicalPosition, WebviewWindow, WindowEvent};

fn capture(window: &WebviewWindow, state: &mut WindowState) -> tauri::Result<()> {
    // Keep the last normal bounds when minimized or maximized, so restore remains useful.
    if window.is_minimized()? {
        return Ok(());
    }
    state.maximized = window.is_maximized()?;
    if !state.maximized && !window.is_fullscreen()? {
        let scale = window.scale_factor()?;
        let size = window.inner_size()?.to_logical::<f64>(scale);
        let position = window.outer_position()?;
        if size.width > 0.0 && size.height > 0.0 {
            state.width = size.width;
            state.height = size.height;
            state.x = position.x;
            state.y = position.y;
        }
    }
    Ok(())
}

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    let root = super::web_pages::development_data_root()
        .map(Ok)
        .unwrap_or_else(|| app.path().app_config_dir())?;
    let path = root.join("window-state.json");
    let (saved, writable) = match window_state::load(&path) {
        Ok(saved) => (saved, true),
        Err(error) => {
            eprintln!("Oriel could not restore window state: {error}");
            (None, false)
        }
    };
    let mut configuration = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == "main")
        .ok_or(tauri::Error::WindowNotFound)?
        .clone();
    if let Some(saved) = &saved {
        // Windows initializes the maximize/restore placement when creating a window.
        // Resizing a hidden window afterwards does not update that normal placement.
        configuration.width = saved.width;
        configuration.height = saved.height;
        if let Some(monitor) = app.available_monitors()?.into_iter().find(|monitor| {
            let area = monitor.work_area();
            i64::from(saved.x) >= i64::from(area.position.x)
                && i64::from(saved.x) < i64::from(area.position.x) + i64::from(area.size.width)
                && i64::from(saved.y) >= i64::from(area.position.y)
                && i64::from(saved.y) < i64::from(area.position.y) + i64::from(area.size.height)
        }) {
            configuration.center = false;
            configuration.x = Some(f64::from(saved.x) / monitor.scale_factor());
            configuration.y = Some(f64::from(saved.y) / monitor.scale_factor());
        }
    }
    let mut builder = tauri::WebviewWindowBuilder::from_config(app, &configuration)?
        .theme(Some(super::web_pages::native_theme(app.handle())));
    if let Some(root) = super::web_pages::development_data_root() {
        builder = builder.data_directory(root.join("shell"));
    }
    let window = builder.build()?;
    #[cfg(target_os = "windows")]
    super::window_frame::apply(&window)?;
    let mut state = WindowState {
        version: 1,
        width: 1100.0,
        height: 760.0,
        x: 0,
        y: 0,
        maximized: false,
    };
    capture(&window, &mut state)?;
    let had_saved_state = saved.is_some();
    if let Some(saved) = saved {
        state = saved;
    }
    let monitors = window.available_monitors()?;
    let to_area = |monitor: &tauri::Monitor| {
        let area = monitor.work_area();
        WorkArea {
            x: area.position.x,
            y: area.position.y,
            width: area.size.width,
            height: area.size.height,
            scale: monitor.scale_factor(),
        }
    };
    let original = monitors
        .iter()
        .find(|monitor| to_area(monitor).contains(state.x, state.y));
    let fallback = window.primary_monitor()?.or(window.current_monitor()?);
    if let Some(monitor) = original.or(fallback.as_ref()) {
        let area = to_area(monitor);
        let scale = window.scale_factor()?;
        let inner = window.inner_size()?;
        let outer = window.outer_size()?;
        let insets = (
            f64::from(outer.width.saturating_sub(inner.width)) / scale,
            f64::from(outer.height.saturating_sub(inner.height)) / scale,
        );
        let configuration = window
            .app_handle()
            .config()
            .app
            .windows
            .iter()
            .find(|config| config.label == window.label());
        let minimum = configuration
            .map(|config| {
                (
                    config.min_width.unwrap_or(0.0),
                    config.min_height.unwrap_or(0.0),
                )
            })
            .unwrap_or((480.0, 600.0));
        state.fit(
            area,
            insets,
            minimum,
            !had_saved_state || original.is_none(),
        );
        window.set_min_size(Some(LogicalSize::new(
            minimum.0.min(state.width),
            minimum.1.min(state.height),
        )))?;
        // Move first so the target monitor's DPI is used when setting the logical size.
        window.set_position(PhysicalPosition::new(state.x, state.y))?;
        window.set_size(LogicalSize::new(state.width, state.height))?;
    }
    let maximized = state.maximized;
    let state = Mutex::new(state);
    if maximized {
        window.maximize()?;
    }
    let observed_window = window.clone();
    window.on_window_event(move |event| {
        if !matches!(
            event,
            WindowEvent::Moved(_)
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::CloseRequested { .. }
        ) {
            return;
        }
        let Ok(mut state) = state.lock() else { return };
        if let Err(error) = capture(&observed_window, &mut state) {
            eprintln!("Oriel could not read window state: {error}");
            return;
        }
        if matches!(event, WindowEvent::CloseRequested { .. }) && writable {
            if let Err(error) = window_state::save(&path, &state) {
                eprintln!("Oriel could not save window state: {error}");
            }
        }
    });
    window.show()?;
    Ok(())
}
