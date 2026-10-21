#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod web_pages;
#[cfg(target_os = "windows")]
mod window_frame;

fn main() {
    let builder = tauri::Builder::default().setup(|app| {
        use tauri::Manager;

        app.manage(web_pages::WebPages::new(app.handle())?);
        if let Some(root) = web_pages::development_data_root() {
            let configuration = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .ok_or("main window configuration is missing")?;
            // The config dataDirectory field only accepts relative paths. The builder's
            // explicit directory keeps debug checks entirely inside their isolated root.
            tauri::WebviewWindowBuilder::from_config(app, configuration)?
                .data_directory(root.join("shell"))
                .build()?;
        }
        #[cfg(target_os = "windows")]
        if let Some(window) = app.get_webview_window("main") {
            window_frame::apply(&window)?;
        }
        Ok(())
    });

    let mut context = tauri::generate_context!();
    if web_pages::development_data_root().is_some() {
        if let Some(window) = context.config_mut().app.windows.first_mut() {
            window.create = false;
        }
    }
    builder
        .invoke_handler(tauri::generate_handler![
            web_pages::workspace_snapshot,
            web_pages::workspace_reload,
            web_pages::workspace_save_entry,
            web_pages::workspace_delete_entry,
            web_pages::workspace_settings,
            web_pages::web_show,
            web_pages::web_navigate,
            web_pages::web_action,
            web_pages::web_clear_passwords,
        ])
        .run(context)
        .expect("Oriel could not start");
}
