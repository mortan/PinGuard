//! Symbol im Infobereich (Tray). Die Farbe zeigt den Gesamtzustand
//! (`model::Status`): grün, gelb, rot, grau. Die Symbole werden zur Laufzeit
//! als 32-Bit-Bitmaps mit Alpha erzeugt (kein Ressourcen-Compiler nötig) und
//! erst beim ersten Bedarf angelegt. Neu gesetzt wird nur bei Änderungen.
//!
//! Klassisches Callback-Verhalten (ohne `NIM_SETVERSION`): `lParam` der
//! Callback-Nachricht ist die Mausnachricht (`WM_LBUTTONUP`, …).

use crate::model::Status;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Callback-Nachricht des Symbols an das Hauptfenster.
pub const WM_TRAY: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;

/// Farben je `Status` (RGB), wie die Ampel im Fenster.
fn color(s: Status) -> u32 {
    match s {
        Status::NoData => 0x8A909C,
        Status::Ok => 0x3CDC78,
        Status::Warn => 0xFFC400,
        Status::Alarm => 0xFF3B3B,
    }
}

fn index(s: Status) -> usize {
    match s {
        Status::NoData => 0,
        Status::Ok => 1,
        Status::Warn => 2,
        Status::Alarm => 3,
    }
}

/// Pixel (BGRA, nicht vorgemultipliziert, Zeile für Zeile von oben) eines
/// runden Symbols: Scheibe in `rgb` mit dunklem Rand, damit sie auf hellen
/// und dunklen Taskleisten erkennbar bleibt. Kanten geglättet.
pub fn icon_pixels(size: usize, rgb: u32) -> Vec<u32> {
    let c = size as f32 / 2.0;
    let r_out = size as f32 * 0.46;
    let ring = (size as f32 / 16.0).max(1.0);
    let mut px = vec![0u32; size * size];
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
            let a = (r_out - d + 0.5).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            // innen Farbe, am Rand dunkel (weich übergeblendet)
            let t = (r_out - ring - d + 0.5).clamp(0.0, 1.0);
            let mix = |sh: u32| {
                let f = ((rgb >> sh) & 0xFF) as f32;
                let b = 0x18 as f32;
                (f * t + b * (1.0 - t)).round() as u32
            };
            let alpha = (a * 255.0).round() as u32;
            px[y * size + x] = alpha << 24 | mix(16) << 16 | mix(8) << 8 | mix(0);
        }
    }
    px
}

unsafe fn make_icon(size: i32, rgb: u32) -> HICON {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size, // von oben nach unten
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let Ok(color) = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else { return HICON::default() };
    if !bits.is_null() {
        let px = icon_pixels(size as usize, rgb);
        std::ptr::copy_nonoverlapping(px.as_ptr(), bits as *mut u32, px.len());
    }
    // Maske: alles 0 (deckend), die Transparenz kommt aus dem Alphakanal.
    let stride = (size as usize).div_ceil(16) * 2;
    let zeros = vec![0u8; stride * size as usize];
    let mask = CreateBitmap(size, size, 1, 1, Some(zeros.as_ptr() as *const _));
    let ii = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
    let icon = CreateIconIndirect(&ii).unwrap_or_default();
    let _ = DeleteObject(color);
    let _ = DeleteObject(mask);
    icon
}

pub struct Tray {
    hwnd: HWND,
    /// Symbole je Status, bei Bedarf erzeugt
    icons: [HICON; 4],
    /// Im Infobereich eingetragen
    added: bool,
    shown: Option<Status>,
    tip: String,
}

impl Tray {
    pub fn new(hwnd: HWND) -> Tray {
        Tray { hwnd, icons: Default::default(), added: false, shown: None, tip: String::new() }
    }

    fn icon(&mut self, s: Status) -> HICON {
        let i = index(s);
        if self.icons[i].is_invalid() {
            let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16);
            self.icons[i] = unsafe { make_icon(size, color(s)) };
        }
        self.icons[i]
    }

    fn data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: self.hwnd, uID: ICON_ID, ..Default::default() }
    }

    /// Symbol eintragen bzw. aktualisieren; ändert nur, was sich geändert hat.
    pub fn set(&mut self, status: Status, tip: &str) {
        if self.added && self.shown == Some(status) && self.tip == tip {
            return;
        }
        let mut d = self.data();
        d.uFlags = NIF_ICON | NIF_TIP | NIF_MESSAGE;
        d.uCallbackMessage = WM_TRAY;
        d.hIcon = self.icon(status);
        // Tooltip auf 127 Zeichen begrenzen (Puffer mit Nullabschluss)
        for (dst, src) in d.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
            *dst = src;
        }
        let ok = unsafe { Shell_NotifyIconW(if self.added { NIM_MODIFY } else { NIM_ADD }, &d) }.as_bool();
        if ok {
            self.added = true;
            self.shown = Some(status);
            self.tip = tip.to_string();
        }
    }

    /// Ist das Symbol im Infobereich eingetragen?
    pub fn is_added(&self) -> bool {
        self.added
    }

    /// Nach einem Explorer-Neustart (`TaskbarCreated`) neu eintragen.
    pub fn recreate(&mut self) {
        self.added = false;
        self.shown = None;
    }

    pub fn remove(&mut self) {
        if self.added {
            let d = self.data();
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &d);
            }
            self.added = false;
            self.shown = None;
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.remove();
        for i in &mut self.icons {
            if !i.is_invalid() {
                unsafe {
                    let _ = DestroyIcon(*i);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_shape() {
        let px = icon_pixels(16, 0x3CDC78);
        assert_eq!(px.len(), 256);
        // Ecken durchsichtig, Mitte deckend in der Statusfarbe (BGRA)
        assert_eq!(px[0] >> 24, 0);
        assert_eq!(px[255] >> 24, 0);
        assert_eq!(px[8 * 16 + 8], 0xFF3CDC78);
        // Rand dunkel, aber sichtbar
        let edge = px[8 * 16];
        assert!(edge >> 24 > 0 && (edge >> 8 & 0xFF) < 0x80, "{edge:08X}");
        // alle vier Farben verschieden
        let cols: Vec<u32> = [Status::NoData, Status::Ok, Status::Warn, Status::Alarm].iter().map(|s| color(*s)).collect();
        assert!(cols.iter().enumerate().all(|(i, c)| !cols[..i].contains(c)));
    }
}
