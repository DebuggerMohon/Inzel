//! Global hotkeys with separate press / release events (needed for hold-to-talk).
//! A low-level keyboard hook runs on its own thread; keys are never swallowed.
use crate::events::{emit, Event};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::*;

const ORDER: [&str; 4] = ["ctrl", "alt", "shift", "windows"];

#[derive(Default)]
struct State {
    bindings: HashMap<String, HashSet<String>>,
    active: HashMap<String, bool>,
    pressed: HashSet<String>,
    capture: bool,
    cap_keys: Vec<String>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap();
    f(g.get_or_insert_with(State::default))
}

pub fn parse(combo: &str) -> HashSet<String> {
    combo.split('+').map(|p| p.trim().to_lowercase()).filter(|p| !p.is_empty()).collect()
}

pub fn format_combo(keys: &[String]) -> String {
    let mut mods: Vec<String> = ORDER.iter().filter(|m| keys.iter().any(|k| k == *m)).map(|s| s.to_string()).collect();
    let mut rest: Vec<String> = keys.iter().filter(|k| !ORDER.contains(&k.as_str())).cloned().collect();
    rest.sort();
    mods.append(&mut rest);
    mods.join("+")
}

pub fn pretty(combo: &str) -> String {
    combo
        .split('+')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let is_f = p.starts_with('f') && p.len() > 1 && p[1..].chars().all(|c| c.is_ascii_digit());
            if is_f {
                p.to_uppercase()
            } else {
                let mut c = p.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn key_name(vk: u32) -> Option<String> {
    let s = match vk {
        0xA0 | 0xA1 | 0x10 => "shift",
        0xA2 | 0xA3 | 0x11 => "ctrl",
        0xA4 | 0xA5 | 0x12 => "alt",
        0x5B | 0x5C => "windows",
        0x08 => "backspace",
        0x09 => "tab",
        0x0D => "enter",
        0x1B => "esc",
        0x20 => "space",
        0x14 => "caps lock",
        0x21 => "page up",
        0x22 => "page down",
        0x23 => "end",
        0x24 => "home",
        0x25 => "left",
        0x26 => "up",
        0x27 => "right",
        0x28 => "down",
        0x2C => "print screen",
        0x2D => "insert",
        0x2E => "delete",
        0x13 => "pause",
        0x90 => "num lock",
        0x91 => "scroll lock",
        0x30..=0x39 => return Some(char::from((vk - 0x30) as u8 + b'0').to_string()),
        0x41..=0x5A => return Some(char::from((vk - 0x41) as u8 + b'a').to_string()),
        0x60..=0x69 => return Some(format!("num {}", vk - 0x60)),
        0x70..=0x87 => return Some(format!("f{}", vk - 0x70 + 1)),
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        _ => return None,
    };
    Some(s.to_string())
}

pub fn set_binding(name: &str, combo: &str) {
    with(|s| {
        s.bindings.insert(name.to_string(), parse(combo));
        s.active.insert(name.to_string(), false);
    });
}

/// The next key combination pressed is reported through Event::Capture; Esc cancels (None).
pub fn capture() {
    with(|s| {
        s.cap_keys.clear();
        s.capture = true;
    });
}
/// Pressing and releasing Alt (or Win) alone makes Windows activate the window's menu bar, which steals the
/// text cursor from the input box. Tapping an unassigned key while the modifier is held cancels that.
fn mask_menu_activation() {
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0xE8), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    };
    unsafe {
        SendInput(&[key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)], std::mem::size_of::<INPUT>() as i32);
    }
}
fn on_key(k: String, down: bool) {
    let mut events: Vec<Event> = Vec::new();
    with(|s| {
        if down {
            if s.pressed.contains(&k) {
                return; // auto-repeat
            }
            s.pressed.insert(k.clone());
            if s.capture {
                if k == "esc" {
                    s.capture = false;
                    events.push(Event::Capture(None));
                } else if !s.cap_keys.contains(&k) {
                    s.cap_keys.push(k.clone());
                }
                return;
            }
            let names: Vec<String> = s.bindings.keys().cloned().collect();
            for name in names {
                let combo = &s.bindings[&name];
                if !combo.is_empty() && combo.is_subset(&s.pressed) && !s.active.get(&name).copied().unwrap_or(false) {
                    s.active.insert(name.clone(), true);
                    events.push(Event::Hotkey(name, true));
                }
            }
        } else {
            s.pressed.remove(&k);
            if s.capture {
                if !s.cap_keys.is_empty() {
                    s.capture = false;
                    let combo = format_combo(&s.cap_keys);
                    events.push(Event::Capture(Some(combo)));
                }
                return;
            }
            let names: Vec<String> = s.bindings.keys().cloned().collect();
            for name in names {
                if s.active.get(&name).copied().unwrap_or(false) && s.bindings[&name].contains(&k) {
                    s.active.insert(name.clone(), false);
                    events.push(Event::Hotkey(name, false));
                }
            }
        }
    });
        let mask = down && (k == "alt" || k == "windows") && events.iter().any(|e| matches!(e, Event::Hotkey(_, true)));
    for e in events {
        emit(e);
    }
    if mask {
        mask_menu_activation();
    }
}

unsafe extern "system" fn hook_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        let injected = info.flags.0 & 0x10 != 0; // LLKHF_INJECTED: ignore our own SendInput
        if !injected {
            if let Some(name) = key_name(info.vkCode) {
                match wp.0 as u32 {
                    WM_KEYDOWN | WM_SYSKEYDOWN => on_key(name, true),
                    WM_KEYUP | WM_SYSKEYUP => on_key(name, false),
                    _ => {}
                }
            }
        }
    }
    CallNextHookEx(HHOOK::default(), code, wp, lp)
}

/// Starts the hook thread (call once).
pub fn start() {
    std::thread::spawn(|| unsafe {
        let hmod = GetModuleHandleW(None).map(|m| HINSTANCE(m.0)).unwrap_or_default();
        let _hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hmod, 0);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
