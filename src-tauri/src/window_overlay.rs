use std::sync::Mutex;
use tauri::{
    App, AppHandle, Emitter, EventTarget, Manager, Webview, WebviewWindow, Window, WindowEvent,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverlayMode {
    Settings,
    Add,
    Edit,
    Preparations,
    Help,
    Passwords,
    #[serde(rename = "tool-menu")]
    ToolMenu,
    #[serde(rename = "page-menu")]
    PageMenu,
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

use super::overlay_state::{Failure, Phase, State};
type OverlayState = State<OverlayRequest>;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayStatus {
    supported: bool,
    ready: bool,
    disabled: Option<OverlayFailure>,
    request: Option<RequestStatus>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestStatus {
    request_id: u32,
    mode: OverlayMode,
    phase: &'static str,
    elapsed_ms: u128,
    failure: Option<OverlayFailure>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverlayFailure {
    stage: String,
    message: String,
    infrastructure: bool,
}
impl From<&Failure> for OverlayFailure {
    fn from(f: &Failure) -> Self {
        Self {
            stage: f.stage.clone(),
            message: f.message.clone(),
            infrastructure: f.infrastructure,
        }
    }
}

// Every transition that touches a window runs on the UI thread. State locks only
// protect data copies; they never cover a window call or an async wait.
async fn on_ui<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(AppHandle) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, mut rx) = tauri::async_runtime::channel(1);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.try_send(f(handle));
    })
    .map_err(|e| e.to_string())?;
    rx.recv().await.ok_or("悬浮层主线程响应中断。")?
}
fn status(app: &AppHandle) -> OverlayStatus {
    let supported = cfg!(windows) && app.get_webview_window("overlay").is_some();
    let runtime = app.state::<Mutex<OverlayState>>();
    let mut state = runtime.lock().unwrap();
    let was_disabled = state.disabled.is_some();
    state.expire();
    if !was_disabled {
        if let (Some(f), Some(r)) = (&state.disabled, &state.request) {
            eprintln!(
                "Oriel overlay request={} mode={:?} failed_stage={} elapsed_ms={} message={}",
                r.id,
                r.value.mode,
                f.stage,
                r.started.elapsed().as_millis(),
                f.message
            );
        }
    }
    OverlayStatus {
        supported,
        ready: state.ready,
        disabled: state.disabled.as_ref().map(Into::into),
        request: state.request.as_ref().map(|r| RequestStatus {
            request_id: r.id,
            mode: r.value.mode.clone(),
            phase: r.phase.as_str(),
            elapsed_ms: r.started.elapsed().as_millis(),
            failure: r.failure.as_ref().map(Into::into),
        }),
    }
}
fn trace(app: &AppHandle, stage: &str) {
    let runtime = app.state::<Mutex<OverlayState>>();
    let state = runtime.lock().unwrap();
    if let Some(r) = &state.request {
        eprintln!(
            "Oriel overlay request={} mode={:?} stage={} elapsed_ms={}",
            r.id,
            r.value.mode,
            stage,
            r.started.elapsed().as_millis()
        );
    } else {
        eprintln!(
            "Oriel overlay stage={stage} elapsed_ms={}",
            state.created_at.elapsed().as_millis()
        );
    }
}
fn require_overlay(caller: &Webview) -> Result<(), String> {
    super::web_pages::authorize(caller)?;
    if caller.label() != "overlay" {
        return Err("仅悬浮窗口可确认或读取呈现请求。".into());
    }
    Ok(())
}
fn focus_main(app: &AppHandle) {
    if let Some(main) = app.get_window("main") {
        let _ = main.set_focus();
    }
    // Native activation can otherwise focus the last page child instead of the
    // host WebView. Restore the host before its DOM trigger receives focus.
    if let Some(host) = app.get_webview("main") {
        let _ = host.set_focus();
    }
}
fn hide_window(app: &AppHandle, request_id: u32) -> Result<(), String> {
    if let Some(overlay) = app.get_webview_window("overlay") {
        overlay.hide().map_err(|e| e.to_string())?;
    }
    let _ = app.emit_to(EventTarget::webview("overlay"), "overlay-reset", request_id);
    let _ = app.emit_to(EventTarget::webview("main"), "overlay-active", false);
    Ok(())
}

