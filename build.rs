//! Bindet `assets/pinguard.ico` als Anwendungssymbol in die EXE ein
//! (Symbolgruppe 1, die `main.rs` per `LoadIconW` lädt und der Explorer
//! anzeigt). Die `.res`-Datei wird hier direkt geschrieben, damit weder der
//! Ressourcen-Compiler (`rc.exe`) noch eine zusätzliche Crate nötig ist; der
//! MSVC-Linker nimmt `.res`-Dateien unmittelbar an.

use std::path::PathBuf;

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Hängt eine Ressource mit numerischem Typ und Namen an (32-Byte-Kopf,
/// Daten auf 4 Byte aufgefüllt).
fn push(res: &mut Vec<u8>, kind: u16, id: u16, flags: u16, data: &[u8]) {
    res.extend((data.len() as u32).to_le_bytes());
    res.extend(32u32.to_le_bytes());
    for v in [0xFFFF, kind, 0xFFFF, id] {
        res.extend(v.to_le_bytes());
    }
    res.extend(0u32.to_le_bytes()); // DataVersion
    res.extend(flags.to_le_bytes()); // MemoryFlags
    res.extend(0u16.to_le_bytes()); // Sprache: neutral
    res.extend(0u32.to_le_bytes()); // Version
    res.extend(0u32.to_le_bytes()); // Characteristics
    res.extend(data);
    while !res.len().is_multiple_of(4) {
        res.push(0);
    }
}

/// Wandelt eine ICO-Datei in eine `.res` mit den Einzelbildern (RT_ICON
/// 1..n) und der Symbolgruppe 1.
fn ico_to_res(ico: &[u8]) -> Vec<u8> {
    assert!(ico.len() >= 6 && u16_at(ico, 0) == 0 && u16_at(ico, 2) == 1, "keine ICO-Datei");
    let count = u16_at(ico, 4) as usize;
    let mut res = Vec::new();
    push(&mut res, 0, 0, 0, &[]); // leere Kopf-Ressource: 32-Bit-Format
    let mut group = Vec::new();
    group.extend([0u16, 1, count as u16].iter().flat_map(|v| v.to_le_bytes()));
    for i in 0..count {
        let e = &ico[6 + i * 16..6 + (i + 1) * 16];
        let (size, offset) = (u32_at(e, 8) as usize, u32_at(e, 12) as usize);
        let id = i as u16 + 1;
        push(&mut res, RT_ICON, id, 0x1010, &ico[offset..offset + size]);
        // Gruppeneintrag: wie in der ICO-Datei, aber Bild-ID statt Dateiversatz
        group.extend(&e[..12]);
        group.extend(id.to_le_bytes());
    }
    push(&mut res, RT_GROUP_ICON, 1, 0x1030, &group);
    res
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/pinguard.ico");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let ico = std::fs::read("assets/pinguard.ico").expect("assets/pinguard.ico fehlt");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("pinguard.res");
    std::fs::write(&out, ico_to_res(&ico)).expect(".res schreiben");
    println!("cargo:rustc-link-arg-bins={}", out.display());
}
