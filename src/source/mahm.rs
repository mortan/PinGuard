//! Leser für MSI Afterburners Hardware-Monitoring-Shared-Memory
//! (`MAHMSharedMemory`, Layout v2.0).
//!
//! Das Parsing arbeitet auf `&[u8]` und ist damit gegen Byte-Puffer testbar;
//! `MahmSource` kümmert sich nur um Mapping, Wiederverbindung und Stale-Erkennung.

use super::{PsuSource, Reading};
use crate::model::{idx, PsuSnapshot, SOURCE_NAMES};

pub const SIGNATURE: u32 = 0x4D41_484D; // 'MAHM'
pub const HEADER_MIN: usize = 24;
const NAME_LEN: usize = 260;
const OFF_UNITS: usize = 260;
const OFF_DATA: usize = 1300;
const OFF_GPU: usize = 1316;
const ENTRY_MIN: usize = 1324;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub signature: u32,
    pub version: u32,
    pub header_size: u32,
    pub num_entries: u32,
    pub entry_size: u32,
    pub time: i32,
}

impl Header {
    pub fn total_size(&self) -> usize {
        self.header_size as usize + self.num_entries as usize * self.entry_size as usize
    }
    pub fn plausible(&self) -> bool {
        self.signature == SIGNATURE
            && self.version >= 0x0002_0000
            && self.header_size as usize >= HEADER_MIN
            && self.entry_size as usize >= ENTRY_MIN
            && self.num_entries < 4096
    }
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

pub fn parse_header(b: &[u8]) -> Option<Header> {
    if b.len() < HEADER_MIN {
        return None;
    }
    Some(Header {
        signature: u32_at(b, 0),
        version: u32_at(b, 4),
        header_size: u32_at(b, 8),
        num_entries: u32_at(b, 12),
        entry_size: u32_at(b, 16),
        time: u32_at(b, 20) as i32,
    })
}

/// Vergleicht einen nullterminierten char[260]-Eintrag mit `s`.
fn cstr_eq(field: &[u8], s: &str) -> bool {
    let s = s.as_bytes();
    field.len() > s.len() && &field[..s.len()] == s && field[s.len()] == 0
}

/// Position der gesuchten Quellen (Eintragsindex) für ein bestimmtes Layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Index {
    pub slots: [Option<u32>; idx::COUNT],
    num_entries: u32,
    entry_size: u32,
    header_size: u32,
}

impl Index {
    fn matches_layout(&self, h: &Header) -> bool {
        self.num_entries == h.num_entries
            && self.entry_size == h.entry_size
            && self.header_size == h.header_size
    }
}

fn entry<'a>(b: &'a [u8], h: &Header, i: u32) -> Option<&'a [u8]> {
    let o = h.header_size as usize + i as usize * h.entry_size as usize;
    b.get(o..o + h.entry_size as usize)
}

fn slot_matches(e: &[u8], field: usize) -> bool {
    if !cstr_eq(&e[..NAME_LEN], SOURCE_NAMES[field]) {
        return false;
    }
    if field == idx::GPU_W {
        // "Power" kann je nach Karte auch in % geliefert werden; nur Watt
        // der ersten GPU übernehmen.
        return cstr_eq(&e[OFF_UNITS..OFF_UNITS + NAME_LEN], "W") && u32_at(e, OFF_GPU) == 0;
    }
    true
}

/// Sucht alle bekannten Quellen per Name.
pub fn build_index(b: &[u8], h: &Header) -> Index {
    let mut slots = [None; idx::COUNT];
    for i in 0..h.num_entries {
        let Some(e) = entry(b, h, i) else { break };
        for (f, slot) in slots.iter_mut().enumerate() {
            if slot.is_none() && slot_matches(e, f) {
                *slot = Some(i);
                break;
            }
        }
    }
    Index { slots, num_entries: h.num_entries, entry_size: h.entry_size, header_size: h.header_size }
}

fn valid(v: f32) -> Option<f32> {
    // Afterburner meldet nicht verfügbare Werte als FLT_MAX.
    (v.is_finite() && v.abs() < 1.0e30).then_some(v)
}

/// Liest die Werte über einen Index. `None`, wenn der Index nicht mehr zum
/// Speicher passt (Quellen in Afterburner umkonfiguriert) – dann neu bauen.
pub fn read_values(b: &[u8], h: &Header, ix: &Index) -> Option<PsuSnapshot> {
    if !ix.matches_layout(h) {
        return None;
    }
    let mut s = PsuSnapshot::default();
    for (f, slot) in ix.slots.iter().enumerate() {
        if let Some(i) = *slot {
            let e = entry(b, h, i)?;
            if !slot_matches(e, f) {
                return None;
            }
            s.vals[f] = valid(f32::from_le_bytes(e[OFF_DATA..OFF_DATA + 4].try_into().unwrap()));
        }
    }
    Some(s)
}

