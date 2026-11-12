//! Eigener Warnton für Alarme.
//!
//! Der Ton wird beim ersten Alarm einmal im Speicher als WAV erzeugt (keine
//! Datei, keine Ressource) und asynchron über `PlaySoundW` abgespielt.
//! Klang: zweimal vier schnelle Pieptöne bei 1760 Hz (A6) mit etwas 3. Harmonischer,
//! wie ein Messgerät. Dauer gut 1 s.

use std::f32::consts::TAU;
use std::sync::OnceLock;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

const RATE: u32 = 22_050;
/// Tonhöhe der Pieptöne (Hz)
const FREQ: f32 = 1760.0;
const BEEPS: u32 = 4;
const GROUPS: u32 = 2;
const BEEP_MS: u32 = 70;
/// Abstand von Piepton-Anfang zu Piepton-Anfang
const BEEP_STEP_MS: u32 = 130;
/// zusätzliche Pause zwischen den Gruppen
const GROUP_GAP_MS: u32 = 250;
/// Spitzenpegel relativ zu Vollaussteuerung
const LEVEL: f32 = 0.7;

/// Spielt den Warnton ab (kehrt sofort zurück; ein laufender Ton wird neu gestartet).
pub fn play() {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    let wav = WAV.get_or_init(alarm_wav);
    // SND_MEMORY: der Zeiger zeigt auf das WAV-Abbild; es bleibt als static gültig.
    unsafe {
        let _ = PlaySoundW(PCWSTR(wav.as_ptr() as *const u16), HMODULE::default(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

fn samples(ms: u32) -> usize {
    (RATE * ms / 1000) as usize
}

/// Ein Piepton ab `start`: Grundton + 3. Harmonische, 4 ms Ein- und Ausblenden gegen Knacken.
fn beep(out: &mut [f32], start: usize) {
    let n = samples(BEEP_MS);
    let fade = samples(4) as f32;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let env = (i as f32 / fade).min(1.0) * ((n - i) as f32 / fade).min(1.0);
        out[start + i] += env * ((TAU * FREQ * t).sin() + 0.2 * (TAU * 3.0 * FREQ * t).sin());
    }
}

/// Vollständige WAV-Datei (PCM, 16 Bit, mono).
pub fn alarm_wav() -> Vec<u8> {
    let starts: Vec<u32> = (0..GROUPS * BEEPS).map(|i| i * BEEP_STEP_MS + (i / BEEPS) * GROUP_GAP_MS).collect();
    let end_ms = starts.last().unwrap() + BEEP_MS + 20;
    let mut s = vec![0.0f32; samples(end_ms)];
    for ms in starts {
        beep(&mut s, samples(ms));
    }
    let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let scale = LEVEL * i16::MAX as f32 / peak;

    let data_len = (s.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    let mut put = |b: &[u8]| w.extend_from_slice(b);
    put(b"RIFF");
    put(&(36 + data_len).to_le_bytes());
    put(b"WAVEfmt ");
    put(&16u32.to_le_bytes());
    put(&1u16.to_le_bytes()); // PCM
    put(&1u16.to_le_bytes()); // mono
    put(&RATE.to_le_bytes());
    put(&(RATE * 2).to_le_bytes()); // Bytes pro Sekunde
    put(&2u16.to_le_bytes()); // Blockgröße
    put(&16u16.to_le_bytes()); // Bit pro Sample
    put(b"data");
    put(&data_len.to_le_bytes());
    for v in s {
        put(&((v * scale).round() as i16).to_le_bytes());
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_and_level() {
        let w = alarm_wav();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()) as usize, w.len() - 8);
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize, w.len() - 44);
        let pcm: Vec<i16> = w[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        let peak = pcm.iter().map(|v| v.unsigned_abs()).max().unwrap();
        assert!((22_000..23_500).contains(&peak), "{peak}");
        // Anfang und Ende still (kein Knacken)
        assert_eq!(pcm[0], 0);
        assert_eq!(*pcm.last().unwrap(), 0);
        let secs = pcm.len() as f32 / RATE as f32;
        assert!((1.1..1.4).contains(&secs), "{secs}");
    }

    /// Schreibt den Ton zum Anhören nach %TEMP%\pinguard-alarm.wav
    /// (`cargo test -- --ignored write_sample`).
    #[test]
    #[ignore]
    fn write_sample() {
        std::fs::write(std::env::temp_dir().join("pinguard-alarm.wav"), alarm_wav()).unwrap();
    }
}
