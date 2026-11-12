//! Datenmodell: Rohwerte (`PsuSnapshot`), abgeleitete Kennzahlen, Peaks und
//! gelatchte Fehler. Enthält keine Windows-Abhängigkeiten und ist vollständig
//! per Unit-Test prüfbar. Sichtbare Texte kommen aus `i18n`.

/// Konfigurierbare Grenzwerte. Die Standardwerte entsprechen den
/// RTSS-Overlays bzw. der 12V-2x6-Spezifikation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// Ab diesem Pin-Strom wird gelb gezeichnet.
    pub pin_warn_a: f32,
    /// Ab hier rot und Überstrom-Fehler (Spezifikation: 9,2 A pro Pin).
    pub pin_limit_a: f32,
    /// Vollausschlag der Pin-Balken.
    pub full_scale_a: f32,
    /// Wie Schieflast gemessen wird.
    pub imbalance_mode: ImbalanceMode,
    /// Schieflast-Grenze in A (max − min über die 6 Pins). Standard 3 A;
    /// das MSI-PSU-Plugin (`CurrentDif`, GPU Safeguard+) nutzt 5,5 A, was bei
    /// ~8,3 A/Pin unter Volllast erst sehr spät anschlägt.
    pub imbalance_a: f32,
    /// Schieflast-Grenze in % (größte Abweichung eines Pins vom Mittelwert).
    pub imbalance_pct: f32,
    /// Prozent-Messung erst ab diesem Steckerstrom (Σ der 6 Pins), weil bei
    /// wenig Last schon Messrauschen große Prozentwerte ergibt.
    pub imbalance_min_load_a: f32,
    /// Nennleistung eines Steckers (Bezug für die Auslastung).
    pub connector_rated_w: f32,
}

/// Messart der Schieflast.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ImbalanceMode {
    /// Absolut: größter − kleinster Pin in A (wie das MSI-Plugin).
    #[default]
    Amps,
    /// Relativ: größte Abweichung vom Mittelwert in %, mit Mindestlast
    /// (ähnlich WireView Pro II).
    Percent,
}

impl ImbalanceMode {
    pub fn key(self) -> &'static str {
        match self {
            ImbalanceMode::Amps => "amps",
            ImbalanceMode::Percent => "percent",
        }
    }
    pub fn from_key(k: &str) -> Option<Self> {
        match k {
            "amps" => Some(ImbalanceMode::Amps),
            "percent" => Some(ImbalanceMode::Percent),
            _ => None,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            // 600 W gleichmäßig verteilt sind 8,33 A je Pin: bis dahin grün,
            // gelb erst, wenn ein Pin deutlich mehr als seinen Anteil trägt.
            pin_warn_a: 8.5,
            pin_limit_a: 9.2,
            full_scale_a: 12.0,
            imbalance_mode: ImbalanceMode::Percent,
            imbalance_a: 3.0,
            imbalance_pct: 30.0,
            imbalance_min_load_a: 20.0,
            connector_rated_w: 600.0,
        }
    }
}

/// Feldindizes für `Limits::validate` (Reihenfolge im Einstellungsdialog).
pub mod field {
    pub const PIN_WARN: usize = 0;
    pub const PIN_LIMIT: usize = 1;
    pub const FULL_SCALE: usize = 2;
    pub const IMB_A: usize = 3;
    pub const IMB_PCT: usize = 4;
    pub const IMB_MIN_LOAD: usize = 5;
    pub const RATED_W: usize = 6;
    pub const COUNT: usize = 7;
}

impl Limits {
    /// Ampelfarbe eines Pin-Stroms: 0 = grün, 1 = gelb, 2 = rot.
    pub fn pin_level(&self, a: f32) -> u8 {
        if a >= self.pin_limit_a {
            2
        } else if a >= self.pin_warn_a {
            1
        } else {
            0
        }
    }

    /// Schieflast-Kennwert eines Steckers in der gewählten Messart
    /// (A bzw. %).
    pub fn imbalance_value(&self, st: &ConnectorStats) -> f32 {
        match self.imbalance_mode {
            ImbalanceMode::Amps => st.imbalance_a,
            ImbalanceMode::Percent => st.deviation_pct,
        }
    }

    /// Grenze in der gewählten Messart (A bzw. %).
    pub fn imbalance_limit(&self) -> f32 {
        match self.imbalance_mode {
            ImbalanceMode::Amps => self.imbalance_a,
            ImbalanceMode::Percent => self.imbalance_pct,
        }
    }

    /// Wird die Schieflast dieses Steckers überhaupt bewertet? (Stecker
    /// führt Strom; bei Prozent zusätzlich Mindestlast erreicht.)
    pub fn imbalance_armed(&self, st: &ConnectorStats) -> bool {
        st.active() && (self.imbalance_mode == ImbalanceMode::Amps || st.sum_a >= self.imbalance_min_load_a)
    }

    /// 0 = unauffällig (oder nicht bewertet), 1 = ab halber Grenze, 2 = Alarm.
    pub fn imbalance_level(&self, st: &ConnectorStats) -> u8 {
        if !self.imbalance_armed(st) {
            return 0;
        }
        let (v, l) = (self.imbalance_value(st), self.imbalance_limit());
        if v >= l {
            2
        } else if v >= l * 0.5 {
            1
        } else {
            0
        }
    }

    /// Prüft die Werte; bei einem Fehler: (Feldindex aus `field`, Meldung
    /// in der aktuellen Sprache).
    pub fn validate(&self) -> Result<(), (usize, &'static str)> {
        let ok = |v: f32, lo: f32, hi: f32| v.is_finite() && v >= lo && v <= hi;
        let t = crate::i18n::t();
        if !ok(self.pin_warn_a, 0.1, 50.0) {
            return Err((field::PIN_WARN, t.err_warn_range));
        }
        if !ok(self.pin_limit_a, 0.1, 50.0) {
            return Err((field::PIN_LIMIT, t.err_limit_range));
        }
        if self.pin_warn_a > self.pin_limit_a {
            return Err((field::PIN_WARN, t.err_warn_above));
        }
        if !ok(self.full_scale_a, 1.0, 50.0) {
            return Err((field::FULL_SCALE, t.err_full_scale));
        }
        if !ok(self.imbalance_a, 0.1, 50.0) {
            return Err((field::IMB_A, t.err_imb_a));
        }
        if !ok(self.imbalance_pct, 1.0, 500.0) {
            return Err((field::IMB_PCT, t.err_imb_pct));
        }
        if !ok(self.imbalance_min_load_a, 0.0, 300.0) {
            return Err((field::IMB_MIN_LOAD, t.err_min_load));
        }
        if !ok(self.connector_rated_w, 50.0, 2000.0) {
            return Err((field::RATED_W, t.err_rated_w));
        }
        Ok(())
    }
}

