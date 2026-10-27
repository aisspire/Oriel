//! Acrylic is decorative: policy/OS/API failures retain the transparent window.
use std::{ffi::c_void, mem::size_of, ptr};
use tauri::{
    window::{Color, Effect, EffectsBuilder},
    WebviewWindow,
};

#[repr(C)]
struct OsVersion {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    service_pack: [u16; 128],
}
#[repr(C)]
struct HighContrast {
    size: u32,
    flags: u32,
    scheme: *mut u16,
}
#[repr(C)]
#[derive(Default)]
struct PowerStatus {
    ac: u8,
    battery: u8,
    percent: u8,
    saver: u8,
    life: u32,
    full_life: u32,
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(info: *mut OsVersion) -> i32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegGetValueW(
        key: *mut c_void,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> i32;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn SystemParametersInfoW(action: u32, param: u32, data: *mut c_void, flags: u32) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemPowerStatus(status: *mut PowerStatus) -> i32;
}
#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmGetWindowAttribute(
        hwnd: *mut c_void,
        attribute: u32,
        value: *mut c_void,
        size: u32,
    ) -> i32;
}

fn material_build() -> Option<u32> {
    let mut version = OsVersion {
        size: size_of::<OsVersion>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        service_pack: [0; 128],
    };
    let mut contrast = HighContrast {
        size: size_of::<HighContrast>() as u32,
        flags: 0,
        scheme: ptr::null_mut(),
    };
    let mut power = PowerStatus::default();
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let name: Vec<u16> = "EnableTransparency\0".encode_utf16().collect();
    let mut transparency: u32 = 1;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: all buffers have the declared Win32 ABI sizes and live through the
    // calls. HKEY_CURRENT_USER is a predefined pseudo-handle; registry is read-only.
    unsafe {
        if RtlGetVersion(&mut version) < 0 || version.major < 10 || version.build < 17763 {
            return None;
        }
        let result = RegGetValueW(
            0x80000001u32 as i32 as isize as *mut c_void,
            key.as_ptr(),
            name.as_ptr(),
            0x10,
            ptr::null_mut(),
            (&mut transparency as *mut u32).cast(),
            &mut size,
        );
        if result == 0 && transparency == 0 {
            return None;
        }
        if SystemParametersInfoW(
            0x42,
            contrast.size,
            (&mut contrast as *mut HighContrast).cast(),
            0,
        ) == 0
            || contrast.flags & 1 != 0
        {
            return None;
        }
        if GetSystemPowerStatus(&mut power) != 0 && power.saver != 0 {
            return None;
        }
    }
    Some(version.build)
}

pub fn apply(window: &WebviewWindow) {
    // Re-evaluate each opening, including when settings changed during the session.
    let _ = window.set_effects(None);
    let Some(build) = material_build() else {
        eprintln!("Oriel overlay material=shade reason=system-policy-or-unsupported");
        return;
    };
    let result = window.set_effects(
        EffectsBuilder::new()
            .effect(Effect::Acrylic)
            .color(Color(0, 0, 0, 0))
            .build(),
    );
    // Tauri 2.12.4 discards the underlying vibrancy result. On Windows 11 22H2+
    // verify the actual DWM attribute instead of treating enqueue success as proof.
    let applied = result.is_ok()
        && (build < 22621
            || window.hwnd().is_ok_and(|hwnd| {
                let mut backdrop: u32 = 0;
                // SAFETY: HWND belongs to the live overlay and output is a u32 DWM value.
                unsafe {
                    DwmGetWindowAttribute(
                        hwnd.0,
                        38,
                        (&mut backdrop as *mut u32).cast(),
                        size_of::<u32>() as u32,
                    ) >= 0
                        && backdrop == 3
                }
            }));
    if !applied {
        let _ = window.set_effects(None);
    }
    eprintln!(
        "Oriel overlay material={}",
        if applied {
            "acrylic-requested"
        } else {
            "shade"
        }
    );
}
