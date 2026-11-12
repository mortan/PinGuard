//! Zeichnen mit GDI (Text, ClearType) und GDI+ (geglättete Formen) in einen
//! Backbuffer. Gezeichnet wird nur auf Anforderung (neue Daten, WM_PAINT,
//! Größenänderung) – es gibt keine Animationsschleife.
//!
//! Bewusst kein Direct2D: Direct2D lädt auch im Software-Modus D3D/WARP, und
//! dann injiziert sich RTSS (samt NVAPI) in den Prozess – das kostete
//! ~30 MB Working Set und ~60 MB privaten Speicher mehr.
//!
//! Das Layout ist in einem festen Basisraster (`BASE_W` × `BASE_H` DIPs)
//! definiert und wird auf die Fenstergröße skaliert.

use crate::model::*;
use crate::source::Reading;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::GdiPlus::*;

pub const BASE_W: f32 = 440.0;
pub const BASE_H: f32 = 196.0;
/// Statuszeile (Klick = Fehler zurücksetzen), in Basiskoordinaten.
pub const STATUS_Y0: f32 = 166.0;
pub const STATUS_Y1: f32 = 190.0;
pub const STATUS_X0: f32 = 6.0;
pub const STATUS_X1: f32 = 434.0;

// Farben (Ampel aus den RTSS-Overlays übernommen)
const GREEN: u32 = 0x3CDC78;
const YELLOW: u32 = 0xFFC400;
const RED: u32 = 0xFF3B3B;
const BG: u32 = 0x101216;
const PANEL: u32 = 0x1A1D23;
const TRACK: u32 = 0x272B33;
const TEXT: u32 = 0xF0F0F0;
const DIM: u32 = 0x8A909C;
const FAINT: u32 = 0x4A505C;
/// Pin-Nummern: heller als `FAINT`, damit sie auch klein lesbar bleiben.
const PIN_NO: u32 = 0x6A7080;
const ACCENT: u32 = 0x4FC3F7;
const AMBER: u32 = 0xFFA726;

fn argb(c: u32, a: f32) -> u32 {
    ((a.clamp(0.0, 1.0) * 255.0) as u32) << 24 | (c & 0xFFFFFF)
}

/// RGB → COLORREF (BGR); Alpha wird mit dem Panel-Hintergrund vorgemischt,
/// weil GDI-Text keine Transparenz kennt.
fn colorref(c: u32, a: f32) -> COLORREF {
    let mix = |sh: u32| {
        let f = ((c >> sh) & 0xFF) as f32;
        let b = ((PANEL >> sh) & 0xFF) as f32;
        (f * a + b * (1.0 - a)).round() as u32
    };
    COLORREF(mix(16) | mix(8) << 8 | mix(0) << 16)
}

fn level_color(l: &Limits, a: f32) -> u32 {
    match l.pin_level(a) {
        0 => GREEN,
        1 => YELLOW,
        _ => RED,
    }
}

/// Farbe einer Kennzahl: unauffällig weiß, Farbe nur bei Warnung/Alarm.
fn value_color(level: u8) -> u32 {
    match level {
        0 => TEXT,
        1 => YELLOW,
        _ => RED,
    }
}

#[derive(Clone, Copy)]
struct R {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

fn rect(l: f32, t: f32, r: f32, b: f32) -> R {
    R { left: l, top: t, right: r, bottom: b }
}

// Balkenfläche (Basiskoordinaten); von Zeichnen und Tooltips gemeinsam genutzt.
const BAR_TOP: f32 = 30.0;
const BAR_BOTTOM: f32 = 124.0;
const BAR_X0: f32 = 14.0;
const BAR_X1: f32 = 198.0;

/// y-Position eines Pin-Stroms in der Balkenfläche.
fn amp_y(lim: &Limits, a: f32) -> f32 {
    let fs = lim.full_scale_a;
    BAR_BOTTOM - (a.clamp(0.0, fs) / fs) * (BAR_BOTTOM - BAR_TOP)
}

/// Breite einer Steckergruppe und Abstand zwischen den Gruppen.
fn bar_groups(n: usize) -> (f32, f32) {
    let gap = if n == 2 { 10.0 } else { 0.0 };
    ((BAR_X1 - BAR_X0 - gap * (n as f32 - 1.0)) / n as f32, gap)
}

const CHIPS: [(Fault, &str); FAULT_COUNT] = [
    (Fault::Alarm1, "ALARM S1"),
    (Fault::Alarm2, "ALARM S2"),
    (Fault::OverCurrent, "ÜBERSTROM"),
    (Fault::Imbalance, "SCHIEFLAST"),
    (Fault::NoData, "VERBINDUNG"),
];

/// Die angezeigten Statusfelder mit ihrer Fläche.
fn visible_chips(v: &View) -> Vec<(Fault, &'static str, R)> {
    let chips: Vec<_> = CHIPS.iter().filter(|(f, _)| v.monitor.show_fault(*f)).collect();
    let gap = 5.0;
    let n = chips.len() as f32;
    let cw = (STATUS_X1 - STATUS_X0 - gap * (n - 1.0)) / n;
    chips
        .iter()
        .enumerate()
        .map(|(i, (f, label))| {
            let x = STATUS_X0 + i as f32 * (cw + gap);
            (*f, *label, rect(x, STATUS_Y0, x + cw, STATUS_Y1))
        })
        .collect()
}

/// Zahl ohne unnötige Nachkommastellen, mit Dezimalkomma.
fn num(v: f32) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').replace('.', ",")
}

