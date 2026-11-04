//! Hides or restores the Windows taskbar, so it can't show over YASAFF while it's fullscreen.

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, SW_HIDE, SW_SHOW, ShowWindow};
    use windows_sys::core::BOOL;

    unsafe extern "system" fn collect(hwnd: HWND, found: LPARAM) -> BOOL {
        let mut class = [0u16; 32];
        let len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) }.max(0) as usize;
        if matches!(String::from_utf16_lossy(&class[..len]).as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
            unsafe { (*(found as *mut Vec<HWND>)).push(hwnd) };
        }
        TRUE
    }

    /// Hides (or restores) the taskbar on every monitor.
    pub fn set_hidden(hidden: bool) {
        let mut found: Vec<HWND> = Vec::new();
        unsafe { EnumWindows(Some(collect), &mut found as *mut _ as LPARAM) };
        let cmd = if hidden { SW_HIDE } else { SW_SHOW };
        for hwnd in found {
            unsafe { ShowWindow(hwnd, cmd) };
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn set_hidden(_hidden: bool) {}
}

pub use imp::set_hidden;