/// Verhalten eines Alarms (einer Fehlerart der Statuszeile).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlarmCfg {
    /// Wird überhaupt ausgewertet und angezeigt.
    pub enabled: bool,
    /// Bleibt markiert, bis er per Klick zurückgesetzt wird.
    pub latch: bool,
    /// Warnton, sobald der Alarm auftritt.
    pub sound: bool,
    /// Fenster wiederherstellen/nach vorne holen und in der Taskleiste blinken.
    pub show: bool,
}

impl Default for AlarmCfg {
    fn default() -> Self {
        AlarmCfg { enabled: true, latch: true, sound: false, show: false }
    }
}

/// Ein- und ausblendbare Anzeigen im Kennzahlen-Panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    ConnW = 0,
    SumA = 1,
    MaxPin = 2,
    Imbalance = 3,
    Load = 4,
    GpuW = 5,
    PsuW = 6,
    Eff = 7,
    Temp = 8,
    Fan = 9,
    /// Zeile mit den drei Spannungsschienen
    Rails = 10,
    /// Verlaufskurve der Steckerleistung statt der Gruppe „Netzteil“
    History = 11,
    /// Gestrichelte Mittelwert-Linie über den Pin-Balken
    MeanLine = 12,
    /// Gelbe Warnlinie in der Balkenfläche (die Gelbfärbung bleibt)
    WarnLine = 13,
}
pub const METRIC_COUNT: usize = 14;
pub const METRICS: [Metric; METRIC_COUNT] = [
    Metric::ConnW,
    Metric::SumA,
    Metric::MaxPin,
    Metric::Imbalance,
    Metric::Load,
    Metric::GpuW,
    Metric::PsuW,
    Metric::Eff,
    Metric::Temp,
    Metric::Fan,
    Metric::Rails,
    Metric::History,
    Metric::MeanLine,
    Metric::WarnLine,
];
/// Kennzahlen der Gruppe „Stecker“ bzw. „Netzteil“, in Anzeigereihenfolge.
pub const CONNECTOR_METRICS: [Metric; 6] = [Metric::ConnW, Metric::SumA, Metric::MaxPin, Metric::Imbalance, Metric::Load, Metric::GpuW];
pub const PSU_METRICS: [Metric; 4] = [Metric::PsuW, Metric::Eff, Metric::Temp, Metric::Fan];

impl Metric {
    /// Schlüssel in der INI-Datei (`show_<key>`).
    pub fn key(self) -> &'static str {
        ["conn_w", "sum_a", "max_pin", "imbalance", "load", "gpu_w", "psu_w", "eff", "temp", "fan", "rails", "history", "mean_line", "warn_line"][self as usize]
    }
    /// Beschriftung im Einstellungsdialog (aktuelle Sprache).
    pub fn title(self) -> &'static str {
        crate::i18n::t().metric_titles[self as usize]
    }
    /// Standard: alles außer Auslastung (hängt vom Power-Limit ab),
    /// GPU-Leistung (die Steckerleistung ist das Entscheidende) und der
    /// Verlaufskurve (verdrängt die Netzteil-Werte) und der Mittelwert-Linie.
    pub fn default_shown(self) -> bool {
        !matches!(self, Metric::Load | Metric::GpuW | Metric::History | Metric::MeanLine)
    }
}

/// Gesamte fachliche Konfiguration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub limits: Limits,
    pub alarms: [AlarmCfg; FAULT_COUNT],
    /// Sichtbarkeit je `Metric`.
    pub show: [bool; METRIC_COUNT],
}

impl Default for Config {
    fn default() -> Self {
        Config { limits: Limits::default(), alarms: FAULTS.map(Fault::default_alarm), show: METRICS.map(Metric::default_shown) }
    }
}

impl Config {
    pub fn alarm(&self, f: Fault) -> AlarmCfg {
        self.alarms[f as usize]
    }
    pub fn shows(&self, m: Metric) -> bool {
        self.show[m as usize]
    }
}

/// Ein Stecker gilt als belegt, wenn sein Summenstrom darüber liegt.
pub const CONNECTOR_ACTIVE_A: f32 = 0.5;

pub const PINS: usize = 6;
pub const CONNECTORS: usize = 2;

/// Index der Werte in `PsuSnapshot::vals`. Die Reihenfolge entspricht
/// `SOURCE_NAMES`.
pub mod idx {
    pub const PIN0: usize = 0; // 12 Einträge: Stecker 1 Pin 1–6, Stecker 2 Pin 1–6
    pub const ALARM0: usize = 12; // 2 Einträge
    pub const V12: usize = 14;
    pub const V5: usize = 15;
    pub const V33: usize = 16;
    pub const A12: usize = 17;
    pub const A5: usize = 18;
    pub const A33: usize = 19;
    pub const PSU_W: usize = 20;
    pub const EFF: usize = 21;
    pub const TEMP: usize = 22;
    pub const FAN: usize = 23;
    pub const GPU_W: usize = 24;
    pub const COUNT: usize = 25;
}

/// Exakte Quellennamen in Afterburners Shared Memory, in `idx`-Reihenfolge.
pub const SOURCE_NAMES: [&str; idx::COUNT] = [
    "PSU 12VHPWR1_1 current",
    "PSU 12VHPWR1_2 current",
    "PSU 12VHPWR1_3 current",
    "PSU 12VHPWR1_4 current",
    "PSU 12VHPWR1_5 current",
    "PSU 12VHPWR1_6 current",
    "PSU 12VHPWR2_1 current",
    "PSU 12VHPWR2_2 current",
    "PSU 12VHPWR2_3 current",
    "PSU 12VHPWR2_4 current",
    "PSU 12VHPWR2_5 current",
    "PSU 12VHPWR2_6 current",
    "PSU 12VHPWR1 alarm",
    "PSU 12VHPWR2 alarm",
    "PSU 12V voltage",
    "PSU 5V voltage",
    "PSU 3.3V voltage",
    "PSU 12V current",
    "PSU 5V current",
    "PSU 3.3V current",
    "PSU power",
    "PSU efficiency",
    "PSU temperature",
    "PSU fan speed",
    "Power", // GPU-Board-Power (nur mit Einheit "W" übernommen)
];

/// Ein Satz Rohwerte. Fehlende Quellen sind `None` (normal, je nachdem was in
/// Afterburner aktiviert ist).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PsuSnapshot {
    pub vals: [Option<f32>; idx::COUNT],
}

