//! Insert text at the cursor of the focused window (clipboard paste, or simulated typing).
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

const CF_UNICODETEXT: u32 = 13;

fn modifiers_down() -> bool {
    [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
        .iter()
        .any(|k| unsafe { (GetAsyncKeyState(k.0 as i32) as u16) & 0x8000 != 0 })
}

fn wait_modifiers_released() {
    let end = Instant::now() + Duration::from_millis(1000);
    while Instant::now() < end && modifiers_down() {
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn key_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    }
}

fn send(inputs: &[INPUT]) {
    unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Open the clipboard, retrying briefly because other apps may hold it.
fn open_clipboard() -> bool {
    for _ in 0..10 {
        if unsafe { OpenClipboard(HWND::default()) }.is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

pub fn get_clipboard_text() -> Option<String> {
    if !open_clipboard() {
        return None;
    }
    let mut result = None;
    unsafe {
        if let Ok(h) = GetClipboardData(CF_UNICODETEXT) {
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u16;
            if !p.is_null() {
                let mut len = 0usize;
                while *p.add(len) != 0 {
                    len += 1;
                }
                result = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)));
                let _ = GlobalUnlock(g);
            }
        }
        let _ = CloseClipboard();
    }
    result
}

pub fn set_clipboard_text(text: &str) -> bool {
    let mut w: Vec<u16> = text.encode_utf16().collect();
    w.push(0);
    if !open_clipboard() {
        return false;
    }
    let mut ok = false;
    unsafe {
        let _ = EmptyClipboard();
        if let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2) {
            let p = GlobalLock(g) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
                let _ = GlobalUnlock(g);
                ok = SetClipboardData(CF_UNICODETEXT, HANDLE(g.0)).is_ok();
            }
        }
        let _ = CloseClipboard();
    }
    ok
}

fn type_text(text: &str) {
    for ch in text.chars() {
        if ch == '\n' {
            send(&[key_input(VK_RETURN.0, 0, KEYBD_EVENT_FLAGS(0)), key_input(VK_RETURN.0, 0, KEYEVENTF_KEYUP)]);
            continue;
        }
        let mut buf = [0u16; 2];
        for unit in ch.encode_utf16(&mut buf) {
            send(&[
                key_input(0, *unit, KEYEVENTF_UNICODE),
                key_input(0, *unit, KEYBD_EVENT_FLAGS(KEYEVENTF_UNICODE.0 | KEYEVENTF_KEYUP.0)),
            ]);
        }
    }
}

pub fn insert_text(text: &str, method: &str) {
    if text.is_empty() {
        return;
    }
    wait_modifiers_released();
    if method == "type" {
        type_text(text);
        return;
    }
    let old = get_clipboard_text();
    set_clipboard_text(text);
    std::thread::sleep(Duration::from_millis(50));
    send(&[
        key_input(VK_CONTROL.0, 0, KEYBD_EVENT_FLAGS(0)),
        key_input(0x56, 0, KEYBD_EVENT_FLAGS(0)), // V
        key_input(0x56, 0, KEYEVENTF_KEYUP),
        key_input(VK_CONTROL.0, 0, KEYEVENTF_KEYUP),
    ]);
    std::thread::sleep(Duration::from_millis(300));
    if let Some(o) = old {
        if !o.is_empty() {
            set_clipboard_text(&o); // restore previous (text) clipboard
        }
    }
}
