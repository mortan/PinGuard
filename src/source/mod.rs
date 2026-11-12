//! Datenquellen. Die App kennt nur das Trait; v1 liest MSI Afterburners Shared
//! Memory, eine spätere Version kann das Netzteil direkt per HID abfragen.

pub mod mahm;
pub mod mock;

use crate::model::PsuSnapshot;

/// Ergebnis einer Abfrage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reading {
    /// Aktuelle PSU-Werte.
    Data(PsuSnapshot),
    /// Quelle erreichbar (Afterburner läuft), aber keine PSU-Werte vorhanden.
    NoPsu,
    /// Quelle nicht erreichbar (Afterburner läuft nicht / hängt).
    Offline,
}

pub trait PsuSource {
    /// Fragt den aktuellen Stand ab. Muss billig sein (wird im Timer-Takt
    /// aufgerufen) und kümmert sich selbst um (Wieder-)Verbindung.
    fn poll(&mut self) -> Reading;
    /// Sinnvolles Abfrageintervall in Millisekunden.
    fn period_ms(&self) -> u32;
    /// Kurzer Name für Hinweise in der UI.
    fn name(&self) -> &'static str;
}