impl PsuSnapshot {
    pub fn get(&self, i: usize) -> Option<f32> {
        self.vals[i]
    }
    pub fn pin(&self, conn: usize, pin: usize) -> Option<f32> {
        self.vals[idx::PIN0 + conn * PINS + pin]
    }
    pub fn alarm(&self, conn: usize) -> Option<bool> {
        self.vals[idx::ALARM0 + conn].map(|v| v >= 0.5)
    }
    /// Liefert die Pinwerte als Array (fehlende = 0) und ob mindestens ein Pin
    /// vorhanden ist.
    pub fn pins(&self, conn: usize) -> ([f32; PINS], bool) {
        let mut out = [0.0; PINS];
        let mut any = false;
        for (p, o) in out.iter_mut().enumerate() {
            if let Some(v) = self.pin(conn, p) {
                *o = v;
                any = true;
            }
        }
        (out, any)
    }
    /// Hat der Snapshot überhaupt PSU-Werte (nicht nur GPU-Power)?
    pub fn has_psu(&self) -> bool {
        self.vals[..idx::GPU_W].iter().any(|v| v.is_some())
    }
}

/// Abgeleitete Werte eines Steckers.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ConnectorStats {
    pub pins: [f32; PINS],
    pub present: bool,
    pub sum_a: f32,
    pub watts: f32,
    pub max_a: f32,
    pub max_pin: usize,
    pub min_a: f32,
    /// max − min
    pub imbalance_a: f32,
    /// größte Abweichung vom Mittelwert in %
    pub deviation_pct: f32,
}

impl ConnectorStats {
    pub fn from_snapshot(s: &PsuSnapshot, conn: usize) -> Self {
        let (pins, present) = s.pins(conn);
        let volts = s.get(idx::V12).filter(|v| *v > 6.0).unwrap_or(12.0);
        let sum_a: f32 = pins.iter().sum();
        let (mut max_a, mut max_pin, mut min_a) = (f32::MIN, 0, f32::MAX);
        for (i, &v) in pins.iter().enumerate() {
            if v > max_a {
                max_a = v;
                max_pin = i;
            }
            min_a = min_a.min(v);
        }
        let mean = sum_a / PINS as f32;
        let deviation_pct = if mean > 0.5 {
            pins.iter().map(|v| (v - mean).abs()).fold(0.0, f32::max) / mean * 100.0
        } else {
            0.0
        };
        ConnectorStats {
            pins,
            present,
            sum_a,
            watts: sum_a * volts,
            max_a,
            max_pin,
            min_a,
            imbalance_a: max_a - min_a,
            deviation_pct,
        }
    }
    pub fn active(&self) -> bool {
        self.present && self.sum_a > CONNECTOR_ACTIVE_A
    }
}

/// Welche Stecker angezeigt werden: Stecker 1 immer (bzw. wenn nur Stecker 2
/// Strom führt, nur dieser), Stecker 2 zusätzlich nur, wenn er Strom führt.
pub fn visible_connectors(c: &[ConnectorStats; CONNECTORS]) -> ([usize; 2], usize) {
    match (c[0].active(), c[1].active()) {
        (_, false) => ([0, 0], 1),
        (false, true) => ([1, 0], 1),
        (true, true) => ([0, 1], 2),
    }
}

/// Kennwerte über die angezeigten Stecker (wie im Kennzahlen-Panel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    /// Steckerleistung in W (Σ der angezeigten Stecker)
    pub watts: f32,
    /// Höchster Pin-Strom, sein Stecker und Pin (0-basiert)
    pub max_a: f32,
    pub max_conn: usize,
    pub max_pin: usize,
    /// Zwei Stecker angezeigt
    pub both: bool,
}

/// Kennwerte der angezeigten Stecker; `None`, wenn keiner Pinwerte liefert.
pub fn summary(s: &PsuSnapshot) -> Option<Summary> {
    let stats = [ConnectorStats::from_snapshot(s, 0), ConnectorStats::from_snapshot(s, 1)];
    let (vis, n) = visible_connectors(&stats);
    let vis = &vis[..n];
    if !vis.iter().any(|&c| stats[c].present) {
        return None;
    }
    let mut out = Summary { watts: 0.0, max_a: 0.0, max_conn: 0, max_pin: 0, both: n == 2 };
    for &c in vis {
        out.watts += stats[c].watts;
        if stats[c].max_a >= out.max_a {
            out.max_a = stats[c].max_a;
            out.max_conn = c;
            out.max_pin = stats[c].max_pin;
        }
    }
    Some(out)
}

/// Länge der Verlaufskurve: ein Wert je Sekunde, zwei Minuten.
pub const HISTORY_LEN: usize = 120;
/// Lücke bis zu dieser Länge (s) wird mit dem neuen Wert aufgefüllt (Timer
/// verspätet), längere Lücken bleiben leer.
const HISTORY_FILL_S: u64 = 3;

/// Ringpuffer fester Größe mit der Steckerleistung je Sekunde. Fehlende
/// Daten sind `NaN` und werden als Lücke gezeichnet, nicht als 0.
#[derive(Clone, Debug)]
pub struct PowerHistory {
    vals: [f32; HISTORY_LEN],
    /// nächster Schreibplatz (= ältester Wert)
    head: usize,
    /// Sekunde des neuesten Werts
    last_s: Option<u64>,
}

impl Default for PowerHistory {
    fn default() -> Self {
        PowerHistory { vals: [f32::NAN; HISTORY_LEN], head: 0, last_s: None }
    }
}

impl PowerHistory {
    fn push(&mut self, v: f32) {
        self.vals[self.head] = v;
        self.head = (self.head + 1) % HISTORY_LEN;
    }

    /// Nimmt den Wert für die Sekunde `sec` (monotone Zeit) auf. Mehrere
    /// Werte in derselben Sekunde: der höchste zählt (Spitzen gehen nicht
    /// verloren). Liefert, ob sich die Kurve geändert hat.
    pub fn record(&mut self, sec: u64, w: Option<f32>) -> bool {
        let v = w.filter(|v| v.is_finite()).unwrap_or(f32::NAN);
        match self.last_s {
            // gleiche Sekunde (oder Uhr zurückgesprungen): neuesten Platz anpassen
            Some(l) if sec <= l => {
                let i = (self.head + HISTORY_LEN - 1) % HISTORY_LEN;
                let old = self.vals[i];
                let new = if old.is_nan() { v } else if v.is_nan() { old } else { old.max(v) };
                self.vals[i] = new;
                return new.to_bits() != old.to_bits();
            }
            Some(l) => {
                let gap = sec - l;
                let fill = if gap <= HISTORY_FILL_S { v } else { f32::NAN };
                for _ in 1..gap.min(HISTORY_LEN as u64) {
                    self.push(fill);
                }
                self.push(v);
            }
            None => self.push(v),
        }
        self.last_s = Some(sec);
        true
    }

