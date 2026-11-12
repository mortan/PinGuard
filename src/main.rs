//! GpuMon – kompakter 12V-2x6-/PSU-Monitor für das MSI MPG Ai1300TS.
//!
//! Win32-Fenster ohne Rahmen, GDI/GDI+-Zeichnen nur bei neuen Daten.
//! Aufruf: `gpumon.exe [--mock] [--topmost]`
//!   --mock     Fake-Daten statt Afterburner
//!   --topmost  immer im Vordergrund (wird auch über das Kontextmenü gemerkt)

#![windows_subsystem = "windows"]

mod model;
mod render;
mod settings;
mod source;

use model::Monitor;
use render::{Renderer, View};
use settings::Settings;
use source::{mahm::MahmSource, mock::MockSource, PsuSource, Reading};

use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const TIMER_ID: usize = 1;
const CMD_RESET_PEAKS: usize = 1;
const CMD_RESET_FAULTS: usize = 2;
const CMD_TOPMOST: usize = 3;
const CMD_EXIT: usize = 4;
const CLASS: PCWSTR = w!("GpuMonWindow");

struct App {
    source: Box<dyn PsuSource>,
    reading: Reading,
    monitor: Monitor,
    renderer: Renderer,
    settings: Settings,
    minimized: bool,
}

impl App {
    fn tick(&mut self, hwnd: HWND) {
        let r = self.source.poll();
        if r != self.reading {
            self.reading = r;
            self.monitor.update(match &self.reading {
                Reading::Data(s) => Some(s),
                _ => None,
            });
            self.invalidate(hwnd);
        }
    }

    fn invalidate(&self, hwnd: HWND) {
        if !self.minimized {
            unsafe {
                let _ = InvalidateRect(hwnd, None, false);
            }
        }
    }

    fn save_rect(&mut self, hwnd: HWND) {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                return;
            }
            let mut r = RECT::default();
            if GetWindowRect(hwnd, &mut r).is_ok() {
                self.settings.rect = Some((r.left, r.top, r.right - r.left, r.bottom - r.top));
            }
        }
    }
}

fn client_size(hwnd: HWND) -> (u32, u32) {
    let mut r = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut r);
    }
    ((r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32)
}

fn lparam_point(lp: LPARAM) -> POINT {
    POINT { x: (lp.0 & 0xFFFF) as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32 }
}

unsafe fn app<'a>(hwnd: HWND) -> Option<&'a mut App> {
    let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    p.as_mut()
}

fn in_status(app: &App, x: i32, y: i32) -> bool {
    let (bx, by) = app.renderer.to_base(x, y);
    (render::STATUS_X0..=render::STATUS_X1).contains(&bx) && (render::STATUS_Y0..=render::STATUS_Y1).contains(&by)
}

