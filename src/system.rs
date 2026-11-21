//! Windows helpers: single instance, start with Windows, memory trimming.
use crate::util::WM_SHOWAPP;
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, LPARAM, WPARAM};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};
use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// True if this is the only running instance. (Same mutex name as the installer's AppMutex.)
pub fn single_instance() -> bool {
    unsafe {
        match CreateMutexW(None, false, w!("Inzel_SingleInstance_Mutex")) {
            Ok(h) => {
                std::mem::forget(h); // keep it for the life of the process
                GetLastError() != ERROR_ALREADY_EXISTS
            }
            Err(_) => true,
        }
    }
}

/// Ask the already running instance to show its window.
pub fn activate_existing() {
    unsafe {
        if let Ok(h) = FindWindowW(w!("InzelMain"), None) {
            let _ = PostMessageW(h, WM_SHOWAPP, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn set_autostart(enable: bool) {
    let _ = (|| -> std::io::Result<()> {
        let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)?;
        if enable {
            let exe = std::env::current_exe()?;
            key.set_value("Inzel", &format!("\"{}\" --minimized", exe.display()))?;
        } else {
            let _ = key.delete_value("Inzel");
        }
        Ok(())
    })();
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> isize;
    fn SetProcessWorkingSetSize(process: isize, min: usize, max: usize) -> i32;
}

/// Give unused memory pages back to Windows (Task Manager shows a much smaller number while idle).
pub fn trim_memory() {
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}