    /// Werte vom ältesten zum neuesten (`None` = Lücke).
    pub fn iter(&self) -> impl Iterator<Item = Option<f32>> + '_ {
        (0..HISTORY_LEN).map(move |k| Some(self.vals[(self.head + k) % HISTORY_LEN]).filter(|v| !v.is_nan()))
    }

    /// Neuester Wert.
    pub fn latest(&self) -> Option<f32> {
        self.last_s?;
        Some(self.vals[(self.head + HISTORY_LEN - 1) % HISTORY_LEN]).filter(|v| !v.is_nan())
    }

    /// Höchster Wert im Fenster: (Position 0 = ältester, Wert); bei
    /// gleichen Werten der neueste.
    pub fn peak(&self) -> Option<(usize, f32)> {
        self.iter().enumerate().filter_map(|(i, v)| v.map(|v| (i, v))).fold(None, |best, (i, v)| match best {
            Some((_, b)) if b > v => best,
            _ => Some((i, v)),
        })
    }
}

/// Oberer Skalenwert der Verlaufskurve: nächste volle 100 W über dem
/// Maximum (mit etwas Luft für die Peak-Markierung), mindestens 100 W.
pub fn history_scale(max_w: f32) -> f32 {
    if !max_w.is_finite() || max_w <= 0.0 {
        return 100.0;
    }
    ((max_w * 1.03 / 100.0).ceil() * 100.0).max(100.0)
}

/// Fehlerarten der Statuszeile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Alarm1 = 0,
    Alarm2 = 1,
    OverCurrent = 2,
    Imbalance = 3,
    NoData = 4,
}
pub const FAULT_COUNT: usize = 5;
pub const FAULTS: [Fault; FAULT_COUNT] = [Fault::Alarm1, Fault::Alarm2, Fault::OverCurrent, Fault::Imbalance, Fault::NoData];

impl Fault {
    /// Schlüssel in der INI-Datei.
    pub fn key(self) -> &'static str {
        ["alarm1", "alarm2", "overcurrent", "imbalance", "nodata"][self as usize]
    }
    /// Beschriftung im Einstellungsdialog (aktuelle Sprache).
    pub fn title(self) -> &'static str {
        crate::i18n::t().fault_titles[self as usize]
    }
    /// Standardverhalten: alle aktiv und haltend. Überstrom und die Alarme
    /// des Netzteils melden sich zusätzlich mit Ton, weil man beim Spielen
    /// selten auf den zweiten Monitor schaut. Schieflast und Verbindung
    /// bleiben still (treten öfter auf und sind weniger dringend).
    pub fn default_alarm(self) -> AlarmCfg {
        let sound = matches!(self, Fault::Alarm1 | Fault::Alarm2 | Fault::OverCurrent);
        AlarmCfg { sound, ..AlarmCfg::default() }
    }
}

/// Lokale Uhrzeit. Kommt von außen (`main.rs` per `GetLocalTime`), damit das
/// Modell ohne Windows testbar bleibt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LocalTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl LocalTime {
    /// ISO 8601 ohne Zeitzone, z. B. `2026-11-12T14:32:05`.
    pub fn iso(&self) -> String {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", self.year, self.month, self.day, self.hour, self.minute, self.second)
    }
    /// Uhrzeit `14:32`.
    pub fn hhmm(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

/// Zeitpunkt einer Auswertung: monotone Zeit für Dauern, Uhrzeit für die
/// Anzeige.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Now {
    pub ms: u64,
    pub local: LocalTime,
}

/// Schlimmster Wert während eines Alarms.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Worst {
    /// Nur Zeit und Dauer (Netzteil-Alarme, Verbindung)
    #[default]
    None,
    /// Höchster Pin-Strom (Stecker und Pin 0-basiert)
    Pin { conn: usize, pin: usize, amps: f32 },
    /// Höchste Schieflast in der Messart, die beim Auftreten galt
    Imbalance { value: f32, mode: ImbalanceMode },
}

/// Gedächtnis eines Alarms: letzte Auslösung, Dauer, schlimmster Wert.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct AlarmMemo {
    /// Uhrzeit der letzten Auslösung
    pub at: LocalTime,
    pub start_ms: u64,
    /// Zuletzt aktiv gesehen bzw. Ende (monotone Zeit)
    pub end_ms: u64,
    pub worst: Worst,
}

impl AlarmMemo {
    /// Dauer in Sekunden; bei aktivem Alarm bis `now_ms`.
    pub fn duration_s(&self, active: bool, now_ms: u64) -> u64 {
        let end = if active { self.end_ms.max(now_ms) } else { self.end_ms };
        end.saturating_sub(self.start_ms) / 1000
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FaultState {
    pub active: bool,
    /// War seit dem letzten Zurücksetzen mindestens einmal aktiv.
    pub latched: bool,
}

/// Gesamtzustand für das Tray-Symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// grau: keine Daten
    NoData,
    /// grün
    Ok,
    /// gelb: Warnstufe erreicht oder Alarm gehalten
    Warn,
    /// rot: Alarm aktiv
    Alarm,
}

/// Peaks und gelatchte Fehler über die Laufzeit.
#[derive(Clone, Debug, Default)]
pub struct Monitor {
    pub peaks: [[f32; PINS]; CONNECTORS],
    /// Höchste Steckerleistung (W) seit Start/Reset, über die angezeigten Stecker
    pub power_peak: f32,
    pub faults: [FaultState; FAULT_COUNT],
    /// Stecker hat seit dem Start mindestens einmal Strom geführt.
    pub used: [bool; CONNECTORS],
    /// Einmal gültige Daten gesehen (ohne das wird "keine Daten" beim Start
    /// nicht gelatcht).
    seen_data: bool,
    /// Steckerleistung der letzten zwei Minuten
    pub history: PowerHistory,
    /// Gedächtnis je Alarm (seit dem letzten Zurücksetzen)
    pub memo: [Option<AlarmMemo>; FAULT_COUNT],
}

impl Monitor {
    /// Steckerleistung für die Verlaufskurve aufnehmen; `sec` ist eine
    /// monotone Zeit in Sekunden. Liefert, ob sich die Kurve geändert hat.
    pub fn record_power(&mut self, sec: u64, snap: Option<&PsuSnapshot>) -> bool {
        let w = snap.filter(|s| s.has_psu()).and_then(summary).map(|s| s.watts);
        self.history.record(sec, w)
    }

