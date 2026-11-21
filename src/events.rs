//! Thread-safe event queue: worker threads push events, the UI thread drains them.
use crate::util::WM_EVENTS;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Idle,
    Recording,
    Processing,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub time: String,
    pub src: String,
    pub dst: Option<String>,
    pub original: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub enum Event {
    Level(f32),
    State(State),
    Result(Entry),
    /// Either a locale key starting with "err_" or a plain message.
    Error(String),
    /// Locale key shown briefly in the recording bar.
    Notice(String),
    UiLangReady(String),
    Hotkey(String, bool), // name, is_down
    Capture(Option<String>),
}

static QUEUE: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

pub fn set_main(hwnd: HWND) {
    MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
}

pub fn emit(ev: Event) {
    let was_empty;
    {
        let mut q = QUEUE.lock().unwrap();
        was_empty = q.is_empty();
        if let (Event::Level(_), Some(Event::Level(_))) = (&ev, q.back()) {
            q.pop_back(); // keep only the newest level
        }
        q.push_back(ev);
    }
    if was_empty {
        let h = MAIN_HWND.load(Ordering::SeqCst);
        if h != 0 {
            unsafe {
                let _ = PostMessageW(HWND(h as *mut _), WM_EVENTS, WPARAM(0), LPARAM(0));
            }
        }
    }
}

pub fn drain() -> Vec<Event> {
    let mut q = QUEUE.lock().unwrap();
    q.drain(..).collect()
}
