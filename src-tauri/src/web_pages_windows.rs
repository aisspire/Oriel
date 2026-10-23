//! WebView2 operations stay on its owning UI thread. No credentials cross into Vue.
use super::*;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use webview2_com::{
    take_pwstr, ClearBrowsingDataCompletedHandler, DocumentTitleChangedEventHandler,
    FaviconChangedEventHandler, GetFaviconCompletedHandler, HistoryChangedEventHandler,
    Microsoft::Web::WebView2::Win32::*, NavigationCompletedEventHandler,
    NavigationStartingEventHandler, SourceChangedEventHandler,
};
use windows::{
    core::{Interface, BOOL, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    },
};

async fn receive<T: Send + 'static>(rx: mpsc::Receiver<Result<T>>) -> Result<T> {
    tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs(20))
            .map_err(|_| "原生浏览操作未完成，请重试。".to_owned())?
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn with_core<T, F>(view: &Webview, action: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(ICoreWebView2) -> windows::core::Result<T> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    view.with_webview(move |platform| {
        // SAFETY: Tauri executes this closure on the WebView2 controller's UI thread.
        let result = unsafe { platform.controller().CoreWebView2() }
            .and_then(action)
            .map_err(|e| format!("WebView2 操作失败：{e}"));
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    receive(rx).await
}

fn apply_theme(core: &ICoreWebView2, app: &AppHandle) -> windows::core::Result<()> {
    // Read effective theme on the owning UI thread, never a value captured before async creation.
    let color = match native_theme(app) {
        tauri::Theme::Dark => COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK,
        _ => COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT,
    };
    unsafe {
        core.cast::<ICoreWebView2_13>()?
            .Profile()?
            .SetPreferredColorScheme(color)
    }
}
pub async fn theme(view: &Webview, app: AppHandle) -> Result<()> {
    with_core(view, move |core| apply_theme(&core, &app)).await
}

fn configure(core: &ICoreWebView2) -> windows::core::Result<()> {
    // SAFETY: only called from with_webview, on the owning UI thread.
    unsafe {
        let settings = core.Settings()?;
        settings.SetIsWebMessageEnabled(false)?;
        settings.SetAreHostObjectsAllowed(false)?;
        let settings4 = settings.cast::<ICoreWebView2Settings4>()?;
        settings4.SetIsGeneralAutofillEnabled(false)?;
        settings4.SetIsPasswordAutosaveEnabled(true)
    }
}

fn publish(
    app: &AppHandle,
    id: &str,
    label: &str,
    core: &ICoreWebView2,
    loading: Option<bool>,
    error: Option<Option<String>>,
) -> windows::core::Result<()> {
    // Each getter allocates its returned string with CoTaskMemAlloc; take_pwstr frees it.
    unsafe {
        let mut source = PWSTR::null();
        core.Source(&mut source)?;
        let source = take_pwstr(source);
        let mut title = PWSTR::null();
        core.DocumentTitle(&mut title)?;
        let title = take_pwstr(title);
        let mut back = BOOL::default();
        core.CanGoBack(&mut back)?;
        let mut forward = BOOL::default();
        core.CanGoForward(&mut forward)?;
        update_state(app, id, label, |state| {
            if source != "about:blank" {
                state.url = source;
            }
            if !title.is_empty() && title != "about:blank" {
                state.title = title;
            }
            state.can_go_back = back.as_bool();
            state.can_go_forward = forward.as_bool();
            if let Some(loading) = loading {
                state.loading = loading;
            }
            if let Some(error) = error {
                state.error = error;
            }
        });
    }
    Ok(())
}

pub async fn initialize(view: &Webview, app: AppHandle, id: String, label: String) -> Result<()> {
    with_core(view, move |core| {
        configure(&core)?;
        apply_theme(&core, &app)?;
        let navigation = Arc::new(AtomicU64::new(0));
        let favicon_generation = Arc::new(AtomicU64::new(0));
        unsafe {
            let (app, id, label, navigation) =
                (app.clone(), id.clone(), label.clone(), navigation.clone());
            let favicon_generation = favicon_generation.clone();
            let handler = NavigationStartingEventHandler::create(Box::new(move |sender, args| {
                favicon_generation.fetch_add(1, Ordering::Relaxed);
                if let (Some(core), Some(args)) = (sender, args) {
                    let mut nav_id = 0;
                    args.NavigationId(&mut nav_id)?;
                    navigation.store(nav_id, Ordering::Relaxed);
                    publish(&app, &id, &label, &core, Some(true), Some(None))?;
                }
                Ok(())
            }));
            core.add_NavigationStarting(&handler, &mut 0)?;
        }
        // Older runtimes without this optional interface keep the monogram fallback.
        if let Ok(favicon_core) = core.cast::<ICoreWebView2_15>() {
            let (app, id, label) = (app.clone(), id.clone(), label.clone());
            let handler = FaviconChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(core) = sender {
                    let _ = request_favicon(&core, &app, &id, &label, favicon_generation.clone());
                }
                Ok(())
            }));
            unsafe {
                let _ = favicon_core.add_FaviconChanged(&handler, &mut 0);
            }
        }
        unsafe {
            let (app, id, label) = (app.clone(), id.clone(), label.clone());
            let handler = HistoryChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(core) = sender {
                    publish(&app, &id, &label, &core, None, None)?;
                }
                Ok(())
            }));
            core.add_HistoryChanged(&handler, &mut 0)?;
        }
        unsafe {
            let (app, id, label) = (app.clone(), id.clone(), label.clone());
            let handler = SourceChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(core) = sender {
                    publish(&app, &id, &label, &core, None, None)?;
                }
                Ok(())
            }));
            core.add_SourceChanged(&handler, &mut 0)?;
        }
        unsafe {
            let (app, id, label) = (app.clone(), id.clone(), label.clone());
            let handler = DocumentTitleChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(core) = sender {
                    publish(&app, &id, &label, &core, None, None)?;
                }
                Ok(())
            }));
            core.add_DocumentTitleChanged(&handler, &mut 0)?;
        }
        unsafe {
            let (app, id, label) = (app.clone(), id.clone(), label.clone());
            let handler = NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
                if let (Some(core), Some(args)) = (sender, args) {
                    let mut nav_id = 0;
                    args.NavigationId(&mut nav_id)?;
                    if nav_id != navigation.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    let mut success = BOOL::default();
                    args.IsSuccess(&mut success)?;
                    let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                    args.WebErrorStatus(&mut status)?;
                    let error = if !success.as_bool()
                        && status != COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED
                    {
                        Some(format!(
                            "网页加载失败（{}），请检查网址、网络或代理后重试。",
                            status.0
                        ))
                    } else {
                        None
                    };
                    publish(&app, &id, &label, &core, Some(false), Some(error))?;
                }
                Ok(())
            }));
            core.add_NavigationCompleted(&handler, &mut 0)?;
        }
        publish(&app, &id, &label, &core, None, None)
    })
    .await
}

