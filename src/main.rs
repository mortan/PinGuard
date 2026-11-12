//! GpuMon – kompakter 12V-2x6-/PSU-Monitor für das MSI MPG Ai1300TS.
//!
//! Win32-Fenster ohne Rahmen, GDI/GDI+-Zeichnen nur bei neuen Daten.
//! Aufruf: `gpumon.exe [--mock] [--topmost]`
//!   --mock     Fake-Daten statt Afterburner (--mock2: beide Stecker belegt)
//!   --topmost  immer im Vordergrund (wird auch über das Kontextmenü gemerkt)

#![windows_subsystem = "windows"]

mod autostart;
mod events;
mod i18n;
mod model;
mod render;
mod settings;
mod settings_dlg;
mod source;
mod tip;
mod tray;

use i18n::{Lang, LangPref};
use model::Monitor;
use render::{Renderer, View};
use settings::Settings;
use source::{mahm::MahmSource, mock::MockSource, PsuSource, Reading};

use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Globalization::GetUserDefaultUILanguage;
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

const TIMER_ID: usize = 1;
/// Verzögerung, bis ein Tooltip erscheint.
const TIMER_TIP: usize = 2;
const TIP_DELAY_MS: u32 = 450;
// Werte aus WinUser.h
const WM_NCMOUSELEAVE: u32 = 0x02A2;
const WM_MOUSELEAVE: u32 = 0x02A3;
const CMD_RESET_PEAKS: usize = 1;
const CMD_RESET_FAULTS: usize = 2;
const CMD_TOPMOST: usize = 3;
const CMD_EXIT: usize = 4;
const CMD_SETTINGS: usize = 5;
/// Sprache: Auto, Deutsch, English (zusammenhängend, für `CheckMenuRadioItem`)
const CMD_LANG0: usize = 6;
const LANG_PREFS: [LangPref; 3] = [LangPref::Auto, LangPref::De, LangPref::En];
const CMD_LOG_RECORD: usize = 10;
const CMD_LOG_OPEN: usize = 11;
const CMD_TRAY: usize = 12;
const CMD_AUTOSTART: usize = 13;
const CLASS: PCWSTR = w!("GpuMonWindow");

struct App {
    source: Box<dyn PsuSource>,
    reading: Reading,
    monitor: Monitor,
    renderer: Renderer,
    settings: Settings,
    minimized: bool,
    tip: tip::Tip,
    /// Bereich (`Hint::id`) unter der Maus und die Stelle in Basiskoordinaten.
    tip_region: Option<u32>,
    tip_base: (f32, f32),
    tray: tray::Tray,
    /// Registrierte Nachricht `TaskbarCreated` (Explorer neu gestartet)
    taskbar_msg: u32,
}

impl App {
    fn tick(&mut self, hwnd: HWND) {
        let r = self.source.poll();
        let changed = r != self.reading;
        if changed {
            self.reading = r;
            let raised = self.update_monitor();
            self.log_events(raised);
            self.signal(hwnd, raised);
        }
        // Verlauf je Sekunde aufnehmen, auch wenn sich nichts ändert (die
        // Kurve wandert weiter); neu zeichnen nur, wenn sie zu sehen ist.
        let snap = match &self.reading {
            Reading::Data(s) => Some(s),
            _ => None,
        };
        let moved = self.monitor.record_power(now_ms() / 1000, snap) && self.settings.config.shows(model::Metric::History);
        if changed || moved {
            self.invalidate(hwnd);
        }
        // Sichtbaren Tooltip auch ohne neue Daten nachführen (Dauer eines
        // aktiven Alarms läuft weiter); ohne Tooltip kostet das nichts.
        self.tip_refresh(hwnd);
    }

    fn hint_at(&self, bx: f32, by: f32) -> Option<render::Hint> {
        if let Some(b) = self.button_at(bx, by) {
            let tx = i18n::t();
            let text = match b {
                render::WinButton::Minimize if self.settings.tray => tx.btn_minimize_tray,
                render::WinButton::Minimize => tx.btn_minimize,
                render::WinButton::Close => tx.btn_close,
            };
            return Some(render::Hint { id: 400 + b as u32, text: text.to_string() });
        }
        let view = View { reading: &self.reading, monitor: &self.monitor, config: &self.settings.config, source_name: self.source.name(), now_ms: now_ms(), mean_legend: self.renderer.mean_legend };
        render::hint_at(bx, by, &view)
    }

