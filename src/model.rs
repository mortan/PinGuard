//! Datenmodell: Rohwerte (`PsuSnapshot`), abgeleitete Kennzahlen, Peaks und
//! gelatchte Fehler. Enthält keine Windows-Abhängigkeiten und ist vollständig
//! per Unit-Test prüfbar.

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
            pin_warn_a: 8.0,
            pin_limit_a: 9.2,
            full_scale_a: 12.0,
            imbalance_mode: ImbalanceMode::Amps,
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

    /// Prüft die Werte; bei einem Fehler: (Feldindex aus `field`, Meldung).
    pub fn validate(&self) -> Result<(), (usize, &'static str)> {
        let ok = |v: f32, lo: f32, hi: f32| v.is_finite() && v >= lo && v <= hi;
        if !ok(self.pin_warn_a, 0.1, 50.0) {
            return Err((field::PIN_WARN, "Gelb-Grenze muss zwischen 0,1 und 50 A liegen."));
        }
        if !ok(self.pin_limit_a, 0.1, 50.0) {
            return Err((field::PIN_LIMIT, "Rot-Grenze muss zwischen 0,1 und 50 A liegen."));
        }
        if self.pin_warn_a > self.pin_limit_a {
            return Err((field::PIN_WARN, "Gelb-Grenze darf nicht über der Rot-Grenze liegen."));
        }
        if !ok(self.full_scale_a, 1.0, 50.0) {
            return Err((field::FULL_SCALE, "Vollausschlag muss zwischen 1 und 50 A liegen."));
        }
        if !ok(self.imbalance_a, 0.1, 50.0) {
            return Err((field::IMB_A, "Schieflast-Grenze muss zwischen 0,1 und 50 A liegen."));
        }
        if !ok(self.imbalance_pct, 1.0, 500.0) {
            return Err((field::IMB_PCT, "Schieflast-Grenze muss zwischen 1 und 500 % liegen."));
        }
        if !ok(self.imbalance_min_load_a, 0.0, 300.0) {
            return Err((field::IMB_MIN_LOAD, "Mindestlast muss zwischen 0 und 300 A liegen."));
        }
        if !ok(self.connector_rated_w, 50.0, 2000.0) {
            return Err((field::RATED_W, "Nennleistung muss zwischen 50 und 2000 W liegen."));
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

/// Gesamte fachliche Konfiguration.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Config {
    pub limits: Limits,
    pub alarms: [AlarmCfg; FAULT_COUNT],
}

impl Config {
    pub fn alarm(&self, f: Fault) -> AlarmCfg {
        self.alarms[f as usize]
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
    /// Beschriftung im Einstellungsdialog.
    pub fn title(self) -> &'static str {
        ["Netzteil-Alarm Stecker 1", "Netzteil-Alarm Stecker 2", "Überstrom (Pin ≥ Rot)", "Schieflast", "Verbindung verloren"][self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FaultState {
    pub active: bool,
    /// War seit dem letzten Zurücksetzen mindestens einmal aktiv.
    pub latched: bool,
}

/// Peaks und gelatchte Fehler über die Laufzeit.
#[derive(Clone, Debug, Default)]
pub struct Monitor {
    pub peaks: [[f32; PINS]; CONNECTORS],
    pub faults: [FaultState; FAULT_COUNT],
    /// Stecker hat seit dem Start mindestens einmal Strom geführt.
    pub used: [bool; CONNECTORS],
    /// Einmal gültige Daten gesehen (ohne das wird "keine Daten" beim Start
    /// nicht gelatcht).
    seen_data: bool,
}

impl Monitor {
    /// Übernimmt einen neuen Zustand. `snap = None` heißt: keine PSU-Daten.
    /// Liefert die Alarme, die gerade neu aufgetreten sind (steigende Flanke).
    pub fn update(&mut self, snap: Option<&PsuSnapshot>, cfg: &Config) -> [bool; FAULT_COUNT] {
        let lim = &cfg.limits;
        let mut active = [false; FAULT_COUNT];
        match snap {
            Some(s) if s.has_psu() => {
                self.seen_data = true;
                for c in 0..CONNECTORS {
                    let st = ConnectorStats::from_snapshot(s, c);
                    self.used[c] |= st.active();
                    for p in 0..PINS {
                        self.peaks[c][p] = self.peaks[c][p].max(st.pins[p]);
                    }
                    if st.present && st.max_a >= lim.pin_limit_a {
                        active[Fault::OverCurrent as usize] = true;
                    }
                    if lim.imbalance_level(&st) == 2 {
                        active[Fault::Imbalance as usize] = true;
                    }
                }
                active[Fault::Alarm1 as usize] = s.alarm(0).unwrap_or(false);
                active[Fault::Alarm2 as usize] = s.alarm(1).unwrap_or(false);
            }
            _ => active[Fault::NoData as usize] = true,
        }
        let mut raised = [false; FAULT_COUNT];
        for (i, (f, a)) in self.faults.iter_mut().zip(active).enumerate() {
            let ac = cfg.alarms[i];
            let a = a && ac.enabled;
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
        raised
    }

    pub fn clear_faults(&mut self) {
        for f in &mut self.faults {
            f.latched = f.active;
        }
    }

    pub fn clear_peaks(&mut self) {
        self.peaks = Default::default();
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
        assert_eq!(Limits::default().pin_level(7.99), 0);
        assert_eq!(Limits::default().pin_level(8.0), 1);
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
        let mut m2 = Monitor::default();
        m2.update(Some(&s), &Config::default());
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
    fn validate_limits() {
        assert!(Limits::default().validate().is_ok());
        let l = Limits { pin_warn_a: 9.5, ..Limits::default() };
        assert_eq!(l.validate().unwrap_err().0, 0);
        let l = Limits { connector_rated_w: 10.0, ..Limits::default() };
        assert_eq!(l.validate().unwrap_err().0, field::RATED_W);
    }
}
