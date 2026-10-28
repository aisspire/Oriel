//! Host-owned web-page capability. Plugins supply declarations, never native handles.
use oriel_core::{
    unique_id, BrowserSidebar, ConfigStore, ProxyPolicy, RegisteredEntry, Snapshot, Theme,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, EventTarget, Manager, Webview};

#[cfg(windows)]
#[path = "web_pages_windows.rs"]
mod platform;
#[cfg(not(windows))]
mod platform {
    use super::*;
    pub struct ProcessWatch;
    pub async fn initialize(_: &Webview, _: AppHandle, _: String, _: String) -> Result<(), String> {
        Err("内置浏览器目前支持 Windows 桌面。".into())
    }
    pub async fn action(_: &Webview, _: String) -> Result<(), String> {
        Err("此平台尚不支持原生浏览操作。".into())
    }
    pub async fn clear_passwords(_: &Webview) -> Result<(), String> {
        Err("此平台尚不支持原生密码管理。".into())
    }
    pub async fn watch(_: &Webview) -> Result<Arc<ProcessWatch>, String> {
        Ok(Arc::new(ProcessWatch))
    }
    pub async fn wait_exit(_: Arc<ProcessWatch>) -> Result<(), String> {
        Ok(())
    }
}

type Result<T> = std::result::Result<T, String>;
const BUILTIN: &str = include_str!("../../plugins/builtin/browser/plugin.json");
const MAX_FAVICON_BYTES: usize = 256 * 1024;