    /// Fensterknopf an dieser Stelle, falls die Knöpfe gerade zu sehen sind.
    fn button_at(&self, bx: f32, by: f32) -> Option<render::WinButton> {
        if self.renderer.chrome.shown {
            render::button_at(bx, by)
        } else {
            None
        }
    }

    /// Fensterknöpfe ein-/ausblenden und den Knopf unter der Maus
    /// hervorheben; zeichnet nur bei einer Änderung neu.
    fn update_chrome(&mut self, hwnd: HWND) {
        let chrome = match self.cursor(hwnd) {
            Some((_, (bx, by))) => render::Chrome { shown: true, hot: render::button_at(bx, by) },
            None => render::Chrome::default(),
        };
        if chrome != self.renderer.chrome {
            self.renderer.chrome = chrome;
            if !self.minimized {
                unsafe {
                    let _ = InvalidateRect(hwnd, None, false);
                }
            }
        }
    }

    /// Mauszeiger (Bildschirm- und Basiskoordinaten), falls er über dem
    /// sichtbaren Teil des Fensters steht.
    fn cursor(&self, hwnd: HWND) -> Option<(POINT, (f32, f32))> {
        unsafe {
            let mut pt = POINT::default();
            GetCursorPos(&mut pt).ok()?;
            if WindowFromPoint(pt) != hwnd {
                return None;
            }
            let mut c = pt;
            let _ = ScreenToClient(hwnd, &mut c);
            Some((pt, self.renderer.to_base(c.x, c.y)))
        }
    }