    /// Wie `update_at`, ohne Zeitbezug (nur für Tests).
    #[cfg(test)]
    pub fn update(&mut self, snap: Option<&PsuSnapshot>, cfg: &Config) -> [bool; FAULT_COUNT] {
        self.update_at(snap, cfg, Now::default())
    }

    /// Übernimmt einen neuen Zustand. `snap = None` heißt: keine PSU-Daten.
    /// Liefert die Alarme, die gerade neu aufgetreten sind (steigende Flanke).
    pub fn update_at(&mut self, snap: Option<&PsuSnapshot>, cfg: &Config, now: Now) -> [bool; FAULT_COUNT] {
        let lim = &cfg.limits;
        let mut active = [false; FAULT_COUNT];
        // Schlimmste Werte dieses Standes (für das Alarm-Gedächtnis)
        let mut worst_pin: Option<(usize, usize, f32)> = None;
        let mut worst_imb: Option<f32> = None;
        match snap {
            Some(s) if s.has_psu() => {
                self.seen_data = true;
                if let Some(sm) = summary(s) {
                    self.power_peak = self.power_peak.max(sm.watts);
                }
                for c in 0..CONNECTORS {
                    let st = ConnectorStats::from_snapshot(s, c);
                    self.used[c] |= st.active();
                    for p in 0..PINS {
                        self.peaks[c][p] = self.peaks[c][p].max(st.pins[p]);
                    }
                    if st.present && st.max_a >= lim.pin_limit_a {
                        active[Fault::OverCurrent as usize] = true;
                    }
                    if st.present && worst_pin.is_none_or(|w| st.max_a > w.2) {
                        worst_pin = Some((c, st.max_pin, st.max_a));
                    }
                    if lim.imbalance_level(&st) == 2 {
                        active[Fault::Imbalance as usize] = true;
                    }
                    if lim.imbalance_armed(&st) {
                        let v = lim.imbalance_value(&st);
                        worst_imb = Some(worst_imb.map_or(v, |w| w.max(v)));
                    }
                }
                active[Fault::Alarm1 as usize] = s.alarm(0).unwrap_or(false);
                active[Fault::Alarm2 as usize] = s.alarm(1).unwrap_or(false);
            }
            _ => active[Fault::NoData as usize] = true,
        }
        let mut raised = [false; FAULT_COUNT];
        let mut was = [false; FAULT_COUNT];
        for (i, (f, a)) in self.faults.iter_mut().zip(active).enumerate() {
            let ac = cfg.alarms[i];
            let a = a && ac.enabled;
            was[i] = f.active;
            raised[i] = a && !f.active;
            f.active = a;
            f.latched = if ac.enabled && ac.latch { f.latched || a } else { a };
        }
        if !self.seen_data {
            // Vor dem ersten Kontakt ist "keine Daten" nur aktiv, nicht gelatcht
            // und auch kein neuer Alarm.
            self.faults[Fault::NoData as usize].latched = false;
            raised[Fault::NoData as usize] = false;
        }

        // Alarm-Gedächtnis: bei steigender Flanke neu anlegen, solange aktiv
        // Ende und schlimmsten Wert fortschreiben, beim Abfallen das Ende
        // festhalten. Ohne Anzeige (weder aktiv noch gehalten) vergessen.
        for (i, f) in FAULTS.iter().enumerate() {
            let st = self.faults[i];
            if raised[i] {
                self.memo[i] = Some(AlarmMemo { at: now.local, start_ms: now.ms, end_ms: now.ms, worst: Worst::None });
            }
            let Some(m) = self.memo[i].as_mut() else { continue };
            if st.active {
                m.end_ms = now.ms;
                match f {
                    Fault::OverCurrent => {
                        if let Some((conn, pin, amps)) = worst_pin {
                            if !matches!(m.worst, Worst::Pin { amps: a, .. } if a >= amps) {
                                m.worst = Worst::Pin { conn, pin, amps };
                            }
                        }
                    }
                    Fault::Imbalance => {
                        if let Some(v) = worst_imb {
                            let mode = lim.imbalance_mode;
                            match &mut m.worst {
                                Worst::Imbalance { value, mode: m0 } if *m0 == mode => *value = value.max(v),
                                w => *w = Worst::Imbalance { value: v, mode },
                            }
                        }
                    }
                    _ => {}
                }
            } else if was[i] {
                m.end_ms = now.ms;
            }
            if !st.active && !st.latched {
                self.memo[i] = None;
            }
        }
        raised
    }

    /// Gehaltene Fehler zurücksetzen; das Gedächtnis inaktiver Alarme wird
    /// gelöscht, aktive behalten ihren Beginn.
    pub fn clear_faults(&mut self) {
        for (f, m) in self.faults.iter_mut().zip(&mut self.memo) {
            f.latched = f.active;
            if !f.active {
                *m = None;
            }
        }
    }

    /// Gesamtzustand (Farbe des Tray-Symbols). Vorrang: aktiver Alarm, dann
    /// keine Daten, dann gehaltener Alarm oder Warnstufe, sonst in Ordnung.
    pub fn status(&self, snap: Option<&PsuSnapshot>, cfg: &Config) -> Status {
        if self.faults.iter().zip(FAULTS).any(|(f, k)| f.active && k != Fault::NoData) {
            return Status::Alarm;
        }
        let Some(s) = snap.filter(|s| s.has_psu()) else { return Status::NoData };
        if self.faults.iter().any(|f| f.latched) {
            return Status::Warn;
        }
        let lim = &cfg.limits;
        let warn = (0..CONNECTORS).map(|c| ConnectorStats::from_snapshot(s, c)).any(|st| {
            st.present && (st.pins.iter().any(|a| lim.pin_level(*a) >= 1) || lim.imbalance_level(&st) >= 1)
        });
        if warn {
            Status::Warn
        } else {
            Status::Ok
        }
    }

    /// Gedächtnis eines Alarms, solange er aktiv oder gehalten ist.
    pub fn memo(&self, f: Fault) -> Option<AlarmMemo> {
        self.memo[f as usize]
    }

    /// Pin-Peaks und Peak der Steckerleistung zurücksetzen.
    pub fn clear_peaks(&mut self) {
        self.peaks = Default::default();
        self.power_peak = 0.0;
    }

    /// Statusfeld anzeigen? ALARM S2 nur, wenn Stecker 2 schon einmal belegt
    /// war oder der Alarm aktiv/gehalten ist (Stecker 2 ist oft unbelegt).
    pub fn show_fault(&self, f: Fault) -> bool {
        let st = self.fault(f);
        f != Fault::Alarm2 || self.used[1] || st.active || st.latched
    }

