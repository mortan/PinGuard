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
| Balken links | 6 Pins des aktiven Steckers. Ampel (Standard, einstellbar): grün < 8 A, gelb 8–9,2 A, rot ≥ 9,2 A. Vollausschlag 12 A, rote Linie an der Rot-Grenze, gelbe Linie an der Gelb-Grenze (ausblendbar), Strich = Peak seit Start/Reset. Optional eine gestrichelte blaue **Mittelwert-Linie** je Stecker (Durchschnitt der sechs Pins, nur wenn der Stecker Strom führt), in der Ein-Stecker-Ansicht mit Legende „Ø 7,0 A“ in der Titelzeile, sofern sie neben den Titel passt. Stecker 2 erscheint nur, wenn er Strom führt. |
| Kennzahlen rechts | Zwei Gruppen. **Stecker:** Leistung (Σ Pins × gemessene 12 V; Tooltip mit Peak seit Start/Reset), Σ Strom, Max-Pin, Schieflast (Δ Pins in A oder Abw. in %, je nach Messart). **Netzteil:** DC-Leistung, Effizienz, Temperatur, Lüfter. Zusätzlich einblendbar: Auslastung (bezogen auf die Nennleistung je Stecker) und GPU-Board-Power (Afterburner `Power`). |
| Verlaufskurve | Optional statt der Gruppe „Netzteil“: **VERLAUF · 2 MIN**, die Steckerleistung der letzten 2 Minuten (ein Wert je Sekunde). Skala automatisch auf die nächsten vollen 100 W über dem Maximum (mindestens 100 W), höchster Wert als Punkt mit „Peak NNN W“, Lücke = keine Daten. Tooltip mit aktuellem Wert, Peak und dessen Alter. |
| Rails | 12 V / 5 V / 3,3 V: Spannung und Strom (ausblendbar) |
| Verbindung | Kleiner Punkt am Ende der Überschrift „NETZTEIL“ (bzw. „VERLAUF“): grün = Daten kommen, rot mit „keine Daten“ = Verbindung zur Datenquelle weg, orange Ring = war unterbrochen (Klick setzt zurück), blass = Alarm abgeschaltet. |
| Statuszeile | ALARM S1/S2 (Alarm-Flag, das das Netzteil selbst je Stecker meldet), ÜBERSTROM (Pin ≥ Rot-Grenze), SCHIEFLAST (Grenze in A oder %, siehe unten; Standard 3 A). **Rot** = aktiv, **orange umrandet** = war aktiv (gelatcht), blass = abgeschaltet. ALARM S2 erscheint erst, wenn Stecker 2 seit dem Start Strom geführt hat oder sein Alarm auslöst. |
| Alarm-Gedächtnis | Solange ein Alarm aktiv oder gehalten ist, nennt der Tooltip des Statusfelds bzw. des Verbindungspunkts die letzte Auslösung, z. B. „Ausgelöst 14:32 Uhr, Pin 3: 9,6 A, Dauer 4 s“: bei Überstrom den höchsten Pin-Strom (mit Stecker, sobald Stecker 2 im Spiel ist), bei Schieflast den höchsten Wert in der gewählten Messart, bei Netzteil-Alarmen und Verbindung nur Uhrzeit und Dauer. Bei aktivem Alarm läuft die Dauer mit. Zurücksetzen der Fehler löscht das Gedächtnis (ein noch aktiver Alarm behält seinen Beginn). |

Bedienung:
- **Minimieren und Beenden:** Zwei Knöpfe erscheinen oben rechts, solange die Maus über dem Fenster steht. Mit *In den Infobereich minimieren* geht Minimieren direkt dorthin.
- Fenster an beliebiger Stelle ziehen. Größe an den Rändern ändern, das Seitenverhältnis bleibt dabei erhalten.
- **Klick auf die Statuszeile oder den Verbindungspunkt:** gelatchte Fehler zurücksetzen
- **Doppelklick:** Peaks zurücksetzen (Pin-Peaks und Peak der Steckerleistung)
- **Rechtsklick:** Menü mit Peaks/Fehler zurücksetzen, *Grenzen und Alarme …*, *Immer im Vordergrund*, *In den Infobereich minimieren*, *Mit Windows starten*, *Sprache*, *Ereignis-Log* (*Alarme aufzeichnen*, *Ordner öffnen*), Beenden
- Position, Größe, *Immer im Vordergrund*, Infobereich, Ereignis-Log, Sprache, Grenzwerte und Alarme werden in `%APPDATA%\GpuMon\gpumon.ini` gespeichert.
- Es läuft nur eine Instanz. Ein zweiter Start holt das vorhandene Fenster nach vorn (auch aus dem Infobereich).

