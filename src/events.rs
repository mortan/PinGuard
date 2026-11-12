//! Ereignis-Log: bei jedem neu auftretenden Alarm eine Zeile in
//! `%APPDATA%\GpuMon\events.csv`. Trennzeichen Semikolon, Dezimalpunkt,
//! unabhängig von der Sprache der Oberfläche, damit die Datei
//! maschinenlesbar bleibt. Schreibfehler werden still ignoriert.
//!
//! Größe: Überschreitet die Datei `MAX_BYTES`, wird sie einmal in
//! `events.old.csv` umbenannt (eine ältere wird dabei ersetzt) und neu
//! begonnen. Zusammen belegen beide höchstens gut 2 MB.

use crate::model::{idx, summary, Fault, LocalTime, PsuSnapshot, CONNECTORS, PINS};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "events.csv";
pub const OLD_NAME: &str = "events.old.csv";
pub const MAX_BYTES: u64 = 1_000_000;

/// Ordner der Einstellungen und des Logs (`%APPDATA%\GpuMon`).
pub fn dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("GpuMon"))
}

/// Kopfzeile: Zeit, Alarm, 12 Pin-Ströme, 12-V-Spannung, Steckerleistung.
pub fn header() -> String {
    let mut h = String::from("time;alarm");
    for c in 0..CONNECTORS {
        for p in 0..PINS {
            h += &format!(";c{}_pin{}_a", c + 1, p + 1);
        }
    }
    h += ";v12_v;connector_w";
    h
}

/// Eine Zeile für einen neu aufgetretenen Alarm. Fehlende Werte bleiben leer.
pub fn line(at: &LocalTime, fault: Fault, snap: Option<&PsuSnapshot>) -> String {
    let num = |v: Option<f32>, d: usize| v.filter(|v| v.is_finite()).map_or(String::new(), |v| format!("{v:.d$}"));
    let mut l = format!("{};{}", at.iso(), fault.key());
    for c in 0..CONNECTORS {
        for p in 0..PINS {
            l.push(';');
            l += &num(snap.and_then(|s| s.pin(c, p)), 2);
        }
    }
    l.push(';');
    l += &num(snap.and_then(|s| s.get(idx::V12)), 2);
    l.push(';');
    l += &num(snap.filter(|s| s.has_psu()).and_then(summary).map(|s| s.watts), 1);
    l
}

/// Hängt Zeilen an die Datei `path` an; legt sie samt Kopfzeile an und
/// benennt sie oberhalb von `max` Byte einmal in `OLD_NAME` um.
pub fn append(path: &Path, lines: &[String], max: u64) -> std::io::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let size = std::fs::metadata(path).map(|m| m.len()).ok();
    if size.is_some_and(|s| s > max) {
        std::fs::rename(path, path.with_file_name(OLD_NAME))?;
    }
    let fresh = size.is_none_or(|s| s > max);
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    let mut out = String::new();
    if fresh {
        out += &header();
        out += "\r\n";
    }
    for l in lines {
        out += l;
        out += "\r\n";
    }
    f.write_all(out.as_bytes())
}

/// Schreibt die Zeilen ins Log unter `%APPDATA%\GpuMon`; Fehler still.
pub fn log(lines: &[String]) {
    if let Some(d) = dir() {
        let _ = append(&d.join(FILE_NAME), lines, MAX_BYTES);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_line_and_header() {
        let h = header();
        assert!(h.starts_with("time;alarm;c1_pin1_a;") && h.ends_with(";c2_pin6_a;v12_v;connector_w"));
        assert_eq!(h.split(';').count(), 2 + 12 + 2);
        let mut s = PsuSnapshot::default();
        for p in 0..PINS {
            s.vals[idx::PIN0 + p] = Some(7.5);
        }
        s.vals[idx::PIN0 + 2] = Some(9.25);
        s.vals[idx::V12] = Some(12.05);
        let at = LocalTime { year: 2026, month: 11, day: 12, hour: 14, minute: 32, second: 5 };
        // Sprache spielt keine Rolle: immer Dezimalpunkt
        crate::i18n::set_lang(crate::i18n::Lang::De);
        let l = line(&at, Fault::OverCurrent, Some(&s));
        assert_eq!(l, "2026-11-12T14:32:05;overcurrent;7.50;7.50;9.25;7.50;7.50;7.50;;;;;;;12.05;563.3");
        assert_eq!(l.split(';').count(), h.split(';').count());
        // ohne Daten: nur Zeit und Art
        assert_eq!(line(&at, Fault::NoData, None), format!("2026-11-12T14:32:05;nodata{}", ";".repeat(14)));
    }

    #[test]
    fn append_creates_and_rotates() {
        let dir = std::env::temp_dir().join(format!("gpumon-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join(FILE_NAME);
        append(&p, &["a".into()], 200).unwrap();
        append(&p, &["b".into(), "c".into()], 200).unwrap();
        let t = std::fs::read_to_string(&p).unwrap();
        assert_eq!(t, format!("{}\r\na\r\nb\r\nc\r\n", header()));
        // über der Grenze: alte Datei umbenennen, neue mit Kopfzeile
        append(&p, &["d".into()], 10).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), format!("{}\r\nd\r\n", header()));
        assert_eq!(std::fs::read_to_string(dir.join(OLD_NAME)).unwrap(), t);
        // ein zweites Mal: die ältere wird ersetzt
        append(&p, &["e".into()], 10).unwrap();
        assert!(std::fs::read_to_string(dir.join(OLD_NAME)).unwrap().ends_with("d\r\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
