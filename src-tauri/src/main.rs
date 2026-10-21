#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
mod window_frame;

fn main() {
    let builder = tauri::Builder::default();

    #[cfg(target_os = "windows")]
    let builder = builder.setup(|app| {
        use tauri::Manager;

        if let Some(window) = app.get_webview_window("main") {
            window_frame::apply(&window)?;
        }
        Ok(())
    });

    builder
        .run(tauri::generate_context!())
        .expect("Oriel could not start");
}
