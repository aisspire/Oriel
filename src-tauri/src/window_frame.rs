use std::{ffi::c_void, mem::size_of_val, ptr};

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWCP_ROUND: u32 = 2;
const DWMWA_COLOR_NONE: u32 = 0xffff_fffe;

const GWL_STYLE: i32 = -16;
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_CAPTION: u32 = 0x00c0_0000;

const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_FRAMECHANGED: u32 = 0x0020;

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: *mut c_void,
        attribute: u32,
        value: *const c_void,
        size: u32,
    ) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowLongW(hwnd: *mut c_void, n_index: i32) -> i32;
    fn SetWindowLongW(hwnd: *mut c_void, n_index: i32, new_long: i32) -> i32;
    fn SetWindowPos(
        hwnd: *mut c_void,
        hwnd_insert_after: *mut c_void,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
}

pub fn apply(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?.0;

    // Enable native Win32 sizing borders (WS_THICKFRAME) so mouse drag-resizing works
    // even with custom decorations: false under Tauri's unstable multi-webview features.
    let current_style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) };
    if current_style != 0 {
        let new_style = (current_style as u32 | WS_THICKFRAME) & !WS_CAPTION;
        unsafe {
            SetWindowLongW(hwnd, GWL_STYLE, new_style as i32);
            SetWindowPos(
                hwnd,
                ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
            );
        }
    }

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
