//! Einstellungsdialog für Grenzwerte und Alarme. Modaler Win32-Dialog aus
//! einer Speichervorlage (`DialogBoxIndirectParamW`), die Steuerelemente
//! werden in `WM_INITDIALOG` in Dialogeinheiten angelegt. Dadurch gibt es
//! Tab-Navigation, Enter/Esc und DPI-Skalierung ohne Ressourcendatei.

use crate::model::{AlarmCfg, Config, Limits, FAULTS, FAULT_COUNT};
use crate::settings::parse_num;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::HFONT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;

// Fensterstile der Standard-Steuerelemente (Werte aus WinUser.h).
const BS_PUSHBUTTON: u32 = 0x0;
const BS_DEFPUSHBUTTON: u32 = 0x1;
const BS_AUTOCHECKBOX: u32 = 0x3;
const BS_GROUPBOX: u32 = 0x7;
const ES_AUTOHSCROLL: u32 = 0x80;
const SS_LEFT: u32 = 0x0;
const SS_CENTER: u32 = 0x1;
const DS_SETFONT: u32 = 0x40;
const DS_MODALFRAME: u32 = 0x80;
const DS_FIXEDSYS: u32 = 0x8;
const DS_CENTER: u32 = 0x800;
const EM_SETSEL: u32 = 0xB1;

const ID_LIMIT0: i32 = 100; // 5 Eingabefelder
const ID_ALARM0: i32 = 200; // je Fehlerart 4 Kästchen
const ID_DEFAULTS: i32 = 300;

const DLG_W: i16 = 262;
const DLG_H: i16 = 246;

const LIMIT_ROWS: [(&str, &str); 5] = [
    ("Pin gelb ab", "A"),
    ("Pin rot / Überstrom ab", "A"),
    ("Balken-Vollausschlag", "A"),
    ("Schieflast (max − min) ab", "A"),
    ("Nennleistung je Stecker", "W"),
];
const ALARM_COLS: [&str; 4] = ["Aktiv", "Halten", "Ton", "Zeigen"];
const COL_X: [i32; 4] = [130, 160, 190, 220];

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Zahl für ein Eingabefeld: ohne unnötige Nachkommastellen, mit Komma.
fn fmt_num(v: f32) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.replace('.', ",")
}

fn limit_vals(l: &Limits) -> [f32; 5] {
    [l.pin_warn_a, l.pin_limit_a, l.full_scale_a, l.imbalance_a, l.connector_rated_w]
}

fn alarm_bits(a: &AlarmCfg) -> [bool; 4] {
    [a.enabled, a.latch, a.sound, a.show]
}

/// Speichervorlage: DLGTEMPLATE ohne Elemente, mit Titel und Dialogschrift.
fn template() -> Vec<u32> {
    let style = WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0 | DS_MODALFRAME | DS_SETFONT | DS_FIXEDSYS | DS_CENTER;
    let mut t: Vec<u16> = Vec::new();
    t.extend_from_slice(&[style as u16, (style >> 16) as u16, 0, 0]); // style, exStyle
    t.push(0); // cdit
    for v in [0i16, 0, DLG_W, DLG_H] {
        t.push(v as u16);
    }
    t.push(0); // kein Menü
    t.push(0); // Standard-Dialogklasse
    t.extend(wide("GpuMon – Einstellungen"));
    t.push(9); // Schriftgröße (pt)
    t.extend(wide("MS Shell Dlg")); // mit DS_FIXEDSYS → Segoe UI
    // DWORD-ausgerichtet ablegen
    if t.len() % 2 == 1 {
        t.push(0);
    }
    t.chunks(2).map(|c| c[0] as u32 | (c[1] as u32) << 16).collect()
}

struct Ctx {
    cfg: Config,
    font: HFONT,
}

unsafe fn ctl(dlg: HWND, font: HFONT, class: PCWSTR, text: &str, style: u32, x: i32, y: i32, cx: i32, cy: i32, id: i32) -> HWND {
    let mut r = RECT { left: x, top: y, right: x + cx, bottom: y + cy };
    let _ = MapDialogRect(dlg, &mut r);
    let ex = if class == w!("EDIT") { WS_EX_CLIENTEDGE } else { WINDOW_EX_STYLE(0) };
    let t = wide(text);
    let h = CreateWindowExW(
        ex,
        class,
        PCWSTR(t.as_ptr()),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | style),
        r.left,
        r.top,
        r.right - r.left,
        r.bottom - r.top,
        dlg,
        HMENU(id as isize as _),
        GetModuleHandleW(None).map(|m| HINSTANCE(m.0)).unwrap_or_default(),
        None,
    )
    .unwrap_or_default();
    SendMessageW(h, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
    h
}

