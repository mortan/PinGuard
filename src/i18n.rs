//! Lokalisierung: Deutsch und Englisch. Alle sichtbaren Texte der Oberfläche
//! stehen hier, feste Texte in einer `Texts`-Tabelle je Sprache, Texte mit
//! Platzhaltern als Funktionen. Keine Windows-Abhängigkeit: die
//! Systemsprache ermittelt `main.rs` und übergibt sie als LANGID.
//!
//! Die aktuelle Sprache ist threadlokal. Die App zeichnet und bedient alles
//! im UI-Thread, und Tests (je eigener Thread) können ihre Sprache setzen,
//! ohne sich gegenseitig zu stören.

use crate::model::{FAULT_COUNT, METRIC_COUNT};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

/// Vom Nutzer gewählte Sprache (INI `lang`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LangPref {
    /// Nach der Windows-Anzeigesprache
    #[default]
    Auto,
    De,
    En,
}

impl LangPref {
    pub fn key(self) -> &'static str {
        match self {
            LangPref::Auto => "auto",
            LangPref::De => "de",
            LangPref::En => "en",
        }
    }
    pub fn from_key(k: &str) -> Option<Self> {
        match k {
            "auto" => Some(LangPref::Auto),
            "de" => Some(LangPref::De),
            "en" => Some(LangPref::En),
            _ => None,
        }
    }
    /// Tatsächliche Sprache; `system` gilt bei `Auto`.
    pub fn resolve(self, system: Lang) -> Lang {
        match self {
            LangPref::Auto => system,
            LangPref::De => Lang::De,
            LangPref::En => Lang::En,
        }
    }
}

impl Lang {
    /// Aus einer Windows-LANGID: Deutsch bei primärer Sprache LANG_GERMAN
    /// (0x07, alle Regionen), sonst Englisch.
    pub fn from_langid(id: u16) -> Lang {
        if id & 0x3FF == 0x07 {
            Lang::De
        } else {
            Lang::En
        }
    }
}

thread_local! {
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::De) };
}

pub fn lang() -> Lang {
    CURRENT.with(|c| c.get())
}

pub fn set_lang(l: Lang) {
    CURRENT.with(|c| c.set(l));
}

/// Feste Texte der aktuellen Sprache.
pub fn t() -> &'static Texts {
    match lang() {
        Lang::De => &DE,
        Lang::En => &EN,
    }
}

/// Sprachnamen im Menü, immer in der eigenen Sprache.
pub const NAME_DE: &str = "Deutsch";
pub const NAME_EN: &str = "English";

// ---------------------------------------------------------------- Zahlen

fn localize(s: &str) -> String {
    match lang() {
        Lang::De => s.replace('.', ","),
        Lang::En => s.to_string(),
    }
}

/// Zahl mit fester Anzahl Nachkommastellen und dem Dezimalzeichen der Sprache.
pub fn fixed(v: f32, decimals: usize) -> String {
    localize(&format!("{:.*}", decimals, v))
}

/// Zahl ohne unnötige Nachkommastellen (höchstens 2), Dezimalzeichen der Sprache.
pub fn short(v: f32) -> String {
    let s = format!("{:.2}", v);
    localize(s.trim_end_matches('0').trim_end_matches('.'))
}

// ---------------------------------------------------------------- feste Texte

pub struct Texts {
    // Kontextmenü
    pub menu_reset_peaks: &'static str,
    pub menu_reset_faults: &'static str,
    pub menu_settings: &'static str,
    pub menu_topmost: &'static str,
    pub menu_language: &'static str,
    pub menu_lang_auto: &'static str,
    pub menu_exit: &'static str,

