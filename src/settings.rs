//! Fensterposition/-größe, "immer im Vordergrund", Grenzwerte und Alarme in
//! `%APPDATA%\GpuMon\gpumon.ini` (einfaches key=value).
//!
//! Grenzwerte: `pin_warn`, `pin_limit`, `full_scale` (A), `rated_w` (W),
//! Schieflast `imbalance_mode` (amps/percent), `imbalance` (A),
//! `imbalance_pct` (%), `imbalance_min_load` (A). Alarme: je Fehlerart `<key>_on`, `<key>_latch`,
//! `<key>_sound`, `<key>_show` (0/1), mit `<key>` aus `Fault::key`.
//! Anzeige: `show_<key>` (0/1) mit `<key>` aus `Metric::key`.
//! Sprache: `lang` = `auto` (nach Windows), `de` oder `en`; fehlt der
//! Schlüssel (ältere INI), gilt `auto`. Ereignis-Log `event_log` (Standard 1),
//! Infobereich `tray` (Standard 0). Autostart steht nicht hier, sondern
//! nur in der Registry.

use crate::i18n::LangPref;
use crate::model::{Config, ImbalanceMode, FAULTS, METRICS};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Fensterrechteck in physischen Bildschirmpixeln (x, y, Breite, Höhe).
    pub rect: Option<(i32, i32, i32, i32)>,
    pub topmost: bool,
    /// Gewählte Sprache der Oberfläche
    pub lang: LangPref,
    /// Neue Alarme in `events.csv` aufzeichnen (Standard an)
    pub event_log: bool,
    /// Symbol im Infobereich, Minimieren blendet aus der Taskleiste aus
    /// (Standard aus)
    pub tray: bool,
    pub config: Config,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { rect: None, topmost: false, lang: LangPref::Auto, event_log: true, tray: false, config: Config::default() }
    }
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("GpuMon").join("gpumon.ini"))
}

/// Zahl mit Punkt oder Komma.
pub fn parse_num(v: &str) -> Option<f32> {
    v.trim().replace(',', ".").parse().ok().filter(|f: &f32| f.is_finite())
}