### Infobereich (Tray)

*Rechtsklick → In den Infobereich minimieren* (INI `tray` = 0/1, Standard 0). Ist die Option an, zeigt
ein rundes Symbol im Infobereich den Zustand: **grün** = in Ordnung, **gelb** = Warnstufe erreicht
(Pin gelb oder Schieflast ab halber Grenze) oder Alarm gehalten, **rot** = Alarm aktiv, **grau** = keine
Daten. Sein Tooltip nennt Steckerleistung, höchsten Pin und aktive Alarme. Minimieren blendet das
Fenster aus der Taskleiste aus; **Linksklick** auf das Symbol holt es zurück, **Rechtsklick** öffnet das
Kontextmenü. Die Alarm-Option *Zeigen* holt das Fenster auch aus dem Infobereich. Die Symbole werden
zur Laufzeit erzeugt und nur bei einem Zustandswechsel neu gesetzt; nach einem Neustart des Explorers
wird das Symbol neu eingetragen.

### Mit Windows starten

*Rechtsklick → Mit Windows starten* schreibt bzw. löscht den Wert `GpuMon` unter
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (Pfad der laufenden EXE in Anführungszeichen).
Das Häkchen wird bei jedem Öffnen des Menüs aus der Registry gelesen und ist nur gesetzt, wenn der
Eintrag auf genau diese EXE zeigt. Nicht in der INI gespeichert.

### Ereignis-Log

Bei jedem neu auftretenden Alarm hängt GpuMon eine Zeile an `%APPDATA%\GpuMon\events.csv` an (INI
`event_log` = 0/1, Standard 1; Menü *Ereignis-Log → Alarme aufzeichnen*). Spalten: `time` (ISO, lokale
Zeit), `alarm` (`alarm1`, `alarm2`, `overcurrent`, `imbalance`, `nodata`), `c1_pin1_a` … `c2_pin6_a`,
`v12_v`, `connector_w`. Trennzeichen Semikolon, Dezimalpunkt, unabhängig von der Sprache; fehlende
Werte bleiben leer. Ab 1 MB wird die Datei in `events.old.csv` umbenannt (eine ältere wird ersetzt)
und neu begonnen. Schreibfehler werden ignoriert. *Ordner öffnen* zeigt die Datei im Explorer.

### Sprache

Die Oberfläche gibt es auf Deutsch und Englisch. Standard ist *Automatisch*: Deutsch, wenn die
Windows-Anzeigesprache Deutsch ist (alle Regionen), sonst Englisch. Über *Rechtsklick → Sprache*
lässt sich *Automatisch (Windows)*, *Deutsch* oder *English* fest wählen; das wirkt sofort, ohne
Neustart. INI-Schlüssel `lang` = `auto` / `de` / `en` (fehlt er, gilt `auto`). Zahlen erscheinen
im Format der Sprache (Deutsch `9,2 A`, Englisch `9.2 A`). Im Einstellungsdialog werden Komma und
Punkt immer akzeptiert. Englische Kurzformen: Statusfelder ALARM C1/C2, OVERCURRENT, IMBALANCE;
Kennzahlen u. a. „Σ Amps“, „Dev.“, „Eff.“.

### Grenzen und Alarme

Über *Rechtsklick → Grenzen und Alarme …* (wirkt sofort, Komma oder Punkt als Dezimalzeichen):

| Grenzwert | Standard | INI-Schlüssel |
|---|---|---|
| Pin gelb ab | 8 A | `pin_warn` |
| Pin rot / Überstrom ab | 9,2 A | `pin_limit` |
| Balken-Vollausschlag | 12 A | `full_scale` |
| Schieflast messen als | Ampere | `imbalance_mode` (`amps` / `percent`) |
| Schieflast (max − min) ab, Modus Ampere | 3 A | `imbalance` |
| Schieflast (größte Abweichung eines Pins vom Mittelwert) ab, Modus Prozent | 30 % | `imbalance_pct` |
| … Prozent-Modus erst ab Steckerstrom (Σ 6 Pins) | 20 A | `imbalance_min_load` |
| Nennleistung je Stecker (Bezug für die Auslastung) | 600 W | `rated_w` |