/// Tooltip für eine Stelle der Oberfläche. `id` kennzeichnet den Bereich
/// (bleibt gleich, solange die Maus im selben Bereich ist), der Text kann
/// sich mit den Daten ändern.
pub struct Hint {
    pub id: u32,
    pub text: String,
}

/// Liefert den Tooltip für einen Punkt in Basiskoordinaten.
pub fn hint_at(x: f32, y: f32, v: &View) -> Option<Hint> {
    let lim = &v.config.limits;
    let hint = |id: u32, text: String| Some(Hint { id, text });

    // Statuszeile
    if (STATUS_Y0..=STATUS_Y1).contains(&y) {
        let (fault, _, _) = visible_chips(v).into_iter().find(|(_, _, r)| x >= r.left && x <= r.right)?;
        let what = match fault {
            Fault::Alarm1 => "Netzteil-Alarm Stecker 1: vom Netzteil selbst gemeldet".to_string(),
            Fault::Alarm2 => "Netzteil-Alarm Stecker 2: vom Netzteil selbst gemeldet".to_string(),
            Fault::OverCurrent => format!("Überstrom: ein Pin führt {} A oder mehr", num(lim.pin_limit_a)),
            Fault::Imbalance => match lim.imbalance_mode {
                ImbalanceMode::Amps => format!("Schieflast: größter minus kleinster Pin-Strom ab {} A", num(lim.imbalance_a)),
                ImbalanceMode::Percent => format!("Schieflast: ein Pin weicht {} % oder mehr vom Mittel ab", num(lim.imbalance_pct)),
            },
            Fault::NoData => format!("Verbindung: keine Netzteil-Daten von {}", v.source_name),
        };
        let st = v.monitor.fault(fault);
        let state = if !v.config.alarm(fault).enabled {
            "Alarm ist abgeschaltet."
        } else if st.active {
            "Jetzt aktiv."
        } else if st.latched {
            "War ausgelöst – Klick setzt zurück."
        } else {
            "In Ordnung."
        };
        return hint(300 + fault as u32, format!("{what}\n{state}"));
    }

    let Reading::Data(snap) = v.reading else { return None };
    let stats = [ConnectorStats::from_snapshot(snap, 0), ConnectorStats::from_snapshot(snap, 1)];
    let (vis, nvis) = visible_connectors(&stats);
    let vis = &vis[..nvis];
    let both = nvis == 2;

    // Balken-Panel
    if (BAR_X0 - 3.0..=BAR_X1 + 3.0).contains(&x) {
        let in_bars = (BAR_TOP..=BAR_BOTTOM).contains(&y);
        let on_label = !both && x >= 158.0 && (9.0..=25.0).contains(&y);
        if on_label || (in_bars && (y - amp_y(lim, lim.pin_limit_a)).abs() <= 2.5) {
            return hint(1, format!("Überstrom-Grenze: {} A je Pin\nAb hier wird der Pin rot und ÜBERSTROM löst aus.", num(lim.pin_limit_a)));
        }
        if in_bars && (y - amp_y(lim, lim.pin_warn_a)).abs() <= 2.5 {
            return hint(2, format!("Warnschwelle: {} A je Pin\nAb hier wird der Pin gelb.", num(lim.pin_warn_a)));
        }
        if (26.0..=157.0).contains(&y) && (BAR_X0..BAR_X1).contains(&x) {
            let (group_w, gap) = bar_groups(nvis);
            let g = (((x - BAR_X0) / (group_w + gap)) as usize).min(nvis - 1);
            let lx = x - BAR_X0 - g as f32 * (group_w + gap);
            if lx < group_w {
                let p = ((lx / group_w * PINS as f32) as usize).min(PINS - 1);
                let c = vis[g];
                let now = if stats[c].present { format!("{} A", de(stats[c].pins[p], 2)) } else { "keine Daten".into() };
                let text = format!(
                    "Stecker {} · Pin {}\nAktuell: {}\nPeak: {} A (Doppelklick setzt zurück)",
                    c + 1,
                    p + 1,
                    now,
                    de(v.monitor.peaks[c][p], 2)
                );
                return hint(100 + (c * PINS + p) as u32, text);
            }
        }
        return None;
    }

    // Kennzahlen
    if (212.0..434.0).contains(&x) && (10.0..130.0).contains(&y) {
        let col = (((x - 212.0) / 111.0) as usize).min(1);
        let row = (((y - 10.0) / 24.0) as usize).min(4);
        let scope = if both { " beider Stecker" } else { "" };
        let text = match (row, col) {
            (0, 0) => format!("Leistung über den 12V-2x6-Stecker\nSumme der Pin-Ströme{scope} × 12-V-Spannung"),
            (0, 1) => format!("Auslastung: Steckerleistung bezogen auf {} W Nennleistung je Stecker\nGelb ab 85 %, rot ab 100 %", num(lim.connector_rated_w)),
            (1, 0) => format!("Summe der Pin-Ströme{scope}"),
            (1, 1) => format!(
                "Höchster Pin-Strom im Moment\n{}\nGelb ab {} A, rot ab {} A",
                if both { "1.3 = Stecker 1, Pin 3" } else { "P3 = Pin 3" },
                num(lim.pin_warn_a),
                num(lim.pin_limit_a)
            ),
            (2, 0) => {
                let worst = if both { "\nAngezeigt wird der höhere Wert beider Stecker." } else { "" };
                match lim.imbalance_mode {
                    ImbalanceMode::Amps => format!(
                        "Schieflast: größter minus kleinster Pin-Strom\nGelb ab {} A, Alarm ab {} A{worst}",
                        num(lim.imbalance_a * 0.5),
                        num(lim.imbalance_a)
                    ),
                    ImbalanceMode::Percent => format!(
                        "Schieflast: größte Abweichung eines Pins vom Mittelwert\nGelb ab {} %, Alarm ab {} %, bewertet ab {} A Steckerstrom{worst}",
                        num(lim.imbalance_pct * 0.5),
                        num(lim.imbalance_pct),
                        num(lim.imbalance_min_load_a)
                    ),
                }
            }
            (2, 1) => format!("Leistungsaufnahme der Grafikkarte\n(Sensor „Power“ aus {})", v.source_name),
            (3, 0) => "Gesamte Ausgangsleistung des Netzteils (DC-Seite)".to_string(),
            (3, 1) => "Vom Netzteil gemeldeter Wirkungsgrad".to_string(),
            (4, 0) => "Temperatur im Netzteil".to_string(),
            _ => "Drehzahl des Netzteillüfters\n0 rpm: Lüfter steht".to_string(),
        };
        return hint(200 + (row * 2 + col) as u32, text);
    }

    // Rails
    if (218.0..431.0).contains(&x) && (132.0..=157.0).contains(&y) {
        let i = (((x - 218.0) / 71.0) as usize).min(2);
        let name = ["12-V", "5-V", "3,3-V"][i];
        return hint(220 + i as u32, format!("{name}-Schiene des Netzteils: Spannung · Strom"));
    }
    None
}