    pub fn fault(&self, f: Fault) -> FaultState {
        self.faults[f as usize]
    }

    #[allow(dead_code)] // in Tests genutzt
    pub fn any_latched_inactive(&self) -> bool {
        self.faults.iter().any(|f| f.latched && !f.active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(c1: [f32; 6], c2: [f32; 6]) -> PsuSnapshot {
        let mut s = PsuSnapshot::default();
        for p in 0..6 {
            s.vals[p] = Some(c1[p]);
            s.vals[6 + p] = Some(c2[p]);
        }
        s.vals[idx::V12] = Some(12.0);
        s.vals[idx::ALARM0] = Some(0.0);
        s.vals[idx::ALARM0 + 1] = Some(0.0);
        s
    }

    #[test]
    fn defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.limits.imbalance_mode, ImbalanceMode::Percent);
        for f in FAULTS {
            let a = cfg.alarm(f);
            assert!(a.enabled && a.latch && !a.show, "{f:?}");
            let loud = matches!(f, Fault::Alarm1 | Fault::Alarm2 | Fault::OverCurrent);
            assert_eq!(a.sound, loud, "{f:?}");
        }
    }

    #[test]
    fn stats() {
        let s = snap([7.5, 7.0, 8.0, 7.5, 7.5, 7.5], [0.0; 6]);
        let c = ConnectorStats::from_snapshot(&s, 0);
        assert!((c.sum_a - 45.0).abs() < 1e-4);
        assert!((c.watts - 540.0).abs() < 1e-3);
        assert_eq!(c.max_pin, 2);
        assert!((c.imbalance_a - 1.0).abs() < 1e-5);
        assert!((c.deviation_pct - 6.6667).abs() < 0.01);
        assert!(c.active());
        assert!(!ConnectorStats::from_snapshot(&s, 1).active());
    }

    #[test]
    fn visible() {
        let mk = |a: f32, b: f32| {
            let s = snap([a; 6], [b; 6]);
            [ConnectorStats::from_snapshot(&s, 0), ConnectorStats::from_snapshot(&s, 1)]
        };
        assert_eq!(visible_connectors(&mk(0.0, 0.0)), ([0, 0], 1));
        assert_eq!(visible_connectors(&mk(5.0, 0.0)), ([0, 0], 1));
        assert_eq!(visible_connectors(&mk(0.0, 5.0)), ([1, 0], 1));
        assert_eq!(visible_connectors(&mk(5.0, 5.0)), ([0, 1], 2));
    }

    #[test]
    fn levels() {
        assert_eq!(Limits::default().pin_level(8.33), 0); // 600 W gleichmäßig
        assert_eq!(Limits::default().pin_level(8.5), 1);
        assert_eq!(Limits::default().pin_level(9.19), 1);
        assert_eq!(Limits::default().pin_level(9.2), 2);
    }

    #[test]
    fn faults_latch_and_clear() {
        let mut m = Monitor::default();
        m.update(None, &Config::default()); // Start ohne Daten: aktiv, nicht gelatcht
        assert!(m.fault(Fault::NoData).active && !m.fault(Fault::NoData).latched);

        m.update(Some(&snap([9.5, 7.0, 7.0, 7.0, 7.0, 7.0], [0.0; 6])), &Config::default());
        assert!(m.fault(Fault::OverCurrent).active);
        assert!(!m.fault(Fault::NoData).active);
        assert!((m.peaks[0][0] - 9.5).abs() < 1e-6);

        m.update(Some(&snap([7.0; 6], [0.0; 6])), &Config::default());
        let oc = m.fault(Fault::OverCurrent);
        assert!(!oc.active && oc.latched);
        assert!(m.any_latched_inactive());

        m.update(None, &Config::default()); // Verbindung verloren -> gelatcht
        assert!(m.fault(Fault::NoData).latched);

        m.update(Some(&snap([7.0; 6], [0.0; 6])), &Config::default());
        m.clear_faults();
        assert!(!m.any_latched_inactive());
        assert!(!m.fault(Fault::OverCurrent).latched);
        assert!((m.peaks[0][0] - 9.5).abs() < 1e-6);
        m.clear_peaks();
        assert_eq!(m.peaks[0][0], 0.0);
    }

    #[test]
    fn imbalance_and_alarm() {
        let mut m = Monitor::default();
        let mut s = snap([9.0, 3.0, 6.0, 6.0, 6.0, 6.0], [0.0; 6]);
        s.vals[idx::ALARM0 + 1] = Some(1.0);
        m.update(Some(&s), &Config::default());
        assert!(m.fault(Fault::Imbalance).active);
        assert!(m.fault(Fault::Alarm2).active);
        assert!(!m.fault(Fault::Alarm1).active);
        assert!(!m.fault(Fault::OverCurrent).active);
    }

    #[test]
    fn configurable_limits_and_alarms() {
        let mut cfg = Config::default();
        cfg.limits.pin_limit_a = 10.0;
        let mut m = Monitor::default();
        let s = snap([9.5, 7.0, 7.0, 7.0, 7.0, 7.0], [0.0; 6]);
        assert!(!m.update(Some(&s), &cfg)[Fault::OverCurrent as usize]);
        assert!(!m.fault(Fault::OverCurrent).active);

        // steigende Flanke wird genau einmal gemeldet
        let hot = snap([10.5, 7.0, 7.0, 7.0, 7.0, 7.0], [0.0; 6]);
        assert!(m.update(Some(&hot), &cfg)[Fault::OverCurrent as usize]);
        assert!(!m.update(Some(&hot), &cfg)[Fault::OverCurrent as usize]);

        // ohne Halten: verschwindet mit dem Ereignis
        cfg.alarms[Fault::OverCurrent as usize].latch = false;
        m.update(Some(&s), &cfg);
        assert!(!m.fault(Fault::OverCurrent).latched);

        // deaktiviert: nie aktiv, gelatchter Zustand verschwindet
        cfg.alarms[Fault::OverCurrent as usize] = AlarmCfg { enabled: false, ..AlarmCfg::default() };
        let r = m.update(Some(&hot), &cfg);
        assert!(!r[Fault::OverCurrent as usize]);
        assert_eq!(m.fault(Fault::OverCurrent), FaultState::default());
    }

