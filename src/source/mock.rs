//! Fake-Daten zum Testen der Oberfläche ohne Hardware (`--mock`).
//! Zyklus 60 s: Lastrampe bis in den roten Bereich, Schieflast, kurzer Alarm,
//! zum Schluss 5 s "Afterburner aus", damit auch der Hinweis sichtbar wird.

use super::{PsuSource, Reading};
use crate::model::{idx, PsuSnapshot};
use std::time::Instant;

pub struct MockSource {
    start: Instant,
    /// Zweiter Stecker führt ebenfalls Strom (`--mock2`).
    dual: bool,
}

impl MockSource {
    pub fn new(dual: bool) -> Self {
        MockSource { start: Instant::now(), dual }
    }
}

impl PsuSource for MockSource {
    fn poll(&mut self) -> Reading {
        let t = self.start.elapsed().as_secs_f32() % 60.0;
        if t >= 55.0 {
            return Reading::Offline;
        }
        // Dreieck 0 → 1 → 0 über 40 s, danach Teillast mit Schieflast.
        let load = if t < 40.0 { 1.0 - ((t - 20.0) / 20.0).abs() } else { 0.55 };
        let base = 0.4 + load * 9.0;
        let mut s = PsuSnapshot::default();
        let skew = [0.0, -0.3, 0.9, 0.1, -0.2, 0.2];
        for p in 0..6 {
            let mut v = base + skew[p] * load * 1.5 + (t * 3.1 + p as f32).sin() * 0.08;
            if t >= 40.0 && p == 1 {
                v = 0.3; // Schieflast: ein Pin fast stromlos
            }
            if t >= 40.0 && p == 4 {
                v += 1.6;
            }
            s.vals[idx::PIN0 + p] = Some(v.max(0.0));
            let v2 = if self.dual { (v * 0.85 + [0.2, -0.1, 0.0, 0.3, -0.2, 0.1][p]).max(0.0) } else { 0.0 };
            s.vals[idx::PIN0 + 6 + p] = Some(v2);
        }
        let pins: f32 = s.vals[..12].iter().map(|v| v.unwrap()).sum();
        let a12 = pins + 4.0;
        s.vals[idx::ALARM0] = Some(if (45.0..48.0).contains(&t) { 1.0 } else { 0.0 });
        s.vals[idx::ALARM0 + 1] = Some(0.0);
        s.vals[idx::V12] = Some(12.09 - load * 0.06);
        s.vals[idx::V5] = Some(5.05);
        s.vals[idx::V33] = Some(3.33);
        s.vals[idx::A12] = Some(a12);
        s.vals[idx::A5] = Some(1.2);
        s.vals[idx::A33] = Some(0.8);
        let dc = a12 * 12.05 + 9.0;
        let eff = 88.0 + load * 5.0;
        s.vals[idx::PSU_W] = Some(dc);
        s.vals[idx::EFF] = Some(eff);
        s.vals[idx::TEMP] = Some(40.0 + load * 12.0);
        s.vals[idx::FAN] = Some(if load > 0.4 { 400.0 + load * 900.0 } else { 0.0 });
        s.vals[idx::GPU_W] = Some(pins * 12.0 * 0.97);
        Reading::Data(s)
    }

    fn period_ms(&self) -> u32 {
        500
    }

    fn name(&self) -> &'static str {
        "Mock"
    }
}
