# Handover: GpuMon – kompakter 12V-2x6-/PSU-Monitor

## Ziel

Eine kleine Windows-Desktop-Anwendung, die die Pin-Ströme des 12V-2x6-Steckers
(12VHPWR) und die übrigen Netzteilwerte des **MSI MPG Ai1300TS PCIE5** grafisch
ansprechend darstellt. Vorbild ist die WireView2-Software (Per-Wire-Balken,
Fault-Status, Gauges), aber **kompakter** und nur mit den Werten, die das
Netzteil tatsächlich liefert.

Das Fenster läuft beim Spielen auf dem **zweiten Monitor**. Daraus folgt die
wichtigste Anforderung: **minimaler Verbrauch von RAM, CPU und GPU**. Die
Anwendung darf die Spiel-Performance nicht messbar beeinflussen.

Die Technologie ist frei wählbar. Rust ist erwünscht, sofern es keinen guten
Grund für etwas anderes gibt (siehe Empfehlung unten).

## Ausgangslage (bereits geklärt)

### Datenquelle: MSI Afterburner Shared Memory (MAHM)

Auf dem Rechner laufen MSI Afterburner 4.6.7, RTSS 7.3.5 und HWiNFO64 8.60.
Afterburner bringt das Plugin `Plugins\Monitoring\PSU.dll` mit („PSU monitoring
plugin for MSI Afterburner“). Es liest das Netzteil per USB aus und stellt alle
Werte in Afterburners Shared Memory bereit. **Das wurde live verifiziert**: Bei
~500 W GPU-Last kommen ca. 7,5 A pro Pin an.

Shared-Memory-Layout (verifiziert mit Python, v2.0):

| Feld | Wert |
|---|---|
| Name | `MAHMSharedMemory` (per `OpenFileMappingW` + `MapViewOfFile`, nur lesen) |
| Header | `dwSignature` (u32, `'MAHM'` = `0x4D41484D`; `0xDEAD` = Afterburner beendet sich), `dwVersion` (u32, `0x00020000`), `dwHeaderSize` (u32, 32), `dwNumEntries` (u32), `dwEntrySize` (u32, 1324), danach `time` (i32) |
| Eintrag (Offsets) | `szSrcName` 0 (char[260]), `szSrcUnits` 260, `szLocalizedSrcName` 520, `szLocalizedSrcUnits` 780, `szRecommendedFormat` 1040, `data` f32 @1300, `minLimit` f32 @1304, `maxLimit` f32 @1308, `dwFlags` u32 @1312, `dwGpu` u32 @1316, `dwSrcId` u32 @1320 |

Hinweise:
- Die Größe der View muss `headerSize + numEntries * entrySize` sein. Ein zu
  großes Mapping schlägt mit „Zugriff verweigert“ fehl. Am besten zuerst den
  Header mappen und danach mit der richtigen Größe neu mappen, oder Länge 0
  (ganze Section) verwenden.
- Die Strings sind ANSI (cp1252). Die Einheit von `PSU temperature` ist `°C` als
  0xB0.
- Die Reihenfolge und Anzahl der Einträge hängen davon ab, welche Quellen der
  Benutzer in Afterburner aktiviert hat. Einträge daher **per Name suchen**, nie
  per Index. Den Index-Lookup nach einer Änderung von `dwNumEntries` neu
  aufbauen.
- Alle PSU-Quellen haben `dwSrcId = 0x000000F6` (Plugin-PSU) und `dwGpu = 0`.
- Afterburner aktualisiert die Werte im Takt seiner Abfrageperiode
  (Standard 1000 ms, in Afterburner einstellbar). Öfter als diese Periode zu
  lesen, bringt nichts.

Verfügbare Quellennamen (exakt, aus `PSU.dll` und live geprüft):

