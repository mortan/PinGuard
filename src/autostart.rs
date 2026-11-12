//! „Mit Windows starten“ über `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`,
//! Wert `PinGuard` = Pfad der laufenden EXE in Anführungszeichen. Der Zustand
//! wird bei Bedarf aus der Registry gelesen und nicht in der INI gespeichert.
//!
//! Tests prüfen nur die reinen Funktionen (`command`, `matches`); die
//! Registry wird in Tests weder gelesen noch geschrieben.

use std::path::Path;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::*;

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("PinGuard");

/// Kommandozeile für den Run-Schlüssel: Pfad in Anführungszeichen (Pfade mit
/// Leerzeichen), ohne weitere Argumente.
pub fn command(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// Zeigt der eingetragene Wert auf diese EXE? Groß-/Kleinschreibung und
/// fehlende Anführungszeichen spielen keine Rolle.
pub fn matches(value: &str, exe: &Path) -> bool {
    let v = value.trim();
    let v = v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(v);
    v.eq_ignore_ascii_case(&exe.display().to_string())
}

/// Aktueller Wert `PinGuard` im Run-Schlüssel, falls vorhanden.
fn read() -> Option<String> {
    unsafe {
        let mut buf = [0u16; 1024];
        let mut len = std::mem::size_of_val(&buf) as u32;
        let r = RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut len));
        if r != ERROR_SUCCESS {
            return None;
        }
        let n = (len as usize / 2).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..n]);
        Some(s.trim_end_matches('\0').to_string())
    }
}

/// Startet PinGuard mit Windows (Eintrag zeigt auf die laufende EXE)?
pub fn is_enabled() -> bool {
    match (read(), std::env::current_exe()) {
        (Some(v), Ok(exe)) => matches(&v, &exe),
        _ => false,
    }
}

/// Eintrag setzen (auf die laufende EXE) bzw. entfernen. Liefert Erfolg.
pub fn set(on: bool) -> bool {
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return false };
            let data: Vec<u16> = command(&exe).encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = (data.len() * 2) as u32;
            RegSetKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE, REG_SZ.0, Some(data.as_ptr() as *const _), bytes) == ERROR_SUCCESS
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE) == ERROR_SUCCESS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn command_line() {
        let exe = PathBuf::from(r"C:\Program Files\PinGuard\pinguard.exe");
        assert_eq!(command(&exe), r#""C:\Program Files\PinGuard\pinguard.exe""#);
        assert!(matches(&command(&exe), &exe));
        assert!(matches(r#""c:\program files\pinguard\PINGUARD.EXE""#, &exe));
        assert!(matches(r"C:\Program Files\PinGuard\pinguard.exe ", &exe));
        assert!(!matches(r#""C:\Tools\pinguard.exe""#, &exe));
        assert!(!matches("", &exe));
    }
}
