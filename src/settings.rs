//! Fensterposition/-größe und "immer im Vordergrund" in
//! `%APPDATA%\GpuMon\gpumon.ini` (einfaches key=value).

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Settings {
    /// Fensterrechteck in physischen Bildschirmpixeln (x, y, Breite, Höhe).
    pub rect: Option<(i32, i32, i32, i32)>,
    pub topmost: bool,
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("GpuMon").join("gpumon.ini"))
}

impl Settings {
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        let (mut x, mut y, mut w, mut h) = (None, None, None, None);
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "x" => x = v.parse().ok(),
                "y" => y = v.parse().ok(),
                "w" => w = v.parse().ok(),
                "h" => h = v.parse().ok(),
                "topmost" => s.topmost = v == "1",
                _ => {}
            }
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

    #[test]
    fn roundtrip() {
        let s = Settings { rect: Some((-1700, 120, 660, 294)), topmost: true };
        assert_eq!(Settings::parse(&s.serialize()), s);
        assert_eq!(Settings::parse("garbage\nx=1"), Settings::default());
    }
}
