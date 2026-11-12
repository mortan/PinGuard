//! Einstellungsdialog für Grenzwerte, Alarme und die angezeigten Kennzahlen. Modaler Win32-Dialog aus
//! einer Speichervorlage (`DialogBoxIndirectParamW`), die Steuerelemente
//! werden in `WM_INITDIALOG` in Dialogeinheiten angelegt. Dadurch gibt es
//! Tab-Navigation, Enter/Esc und DPI-Skalierung ohne Ressourcendatei.

use crate::i18n::{short, t};
use crate::model::{field, AlarmCfg, Config, ImbalanceMode, Limits, Metric, CONNECTOR_METRICS, FAULTS, FAULT_COUNT, METRICS, METRIC_COUNT, PSU_METRICS};use crate::settings::parse_num;
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

const CBS_DROPDOWNLIST: u32 = 0x3;
const CB_ADDSTRING: u32 = 0x143;
const CB_GETCURSEL: u32 = 0x147;
const CB_SETCURSEL: u32 = 0x14E;
const CBN_SELCHANGE: u32 = 1;

const ID_LIMIT0: i32 = 100; // Eingabefelder in `field`-Reihenfolge
const ID_ALARM0: i32 = 200; // je Fehlerart 4 Kästchen
const ID_DEFAULTS: i32 = 300;
const ID_IMB_MODE: i32 = 310;
const ID_SHOW0: i32 = 400; // je `Metric` ein Kästchen

/// Breite der linken Spalte (Grenzwerte, Alarme); rechts daneben "Anzeige".
const LEFT_W: i32 = 262;
const DLG_W: i16 = 372;
const DLG_H: i16 = 319;

/// Zeilen der Gruppe "Grenzwerte": Einheit, Feldindex (`None` = Auswahl der
/// Schieflast-Messart). Die Beschriftungen stehen in `Texts::limit_rows`.
const LIMIT_ROWS: [(&str, Option<usize>); 8] = [
    ("A", Some(field::PIN_WARN)),
    ("A", Some(field::PIN_LIMIT)),
    ("A", Some(field::FULL_SCALE)),
    ("", None),
    ("A", Some(field::IMB_A)),
    ("%", Some(field::IMB_PCT)),
    ("A", Some(field::IMB_MIN_LOAD)),
    ("W", Some(field::RATED_W)),
];
/// Reihenfolge wie `Texts::modes`.
const MODES: [ImbalanceMode; 2] = [ImbalanceMode::Amps, ImbalanceMode::Percent];
const COL_X: [i32; 4] = [130, 160, 190, 220];

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn limit_vals(l: &Limits) -> [f32; field::COUNT] {
    [l.pin_warn_a, l.pin_limit_a, l.full_scale_a, l.imbalance_a, l.imbalance_pct, l.imbalance_min_load_a, l.connector_rated_w]
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
    t.extend(wide(crate::i18n::t().dlg_title));
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
        // Zahl im Format der Sprache; die Eingabe akzeptiert Punkt und Komma.
        set_text(dlg, ID_LIMIT0 + i as i32, &short(*v));
    }
    let mode = MODES.iter().position(|m| *m == cfg.limits.imbalance_mode).unwrap_or(0);
    SendDlgItemMessageW(dlg, ID_IMB_MODE, CB_SETCURSEL, WPARAM(mode), LPARAM(0));
    for (fi, a) in cfg.alarms.iter().enumerate() {
        for (ci, on) in alarm_bits(a).iter().enumerate() {
            set_check(dlg, ID_ALARM0 + (fi * 4 + ci) as i32, *on);
        }
    }
    for m in METRICS {
        set_check(dlg, ID_SHOW0 + m as i32, cfg.shows(m));
    }
    update_enabled(dlg);
}

