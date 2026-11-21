//! Small always-on-top recording bar: LED + 4-row dot meter + label + cancel. Never takes focus.
use crate::gfx::{Canvas, Fonts, R};
use crate::theme::*;
use crate::util::{pcwstr, wide_z};
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const W: i32 = 232;
const H: i32 = 34;
const COLS: usize = 12;
const ROWS: usize = 4;
const T_TICK: usize = 1;
const T_NOTICE: usize = 2;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Idle,
    Recording,
    Processing,
    Notice,
}

pub struct Overlay {
    hwnd: HWND,
    sc: f32,
    mode: Mode,
    levels: [f32; COLS],
    notice_active: bool,
    label: String,
    label_color: u32,
    led: u32,
    pub bn: bool,
    fonts: Fonts,
    t0: Instant,
}

impl Overlay {
    pub fn create(hinst: HINSTANCE, sc: f32) -> Overlay {
        unsafe {
            let class = wide_z("InzelOverlay");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(overlay_proc),
                hInstance: hinst,
                lpszClassName: pcwstr(&class),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                pcwstr(&class),
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                (W as f32 * sc) as i32,
                (H as f32 * sc) as i32,
                None,
                None,
                hinst,
                None,
            )
            .unwrap_or_default();
            Overlay {
                hwnd,
                sc,
                mode: Mode::Idle,
                levels: [0.0; COLS],
                notice_active: false,
                label: String::new(),
                label_color: FG,
                led: REC,
                bn: false,
                fonts: Fonts::default(),
                t0: Instant::now(),
            }
        }
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.sc).round() as i32
    }

    fn show(&self) {
        unsafe {
            let sw = GetSystemMetrics(SM_CXSCREEN);
            let sh = GetSystemMetrics(SM_CYSCREEN);
            let (w, h) = (self.s(W as f32), self.s(H as f32));
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, (sw - w) / 2, sh - h - self.s(80.0), w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    fn hide(&mut self) {
        unsafe {
            let _ = KillTimer(self.hwnd, T_TICK);
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    // ---- public ------------------------------------------------------------------
    pub fn set_state(&mut self, state: crate::events::State, label: &str) {
        use crate::events::State;
        match state {
            State::Idle => {
                self.mode = Mode::Idle;
                if !self.notice_active {
                    self.hide();
                }
            }
            State::Recording | State::Processing => {
                self.mode = if state == State::Recording { Mode::Recording } else { Mode::Processing };
                self.notice_active = false;
                self.label = label.to_string();
                self.label_color = FG;
                self.led = if state == State::Recording { REC } else { ACCENT };
                self.levels = [0.0; COLS];
                self.show();
                unsafe {
                    SetTimer(self.hwnd, T_TICK, 60, None);
                }
            }
        }
    }

    pub fn push_level(&mut self, v: f32) {
        if self.mode == Mode::Recording {
            self.levels.rotate_left(1);
            self.levels[COLS - 1] = v;
            unsafe {
                let _ = InvalidateRect(self.hwnd, None, false);
            }
        }
    }

    pub fn notice(&mut self, text: &str, color: Option<u32>, ms: u32) {
        self.mode = Mode::Notice;
        self.notice_active = true;
        let t: String = if text.chars().count() <= 34 { text.to_string() } else { text.chars().take(33).collect::<String>() + "…" };
        self.label = t;
        self.label_color = color.unwrap_or(FG);
        self.led = color.unwrap_or(MUTED);
        self.show();
        unsafe {
            let _ = KillTimer(self.hwnd, T_TICK);
            SetTimer(self.hwnd, T_NOTICE, ms, None);
        }
    }

    pub fn end_notice(&mut self) {
        self.notice_active = false;
        unsafe {
            let _ = KillTimer(self.hwnd, T_NOTICE);
        }
        if self.mode == Mode::Notice || self.mode == Mode::Idle {
            self.mode = Mode::Idle;
            self.hide();
        }
    }

    fn tick(&mut self) {
        let now = self.t0.elapsed().as_secs_f32();
        match self.mode {
            Mode::Recording => {
                let t = (1.0 + (now * std::f32::consts::TAU / 2.4).sin()) / 2.0;
                self.led = mix(rgb(0x4A, 0x16, 0x12), REC, 0.25 + 0.75 * t);
            }
            Mode::Processing => {
                for i in 0..COLS {
                    self.levels[i] = 0.5 + 0.5 * (now * 5.0 + i as f32 * 0.7).sin();
                }
            }
            _ => {}
        }
        unsafe {
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    fn paint(&mut self, hwnd: HWND) {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let (w, h) = (self.s(W as f32), self.s(H as f32));
            let cv = Canvas::new(hdc, w, h);
            let bw = self.s(1.0).max(1);
            cv.boxed(R::new(0, 0, w, h), CARD, BORDER, bw);
            let cy = h as f32 / 2.0;
            // LED
            cv.fill(R::new(self.s(11.0), (cy - self.s(4.0) as f32) as i32, self.s(8.0), self.s(8.0)), self.led);
            let matrix = self.mode == Mode::Recording || self.mode == Mode::Processing;
            if matrix {
                let dot = self.s(3.6).max(2);
                for col in 0..COLS {
                    let x = 33.0 + col as f32 * 7.5;
                    let lit = ((self.levels[col] * ROWS as f32 + 0.5) as usize).min(ROWS);
                    for row in 0..ROWS {
                        let y = H as f32 / 2.0 + ((ROWS as f32 - 1.0) / 2.0 - row as f32) * 5.2;
                        let r = R::new(self.s(x - 1.8), self.s(y - 1.8), dot, dot);
                        cv.fill(r, if row < lit { ACCENT } else { BORDER });
                    }
                }
            }
            let font = self.fonts.get(self.s(11.0), true, self.bn);
            let lx = if matrix { self.s(134.0) } else { self.s(28.0) };
            let lw = if matrix { self.s(78.0) } else { self.s(172.0) };
            cv.text(R::new(lx, 0, lw, h), &self.label, font, self.label_color, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
            let cfont = self.fonts.get(self.s(12.0), true, false);
            cv.text(R::new(w - self.s(28.0), 0, self.s(28.0), h), "✕", cfont, MUTED, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            let _ = BitBlt(hdc, 0, 0, w, h, cv.hdc, 0, 0, SRCCOPY);
            let _ = EndPaint(hwnd, &ps);
        }
    }
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let a = crate::ui::app_opt();
    let Some(app) = a else { return DefWindowProcW(hwnd, msg, wp, lp) };
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEACTIVATE => LRESULT(3), // MA_NOACTIVATE
        WM_PAINT => {
            if let Some(o) = app.overlay.as_mut() {
                o.paint(hwnd);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if let Some(o) = app.overlay.as_mut() {
                if wp.0 == T_TICK {
                    o.tick();
                } else if wp.0 == T_NOTICE {
                    o.end_notice();
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = crate::util::loword(lp.0);
            let sc = app.overlay.as_ref().map(|o| o.sc).unwrap_or(1.0);
            if x as f32 > (W as f32 - 28.0) * sc {
                crate::ui::cancel_recording();
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