// ---------------------------------------------------------------------------
// Windows-Teil: Mapping und Verbindungsverwaltung
// ---------------------------------------------------------------------------

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Memory::{
    MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS,
};
use windows::Win32::System::SystemInformation::GetTickCount64;

/// Nach so vielen ms ohne Änderung des Zeitstempels gilt Afterburner als hängend
/// bzw. beendet (Mapping wird geschlossen und neu geöffnet).
const STALE_MS: u64 = 10_000;
const RETRY_MS: u64 = 2_000;

struct Mapping {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    len: usize,
}

impl Mapping {
    fn bytes(&self) -> &[u8] {
        // SAFETY: Die View ist `len` Bytes groß und lebt so lange wie `self`.
        // Afterburner schreibt parallel; wir lesen nur einfache Zahlen/Strings
        // und prüfen alles, was wir lesen.
        unsafe { std::slice::from_raw_parts(self.view.Value as *const u8, self.len) }
    }

    fn open(name: &[u16]) -> Option<Mapping> {
        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ.0, false, PCWSTR(name.as_ptr())).ok()?;
            let map = |len: usize| {
                let v = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, len);
                (!v.Value.is_null()).then_some(v)
            };
            let Some(v) = map(HEADER_MIN) else {
                let _ = CloseHandle(handle);
                return None;
            };
            let hdr = parse_header(std::slice::from_raw_parts(v.Value as *const u8, HEADER_MIN));
            let _ = UnmapViewOfFile(v);
            let hdr = match hdr {
                Some(h) if h.plausible() => h,
                _ => {
                    let _ = CloseHandle(handle);
                    return None;
                }
            };
            let len = hdr.total_size();
            match map(len) {
                Some(view) => Some(Mapping { handle, view, len }),
                None => {
                    let _ = CloseHandle(handle);
                    None
                }
            }
        }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.handle);
        }
    }
}

pub struct MahmSource {
    name: Vec<u16>,
    map: Option<Mapping>,
    index: Option<Index>,
    next_retry: u64,
    last_time: i32,
    last_change: u64,
}

impl MahmSource {
    pub const DEFAULT_NAME: &'static str = "MAHMSharedMemory";

    /// `name`: Name der Section (Standard `MAHMSharedMemory`; anderer Name nur
    /// zum Testen der Wiederverbindung mit einer simulierten Section).
    pub fn new(name: &str) -> Self {
        let name = name.encode_utf16().chain(Some(0)).collect();
        MahmSource { name, map: None, index: None, next_retry: 0, last_time: 0, last_change: 0 }
    }

    fn disconnect(&mut self, now: u64, retry_in: u64) -> Reading {
        self.map = None;
        self.index = None;
        self.next_retry = now + retry_in;
        Reading::Offline
    }

    fn poll_at(&mut self, now: u64, depth: u8) -> Reading {
        if self.map.is_none() {
            if now < self.next_retry {
                return Reading::Offline;
            }
            match Mapping::open(&self.name) {
                Some(m) => {
                    self.map = Some(m);
                    self.last_time = i32::MIN;
                    self.last_change = now;
                }
                None => return self.disconnect(now, RETRY_MS),
            }
        }
        let m = self.map.as_ref().unwrap();
        let b = m.bytes();
        let Some(h) = parse_header(b) else { return self.disconnect(now, RETRY_MS) };
        if !h.plausible() {
            // 0xDEAD: Afterburner beendet sich gerade.
            return self.disconnect(now, RETRY_MS);
        }
        if h.total_size() != m.len {
            // Anzahl der Quellen geändert: neu mappen.
            self.map = None;
            self.index = None;
            return if depth == 0 { self.poll_at(now, 1) } else { self.disconnect(now, RETRY_MS) };
        }
        if h.time != self.last_time {
            self.last_time = h.time;
            self.last_change = now;
        } else if now - self.last_change > STALE_MS {
            // Zeitstempel steht: Afterburner wurde hart beendet oder hängt.
            // Mapping freigeben, damit die Section verschwinden kann.
            return self.disconnect(now, RETRY_MS);
        }

        let snap = match self.index.as_ref().and_then(|ix| read_values(b, &h, ix)) {
            Some(s) => s,
            None => {
                let ix = build_index(b, &h);
                let s = read_values(b, &h, &ix).unwrap_or_default();
                self.index = Some(ix);
                s
            }
        };
        if snap.has_psu() {
            Reading::Data(snap)
        } else {
            Reading::NoPsu
        }
    }
}

