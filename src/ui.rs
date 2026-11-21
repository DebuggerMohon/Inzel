//! Main window: custom-drawn (GDI) Home and Settings pages. Immediate-mode painting: every paint draws the
//! current page and records clickable regions ("hits"); mouse clicks look the region up. Square corners everywhere,
//! exactly like the Python/CustomTkinter version.
use crate::config::Cfg;
use crate::engine::Engine;
use crate::events::{self, Entry, Event, State};
use crate::gfx::{Canvas, Fonts, R};
use crate::hotkeys::{self, pretty};
use crate::i18n::{build_locale, has_locale, I18n};
use crate::languages::{display_name, short, LANGS};
use crate::overlay::Overlay;
use crate::theme::*;
use crate::util::*;
use crate::{audio, inject, system, tray, APP_NAME, AUTHOR, GITHUB_URL, VERSION};
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;

const T_STATUS: usize = 10;
const T_COPIED: usize = 11;
const T_TRIM: usize = 12;
const T_CARET: usize = 13;
const WM_MOUSELEAVE: u32 = 0x02A3;

static APP: AtomicPtr<App> = AtomicPtr::new(std::ptr::null_mut());
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub fn app_opt() -> Option<&'static mut App> {
    let p = APP.load(Ordering::SeqCst);
    if p.is_null() {
        None
    } else {
        Some(unsafe { &mut *p })
    }
}

/// Overlay "✕" button.
pub fn cancel_recording() {
    if let Some(a) = app_opt() {
        let _ = a.cmd_tx.send(Cmd::Stop(true));
    }
}