/// Zahl mit deutschem Dezimalkomma.
fn de(v: f32, decimals: usize) -> String {
    format!("{:.*}", decimals, v).replace('.', ",")
}

fn opt(v: Option<f32>, decimals: usize, unit: &str) -> String {
    match v {
        Some(v) => format!("{} {}", de(v, decimals), unit),
        None => "–".into(),
    }
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

/// Schriften; Größe in DIPs im Basisraster und Gewicht in `FONT_SPECS`.
#[derive(Clone, Copy)]
enum F {
    Label,
    Value,
    Small,
    Title,
    Chip,
    Msg,
    /// Pinwerte in der Zwei-Stecker-Ansicht
    Tiny,
}
const FONT_SPECS: [(f32, i32); 7] = [(10.0, 400), (14.0, 600), (10.5, 600), (11.0, 600), (9.5, 700), (13.0, 600), (8.5, 600)];

pub struct Renderer {
    gdip_token: usize,
    dpi: f32,
    /// Skalierung Basis → DIP und Versatz in DIPs (für Hit-Tests).
    pub scale: f32,
    pub offset: (f32, f32),
    // Backbuffer
    mem_dc: HDC,
    bmp: HBITMAP,
    old_bmp: HGDIOBJ,
    buf_size: (i32, i32),
    // Schriften für die aktuelle Pixelskalierung
    fonts: Vec<HFONT>,
    font_px: f32,
}

/// Was gezeichnet werden soll.
pub struct View<'a> {
    pub reading: &'a Reading,
    pub monitor: &'a Monitor,
    pub config: &'a Config,
    pub source_name: &'a str,
}

