use std::{ffi::c_void, mem::size_of_val};

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWCP_ROUND: u32 = 2;
const DWMWA_COLOR_NONE: u32 = 0xffff_fffe;

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: *mut c_void,
        attribute: u32,
        value: *const c_void,
        size: u32,
    ) -> i32;
}

pub fn apply(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?.0;

    // Windows 11 clips the entire native window and handles maximized/snapped corners.
    // This retains decorations: false and shadow: false without a visible frame.
    set_attribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
    set_attribute(hwnd, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE);
    Ok(())
}

fn set_attribute(hwnd: *mut c_void, attribute: u32, value: u32) {
    // SAFETY: hwnd belongs to the live Tauri window; value stays valid for this call
    // and its buffer size matches the u32 DWM attribute value.
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            (&value as *const u32).cast(),
            size_of_val(&value) as u32,
        )
    };

    // Older Windows versions may not support these attributes; keep the app usable.
    if result < 0 {
        eprintln!("Oriel window attribute {attribute} is unavailable: {result:#x}");
    }
}