impl Settings {
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        let (mut x, mut y, mut w, mut h) = (None, None, None, None);
        let mut lim = s.config.limits;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let num = |dst: &mut f32| {
                if let Some(n) = parse_num(v) {
                    *dst = n;
                }
            };
            match k {
                "x" => x = v.parse().ok(),
                "y" => y = v.parse().ok(),
                "w" => w = v.parse().ok(),
                "h" => h = v.parse().ok(),
                "topmost" => s.topmost = v == "1",
                "event_log" => s.event_log = v == "1",
                "tray" => s.tray = v == "1",
                "lang" => {
                    if let Some(l) = LangPref::from_key(v) {
                        s.lang = l;
                    }
                }
                "pin_warn" => num(&mut lim.pin_warn_a),
                "pin_limit" => num(&mut lim.pin_limit_a),
                "full_scale" => num(&mut lim.full_scale_a),
                "imbalance" => num(&mut lim.imbalance_a),
                "imbalance_mode" => {
                    if let Some(m) = ImbalanceMode::from_key(v) {
                        lim.imbalance_mode = m;
                    }
                }
                "imbalance_pct" => num(&mut lim.imbalance_pct),
                "imbalance_min_load" => num(&mut lim.imbalance_min_load_a),
                "rated_w" => num(&mut lim.connector_rated_w),
                _ if k.starts_with("show_") => {
                    if let Some(m) = METRICS.iter().find(|m| m.key() == &k[5..]) {
                        s.config.show[*m as usize] = v == "1";
                    }
                }
                _ => {
                    let Some((fk, attr)) = k.rsplit_once('_') else { continue };
                    let Some(f) = FAULTS.iter().find(|f| f.key() == fk) else { continue };
                    let a = &mut s.config.alarms[*f as usize];
                    let on = v == "1";
                    match attr {
                        "on" => a.enabled = on,
                        "latch" => a.latch = on,
                        "sound" => a.sound = on,
                        "show" => a.show = on,
                        _ => {}
                    }
                }
            }
        }
        // Ungültige (z. B. von Hand verbogene) Grenzwerte → Standard.
        if lim.validate().is_ok() {
            s.config.limits = lim;
        }
        if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
            if w > 50 && h > 30 {
                s.rect = Some((x, y, w, h));
            }
        }
        s
    }

    pub fn serialize(&self) -> String {
        let mut out = String::from("# GpuMon\n");
        if let Some((x, y, w, h)) = self.rect {
            out += &format!("x={x}\ny={y}\nw={w}\nh={h}\n");
        }
        out += &format!("topmost={}\n", self.topmost as u8);
        out += "# Infobereich: Symbol zeigen, Minimieren blendet aus der Taskleiste aus\n";
        out += &format!("tray={}\n", self.tray as u8);
        out += "# Neue Alarme in events.csv aufzeichnen\n";
        out += &format!("event_log={}\n", self.event_log as u8);
        out += "# Sprache: auto (nach Windows), de, en\n";
        out += &format!("lang={}\n", self.lang.key());
        let l = &self.config.limits;
        out += "\n# Grenzwerte (A bzw. W)\n";
        out += &format!(
            "pin_warn={}\npin_limit={}\nfull_scale={}\nrated_w={}\n",
            l.pin_warn_a, l.pin_limit_a, l.full_scale_a, l.connector_rated_w
        );
        out += "# Schieflast: amps = max − min in A (imbalance), percent = Abweichung vom Mittel in %\n";
        out += "# (imbalance_pct), bewertet erst ab imbalance_min_load A Steckerstrom\n";
        out += &format!(
            "imbalance_mode={}\nimbalance={}\nimbalance_pct={}\nimbalance_min_load={}\n",
            l.imbalance_mode.key(),
            l.imbalance_a,
            l.imbalance_pct,
            l.imbalance_min_load_a
        );
        out += "\n# Alarme: on = auswerten, latch = halten bis Klick, sound = Warnton, show = Fenster nach vorne\n";
        for f in FAULTS {
            let a = self.config.alarm(f);
            let k = f.key();
            out += &format!(
                "{k}_on={}\n{k}_latch={}\n{k}_sound={}\n{k}_show={}\n",
                a.enabled as u8, a.latch as u8, a.sound as u8, a.show as u8
            );
        }
        out += "\n# Anzeige: welche Kennzahlen rechts erscheinen (0/1)\n";
        for m in METRICS {
            out += &format!("show_{}={}\n", m.key(), self.config.shows(m) as u8);
        }
        out
    }

    pub fn load() -> Settings {
        path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, self.serialize());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AlarmCfg, Fault, ImbalanceMode, Limits, Metric};

    #[test]
    fn roundtrip() {
        let mut s = Settings { rect: Some((-1700, 120, 660, 294)), topmost: true, lang: LangPref::En, event_log: false, tray: true, config: Config::default() };
        s.config.show[Metric::History as usize] = true;
        s.config.show[Metric::MeanLine as usize] = true;
        s.config.show[Metric::WarnLine as usize] = false;
        s.config.limits = Limits {
            pin_warn_a: 7.5,
            pin_limit_a: 8.8,
            full_scale_a: 10.0,
            imbalance_mode: ImbalanceMode::Percent,
            imbalance_a: 4.25,
            imbalance_pct: 25.0,
            imbalance_min_load_a: 12.5,
            connector_rated_w: 450.0,
        };
        s.config.alarms[Fault::Imbalance as usize] = AlarmCfg { enabled: true, latch: false, sound: true, show: true };
        s.config.alarms[Fault::NoData as usize].enabled = false;
        s.config.show[Metric::Load as usize] = true;
        s.config.show[Metric::Fan as usize] = false;
        assert_eq!(Settings::parse(&s.serialize()), s);
        assert_eq!(Settings::parse("garbage\nx=1"), Settings::default());
        for l in [LangPref::Auto, LangPref::De, LangPref::En] {
            let s = Settings { lang: l, ..Settings::default() };
            assert_eq!(Settings::parse(&s.serialize()).lang, l);
        }
    }

    #[test]
    fn old_ini_and_bad_values() {
        // INI aus v1 ohne Grenzwerte → Standard
        let s = Settings::parse("x=1\ny=2\nw=600\nh=300\ntopmost=0\n");
        assert_eq!(s.config, Config::default());
        // neue Schlüssel fehlen: Log an, Tray aus, keine Verlaufskurve
        assert!(s.event_log && !s.tray && !s.config.shows(Metric::History));
        assert!(s.config.shows(Metric::WarnLine) && !s.config.shows(Metric::MeanLine));
        let s3 = Settings::parse("show_warn_line=0\nshow_mean_line=1");
        assert!(!s3.config.shows(Metric::WarnLine) && s3.config.shows(Metric::MeanLine));
        let s2 = Settings::parse("event_log=0\ntray=1\nshow_history=1");
        assert!(!s2.event_log && s2.tray && s2.config.shows(Metric::History));
        // ohne `lang` → automatisch, unbekannter Wert ebenso
        assert_eq!(s.lang, LangPref::Auto);
        assert_eq!(Settings::parse("lang=fr").lang, LangPref::Auto);
        assert_eq!(Settings::parse("lang = de").lang, LangPref::De);
        // Komma erlaubt
        assert_eq!(Settings::parse("pin_limit=9,5").config.limits.pin_limit_a, 9.5);
        // inkonsistent (gelb > rot) → Standard
        assert_eq!(Settings::parse("pin_warn=11\npin_limit=9").config.limits, Limits::default());
    }
}