enum Cmd {
    Start,
    Stop(bool),
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum PopupKind {
    Speak,
    Target,
    Mic,
}

#[derive(Clone, PartialEq)]
enum Action {
    Noop,
    OpenSettings,
    Back,
    Clear,
    Copy(usize),
    OpenPopup(PopupKind),
    ClosePopup,
    Pick(String),
    SetTranslate(bool),
    Capture(&'static str),
    RecMode(bool),
    Toggle(&'static str),
    Slider,
    UiLang(&'static str),
    Output(&'static str),
    Github,
}

struct Hit {
    r: R,
    a: Action,
}

struct Item {
    id: String,
    label: String,
    hay: String,
}

struct Popup {
    kind: PopupKind,
    anchor: R,
    items: Vec<Item>,
    selected: String,
    query: String,
    scroll: i32,
}

pub struct App {
    hwnd: HWND,
    sc: f32,
    cfg: Cfg,
    i18n: I18n,
    engine: Arc<Engine>,
    cmd_tx: mpsc::Sender<Cmd>,
    pub overlay: Option<Overlay>,
    logo: HICON,
    tray_icon: HICON,

    history: Vec<Entry>,
    heights: Vec<(i32, i32)>, // cached (text width, text height) per history entry
    cur_state: State,
    page: Page,
    popup: Option<Popup>,
    mic_names: Vec<String>,
    capture_key: Option<&'static str>,
    status_msg: Option<String>,
    copied: Option<usize>,

    // painting
    canvas: Option<Canvas>,
    fonts: Fonts,
    hits: Vec<Hit>,
    clip: Option<R>,
    dry: bool,
    block_hover: bool,
    mx: i32,
    my: i32,
    tracking: bool,
    drag: bool,
    slider: R,
    scroll_home: i32,
    scroll_settings: i32,
    content_h: i32,
    max_scroll_home: i32,
}

fn lvc() -> DRAW_TEXT_FORMAT {
    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS
}
fn cvc() -> DRAW_TEXT_FORMAT {
    DT_CENTER | DT_VCENTER | DT_SINGLELINE
}

impl App {
    // ---- small helpers ---------------------------------------------------------------
    fn s(&self, v: i32) -> i32 {
        (v as f32 * self.sc).round() as i32
    }
    fn bw(&self) -> i32 {
        self.s(1).max(1)
    }
    fn t(&self, key: &str) -> String {
        self.i18n.t(key)
    }
    fn bn(&self) -> bool {
        self.i18n.lang == "bn"
    }
    fn redraw(&self) {
        unsafe {
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }
    fn label(&self) -> String {
        let (src, dst) = (self.cfg.str("speak_lang"), self.cfg.str("target_lang"));
        if self.cfg.bool("translate_enabled") && src != dst {
            format!("{} → {}", short(&src), short(&dst))
        } else {
            short(&src)
        }
    }
    fn lang_label(&self, code: &str) -> String {
        if code == "auto" {
            self.t("auto_detect")
        } else {
            display_name(code)
        }
    }
    fn ov(&mut self) -> &mut Overlay {
        self.overlay.as_mut().expect("overlay")
    }

    // ---- drawing primitives (all no-ops while `dry` is set; used to measure layouts) --------
    fn font(&mut self, px: i32, bold: bool) -> HFONT {
        let nirmala = self.bn();
        let px = self.s(px);
        self.fonts.get(px, bold, nirmala)
    }
    fn rect(&self, cv: &Canvas, r: R, c: u32) {
        if !self.dry {
            cv.fill(r, c);
        }
    }
    fn boxed(&self, cv: &Canvas, r: R, fill: u32, border: u32) {
        if !self.dry {
            cv.boxed(r, fill, border, self.bw());
        }
    }
    fn txt(&mut self, cv: &Canvas, r: R, s: &str, px: i32, bold: bool, color: u32, flags: DRAW_TEXT_FORMAT) {
        if self.dry {
            return;
        }
        let f = self.font(px, bold);
        cv.text(r, s, f, color, flags);
    }
    fn txt_nirmala(&mut self, cv: &Canvas, r: R, s: &str, px: i32, color: u32, flags: DRAW_TEXT_FORMAT) {
        if self.dry {
            return;
        }
        let px = self.s(px);
        let f = self.fonts.get(px, false, true);
        cv.text(r, s, f, color, flags);
    }
    fn measure(&mut self, cv: &Canvas, s: &str, px: i32, bold: bool, max_w: i32) -> (i32, i32) {
        let f = self.font(px, bold);
        cv.measure(s, f, max_w)
    }
    fn hit(&mut self, r: R, a: Action) {
        if self.dry {
            return;
        }
        let r = match self.clip {
            Some(c) => match r.intersect(&c) {
                Some(x) => x,
                None => return,
            },
            None => r,
        };
        self.hits.push(Hit { r, a });
    }
    fn hover(&self, r: R) -> bool {
        if self.block_hover || !r.contains(self.mx, self.my) {
            return false;
        }
        match self.clip {
            Some(c) => c.contains(self.mx, self.my),
            None => true,
        }
    }

    fn button(&mut self, cv: &Canvas, r: R, label: &str, px: i32, accent: bool, a: Action) {
        let hov = self.hover(r);
        let bg = if accent {
            if hov { ACCENT_HOVER } else { ACCENT }
        } else if hov {
            BORDER
        } else {
            CARD2
        };
        self.rect(cv, r, bg);
        self.txt(cv, r, label, px, accent, FG, cvc());
        self.hit(r, a);
    }

    /// Segmented control; returns its width. Height is 34.
    fn segmented(&mut self, cv: &Canvas, x: i32, y: i32, labels: &[String], selected: usize, actions: Vec<Action>) -> i32 {
        let pad = self.s(3);
        let mut widths = Vec::new();
        for l in labels {
            let (tw, _) = self.measure(cv, l, 13, false, 0);
            widths.push((tw + self.s(36)).max(self.s(64)));
        }
        let total: i32 = widths.iter().sum::<i32>() + 2 * pad;
        let h = self.s(34);
        self.rect(cv, R::new(x, y, total, h), CARD2);
        let mut cx = x + pad;
        for (i, l) in labels.iter().enumerate() {
            let seg = R::new(cx, y + pad, widths[i], h - 2 * pad);
            let hov = self.hover(seg);
            let bg = if i == selected {
                if hov { ACCENT_HOVER } else { ACCENT }
            } else if hov {
                BORDER
            } else {
                CARD2
            };
            self.rect(cv, seg, bg);
            self.txt(cv, seg, l, 13, false, FG, cvc());
            self.hit(seg, actions[i].clone());
            cx += widths[i];
        }
        total
    }

    fn segmented_width(&mut self, cv: &Canvas, labels: &[String]) -> i32 {
        let mut total = 2 * self.s(3);
        for l in labels {
            let (tw, _) = self.measure(cv, l, 13, false, 0);
            total += (tw + self.s(36)).max(self.s(64));
        }
        total
    }

    /// Toggle switch with a label; returns height (22).
    fn switch(&mut self, cv: &Canvas, x: i32, y: i32, label: &str, on: bool, key: &'static str) -> i32 {
        let (tw, _) = self.measure(cv, label, 13, false, 0);
        let track = R::new(x, y, self.s(44), self.s(22));
        let full = R::new(x, y, track.w + self.s(8) + tw, track.h);
        let hov = self.hover(full);
        self.rect(cv, track, if on { ACCENT } else { BORDER });
        let kw = self.s(14);
        let kx = if on { track.right() - kw - self.s(2) } else { track.x + self.s(2) };
        self.rect(cv, R::new(kx, y + self.s(2), kw, track.h - self.s(4)), if hov { KNOB_HOVER } else { FG });
        self.txt(cv, R::new(track.right() + self.s(8), y, tw + self.s(4), track.h), label, 13, false, FG, lvc());
        self.hit(full, Action::Toggle(key));
        track.h
    }

    // ---- page chrome -----------------------------------------------------------------
    fn draw_header(&mut self, cv: &Canvas, settings: bool) {
        let (w, hh) = (cv.w, self.s(58));
        self.rect(cv, R::new(0, 0, w, hh), BG);
        let sz = self.s(30);
        if !self.dry {
            unsafe {
                let _ = DrawIconEx(cv.hdc, self.s(18), (hh - sz) / 2, self.logo, sz, sz, 0, HBRUSH::default(), DI_NORMAL);
            }
        }
        let tx = self.s(18) + sz + self.s(10);
        self.txt(cv, R::new(tx, 0, self.s(300), hh), APP_NAME, 18, true, FG, lvc());
        if settings {
            let r = R::new(w - self.s(18) - self.s(110), (hh - self.s(34)) / 2, self.s(110), self.s(34));
            let label = format!("←  {}", self.t("back"));
            self.button(cv, r, &label, 13, false, Action::Back);
        } else {
            let r = R::new(w - self.s(18) - self.s(40), (hh - self.s(34)) / 2, self.s(40), self.s(34));
            let hov = self.hover(r);
            self.rect(cv, r, if hov { BORDER } else { CARD2 });
            // "sliders" icon: three bars with square knobs
            let (ix, iy, isz) = (r.x + (r.w - self.s(20)) / 2, r.y + (r.h - self.s(20)) / 2, self.s(20));
            for (fy, fk) in [(0.25f32, 0.66f32), (0.5, 0.34), (0.75, 0.6)] {
                let yy = iy + (isz as f32 * fy) as i32;
                let lw = self.s(2).max(1);
                self.rect(cv, R::new(ix + (isz as f32 * 0.12) as i32, yy - lw / 2, (isz as f32 * 0.76) as i32, lw), FG);
                let k = self.s(6);
                let kx = ix + (isz as f32 * fk) as i32 - k / 2;
                self.rect(cv, R::new(kx, yy - k / 2, k, k), FG);
                self.rect(cv, R::new(kx + lw / 2 + 1, yy - k / 2 + lw / 2 + 1, (k - lw - 1).max(1), (k - lw - 1).max(1)), CARD2);
            }
            self.hit(r, Action::OpenSettings);
        }
        self.rect(cv, R::new(0, hh, w, self.bw()), BORDER);
    }

    fn draw_footer(&mut self, cv: &Canvas) {
        let (w, h) = (cv.w, cv.h);
        let fh = self.s(30);
        let top = h - fh;
        self.rect(cv, R::new(0, top - self.bw(), w, self.bw()), BORDER);
        self.rect(cv, R::new(0, top, w, fh), BG);
        let text = self.i18n.tf("version_by", &[("name", APP_NAME.to_string()), ("version", VERSION.to_string()), ("author", AUTHOR.to_string())]);
        self.txt(cv, R::new(self.s(18), top, w / 2, fh), &text, 11, false, MUTED, lvc());
        let link = "GitHub ↗";
        let (tw, _) = self.measure(cv, link, 11, true, 0);
        let r = R::new(w - self.s(18) - tw, top, tw, fh);
        self.txt(cv, R::new(r.x, r.y, r.w + self.s(6), r.h), link, 11, true, ACCENT, lvc());
        self.hit(r, Action::Github);
    }

    // ---- home page ---------------------------------------------------------------------
    fn entry_text_height(&mut self, cv: &Canvas, idx: usize, text_w: i32) -> i32 {
        if self.heights.len() < self.history.len() {
            self.heights.resize(self.history.len(), (0, 0));
        }
        if self.heights[idx].0 != text_w {
            let px = self.s(15);
            let f = self.fonts.get(px, false, true);
            let text = self.history[idx].text.clone();
            let (_, th) = cv.measure(&text, f, text_w);
            self.heights[idx] = (text_w, th);
        }
        self.heights[idx].1
    }

    fn draw_home(&mut self, cv: &Canvas) {
        let (w, h) = (cv.w, cv.h);
        self.draw_header(cv, false);
        self.draw_footer(cv);
        let top = self.s(58) + self.bw();
        let bottom = h - self.s(30) - self.bw();
        let (mx, my) = (self.s(16), self.s(14));
        let area = R::new(mx, top + my, w - 2 * mx, bottom - top - 2 * my);
        let side_w = self.s(340);
        let gap = self.s(12);
        let left = R::new(area.x, area.y, area.w - side_w - gap, area.h);
        let side = R::new(left.right() + gap, area.y, side_w, area.h);
        let bw = self.bw();

        // ---- left card: transcript
        self.boxed(cv, left, CARD, BORDER);
        let head = R::new(left.x + bw, left.y + bw, left.w - 2 * bw, self.s(10) + self.s(28) + self.s(10));
        let title = self.t("transcript").to_uppercase();
        self.txt(cv, R::new(head.x + self.s(14), head.y, head.w / 2, head.h), &title, 11, true, MUTED, lvc());
        let clear = R::new(head.right() - self.s(14) - self.s(70), head.y + self.s(10), self.s(70), self.s(28));
        let clear_label = self.t("clear");
        self.button(cv, clear, &clear_label, 11, false, Action::Clear);
        self.rect(cv, R::new(left.x + bw, head.bottom(), left.w - 2 * bw, bw), BORDER);
        let vp = R::new(left.x + bw + self.s(4), head.bottom() + bw + self.s(4), left.w - 2 * bw - self.s(8), left.bottom() - bw - self.s(4) - (head.bottom() + bw + self.s(4)));

        if !self.dry {
            cv.clip(vp);
        }
        self.clip = Some(vp);
        if self.history.is_empty() {
            let msg = self.t("empty_history");
            let wrap = (left.w - self.s(90)).max(self.s(220));
            let (_, th) = self.measure(cv, &msg, 12, false, wrap);
            self.txt(cv, R::new(vp.x + (vp.w - wrap) / 2, vp.y + self.s(40), wrap, th + 4), &msg, 12, false, MUTED, DT_CENTER | DT_WORDBREAK);
            self.max_scroll_home = 0;
        } else {
            let ew = vp.w - self.s(8);
            let text_w = ew - self.s(24);
            let mut y = vp.y + self.s(4) - self.scroll_home;
            let y_start = y;
            for idx in (0..self.history.len()).rev() {
                let th = self.entry_text_height(cv, idx, text_w);
                let eh = self.s(8) + self.s(24) + self.s(4) + th + self.s(10) + 2 * bw;
                let card = R::new(vp.x + self.s(4), y, ew, eh);
                if card.bottom() > vp.y && card.y < vp.bottom() {
                    self.draw_entry(cv, idx, card, text_w, th);
                }
                y += eh + self.s(8);
            }
            let content = y - y_start + self.s(4);
            self.max_scroll_home = (content - vp.h).max(0);
        }
        self.clip = None;
        if !self.dry {
            cv.unclip();
        }
        self.scroll_home = self.scroll_home.clamp(0, self.max_scroll_home);

        // ---- right sidebar: status card
        let c1 = R::new(side.x, side.y, side.w, 2 * bw + self.s(12) + self.s(42) + self.s(12));
        self.boxed(cv, c1, CARD, BORDER);
        let ix = c1.x + bw + self.s(14);
        let iy = c1.y + bw + self.s(12);
        let led = match self.cur_state {
            State::Recording => REC,
            State::Processing => ACCENT,
            State::Idle => MUTED,
        };
        let ls = self.s(14);
        self.rect(cv, R::new(ix + self.s(2), iy + (self.s(42) - ls) / 2, ls, ls), led);
        let tx = ix + self.s(2) + ls + self.s(12);
        let (status, status_color) = match &self.status_msg {
            Some(m) => (m.chars().take(60).collect::<String>(), REC),
            None => (
                self.t(match self.cur_state {
                    State::Recording => "status_recording",
                    State::Processing => "status_processing",
                    State::Idle => "status_idle",
                }),
                FG,
            ),
        };
        self.txt(cv, R::new(tx, iy, c1.right() - tx - self.s(8), self.s(23)), &status, 15, true, status_color, lvc());
        let hint_key = if self.cfg.str("record_mode") == "hold" { "hint_hold" } else { "hint_toggle" };
        let hint = self.i18n.tf(hint_key, &[("hotkey", pretty(&self.cfg.str("hotkey_dictate")))]);
        self.txt(cv, R::new(tx, iy + self.s(23), c1.right() - tx - self.s(8), self.s(19)), &hint, 12, false, MUTED, lvc());

        // ---- right sidebar: language / mode card
        let translate_on = self.cfg.bool("translate_enabled");
        let c2h = 2 * bw + self.s(12 + 16 + 2 + 36 + 10 + 16 + 2 + 36 + 14 + 34 + 4 + 16 + 12);
        let c2 = R::new(side.x, c1.bottom() + self.s(12), side.w, c2h);
        self.boxed(cv, c2, CARD, BORDER);
        let (cx, cw) = (c2.x + bw + self.s(14), c2.w - 2 * bw - self.s(28));
        let mut cy = c2.y + bw + self.s(12);
        let l1 = self.t("i_speak");
        self.txt(cv, R::new(cx, cy, cw, self.s(16)), &l1, 11, false, MUTED, lvc());
        cy += self.s(16) + self.s(2);
        let b1 = R::new(cx, cy, cw, self.s(36));
        self.lang_button(cv, b1, &self.lang_label(&self.cfg.str("speak_lang")), true, Action::OpenPopup(PopupKind::Speak));
        cy += self.s(36) + self.s(10);
        let l2 = self.t("translate_to");
        self.txt(cv, R::new(cx, cy, cw, self.s(16)), &l2, 11, false, MUTED, lvc());
        cy += self.s(16) + self.s(2);
        let b2 = R::new(cx, cy, cw, self.s(36));
        self.lang_button(cv, b2, &self.lang_label(&self.cfg.str("target_lang")), translate_on, Action::OpenPopup(PopupKind::Target));
        cy += self.s(36) + self.s(14);
        // Dictate | Translate (full width)
        let labels = [self.t("mode_dictate"), self.t("mode_translate")];
        let seg_h = self.s(34);
        let pad = self.s(3);
        self.rect(cv, R::new(cx, cy, cw, seg_h), CARD2);
        let half = (cw - 2 * pad) / 2;
        for i in 0..2 {
            let seg = R::new(cx + pad + i as i32 * half, cy + pad, half, seg_h - 2 * pad);
            let selected = (i == 1) == translate_on;
            let hov = self.hover(seg);
            let bg = if selected { if hov { ACCENT_HOVER } else { ACCENT } } else if hov { BORDER } else { CARD2 };
            self.rect(cv, seg, bg);
            self.txt(cv, seg, &labels[i], 13, false, FG, cvc());
            self.hit(seg, Action::SetTranslate(i == 1));
        }
        cy += seg_h + self.s(4);
        let mh = self.i18n.tf("mode_hint", &[("hotkey", pretty(&self.cfg.str("hotkey_translate")))]);
        self.txt(cv, R::new(cx, cy, cw, self.s(16)), &mh, 11, false, MUTED, lvc());
    }

    fn lang_button(&mut self, cv: &Canvas, r: R, label: &str, enabled: bool, a: Action) {
        let hov = enabled && self.hover(r);
        self.rect(cv, r, if hov { BORDER } else { CARD2 });
        let text = format!("{}  ▾", label);
        self.txt(cv, R::new(r.x + self.s(12), r.y, r.w - self.s(16), r.h), &text, 13, false, if enabled { FG } else { MUTED }, lvc());
        if enabled {
            self.hit(r, a);
        }
    }

    fn draw_entry(&mut self, cv: &Canvas, idx: usize, card: R, text_w: i32, th: i32) {
        self.boxed(cv, card, BG, BORDER);
        let e = self.history[idx].clone();
        let lang = match &e.dst {
            Some(d) => format!("{}  →  {}", short(&e.src), short(d)),
            None => short(&e.src),
        };
        let meta = format!("{}   ·   {}", e.time, lang);
        let top = card.y + self.s(8);
        self.txt(cv, R::new(card.x + self.s(12), top, card.w - self.s(100), self.s(24)), &meta, 11, false, MUTED, lvc());
        let copy = R::new(card.right() - self.s(12) - self.s(56), top, self.s(56), self.s(24));
        let label = if self.copied == Some(idx) { self.t("copied") } else { self.t("copy") };
        self.button(cv, copy, &label, 11, false, Action::Copy(idx));
        let tr = R::new(card.x + self.s(12), top + self.s(24) + self.s(4), text_w, th + 2);
        self.txt_nirmala(cv, tr, &e.text, 15, FG, DT_LEFT | DT_WORDBREAK);
    }

    // ---- settings page ----------------------------------------------------------------
    fn draw_settings(&mut self, cv: &Canvas) {
        let (w, h) = (cv.w, cv.h);
        self.draw_header(cv, true);
        self.draw_footer(cv);
        let top = self.s(58) + self.bw();
        let bottom = h - self.s(30) - self.bw();
        let vp = R::new(0, top, w, bottom - top);
        let cw = self.s(580).min(w - self.s(40));
        let x0 = (w - cw) / 2;
        self.slider = R::default();

        if !self.dry {
            cv.clip(vp);
        }
        self.clip = Some(vp);
        let start = top + self.s(8) - self.scroll_settings;
        let mut y = start;
        for which in 0..5u8 {
            let title = match which {
                0 => self.t("hotkeys"),
                1 => self.t("recording"),
                2 => self.t("language"),
                3 => self.t("output"),
                _ => self.t("system"),
            };
            y += self.s(14);
            self.txt(cv, R::new(x0 + self.s(4), y, cw, self.s(16)), &title.to_uppercase(), 11, true, MUTED, lvc());
            y += self.s(16) + self.s(4);
            let (card_x, card_w, bw) = (x0 + self.s(2), cw - self.s(4), self.bw());
            let was = self.dry;
            self.dry = true;
            let body_h = self.section_body(cv, which, card_x + bw, y + bw, card_w - 2 * bw);
            self.dry = was;
            let card = R::new(card_x, y, card_w, body_h + 2 * bw);
            self.boxed(cv, card, CARD, BORDER);
            self.section_body(cv, which, card_x + bw, y + bw, card_w - 2 * bw);
            y += card.h + self.s(2);
        }
        y += self.s(8);
        self.clip = None;
        if !self.dry {
            cv.unclip();
        }
        self.content_h = y - start + self.s(8);
        let max = (self.content_h - vp.h).max(0);
        self.scroll_settings = self.scroll_settings.clamp(0, max);
    }

    fn section_body(&mut self, cv: &Canvas, which: u8, x: i32, y: i32, w: i32) -> i32 {
        let mut cy = y;
        match which {
            0 => {
                for (lk, ck) in [("dictation_hotkey", "hotkey_dictate"), ("translate_hotkey", "hotkey_translate")] {
                    cy += self.s(10);
                    let label = self.t(lk);
                    self.txt(cv, R::new(x + self.s(14), cy, w - self.s(200), self.s(34)), &label, 13, false, FG, lvc());
                    let br = R::new(x + w - self.s(14) - self.s(170), cy, self.s(170), self.s(34));
                    let capturing = self.capture_key == Some(ck);
                    let text = if capturing { self.t("press_keys") } else { pretty(&self.cfg.str(ck)) };
                    self.button(cv, br, &text, 13, capturing, Action::Capture(ck));
                    cy += self.s(34) + self.s(10);
                }
            }
            1 => {
                cy += self.s(12);
                let l = self.t("record_mode");
                self.txt(cv, R::new(x + self.s(16), cy, w, self.s(18)), &l, 13, false, FG, lvc());
                cy += self.s(18) + self.s(4);
                let labels = [self.t("mode_hold"), self.t("mode_toggle")];
                let sel = if self.cfg.str("record_mode") == "hold" { 0 } else { 1 };
                self.segmented(cv, x + self.s(14), cy, &labels, sel, vec![Action::RecMode(true), Action::RecMode(false)]);
                cy += self.s(34) + self.s(8);
                cy += self.s(10);
                let l = self.t("auto_stop");
                let on = self.cfg.bool("auto_stop");
                cy += self.switch(cv, x + self.s(14), cy, &l, on, "auto_stop") + self.s(2);
                let secs = self.cfg.f64("auto_stop_seconds");
                let l = self.i18n.tf("auto_stop_after", &[("s", format!("{:.1}", secs))]);
                self.txt(cv, R::new(x + self.s(16), cy, w, self.s(18)), &l, 12, false, MUTED, lvc());
                cy += self.s(18) + self.s(2);
                // slider 1..6 in 0.5 steps
                let track = R::new(x + self.s(16), cy, w - self.s(32), self.s(16));
                let kw = self.s(14);
                let frac = ((secs - 1.0) / 5.0).clamp(0.0, 1.0) as f32;
                let kx = track.x + ((track.w - kw) as f32 * frac) as i32;
                let bar = R::new(track.x, track.y + self.s(5), track.w, self.s(6));
                self.rect(cv, bar, BORDER);
                self.rect(cv, R::new(bar.x, bar.y, kx - bar.x + kw / 2, bar.h), ACCENT);
                let hov = self.hover(track) || self.drag;
                self.rect(cv, R::new(kx, track.y, kw, track.h), if hov { KNOB_HOVER } else { FG });
                self.hit(track, Action::Slider);
                if !self.dry {
                    self.slider = track;
                }
                cy += self.s(16) + self.s(10);
                cy += self.s(12);
                let l = self.t("microphone");
                self.txt(cv, R::new(x + self.s(16), cy, w, self.s(18)), &l, 13, false, FG, lvc());
                cy += self.s(18) + self.s(4);
                let cur = match self.cfg.opt_str("mic") {
                    Some(m) if self.mic_names.contains(&m) => m,
                    _ => self.t("default_mic"),
                };
                let r = R::new(x + self.s(14), cy, self.s(300).min(w - self.s(28)), self.s(34));
                let hov = self.hover(r);
                self.rect(cv, r, if hov { BORDER } else { CARD2 });
                self.txt(cv, R::new(r.x + self.s(12), r.y, r.w - self.s(36), r.h), &cur, 13, false, FG, lvc());
                self.txt(cv, R::new(r.right() - self.s(26), r.y, self.s(20), r.h), "▾", 13, false, FG, cvc());
                self.hit(r, Action::OpenPopup(PopupKind::Mic));
                cy += self.s(34) + self.s(8);
            }
            2 => {
                cy += self.s(10);
                let l = self.t("app_language");
                self.txt(cv, R::new(x + self.s(14), cy, w / 2, self.s(34)), &l, 13, false, FG, lvc());
                let labels = ["English".to_string(), "বাংলা".to_string()];
                let sw = self.segmented_width(cv, &labels);
                let sel = if self.cfg.str("ui_language") == "bn" { 1 } else { 0 };
                self.segmented(cv, x + w - self.s(14) - sw, cy, &labels, sel, vec![Action::UiLang("en"), Action::UiLang("bn")]);
                cy += self.s(34) + self.s(10);
            }
            3 => {
                cy += self.s(10);
                let l = self.t("output_method");
                self.txt(cv, R::new(x + self.s(14), cy, w / 2, self.s(34)), &l, 13, false, FG, lvc());
                let labels = [self.t("out_paste"), self.t("out_type")];
                let sw = self.segmented_width(cv, &labels);
                let sel = if self.cfg.str("output") == "paste" { 0 } else { 1 };
                self.segmented(cv, x + w - self.s(14) - sw, cy, &labels, sel, vec![Action::Output("paste"), Action::Output("type")]);
                cy += self.s(34) + self.s(10);
                cy += self.s(2);
                let l = self.t("trailing_space");
                let on = self.cfg.bool("trailing_space");
                cy += self.switch(cv, x + self.s(14), cy, &l, on, "trailing_space") + self.s(12);
            }
            _ => {
                let items = [
                    ("start_with_windows", "start_with_windows_d", 12, false),
                    ("start_minimized", "start_minimized_d", 8, false),
                    ("close_to_tray", "close_to_tray_d", 8, true),
                ];
                for (key, dkey, pad_top, last) in items {
                    cy += self.s(pad_top);
                    let l = self.t(key);
                    let on = self.cfg.bool(key);
                    cy += self.switch(cv, x + self.s(14), cy, &l, on, key);
                    let d = self.t(dkey);
                    let wrap = self.s(500).min(w - self.s(32));
                    let (_, th) = self.measure(cv, &d, 11, false, wrap);
                    cy += self.s(2);
                    self.txt(cv, R::new(x + self.s(16), cy, wrap, th + 2), &d, 11, false, MUTED, DT_LEFT | DT_WORDBREAK);
                    cy += th + if last { self.s(12) } else { self.s(4) };
                }
            }
        }
        cy - y
    }

    // ---- popup (language picker / microphone list) ---------------------------------------
    fn open_popup(&mut self, kind: PopupKind, anchor: R) {
        let mut items: Vec<Item> = Vec::new();
        let langs = |items: &mut Vec<Item>| {
            for (code, _s, en, native) in LANGS.iter() {
                items.push(Item { id: code.to_string(), label: display_name(code), hay: format!("{} {}", en, native).to_lowercase() });
            }
        };
        let selected;
        match kind {
            PopupKind::Speak => {
                let a = self.t("auto_detect");
                items.push(Item { id: "auto".into(), label: a.clone(), hay: a.to_lowercase() });
                langs(&mut items);
                selected = self.cfg.str("speak_lang");
            }
            PopupKind::Target => {
                langs(&mut items);
                selected = self.cfg.str("target_lang");
            }
            PopupKind::Mic => {
                self.mic_names = audio::list_input_devices();
                let d = self.t("default_mic");
                items.push(Item { id: String::new(), label: d.clone(), hay: d.to_lowercase() });
                for n in &self.mic_names {
                    items.push(Item { id: n.clone(), label: n.clone(), hay: n.to_lowercase() });
                }
                selected = self.cfg.opt_str("mic").unwrap_or_default();
            }
        }
                self.popup = Some(Popup { kind, anchor, items, selected, query: String::new(), scroll: 0 });
        unsafe {
            let _ = SetFocus(self.hwnd);
            SetTimer(self.hwnd, T_CARET, 530, None); // blinking caret
        }
        self.redraw();
    }

    fn draw_popup(&mut self, cv: &Canvas) {
        let Some(mut p) = self.popup.take() else { return };
        self.block_hover = false;
        let (w, h) = (cv.w, cv.h);
        let search = p.kind != PopupKind::Mic;
        let q = p.query.trim().to_lowercase();
        let idx: Vec<usize> = (0..p.items.len()).filter(|&i| q.is_empty() || p.items[i].hay.contains(&q)).collect();
        let step = self.s(36);
        let (pw, ph) = if search {
            (self.s(320), self.s(420))
        } else {
            (p.anchor.w.max(self.s(220)), (self.s(8) + idx.len() as i32 * step).clamp(self.s(60), self.s(320)))
        };
        let px = p.anchor.x.min(w - pw - self.s(10)).max(0);
        let mut py = p.anchor.bottom() + self.s(4);
        if py + ph > h - self.s(10) {
            py = (p.anchor.y - ph - self.s(4)).max(0);
        }
        let panel = R::new(px, py, pw, ph);
        self.hit(R::new(0, 0, w, h), Action::ClosePopup);
        self.hit(panel, Action::Noop);
        self.boxed(cv, panel, BG, BORDER);
        let bw = self.bw();
        let list;
        if search {
                        let sb = R::new(px + self.s(12), py + self.s(12), pw - self.s(24), self.s(36));
            self.boxed(cv, sb, CARD2, ACCENT); // orange border = active input
            let inner = R::new(sb.x + self.s(10), sb.y, sb.w - self.s(20), sb.h);
            let blink = (unsafe { GetTickCount64() } / 500) % 2 == 0;
            let cw = bw.max(self.s(1));
            if p.query.is_empty() {
                let ph_text = self.t("search");
                self.txt(cv, inner, &ph_text, 13, false, MUTED, lvc());
                if blink {
                    self.rect(cv, R::new(inner.x, sb.y + self.s(9), cw, sb.h - self.s(18)), FG);
                }
            } else {
                self.txt(cv, inner, &p.query, 13, false, FG, lvc());
                let (tw, _) = self.measure(cv, &p.query, 13, false, 0);
                if blink {
                    self.rect(cv, R::new(inner.x + tw + self.s(2), sb.y + self.s(9), cw, sb.h - self.s(18)), FG);
                }
            }
            let y0 = sb.bottom() + self.s(12);
            list = R::new(px + self.s(6), y0, pw - self.s(12), py + ph - self.s(10) - y0);
        } else {
            list = R::new(px + self.s(6), py + self.s(4), pw - self.s(12), ph - self.s(8));
        }
        let max_scroll = (idx.len() as i32 * step - list.h).max(0);
        p.scroll = p.scroll.clamp(0, max_scroll);
        if !self.dry {
            cv.clip(list);
        }
        self.clip = Some(list);
        for (n, &i) in idx.iter().enumerate() {
            let r = R::new(list.x, list.y - p.scroll + n as i32 * step + self.s(1), list.w, self.s(34));
            if r.bottom() < list.y || r.y > list.bottom() {
                continue;
            }
            let sel = p.items[i].id == p.selected;
            let hov = self.hover(r);
            if sel {
                self.rect(cv, r, if hov { ACCENT_HOVER } else { ACCENT });
            } else if hov {
                self.rect(cv, r, CARD2);
            }
            let label = p.items[i].label.clone();
            self.txt(cv, R::new(r.x + self.s(12), r.y, r.w - self.s(16), r.h), &label, 13, false, FG, lvc());
            self.hit(r, Action::Pick(p.items[i].id.clone()));
        }
        self.clip = None;
        if !self.dry {
            cv.unclip();
        }
        self.popup = Some(p);
    }

    // ---- painting ------------------------------------------------------------------------
    fn paint(&mut self, cv: &Canvas) {
        cv.fill(R::new(0, 0, cv.w, cv.h), BG);
        self.hits.clear();
        self.dry = false;
        self.block_hover = self.popup.is_some();
        match self.page {
            Page::Home => self.draw_home(cv),
            Page::Settings => self.draw_settings(cv),
        }
        self.draw_popup(cv);
    }

    fn on_paint(&mut self, hwnd: HWND) {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
            if w > 0 && h > 0 {
                let need = match &self.canvas {
                    Some(c) => c.w != w || c.h != h,
                    None => true,
                };
                if need {
                    self.canvas = None;
                    self.canvas = Some(Canvas::new(hdc, w, h));
                }
                let cv = self.canvas.take().unwrap();
                self.paint(&cv);
                let _ = BitBlt(hdc, 0, 0, w, h, cv.hdc, 0, 0, SRCCOPY);
                self.canvas = Some(cv);
            }
            let _ = EndPaint(hwnd, &ps);
        }
    }

    /// Free the back buffer and give memory back to Windows while the window is hidden.
    fn release_memory(&mut self) {
        self.canvas = None;
        self.fonts.clear();
        system::trim_memory();
    }

    // ---- actions ---------------------------------------------------------------------------
    fn set_translate(&mut self, on: bool) {
        self.cfg.set_bool("translate_enabled", on);
        let (src, dst) = (self.cfg.str("speak_lang"), self.cfg.str("target_lang"));
        let text = if on { format!("{} → {}", short(&src), short(&dst)) } else { self.t("mode_dictate") };
        self.ov().notice(&text, if on { Some(ACCENT) } else { None }, 1200);
        self.redraw();
    }

    fn set_ui_language(&mut self, code: &'static str) {
        if code == "en" || has_locale(code) {
            self.apply_ui_language(code);
            return;
        }
        let msg = self.t("ui_translating");
        self.ov().notice(&msg, Some(ACCENT), 15000);
        std::thread::spawn(move || match build_locale(code) {
            Ok(()) => events::emit(Event::UiLangReady(code.to_string())),
            Err(e) => {
                eprintln!("ui language error: {e}");
                events::emit(Event::Error("err_ui_lang".into()));
            }
        });
    }

    fn apply_ui_language(&mut self, code: &str) {
        self.cfg.set_str("ui_language", code);
        self.i18n.load(code);
        let bn = self.bn();
        self.ov().bn = bn;
        self.fonts.clear();
        self.heights.clear();
        self.redraw();
    }

    fn pick(&mut self, id: String) {
        let Some(p) = self.popup.take() else { return };
        match p.kind {
            PopupKind::Speak => self.cfg.set_str("speak_lang", &id),
            PopupKind::Target => self.cfg.set_str("target_lang", &id),
            PopupKind::Mic => {
                if id.is_empty() {
                    self.cfg.set("mic", serde_json::Value::Null)
                } else {
                    self.cfg.set_str("mic", &id)
                }
            }
        }
    }

    fn slider_set(&mut self, x: i32) {
        let r = self.slider;
        if r.w <= 0 {
            return;
        }
        let kw = self.s(14);
        let frac = ((x - r.x - kw / 2) as f64 / (r.w - kw) as f64).clamp(0.0, 1.0);
        let v = ((1.0 + 5.0 * frac) * 2.0).round() / 2.0;
        self.cfg.set("auto_stop_seconds", serde_json::json!(v));
    }

    fn perform(&mut self, hwnd: HWND, a: Action, r: R) {
        match a {
            Action::Noop => {}
            Action::OpenSettings => {
                self.popup = None;
                self.page = Page::Settings;
                self.scroll_settings = 0;
                self.mic_names = audio::list_input_devices();
            }
            Action::Back => {
                self.popup = None;
                self.page = Page::Home;
            }
            Action::Clear => {
                self.history.clear();
                self.heights.clear();
                self.scroll_home = 0;
            }
            Action::Copy(i) => {
                if let Some(e) = self.history.get(i) {
                    inject::set_clipboard_text(&e.text);
                    self.copied = Some(i);
                    unsafe {
                        SetTimer(hwnd, T_COPIED, 1200, None);
                    }
                }
            }
            Action::OpenPopup(kind) => self.open_popup(kind, r),
            Action::ClosePopup => self.popup = None,
            Action::Pick(id) => self.pick(id),
            Action::SetTranslate(on) => self.set_translate(on),
            Action::Capture(key) => {
                self.capture_key = Some(key);
                hotkeys::capture();
            }
            Action::RecMode(hold) => self.cfg.set_str("record_mode", if hold { "hold" } else { "toggle" }),
            Action::Toggle(key) => {
                let v = !self.cfg.bool(key);
                self.cfg.set_bool(key, v);
                if key == "start_with_windows" {
                    system::set_autostart(v);
                }
            }
            Action::Slider => {
                self.drag = true;
                unsafe {
                    SetCapture(hwnd);
                }
                self.slider_set(self.mx);
            }
            Action::UiLang(code) => self.set_ui_language(code),
            Action::Output(m) => self.cfg.set_str("output", m),
            Action::Github => unsafe {
                let url = wide_z(GITHUB_URL);
                ShellExecuteW(HWND::default(), w!("open"), pcwstr(&url), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
            },
        }
        self.redraw();
    }

    // ---- events from worker threads ---------------------------------------------------------
    fn handle(&mut self, hwnd: HWND, ev: Event) {
        match ev {
            Event::Level(v) => self.ov().push_level(v),
            Event::State(st) => {
                self.cur_state = st;
                let label = self.label();
                self.ov().set_state(st, &label);
                self.redraw();
                if st == State::Idle {
                    unsafe {
                        if !IsWindowVisible(hwnd).as_bool() {
                            SetTimer(hwnd, T_TRIM, 2000, None);
                        }
                    }
                }
            }
            Event::Result(e) => {
                self.history.push(e);
                self.redraw();
            }
            Event::Error(m) => {
                let msg = if m.starts_with("err_") { self.t(&m) } else { m };
                self.ov().notice(&msg, Some(REC), 3000);
                self.status_msg = Some(msg);
                unsafe {
                    SetTimer(hwnd, T_STATUS, 3000, None);
                }
                self.redraw();
            }
            Event::UiLangReady(code) => {
                self.ov().end_notice();
                self.apply_ui_language(&code);
            }
            Event::Notice(key) => {
                let t = self.t(&key);
                self.ov().notice(&t, None, 1500);
            }
            Event::Hotkey(name, down) => self.on_hotkey(&name, down),
            Event::Capture(combo) => {
                if let (Some(c), Some(key)) = (combo, self.capture_key) {
                    self.cfg.set_str(key, &c);
                    hotkeys::set_binding(if key == "hotkey_dictate" { "dictate" } else { "translate" }, &c);
                }
                self.capture_key = None;
                self.redraw();
            }
        }
    }

    fn on_hotkey(&mut self, name: &str, down: bool) {
        if name == "dictate" {
            let hold = self.cfg.str("record_mode") == "hold";
            if down {
                if hold || self.engine.state() != State::Recording {
                    let _ = self.cmd_tx.send(Cmd::Start);
                } else {
                    let _ = self.cmd_tx.send(Cmd::Stop(false));
                }
            } else if hold {
                let _ = self.cmd_tx.send(Cmd::Stop(false));
            }
        } else if name == "translate" && down {
            let on = !self.cfg.bool("translate_enabled");
            self.set_translate(on);
        }
    }

    fn show_window(&mut self, hwnd: HWND) {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            } else {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            let _ = SetForegroundWindow(hwnd);
        }
        self.redraw();
    }

    fn quit(&mut self, hwnd: HWND) -> ! {
        tray::remove(hwnd);
        std::process::exit(0);
    }
}

// ======================================================================================
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(a) = app_opt() else { return DefWindowProcW(hwnd, msg, wp, lp) };
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            a.on_paint(hwnd);
            LRESULT(0)
        }
        WM_SIZE => {
            a.canvas = None;
            if wp.0 as u32 == SIZE_MINIMIZED {
                a.release_memory();
            }
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let mmi = &mut *(lp.0 as *mut MINMAXINFO);
            mmi.ptMinTrackSize = POINT { x: a.s(860), y: a.s(560) };
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            a.mx = loword(lp.0);
            a.my = hiword(lp.0);
            if !a.tracking {
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                let _ = TrackMouseEvent(&mut t);
                a.tracking = true;
            }
            if a.drag {
                a.slider_set(a.mx);
            }
            a.redraw();
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            a.tracking = false;
            a.mx = -1;
            a.my = -1;
            a.redraw();
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if loword(lp.0) == 1 {
                let over = a.hits.iter().rev().find(|h| h.r.contains(a.mx, a.my)).map(|h| h.a.clone());
                let hand = !matches!(over, None | Some(Action::Noop) | Some(Action::ClosePopup));
                let id = if hand { IDC_HAND } else { IDC_ARROW };
                if let Ok(c) = LoadCursorW(None, id) {
                    SetCursor(c);
                }
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_LBUTTONDOWN => {
            let _ = SetFocus(hwnd);
            a.mx = loword(lp.0);
            a.my = hiword(lp.0);
            let found = a.hits.iter().rev().find(|h| h.r.contains(a.mx, a.my)).map(|h| (h.a.clone(), h.r));
            if let Some((act, r)) = found {
                a.perform(hwnd, act, r);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if a.drag {
                a.drag = false;
                let _ = ReleaseCapture();
                a.redraw();
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = hiword(wp.0 as isize);
            let step = -(delta * a.s(60)) / 120;
            if let Some(p) = a.popup.as_mut() {
                p.scroll = (p.scroll + step).max(0);
            } else if a.page == Page::Settings {
                a.scroll_settings = (a.scroll_settings + step).max(0);
            } else {
                a.scroll_home = (a.scroll_home + step).clamp(0, a.max_scroll_home);
            }
            a.redraw();
            LRESULT(0)
        }
                WM_CHAR => {
            if let Some(p) = a.popup.as_mut() {
                if p.kind != PopupKind::Mic {
                    let c = wp.0 as u32;
                    if c == 8 {
                        p.query.pop();
                    } else if c == 22 {
                        // Ctrl+V
                        if let Some(t) = inject::get_clipboard_text() {
                            p.query.push_str(t.lines().next().unwrap_or(""));
                        }
                    } else if c >= 32 && c != 127 {
                        if let Some(ch) = char::from_u32(c) {
                            p.query.push(ch);
                        }
                    }
                    p.scroll = 0;
                    a.redraw();
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 as u16 == VK_ESCAPE.0 && a.popup.is_some() {
                a.popup = None;
                a.redraw();
            }
            LRESULT(0)
        }
        WM_TIMER => {
            let _ = KillTimer(hwnd, wp.0);
            match wp.0 {
                T_STATUS => {
                    a.status_msg = None;
                    a.redraw();
                }
                T_COPIED => {
                    a.copied = None;
                    a.redraw();
                }
                                T_CARET => {
                    if a.popup.is_some() {
                        SetTimer(hwnd, T_CARET, 530, None);
                        a.redraw();
                    }
                }
                T_TRIM => {
                    if !IsWindowVisible(hwnd).as_bool() {
                        a.release_memory();
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_EVENTS => {
            for ev in events::drain() {
                a.handle(hwnd, ev);
            }
            LRESULT(0)
        }
        WM_TRAY => {
            match loword(lp.0) as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => a.show_window(hwnd),
                WM_RBUTTONUP => {
                    let labels = (a.t("tray_open"), a.t("tray_translate"), a.t("tray_quit"));
                    let cmd = tray::show_menu(hwnd, (labels.0.as_str(), labels.1.as_str(), labels.2.as_str()), a.cfg.bool("translate_enabled"));
                    match cmd {
                        tray::ID_SHOW => a.show_window(hwnd),
                        tray::ID_TRANSLATE => {
                            let on = !a.cfg.bool("translate_enabled");
                            a.set_translate(on);
                        }
                        tray::ID_QUIT => a.quit(hwnd),
                        _ => {}
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_SHOWAPP => {
            a.show_window(hwnd);
            LRESULT(0)
        }
        WM_SHOWWINDOW => {
            if wp.0 == 0 {
                SetTimer(hwnd, T_TRIM, 1500, None);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CLOSE => {
            if a.cfg.bool("close_to_tray") {
                let _ = ShowWindow(hwnd, SW_HIDE);
            } else {
                a.quit(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            a.quit(hwnd);
        }
        m if m == TASKBAR_CREATED.load(Ordering::Relaxed) && m != 0 => {
            tray::add(hwnd, a.tray_icon, APP_NAME);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn load_icon(hinst: HINSTANCE, size: i32) -> HICON {
    unsafe {
        LoadImageW(hinst, PCWSTR(1 as *const u16), IMAGE_ICON, size, size, LR_DEFAULTCOLOR)
            .map(|h| HICON(h.0))
            .unwrap_or_default()
    }
}

pub fn run(start_hidden: bool) {
    unsafe {
        let _ = SetProcessDPIAware();
        let sc = GetDpiForSystem() as f32 / 96.0;
        let hinst = GetModuleHandleW(None).map(|m| HINSTANCE(m.0)).unwrap_or_default();

        let cfg = Cfg::load();
        let i18n = I18n::new(&cfg.str("ui_language"));
        let engine = Engine::new(cfg.clone());
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        {
            let eng = engine.clone();
            std::thread::spawn(move || {
                for c in cmd_rx {
                    match c {
                        Cmd::Start => eng.start(),
                        Cmd::Stop(cancel) => eng.stop(cancel),
                    }
                }
            });
        }

        let logo = load_icon(hinst, (30.0 * sc) as i32);
        let small = load_icon(hinst, GetSystemMetrics(SM_CXSMICON));
        let big = load_icon(hinst, GetSystemMetrics(SM_CXICON));

        let app = Box::new(App {
            hwnd: HWND::default(),
            sc,
            cfg: cfg.clone(),
            i18n,
            engine,
            cmd_tx,
            overlay: None,
            logo,
            tray_icon: small,
            history: Vec::new(),
            heights: Vec::new(),
            cur_state: State::Idle,
            page: Page::Home,
            popup: None,
            mic_names: Vec::new(),
            capture_key: None,
            status_msg: None,
            copied: None,
            canvas: None,
            fonts: Fonts::default(),
            hits: Vec::new(),
            clip: None,
            dry: false,
            block_hover: false,
            mx: -1,
            my: -1,
            tracking: false,
            drag: false,
            slider: R::default(),
            scroll_home: 0,
            scroll_settings: 0,
            content_h: 0,
            max_scroll_home: 0,
        });
        APP.store(Box::into_raw(app), Ordering::SeqCst);
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);

        let class = wide_z("InzelMain");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: pcwstr(&class),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hIcon: big,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let a = app_opt().unwrap();
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let ww = a.s(1100).min(sw - 80);
        let wh = a.s(720).min(sh - 100);
        let title = wide_z(APP_NAME);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            pcwstr(&class),
            pcwstr(&title),
            WS_OVERLAPPEDWINDOW,
            (sw - ww) / 2,
            ((sh - wh) / 2 - 20).max(0),
            ww,
            wh,
            None,
            None,
            hinst,
            None,
        )
        .expect("cannot create the main window");
        a.hwnd = hwnd;
        events::set_main(hwnd);
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const i32 as *const _, 4);
        SendMessageW(hwnd, WM_SETICON, WPARAM(0), LPARAM(small.0 as isize));

        let mut ov = Overlay::create(hinst, sc);
        ov.bn = a.bn();
        a.overlay = Some(ov);
        tray::add(hwnd, small, APP_NAME);
        hotkeys::set_binding("dictate", &cfg.str("hotkey_dictate"));
        hotkeys::set_binding("translate", &cfg.str("hotkey_translate"));
        hotkeys::start();

        if !(start_hidden || cfg.bool("start_minimized")) {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = UpdateWindow(hwnd);
        } else {
            SetTimer(hwnd, T_TRIM, 3000, None);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
