# GpuMon

Kompakter Monitor für den 12V-2x6-Stecker (12VHPWR) und die Netzteilwerte des
**MSI MPG Ai1300TS PCIE5**. Er zeigt Ströme pro Pin, Fehlerstatus und
Kennzahlen, ähnlich wie WireView, nur dichter gepackt. Gedacht für den zweiten
Monitor beim Spielen, daher auf minimalen Verbrauch ausgelegt.

![GpuMon live](docs/screenshot.png)

Mock-Modus mit Schieflast, Alarm und gelatchtem Überstrom: [docs/screenshot-mock-faults.png](docs/screenshot-mock-faults.png)

## Anzeige

| Bereich | Inhalt |
|---|---|
| Balken links | 6 Pins des aktiven Steckers. Ampel: grün < 8 A, gelb 8–9,2 A, rot ≥ 9,2 A. Vollausschlag 12 A, rote Linie bei 9,2 A, gelbe Linie bei 8 A, Strich = Peak seit Start/Reset. Stecker 2 erscheint nur, wenn er Strom führt. |
| Kennzahlen rechts | Stecker-Leistung (Σ Pins × gemessene 12 V), Auslastung (bezogen auf 600 W), Σ Strom, Max-Pin, Δ Pins (max − min), GPU-Board-Power (Afterburner `Power`), PSU DC, Effizienz, Temperatur, Lüfter |
| Rails | 12 V / 5 V / 3,3 V: Spannung und Strom |
| Statuszeile | ALARM S1/S2 (Netzteil-Alarm), ÜBERSTROM (Pin ≥ 9,2 A), SCHIEFLAST (Δ ≥ 5,5 A = `CurrentDif` des MSI-Plugins), VERBINDUNG. **Rot** = aktiv, **orange umrandet** = war aktiv (gelatcht). |

Bedienung:
- Fenster an beliebiger Stelle ziehen. Größe an den Rändern ändern, das Seitenverhältnis bleibt dabei erhalten.
- **Klick auf die Statuszeile:** gelatchte Fehler zurücksetzen
- **Doppelklick:** Peaks zurücksetzen
- **Rechtsklick:** Menü mit Peaks/Fehler zurücksetzen, *Immer im Vordergrund*, Beenden
- Position, Größe und *Immer im Vordergrund* werden in `%APPDATA%\GpuMon\gpumon.ini` gespeichert.
- Es läuft nur eine Instanz. Ein zweiter Start holt das vorhandene Fenster nach vorn.

Kommandozeile: `gpumon.exe [--mock] [--topmost] [--mahm-name NAME]`
- `--mock` erzeugt Fake-Daten (Rampe bis Rot, Schieflast, Alarm, 5 s „offline“) zum Testen ohne Hardware.
- `--mahm-name` liest eine andere Shared-Memory-Section, nur für Tests.

## Voraussetzungen

MSI Afterburner (getestet: 4.6.7) mit dem mitgelieferten PSU-Plugin
(`Plugins\Monitoring\PSU.dll`), das das Netzteil per USB ausliest. GpuMon liest
Afterburners Shared Memory `MAHMSharedMemory` nur lesend aus und sucht die
Quellen per Name (`PSU 12VHPWR1_1 current` …). Die Reihenfolge, in der die
Quellen in Afterburner aktiviert sind, spielt deshalb keine Rolle.

Läuft Afterburner nicht, zeigt GpuMon einen Hinweis. Alle 2 s wird erneut
versucht zu verbinden. Wenn Afterburner sich beendet (Signatur `0xDEAD`) oder
der Zeitstempel 10 s lang stehen bleibt (Afterburner abgestürzt oder hängt),
gibt GpuMon das Mapping frei und verbindet sich neu.

Hinweis zu den Werten: `PSU power` ist die **DC-Leistung der 12-V-Schiene**
(12V current × 12V voltage), nicht die Aufnahme an der Steckdose. Die
AC-Aufnahme beträgt ungefähr PSU DC ÷ Effizienz.

## Bauen

```bash
cargo build --release
```

Das Ergebnis ist `target\release\gpumon.exe` (~190 KB, statisch gelinkt, ohne Runtime und ohne Konsolenfenster).
Tests: `cargo test` (Modell, MAHM-Parsing gegen Byte-Puffer, Einstellungen).