unsafe fn show_menu(hwnd: HWND, app: &mut App, mut pt: POINT) {
    if pt.x == -1 && pt.y == -1 {
        let _ = GetCursorPos(&mut pt);
    }
    let Ok(menu) = CreatePopupMenu() else { return };
    let _ = AppendMenuW(menu, MF_STRING, CMD_RESET_PEAKS, w!("Peaks zurücksetzen\tDoppelklick"));
    let _ = AppendMenuW(menu, MF_STRING, CMD_RESET_FAULTS, w!("Fehler zurücksetzen\tKlick auf Status"));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
    let top = if app.settings.topmost { MF_CHECKED } else { MF_UNCHECKED };
    let _ = AppendMenuW(menu, MF_STRING | top, CMD_TOPMOST, w!("Immer im Vordergrund"));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
    let _ = AppendMenuW(menu, MF_STRING, CMD_EXIT, w!("Beenden"));
    let _ = SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
    let _ = DestroyMenu(menu);
    match cmd.0 as usize {
        CMD_RESET_PEAKS => {
            app.monitor.clear_peaks();
            app.invalidate(hwnd);
        }
        CMD_RESET_FAULTS => {
            app.monitor.clear_faults();
            app.invalidate(hwnd);
        }
        CMD_TOPMOST => {
            app.settings.topmost = !app.settings.topmost;
            let after = if app.settings.topmost { HWND_TOPMOST } else { HWND_NOTOPMOST };
            let _ = SetWindowPos(hwnd, after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            app.settings.save();
        }
        CMD_EXIT => {
            let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        _ => {}
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let Some(app) = app(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
        match msg {
            WM_TIMER if wp.0 == TIMER_ID => {
                app.tick(hwnd);
                LRESULT(0)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let (w, h) = client_size(hwnd);
                let view = View { reading: &app.reading, monitor: &app.monitor, source_name: app.source.name() };
                app.renderer.render(dc, w, h, &view);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_NCCALCSIZE if wp.0 != 0 => LRESULT(0), // ganze Fensterfläche ist Client
            WM_NCHITTEST => {
                let mut pt = lparam_point(lp);
                let _ = ScreenToClient(hwnd, &mut pt);
                let (w, h) = client_size(hwnd);
                let b = (6 * GetDpiForWindow(hwnd) / 96) as i32;
                let (l, r, t, bo) = (pt.x < b, pt.x >= w as i32 - b, pt.y < b, pt.y >= h as i32 - b);
                let ht = match (l, r, t, bo) {
                    (true, _, true, _) => HTTOPLEFT,
                    (_, true, true, _) => HTTOPRIGHT,
                    (true, _, _, true) => HTBOTTOMLEFT,
                    (_, true, _, true) => HTBOTTOMRIGHT,
                    (true, _, _, _) => HTLEFT,
                    (_, true, _, _) => HTRIGHT,
                    (_, _, true, _) => HTTOP,
                    (_, _, _, true) => HTBOTTOM,
                    _ if in_status(app, pt.x, pt.y) => HTCLIENT,
                    _ => HTCAPTION, // überall sonst: Fenster ziehen
                };
                LRESULT(ht as isize)
            }
            WM_LBUTTONUP => {
                let pt = lparam_point(lp);
                if in_status(app, pt.x, pt.y) {
                    app.monitor.clear_faults();
                    app.invalidate(hwnd);
                }
                LRESULT(0)
            }
            WM_NCLBUTTONDBLCLK if wp.0 == HTCAPTION as usize => {
                app.monitor.clear_peaks();
                app.invalidate(hwnd);
                LRESULT(0)
            }
            WM_NCRBUTTONUP if wp.0 == HTCAPTION as usize => {
                show_menu(hwnd, app, lparam_point(lp));
                LRESULT(0)
            }
            WM_CONTEXTMENU => {
                show_menu(hwnd, app, lparam_point(lp));
                LRESULT(0)
            }
            WM_SIZE => {
                let minimized = wp.0 == SIZE_MINIMIZED as usize;
                if minimized {
                    // Backbuffer freigeben, solange nichts zu sehen ist.
                    app.renderer.discard();
                } else {
                    let _ = InvalidateRect(hwnd, None, false);
                }
                app.minimized = minimized;
                LRESULT(0)
            }
            WM_DPICHANGED => {
                app.renderer.set_dpi((wp.0 & 0xFFFF) as u32);
                let r = &*(lp.0 as *const RECT);
                let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                LRESULT(0)
            }
            WM_SIZING => {
                // Seitenverhältnis des Layouts beibehalten (keine leeren Ränder).
                let r = &mut *(lp.0 as *mut RECT);
                let ratio = render::BASE_W / render::BASE_H;
                let edge = wp.0 as u32;
                if edge == WMSZ_TOP || edge == WMSZ_BOTTOM {
                    r.right = r.left + ((r.bottom - r.top) as f32 * ratio).round() as i32;
                } else {
                    let h = ((r.right - r.left) as f32 / ratio).round() as i32;
                    if edge == WMSZ_TOPLEFT || edge == WMSZ_TOPRIGHT {
                        r.top = r.bottom - h;
                    } else {
                        r.bottom = r.top + h;
                    }
                }
                LRESULT(1)
            }
            WM_GETMINMAXINFO => {
                let mmi = &mut *(lp.0 as *mut MINMAXINFO);
                let dpi = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
                mmi.ptMinTrackSize = POINT {
                    x: (render::BASE_W * 0.5 * dpi) as i32,
                    y: (render::BASE_H * 0.5 * dpi) as i32,
                };
                LRESULT(0)
            }
            WM_EXITSIZEMOVE => {
                app.save_rect(hwnd);
                app.settings.save();
                LRESULT(0)
            }
            WM_CLOSE => {
                app.save_rect(hwnd);
                app.settings.save();
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                let _ = KillTimer(hwnd, TIMER_ID);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(app as *mut App));
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// Liegt das Rechteck (zumindest teilweise) auf einem vorhandenen Monitor?
fn on_screen(x: i32, y: i32, w: i32, h: i32) -> bool {
    let r = RECT { left: x, top: y, right: x + w, bottom: y + h };
    unsafe { !MonitorFromRect(&r, MONITOR_DEFAULTTONULL).is_invalid() }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |a: &str| args.iter().any(|x| x == a);

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // Nur eine Instanz: vorhandenes Fenster nach vorne holen.
        let _mutex = CreateMutexW(None, true, w!("Local\\GpuMon_SingleInstance"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(h) = FindWindowW(CLASS, None) {
                let _ = ShowWindow(h, SW_RESTORE);
                let _ = SetForegroundWindow(h);
            }
            return Ok(());
        }

        let mut settings = Settings::load();
        if has("--topmost") {
            settings.topmost = true;
        }
        let source: Box<dyn PsuSource> = if has("--mock") { Box::new(MockSource::new()) } else {
            let name = args.iter().position(|a| a == "--mahm-name").and_then(|i| args.get(i + 1));
            Box::new(MahmSource::new(name.map_or(MahmSource::DEFAULT_NAME, |s| s.as_str())))
        };
        let period = source.period_ms();
        let Some(renderer) = Renderer::new() else { return Ok(()) };

        let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hIcon: LoadIconW(hinst, PCWSTR(1 as _)).unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let dpi = GetDpiForSystem().max(96) as f32 / 96.0;
        let (dw, dh) = ((render::BASE_W * 1.25 * dpi) as i32, (render::BASE_H * 1.25 * dpi) as i32);
        let (x, y, w, h) = match settings.rect {
            Some((x, y, w, _)) if on_screen(x, y, w, 10) => (x, y, w, (w as f32 * render::BASE_H / render::BASE_W).round() as i32),
            _ => (CW_USEDEFAULT, CW_USEDEFAULT, dw, dh),
        };
        let ex = if settings.topmost { WS_EX_TOPMOST } else { WINDOW_EX_STYLE(0) };
        let hwnd = CreateWindowExW(
            ex,
            CLASS,
            w!("GpuMon"),
            WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX | WS_SYSMENU,
            x,
            y,
            w,
            h,
            None,
            None,
            hinst,
            None,
        )?;

        let mut app = Box::new(App {
            source,
            reading: Reading::Offline,
            monitor: Monitor::default(),
            renderer,
            settings,
            minimized: false,
        });
        app.renderer.set_dpi(GetDpiForWindow(hwnd));
        app.monitor.update(None);
        app.tick(hwnd);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(app) as isize);

        // Abgerundete Ecken, dunkler Rahmen (Windows 11).
        let round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const _ as _, 4);
        let dark: BOOL = TRUE;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
        let border: u32 = 0x00302A26; // COLORREF (BGR)
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border as *const _ as _, 4);
        // Rahmen neu berechnen lassen (WM_NCCALCSIZE greift jetzt)
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER);

        SetTimer(hwnd, TIMER_ID, period, None);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