    #[test]
    fn alarm_s2_only_when_connector2_used() {
        let cfg = Config::default();
        let mut m = Monitor::default();
        m.update(Some(&snap([7.0; 6], [0.0; 6])), &cfg);
        assert!(!m.show_fault(Fault::Alarm2) && m.show_fault(Fault::Alarm1));
        // Alarm von Stecker 2 wird trotzdem angezeigt
        let mut s = snap([7.0; 6], [0.0; 6]);
        s.vals[idx::ALARM0 + 1] = Some(1.0);
        m.update(Some(&s), &cfg);
        assert!(m.show_fault(Fault::Alarm2));
        m.update(Some(&snap([7.0; 6], [0.0; 6])), &cfg);
        m.clear_faults();
        assert!(!m.show_fault(Fault::Alarm2));
        // einmal belegt -> bleibt sichtbar
        m.update(Some(&snap([7.0; 6], [5.0; 6])), &cfg);
        m.update(Some(&snap([7.0; 6], [0.0; 6])), &cfg);
        assert!(m.show_fault(Fault::Alarm2));
    }

    #[test]
    fn imbalance_percent_mode() {
        let mut cfg = Config::default();
        cfg.limits.imbalance_mode = ImbalanceMode::Percent;
        // Mittel 6 A, Pin 1 mit 8 A -> 33 % Abweichung, Δ nur 2,4 A
        let s = snap([8.0, 5.6, 5.6, 5.6, 5.6, 5.6], [0.0; 6]);
        let st = ConnectorStats::from_snapshot(&s, 0);
        assert!(st.sum_a >= 20.0 && st.deviation_pct > 30.0);
        let mut m = Monitor::default();
        m.update(Some(&s), &cfg);
        assert!(m.fault(Fault::Imbalance).active);
        // gleiche Lage im Ampere-Modus: kein Alarm
        let mut amps = Config::default();
        amps.limits.imbalance_mode = ImbalanceMode::Amps;
        let mut m2 = Monitor::default();
        m2.update(Some(&s), &amps);
        assert!(!m2.fault(Fault::Imbalance).active);
        // unter Mindestlast (Σ 9,6 A) wird nicht bewertet, obwohl ein Pin fehlt
        let low = snap([2.0, 0.0, 1.9, 1.9, 1.9, 1.9], [0.0; 6]);
        let mut m3 = Monitor::default();
        m3.update(Some(&low), &cfg);
        assert!(!m3.fault(Fault::Imbalance).active);
        assert_eq!(cfg.limits.imbalance_level(&ConnectorStats::from_snapshot(&low, 0)), 0);
        // mit Mindestlast 5 A schon
        cfg.limits.imbalance_min_load_a = 5.0;
        m3.update(Some(&low), &cfg);
        assert!(m3.fault(Fault::Imbalance).active);
    }

    #[test]
    fn history_ring_and_gaps() {
        let mut h = PowerHistory::default();
        assert_eq!(h.latest(), None);
        assert_eq!(h.peak(), None);
        assert!(h.iter().all(|v| v.is_none()));
        assert!(h.record(10, Some(300.0)));
        // gleiche Sekunde: der höhere Wert zählt, ein niedrigerer ändert nichts
        assert!(h.record(10, Some(350.0)));
        assert!(!h.record(10, Some(200.0)));
        assert!(!h.record(10, None));
        assert_eq!(h.latest(), Some(350.0));
        // kurze Lücke (Timer verspätet) wird aufgefüllt
        h.record(12, Some(400.0));
        let v: Vec<_> = h.iter().collect();
        assert_eq!(&v[HISTORY_LEN - 3..], &[Some(350.0), Some(400.0), Some(400.0)]);
        // keine Daten: Lücke, nicht 0
        h.record(13, None);
        assert_eq!(h.latest(), None);
        assert_eq!(h.iter().last(), Some(None));
        // lange Pause: dazwischen leer
        h.record(30, Some(100.0));
        let v: Vec<_> = h.iter().collect();
        assert_eq!(v[HISTORY_LEN - 1], Some(100.0));
        assert!(v[HISTORY_LEN - 17..HISTORY_LEN - 1].iter().all(|x| x.is_none()));
        assert_eq!(h.peak(), Some((HISTORY_LEN - 19, 400.0)));
        // nach zwei Minuten ist der alte Peak herausgefallen
        for s in 31..31 + HISTORY_LEN as u64 {
            h.record(s, Some(50.0));
        }
        assert_eq!(h.peak().map(|p| p.1), Some(50.0));
        assert_eq!(h.iter().count(), HISTORY_LEN);
        // riesiger Sprung: kein Überlauf, alles leer bis auf den neuen Wert
        h.record(1_000_000, Some(10.0));
        assert_eq!(h.iter().filter(|v| v.is_some()).count(), 1);
    }

    #[test]
    fn history_scale_rounds_up() {
        assert_eq!(history_scale(0.0), 100.0);
        assert_eq!(history_scale(f32::NAN), 100.0);
        assert_eq!(history_scale(40.0), 100.0);
        assert_eq!(history_scale(96.0), 100.0);
        assert_eq!(history_scale(98.0), 200.0);
        assert_eq!(history_scale(512.0), 600.0);
        assert_eq!(history_scale(590.0), 700.0);
        assert_eq!(history_scale(1150.0), 1200.0);
    }

    #[test]
    fn summary_and_record_power() {
        let s = snap([7.5, 7.0, 8.0, 7.5, 7.5, 7.5], [0.0; 6]);
        let sm = summary(&s).unwrap();
        assert!((sm.watts - 540.0).abs() < 1e-3);
        assert_eq!((sm.max_conn, sm.max_pin, sm.both), (0, 2, false));
        let both = summary(&snap([5.0; 6], [6.0, 6.0, 9.0, 6.0, 6.0, 6.0])).unwrap();
        assert!(both.both && both.max_conn == 1 && both.max_pin == 2);
        assert!((both.watts - (30.0 + 39.0) * 12.0).abs() < 1e-3);
        assert_eq!(summary(&PsuSnapshot::default()), None);
        // Peak der Steckerleistung: bleibt bis zum Zurücksetzen der Peaks
        let mut pm = Monitor::default();
        pm.update(Some(&s), &Config::default());
        pm.update(Some(&snap([5.0; 6], [0.0; 6])), &Config::default());
        assert!((pm.power_peak - 540.0).abs() < 1e-3);
        pm.update(None, &Config::default());
        pm.clear_faults();
        assert!((pm.power_peak - 540.0).abs() < 1e-3);
        pm.clear_peaks();
        assert_eq!(pm.power_peak, 0.0);
        pm.update(Some(&snap([5.0; 6], [0.0; 6])), &Config::default());
        assert!((pm.power_peak - 360.0).abs() < 1e-3);
        let mut m = Monitor::default();
        assert!(m.record_power(1, Some(&s)));
        assert!(m.record_power(2, None));
        assert_eq!(m.history.iter().filter(|v| v.is_some()).count(), 1);
    }