pub fn create(app: &App) -> tauri::Result<()> {
    app.manage(Mutex::new(OverlayState::default()));
    let created = std::time::Instant::now();
    eprintln!("Oriel overlay stage=create-start");
    // Other platforms keep using the in-page dialogs.
    #[cfg(not(windows))]
    {
        let _ = created;
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
        let main = app.get_window("main").expect("main window created first");
        let mut builder = tauri::WebviewWindowBuilder::from_config(app, &configuration)?
            .owner_raw(main.hwnd()?)
            .theme(Some(super::web_pages::native_theme(app.handle())));
        if let Some(root) = super::web_pages::development_data_root() {
            builder = builder.data_directory(root.join("shell"));
        }
        let overlay = builder
            .background_color(tauri::window::Color(0, 0, 0, 0))
            .build()?;
        eprintln!(
            "Oriel overlay stage=created elapsed_ms={}",
            created.elapsed().as_millis()
        );
        overlay.set_ignore_cursor_events(false)?;
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

pub fn sync_bounds(main: &Window, overlay: &WebviewWindow) -> tauri::Result<()> {
    // Use the main client area; outer bounds include the native resize frame.
    overlay.set_position(main.inner_position()?)?;
    overlay.set_size(main.inner_size()?)?;
    Ok(())
}

pub fn sync_on_event(main: &Window, event: &WindowEvent) {
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
pub async fn overlay_status(caller: Webview, app: AppHandle) -> Result<OverlayStatus, String> {
    super::web_pages::authorize(&caller)?;
    let main = caller.label() == "main";
    on_ui(app, move |app| {
        let result = status(&app);
        if main && result.ready && result.request.as_ref().is_some_and(|r| r.phase == "queued") {
            // Hidden WebView timers may be throttled. A best-effort script wake
            // prompts another invoke-based pull, never carries request data.
            if let Some(overlay) = app.get_webview_window("overlay") {
                let _ = overlay.eval("document.dispatchEvent(new Event('oriel-overlay-poll'))");
            }
        }
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn overlay_ready(caller: Webview, app: AppHandle) -> Result<OverlayStatus, String> {
    require_overlay(&caller)?;
    on_ui(app, |app| {
        app.state::<Mutex<OverlayState>>().lock().unwrap().ready = true;
        trace(&app, "ready");
        Ok(status(&app))
    })
    .await
}

#[tauri::command]
pub async fn overlay_show(
    caller: Webview,
    app: AppHandle,
    request: OverlayRequest,
) -> Result<OverlayStatus, String> {
    super::web_pages::authorize(&caller)?;
    if caller.label() != "main" {
        return Err("仅主窗口可打开悬浮层。".into());
    }
    on_ui(app, move |app| {
        let overlay = app
            .get_webview_window("overlay")
            .ok_or("悬浮模态窗口未就绪。")?;
        app.state::<Mutex<OverlayState>>()
            .lock()
            .unwrap()
            .submit(request.request_id, request.clone())?;
        trace(&app, "queued");
        // Best effort hint. The renderer also pulls pending requests via invoke.
        let _ = overlay.emit("overlay-open", ());
        Ok(status(&app))
    })
    .await
}

#[tauri::command]
pub async fn overlay_pending(
    caller: Webview,
    app: AppHandle,
) -> Result<Option<OverlayRequest>, String> {
    require_overlay(&caller)?;
    on_ui(app, |app| {
        let request = {
            let runtime = app.state::<Mutex<OverlayState>>();
            let mut state = runtime.lock().unwrap();
            state.expire();
            let request = state
                .request
                .as_ref()
                .filter(|r| matches!(r.phase, Phase::Queued | Phase::Received))
                .map(|r| r.value.clone());
            if let Some(r) = &request {
                state.advance(r.request_id, Phase::Received)?;
            }
            request
        };
        if request.is_some() {
            trace(&app, "received");
        }
        Ok(request)
    })
    .await
}

#[tauri::command]
pub async fn overlay_presented(
    caller: Webview,
    app: AppHandle,
    request_id: u32,
) -> Result<OverlayStatus, String> {
    require_overlay(&caller)?;
    on_ui(app, move |app| {
        app.state::<Mutex<OverlayState>>()
            .lock()
            .unwrap()
            .advance(request_id, Phase::Mounted)?;
        trace(&app, "mounted");
        let result = (|| {
            // main hosts child page WebViews after browsing starts. Tauri's
            // get_webview_window excludes such multi-WebView windows.
            let main = app.get_window("main").ok_or("主窗口不存在。")?;
            let overlay = app.get_webview_window("overlay").ok_or("悬浮层不存在。")?;
            sync_bounds(&main, &overlay).map_err(|e| e.to_string())?;
            eprintln!("Oriel overlay material=transparent-css");
            // No state lock held during any of the native operations above/below.
            app.state::<Mutex<OverlayState>>()
                .lock()
                .unwrap()
                .advance(request_id, Phase::Shown)?;
            overlay.show().map_err(|e| e.to_string())?;
            overlay.set_focus().map_err(|e| e.to_string())?;
            Ok::<(), String>(())
        })();
        if let Err(message) = result {
            trace(&app, &format!("failed:show message={message}"));
            app.state::<Mutex<OverlayState>>().lock().unwrap().fail(
                Some(request_id),
                Failure {
                    stage: "show".into(),
                    message,
                    infrastructure: true,
                },
            );
            let _ = hide_window(&app, request_id);
        } else {
            trace(&app, "shown");
            #[cfg(all(windows, debug_assertions))]
            super::web_pages::trace_overlay_surface(&app, request_id);
            let _ = app.emit_to(EventTarget::webview("main"), "overlay-active", true);
        }
        Ok(status(&app))
    })
    .await
}

#[tauri::command]
pub async fn overlay_fail(
    caller: Webview,
    app: AppHandle,
    request_id: Option<u32>,
    failure: OverlayFailure,
) -> Result<OverlayStatus, String> {
    super::web_pages::authorize(&caller)?;
    if caller.label() == "main" && !failure.infrastructure {
        return Err("主窗口仅可报告悬浮基础设施故障。".into());
    }
    if failure.stage.len() > 80 || failure.message.chars().count() > 4096 {
        return Err("悬浮层诊断过长。".into());
    }
    on_ui(app, move |app| {
        let (accepted, affected) = {
            let runtime = app.state::<Mutex<OverlayState>>();
            let mut state = runtime.lock().unwrap();
            let affected = state
                .request
                .as_ref()
                .filter(|r| r.phase.pending() || r.phase == Phase::Shown)
                .map(|r| r.id);
            let accepted = state.fail(
                request_id,
                Failure {
                    stage: failure.stage.clone(),
                    message: failure.message.clone(),
                    infrastructure: failure.infrastructure,
                },
            );
            (accepted, affected)
        };
        if accepted {
            trace(
                &app,
                &format!("failed:{} message={}", failure.stage, failure.message),
            );
            if let Some(id) = request_id.or(affected) {
                let _ = hide_window(&app, id);
                focus_main(&app);
            }
        }
        Ok(status(&app))
    })
    .await
}

#[tauri::command]
pub async fn overlay_hide(
    caller: Webview,
    app: AppHandle,
    request_id: u32,
) -> Result<OverlayStatus, String> {
    super::web_pages::authorize(&caller)?;
    on_ui(app, move |app| {
        let accepted = app
            .state::<Mutex<OverlayState>>()
            .lock()
            .unwrap()
            .cancel(request_id);
        if accepted {
            hide_window(&app, request_id)?;
            focus_main(&app);
            trace(&app, "closed");
        }
        Ok(status(&app))
    })
    .await
}