fn valid_favicon(image: &[u8]) -> bool {
    image.len() >= 24
        && image.len() <= MAX_FAVICON_BYTES
        && image.starts_with(b"\x89PNG\r\n\x1a\n")
        && &image[12..16] == b"IHDR"
        && (1..=1024).contains(&u32::from_be_bytes(image[16..20].try_into().unwrap()))
        && (1..=1024).contains(&u32::from_be_bytes(image[20..24].try_into().unwrap()))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebFavicon {
    entry_id: String,
    registered_url: String,
    image: Vec<u8>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSessionState {
    pub entry_id: String,
    pub url: String,
    pub title: String,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct ViewBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl ViewBounds {
    fn validate(&self) -> Result<()> {
        if [self.x, self.y, self.width, self.height]
            .iter()
            .any(|v| !v.is_finite())
            || self.x < 0.
            || self.y < 0.
            || self.width < 1.
            || self.height < 1.
            || self.width > 20000.
            || self.height > 20000.
        {
            return Err("网页区域尺寸无效。".into());
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Session {
    label: String,
    proxy: ProxyPolicy,
    initial_url: Option<String>,
    state: WebSessionState,
}
#[derive(Default)]
struct Display {
    id: Option<String>,
    bounds: ViewBounds,
    visible: bool,
}

pub struct WebPages {
    store: Mutex<ConfigStore>,
    config_path: PathBuf,
    data_root: PathBuf,
    builtin: String,
    sessions: Mutex<HashMap<String, Session>>,
    blocked: Mutex<HashMap<String, String>>,
    retired: Mutex<HashMap<String, Arc<platform::ProcessWatch>>>,
    resume_urls: Mutex<HashMap<String, String>>,
    display: Mutex<Display>,
    operation: tauri::async_runtime::Mutex<()>,
    theme: Mutex<Theme>,
    theme_initialized: std::sync::atomic::AtomicBool,
    services: Arc<Mutex<oriel_core::services::Services>>,
    preparations: Arc<oriel_core::preparation::Preparations>,
    prepared_specs: Mutex<HashMap<String, (oriel_core::preparation::Preparation, Option<String>)>>,
    preparation_inputs:
        Mutex<HashMap<String, (Option<oriel_core::preparation::Preparation>, Option<String>)>>,
    prepared: Mutex<HashMap<String, Option<String>>>,
    preparation_errors: Mutex<HashMap<String, String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostSnapshot {
    #[serde(flatten)]
    snapshot: Snapshot,
    browser_supported: bool,
    config_path: String,
}

impl WebPages {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let mut config_root = app.path().app_config_dir().map_err(|e| e.to_string())?;
        let mut data_root = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
        if let Some(root) = development_data_root() {
            config_root = root.clone();
            data_root = root;
        }
        let config_path = config_root.join("workspace.json");
        let resource = app
            .path()
            .resource_dir()
            .ok()
            .map(|p| p.join("plugins/builtin/browser/plugin.json"));
        let builtin = resource
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_else(|| BUILTIN.to_owned());
        let store = ConfigStore::open(config_path.clone());
        let theme = store.workspace.theme.unwrap_or_default();
        Ok(Self {
            store: Mutex::new(store),
            theme: Mutex::new(theme),
            theme_initialized: std::sync::atomic::AtomicBool::new(false),
            config_path,
            data_root,
            builtin,
            sessions: Mutex::default(),
            blocked: Mutex::default(),
            retired: Mutex::default(),
            resume_urls: Mutex::default(),
            display: Mutex::default(),
            operation: tauri::async_runtime::Mutex::new(()),
            services: Arc::default(),
            preparations: Arc::default(),
            prepared_specs: Mutex::default(),
            preparation_inputs: Mutex::default(),
            prepared: Mutex::default(),
            preparation_errors: Mutex::default(),
        })
    }

    fn snapshot(&self) -> HostSnapshot {
        // An unavailable plugin directory must not prevent the home page or browser opening.
        let directory = self.config_path.parent().unwrap().join("plugins");
        let creation = std::fs::create_dir_all(directory);
        let mut snapshot = self.store.lock().unwrap().snapshot(&self.builtin);
        if let Err(error) = creation {
            snapshot
                .diagnostics
                .push(format!("无法创建默认插件读取目录：{error}"));
        }
        snapshot.theme = Some(*self.theme.lock().unwrap());
        HostSnapshot {
            snapshot,
            browser_supported: cfg!(windows),
            config_path: self.config_path.display().to_string(),
        }
    }

    fn entry(&self, id: &str) -> Result<(RegisteredEntry, ProxyPolicy)> {
        let snapshot = self.store.lock().unwrap().snapshot(&self.builtin);
        let entry = snapshot
            .entries
            .into_iter()
            .find(|e| e.entry.id == id)
            .ok_or("网页入口不存在，请重新加载。")?;
        let proxy = entry.entry.proxy.resolved(&snapshot.default_proxy);
        Ok((entry, proxy))
    }

    async fn close_session(&self, app: &AppHandle, id: &str, resume: bool) -> Result<()> {
        let session = self.sessions.lock().unwrap().get(id).cloned();
        if let Some(session) = session {
            // Obtain a process handle before closing: a PID alone can be reused by Windows.
            let mut error = None;
            let watch = if let Some(view) = app.get_webview(&session.label) {
                view.hide().map_err(|e| e.to_string())?;
                match platform::watch(&view).await {
                    Ok(watch) => Some(watch),
                    Err(cause) => {
                        error = Some(cause);
                        None
                    }
                }
            } else {
                None
            };
            self.sessions.lock().unwrap().remove(id);
            if resume && !session.state.url.is_empty() {
                self.resume_urls
                    .lock()
                    .unwrap()
                    .insert(id.to_owned(), session.state.url);
            }
            for (label, popup) in app.webview_windows() {
                if label.starts_with(&format!("{}-popup-", session.label)) {
                    let _ = popup.hide();
                    if let Err(cause) = popup.destroy() {
                        error = Some(cause.to_string());
                    }
                }
            }
            if let Some(view) = app.get_webview(&session.label) {
                if let Err(cause) = view.close() {
                    error = Some(cause.to_string());
                }
            }
            if let Some(watch) = watch {
                self.retired.lock().unwrap().insert(id.to_owned(), watch);
            }
            if let Some(error) = error {
                let error = format!("无法确认旧网页已关闭，请退出并重新打开 Oriel：{error}");
                self.blocked
                    .lock()
                    .unwrap()
                    .insert(id.to_owned(), error.clone());
                return Err(error);
            }
        }
        self.await_retired(id).await
    }

    async fn await_retired(&self, id: &str) -> Result<()> {
        let watch = self.retired.lock().unwrap().get(id).cloned();
        if let Some(watch) = watch {
            platform::wait_exit(watch).await?;
            self.retired.lock().unwrap().remove(id);
        }
        Ok(())
    }

    async fn reconcile(&self, app: &AppHandle) -> Vec<String> {
        let inputs = self.preparation_inputs.lock().unwrap().clone();
        for (id, expected) in inputs {
            let current = self
                .entry(&id)
                .ok()
                .map(|(e, _)| (e.entry.preparation, e.entry.url));
            if current.as_ref() != Some(&expected) {
                self.preparation_inputs.lock().unwrap().remove(&id);
                self.prepared.lock().unwrap().remove(&id);
                self.prepared_specs.lock().unwrap().remove(&id);
                self.preparation_errors.lock().unwrap().remove(&id);
            }
        }
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .unwrap()
            .iter()
            .map(|(id, value)| (id.clone(), value.clone()))
            .collect();
        let mut errors = vec![];
        for (id, session) in sessions {
            let entry = self.entry(&id);
            let (remove, resume) = match &entry {
                Ok((entry, proxy)) => (
                    proxy != &session.proxy || entry.entry.url != session.initial_url,
                    entry.entry.url == session.initial_url,
                ),
                Err(_) => (true, false),
            };
            if remove {
                if let Err(error) = self.close_session(app, &id, resume).await {
                    errors.push(error);
                }
            }
        }
        errors
    }

    async fn ensure(&self, app: &AppHandle, id: &str) -> Result<Webview> {
        if self.preparations.is_closing() {
            return Err("Oriel 正在退出。".into());
        }
        if !cfg!(windows) {
            return Err("内置浏览器目前支持 Windows 桌面；此预览不提供原生网页浏览。".into());
        }
        if let Some(error) = self.blocked.lock().unwrap().get(id).cloned() {
            return Err(error);
        }
        self.await_retired(id).await?;
        let (entry, proxy) = self.entry(id)?;
        let outdated = self.sessions.lock().unwrap().get(id).map(|session| {
            (
                session.proxy != proxy || session.initial_url != entry.entry.url,
                session.initial_url == entry.entry.url,
            )
        });
        if let Some((true, resume)) = outdated {
            self.close_session(app, id, resume).await?;
        }
        if let Some(label) = self
            .sessions
            .lock()
            .unwrap()
            .get(id)
            .map(|s| s.label.clone())
        {
            return app
                .get_webview(&label)
                .ok_or("网页实例已关闭，请重新加载接入。".into());
        }
        let page_url = if let Some(spec) = &entry.entry.preparation {
            let expected = (spec.clone(), entry.entry.url.clone());
            let current = self.prepared_specs.lock().unwrap().get(id) == Some(&expected);
            let prepared_url = if current {
                self.prepared.lock().unwrap().get(id).cloned().flatten()
            } else {
                None
            };
            // Registered pages remain usable after failure or a configuration edit.
            // Merely showing a page never executes or retries its program.
            Some(
                prepared_url
                    .or_else(|| entry.entry.url.clone())
                    .ok_or("没有登记地址，请先准备程序取得网页地址。")?,
            )
        } else if entry.entry.service.is_some() || !entry.entry.prerequisites.is_empty() {
            self.prepared
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or("请先完成前置资源，失败后显式重试。")?
        } else {
            None
        };
        let profile = self.data_root.join("web-profiles").join(id);
        std::fs::create_dir_all(&profile).map_err(|e| format!("无法创建网页数据目录：{e}"))?;
        let label = format!("web-{}", unique_id());
        let resume_url = self.resume_urls.lock().unwrap().remove(id);
        let url = if entry.entry.preparation.is_some() {
            page_url.or(resume_url)
        } else {
            resume_url.or(page_url)
        }
        .or_else(|| entry.entry.url.clone())
        .unwrap_or_default();
        let state = WebSessionState {
            entry_id: id.to_owned(),
            title: entry.entry.title.clone(),
            url: url.clone(),
            ..Default::default()
        };
        self.sessions.lock().unwrap().insert(
            id.to_owned(),
            Session {
                label: label.clone(),
                proxy: proxy.clone(),
                initial_url: entry.entry.url,
                state,
            },
        );
        let result = (|| {
            let popup_app = app.clone();
            let popup_profile = profile.clone();
            let popup_proxy = proxy.clone();
            let parent_label = label.clone();
            let builder = tauri::webview::WebviewBuilder::new(
                &label,
                tauri::WebviewUrl::External("about:blank".parse().unwrap()),
            )
            .data_directory(profile)
            .disable_drag_drop_handler()
            .on_navigation(|url| {
                url.as_str() == "about:blank" || oriel_core::normalize_url(url.as_str()).is_ok()
            })
            .on_new_window(move |url, features| {
                if oriel_core::normalize_url(url.as_str()).is_err() && url.as_str() != "about:blank"
                {
                    return tauri::webview::NewWindowResponse::Deny;
                }
                let builder = tauri::WebviewWindowBuilder::new(
                    &popup_app,
                    format!("{parent_label}-popup-{}", unique_id()),
                    tauri::WebviewUrl::External("about:blank".parse().unwrap()),
                )
                .title("网页窗口")
                .theme(Some(native_theme(&popup_app)))
                .window_features(features)
                .data_directory(popup_profile.clone())
                .on_navigation(|url| {
                    url.as_str() == "about:blank" || oriel_core::normalize_url(url.as_str()).is_ok()
                });
                let builder = apply_popup_proxy(builder, &popup_proxy);
                match builder.build() {
                    Ok(window) => {
                        platform_popup_init(&window);
                        tauri::webview::NewWindowResponse::Create { window }
                    }
                    Err(_) => tauri::webview::NewWindowResponse::Deny,
                }
            });
            let builder = apply_proxy(builder, &proxy);
            let window = app.get_window("main").ok_or("主窗口不存在。")?;
            window
                .add_child(
                    builder,
                    tauri::LogicalPosition::new(0., 0.),
                    tauri::LogicalSize::new(1., 1.),
                )
                .map_err(|e| format!("无法创建网页：{e}"))
        })();
        let view = match result {
            Ok(view) => view,
            Err(error) => {
                self.sessions.lock().unwrap().remove(id);
                return Err(error);
            }
        };
        view.hide().map_err(|e| e.to_string())?;
        if let Err(error) = platform::initialize(&view, app.clone(), id.to_owned(), label).await {
            let _ = view.close();
            self.sessions.lock().unwrap().remove(id);
            return Err(format!("网页初始化失败：{error}"));
        }
        if !url.is_empty() {
            if let Err(error) = view.navigate(url.parse().map_err(|_| "网页地址无效。")?) {
                let _ = view.close();
                self.sessions.lock().unwrap().remove(id);
                return Err(error.to_string());
            }
        }
        Ok(view)
    }

    fn apply_display(&self, app: &AppHandle) -> Result<()> {
        let display = self.display.lock().unwrap();
        let sessions = self.sessions.lock().unwrap();
        for (id, session) in sessions.iter() {
            if let Some(view) = app.get_webview(&session.label) {
                if display.id.as_ref() == Some(id)
                    && display.visible
                    && !session.state.url.is_empty()
                {
                    let b = display.bounds;
                    view.set_bounds(tauri::Rect {
                        position: tauri::LogicalPosition::new(b.x, b.y).into(),
                        size: tauri::LogicalSize::new(b.width, b.height).into(),
                    })
                    .map_err(|e| e.to_string())?;
                    view.show().map_err(|e| e.to_string())?;
                } else {
                    view.hide().map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }
}

/// Debug builds can isolate all application data for native smoke tests. Release ignores it.
pub fn development_data_root() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        std::env::var_os("ORIEL_DEV_DATA_DIR")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    } else {
        None
    }
}

// Debug isolation only: observe the actual controller, not the requested Display
// flag. No new IPC permission or external-page API is introduced for acceptance.
#[cfg(all(windows, debug_assertions))]
pub(super) fn trace_overlay_surface(app: &AppHandle, request_id: u32) {
    if development_data_root().is_none() {
        return;
    }
    let runtime = app.state::<WebPages>();
    let id = runtime.display.lock().unwrap().id.clone();
    let Some(id) = id else { return };
    let label = runtime
        .sessions
        .lock()
        .unwrap()
        .get(&id)
        .map(|s| s.label.clone());
    if let Some(view) = label.and_then(|label| app.get_webview(&label)) {
        platform::trace_overlay_visibility(&view, request_id, id);
    }
}

// Explicit browser arguments are needed for direct mode; preserve Wry's default arguments.
fn proxy_arguments(proxy: &ProxyPolicy) -> Option<String> {
    const DEFAULT_ARGUMENTS: &str =
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";
    match proxy {
        ProxyPolicy::Direct => Some(format!("{DEFAULT_ARGUMENTS} --no-proxy-server")),
        ProxyPolicy::Custom { .. } => Some(format!(
            "{DEFAULT_ARGUMENTS} --proxy-server={}",
            proxy
                .endpoint()
                .expect("validated proxy")
                .expect("custom endpoint")
        )),
        _ => None,
    }
}
fn apply_proxy(
    mut builder: tauri::webview::WebviewBuilder<tauri::Wry>,
    proxy: &ProxyPolicy,
) -> tauri::webview::WebviewBuilder<tauri::Wry> {
    if let Some(arguments) = proxy_arguments(proxy) {
        builder = builder.additional_browser_args(&arguments);
    }
    builder
}
fn apply_popup_proxy<'a>(
    mut builder: tauri::WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
    proxy: &ProxyPolicy,
) -> tauri::WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    if let Some(arguments) = proxy_arguments(proxy) {
        builder = builder.additional_browser_args(&arguments);
    }
    builder
}
fn platform_popup_init(window: &tauri::WebviewWindow) {
    #[cfg(windows)]
    platform::initialize_popup(window);
    #[cfg(not(windows))]
    let _ = window;
}

/// Every application command has a caller guard in addition to Tauri's per-view ACL.
pub(crate) fn authorize(caller: &Webview) -> Result<()> {
    let url = caller.url().map_err(|_| "无法确认宿主调用来源。")?;
    let local = if cfg!(debug_assertions) {
        url.scheme() == "http" && url.host_str() == Some("127.0.0.1") && url.port() == Some(1420)
    } else {
        url.scheme() == "tauri" && url.host_str() == Some("localhost")
            || matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost")
    };
    if (!matches!(caller.label(), "main" | "overlay")) || !local {
        return Err("此网页无权调用宿主管理能力。".into());
    }
    Ok(())
}

pub(super) fn update_state(
    app: &AppHandle,
    id: &str,
    label: &str,
    update: impl FnOnce(&mut WebSessionState),
) {
    let runtime = app.state::<WebPages>();
    let state = {
        let mut sessions = runtime.sessions.lock().unwrap();
        let Some(session) = sessions.get_mut(id).filter(|s| s.label == label) else {
            return;
        };
        update(&mut session.state);
        session.state.clone()
    };
    let _ = app.emit_to(EventTarget::webview("main"), "web-session", state);
}

#[cfg(windows)]
fn favicon_registration(app: &AppHandle, id: &str, label: &str) -> Option<String> {
    app.state::<WebPages>()
        .sessions
        .lock()
        .unwrap()
        .get(id)
        .filter(|session| session.label == label)?
        .initial_url
        .clone()
}

#[cfg(windows)]
fn publish_favicon(app: &AppHandle, id: &str, label: &str, registered_url: &str, image: Vec<u8>) {
    if !valid_favicon(&image) {
        return;
    }
    let runtime = app.state::<WebPages>();
    let valid = runtime
        .sessions
        .lock()
        .unwrap()
        .get(id)
        .is_some_and(|session| {
            session.label == label
                && session.initial_url.as_deref() == Some(registered_url)
                && oriel_core::same_web_origin(&session.state.url, registered_url)
        });
    if !valid
        || !runtime
            .entry(id)
            .is_ok_and(|(entry, _)| entry.entry.url.as_deref() == Some(registered_url))
    {
        return;
    }
    // Keep the registered URL and PNG in one replaceable cache record.
    let directory = runtime.data_root.join("favicons");
    let pending = directory.join(format!("{}.tmp", unique_id()));
    let mut cached = registered_url.as_bytes().to_vec();
    cached.push(b'\n');
    cached.extend_from_slice(&image);
    let _ = std::fs::create_dir_all(&directory)
        .and_then(|_| std::fs::write(&pending, cached))
        .and_then(|_| std::fs::rename(&pending, directory.join(id)));
    let _ = std::fs::remove_file(pending);
    // Also save as {entryId}.png for direct asset and external viewer access
    let pending_png = directory.join(format!("{}.png.tmp", unique_id()));
    let _ = std::fs::write(&pending_png, &image)
        .and_then(|_| std::fs::rename(&pending_png, directory.join(format!("{id}.png"))));
    let _ = std::fs::remove_file(pending_png);
    let _ = app.emit_to(
        EventTarget::webview("main"),
        "web-favicon",
        WebFavicon {
            entry_id: id.to_owned(),
            registered_url: registered_url.to_owned(),
            image,
        },
    );
}

fn to_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) & 0x3F] as char);
        out.push(TABLE[(triple >> 12) & 0x3F] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(triple >> 6) & 0x3F] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[triple & 0x3F] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaviconResult {
    entry_id: String,
    registered_url: String,
    data_url: String,
}

#[tauri::command]
pub fn web_favicon(
    caller: Webview,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
) -> Result<Option<FaviconResult>> {
    authorize(&caller)?;
    let (entry, _) = match runtime.entry(&entry_id) {
        Ok(res) => res,
        Err(_) => return Ok(None),
    };
    let url = match entry.entry.url.as_deref() {
        Some(u) => u,
        None => return Ok(None),
    };
    let directory = runtime.data_root.join("favicons");
    let path = directory.join(&entry_id);
    if std::fs::metadata(&path).ok().map(|m| m.len()).unwrap_or(0)
        > (MAX_FAVICON_BYTES + 8193) as u64
    {
        return Ok(None);
    }
    let cached = match std::fs::read(&path) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    let separator = match cached.iter().position(|byte| *byte == b'\n') {
        Some(s) => s,
        None => return Ok(None),
    };
    let origin_url = match std::str::from_utf8(&cached[..separator]) {
        Ok(u) => u,
        Err(_) => return Ok(None),
    };
    let image = &cached[separator + 1..];
    if oriel_core::same_web_origin(url, origin_url) && valid_favicon(image) {
        let png_path = directory.join(format!("{entry_id}.png"));
        if !png_path.exists() {
            let _ = std::fs::write(&png_path, image);
        }
        Ok(Some(FaviconResult {
            entry_id,
            registered_url: url.to_owned(),
            data_url: format!("data:image/png;base64,{}", to_base64(image)),
        }))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub fn workspace_snapshot(
    caller: Webview,
    runtime: tauri::State<'_, WebPages>,
) -> Result<HostSnapshot> {
    authorize(&caller)?;
    Ok(runtime.snapshot())
}

#[tauri::command]
pub async fn workspace_reload(
    caller: Webview,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
) -> Result<HostSnapshot> {
    authorize(&caller)?;
    let _operation = runtime.operation.lock().await;
    *runtime.store.lock().unwrap() = ConfigStore::open(runtime.config_path.clone());
    let loaded_theme = runtime.store.lock().unwrap().workspace.theme;
    let mut theme_errors = vec![];
    if let Some(theme) = loaded_theme {
        *runtime.theme.lock().unwrap() = theme;
        theme_errors = propagate_theme(&app).await;
        let result = ThemeResult {
            theme,
            diagnostics: theme_errors.clone(),
            persisted: true,
        };
        for label in ["main", "overlay"] {
            let _ = app.emit_to(EventTarget::webview(label), "workspace-theme", &result);
        }
    }
    let errors = runtime.reconcile(&app).await;
    let mut snapshot = runtime.snapshot();
    snapshot.snapshot.diagnostics.extend(errors);
    snapshot.snapshot.diagnostics.extend(theme_errors);
    Ok(snapshot)
}

#[tauri::command]
pub async fn workspace_save_entry(
    caller: Webview,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    id: Option<String>,
    title: String,
    url: String,
    proxy: ProxyPolicy,
    preparation: Option<oriel_core::preparation::Preparation>,
    replace_preparation: Option<bool>,
) -> Result<HostSnapshot> {
    authorize(&caller)?;
    let _operation = runtime.operation.lock().await;
    runtime.store.lock().unwrap().save_entry_prepared(
        id,
        title,
        url,
        proxy,
        preparation,
        replace_preparation.unwrap_or(false),
    )?;
    let errors = runtime.reconcile(&app).await;
    let mut snapshot = runtime.snapshot();
    snapshot.snapshot.diagnostics.extend(errors);
    Ok(snapshot)
}

#[tauri::command]
pub async fn workspace_delete_entry(
    caller: Webview,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    id: String,
) -> Result<HostSnapshot> {
    authorize(&caller)?;
    let _operation = runtime.operation.lock().await;
    if !runtime
        .store
        .lock()
        .unwrap()
        .workspace
        .web_entries
        .iter()
        .any(|e| e.id == id)
    {
        return Err("只能移除自定义网页。".into());
    }
    let preparations = runtime.preparations.clone();
    let stopping_id = id.clone();
    tauri::async_runtime::spawn_blocking(move || preparations.retire(&stopping_id))
        .await
        .map_err(|e| e.to_string())??;
    if let Err(error) = runtime.store.lock().unwrap().delete_entry(&id) {
        runtime.preparations.allow(&id);
        return Err(error);
    }
    runtime.prepared.lock().unwrap().remove(&id);
    runtime.prepared_specs.lock().unwrap().remove(&id);
    runtime.preparation_errors.lock().unwrap().remove(&id);
    let errors = runtime.reconcile(&app).await;
    let mut snapshot = runtime.snapshot();
    snapshot.snapshot.diagnostics.extend(errors);
    Ok(snapshot)
}

#[tauri::command]
pub async fn workspace_reorder_entries(
    caller: Webview,
    runtime: tauri::State<'_, WebPages>,
    ids: Vec<String>,
) -> Result<HostSnapshot> {
    authorize(&caller)?;
    let _operation = runtime.operation.lock().await;
    runtime.store.lock().unwrap().reorder_entries(ids)?;
    Ok(runtime.snapshot())
}

#[tauri::command]
pub async fn workspace_browser_sidebar(
    caller: Webview,
    runtime: tauri::State<'_, WebPages>,
    sidebar: BrowserSidebar,
) -> Result<HostSnapshot> {
    authorize(&caller)?;
    let _operation = runtime.operation.lock().await;
    runtime
        .store
        .lock()
        .unwrap()
        .save_browser_sidebar(sidebar)?;
    Ok(runtime.snapshot())
}

#[tauri::command]
pub async fn workspace_settings(
    caller: Webview,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    default_proxy: ProxyPolicy,
    plugin_directories: Vec<PathBuf>,
    plugin_roots: Option<Vec<PathBuf>>,
) -> Result<HostSnapshot> {
    authorize(&caller)?;
    let _operation = runtime.operation.lock().await;
    {
        let mut store = runtime.store.lock().unwrap();
        let mut next = store.workspace.clone();
        next.default_proxy = default_proxy;
        next.plugin_directories = plugin_directories;
        if let Some(roots) = plugin_roots {
            next.plugin_roots = roots;
        }
        store.commit(next)?;
    }
    let errors = runtime.reconcile(&app).await;
    let mut snapshot = runtime.snapshot();
    snapshot.snapshot.diagnostics.extend(errors);
    Ok(snapshot)
}

#[tauri::command]
pub fn plugin_documentation(
    caller: Webview,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
) -> Result<String> {
    authorize(&caller)?;
    runtime
        .store
        .lock()
        .unwrap()
        .plugin_documentation(&runtime.builtin, &entry_id)
}

#[tauri::command]
pub async fn web_show(
    caller: Webview,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    entry_id: Option<String>,
    bounds: ViewBounds,
    visible: bool,
) -> Result<Option<WebSessionState>> {
    authorize(&caller)?;
    bounds.validate()?;
    let _operation = runtime.operation.lock().await;
    *runtime.display.lock().unwrap() = Display {
        id: entry_id.clone(),
        bounds,
        visible,
    };
    // Hide the old native surface before creating another one or opening a local dialog.
    runtime.apply_display(&app)?;
    if let Some(id) = entry_id {
        runtime.entry(&id)?;
        if visible {
            runtime.ensure(&app, &id).await?;
            runtime.apply_display(&app)?;
        }
        return Ok(runtime
            .sessions
            .lock()
            .unwrap()
            .get(&id)
            .map(|s| s.state.clone()));
    }
    Ok(None)
}

#[tauri::command]
pub async fn web_navigate(
    caller: Webview,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
    url: String,
) -> Result<()> {
    authorize(&caller)?;
    let url = oriel_core::normalize_url(&url)?;
    let _operation = runtime.operation.lock().await;
    let view = runtime.ensure(&app, &entry_id).await?;
    view.navigate(url.parse().map_err(|_| "网页地址无效。")?)
        .map_err(|e| e.to_string())?;
    update_state(&app, &entry_id, view.label(), |state| {
        state.url = url;
        state.loading = true;
        state.error = None;
    });
    runtime.apply_display(&app)
}

// Read diagnostics without creating, moving or changing visibility of a page.
#[tauri::command]
pub fn web_session_state(
    caller: Webview,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
) -> Result<Option<WebSessionState>> {
    authorize(&caller)?;
    runtime.entry(&entry_id)?;
    Ok(runtime
        .sessions
        .lock()
        .unwrap()
        .get(&entry_id)
        .map(|s| s.state.clone()))
}

#[tauri::command]
pub async fn web_action(
    caller: Webview,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
    action: String,
) -> Result<()> {
    authorize(&caller)?;
    if !matches!(action.as_str(), "back" | "forward" | "reload" | "stop") {
        return Err("不支持的浏览操作。".into());
    }
    let _operation = runtime.operation.lock().await;
    let view = runtime.ensure(&app, &entry_id).await?;
    platform::action(&view, action).await
}

#[tauri::command]
pub async fn web_clear_passwords(
    caller: Webview,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
) -> Result<()> {
    authorize(&caller)?;
    let _operation = runtime.operation.lock().await;
    let view = runtime.ensure(&app, &entry_id).await?;
    platform::clear_passwords(&view).await?;
    view.reload().map_err(|e| e.to_string())
}

// Waiting for preparation never holds the surface lock or blocks other plugins.
#[tauri::command]
pub async fn web_prepare(
    caller: Webview,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
    retry: bool,
) -> Result<()> {
    authorize(&caller)?;
    if runtime.preparations.is_closing() {
        return Err("Oriel 正在退出。".into());
    }
    let (entry, _) = runtime.entry(&entry_id)?;
    if let Some(spec) = &entry.entry.preparation {
        // Existing pages survive edits and stop; only explicit preparation replaces them.
        if !retry && runtime.sessions.lock().unwrap().contains_key(&entry_id) {
            return Ok(());
        }
        runtime.preparation_inputs.lock().unwrap().insert(
            entry_id.clone(),
            (entry.entry.preparation.clone(), entry.entry.url.clone()),
        );
        let result = match spec.resolve(entry.directory.as_deref()) {
            Ok(resolved) => {
                let manager = runtime.preparations.clone();
                let id = entry_id.clone();
                let title = entry.entry.title.clone();
                let page_url = entry.entry.url.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    manager.prepare_with_page_url(
                        &id,
                        &title,
                        resolved,
                        retry,
                        crate::preparation_platform::launch,
                        page_url,
                    )
                })
                .await
                .map_err(|e| e.to_string())?
            }
            Err(error) => Err(oriel_core::preparation::PreparationFailure::from(error)),
        };
        let _operation = runtime.operation.lock().await;
        if runtime.preparations.is_closing() {
            return Err("Oriel 正在退出。".into());
        }
        let (current, _) = runtime.entry(&entry_id)?;
        if current.entry.preparation != entry.entry.preparation
            || current.entry.url != entry.entry.url
            || current.directory != entry.directory
        {
            return Err("准备期间配置已变更，请重新准备并加载。".into());
        }
        let generation = match &result {
            Ok(r) => Some(r.generation),
            Err(e) => e.generation,
        };
        if let Some(generation) = generation {
            if runtime
                .preparations
                .views()
                .iter()
                .find(|v| v.entry_id == entry_id)
                .is_none_or(|v| v.generation != generation)
            {
                return Err("本次准备结果已被新运行替代。".into());
            }
        }
        let program_result = result.map_err(|e| e.error).and_then(|result| {
            let view = runtime
                .preparations
                .views()
                .into_iter()
                .find(|v| v.entry_id == entry_id);
            if !view.is_some_and(|v| {
                matches!(
                    v.state,
                    oriel_core::preparation::State::Ready
                        | oriel_core::preparation::State::Completed
                )
            }) {
                return Err("前置程序已停止或失败，请重新准备。".into());
            }
            Ok(result.url)
        });
        let target =
            oriel_core::preparation::page_target(entry.entry.url.as_deref(), program_result);
        let target = match target {
            Ok(target) => target,
            Err(error) => {
                if let Some(generation) = generation {
                    runtime
                        .preparations
                        .reject_result(&entry_id, generation, &error);
                }
                runtime.prepared.lock().unwrap().remove(&entry_id);
                runtime.prepared_specs.lock().unwrap().remove(&entry_id);
                runtime
                    .preparation_errors
                    .lock()
                    .unwrap()
                    .insert(entry_id, error.clone());
                return Err(error);
            }
        };
        if let Some(error) = target.preparation_error {
            if let Some(generation) = generation {
                runtime
                    .preparations
                    .reject_result(&entry_id, generation, &error);
            }
            runtime
                .preparation_errors
                .lock()
                .unwrap()
                .insert(entry_id.clone(), error);
        } else {
            runtime.preparation_errors.lock().unwrap().remove(&entry_id);
        }
        let url = target.url;
        runtime.resume_urls.lock().unwrap().remove(&entry_id);
        runtime
            .prepared
            .lock()
            .unwrap()
            .insert(entry_id.clone(), Some(url.clone()));
        runtime
            .prepared_specs
            .lock()
            .unwrap()
            .insert(entry_id.clone(), (spec.clone(), entry.entry.url.clone()));
        let session = runtime.sessions.lock().unwrap().get(&entry_id).cloned();
        if retry {
            if let Some(session) = session {
                let view = app.get_webview(&session.label).ok_or("网页实例已关闭。")?;
                if session.state.url == url {
                    view.reload().map_err(|e| e.to_string())?;
                } else {
                    view.navigate(url.parse().map_err(|_| "网页地址无效。")?)
                        .map_err(|e| e.to_string())?;
                }
                if let Some(session) = runtime.sessions.lock().unwrap().get_mut(&entry_id) {
                    session.state.url = url;
                    session.state.error = None;
                }
            }
        }
        let _ = app.emit_to(EventTarget::webview("main"), "web-prepared", &entry_id);
        return Ok(());
    }
    // Legacy service semantics remain compatible, including existing-page reuse.
    if runtime.sessions.lock().unwrap().contains_key(&entry_id) {
        return Ok(());
    }
    if retry {
        runtime.preparation_errors.lock().unwrap().remove(&entry_id);
    }
    if let Some(error) = runtime
        .preparation_errors
        .lock()
        .unwrap()
        .get(&entry_id)
        .cloned()
    {
        return Err(error);
    }
    let declarations = runtime
        .store
        .lock()
        .unwrap()
        .snapshot(&runtime.builtin)
        .services;
    let services = runtime.services.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        oriel_core::services::Services::prepare(&services, &declarations, &entry.entry)
    })
    .await
    .map_err(|e| e.to_string())?;
    let page = match result {
        Ok(page) => page,
        Err(error) => {
            runtime
                .preparation_errors
                .lock()
                .unwrap()
                .insert(entry_id, error.clone());
            return Err(error);
        }
    };
    runtime.prepared.lock().unwrap().insert(entry_id, page);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationStatus {
    #[serde(flatten)]
    run: oriel_core::preparation::RunView,
    configuration_changed: bool,
}
#[tauri::command]
pub fn preparation_states(
    caller: Webview,
    runtime: tauri::State<'_, WebPages>,
) -> Result<Vec<PreparationStatus>> {
    authorize(&caller)?;
    let errors = runtime.preparation_errors.lock().unwrap().clone();
    let mut runs = runtime.preparations.views();
    for (id, error) in &errors {
        if !runs.iter().any(|r| &r.entry_id == id) {
            if let Ok((entry, _)) = runtime.entry(id) {
                if entry.entry.preparation.is_some() {
                    runs.push(oriel_core::preparation::RunView {
                        entry_id: id.clone(),
                        title: entry.entry.title,
                        generation: 0,
                        state: oriel_core::preparation::State::Failed,
                        url: None,
                        error: Some(error.clone()),
                        exit_code: None,
                        cleaned: true,
                        logs: Default::default(),
                    });
                }
            }
        }
    }
    Ok(runs
        .into_iter()
        .map(|mut run| {
            let configuration_changed = run.generation != 0
                && runtime
                    .entry(&run.entry_id)
                    .ok()
                    .and_then(|(entry, _)| {
                        let p = entry
                            .entry
                            .preparation
                            .as_ref()?
                            .resolve(entry.directory.as_deref())
                            .ok()?;
                        Some((p, entry.entry.url))
                    })
                    .is_none_or(|(p, url)| {
                        !runtime
                            .preparations
                            .matches(&run.entry_id, &p, url.as_deref())
                    });
            if let Some(error) = errors.get(&run.entry_id) {
                run.error = Some(error.clone());
            }
            run.logs.clear();
            PreparationStatus {
                run,
                configuration_changed,
            }
        })
        .collect())
}
#[tauri::command]
pub fn preparation_logs(
    caller: Webview,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
) -> Result<Vec<String>> {
    authorize(&caller)?;
    Ok(runtime
        .preparations
        .views()
        .into_iter()
        .find(|v| v.entry_id == entry_id)
        .map(|v| v.logs.into_iter().collect())
        .unwrap_or_default())
}
#[tauri::command]
pub async fn preparation_stop(
    caller: Webview,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
) -> Result<()> {
    authorize(&caller)?;
    let manager = runtime.preparations.clone();
    let id = entry_id.clone();
    tauri::async_runtime::spawn_blocking(move || manager.stop(&id))
        .await
        .map_err(|e| e.to_string())??;
    let _operation = runtime.operation.lock().await;
    runtime.prepared.lock().unwrap().remove(&entry_id);
    runtime.prepared_specs.lock().unwrap().remove(&entry_id);
    Ok(())
}
#[tauri::command]
pub async fn preparation_pick_file(caller: Webview) -> Result<Option<String>> {
    authorize(&caller)?;
    #[cfg(windows)]
    let owner = caller.window().hwnd().map_err(|e| e.to_string())?.0 as usize;
    #[cfg(not(windows))]
    let owner = 0;
    tauri::async_runtime::spawn_blocking(move || crate::preparation_platform::pick_file(owner))
        .await
        .map_err(|e| e.to_string())?
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeResult {
    theme: Theme,
    diagnostics: Vec<String>,
    persisted: bool,
}
pub fn native_theme(app: &AppHandle) -> tauri::Theme {
    match *app.state::<WebPages>().theme.lock().unwrap() {
        Theme::Light => tauri::Theme::Light,
        Theme::Dark => tauri::Theme::Dark,
    }
}
#[tauri::command]
pub async fn workspace_theme(
    caller: Webview,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    theme: Option<Theme>,
    legacy: Option<Theme>,
) -> Result<ThemeResult> {
    authorize(&caller)?;
    let _operation = runtime.operation.lock().await;
    let initialized = runtime
        .theme_initialized
        .swap(true, std::sync::atomic::Ordering::SeqCst);
    let saved = runtime.store.lock().unwrap().workspace.theme;
    let selected = theme.unwrap_or_else(|| {
        if initialized {
            *runtime.theme.lock().unwrap()
        } else {
            saved.or(legacy).unwrap_or_default()
        }
    });
    *runtime.theme.lock().unwrap() = selected;
    let mut diagnostics = vec![];
    let persisted = {
        let mut store = runtime.store.lock().unwrap();
        let mut next = store.workspace.clone();
        next.theme = Some(selected);
        match store.commit(next) {
            Ok(()) => true,
            Err(e) => {
                diagnostics.push(e);
                false
            }
        }
    };
    diagnostics.extend(propagate_theme(&app).await);
    let result = ThemeResult {
        theme: selected,
        diagnostics,
        persisted,
    };
    for label in ["main", "overlay"] {
        let _ = app.emit_to(EventTarget::webview(label), "workspace-theme", &result);
    }
    Ok(result)
}
async fn propagate_theme(app: &AppHandle) -> Vec<String> {
    let mut diagnostics = vec![];
    for (_, window) in app.windows() {
        if let Err(e) = window.set_theme(Some(native_theme(app))) {
            diagnostics.push(e.to_string());
        }
    }
    for (_, view) in app.webviews() {
        if let Err(e) = platform_theme(&view, app.clone()).await {
            diagnostics.push(e);
        }
    }
    diagnostics
}
async fn platform_theme(view: &Webview, app: AppHandle) -> Result<()> {
    #[cfg(windows)]
    {
        platform::theme(view, app).await
    }
    #[cfg(not(windows))]
    {
        let _ = (view, app);
        Ok(())
    }
}
pub async fn shutdown(app: AppHandle) {
    let runtime = app.state::<WebPages>();
    runtime.preparations.begin_shutdown();
    // End dependent native views before requesting provider cleanup.
    let labels = runtime
        .sessions
        .lock()
        .unwrap()
        .values()
        .map(|s| s.label.clone())
        .collect::<Vec<_>>();
    for label in labels {
        if let Some(view) = app.get_webview(&label) {
            let _ = view.close();
        }
    }
    let preparations = runtime.preparations.clone();
    if let Ok(errors) = tauri::async_runtime::spawn_blocking(move || preparations.shutdown()).await
    {
        for error in errors {
            eprintln!("前置程序退出清理：{error}");
        }
    }
    let services = runtime.services.clone();
    if let Ok(errors) = tauri::async_runtime::spawn_blocking(move || {
        oriel_core::services::Services::shutdown(&services)
    })
    .await
    {
        for error in errors {
            eprintln!("插件退出清理：{error}");
        }
    }
}