    /// Mausbewegung: Tooltip nach kurzer Verzögerung; ist schon einer zu
    /// sehen, wechselt er sofort zum neuen Bereich.
    fn tip_move(&mut self, hwnd: HWND, nonclient: bool) {
        unsafe {
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: if nonclient { TME_LEAVE | TME_NONCLIENT } else { TME_LEAVE },
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);
            let Some((screen, (bx, by))) = self.cursor(hwnd) else { return self.tip_leave(hwnd) };
            let hint = self.hint_at(bx, by);
            let id = hint.as_ref().map(|h| h.id);
            if id == self.tip_region {
                return;
            }
            self.tip_region = id;
            self.tip_base = (bx, by);
            match hint {
                Some(h) if self.tip.visible => self.tip.show(&h.text, screen, GetDpiForWindow(hwnd)),
                Some(_) => {
                    SetTimer(hwnd, TIMER_TIP, TIP_DELAY_MS, None);
                }
                None => self.tip_cancel(hwnd),
            }
        }
    }

    fn tip_timer(&mut self, hwnd: HWND) {
        unsafe {
            let _ = KillTimer(hwnd, TIMER_TIP);
            let Some((screen, (bx, by))) = self.cursor(hwnd) else { return };
            if let Some(h) = self.hint_at(bx, by).filter(|h| Some(h.id) == self.tip_region) {
                self.tip_base = (bx, by);
                self.tip.show(&h.text, screen, GetDpiForWindow(hwnd));
            }
        }
    }

    /// Text eines sichtbaren Tooltips an neue Daten anpassen.
    fn tip_refresh(&mut self, hwnd: HWND) {
        if !self.tip.visible {
            return;
        }
        match self.hint_at(self.tip_base.0, self.tip_base.1) {
            Some(h) if Some(h.id) == self.tip_region => self.tip.update(&h.text, unsafe { GetDpiForWindow(hwnd) }),
            _ => self.tip_cancel(hwnd),
        }
    }

    /// Tooltip ausblenden; der Bereich bleibt gemerkt, damit er erst nach
    /// einem Bereichswechsel wieder erscheint (z. B. nach einem Klick).
    fn tip_cancel(&mut self, hwnd: HWND) {
        unsafe {
            let _ = KillTimer(hwnd, TIMER_TIP);
        }
        self.tip.hide();
    }

    /// Maus hat das Fenster verlassen.
    fn tip_leave(&mut self, hwnd: HWND) {
        self.tip_region = None;
        self.tip_cancel(hwnd);
    }

    /// Wertet den aktuellen Stand mit der aktuellen Konfiguration aus.
    fn update_monitor(&mut self) -> [bool; model::FAULT_COUNT] {
        let snap = match &self.reading {
            Reading::Data(s) => Some(s),
            _ => None,
        };
        self.monitor.update_at(snap, &self.settings.config, now())
    }

    /// Neu aufgetretene Alarme ins Ereignis-Log (falls eingeschaltet).
    fn log_events(&self, raised: [bool; model::FAULT_COUNT]) {
        if !self.settings.event_log || !raised.iter().any(|r| *r) {
            return;
        }
        let snap = match &self.reading {
            Reading::Data(s) => Some(s),
            _ => None,
        };
        let at = now().local;
        let lines: Vec<String> = model::FAULTS.iter().filter(|f| raised[**f as usize]).map(|f| events::line(&at, *f, snap)).collect();
        events::log(&lines);
    }

    /// Ton bzw. Fenster nach vorne für neu aufgetretene Alarme.
    fn signal(&mut self, hwnd: HWND, raised: [bool; model::FAULT_COUNT]) {
        let cfg = &self.settings.config;
        let any = |pick: fn(&model::AlarmCfg) -> bool| raised.iter().zip(&cfg.alarms).any(|(r, a)| *r && pick(a));
        unsafe {
            if any(|a| a.sound) {
                let _ = MessageBeep(MB_ICONWARNING);
            }
            if any(|a| a.show) {
                // Auch aus dem Infobereich: dort ist das Fenster minimiert
                // und zusätzlich ausgeblendet.
                if IsIconic(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
                // Nach vorne, ohne dem Spiel den Fokus zu nehmen.
                if !self.settings.topmost {
                    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
                    let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags);
                    let _ = SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
                }
                let fi = FLASHWINFO {
                    cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                    hwnd,
                    dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
                    uCount: 0,
                    dwTimeout: 0,
                };
                let _ = FlashWindowEx(&fi);
            }
        }
    }

    /// Tray-Symbol an Zustand und Option anpassen (setzt nur bei Änderung).
    fn sync_tray(&mut self) {
        if !self.settings.tray {
            self.tray.remove();
            return;
        }
        let snap = match &self.reading {
            Reading::Data(s) => Some(s),
            _ => None,
        };
        let status = self.monitor.status(snap, &self.settings.config);
        let view = View { reading: &self.reading, monitor: &self.monitor, config: &self.settings.config, source_name: self.source.name(), now_ms: now_ms(), mean_legend: self.renderer.mean_legend };
        let tip = render::tray_tip(&view);
        self.tray.set(status, &tip);
    }

    /// Fenster aus Taskleiste/Infobereich wiederherstellen und nach vorne holen.
    fn restore(&mut self, hwnd: HWND) {
        unsafe {
            if IsIconic(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }

    /// Nach einer Zustandsänderung: neu zeichnen (falls sichtbar) und das
    /// Tray-Symbol nachführen.
    fn invalidate(&mut self, hwnd: HWND) {
        self.sync_tray();
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

/// Monotone Zeit in Millisekunden (Zeitquelle für das Modell).
fn now_ms() -> u64 {
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }
}

/// Zeitpunkt für das Modell: monotone Zeit und lokale Uhrzeit.
fn now() -> model::Now {
    let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let local = model::LocalTime {
        year: st.wYear,
        month: st.wMonth as u8,
        day: st.wDay as u8,
        hour: st.wHour as u8,
        minute: st.wMinute as u8,
        second: st.wSecond as u8,
    };
    model::Now { ms: now_ms(), local }
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
    if app.button_at(bx, by).is_some() {
        return true;
    }
    (render::STATUS_X0..=render::STATUS_X1).contains(&bx) && (render::STATUS_Y0..=render::STATUS_Y1).contains(&by)
        || render::link_hit(bx, by, &app.settings.config, &app.monitor)
}

/// Sprache der Oberfläche nach Windows-Anzeigesprache.
fn system_lang() -> Lang {
    Lang::from_langid(unsafe { GetUserDefaultUILanguage() })
}

/// Stellt die Sprache der Oberfläche nach der Nutzerwahl ein.
fn apply_lang(pref: LangPref) {
    i18n::set_lang(pref.resolve(system_lang()));
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn append(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, text: &str) {
    let t = wide(text);
    let _ = AppendMenuW(menu, flags, id, PCWSTR(t.as_ptr()));
}

unsafe fn show_menu(hwnd: HWND, app: &mut App, mut pt: POINT) {
    app.tip_cancel(hwnd);
    if pt.x == -1 && pt.y == -1 {
        let _ = GetCursorPos(&mut pt);
    }
    let Ok(menu) = CreatePopupMenu() else { return };
    let tx = i18n::t();
    append(menu, MF_STRING, CMD_RESET_PEAKS, tx.menu_reset_peaks);
    append(menu, MF_STRING, CMD_RESET_FAULTS, tx.menu_reset_faults);
    append(menu, MF_STRING, CMD_SETTINGS, tx.menu_settings);
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
    let top = if app.settings.topmost { MF_CHECKED } else { MF_UNCHECKED };
    append(menu, MF_STRING | top, CMD_TOPMOST, tx.menu_topmost);
    let tray_on = if app.settings.tray { MF_CHECKED } else { MF_UNCHECKED };
    append(menu, MF_STRING | tray_on, CMD_TRAY, tx.menu_tray);
    // Zustand aus der Registry, nicht aus der INI
    let auto_on = if autostart::is_enabled() { MF_CHECKED } else { MF_UNCHECKED };
    append(menu, MF_STRING | auto_on, CMD_AUTOSTART, tx.menu_autostart);
    // Untermenü Sprache; wird mit dem Hauptmenü zerstört.
    if let Ok(sub) = CreatePopupMenu() {
        for (i, name) in [tx.menu_lang_auto, i18n::NAME_DE, i18n::NAME_EN].iter().enumerate() {
            append(sub, MF_STRING, CMD_LANG0 + i, name);
        }
        let cur = LANG_PREFS.iter().position(|p| *p == app.settings.lang).unwrap_or(0);
        let last = CMD_LANG0 + LANG_PREFS.len() - 1;
        let _ = CheckMenuRadioItem(sub, CMD_LANG0 as u32, last as u32, (CMD_LANG0 + cur) as u32, MF_BYCOMMAND.0);
        append(menu, MF_POPUP, sub.0 as usize, tx.menu_language);
    }
    // Untermenü Ereignis-Log
    if let Ok(sub) = CreatePopupMenu() {
        let rec = if app.settings.event_log { MF_CHECKED } else { MF_UNCHECKED };
        append(sub, MF_STRING | rec, CMD_LOG_RECORD, tx.menu_log_record);
        append(sub, MF_STRING, CMD_LOG_OPEN, tx.menu_log_open);
        append(menu, MF_POPUP, sub.0 as usize, tx.menu_event_log);
    }
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
    append(menu, MF_STRING, CMD_EXIT, tx.menu_exit);
    let _ = SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
    let _ = DestroyMenu(menu);
    // Sonst schließt sich ein Menü aus dem Infobereich beim nächsten Mal
    // nicht zuverlässig (bekanntes Verhalten von TrackPopupMenu).
    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
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
        CMD_SETTINGS => {
            if let Some(c) = settings_dlg::edit(hwnd, app.settings.config) {
                app.settings.config = c;
                app.settings.save();
                // Neu auswerten (z. B. abgeschaltete Alarme verschwinden sofort),
                // aber keinen Ton für bereits bestehende Zustände.
                app.update_monitor();
                app.invalidate(hwnd);
            }
        }
        CMD_EXIT => {
            let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        CMD_TRAY => {
            app.settings.tray = !app.settings.tray;
            app.settings.save();
            if !app.settings.tray {
                // Ohne Symbol darf das Fenster nicht unsichtbar bleiben.
                app.restore(hwnd);
            }
            app.sync_tray();
        }
        CMD_AUTOSTART => {
            let _ = autostart::set(!autostart::is_enabled());
        }
        CMD_LOG_RECORD => {
            app.settings.event_log = !app.settings.event_log;
            app.settings.save();
        }
        CMD_LOG_OPEN => {
            // Ordner im Explorer; gibt es die Datei schon, ist sie markiert.
            if let Some(dir) = events::dir() {
                let _ = std::fs::create_dir_all(&dir);
                let file = dir.join(events::FILE_NAME);
                let (exe, args) = if file.exists() {
                    ("explorer.exe".to_string(), format!("/select,\"{}\"", file.display()))
                } else {
                    (dir.display().to_string(), String::new())
                };
                let (exe, args) = (wide(&exe), wide(&args));
                let _ = windows::Win32::UI::Shell::ShellExecuteW(hwnd, w!("open"), PCWSTR(exe.as_ptr()), PCWSTR(args.as_ptr()), None, SW_SHOWNORMAL);
            }
        }
        c if (CMD_LANG0..CMD_LANG0 + LANG_PREFS.len()).contains(&c) => {
            // Wirkt sofort: Fenster, Menü, Tooltips und Dialog lesen die
            // Texte bei jeder Verwendung neu.
            app.settings.lang = LANG_PREFS[c - CMD_LANG0];
            apply_lang(app.settings.lang);
            app.settings.save();
            app.invalidate(hwnd);
        }
        _ => {}
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let Some(app) = app(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
        if msg != 0 && msg == app.taskbar_msg {
            // Explorer neu gestartet: Symbol neu eintragen
            app.tray.recreate();
            app.sync_tray();
            return LRESULT(0);
        }
        match msg {
            tray::WM_TRAY => {
                match (lp.0 & 0xFFFF) as u32 {
                    WM_LBUTTONUP => app.restore(hwnd),
                    WM_RBUTTONUP => show_menu(hwnd, app, POINT { x: -1, y: -1 }),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_TIMER if wp.0 == TIMER_ID => {
                app.tick(hwnd);
                LRESULT(0)
            }
            WM_TIMER if wp.0 == TIMER_TIP => {
                app.tip_timer(hwnd);
                LRESULT(0)
            }
            WM_NCMOUSEMOVE | WM_MOUSEMOVE => {
                app.update_chrome(hwnd);
                app.tip_move(hwnd, msg == WM_NCMOUSEMOVE);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_NCMOUSELEAVE | WM_MOUSELEAVE => {
                // Beim Wechsel zwischen Statuszeile (Client) und Rest
                // (Titelleiste) steht die Maus weiter über dem Fenster.
                if app.cursor(hwnd).is_none() {
                    app.tip_leave(hwnd);
                }
                app.update_chrome(hwnd);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_NCLBUTTONDOWN | WM_NCRBUTTONDOWN | WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                app.tip_cancel(hwnd);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let (w, h) = client_size(hwnd);
                let view = View { reading: &app.reading, monitor: &app.monitor, config: &app.settings.config, source_name: app.source.name(), now_ms: now_ms(), mean_legend: app.renderer.mean_legend };
                app.renderer.render(dc, w, h, &view);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            // Ohne Titelleiste zeichnet Windows beim Aktivieren sonst den
            // klassischen Größenrahmen über die Fensterfläche (die ganze
            // Fläche ist Client). lParam −1 unterdrückt dieses Zeichnen.
            WM_NCACTIVATE => DefWindowProcW(hwnd, msg, wp, LPARAM(-1)),
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
                let (bx, by) = app.renderer.to_base(pt.x, pt.y);
                match app.button_at(bx, by) {
                    Some(render::WinButton::Minimize) => {
                        // Knöpfe ausblenden: beim Minimieren kommt kein
                        // Verlassen-Ereignis mehr. Mit Tray-Option blendet
                        // WM_SIZE das Fenster zusätzlich aus.
                        app.renderer.chrome = render::Chrome::default();
                        app.tip_leave(hwnd);
                        let _ = ShowWindow(hwnd, SW_MINIMIZE);
                    }
                    Some(render::WinButton::Close) => {
                        let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                    None if in_status(app, pt.x, pt.y) => {
                        app.monitor.clear_faults();
                        app.invalidate(hwnd);
                    }
                    None => {}
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
                    app.tip_leave(hwnd);
                    // Mit Tray-Symbol: aus der Taskleiste ausblenden. Nur wenn
                    // das Symbol wirklich eingetragen ist, sonst wäre das
                    // Fenster nicht mehr erreichbar.
                    if app.settings.tray {
                        app.sync_tray();
                        if app.tray.is_added() {
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        }
                    }
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
                app.tray.remove();
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
        apply_lang(settings.lang);
        if has("--topmost") {
            settings.topmost = true;
        }
        let source: Box<dyn PsuSource> = if has("--mock") || has("--mock2") { Box::new(MockSource::new(has("--mock2"))) } else {
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
            tip: tip::Tip::new(hinst, hwnd),
            tray: tray::Tray::new(hwnd),
            taskbar_msg: RegisterWindowMessageW(w!("TaskbarCreated")),
            tip_region: None,
            tip_base: (0.0, 0.0),
        });
        app.renderer.set_dpi(GetDpiForWindow(hwnd));
        app.update_monitor();
        app.tick(hwnd);
        app.sync_tray();
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