pub fn initialize_popup(window: &tauri::WebviewWindow) {
    let app = window.app_handle().clone();
    let _ = window.with_webview(move |platform| {
        let result = unsafe { platform.controller().CoreWebView2() }.and_then(|core| {
            configure(&core)?;
            apply_theme(&core, &app)
        });
        if let Err(error) = result {
            eprintln!("Popup WebView2 initialization failed: {error}");
        }
    });
}

fn request_favicon(
    core: &ICoreWebView2,
    app: &AppHandle,
    id: &str,
    label: &str,
    generation: Arc<AtomicU64>,
) -> windows::core::Result<()> {
    let request = generation.fetch_add(1, Ordering::Relaxed) + 1;
    let Some(registered_url) = favicon_registration(app, id, label) else {
        return Ok(());
    };
    // Compare the top-level document, allowing favicon files hosted on a CDN.
    unsafe {
        let mut source = PWSTR::null();
        core.Source(&mut source)?;
        if !oriel_core::same_web_origin(&take_pwstr(source), &registered_url) {
            return Ok(());
        }
        let (app, id, label) = (app.clone(), id.to_owned(), label.to_owned());
        let handler = GetFaviconCompletedHandler::create(Box::new(move |result, stream| {
            if result.is_err() || generation.load(Ordering::Relaxed) != request {
                return Ok(());
            }
            let Some(stream) = stream else {
                return Ok(());
            };
            if stream
                .Seek(0, windows::Win32::System::Com::STREAM_SEEK_SET, None)
                .is_err()
            {
                return Ok(());
            }
            let mut image = Vec::new();
            loop {
                let mut chunk = [0u8; 4096];
                let mut read = 0;
                if stream
                    .Read(
                        chunk.as_mut_ptr().cast(),
                        chunk.len() as u32,
                        Some(&mut read),
                    )
                    .is_err()
                {
                    return Ok(());
                }
                if read == 0 {
                    break;
                }
                if image.len() + read as usize > MAX_FAVICON_BYTES {
                    return Ok(());
                }
                image.extend_from_slice(&chunk[..read as usize]);
            }
            publish_favicon(&app, &id, &label, &registered_url, image);
            Ok(())
        }));
        core.cast::<ICoreWebView2_15>()?
            .GetFavicon(COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG, &handler)
    }
}

pub async fn action(view: &Webview, action: String) -> Result<()> {
    with_core(view, move |core| unsafe {
        match action.as_str() {
            "back" => core.GoBack(),
            "forward" => core.GoForward(),
            "reload" => core.Reload(),
            "stop" => core.Stop(),
            _ => unreachable!("validated browser action"),
        }
    })
    .await
}

pub async fn clear_passwords(view: &Webview) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    view.with_webview(move |platform| {
        let result = (|| unsafe {
            let core = platform.controller().CoreWebView2()?;
            let profile = core
                .cast::<ICoreWebView2_13>()?
                .Profile()?
                .cast::<ICoreWebView2Profile2>()?;
            let completed_tx = tx.clone();
            let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |result| {
                let _ = completed_tx.send(result.map_err(|e| format!("清除密码失败：{e}")));
                Ok(())
            }));
            profile.ClearBrowsingData(COREWEBVIEW2_BROWSING_DATA_KINDS_PASSWORD_AUTOSAVE, &handler)
        })();
        if let Err(error) = result {
            let _ = tx.send(Err(format!("清除密码失败：{error}")));
        }
    })
    .map_err(|e| e.to_string())?;
    receive(rx).await
}

pub struct ProcessWatch {
    handle: usize,
}
impl Drop for ProcessWatch {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(HANDLE(self.handle as *mut _));
        }
    }
}

pub async fn watch(view: &Webview) -> Result<Arc<ProcessWatch>> {
    with_core(view, |core| unsafe {
        let mut pid = 0;
        core.BrowserProcessId(&mut pid)?;
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, false, pid)?;
        Ok(Arc::new(ProcessWatch {
            handle: handle.0 as usize,
        }))
    })
    .await
}

pub async fn wait_exit(watch: Arc<ProcessWatch>) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        // Never kill shared processes or retry under a different proxy/profile.
        let result = unsafe { WaitForSingleObject(HANDLE(watch.handle as *mut _), 8000) };
        if result == WAIT_OBJECT_0 {
            Ok(())
        } else {
            Err("旧网页进程尚未释放；已停止该入口，请稍后重试。".into())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