    // Hauptfenster
    /// "Stecker" in Panel-Titel und Klammern ("Stecker 1")
    pub connector: &'static str,
    /// Gruppenüberschriften im Kennzahlen-Panel
    pub head_conn: &'static str,
    pub head_psu: &'static str,
    /// Statusfelder in `Fault`-Reihenfolge
    pub chips: [&'static str; FAULT_COUNT],
    // Kennzahl-Labels (kurz: ca. 50 Basiseinheiten bei 10er-Schrift)
    pub lbl_power: &'static str,
    pub lbl_sum: &'static str,
    pub lbl_max: &'static str,
    pub lbl_delta: &'static str,
    pub lbl_dev: &'static str,
    pub lbl_load: &'static str,
    pub lbl_gpu: &'static str,
    pub lbl_eff: &'static str,
    pub lbl_temp: &'static str,
    pub lbl_fan: &'static str,
    // Hinweise bei fehlenden Daten
    pub waiting: &'static str,
    pub no_psu: &'static str,
    pub enable_plugin: &'static str,

    // Tooltips
    pub no_data: &'static str,
    pub st_disabled: &'static str,
    pub st_active: &'static str,
    pub st_latched: &'static str,
    pub st_ok: &'static str,
    pub tip_psu_w: &'static str,
    pub tip_eff: &'static str,
    pub tip_temp: &'static str,
    pub tip_fan: &'static str,

    // Einstellungsdialog
    pub dlg_title: &'static str,
    pub grp_limits: &'static str,
    pub grp_alarms: &'static str,
    pub grp_display: &'static str,
    /// Zeilen der Gruppe "Grenzwerte" (Reihenfolge wie `LIMIT_ROWS`)
    pub limit_rows: [&'static str; 8],
    /// Schieflast-Messart: Ampere, Prozent
    pub modes: [&'static str; 2],
    pub alarm_cols: [&'static str; 4],
    pub alarm_note: &'static str,
    pub disp_conn: &'static str,
    pub disp_psu: &'static str,
    pub load_note: &'static str,
    pub btn_defaults: &'static str,
    pub btn_cancel: &'static str,
    pub err_number: &'static str,
    /// Beschriftungen im Dialog je `Fault` bzw. `Metric`
    pub fault_titles: [&'static str; FAULT_COUNT],
    pub metric_titles: [&'static str; METRIC_COUNT],

    // Meldungen von `Limits::validate`
    pub err_warn_range: &'static str,
    pub err_limit_range: &'static str,
    pub err_warn_above: &'static str,
    pub err_full_scale: &'static str,
    pub err_imb_a: &'static str,
    pub err_imb_pct: &'static str,
    pub err_min_load: &'static str,
    pub err_rated_w: &'static str,
}

static DE: Texts = Texts {
    menu_reset_peaks: "Peaks zurücksetzen\tDoppelklick",
    menu_reset_faults: "Fehler zurücksetzen\tKlick auf Status",
    menu_settings: "Grenzen und Alarme …",
    menu_topmost: "Immer im Vordergrund",
    menu_language: "Sprache",
    menu_lang_auto: "Automatisch (Windows)",
    menu_exit: "Beenden",

    connector: "Stecker",
    head_conn: "STECKER",
    head_psu: "NETZTEIL",
    chips: ["ALARM S1", "ALARM S2", "ÜBERSTROM", "SCHIEFLAST", "VERBINDUNG"],
    lbl_power: "Leistung",
    lbl_sum: "Σ Strom",
    lbl_max: "Max",
    lbl_delta: "Δ Pins",
    lbl_dev: "Abw.",
    lbl_load: "Auslast.",
    lbl_gpu: "GPU",
    lbl_eff: "Effizienz",
    lbl_temp: "Temp.",
    lbl_fan: "Lüfter",
    waiting: "warte auf Daten …",
    no_psu: "Keine PSU-Daten",
    enable_plugin: "PSU-Plugin in Afterburner aktivieren",

    no_data: "keine Daten",
    st_disabled: "Alarm ist abgeschaltet.",
    st_active: "Jetzt aktiv.",
    st_latched: "War ausgelöst – Klick setzt zurück.",
    st_ok: "In Ordnung.",
    tip_psu_w: "Gesamte Ausgangsleistung des Netzteils (DC-Seite)",
    tip_eff: "Vom Netzteil gemeldeter Wirkungsgrad",
    tip_temp: "Temperatur im Netzteil",
    tip_fan: "Drehzahl des Netzteillüfters\n0 rpm: Lüfter steht",

    dlg_title: "GpuMon – Einstellungen",
    grp_limits: "Grenzwerte",
    grp_alarms: "Alarme",
    grp_display: "Anzeige",
    limit_rows: [
        "Pin gelb ab",
        "Pin rot / Überstrom ab",
        "Balken-Vollausschlag",
        "Schieflast messen als",
        "Schieflast (max − min) ab",
        "Schieflast (Abw. vom Mittel) ab",
        "  … erst ab Steckerstrom (Σ)",
        "Nennleistung je Stecker",
    ],
    modes: ["Ampere", "Prozent"],
    alarm_cols: ["Aktiv", "Halten", "Ton", "Zeigen"],
    alarm_note: "Halten: bleibt orange markiert, bis die Statuszeile angeklickt wird.\nZeigen: Fenster wiederherstellen, nach vorne holen und blinken.",
    disp_conn: "Stecker",
    disp_psu: "Netzteil",
    load_note: "Auslastung bezieht sich auf die Nennleistung je Stecker.",
    btn_defaults: "Standardwerte",
    btn_cancel: "Abbrechen",
    err_number: "Bitte eine Zahl eingeben.",
    fault_titles: ["Netzteil-Alarm Stecker 1", "Netzteil-Alarm Stecker 2", "Überstrom (Pin ≥ Rot)", "Schieflast", "Verbindung verloren"],
    metric_titles: [
        "Leistung",
        "Σ Strom",
        "Höchster Pin",
        "Schieflast",
        "Auslastung",
        "GPU-Leistung",
        "Leistung (DC)",
        "Effizienz",
        "Temperatur",
        "Lüfter",
        "Spannungsschienen",
    ],

    err_warn_range: "Gelb-Grenze muss zwischen 0,1 und 50 A liegen.",
    err_limit_range: "Rot-Grenze muss zwischen 0,1 und 50 A liegen.",
    err_warn_above: "Gelb-Grenze darf nicht über der Rot-Grenze liegen.",
    err_full_scale: "Vollausschlag muss zwischen 1 und 50 A liegen.",
    err_imb_a: "Schieflast-Grenze muss zwischen 0,1 und 50 A liegen.",
    err_imb_pct: "Schieflast-Grenze muss zwischen 1 und 500 % liegen.",
    err_min_load: "Mindestlast muss zwischen 0 und 300 A liegen.",
    err_rated_w: "Nennleistung muss zwischen 50 und 2000 W liegen.",
};

static EN: Texts = Texts {
    menu_reset_peaks: "Reset peaks\tDouble-click",
    menu_reset_faults: "Reset faults\tClick on status",
    menu_settings: "Limits and alarms …",
    menu_topmost: "Always on top",
    menu_language: "Language",
    menu_lang_auto: "Automatic (Windows)",
    menu_exit: "Exit",

    connector: "Connector",
    head_conn: "CONNECTOR",
    head_psu: "PSU",
    chips: ["ALARM C1", "ALARM C2", "OVERCURR.", "IMBALANCE", "LINK"],
    lbl_power: "Power",
    lbl_sum: "Σ Amps",
    lbl_max: "Max",
    lbl_delta: "Δ Pins",
    lbl_dev: "Dev.",
    lbl_load: "Load",
    lbl_gpu: "GPU",
    lbl_eff: "Eff.",
    lbl_temp: "Temp.",
    lbl_fan: "Fan",
    waiting: "waiting for data …",
    no_psu: "No PSU data",
    enable_plugin: "Enable PSU plugin in Afterburner",

    no_data: "no data",
    st_disabled: "Alarm is disabled.",
    st_active: "Active now.",
    st_latched: "Was triggered – click to reset.",
    st_ok: "OK.",
    tip_psu_w: "Total PSU output power (DC side)",
    tip_eff: "Efficiency reported by the PSU",
    tip_temp: "Temperature inside the PSU",
    tip_fan: "PSU fan speed\n0 rpm: fan stopped",

    dlg_title: "GpuMon – Settings",
    grp_limits: "Limits",
    grp_alarms: "Alarms",
    grp_display: "Display",
    limit_rows: [
        "Pin yellow at",
        "Pin red / overcurrent at",
        "Bar full scale",
        "Measure imbalance as",
        "Imbalance (max − min) at",
        "Imbalance (dev. from mean) at",
        "  … only from connector Σ",
        "Rated power per connector",
    ],
    modes: ["Amps", "Percent"],
    alarm_cols: ["On", "Latch", "Sound", "Show"],
    alarm_note: "Latch: stays marked orange until the status bar is clicked.\nShow: restore the window, bring it to the front and flash.",
    disp_conn: "Connector",
    disp_psu: "PSU",
    load_note: "Load refers to the rated power per connector.",
    btn_defaults: "Defaults",
    btn_cancel: "Cancel",
    err_number: "Please enter a number.",
    fault_titles: ["PSU alarm connector 1", "PSU alarm connector 2", "Overcurrent (pin ≥ red)", "Imbalance", "Connection lost"],
    metric_titles: [
        "Power",
        "Σ Current",
        "Highest pin",
        "Imbalance",
        "Load",
        "GPU power",
        "Power (DC)",
        "Efficiency",
        "Temperature",
        "Fan",
        "Voltage rails",
    ],

    err_warn_range: "Yellow limit must be between 0.1 and 50 A.",
    err_limit_range: "Red limit must be between 0.1 and 50 A.",
    err_warn_above: "Yellow limit must not be above the red limit.",
    err_full_scale: "Full scale must be between 1 and 50 A.",
    err_imb_a: "Imbalance limit must be between 0.1 and 50 A.",
    err_imb_pct: "Imbalance limit must be between 1 and 500 %.",
    err_min_load: "Minimum load must be between 0 and 300 A.",
    err_rated_w: "Rated power must be between 50 and 2000 W.",
};

// ---------------------------------------------------------------- Texte mit Platzhaltern
// Zahlen kommen bereits formatiert (`short`/`fixed`) herein.

fn pick(de: String, en: String) -> String {
    match lang() {
        Lang::De => de,
        Lang::En => en,
    }
}

/// Hinweis, wenn die Datenquelle nicht läuft.
pub fn not_running(source: &str) -> String {
    pick(format!("{source} läuft nicht"), format!("{source} not running"))
}

/// Statuszeile: Netzteil-Alarm von Stecker `n` (1-basiert).
pub fn fault_alarm(n: usize) -> String {
    pick(format!("Netzteil-Alarm Stecker {n}: vom Netzteil selbst gemeldet"), format!("PSU alarm connector {n}: reported by the PSU itself"))
}

pub fn fault_overcurrent(a: &str) -> String {
    pick(format!("Überstrom: ein Pin führt {a} A oder mehr"), format!("Overcurrent: a pin carries {a} A or more"))
}

pub fn fault_imbalance_amps(a: &str) -> String {
    pick(
        format!("Schieflast: größter minus kleinster Pin-Strom ab {a} A"),
        format!("Imbalance: highest minus lowest pin current at {a} A or more"),
    )
}

pub fn fault_imbalance_pct(p: &str) -> String {
    pick(format!("Schieflast: ein Pin weicht {p} % oder mehr vom Mittel ab"), format!("Imbalance: a pin deviates {p} % or more from the mean"))
}

pub fn fault_nodata(source: &str) -> String {
    pick(format!("Verbindung: keine Netzteil-Daten von {source}"), format!("Link: no PSU data from {source}"))
}

/// Rote Linie im Balken-Panel.
pub fn tip_pin_limit(a: &str) -> String {
    pick(
        format!("Überstrom-Grenze: {a} A je Pin\nAb hier wird der Pin rot und ÜBERSTROM löst aus."),
        format!("Overcurrent limit: {a} A per pin\nAt this level the pin turns red and the overcurrent alarm triggers."),
    )
}

/// Gelbe Linie im Balken-Panel.
pub fn tip_pin_warn(a: &str) -> String {
    pick(format!("Warnschwelle: {a} A je Pin\nAb hier wird der Pin gelb."), format!("Warning threshold: {a} A per pin\nAt this level the pin turns yellow."))
}

/// Ein Pin-Balken; `now` ist der fertige Wert ("6,20 A" oder "keine Daten").
pub fn tip_pin(conn: usize, pin: usize, now: &str, peak: &str) -> String {
    pick(
        format!("Stecker {conn} · Pin {pin}\nAktuell: {now}\nPeak: {peak} A (Doppelklick setzt zurück)"),
        format!("Connector {conn} · Pin {pin}\nNow: {now}\nPeak: {peak} A (double-click resets)"),
    )
}

fn scope(both: bool) -> &'static str {
    match (lang(), both) {
        (_, false) => "",
        (Lang::De, true) => " beider Stecker",
        (Lang::En, true) => " of both connectors",
    }
}

pub fn tip_conn_w(both: bool) -> String {
    let s = scope(both);
    pick(
        format!("Leistung über den 12V-2x6-Stecker\nSumme der Pin-Ströme{s} × 12-V-Spannung"),
        format!("Power through the 12V-2x6 connector\nSum of pin currents{s} × 12 V voltage"),
    )
}

pub fn tip_load(rated_w: &str) -> String {
    pick(
        format!("Auslastung: Steckerleistung bezogen auf {rated_w} W Nennleistung je Stecker\nGelb ab 85 %, rot ab 100 %"),
        format!("Load: connector power relative to {rated_w} W rated power per connector\nYellow at 85 %, red at 100 %"),
    )
}

pub fn tip_sum(both: bool) -> String {
    let s = scope(both);
    pick(format!("Summe der Pin-Ströme{s}"), format!("Sum of pin currents{s}"))
}

pub fn tip_max(both: bool, warn: &str, limit: &str) -> String {
    let (ex_de, ex_en) = if both { ("1.3 = Stecker 1, Pin 3", "1.3 = connector 1, pin 3") } else { ("P3 = Pin 3", "P3 = pin 3") };
    pick(
        format!("Höchster Pin-Strom im Moment\n{ex_de}\nGelb ab {warn} A, rot ab {limit} A"),
        format!("Highest pin current right now\n{ex_en}\nYellow at {warn} A, red at {limit} A"),
    )
}

fn worst(both: bool) -> &'static str {
    match (lang(), both) {
        (_, false) => "",
        (Lang::De, true) => "\nAngezeigt wird der höhere Wert beider Stecker.",
        (Lang::En, true) => "\nShows the higher value of both connectors.",
    }
}

pub fn tip_imbalance_amps(both: bool, yellow: &str, alarm: &str) -> String {
    let w = worst(both);
    pick(
        format!("Schieflast: größter minus kleinster Pin-Strom\nGelb ab {yellow} A, Alarm ab {alarm} A{w}"),
        format!("Imbalance: highest minus lowest pin current\nYellow at {yellow} A, alarm at {alarm} A{w}"),
    )
}

pub fn tip_imbalance_pct(both: bool, yellow: &str, alarm: &str, min_load: &str) -> String {
    let w = worst(both);
    pick(
        format!("Schieflast: größte Abweichung eines Pins vom Mittelwert\nGelb ab {yellow} %, Alarm ab {alarm} %, bewertet ab {min_load} A Steckerstrom{w}"),
        format!("Imbalance: largest deviation of a pin from the mean\nYellow at {yellow} %, alarm at {alarm} %, evaluated from {min_load} A connector current{w}"),
    )
}

pub fn tip_gpu(source: &str) -> String {
    pick(
        format!("Leistungsaufnahme der Grafikkarte\n(Sensor „Power“ aus {source})"),
        format!("Power draw of the graphics card\n(sensor “Power” from {source})"),
    )
}

/// Spannungsschiene; `volts` ist die Nennspannung ohne Einheit ("3,3").
pub fn tip_rail(volts: &str) -> String {
    pick(format!("{volts}-V-Schiene des Netzteils: Spannung · Strom"), format!("{volts} V rail of the PSU: voltage · current"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_follow_language() {
        set_lang(Lang::De);
        assert_eq!(fixed(6.2, 2), "6,20");
        assert_eq!(short(9.2), "9,2");
        assert_eq!(short(12.0), "12");
        assert_eq!(short(0.25), "0,25");
        set_lang(Lang::En);
        assert_eq!(fixed(6.2, 2), "6.20");
        assert_eq!(short(9.2), "9.2");
        assert_eq!(short(600.0), "600");
        assert_eq!(fixed(3.3, 0), "3");
    }

    #[test]
    fn pref_and_langid() {
        assert_eq!(Lang::from_langid(0x0407), Lang::De); // de-DE
        assert_eq!(Lang::from_langid(0x0C07), Lang::De); // de-AT
        assert_eq!(Lang::from_langid(0x0807), Lang::De); // de-CH
        assert_eq!(Lang::from_langid(0x0409), Lang::En); // en-US
        assert_eq!(Lang::from_langid(0x040C), Lang::En); // fr-FR
        assert_eq!(LangPref::Auto.resolve(Lang::De), Lang::De);
        assert_eq!(LangPref::Auto.resolve(Lang::En), Lang::En);
        assert_eq!(LangPref::En.resolve(Lang::De), Lang::En);
        assert_eq!(LangPref::De.resolve(Lang::En), Lang::De);
        for p in [LangPref::Auto, LangPref::De, LangPref::En] {
            assert_eq!(LangPref::from_key(p.key()), Some(p));
        }
        assert_eq!(LangPref::from_key("fr"), None);
    }

    #[test]
    fn texts_switch() {
        set_lang(Lang::De);
        assert_eq!(t().chips[2], "ÜBERSTROM");
        assert_eq!(not_running("Afterburner"), "Afterburner läuft nicht");
        set_lang(Lang::En);
        assert_eq!(t().chips[2], "OVERCURR.");
        assert_eq!(not_running("Afterburner"), "Afterburner not running");
        // Englische Tabelle ohne Umlaute/ß (Hinweis auf vergessene Übersetzung)
        let all = [EN.chips.as_slice(), EN.limit_rows.as_slice(), EN.fault_titles.as_slice(), EN.metric_titles.as_slice()].concat();
        assert!(all.iter().all(|s| !s.contains(['ä', 'ö', 'ü', 'Ä', 'Ö', 'Ü', 'ß'])));
    }
}