impl PsuSource for MahmSource {
    fn poll(&mut self) -> Reading {
        let now = unsafe { GetTickCount64() };
        self.poll_at(now, 0)
    }

    fn period_ms(&self) -> u32 {
        250
    }

    fn name(&self) -> &'static str {
        "Afterburner"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ES: usize = 1324;
    const HS: usize = 32;

    fn put_str(buf: &mut [u8], o: usize, s: &str) {
        buf[o..o + s.len()].copy_from_slice(s.as_bytes());
    }

    fn build(entries: &[(&str, &str, f32, u32)]) -> Vec<u8> {
        let mut b = vec![0u8; HS + entries.len() * ES];
        b[0..4].copy_from_slice(&SIGNATURE.to_le_bytes());
        b[4..8].copy_from_slice(&0x0002_0000u32.to_le_bytes());
        b[8..12].copy_from_slice(&(HS as u32).to_le_bytes());
        b[12..16].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        b[16..20].copy_from_slice(&(ES as u32).to_le_bytes());
        b[20..24].copy_from_slice(&1234i32.to_le_bytes());
        for (i, (name, unit, v, gpu)) in entries.iter().enumerate() {
            let o = HS + i * ES;
            put_str(&mut b, o, name);
            put_str(&mut b, o + 260, unit);
            b[o + 1300..o + 1304].copy_from_slice(&v.to_le_bytes());
            b[o + 1316..o + 1320].copy_from_slice(&gpu.to_le_bytes());
            b[o + 1320..o + 1324].copy_from_slice(&0xF6u32.to_le_bytes());
        }
        b
    }

    #[test]
    fn parse_and_lookup_by_name() {
        let b = build(&[
            ("GPU usage", "%", 1.0, 0),
            ("Power", "%", 55.0, 0), // falsche Einheit -> ignorieren
            ("Power", "W", 480.5, 0),
            ("PSU 12VHPWR1_1 current", "A", 7.5, 0),
            ("PSU 12VHPWR1_10 current", "A", 99.0, 0), // Präfix darf nicht matchen
            ("PSU 12VHPWR1_6 current", "A", 7.25, 0),
            ("PSU temperature", "\u{b0}C", 41.0, 0),
            ("PSU fan speed", "RPM", f32::MAX, 0), // nicht verfügbar
            ("PSU 12VHPWR1 alarm", "", 1.0, 0),
        ]);
        let h = parse_header(&b).unwrap();
        assert!(h.plausible());
        assert_eq!(h.num_entries, 9);
        assert_eq!(h.total_size(), b.len());
        let ix = build_index(&b, &h);
        let s = read_values(&b, &h, &ix).unwrap();
        assert_eq!(s.pin(0, 0), Some(7.5));
        assert_eq!(s.pin(0, 5), Some(7.25));
        assert_eq!(s.pin(0, 1), None);
        assert_eq!(s.get(idx::GPU_W), Some(480.5));
        assert_eq!(s.get(idx::TEMP), Some(41.0));
        assert_eq!(s.get(idx::FAN), None);
        assert_eq!(s.alarm(0), Some(true));
        assert!(s.has_psu());
    }

    #[test]
    fn index_invalidated_on_reorder() {
        let b1 = build(&[("PSU power", "W", 600.0, 0), ("PSU efficiency", "%", 92.0, 0)]);
        let h = parse_header(&b1).unwrap();
        let ix = build_index(&b1, &h);
        let b2 = build(&[("PSU efficiency", "%", 92.0, 0), ("PSU power", "W", 600.0, 0)]);
        assert!(read_values(&b2, &parse_header(&b2).unwrap(), &ix).is_none());
        let b3 = build(&[("PSU power", "W", 600.0, 0)]);
        assert!(read_values(&b3, &parse_header(&b3).unwrap(), &ix).is_none());
    }

    #[test]
    fn dead_signature() {
        let mut b = build(&[]);
        b[0..4].copy_from_slice(&0xDEADu32.to_le_bytes());
        assert!(!parse_header(&b).unwrap().plausible());
        assert!(parse_header(&b[..10]).is_none());
    }

    #[test]
    fn no_psu_entries() {
        let b = build(&[("GPU usage", "%", 1.0, 0), ("Power", "W", 30.0, 0)]);
        let h = parse_header(&b).unwrap();
        let s = read_values(&b, &h, &build_index(&b, &h)).unwrap();
        assert!(!s.has_psu());
    }
}