```
PSU 12VHPWR1_1 current … PSU 12VHPWR1_6 current   [A]  Stecker 1, Pins 1–6
PSU 12VHPWR2_1 current … PSU 12VHPWR2_6 current   [A]  Stecker 2 (beim Benutzer unbelegt = 0)
PSU 12VHPWR1 alarm, PSU 12VHPWR2 alarm            [ ]  0/1
PSU 12V voltage, PSU 5V voltage, PSU 3.3V voltage  [V]
PSU 12V current, PSU 5V current, PSU 3.3V current  [A]
PSU 12V1..12V5 voltage/current                     (im Plugin vorhanden, bei diesem Modell evtl. nicht aktiv)
PSU power                                          [W]  (AC- oder DC-Seite – noch prüfen)
PSU efficiency                                     [%]
PSU temperature                                    [°C]
PSU fan speed                                      [RPM] (0 im Leerlauf = Zero-Fan)
```

Zusätzlich interessant (ebenfalls im MAHM vorhanden): `Power` (GPU-Leistung in
W), z. B. für einen Vergleich zwischen GPU-Board-Power und Steckerleistung.

Konfiguration des Plugins (`Plugins\Monitoring\PSU.cfg`): `GPUSafeguard=1`,
`MaxCurrent=12.0`, `CurrentDif=5.5`. Das sind MSIs eigene Alarmgrenzen
(GPU Safeguard+). Sie sind als Referenz für Schwellwerte geeignet.

### Direkter USB-Zugriff (nur als spätere Option)

Das Netzteil hängt als HID-Gerät am Rechner: `USB\VID_0DB0&PID_AA6F`
(„MPG Ai1300TS“). Das Protokoll ist nicht dokumentiert. Die DLL verwendet einen
Mutex `Global\MSI_PSU_Mutex`, um den Zugriff zu serialisieren.

Ein eigener HID-Zugriff würde die Abhängigkeit von Afterburner beseitigen,
erfordert aber Reverse Engineering von `PSU.dll` und muss mit Afterburner
koexistieren (denselben Mutex verwenden). **Für v1 nicht umsetzen.** Die
Datenquelle aber hinter ein Trait legen, damit sie später ersetzbar ist.

### Schwellwerte / Fachliches

- 12V-2x6-Spezifikation: **9,2 A pro Pin** (bei 600 W ≈ 8,3 A pro Pin).
- Farblogik aus den RTSS-Overlays, die der Benutzer mag: grün < 8 A,
  gelb 8–9,2 A, rot ≥ 9,2 A. Balken-Vollausschlag 12 A, rote Linie bei 9,2 A.
- Schieflast: `max − min` über die 6 Pins (Plugin-Grenze 5,5 A) und/oder die
  Abweichung vom Mittelwert in %.
- Aktiver Stecker: automatisch erkennen (Stecker mit Summenstrom > 0). Stecker 2
  nur anzeigen, wenn er Strom führt.

## Technologie-Empfehlung

**Rust + `windows`-Crate (Win32-Fenster) + Direct2D/DirectWrite**, eigenes
Zeichnen, kein UI-Framework.

Begründung:
- Native Win32 + Direct2D hat praktisch keinen Overhead: erwartet < 10 MB RAM,
  ≈ 0 % CPU und vernachlässigbare GPU-Zeit. Auf Windows gibt es nichts
  Sparsameres.
- Immediate-Mode-Frameworks (egui/eframe, iced) bringen wgpu/OpenGL mit,
  belegen 50–150 MB und verleiten zu Dauer-Repaints. WebView-Lösungen (Tauri,
  Electron) sind für dieses Ziel klar ungeeignet.
- Die Oberfläche besteht aus wenigen statischen Elementen (Balken, Bögen, Text).
  Das lässt sich mit Direct2D in wenigen hundert Zeilen zeichnen.
- Gerendert wird **nur bei neuen Daten** (Timer im Takt der Afterburner-Periode,
  z. B. 250–1000 ms) oder bei `WM_PAINT`/Resize. Keine Animationsschleife, kein
  VSync-Loop. Ist das Fenster minimiert oder verdeckt, wird nicht gezeichnet.
- Ein einziges, statisch gelinktes `.exe` ohne Runtime.

Mögliche Alternative, falls der Aufwand für eigenes Zeichnen zu hoch wird:
**Slint** mit Software-Renderer (ebenfalls Rust, ~10–20 MB). Die Entscheidung
und ihre Begründung im README festhalten.

## Funktionsumfang v1

Kompaktes Fenster, Orientierung am WireView-Look (dunkel, Akzentfarben), aber
deutlich dichter. Grobe Aufteilung:

1. **Per-Pin-Balken** (Hauptelement): 6 vertikale Balken mit Ampelfarben,
   9,2-A-Linie, Wert darunter, Pin-Label. Optional Peak-Markierung pro Pin
   (Maximum seit Start/Reset).
2. **Kennzahlen** daneben: Steckerleistung (Σ Pin-Ströme × 12 V), Σ Strom,
   max. Pin, Schieflast (max − min), PSU-Gesamtleistung, Effizienz,
   Temperatur, Lüfterdrehzahl.
3. **Status-/Fault-Zeile** wie bei WireView, aber kompakt: Alarm Stecker 1/2
   (aus `alarm`-Quellen), Pin-Überstrom (≥ 9,2 A), Schieflast, „Afterburner
   nicht aktiv / keine PSU-Daten“. Fehler bleiben gelatcht, bis sie per Klick
   zurückgesetzt werden (wie „Clear“ bei WireView).
4. **Rails** klein: 12 V / 5 V / 3,3 V Spannung und Strom.

Fenster und Bedienung:
- Rahmenloses oder schlankes Fenster, frei skalierbar. Layout skaliert mit, DPI-aware
  (Per-Monitor v2).
- Position und Größe merken (z. B. JSON/INI neben der exe oder in
  `%APPDATA%\GpuMon`), damit es auf dem zweiten Monitor wieder erscheint.
- Optional „immer im Vordergrund“ und Tray-Icon. Doppelklick oder Kontextmenü
  zum Beenden bzw. Zurücksetzen der Peaks.
- Kein Konsolenfenster (`#![windows_subsystem = "windows"]`).

Nicht in v1: Logging/CSV, Diagramm-Historie (höchstens eine kleine Sparkline,
falls sie keinen Mehraufwand verursacht), direkter HID-Zugriff,
Einstellungs-UI.

## Architektur-Vorschlag

```
src/
  main.rs          Fenster, Message-Loop, Timer, DPI, Fensterposition speichern
  source/mod.rs    trait PsuSource { fn poll(&mut self) -> Option<PsuSnapshot> }
  source/mahm.rs   Afterburner-Shared-Memory-Reader (Lookup per Name)
  source/mock.rs   Fake-Daten (Rampen, Schieflast, Alarm) zum UI-Testen ohne Hardware
  model.rs         PsuSnapshot, abgeleitete Werte (Summe, Schieflast, Peaks, Faults)
  render.rs        Direct2D/DirectWrite-Zeichnen, Farben/Schwellwerte als Konstanten
```

- `PsuSnapshot` enthält `Option<f32>` je Wert, denn fehlende Quellen sind normal.
- Unit-Tests für die abgeleiteten Werte und für das MAHM-Parsing gegen einen
  Byte-Puffer.
- Die Mock-Quelle ist per Kommandozeilenschalter (`--mock`) wählbar.

## Abnahme / Messung

- Release-Build, Fenster sichtbar auf zweitem Monitor, Afterburner läuft:
  Working Set < 15 MB, CPU-Mittel ≈ 0 % (Task-Manager / `Get-Process`),
  GPU-Engine-Auslastung des Prozesses ≈ 0 %.
- Werte stimmen mit dem RTSS-Overlay bzw. mit Afterburners Monitoring-Fenster
  überein.
- Verhalten ohne Afterburner: klarer Hinweis im Fenster, kein Absturz,
  automatische Wiederverbindung, sobald Afterburner startet.

## Umgebung

- Windows 11 IoT Enterprise LTSC 2024, Rust 1.99 stable (MSVC), cargo
  vorhanden. Der Ordner ist noch kein Git-Repo, also `git init` ausführen.
- Im Ordner liegen bereits die RTSS-Overlay-Generatoren aus der Vorgänger-Session
  (`make_ovl.py`, `make_mini_ovl.py`, `make_bars_ovl.py`) und die erzeugten
  `.ovl`-Dateien. Sie gehören nicht zur App und können z. B. nach `rtss/`
  verschoben werden. Dort steht auch, wie der Benutzer Farben und Schwellwerte
  gewählt hat.
- Der Benutzer spricht Deutsch. Die UI-Texte dürfen deutsch oder englisch sein,
  bitte nachfragen oder Deutsch wählen.
