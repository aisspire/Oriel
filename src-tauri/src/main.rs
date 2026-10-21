#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod web_pages;
#[cfg(target_os = "windows")]
mod window_frame;
mod window_state;

fn main() {
    let builder = tauri::Builder::default().setup(|app| {
        use tauri::Manager;

        app.manage(web_pages::WebPages::new(app.handle())?);
        window_state::create(app)?;
        Ok(())
    });

    let mut context = tauri::generate_context!();
    // Build the main window with its saved normal size before maximizing or showing it.
    if let Some(window) = context
        .config_mut()
        .app
        .windows
        .iter_mut()
        .find(|w| w.label == "main")
    {
        window.create = false;
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