impl Renderer {
    pub fn new() -> Option<Self> {
        let mut token = 0usize;
        let input = GdiplusStartupInput { GdiplusVersion: 1, ..Default::default() };
        let st = unsafe { GdiplusStartup(&mut token, &input, std::ptr::null_mut()) };
        if st.0 != 0 {
            return None;
        }
        Some(Renderer {
            gdip_token: token,
            dpi: 96.0,
            scale: 1.0,
            offset: (0.0, 0.0),
            mem_dc: HDC::default(),
            bmp: HBITMAP::default(),
            old_bmp: HGDIOBJ::default(),
            buf_size: (0, 0),
            fonts: Vec::new(),
            font_px: 0.0,
        })
    }

    pub fn set_dpi(&mut self, dpi: u32) {
        self.dpi = dpi.max(48) as f32;
    }

    unsafe fn free_buffer(&mut self) {
        if !self.mem_dc.is_invalid() {
            SelectObject(self.mem_dc, self.old_bmp);
            let _ = DeleteObject(self.bmp);
            let _ = DeleteDC(self.mem_dc);
        }
        self.mem_dc = HDC::default();
        self.bmp = HBITMAP::default();
        self.buf_size = (0, 0);
    }

    /// Gibt Backbuffer und Schriften frei (z. B. wenn minimiert).
    pub fn discard(&mut self) {
        unsafe {
            self.free_buffer();
            for f in self.fonts.drain(..) {
                let _ = DeleteObject(f);
            }
        }
        self.font_px = 0.0;
    }

    /// Berechnet Skalierung/Versatz für eine Clientgröße in Pixeln.
    pub fn update_layout(&mut self, w: u32, h: u32) {
        let k = self.dpi / 96.0;
        let (wd, hd) = (w as f32 / k, h as f32 / k);
        self.scale = (wd / BASE_W).min(hd / BASE_H).max(0.1);
        self.offset = ((wd - BASE_W * self.scale) / 2.0, (hd - BASE_H * self.scale) / 2.0);
    }

    /// Wandelt Clientpixel in Basiskoordinaten.
    pub fn to_base(&self, x: i32, y: i32) -> (f32, f32) {
        let k = self.dpi / 96.0;
        ((x as f32 / k - self.offset.0) / self.scale, (y as f32 / k - self.offset.1) / self.scale)
    }

    unsafe fn ensure_buffer(&mut self, wnd_dc: HDC, w: i32, h: i32) {
        if self.buf_size == (w, h) && !self.mem_dc.is_invalid() {
            return;
        }
        self.free_buffer();
        self.mem_dc = CreateCompatibleDC(wnd_dc);
        // Geräteabhängige Bitmap: liegt nicht im privaten Speicher des Prozesses
        // (eine DIB-Section wäre ~10 % schneller, kostet aber w*h*4 Byte RAM).
        self.bmp = CreateCompatibleBitmap(wnd_dc, w, h);
        self.old_bmp = SelectObject(self.mem_dc, self.bmp);
        SetBkMode(self.mem_dc, TRANSPARENT);
        self.buf_size = (w, h);
    }