## Technik und Entscheidungen

- **Rust + `windows`-Crate, reines Win32-Fenster ohne UI-Framework.** Das Fenster ist
  rahmenlos (`WM_NCCALCSIZE`), mit Windows-11-Ecken, Per-Monitor-DPI v2 und skalierendem
  Layout (Basisraster 440 × 196 DIPs).
- **Gezeichnet wird mit GDI/GDI+, nicht mit Direct2D.** Ursprünglich war Direct2D
  vorgesehen (siehe HANDOVER). Gemessen hat sich gezeigt: Direct2D lädt auch im
  Software-Modus D3D11/WARP. Daraufhin hängt sich **RTSS** (samt NVAPI) in den
  Prozess ein, wie in jede D3D-Anwendung. Ergebnis: 43 MB Working Set, 74 MB
  privat, 21 Threads. Mit GDI+ (geglättete Formen) und GDI-ClearType-Text in
  einen Backbuffer bleibt der Prozess für RTSS uninteressant, genauso wie
  Afterburner selbst.
- **Kein Dauer-Rendering.** Ein Timer liest alle 250 ms das Shared Memory (ein
  paar Dutzend Bytes). Neu gezeichnet wird nur, wenn sich die Werte ändern. Bei
  Afterburners Standardperiode ist das 1× pro Sekunde, dazu kommen
  `WM_PAINT`/Resize. Minimiert wird nicht gezeichnet, und Backbuffer und
  Schriften werden freigegeben.
- **Die Datenquelle liegt hinter dem Trait `PsuSource`** (`src/source/`). Ein
  späterer direkter HID-Zugriff (`VID_0DB0&PID_AA6F`, Mutex
  `Global\MSI_PSU_Mutex`) kann Afterburner ersetzen, ohne dass sich an der UI
  etwas ändert.

```
src/
  main.rs          Fenster, Message-Loop, Timer, DPI, Hit-Test, Menü
  settings.rs      Position/Größe/Topmost (INI)
  model.rs         PsuSnapshot, Kennzahlen, Peaks, gelatchte Fehler, Schwellwerte
  render.rs        Layout + Zeichnen (GDI+/GDI), Farben
  source/mod.rs    trait PsuSource
  source/mahm.rs   Afterburner-Shared-Memory (Parsing + Verbindung)
  source/mock.rs   Fake-Daten
rtss/              RTSS-Overlays (*.ovl) und ihre Generatoren (nicht Teil der App)
```

## Gemessener Verbrauch

Release-Build, Fenster 1064 × 474 px auf dem zweiten Monitor (5120 × 1440),
Afterburner liefert Live-Daten, nebenher läuft ein Spiel mit ~500 W GPU-Last.
Messzeitraum 120 s, gemessen per `Get-Process` bzw. Performance-Counter, 16 logische CPUs:

| Größe | Wert |
|---|---|
| Working Set | **14,2–14,3 MB** (davon privat nur **3,0 MB**; der Rest sind gemeinsam genutzte System-DLLs) |
| CPU | 531 ms in 120 s = 0,44 % eines Kerns = **0,03 % gesamt** (Redraw ≈ 4 ms/s; minimiert ≈ 0,5 ms/s) |
| GPU | **0**. Der Prozess hat keine GPU-Engine-Instanz, D3D wird nicht geladen. |
| Threads | 4 |
| Zum Vergleich: erste Version mit Direct2D (Software) | 43 MB WS, 74 MB privat, 21 Threads |

## Abgleich

Live gegen `MAHMSharedMemory` geprüft (gleiche Quelle wie das RTSS-Overlay):
Pins 6,25/6,12/6,06/6,12/6,06/6,25 A, PSU 492 W, 93,9 %, 61 °C,
12,08 V/40,8 A. Alle Werte stimmen exakt überein.

Wiederverbindung mit simulierter Section getestet: Start ohne Quelle → Hinweis,
Quelle erscheint → Daten, `0xDEAD` → Hinweis + gelatchter VERBINDUNG-Fehler,
Quelle wieder da → Daten, eingefrorener Zeitstempel → nach 10 s Hinweis.
