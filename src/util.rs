//! Small helpers shared by all modules.
use windows::core::PCWSTR;
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

pub const WM_EVENTS: u32 = WM_APP + 1; // worker threads -> UI thread
pub const WM_TRAY: u32 = WM_APP + 2; // tray icon callbacks
pub const WM_SHOWAPP: u32 = WM_APP + 3; // second instance asks the first to show itself

/// UTF-16 without terminating NUL (for slice based APIs such as DrawTextW).
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// UTF-16 with terminating NUL (keep the Vec alive while the PCWSTR is used).
pub fn wide_z(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

pub fn pcwstr(v: &[u16]) -> PCWSTR {
    PCWSTR(v.as_ptr())
}

pub fn now_hm() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe { GetLocalTime() };
    format!("{:02}:{:02}", t.wHour, t.wMinute)
}

pub fn loword(v: isize) -> i32 {
    (v & 0xFFFF) as u16 as i16 as i32
}
pub fn hiword(v: isize) -> i32 {
    ((v >> 16) & 0xFFFF) as u16 as i16 as i32
}
