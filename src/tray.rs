//! System tray icon + right-click menu.
use crate::util::{pcwstr, wide_z, WM_TRAY};
use windows::Win32::Foundation::{HWND, POINT, WPARAM, LPARAM};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const ID_SHOW: usize = 1;
pub const ID_TRANSLATE: usize = 2;
pub const ID_QUIT: usize = 3;

fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid
}

pub fn add(hwnd: HWND, icon: HICON, tip: &str) {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    for (i, c) in tip.encode_utf16().take(127).enumerate() {
        nid.szTip[i] = c;
    }
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    }
}

pub fn remove(hwnd: HWND) {
    let nid = data(hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

/// Shows the menu at the cursor and returns the chosen command id (0 = none).
pub fn show_menu(hwnd: HWND, labels: (&str, &str, &str), translate_on: bool) -> usize {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return 0 };
        let (o, t, q) = (wide_z(labels.0), wide_z(labels.1), wide_z(labels.2));
        let _ = AppendMenuW(menu, MF_STRING, ID_SHOW, pcwstr(&o));
        let flags = if translate_on { MF_STRING | MF_CHECKED } else { MF_STRING };
        let _ = AppendMenuW(menu, flags, ID_TRANSLATE, pcwstr(&t));
        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, pcwstr(&q));
        let _ = SetMenuDefaultItem(menu, ID_SHOW as u32, 0);
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        cmd.0 as usize
    }
}
