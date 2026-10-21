//! Host-owned web-page capability. Plugins supply declarations, never native handles.
use oriel_core::{unique_id, BrowserSidebar, ConfigStore, ProxyPolicy, RegisteredEntry, Snapshot};
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
    services: Arc<Mutex<oriel_core::services::Services>>,
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
    favicons: HashMap<String, Vec<u8>>,
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
        Ok(Self {
            store: Mutex::new(ConfigStore::open(config_path.clone())),
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
            prepared: Mutex::default(),
            preparation_errors: Mutex::default(),
        })
    }

    fn snapshot(&self) -> HostSnapshot {
        let snapshot = self.store.lock().unwrap().snapshot(&self.builtin);
        let favicons = snapshot
            .entries
            .iter()
            .filter_map(|entry| {
                let url = entry.entry.url.as_deref()?;
                let path = self.data_root.join("favicons").join(&entry.entry.id);
                if std::fs::metadata(&path).ok()?.len() > (MAX_FAVICON_BYTES + 8193) as u64 {
                    return None;
                }
                let cached = std::fs::read(path).ok()?;
                let separator = cached.iter().position(|byte| *byte == b'\n')?;
                let origin_url = std::str::from_utf8(&cached[..separator]).ok()?;
                let image = &cached[separator + 1..];
                (oriel_core::same_web_origin(url, origin_url) && valid_favicon(image))
                    .then(|| (entry.entry.id.clone(), image.to_vec()))
            })
            .collect();
        HostSnapshot {
            snapshot,
            browser_supported: cfg!(windows),
            config_path: self.config_path.display().to_string(),
            favicons,
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
        let page_url = if entry.entry.service.is_some() || !entry.entry.prerequisites.is_empty() {
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
        let url = self
            .resume_urls
            .lock()
            .unwrap()
            .remove(id)
            .or(page_url)
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
fn authorize(caller: &Webview, _request: &tauri::ipc::Request<'_>) -> Result<()> {
    let url = caller.url().map_err(|_| "无法确认宿主调用来源。")?;
    let local = if cfg!(debug_assertions) {
        url.scheme() == "http" && url.host_str() == Some("127.0.0.1") && url.port() == Some(1420)
    } else {
        url.scheme() == "tauri" && url.host_str() == Some("localhost")
            || matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost")
    };
    if caller.label() != "main" || !local {
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

#[tauri::command]
pub fn workspace_snapshot(
    caller: Webview,
    request: tauri::ipc::Request<'_>,
    runtime: tauri::State<'_, WebPages>,
) -> Result<HostSnapshot> {
    authorize(&caller, &request)?;
    Ok(runtime.snapshot())
}

#[tauri::command]
pub async fn workspace_reload(
    caller: Webview,
    request: tauri::ipc::Request<'_>,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
) -> Result<HostSnapshot> {
    authorize(&caller, &request)?;
    let _operation = runtime.operation.lock().await;
    *runtime.store.lock().unwrap() = ConfigStore::open(runtime.config_path.clone());
    let errors = runtime.reconcile(&app).await;
    let mut snapshot = runtime.snapshot();
    snapshot.snapshot.diagnostics.extend(errors);
    Ok(snapshot)
}

#[tauri::command]
pub async fn workspace_save_entry(
    caller: Webview,
    request: tauri::ipc::Request<'_>,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    id: Option<String>,
    title: String,
    url: String,
    proxy: ProxyPolicy,
) -> Result<HostSnapshot> {
    authorize(&caller, &request)?;
    let _operation = runtime.operation.lock().await;
    runtime
        .store
        .lock()
        .unwrap()
        .save_entry(id, title, url, proxy)?;
    let errors = runtime.reconcile(&app).await;
    let mut snapshot = runtime.snapshot();
    snapshot.snapshot.diagnostics.extend(errors);
    Ok(snapshot)
}

#[tauri::command]
pub async fn workspace_delete_entry(
    caller: Webview,
    request: tauri::ipc::Request<'_>,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    id: String,
) -> Result<HostSnapshot> {
    authorize(&caller, &request)?;
    let _operation = runtime.operation.lock().await;
    runtime.store.lock().unwrap().delete_entry(&id)?;
    let errors = runtime.reconcile(&app).await;
    let mut snapshot = runtime.snapshot();
    snapshot.snapshot.diagnostics.extend(errors);
    Ok(snapshot)
}

#[tauri::command]
pub async fn workspace_reorder_entries(
    caller: Webview,
    request: tauri::ipc::Request<'_>,
    runtime: tauri::State<'_, WebPages>,
    ids: Vec<String>,
) -> Result<HostSnapshot> {
    authorize(&caller, &request)?;
    let _operation = runtime.operation.lock().await;
    runtime.store.lock().unwrap().reorder_entries(ids)?;
    Ok(runtime.snapshot())
}

#[tauri::command]
pub async fn workspace_browser_sidebar(
    caller: Webview,
    request: tauri::ipc::Request<'_>,
    runtime: tauri::State<'_, WebPages>,
    sidebar: BrowserSidebar,
) -> Result<HostSnapshot> {
    authorize(&caller, &request)?;
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
    request: tauri::ipc::Request<'_>,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    default_proxy: ProxyPolicy,
    plugin_directories: Vec<PathBuf>,
) -> Result<HostSnapshot> {
    authorize(&caller, &request)?;
    let _operation = runtime.operation.lock().await;
    {
        let mut store = runtime.store.lock().unwrap();
        let mut next = store.workspace.clone();
        next.default_proxy = default_proxy;
        next.plugin_directories = plugin_directories;
        store.commit(next)?;
    }
    let errors = runtime.reconcile(&app).await;
    let mut snapshot = runtime.snapshot();
    snapshot.snapshot.diagnostics.extend(errors);
    Ok(snapshot)
}

#[tauri::command]
pub async fn web_show(
    caller: Webview,
    request: tauri::ipc::Request<'_>,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    entry_id: Option<String>,
    bounds: ViewBounds,
    visible: bool,
) -> Result<Option<WebSessionState>> {
    authorize(&caller, &request)?;
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
    request: tauri::ipc::Request<'_>,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
    url: String,
) -> Result<()> {
    authorize(&caller, &request)?;
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

#[tauri::command]
pub async fn web_action(
    caller: Webview,
    request: tauri::ipc::Request<'_>,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
    action: String,
) -> Result<()> {
    authorize(&caller, &request)?;
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
    request: tauri::ipc::Request<'_>,
    app: AppHandle,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
) -> Result<()> {
    authorize(&caller, &request)?;
    let _operation = runtime.operation.lock().await;
    let view = runtime.ensure(&app, &entry_id).await?;
    platform::clear_passwords(&view).await?;
    view.reload().map_err(|e| e.to_string())
}

// An explicit preflight runs outside the surface lock. Opening Tether to answer
// authentication remains possible while a dependent page is waiting.
#[tauri::command]
pub async fn web_prepare(
    caller: Webview,
    request: tauri::ipc::Request<'_>,
    runtime: tauri::State<'_, WebPages>,
    entry_id: String,
    retry: bool,
) -> Result<()> {
    authorize(&caller, &request)?;
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
    let (entry, _) = runtime.entry(&entry_id)?;
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
pub async fn shutdown(app: AppHandle) {
    let runtime = app.state::<WebPages>();
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