unsafe fn set_text(dlg: HWND, id: i32, s: &str) {
    let t = wide(s);
    let _ = SetDlgItemTextW(dlg, id, PCWSTR(t.as_ptr()));
}

unsafe fn get_text(dlg: HWND, id: i32) -> String {
    let mut buf = [0u16; 32];
    let n = GetDlgItemTextW(dlg, id, &mut buf) as usize;
    String::from_utf16_lossy(&buf[..n])
}

unsafe fn set_check(dlg: HWND, id: i32, on: bool) {
    SendDlgItemMessageW(dlg, id, BM_SETCHECK, WPARAM(on as usize), LPARAM(0));
}

unsafe fn get_check(dlg: HWND, id: i32) -> bool {
    SendDlgItemMessageW(dlg, id, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1
}

unsafe fn fill(dlg: HWND, cfg: &Config) {
    for (i, v) in limit_vals(&cfg.limits).iter().enumerate() {
        set_text(dlg, ID_LIMIT0 + i as i32, &fmt_num(*v));
    }
    for (fi, a) in cfg.alarms.iter().enumerate() {
        for (ci, on) in alarm_bits(a).iter().enumerate() {
            set_check(dlg, ID_ALARM0 + (fi * 4 + ci) as i32, *on);
        }
    }
    update_enabled(dlg);
}

/// Halten/Ton/Zeigen nur bedienbar, wenn der Alarm aktiv ist.
unsafe fn update_enabled(dlg: HWND) {
    for fi in 0..FAULT_COUNT {
        let on = get_check(dlg, ID_ALARM0 + (fi * 4) as i32);
        for ci in 1..4 {
            if let Ok(h) = GetDlgItem(dlg, ID_ALARM0 + (fi * 4 + ci) as i32) {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow(h, on);
            }
        }
    }
}

/// Liest den Dialog aus. Bei ungültigen Werten: Meldung, Fokus auf das Feld.
unsafe fn read(dlg: HWND) -> Option<Config> {
    let mut vals = [0f32; 5];
    for (i, v) in vals.iter_mut().enumerate() {
        let id = ID_LIMIT0 + i as i32;
        match parse_num(&get_text(dlg, id)) {
            Some(n) => *v = n,
            None => {
                error(dlg, id, "Bitte eine Zahl eingeben.");
                return None;
            }
        }
    }
    let limits = Limits { pin_warn_a: vals[0], pin_limit_a: vals[1], full_scale_a: vals[2], imbalance_a: vals[3], connector_rated_w: vals[4] };
    if let Err((field, msg)) = limits.validate() {
        error(dlg, ID_LIMIT0 + field as i32, msg);
        return None;
    }
    let mut alarms = [AlarmCfg::default(); FAULT_COUNT];
    for (fi, a) in alarms.iter_mut().enumerate() {
        let c = |ci: usize| get_check(dlg, ID_ALARM0 + (fi * 4 + ci) as i32);
        *a = AlarmCfg { enabled: c(0), latch: c(1), sound: c(2), show: c(3) };
    }
    Some(Config { limits, alarms })
}

unsafe fn error(dlg: HWND, id: i32, msg: &str) {
    let m = wide(msg);
    MessageBoxW(dlg, PCWSTR(m.as_ptr()), w!("GpuMon – Einstellungen"), MB_OK | MB_ICONWARNING);
    if let Ok(h) = GetDlgItem(dlg, id) {
        let _ = SetFocus(h);
        SendMessageW(h, EM_SETSEL, WPARAM(0), LPARAM(-1));
    }
}

unsafe fn init(dlg: HWND, ctx: &Ctx) {
    let f = ctx.font;
    let st = w!("STATIC");
    let bt = w!("BUTTON");

    // Grenzwerte
    ctl(dlg, f, bt, "Grenzwerte", BS_GROUPBOX, 7, 5, DLG_W as i32 - 14, 92, -1);
    for (i, (label, unit)) in LIMIT_ROWS.iter().enumerate() {
        let y = 19 + i as i32 * 15;
        ctl(dlg, f, st, label, SS_LEFT, 15, y + 2, 140, 9, -1);
        ctl(dlg, f, w!("EDIT"), "", ES_AUTOHSCROLL | WS_TABSTOP.0 | if i == 0 { WS_GROUP.0 } else { 0 }, 160, y, 44, 12, ID_LIMIT0 + i as i32);
        ctl(dlg, f, st, unit, SS_LEFT, 209, y + 2, 20, 9, -1);
    }

    // Alarme
    let gy = 103;
    ctl(dlg, f, bt, "Alarme", BS_GROUPBOX, 7, gy, DLG_W as i32 - 14, 114, -1);
    for (ci, name) in ALARM_COLS.iter().enumerate() {
        ctl(dlg, f, st, name, SS_CENTER, COL_X[ci] - 8, gy + 12, 26, 9, -1);
    }
    for (fi, fault) in FAULTS.iter().enumerate() {
        let y = gy + 24 + fi as i32 * 13;
        ctl(dlg, f, st, fault.title(), SS_LEFT, 15, y + 1, 110, 9, -1);
        for ci in 0..4 {
            let id = ID_ALARM0 + (fi * 4 + ci) as i32;
            ctl(dlg, f, bt, "", BS_AUTOCHECKBOX | WS_TABSTOP.0, COL_X[ci], y, 10, 10, id);
        }
    }
    ctl(
        dlg,
        f,
        st,
        "Halten: bleibt orange markiert, bis die Statuszeile angeklickt wird.\nZeigen: Fenster wiederherstellen, nach vorne holen und blinken.",
        SS_LEFT,
        15,
        gy + 24 + FAULT_COUNT as i32 * 13 + 2,
        DLG_W as i32 - 30,
        16,
        -1,
    );

    // Knöpfe
    let by = DLG_H as i32 - 21;
    ctl(dlg, f, bt, "Standardwerte", BS_PUSHBUTTON | WS_TABSTOP.0, 7, by, 60, 14, ID_DEFAULTS);
    ctl(dlg, f, bt, "OK", BS_DEFPUSHBUTTON | WS_TABSTOP.0, DLG_W as i32 - 117, by, 52, 14, IDOK.0);
    ctl(dlg, f, bt, "Abbrechen", BS_PUSHBUTTON | WS_TABSTOP.0, DLG_W as i32 - 59, by, 52, 14, IDCANCEL.0);

    fill(dlg, &ctx.cfg);
    if let Ok(h) = GetDlgItem(dlg, ID_LIMIT0) {
        let _ = SetFocus(h);
    }
}

unsafe extern "system" fn dlg_proc(dlg: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> isize {
    match msg {
        WM_INITDIALOG => {
            let ctx = &mut *(lp.0 as *mut Ctx);
            ctx.font = HFONT(SendMessageW(dlg, WM_GETFONT, WPARAM(0), LPARAM(0)).0 as _);
            SetWindowLongPtrW(dlg, GWLP_USERDATA, lp.0);
            init(dlg, ctx);
            0 // Fokus selbst gesetzt
        }
        WM_COMMAND => {
            let ctx = &mut *(GetWindowLongPtrW(dlg, GWLP_USERDATA) as *mut Ctx);
            let id = (wp.0 & 0xFFFF) as i32;
            match id {
                x if x == IDOK.0 => {
                    if let Some(c) = read(dlg) {
                        ctx.cfg = c;
                        let _ = EndDialog(dlg, 1);
                    }
                }
                x if x == IDCANCEL.0 => {
                    let _ = EndDialog(dlg, 0);
                }
                ID_DEFAULTS => fill(dlg, &Config::default()),
                x if (ID_ALARM0..ID_ALARM0 + 4 * FAULT_COUNT as i32).contains(&x) && (x - ID_ALARM0) % 4 == 0 => update_enabled(dlg),
                _ => return 0,
            }
            1
        }
        _ => 0,
    }
}

/// Zeigt den Dialog modal über `owner`. Liefert die neue Konfiguration bei OK.
pub fn edit(owner: HWND, cfg: Config) -> Option<Config> {
    let tpl = template();
    let mut ctx = Ctx { cfg, font: HFONT::default() };
    let r = unsafe {
        DialogBoxIndirectParamW(
            GetModuleHandleW(None).map(|m| HINSTANCE(m.0)).unwrap_or_default(),
            tpl.as_ptr() as *const DLGTEMPLATE,
            owner,
            Some(dlg_proc),
            LPARAM(&mut ctx as *mut Ctx as isize),
        )
    };
    (r == 1).then_some(ctx.cfg)
}
