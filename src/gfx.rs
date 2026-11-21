//! Tiny GDI toolkit: rectangles, borders, text, cached fonts and an off-screen canvas.
//! The whole UI uses square corners, so plain GDI rectangles are all we need.
use crate::util::{pcwstr, wide, wide_z};
use std::collections::HashMap;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct R {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl R {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> R {
        R { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
    pub fn inset(&self, d: i32) -> R {
        R::new(self.x + d, self.y + d, (self.w - 2 * d).max(0), (self.h - 2 * d).max(0))
    }
    pub fn intersect(&self, o: &R) -> Option<R> {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        if r > x && b > y {
            Some(R::new(x, y, r - x, b - y))
        } else {
            None
        }
    }
    pub fn rect(&self) -> RECT {
        RECT { left: self.x, top: self.y, right: self.right(), bottom: self.bottom() }
    }
}

/// Font cache keyed by (pixel height, bold, bangla family).
#[derive(Default)]
pub struct Fonts {
    cache: HashMap<(i32, bool, bool), HFONT>,
}

impl Fonts {
    pub fn get(&mut self, px: i32, bold: bool, nirmala: bool) -> HFONT {
        if let Some(f) = self.cache.get(&(px, bold, nirmala)) {
            return *f;
        }
        let face = wide_z(if nirmala { "Nirmala UI" } else { "Segoe UI" });
        let f = unsafe {
            CreateFontW(
                -px,
                0,
                0,
                0,
                if bold { 700 } else { 400 },
                0,
                0,
                0,                
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                0,
                pcwstr(&face),
            )
        };
        self.cache.insert((px, bold, nirmala), f);
        f
    }

    pub fn clear(&mut self) {
        for (_, f) in self.cache.drain() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(f.0));
            }
        }
    }
}

/// Off-screen bitmap we draw on, then blit to the window in one go (no flicker).
pub struct Canvas {
    pub hdc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    pub w: i32,
    pub h: i32,
}

impl Canvas {
    pub fn new(like: HDC, w: i32, h: i32) -> Canvas {
        unsafe {
            let hdc = CreateCompatibleDC(like);
            let bmp = CreateCompatibleBitmap(like, w.max(1), h.max(1));
            let old = SelectObject(hdc, HGDIOBJ(bmp.0));
            Canvas { hdc, bmp, old, w, h }
        }
    }

    pub fn fill(&self, r: R, color: u32) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        unsafe {
            SetDCBrushColor(self.hdc, COLORREF(color));
            let brush = HBRUSH(GetStockObject(DC_BRUSH).0);
            FillRect(self.hdc, &r.rect(), brush);
        }
    }

    /// Filled rectangle with a border of `bw` pixels.
    pub fn boxed(&self, r: R, fill: u32, border: u32, bw: i32) {
        self.fill(r, border);
        self.fill(r.inset(bw), fill);
    }

    pub fn text(&self, r: R, s: &str, font: HFONT, color: u32, flags: DRAW_TEXT_FORMAT) {
        if s.is_empty() {
            return;
        }
        let mut w = wide(s);
        let mut rc = r.rect();
        unsafe {
            SelectObject(self.hdc, HGDIOBJ(font.0));
            SetTextColor(self.hdc, COLORREF(color));
            SetBkMode(self.hdc, TRANSPARENT);
            DrawTextW(self.hdc, &mut w, &mut rc, flags | DT_NOPREFIX);
        }
    }

    /// (width, height) of the text; wraps at `max_w` when it is > 0.
    pub fn measure(&self, s: &str, font: HFONT, max_w: i32) -> (i32, i32) {
        let mut w = wide(if s.is_empty() { " " } else { s });
        let mut rc = RECT { left: 0, top: 0, right: max_w.max(0), bottom: 0 };
        let mut flags = DT_CALCRECT | DT_NOPREFIX | DT_LEFT;
        if max_w > 0 {
            flags |= DT_WORDBREAK;
        } else {
            flags |= DT_SINGLELINE;
        }
        unsafe {
            SelectObject(self.hdc, HGDIOBJ(font.0));
            DrawTextW(self.hdc, &mut w, &mut rc, flags);
        }
        (rc.right - rc.left, rc.bottom - rc.top)
    }

    pub fn clip(&self, r: R) {
        unsafe {
            SaveDC(self.hdc);
            IntersectClipRect(self.hdc, r.x, r.y, r.right(), r.bottom());
        }
    }

    pub fn unclip(&self) {
        unsafe {
            let _ = RestoreDC(self.hdc, -1);
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            let _ = DeleteDC(self.hdc);
        }
    }
}