    fn at(ms: u64, hour: u8, minute: u8) -> Now {
        Now { ms, local: LocalTime { year: 2026, month: 11, day: 12, hour, minute, second: 0 } }
    }

    #[test]
    fn alarm_memo() {
        let mut cfg = Config::default();
        cfg.limits.imbalance_mode = ImbalanceMode::Amps;
        let mut m = Monitor::default();
        let ok = snap([7.0; 6], [0.0; 6]);
        m.update_at(Some(&ok), &cfg, at(0, 14, 30));
        assert!(m.memo.iter().all(|x| x.is_none()));
        // Überstrom ab 14:32, schlimmster Wert 9,8 A auf Pin 3
        m.update_at(Some(&snap([7.0, 7.0, 9.4, 7.0, 7.0, 7.0], [0.0; 6])), &cfg, at(10_000, 14, 32));
        m.update_at(Some(&snap([7.0, 7.0, 9.8, 7.0, 7.0, 7.0], [0.0; 6])), &cfg, at(11_000, 14, 32));
        m.update_at(Some(&snap([7.0, 7.0, 9.5, 7.0, 7.0, 7.0], [0.0; 6])), &cfg, at(12_000, 14, 32));
        let mo = m.memo(Fault::OverCurrent).unwrap();
        assert_eq!(mo.at.hhmm(), "14:32");
        assert_eq!(mo.worst, Worst::Pin { conn: 0, pin: 2, amps: 9.8 });
        assert_eq!(mo.duration_s(true, 13_500), 3); // läuft bei aktivem Alarm weiter
        // Ende: Dauer bleibt stehen, Gedächtnis bleibt (gehalten)
        m.update_at(Some(&ok), &cfg, at(14_000, 14, 32));
        let mo = m.memo(Fault::OverCurrent).unwrap();
        assert_eq!(mo.duration_s(false, 99_000), 4);
        m.update_at(Some(&ok), &cfg, at(20_000, 14, 33));
        assert_eq!(m.memo(Fault::OverCurrent).unwrap().end_ms, 14_000);
        // erneutes Auslösen ersetzt den alten Eintrag
        m.update_at(Some(&snap([9.3, 7.0, 7.0, 7.0, 7.0, 7.0], [0.0; 6])), &cfg, at(30_000, 14, 40));
        let mo = m.memo(Fault::OverCurrent).unwrap();
        assert_eq!((mo.at.hhmm(), mo.worst), ("14:40".to_string(), Worst::Pin { conn: 0, pin: 0, amps: 9.3 }));
        // Zurücksetzen: aktive behalten ihr Gedächtnis, inaktive verlieren es
        m.clear_faults();
        assert!(m.memo(Fault::OverCurrent).is_some());
        m.update_at(Some(&ok), &cfg, at(31_000, 14, 40));
        m.clear_faults();
        assert!(m.memo(Fault::OverCurrent).is_none());

        // Schieflast: höchster Wert in der gewählten Messart
        m.update_at(Some(&snap([8.0, 4.5, 6.0, 6.0, 6.0, 6.0], [0.0; 6])), &cfg, at(40_000, 15, 0));
        m.update_at(Some(&snap([8.5, 4.5, 6.0, 6.0, 6.0, 6.0], [0.0; 6])), &cfg, at(41_000, 15, 0));
        match m.memo(Fault::Imbalance).unwrap().worst {
            Worst::Imbalance { value, mode } => assert!((value - 4.0).abs() < 1e-4 && mode == ImbalanceMode::Amps),
            w => panic!("{w:?}"),
        }
        // Verbindung: nur Zeit und Dauer
        m.update_at(None, &cfg, at(50_000, 15, 1));
        let nd = m.memo(Fault::NoData).unwrap();
        assert_eq!((nd.worst, nd.start_ms), (Worst::None, 50_000));

        // ohne Halten verschwindet das Gedächtnis mit dem Alarm
        let mut cfg2 = cfg;
        cfg2.alarms[Fault::OverCurrent as usize].latch = false;
        let mut m2 = Monitor::default();
        m2.update_at(Some(&snap([9.5, 7.0, 7.0, 7.0, 7.0, 7.0], [0.0; 6])), &cfg2, at(0, 1, 0));
        assert!(m2.memo(Fault::OverCurrent).is_some());
        m2.update_at(Some(&ok), &cfg2, at(1000, 1, 0));
        assert!(m2.memo(Fault::OverCurrent).is_none());
        // Start ohne Daten: kein Gedächtnis für „keine Daten“
        let mut m3 = Monitor::default();
        m3.update_at(None, &cfg, at(0, 1, 0));
        assert!(m3.memo(Fault::NoData).is_none());
    }

    #[test]
    fn status_levels() {
        let cfg = Config::default();
        let mut m = Monitor::default();
        m.update(None, &cfg);
        assert_eq!(m.status(None, &cfg), Status::NoData);
        let ok = snap([7.0; 6], [0.0; 6]);
        m.update(Some(&ok), &cfg);
        assert_eq!(m.status(Some(&ok), &cfg), Status::Ok);
        // gelber Pin
        let warn = snap([8.5, 7.0, 7.0, 7.0, 7.0, 7.0], [0.0; 6]);
        m.update(Some(&warn), &cfg);
        assert_eq!(m.status(Some(&warn), &cfg), Status::Warn);
        // Überstrom aktiv, danach gehalten
        let hot = snap([9.5, 7.0, 7.0, 7.0, 7.0, 7.0], [0.0; 6]);
        m.update(Some(&hot), &cfg);
        assert_eq!(m.status(Some(&hot), &cfg), Status::Alarm);
        m.update(Some(&ok), &cfg);
        assert_eq!(m.status(Some(&ok), &cfg), Status::Warn);
        m.clear_faults();
        assert_eq!(m.status(Some(&ok), &cfg), Status::Ok);
        // Verbindung weg: grau, auch wenn der Alarm aktiv ist
        m.update(None, &cfg);
        assert_eq!(m.status(None, &cfg), Status::NoData);
    }

    #[test]
    fn local_time_format() {
        let t = LocalTime { year: 2026, month: 3, day: 7, hour: 9, minute: 5, second: 2 };
        assert_eq!(t.iso(), "2026-03-07T09:05:02");
        assert_eq!(t.hhmm(), "09:05");
    }

    #[test]
    fn validate_limits() {
        assert!(Limits::default().validate().is_ok());
        let l = Limits { pin_warn_a: 9.5, ..Limits::default() };
        assert_eq!(l.validate().unwrap_err().0, 0);
        let l = Limits { connector_rated_w: 10.0, ..Limits::default() };
        assert_eq!(l.validate().unwrap_err().0, field::RATED_W);
    }
}
