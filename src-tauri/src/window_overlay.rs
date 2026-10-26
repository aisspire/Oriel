use std::sync::Mutex;
use tauri::{App, AppHandle, Emitter, EventTarget, Manager, Webview, WebviewWindow, WindowEvent};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverlayMode {
    Settings,
    Add,
    Edit,
    Preparations,
    Help,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverlayRequest {
    pub request_id: u32,
    pub mode: OverlayMode,
    pub entry_id: Option<String>,
    #[serde(default)]
    pub confirm_remove: bool,
}

#[derive(Default)]
pub struct OverlayState {
    ready: bool,
    request: Option<u32>,
}

pub fn create(app: &App) -> tauri::Result<()> {
    app.manage(Mutex::new(OverlayState::default()));
    // Other platforms keep using the in-page dialogs.
    #[cfg(not(windows))]
    {
        Ok(())
    }
    #[cfg(windows)]
    {
        let configuration = app
            .config()
            .app
            .windows
            .iter()
            .find(|config| config.label == "overlay")
            .cloned();
        let Some(configuration) = configuration else {
            return Ok(());
        };
        let main = app
            .get_webview_window("main")
            .expect("main window created first");
        let mut builder = tauri::WebviewWindowBuilder::from_config(app, &configuration)?
            .owner(&main)?
            .theme(Some(super::web_pages::native_theme(app.handle())));
        if let Some(root) = super::web_pages::development_data_root() {
            builder = builder.data_directory(root.join("shell"));
        }
        let overlay = builder.build()?;
        let app = app.handle().clone();
        overlay.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = app.emit_to(EventTarget::webview("overlay"), "overlay-close-request", ());
            }
        });
        Ok(())
    }
}

pub fn sync_bounds(main: &WebviewWindow, overlay: &WebviewWindow) -> tauri::Result<()> {
    // Use the main client area; outer bounds include the native resize frame.
    overlay.set_position(main.inner_position()?)?;
    overlay.set_size(main.inner_size()?)?;
    Ok(())
}

pub fn sync_on_event(main: &WebviewWindow, event: &WindowEvent) {
    if matches!(
        event,
        WindowEvent::Moved(_) | WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
    ) {
        if let Some(overlay) = main.app_handle().get_webview_window("overlay") {
            if overlay.is_visible().unwrap_or(false) {
                let _ = sync_bounds(main, &overlay);
            }
        }
    }
    if matches!(event, WindowEvent::CloseRequested { .. }) {
        // Destroy rather than close: ordinary overlay close requests only hide it.
        if let Some(overlay) = main.app_handle().get_webview_window("overlay") {
            let _ = overlay.destroy();
        }
    }
}

#[tauri::command]
pub fn overlay_ready(caller: Webview, app: AppHandle) -> Result<bool, String> {
    super::web_pages::authorize(&caller)?;
    let overlay_state = app.state::<Mutex<OverlayState>>();
    let mut state = overlay_state.lock().unwrap();
    if caller.label() == "overlay" {
        state.ready = true;
    }
    Ok(state.ready)
}

#[tauri::command]
pub fn overlay_show(
    caller: Webview,
    app: AppHandle,
    request: OverlayRequest,
) -> Result<(), String> {
    super::web_pages::authorize(&caller)?;
    if caller.label() != "main" {
        return Err("仅主窗口可打开悬浮层。".into());
    }
    let overlay = app
        .get_webview_window("overlay")
        .ok_or("悬浮模态窗口未就绪。")?;
    let overlay_state = app.state::<Mutex<OverlayState>>();
    let mut state = overlay_state.lock().unwrap();
    if !state.ready || state.request.is_some() {
        return Err("悬浮层尚未就绪或正在处理请求。".into());
    }
    state.request = Some(request.request_id);
    drop(state);
    overlay
        .emit("overlay-open", &request)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn overlay_presented(caller: Webview, app: AppHandle, request_id: u32) -> Result<(), String> {
    super::web_pages::authorize(&caller)?;
    let overlay_state = app.state::<Mutex<OverlayState>>();
    let state = overlay_state.lock().unwrap();
    if caller.label() != "overlay" || state.request != Some(request_id) {
        return Err("悬浮层请求已过期。".into());
    }
    let main = app.get_webview_window("main").ok_or("主窗口不存在。")?;
    let overlay = app.get_webview_window("overlay").ok_or("悬浮层不存在。")?;
    sync_bounds(&main, &overlay).map_err(|e| e.to_string())?;
    overlay.show().map_err(|e| e.to_string())?;
    overlay.set_focus().map_err(|e| e.to_string())?;
    app.emit_to(EventTarget::webview("main"), "overlay-active", true)
        .map_err(|e| e.to_string())?;
    app.emit_to(
        EventTarget::webview("main"),
        "overlay-presented",
        request_id,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn overlay_hide(caller: Webview, app: AppHandle, request_id: u32) -> Result<(), String> {
    super::web_pages::authorize(&caller)?;
    let overlay_state = app.state::<Mutex<OverlayState>>();
    let mut state = overlay_state.lock().unwrap();
    if state.request != Some(request_id) {
        return Ok(());
    }
    state.request = None;
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }
    let _ = app.emit_to(EventTarget::webview("overlay"), "overlay-reset", request_id);
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_focus();
    }
    let _ = app.emit_to(EventTarget::webview("main"), "overlay-active", false);
    Ok(())
}

#[tauri::command]
pub fn overlay_supported(caller: Webview, app: AppHandle) -> Result<bool, String> {
    super::web_pages::authorize(&caller)?;
    Ok(cfg!(windows) && app.get_webview_window("overlay").is_some())
}