**Schieflast-Messart:** *Ampere* vergleicht den stärksten mit dem schwächsten Pin (wie `CurrentDif`
im MSI-Plugin). MSI verwendet dort 5,5 A. Bei ~8,3 A pro Pin unter Volllast und 9,2 A Pin-Limit
schlägt das erst sehr spät an, daher ist der Standard hier 3 A (gelb ab 1,5 A). Das ist bei hoher Last aussagekräftig, übersieht aber bei wenig Last sogar einen
ausgefallenen Pin. *Prozent* bewertet die Abweichung vom Mittelwert und skaliert so mit der Last. Unterhalb der
Mindestlast wird nicht bewertet, weil dort schon Messrauschen große Prozentwerte ergibt (ähnlich
WireView Pro II: „Imbalance Limit (%)“ plus „Imbalance Current Limit (A)“). Ein ausgefallener Pin
entspricht 100 % Abweichung. Die Kennzahl links heißt je nach Modus „Δ Pins“ (A) oder „Abw.“ (%).
Sie wird ab der halben Grenze gelb und ab der Grenze rot.

**Anzeige:** Rechts im Dialog lässt sich jede Kennzahl ein- und ausblenden (INI: `show_<name>` = 0/1 mit
`conn_w`, `sum_a`, `max_pin`, `imbalance`, `load`, `gpu_w`, `psu_w`, `eff`, `temp`, `fan`, `rails`).
Unter „Netzteil“ steht *Verlauf stattdessen* (`show_history`): ersetzt die vier Netzteil-Kennzahlen
durch die Verlaufskurve, deren Kästchen sind dann gesperrt. Unter „Balken“: *Warnlinie (gelb)*
(`show_warn_line`; blendet nur die Linie samt Tooltip aus, die Gelbfärbung bleibt) und
*Mittelwert-Linie* (`show_mean_line`). Standardmäßig aus sind Auslastung, GPU-Leistung, Verlaufskurve
und Mittelwert-Linie; fehlt ein Schlüssel (ältere INI), gilt der Standard.

Je Alarm (`alarm1`, `alarm2`, `overcurrent`, `imbalance`, `nodata`) gibt es vier Schalter:
**Aktiv** (`<alarm>_on`: auswerten und anzeigen), **Halten** (`_latch`: bleibt orange markiert bis zum Klick auf die Statuszeile),
**Ton** (`_sound`: Warnton beim Auftreten) und **Zeigen** (`_show`: Fenster wiederherstellen, ohne Fokuswechsel nach vorne holen und in der Taskleiste blinken).
Standard: alle aktiv und haltend, ohne Ton und ohne Zeigen. Ungültige Grenzwerte in der INI werden ignoriert, dann gelten die Standardwerte.

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
Tests: `cargo test` (Modell inkl. Verlauf und Alarm-Gedächtnis, MAHM-Parsing gegen Byte-Puffer, Einstellungen, CSV-Zeilen,
Tray-Symbolbild, Autostart-Kommandozeile, Tooltips und Textbreiten je Sprache). Die Tests schreiben weder in
`%APPDATA%\GpuMon` noch in die Registry oder den Infobereich (CSV-Test nur im Temp-Ordner).

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
  main.rs          Fenster, Message-Loop, Timer, DPI, Hit-Test, Menü, Zeitquelle
  i18n.rs          Texte Deutsch/Englisch, Zahlenformat, Sprachwahl
  settings.rs      Position/Größe/Topmost, Sprache, Tray, Log, Grenzwerte, Alarme (INI)
  settings_dlg.rs  Einstellungsdialog (Win32-Dialog aus Speichervorlage)
  model.rs         PsuSnapshot, Kennzahlen, Peaks, Fehler, Alarm-Gedächtnis, Verlauf (Ringpuffer),
                   Grenzwert-/Alarm-Konfiguration
  events.rs        Ereignis-Log events.csv
  tray.rs          Symbol im Infobereich (zur Laufzeit erzeugte Icons)
  autostart.rs     Run-Schlüssel in HKCU
  tip.rs           eigener Tooltip
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
