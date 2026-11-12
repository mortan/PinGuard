//! Eigener Tooltip: kleines Popup-Fenster im Stil der App, mit GDI gezeichnet.
//!
//! Bewusst nicht das Tooltip-Control aus comctl32: ohne Manifest sähe es alt
//! und hell aus, und weil fast die ganze Fensterfläche als Titelleiste gilt
//! (Ziehen), müsste die Maus ohnehin von Hand verfolgt werden.
//!
//! Das Fenster nimmt weder Fokus noch Mausklicks an. Der Text liegt als
//! Fenstertext vor, die Schrift in `GWLP_USERDATA`.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS: PCWSTR = w!("GpuMonTip");
// COLORREF (BGR)
const BG: u32 = 0x00332B27;
const FG: u32 = 0x00F0F0F0;
const BORDER: u32 = 0x005C504A;
// Innenabstand und Versatz zum Mauszeiger in DIPs
const PAD: (i32, i32) = (9, 6);
const OFFSET: (i32, i32) = (12, 20);
const FONT_DIP: i32 = 12;

pub struct Tip {
    hwnd: HWND,
    font: HFONT,
    font_dpi: u32,
    text: String,
    anchor: POINT,
    pub visible: bool,
}

impl Tip {
    /// Legt das (unsichtbare) Tooltip-Fenster an; es gehört `owner` und wird
    /// mit ihm zerstört.
    pub fn new(hinst: HINSTANCE, owner: HWND) -> Tip {
        let hwnd = unsafe {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(tip_proc),
                hInstance: hinst,
                lpszClassName: CLASS,
                ..Default::default()
            };
            RegisterClassExW(&wc);
            // Layered + Transparent: Mausereignisse gehen an das Fenster darunter.
            let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT;
            let hwnd = CreateWindowExW(ex, CLASS, w!(""), WS_POPUP, 0, 0, 0, 0, owner, None, hinst, None).unwrap_or_default();
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
            let round = DWMWCP_ROUNDSMALL;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const _ as _, 4);
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &BORDER as *const _ as _, 4);
            hwnd
        };
        Tip { hwnd, font: HFONT::default(), font_dpi: 0, text: String::new(), anchor: POINT::default(), visible: false }
    }

    /// Zeigt den Tooltip neben `anchor` (Bildschirmkoordinaten des Mauszeigers).
    pub fn show(&mut self, text: &str, anchor: POINT, dpi: u32) {
        if self.hwnd.0.is_null() {
            return;
        }
        if self.visible && self.text == text && self.anchor == anchor {
            return;
        }
        let k = |v: i32| v * dpi as i32 / 96;
        unsafe {
            if self.font_dpi != dpi || self.font.is_invalid() {
                if !self.font.is_invalid() {
                    let _ = DeleteObject(self.font);
                }
                // DEFAULT_CHARSET = 1, CLEARTYPE_QUALITY = 5
                self.font = CreateFontW(-k(FONT_DIP), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI"));
                self.font_dpi = dpi;
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, self.font.0 as isize);
            }
            let mut wtext: Vec<u16> = text.encode_utf16().collect();
            let mut rc = RECT::default();
            let dc = GetDC(self.hwnd);
            let old = SelectObject(dc, self.font);
            DrawTextW(dc, &mut wtext, &mut rc, DT_CALCRECT | DT_NOPREFIX);
            SelectObject(dc, old);
            ReleaseDC(self.hwnd, dc);
            let (w, h) = (rc.right + 2 * k(PAD.0), rc.bottom + 2 * k(PAD.1));

            // Rechts unter dem Zeiger; am Bildschirmrand nach innen bzw. über den Zeiger.
            let (mut x, mut y) = (anchor.x + k(OFFSET.0), anchor.y + k(OFFSET.1));
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if GetMonitorInfoW(MonitorFromPoint(anchor, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool() {
                let wa = mi.rcWork;
                if x + w > wa.right {
                    x = wa.right - w;
                }
                if y + h > wa.bottom {
                    y = anchor.y - h - k(8);
                }
                x = x.max(wa.left);
                y = y.max(wa.top);
            }
            wtext.push(0);
            let _ = SetWindowTextW(self.hwnd, PCWSTR(wtext.as_ptr()));
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            let _ = InvalidateRect(self.hwnd, None, false);
        }
        self.text = text.to_string();
        self.anchor = anchor;
        self.visible = true;
    }

    /// Neuer Text an derselben Stelle (z. B. nach neuen Messwerten).
    pub fn update(&mut self, text: &str, dpi: u32) {
        if self.visible {
            self.show(text, self.anchor, dpi);
        }
    }

    pub fn hide(&mut self) {
        if self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }
}

impl Drop for Tip {
    fn drop(&mut self) {
        if !self.font.is_invalid() {
            unsafe {
                let _ = DeleteObject(self.font);
            }
        }
    }
}

extern "system" fn tip_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let brush = CreateSolidBrush(COLORREF(BG));
                FillRect(dc, &rc, brush);
                let _ = DeleteObject(brush);

                let mut buf = [0u16; 512];
                let n = GetWindowTextW(hwnd, &mut buf) as usize;
                let old = SelectObject(dc, HFONT(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as _));
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, COLORREF(FG));
                // Textblock mittig ins Fenster (die Größe wurde aus ihm berechnet).
                let mut tr = RECT::default();
                DrawTextW(dc, &mut buf[..n], &mut tr, DT_CALCRECT | DT_NOPREFIX);
                let (dx, dy) = ((rc.right - tr.right) / 2, (rc.bottom - tr.bottom) / 2);
                let mut tr = RECT { left: dx, top: dy, right: dx + tr.right, bottom: dy + tr.bottom };
                DrawTextW(dc, &mut buf[..n], &mut tr, DT_NOPREFIX | DT_NOCLIP);
                SelectObject(dc, old);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