    unsafe fn ensure_fonts(&mut self, px: f32) {
        if (self.font_px - px).abs() < 1e-3 && !self.fonts.is_empty() {
            return;
        }
        for f in self.fonts.drain(..) {
            let _ = DeleteObject(f);
        }
        for (size, weight) in FONT_SPECS {
            let h = -((size * px).round() as i32).max(1);
            // DEFAULT_CHARSET = 1, CLEARTYPE_QUALITY = 5
            self.fonts.push(CreateFontW(h, 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI")));
        }
        self.font_px = px;
    }

    /// Zeichnet in den Backbuffer und kopiert ihn ins Fenster.
    pub fn render(&mut self, wnd_dc: HDC, w: u32, h: u32, v: &View) {
        if w == 0 || h == 0 {
            return;
        }
        self.update_layout(w, h);
        let k = self.dpi / 96.0;
        let px = self.scale * k;
        unsafe {
            self.ensure_buffer(wnd_dc, w as i32, h as i32);
            self.ensure_fonts(px);
            let dc = self.mem_dc;
            let mut g: *mut GpGraphics = std::ptr::null_mut();
            if GdipCreateFromHDC(dc, &mut g).0 != 0 {
                return;
            }
            let mut brush: *mut GpSolidFill = std::ptr::null_mut();
            let mut path: *mut GpPath = std::ptr::null_mut();
            GdipCreateSolidFill(argb(BG, 1.0), &mut brush);
            GdipCreatePath(FillModeAlternate, &mut path);
            GdipFillRectangle(g, brush as *mut GpBrush, 0.0, 0.0, w as f32, h as f32);
            GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
            GdipSetPixelOffsetMode(g, PixelOffsetModeHalf);
            GdipScaleWorldTransform(g, px, px, MatrixOrderAppend);
            GdipTranslateWorldTransform(g, self.offset.0 * k, self.offset.1 * k, MatrixOrderAppend);

            let p = Painter { g, dc, brush, path, px, ox: self.offset.0 * k, oy: self.offset.1 * k, fonts: &self.fonts, pending: Default::default() };
            p.draw(v);
            GdipFlush(g, FlushIntentionSync);

            GdipDeletePath(path);
            GdipDeleteBrush(brush as *mut GpBrush);
            GdipDeleteGraphics(g);
            let _ = BitBlt(wnd_dc, 0, 0, w as i32, h as i32, dc, 0, 0, SRCCOPY);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.discard();
        unsafe { GdiplusShutdown(self.gdip_token) };
    }
}

/// Zustand eines einzelnen Zeichenvorgangs.
struct Painter<'a> {
    g: *mut GpGraphics,
    dc: HDC,
    brush: *mut GpSolidFill,
    path: *mut GpPath,
    px: f32,
    ox: f32,
    oy: f32,
    fonts: &'a [HFONT],
    /// Seit dem letzten Flush wurde mit GDI+ gezeichnet.
    pending: std::cell::Cell<bool>,
}

impl Painter<'_> {
    fn fill(&self, r: R, c: u32, a: f32) {
        unsafe {
            self.pending.set(true);
            GdipSetSolidFillColor(self.brush, argb(c, a));
            GdipFillRectangle(self.g, self.brush as *mut GpBrush, r.left, r.top, r.right - r.left, r.bottom - r.top);
        }
    }

    fn fill_round(&self, r: R, rad: f32, c: u32, a: f32) {
        let d = (rad * 2.0).min(r.right - r.left).min(r.bottom - r.top);
        if d <= 0.01 {
            return self.fill(r, c, a);
        }
        unsafe {
            let p = self.path;
            GdipResetPath(p);
            GdipAddPathArc(p, r.left, r.top, d, d, 180.0, 90.0);
            GdipAddPathArc(p, r.right - d, r.top, d, d, 270.0, 90.0);
            GdipAddPathArc(p, r.right - d, r.bottom - d, d, d, 0.0, 90.0);
            GdipAddPathArc(p, r.left, r.bottom - d, d, d, 90.0, 90.0);
            GdipClosePathFigure(p);
            self.pending.set(true);
            GdipSetSolidFillColor(self.brush, argb(c, a));
            GdipFillPath(self.g, self.brush as *mut GpBrush, p);
        }
    }

    fn text(&self, s: &str, f: F, r: R, al: Align, c: u32, a: f32) {
        let mut s: Vec<u16> = s.encode_utf16().collect();
        let mut rc = RECT {
            left: (self.ox + r.left * self.px).round() as i32,
            top: (self.oy + r.top * self.px).round() as i32,
            right: (self.ox + r.right * self.px).round() as i32,
            bottom: (self.oy + r.bottom * self.px).round() as i32,
        };
        let flags = DT_SINGLELINE
            | DT_VCENTER
            | DT_NOPREFIX
            | DT_NOCLIP
            | match al {
                Align::Left => DT_LEFT,
                Align::Center => DT_CENTER,
                Align::Right => DT_RIGHT,
            };
        unsafe {
            // GDI+ muss fertig gezeichnet haben, bevor GDI-Text darüber kommt.
            if self.pending.replace(false) {
                GdipFlush(self.g, FlushIntentionSync);
            }
            SelectObject(self.dc, self.fonts[f as usize]);
            SetTextColor(self.dc, colorref(c, a));
            DrawTextW(self.dc, &mut s, &mut rc, flags);
        }
    }

    /// Textbreite in Basiskoordinaten.
    fn text_width(&self, s: &str, f: F) -> f32 {
        let s: Vec<u16> = s.encode_utf16().collect();
        let mut sz = SIZE::default();
        unsafe {
            SelectObject(self.dc, self.fonts[f as usize]);
            let _ = GetTextExtentPoint32W(self.dc, &s, &mut sz);
        }
        sz.cx as f32 / self.px
    }

    fn draw(&self, v: &View) {
        let snap = match v.reading {
            Reading::Data(s) => Some(s),
            _ => None,
        };
        let stats = snap.map(|s| [ConnectorStats::from_snapshot(s, 0), ConnectorStats::from_snapshot(s, 1)]);
        let (vis, nvis) = stats.as_ref().map(visible_connectors).unwrap_or(([0, 0], 1));

        // Panels
        self.fill_round(rect(6.0, 6.0, 206.0, 160.0), 6.0, PANEL, 1.0);
        self.fill_round(rect(212.0, 6.0, 434.0, 160.0), 6.0, PANEL, 1.0);

        self.draw_bars(v, stats.as_ref(), &vis[..nvis]);
        self.draw_metrics(&v.config.limits, snap, stats.as_ref(), &vis[..nvis]);
        self.draw_status(v);

        // Hinweis bei fehlenden Daten über dem Balken-Panel
        let hint = match v.reading {
            Reading::Data(_) => None,
            Reading::Offline => Some((format!("{} läuft nicht", v.source_name), "warte auf Daten …")),
            Reading::NoPsu => Some(("Keine PSU-Daten".to_string(), "PSU-Plugin in Afterburner aktivieren")),
        };
        if let Some((l1, l2)) = hint {
            self.fill_round(rect(14.0, 58.0, 198.0, 108.0), 5.0, BG, 0.88);
            self.text(&l1, F::Msg, rect(14.0, 62.0, 198.0, 82.0), Align::Center, AMBER, 1.0);
            self.text(l2, F::Label, rect(14.0, 84.0, 198.0, 102.0), Align::Center, DIM, 1.0);
        }
    }

    fn draw_bars(&self, v: &View, stats: Option<&[ConnectorStats; 2]>, vis: &[usize]) {
        let (top, bottom) = (BAR_TOP, BAR_BOTTOM);
        let lim = &v.config.limits;
        let y_of = |a: f32| amp_y(lim, a);
        let (x0, x1) = (BAR_X0, BAR_X1);
        let compact = vis.len() == 2;
        let (group_w, gap) = bar_groups(vis.len());

        if compact {
            // Wie in MSI Center (GPU Safeguard+): je Stecker eine Klammer über
            // seinen sechs Pins, beschriftet mit der Steckernummer.
            for (g, &c) in vis.iter().enumerate() {
                let (l, r) = (x0 + g as f32 * (group_w + gap) + 1.0, x0 + g as f32 * (group_w + gap) + group_w - 1.0);
                let mid = (l + r) / 2.0;
                let (yl, half) = (17.0, 27.0);
                self.fill(rect(l, yl - 0.5, mid - half, yl + 0.5), FAINT, 1.0);
                self.fill(rect(mid + half, yl - 0.5, r, yl + 0.5), FAINT, 1.0);
                self.fill(rect(l, yl - 0.5, l + 1.0, 26.0), FAINT, 1.0);
                self.fill(rect(r - 1.0, yl - 0.5, r, 26.0), FAINT, 1.0);
                self.text(&format!("Stecker {}", c + 1), F::Title, rect(mid - half, 9.0, mid + half, 25.0), Align::Center, DIM, 1.0);
            }
        } else {
            let title = format!("12V-2x6 · Stecker {}", vis[0] + 1);
            self.text(&title, F::Title, rect(14.0, 9.0, 200.0, 25.0), Align::Left, DIM, 1.0);
        }

        for (g, &c) in vis.iter().enumerate() {
            let gx = x0 + g as f32 * (group_w + gap);
            let step = group_w / PINS as f32;
            let bw = step * 0.64;
            let st = stats.map(|s| s[c]);
            for p in 0..PINS {
                let bx = gx + p as f32 * step + (step - bw) / 2.0;
                self.fill_round(rect(bx, top, bx + bw, bottom), 2.0, TRACK, 1.0);
                let val = st.filter(|s| s.present).map(|s| s.pins[p]);
                if let Some(a) = val {
                    let col = level_color(lim, a);
                    let y = y_of(a);
                    if bottom - y > 0.5 {
                        self.fill_round(rect(bx, y, bx + bw, bottom), 2.0, col, 1.0);
                    }
                    let txt = de(a, if compact { 1 } else { 2 });
                    let f = if compact { F::Tiny } else { F::Small };
                    self.text(&txt, f, rect(bx - 8.0, 126.0, bx + bw + 8.0, 141.0), Align::Center, col, 1.0);
                } else {
                    self.text("–", F::Small, rect(bx - 8.0, 126.0, bx + bw + 8.0, 141.0), Align::Center, FAINT, 1.0);
                }
                // Peak seit Start / Reset: dünne helle Linie, damit er sich vom
                // Balken abhebt; im gelben/roten Bereich in Ampelfarbe.
                let pk = v.monitor.peaks[c][p];
                if pk > 0.05 {
                    let y = y_of(pk);
                    let (pc, pa) = match lim.pin_level(pk) {
                        0 => (TEXT, 0.8),
                        _ => (level_color(lim, pk), 1.0),
                    };
                    self.fill(rect(bx - 1.0, y - 0.6, bx + bw + 1.0, y + 0.6), pc, pa);
                }
                self.text(&format!("{}", p + 1), F::Label, rect(bx - 6.0, 141.0, bx + bw + 6.0, 155.0), Align::Center, PIN_NO, 1.0);
            }
        }
        // Rot-Grenze (Überstrom) und Gelb-Markierung
        let yl = y_of(lim.pin_limit_a);
        self.fill(rect(x0 - 3.0, yl - 0.6, x1 + 3.0, yl + 0.6), RED, 0.9);
        let yw = y_of(lim.pin_warn_a);
        self.fill(rect(x0 - 3.0, yw - 0.4, x1 + 3.0, yw + 0.4), YELLOW, 0.55);
        let lbl = format!("{} A", de(lim.pin_limit_a, 1));
        let lw = self.text_width(&lbl, F::Label);
        if compact {
            // Die Titelzeile gehört den Klammern: Beschriftung bleibt an der
            // Linie, aber hinterlegt, damit Peaks sie nicht durchstreichen.
            let ly = yl.max(top + 12.0);
            self.fill_round(rect(x1 - lw - 3.0, ly - 13.0, x1 + 3.0, ly - 1.5), 2.0, PANEL, 0.9);
            self.text(&lbl, F::Label, rect(140.0, ly - 13.0, x1, ly - 1.0), Align::Right, RED, 1.0);
        } else {
            // Als Legende in die Titelzeile, frei von Balken und Peaks.
            self.fill(rect(x1 - lw - 11.0, 16.4, x1 - lw - 3.0, 17.6), RED, 0.9);
            self.text(&lbl, F::Label, rect(140.0, 9.0, x1 + 3.0, 25.0), Align::Right, RED, 1.0);
        }
    }

    fn cell(&self, col: usize, row: usize, label: &str, value: &str, vcol: u32) {
        let x = 212.0 + col as f32 * 111.0;
        let y = 10.0 + row as f32 * 24.0;
        if col == 1 {
            self.fill(rect(x, y + 3.0, x + 0.7, y + 21.0), FAINT, 0.6);
        }
        self.text(label, F::Label, rect(x + 9.0, y, x + 60.0, y + 24.0), Align::Left, DIM, 1.0);
        self.text(value, F::Value, rect(x + 30.0, y, x + 104.0, y + 24.0), Align::Right, vcol, 1.0);
    }

    fn draw_metrics(&self, lim: &Limits, snap: Option<&PsuSnapshot>, stats: Option<&[ConnectorStats; 2]>, vis: &[usize]) {
        let g = |i: usize| snap.and_then(|s| s.get(i));
        // Summen über die angezeigten Stecker
        let sel: Option<(f32, f32, f32, usize, usize, f32, u8)> = stats.map(|s| {
            let mut w = 0.0;
            let mut a = 0.0;
            let (mut mx, mut mxc, mut mxp, mut imb, mut imb_lvl) = (0.0f32, 0, 0, 0.0f32, 0u8);
            for &c in vis {
                w += s[c].watts;
                a += s[c].sum_a;
                if s[c].max_a >= mx {
                    mx = s[c].max_a;
                    mxc = c;
                    mxp = s[c].max_pin;
                }
                imb = imb.max(lim.imbalance_value(&s[c]));
                imb_lvl = imb_lvl.max(lim.imbalance_level(&s[c]));
            }
            (w, a, mx, mxc, mxp, imb, imb_lvl)
        });
        let present = stats.map(|s| vis.iter().any(|&c| s[c].present)).unwrap_or(false);
        let sel = sel.filter(|_| present);

        let watts = sel.map(|s| s.0);
        self.cell(0, 0, "Stecker", &opt(watts, 0, "W"), ACCENT);
        let load = watts.map(|w| w / (lim.connector_rated_w * vis.len() as f32) * 100.0);
        let load_col = match load {
            Some(l) if l >= 100.0 => RED,
            Some(l) if l >= 85.0 => YELLOW,
            _ => TEXT,
        };
        self.cell(1, 0, "Auslast.", &opt(load, 0, "%"), load_col);
        self.cell(0, 1, "Σ Strom", &opt(sel.map(|s| s.1), 1, "A"), TEXT);
        let (max_txt, max_col) = match sel {
            Some((_, _, mx, c, p, _, _)) => {
                let pin = if vis.len() == 2 { format!("{}.{}", c + 1, p + 1) } else { format!("P{}", p + 1) };
                (format!("{} A {}", de(mx, 1), pin), value_color(lim.pin_level(mx)))
            }
            None => ("–".into(), TEXT),
        };
        self.cell(1, 1, "Max", &max_txt, max_col);
        // Schieflast in der gewählten Messart; Farbe wie die Alarmbewertung
        // (bei Prozent erst ab Mindestlast).
        let imb_col = value_color(sel.map_or(0, |s| s.6));
        let (imb_label, imb_txt) = match lim.imbalance_mode {
            ImbalanceMode::Amps => ("Δ Pins", opt(sel.map(|s| s.5), 2, "A")),
            ImbalanceMode::Percent => ("Abw.", opt(sel.map(|s| s.5), 0, "%")),
        };
        self.cell(0, 2, imb_label, &imb_txt, imb_col);
        self.cell(1, 2, "GPU", &opt(g(idx::GPU_W), 0, "W"), TEXT);
        self.cell(0, 3, "PSU DC", &opt(g(idx::PSU_W), 0, "W"), TEXT);
        self.cell(1, 3, "Effizienz", &opt(g(idx::EFF), 1, "%"), TEXT);
        self.cell(0, 4, "Temp.", &opt(g(idx::TEMP), 0, "°C"), TEXT);
        self.cell(1, 4, "Lüfter", &opt(g(idx::FAN), 0, "rpm"), TEXT);

        // Rails
        self.fill(rect(220.0, 131.0, 426.0, 131.7), FAINT, 0.6);
        let rails = [("12 V", idx::V12, idx::A12), ("5 V", idx::V5, idx::A5), ("3,3 V", idx::V33, idx::A33)];
        for (i, (name, vi, ai)) in rails.iter().enumerate() {
            let x = 218.0 + i as f32 * 71.0;
            self.text(name, F::Label, rect(x, 132.0, x + 70.0, 144.0), Align::Center, DIM, 1.0);
            let txt = match (g(*vi), g(*ai)) {
                (None, None) => "–".to_string(),
                (v, a) => format!("{} · {}", opt(v, 2, "V"), opt(a, 1, "A")),
            };
            self.text(&txt, F::Label, rect(x - 2.0, 144.0, x + 72.0, 157.0), Align::Center, TEXT, 0.9);
        }
    }

    fn draw_status(&self, v: &View) {
        for (f, label, r) in visible_chips(v) {
            let st = v.monitor.fault(f);
            if !v.config.alarm(f).enabled {
                // Alarm abgeschaltet: nur blass, ohne Statuspunkt
                self.fill_round(r, 4.0, PANEL, 1.0);
                self.text(label, F::Chip, r, Align::Center, FAINT, 1.0);
            } else if st.active {
                self.fill_round(r, 4.0, RED, 1.0);
                self.text(label, F::Chip, r, Align::Center, 0xFFFFFF, 1.0);
            } else if st.latched {
                self.fill_round(r, 4.0, AMBER, 1.0);
                self.fill_round(rect(r.left + 1.2, r.top + 1.2, r.right - 1.2, r.bottom - 1.2), 3.0, PANEL, 1.0);
                self.text(label, F::Chip, r, Align::Center, AMBER, 1.0);
            } else {
                self.fill_round(r, 4.0, PANEL, 1.0);
                // Punkt und Text gemeinsam zentrieren
                let x = (r.left + r.right - (self.text_width(label, F::Chip) + 9.0)) / 2.0;
                self.fill_round(rect(x, r.top + 10.0, x + 4.0, r.top + 14.0), 2.0, GREEN, 0.8);
                self.text(label, F::Chip, rect(x + 9.0, r.top, r.right, r.bottom), Align::Left, DIM, 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> PsuSnapshot {
        let mut s = PsuSnapshot::default();
        for p in 0..PINS {
            s.vals[idx::PIN0 + p] = Some(6.0 + p as f32 * 0.1);
            s.vals[idx::PIN0 + PINS + p] = Some(0.0);
        }
        s.vals[idx::V12] = Some(12.0);
        s
    }

    #[test]
    fn hints() {
        let (cfg, mut mon) = (Config::default(), Monitor::default());
        let reading = Reading::Data(snapshot());
        mon.update(Some(&snapshot()), &cfg);
        let v = View { reading: &reading, monitor: &mon, config: &cfg, source_name: "Test" };

        // Pin 3 von Stecker 1: Wert und Peak, gleiche id im ganzen Balken
        let h = hint_at(88.0, 100.0, &v).unwrap();
        assert_eq!(h.id, 102);
        assert!(h.text.contains("Pin 3") && h.text.contains("6,20 A"));
        assert_eq!(hint_at(90.0, 150.0, &v).unwrap().id, 102);
        // Grenzlinien haben Vorrang vor dem Balken darunter
        assert_eq!(hint_at(88.0, amp_y(&cfg.limits, 9.2), &v).unwrap().id, 1);
        assert_eq!(hint_at(88.0, amp_y(&cfg.limits, 8.0), &v).unwrap().id, 2);
        // Kennzahlen und Rails
        assert!(hint_at(380.0, 46.0, &v).unwrap().text.contains("P3 = Pin 3"));
        assert!(hint_at(250.0, 150.0, &v).unwrap().text.starts_with("12-V"));
        // Statuszeile: ALARM S2 ist ausgeblendet, vier Felder
        assert!(hint_at(60.0, 178.0, &v).unwrap().text.ends_with("In Ordnung."));
        assert_eq!(hint_at(420.0, 178.0, &v).unwrap().id, 300 + Fault::NoData as u32);
        // zwischen den Panels und außerhalb: nichts
        assert!(hint_at(209.0, 100.0, &v).is_none());
        assert!(hint_at(-5.0, 100.0, &v).is_none());

        // ohne Daten nur die Statuszeile
        let off = Reading::Offline;
        let v = View { reading: &off, monitor: &mon, config: &cfg, source_name: "Test" };
        assert!(hint_at(88.0, 100.0, &v).is_none());
        assert!(hint_at(420.0, 178.0, &v).is_some());
    }
}