unsafe fn get_mode(dlg: HWND) -> ImbalanceMode {
    let i = SendDlgItemMessageW(dlg, ID_IMB_MODE, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
    MODES.get(i as usize).copied().unwrap_or_default()
}

unsafe fn enable(dlg: HWND, id: i32, on: bool) {
    if let Ok(h) = GetDlgItem(dlg, id) {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow(h, on);
    }
}

/// Halten/Ton/Zeigen nur bedienbar, wenn der Alarm aktiv ist; von den
/// Schieflast-Feldern nur die der gewählten Messart.
unsafe fn update_enabled(dlg: HWND) {
    for fi in 0..FAULT_COUNT {
        let on = get_check(dlg, ID_ALARM0 + (fi * 4) as i32);
        for ci in 1..4 {
            enable(dlg, ID_ALARM0 + (fi * 4 + ci) as i32, on);
        }
    }
    let pct = get_mode(dlg) == ImbalanceMode::Percent;
    enable(dlg, ID_LIMIT0 + field::IMB_A as i32, !pct);
    enable(dlg, ID_LIMIT0 + field::IMB_PCT as i32, pct);
    enable(dlg, ID_LIMIT0 + field::IMB_MIN_LOAD as i32, pct);
    // Netzteil-Kennzahlen entfallen, solange die Verlaufskurve gezeigt wird.
    let hist = get_check(dlg, ID_SHOW0 + Metric::History as i32);
    for m in PSU_METRICS {
        enable(dlg, ID_SHOW0 + m as i32, !hist);
    }
}

/// Liest den Dialog aus. Bei ungültigen Werten: Meldung, Fokus auf das Feld.
unsafe fn read(dlg: HWND) -> Option<Config> {
    let mut vals = [0f32; field::COUNT];
    for (i, v) in vals.iter_mut().enumerate() {
        let id = ID_LIMIT0 + i as i32;
        match parse_num(&get_text(dlg, id)) {
            Some(n) => *v = n,
            None => {
                error(dlg, id, t().err_number);
                return None;
            }
        }
    }
    let limits = Limits {
        pin_warn_a: vals[field::PIN_WARN],
        pin_limit_a: vals[field::PIN_LIMIT],
        full_scale_a: vals[field::FULL_SCALE],
        imbalance_mode: get_mode(dlg),
        imbalance_a: vals[field::IMB_A],
        imbalance_pct: vals[field::IMB_PCT],
        imbalance_min_load_a: vals[field::IMB_MIN_LOAD],
        connector_rated_w: vals[field::RATED_W],
    };
    if let Err((field, msg)) = limits.validate() {
        error(dlg, ID_LIMIT0 + field as i32, msg);
        return None;
    }
    let mut alarms = [AlarmCfg::default(); FAULT_COUNT];
    for (fi, a) in alarms.iter_mut().enumerate() {
        let c = |ci: usize| get_check(dlg, ID_ALARM0 + (fi * 4 + ci) as i32);
        *a = AlarmCfg { enabled: c(0), latch: c(1), sound: c(2), show: c(3) };
    }
    let mut show = [false; METRIC_COUNT];
    for (i, s) in show.iter_mut().enumerate() {
        *s = get_check(dlg, ID_SHOW0 + i as i32);
    }
    Some(Config { limits, alarms, show })
}

unsafe fn error(dlg: HWND, id: i32, msg: &str) {
    let m = wide(msg);
    let title = wide(t().dlg_title);
    MessageBoxW(dlg, PCWSTR(m.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONWARNING);
    if let Ok(h) = GetDlgItem(dlg, id) {
        let _ = SetFocus(h);
        SendMessageW(h, EM_SETSEL, WPARAM(0), LPARAM(-1));
    }
}

unsafe fn init(dlg: HWND, ctx: &Ctx) {
    let f = ctx.font;
    let tx = t();
    let st = w!("STATIC");
    let bt = w!("BUTTON");

    // Grenzwerte
    let rows = LIMIT_ROWS.len() as i32;
    ctl(dlg, f, bt, tx.grp_limits, BS_GROUPBOX, 7, 5, LEFT_W - 14, 17 + rows * 15, -1);
    for (i, ((unit, fld), label)) in LIMIT_ROWS.iter().zip(tx.limit_rows).enumerate() {
        let y = 19 + i as i32 * 15;
        ctl(dlg, f, st, label, SS_LEFT, 15, y + 2, 140, 9, -1);
        match fld {
            Some(fi) => {
                let style = ES_AUTOHSCROLL | WS_TABSTOP.0 | if i == 0 { WS_GROUP.0 } else { 0 };
                ctl(dlg, f, w!("EDIT"), "", style, 160, y, 44, 12, ID_LIMIT0 + *fi as i32);
                ctl(dlg, f, st, unit, SS_LEFT, 209, y + 2, 20, 9, -1);
            }
            None => {
                // Höhe inkl. aufgeklappter Liste
                let cb = ctl(dlg, f, w!("COMBOBOX"), "", CBS_DROPDOWNLIST | WS_VSCROLL.0 | WS_TABSTOP.0, 160, y, 88, 60, ID_IMB_MODE);
                for name in tx.modes {
                    let t = wide(name);
                    SendMessageW(cb, CB_ADDSTRING, WPARAM(0), LPARAM(t.as_ptr() as isize));
                }
            }
        }
    }

    // Alarme
    let gy = 13 + 17 + rows * 15;
    // Unterkante beider Spalten gemeinsam über den Knöpfen
    let bottom = DLG_H as i32 - 29;
    ctl(dlg, f, bt, tx.grp_alarms, BS_GROUPBOX, 7, gy, LEFT_W - 14, bottom - gy, -1);
    for (ci, name) in tx.alarm_cols.iter().enumerate() {
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
        tx.alarm_note,
        SS_LEFT,
        15,
        gy + 24 + FAULT_COUNT as i32 * 13 + 2,
        LEFT_W - 30,
        16,
        -1,
    );

    // Anzeige: welche Kennzahlen das rechte Panel zeigt
    let ax = LEFT_W - 2;
    ctl(dlg, f, bt, tx.grp_display, BS_GROUPBOX, ax, 5, DLG_W as i32 - 7 - ax, bottom - 5, -1);
    let mut y = 19;
    // Die Verlaufskurve steht bei „Netzteil“, weil sie diese Gruppe ersetzt.
    let psu: [Metric; 5] = [PSU_METRICS[0], PSU_METRICS[1], PSU_METRICS[2], PSU_METRICS[3], Metric::History];
    let groups: [(&str, &[Metric]); 4] = [
        (tx.disp_conn, &CONNECTOR_METRICS),
        (tx.disp_psu, &psu),
        ("", &[Metric::Rails]),
        (tx.disp_bars, &[Metric::WarnLine, Metric::MeanLine]),
    ];
    for (name, ms) in groups {
        if !name.is_empty() {
            ctl(dlg, f, st, name, SS_LEFT, ax + 8, y, 80, 9, -1);
            y += 12;
        }
        for m in ms {
            ctl(dlg, f, bt, m.title(), BS_AUTOCHECKBOX | WS_TABSTOP.0, ax + 14, y, 84, 10, ID_SHOW0 + *m as i32);
            y += 13;
        }
        y += 5;
    }
    ctl(dlg, f, st, tx.load_note, SS_LEFT, ax + 8, y + 2, 92, 26, -1);

    // Knöpfe
    let by = DLG_H as i32 - 21;
    ctl(dlg, f, bt, tx.btn_defaults, BS_PUSHBUTTON | WS_TABSTOP.0, 7, by, 60, 14, ID_DEFAULTS);
    ctl(dlg, f, bt, "OK", BS_DEFPUSHBUTTON | WS_TABSTOP.0, DLG_W as i32 - 117, by, 52, 14, IDOK.0);
    ctl(dlg, f, bt, tx.btn_cancel, BS_PUSHBUTTON | WS_TABSTOP.0, DLG_W as i32 - 59, by, 52, 14, IDCANCEL.0);

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
                ID_IMB_MODE if (wp.0 >> 16) as u32 == CBN_SELCHANGE => update_enabled(dlg),
                x if x == ID_SHOW0 + Metric::History as i32 => update_enabled(dlg),
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
