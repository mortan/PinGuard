# Plan: Eigene Datenquellen für GpuMon (Netzteil ohne Afterburner auslesen)

Stand: 12.11.2026. Diese Datei fasst alles zusammen, was nötig ist, um GpuMon später
um eigene Datenquellen zu erweitern. Sie ist eigenständig nutzbar. Belege und
Herleitungen (Funktionsadressen, Hashes, Logauszüge) stehen ausführlich in
`docs/hid-protokoll-analyse.md`.

Kennzeichnung: **[sicher]** = im Code bzw. live beobachtet, **[abgeleitet]** =
Schlussfolgerung, **[offen]** = noch zu klären.

---

## 1. Ausgangslage (v1)

- GpuMon liest die Werte aus Afterburners Shared Memory `MAHMSharedMemory`. Befüllt wird
  es vom Afterburner-Plugin `Plugins\Monitoring\PSU.dll`, das das Netzteil per USB-HID
  abfragt. Layout, Quellennamen und Verbindungslogik: `src/source/mahm.rs`, `HANDOVER.md`,
  `README.md`.
- Die Datenquelle ist austauschbar über das Trait `PsuSource` (`src/source/mod.rs`):
  ```rust
  pub enum Reading { Data(PsuSnapshot), NoPsu, Offline }
  pub trait PsuSource { fn poll(&mut self) -> Reading; fn period_ms(&self) -> u32; fn name(&self) -> &'static str; }
  ```
  Vorhanden sind `MahmSource` und `MockSource` (`--mock`, `--mock2`). Neue Quellen
  brauchen nur dieses Trait, an der Anzeige ändert sich nichts.
- `PsuSnapshot` (`src/model.rs`) besteht aus `Option<f32>` je Wert (Indizes in `model::idx`).
  Für neue Werte (AC-Spannung, OCP-Status mit Grund) kommen dort neue Felder dazu.
- Rahmenbedingung: Der Verbrauch muss minimal bleiben (heute 14 MB WS, 3 MB privat,
  0,03 % CPU, keine GPU). Kein D3D laden (sonst injiziert sich RTSS), neu zeichnen nur bei
  neuen Daten.

## 2. Optionen im Überblick

| | A: Afterburner-Takt senken | B: `HwinfoSource` | C: `HidSource` (eigener USB-Zugriff) |
|---|---|---|---|
| Aufwand | Einstellung, 0 Code | ~200–250 Zeilen, ½ Tag | ~300–400 Zeilen, ~1,5 Tage inkl. Messtest |
| Ohne Afterburner | nein | ja (braucht HWiNFO) | ja |
| Risiko fürs Netzteil | keins | keins | gering (Whitelist, siehe 5.6) |
| Takt | einstellbar (Default 1 s) | 2 s (in HWiNFO einstellbar) | frei, sinnvoll 1–5 Hz |
| OCP-/Alarmstatus | 0/1 je Stecker | **keiner** | **mit Grund** (Pin-OCP/Schieflast/18 A) |
| Zusatzwerte | – | Total Runtime, Min/Max/Avg | AC-Spannung, Schutz-Alarme, OCP-Schwellen, Laufzeit, Seriennr. |
| Stolpersteine | – | Free-Version: Shared Memory zeitlich begrenzt | 4. Leser am Gerät, Firmware-Updates |

**Empfehlung (aus der Analyse):** C umsetzen, aber gestuft und als **zusätzliche** Quelle
mit MAHM als Fallback. B ist ein günstiger, risikoloser Fallback, ersetzt C aber nicht.
A vorher einmal ausprobieren, das kostet nichts.

## 3. Reihenfolge der Umsetzung

1. **Ohne eigene Zugriffe (risikolos)**
   - [ ] Option A testen: In Afterburner Einstellungen → Überwachung → Abfrageperiode auf
         250 ms stellen. Ändern sich die PSU-Werte in GpuMon dann häufiger als 1×/s?
   - [ ] In `MSI Afterburner\Plugins\Monitoring\PSU.cfg` unter `[Debug]` `EnableLog = 1`
         setzen und Afterburner neu starten. `PSU.log` enthält dann die Geräteliste, den
         Handshake-String und **einmalig alle 30 dekodierten Wörter des READ_ALL-Blocks**.
         Damit die unbekannten Wörter 22–30 deuten (5.4). Danach wieder `EnableLog = 0`.
   - [ ] Als Administrator in Process Explorer bzw. `handle.exe -a -p HWiNFO64` prüfen, ob
         HWiNFO ein Handle auf `\BaseNamedObjects\MSI_PSU_Mutex` und auf das HID-Gerät hält.
2. **Messtest (nur nach ausdrücklicher Freigabe durch den Benutzer)**, siehe Abschnitt 6.
3. **`HidSource` implementieren** (Abschnitt 7). Rate je nach Messtest: 2–5 Hz, wenn die
   Firmware schneller als 1 Hz aktualisiert und keine Kollisionen auftreten, sonst 1 Hz.
4. **Optional `HwinfoSource`** als weiterer Fallback (Abschnitt 8).
5. **Anzeige erweitern** (Abschnitt 9).

---

## 4. Gerät und Zugriff

- USB `VID_0DB0 & PID_AA6F` („MPG Ai1300TS“), HID-Interface `HID\VID_0DB0&PID_AA6F\…`
  **[sicher]**.
- HID-Caps: UsagePage `0xFF00` (herstellerdefiniert), Usage `0x0001`,
  Input = Output = **65 Byte** (Report-ID 0 + 64 Nutzbytes), **keine Feature-Reports**
  **[sicher]**.
- Weitere Modelle mit gleichem Protokoll: `0xE749` MEG Ai1000P, `0x56D4` MEG Ai1300P
  (Multi-Rail-Layout), `0xC9EB` MEG Ai1600T (nur Sammelwerte), `0x808C` MPG Ai1600TS
  (gleiches Layout wie Ai1300TS) **[sicher]**.
- Wer heute schon zugreift **[sicher/abgeleitet]**:

  | Programm | Takt | Mutex `Global\MSI_PSU_Mutex` |
  |---|---|---|
  | Afterburner PSU.dll | 1 s (`HwPollPeriod`) | **nur beim Handshake**, zyklische E0/C1-Abfragen **ohne** Mutex; `hid_read` blockiert **ohne Timeout** |
  | MSI Center (Modul „Power Supply Unit“) | Profil-Timer 60 s, Monitor-Takt [offen] | um **jede** Transaktion, öffnet das Gerät pro Transaktion neu |
  | HWiNFO64 8.60 | 2 s | [offen] |

  GpuMon wäre der vierte Leser.
- Windows stellt jeden Input-Report **jedem** offenen Handle zu. Antworten anderer
  Programme landen also auch in GpuMons Queue (und umgekehrt).

## 5. Protokoll

### 5.1 Transaktion [sicher]

```
HidD_FlushQueue(handle)                 // alte Input-Reports verwerfen
out = [0u8; 65]
out[0] = 0x00                           // Report-ID
out[1] = typ                            // 0x51 READ | 0xFA HANDSHAKE   (0x50 WRITE: NIE)
out[2] = register
WriteFile(handle, out, 65)
ReadFile(handle, in, 65)                // mit Timeout (overlapped)!
// Unter Windows/WriteFile+ReadFile steht in[0] = Report-ID (0); hidapi schneidet sie ab.
// Echo prüfen: in[1] == typ && in[2] == register  (bei hidapi: in[0], in[1])
// Nutzdaten ab in[3]                              (bei hidapi: ab in[2])
```

In der Analyse sind die Offsets aus Sicht von hidapi angegeben (ohne Report-ID-Byte).
**Alle Byte-Angaben unten sind ebenfalls ohne Report-ID gezählt.** Bei rohem `ReadFile`
also +1 rechnen.

### 5.2 Befehlstypen und Register [sicher, Namen aus MSI Center `cPSU.CMD`]

Typ (Byte 1): `0x51` READ, `0x50` WRITE (**nie senden**), `0xFA` HANDSHAKE (mit Register
`0x51`, Antwort ab Byte 2 ein ASCII-String mit Kennung/Version).

| Register | Name | Inhalt | für GpuMon |
|---|---|---|---|
| `0xE0` | READ_ALL | alle Messwerte in einem Block (5.3) | **zyklisch** |
| `0xC1` | OCP12V2x6_STATE | Hardware-OCP-Status je Stecker (5.5) | **zyklisch** |
| `0xE1` | READ_ALERTSTATUS | Schutz-Alarme als Bitfeld (Layout [offen]) | selten (10 s) |
| `0x20` | VIN | AC-Eingangsspannung, LINEAR11 | selten (10 s) |
| `0xD0` / `0xD1` | RUNTIME_NOW / TOTAL | Laufzeit (Format [offen]) | optional |
| `0x10`–`0x13` | MFR_ID / MODEL / REVISION / SERIAL | ASCII | einmalig |
| `0x42` | READ_FANDUTY | Lüfter-Duty (Format [offen]) | optional |
| `0xC0` | OCP-Konfiguration | Schwellen und Zeiten, je u8 (Layout [offen]) | vorerst nicht (nicht in der Whitelist) |
| `0x41`, `0x43`, `0xC2` | Lüftermodus, Zero-Fan, Summer | Einstellungen | **nicht nötig** |
| `0xF1` | SAVE_SETTING | Einstellungen dauerhaft speichern | **niemals** |

Die Einzelregister `0x24`–`0x2C`, `0x30`, `0x40` liefern dasselbe wie READ_ALL und werden
nicht gebraucht.

### 5.3 READ_ALL (`0x51 0xE0`) beim Ai1300TS [sicher]

Wort *i* = Little-Endian-u16 an Byte `2 + 2·i`, jeweils LINEAR11 (5.4).

| Wort | Bytes | Wert | Einheit | MAHM-Name |
|---|---|---|---|---|
| 0 | 2–3 | 12 V Strom | A | PSU 12V current |
| 1 | 4–5 | 5 V Strom | A | PSU 5V current |
| 2 | 6–7 | 3,3 V Strom | A | PSU 3.3V current |
| 3–8 | 8–19 | Stecker 1, Pin 1–6 | A | PSU 12VHPWR1_1…6 current |
| 9–14 | 20–31 | Stecker 2, Pin 1–6 | A | PSU 12VHPWR2_1…6 current |
| 15 | 32–33 | Ausgangsleistung (DC, `P_Out`) | W | PSU power |
| 16 | 34–35 | Effizienz | % | PSU efficiency |
| 17 | 36–37 | Temperatur | °C | PSU temperature |
| 18 | 38–39 | Lüfter | RPM | PSU fan speed |
| 19 | 40–41 | 12 V Spannung | V | PSU 12V voltage |
| 20 | 42–43 | 5 V Spannung | V | PSU 5V voltage |
| 21 | 44–45 | 3,3 V Spannung | V | PSU 3.3V voltage |
| 22–30 | 46–63 | unbekannt | [offen] | – (mit `EnableLog` deuten) |

`PSU power` ist die DC-Ausgangsleistung. Die AC-Aufnahme ist ≈ `P_Out / (Eff / 100)`.

### 5.4 LINEAR11 [sicher]

```rust
fn linear11(w: u16) -> f32 {
    let exp = (w as i16) >> 11;                     // 5 Bit, mit Vorzeichen
    let mant = ((w << 5) as i16) >> 5;              // 11 Bit, mit Vorzeichen (PMBus-konform)
    (mant as f32 * (exp as f32).exp2()).max(0.0)    // Messwerte sind >= 0
}
```

PSU.dll wertet die Mantisse immer ohne Vorzeichen aus (`w & 0x7FF`). Für Werte ≥ 0 ist
das identisch. Das Raster bestätigt den Exponenten: Pins in 1/16 A (−4), Spannungen in
1/256 V (−8), live auch in HWiNFO sichtbar.

Testvektoren aus Live-Werten: 6,5625 A = 105 × 2⁻⁴ → `0xE069`; 12,078 V ≈ 3092 × 2⁻⁸.
Beim Messtest echte Rohwörter als Fixture sichern.

### 5.5 OCP-Status (`0x51 0xC1`) [sicher/abgeleitet]

- Stecker 1: Statusbyte an **Byte 2**, Stecker 2: an **Byte 23** (`2 + 0x15`) [sicher].
- Werte (Enum `OCP_12V2x6_Status`): `0` Normal, `1` OCP_Point (Pin-Überstrom),
  `2` Current_Diff (Schieflast), `3` OCP_18A [sicher].
- Danach folgen je Stecker `RunTime` (u32), `TotalTime` (u32) und 6 Pinströme (u16),
  insgesamt 21 Byte pro Block [abgeleitet]. Ob Momentanwerte oder Werte beim Auslösen
  gemeint sind, ist [offen].
- Der Status wird von der Firmware gesetzt und hält vermutlich bis zur Rücksetzung
  [abgeleitet]. Kurze Überströme sieht man damit auch bei 1 Hz. Das ist wertvoller als
  eine hohe Abfragerate.
- Afterburner liest `C1` nur bei `SoftwareAlarm = 0` in `PSU.cfg` (beim Benutzer: 0).
  Bei `1` berechnet das Plugin den Alarm selbst (`MaxCurrent` / `CurrentDif`).

### 5.6 Sicherheitsregel: Whitelist [verbindlich]

Lesen und Schreiben unterscheiden sich am Typbyte. Feature-Reports gibt es nicht. Deshalb:

- Fest einkompilierte Whitelist, **keine generische Sende-API**:
  `[0xFA, 0x51]` und `[0x51, r]` mit `r ∈ {0xE0, 0xC1, 0xE1, 0x20, 0xD0, 0xD1, 0x10, 0x11, 0x12, 0x13, 0x42}` (wie in der Analyse).
- Alle übrigen Bytes des Reports sind 0.
- Nur Register, die Afterburner oder MSI Center selbst lesen. Ob ein Lesebefehl auf ein
  unbekanntes Register Nebenwirkungen hat, ist nicht prüfbar.
- Unit-Test, der sicherstellt, dass der Report-Builder nur Whitelist-Kombinationen erzeugt.

---

## 6. Messtest (vor der Implementierung, nur mit Freigabe)

Kleines separates Testprogramm, z. B. `examples/hid_probe.rs` oder als `--hid-probe`-Schalter.
Es sendet ausschließlich Whitelist-Befehle unter dem Mutex.

1. Gerät öffnen, Handshake, `0x10`–`0x13` einmalig lesen und protokollieren.
2. 30 s lang `0x51 0xE0` mit 10 Hz. Rohwörter mit Zeitstempel protokollieren und zählen, wie
   oft sich die Pinwörter ändern → tatsächliche Aktualisierungsrate der Firmware.
3. `C1`, `E1`, `D0`, `D1`, `0x20`, `0x42` je einmal roh protokollieren und mit der
   Anzeige von MSI Center / HWiNFO vergleichen → Formate klären.
4. Parallel prüfen, ob Afterburner Fehler meldet: Afterburner-Monitoring auf Lücken
   beobachten bzw. `EnableLog = 1` und in `PSU.log` nach „Failed to update data“ suchen.
   Achtung: PSU.dll liest ohne Timeout. Würde die Firmware bei Kollision eine Anfrage
   verwerfen, hinge Afterburners Abfrage-Thread. Abbruchkriterium: Afterburner-PSU-Werte
   frieren ein → Test sofort beenden, Afterburner neu starten.
5. Prüfen, ob `CreateFileW` mit Lese- und Schreibrecht **ohne Admin** klappt (Afterburner
   läuft erhöht, GpuMon nicht). Ohne Schreibrecht ist kein Request möglich, dann bleibt
   nur MAHM bzw. HWiNFO.
6. Ergebnis als Fixture sichern (`tests/fixtures/e0_*.bin`, `c1_*.bin`).

## 7. Implementierung `HidSource` (`src/source/hid.rs`)

- **Gerätesuche:** `SetupDiGetClassDevsW(&GUID_DEVINTERFACE_HID, …, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)`
  → `SetupDiEnumDeviceInterfaces` → `SetupDiGetDeviceInterfaceDetailW` → Pfad öffnen →
  `HidD_GetAttributes` (VID `0x0DB0`, PID `0xAA6F`, optional `0x808C`) und `HidD_GetPreparsedData`
  + `HidP_GetCaps` (UsagePage `0xFF00`, Report-Längen 65).
- **Öffnen:** `CreateFileW(GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
  OPEN_EXISTING, FILE_FLAG_OVERLAPPED)`. Handle **offen halten** (wie PSU.dll). Bei Fehlern
  schließen und mit Backoff 2–5 s neu suchen (wie `MahmSource`).
- **Transaktion:**
  1. `CreateMutexW(None, false, "Global\\MSI_PSU_Mutex")` einmalig, pro Transaktion
     `WaitForSingleObject(mutex, 200)`. Bei `WAIT_TIMEOUT` den Zyklus auslassen, bei
     `WAIT_ABANDONED` normal weitermachen. Danach `ReleaseMutex`.
  2. `HidD_FlushQueue` → `WriteFile` (overlapped, Timeout ~100 ms) → `ReadFile` (overlapped,
     Timeout ~100 ms, sonst `CancelIoEx`).
  3. Echo prüfen. Fremde Antworten (anderes Register) verwerfen und höchstens 2 weitere
     Reports lesen, dann aufgeben. Bei Fehlschlag den letzten Wert halten.
- **Takt:** zyklisch `E0` + `C1` (1 Hz, nach Messtest ggf. 2–5 Hz), alle 10 s `0x20` und
  `E1`. Möglichst dieselben Befehle wie Afterburner, dann sind fremde Antworten in der
  eigenen Queue inhaltlich gleich und harmlos.
- **Abbildung:** LINEAR11 → `PsuSnapshot` nach Tabelle 5.3. `GPU_W` kommt aus HID nicht;
  bei Bedarf weiterhin aus MAHM oder weglassen.
- **Quellwahl** in `main.rs`: `--source auto|hid|mahm|hwinfo` (Standard `auto`: HID, sonst
  MAHM). In der INI merken. `name()` zeigt die aktive Quelle im Hinweis an.
- **Firmware-Schutz:** Handshake-String und PID prüfen. Bei unbekannter Version weiter
  lesen, aber in der UI „ungetestete Firmware“ anzeigen.
- **Cargo-Features:** `Win32_Devices_HumanInterfaceDevice`,
  `Win32_Devices_DeviceAndDriverInstallation`, `Win32_Storage_FileSystem`, `Win32_System_IO`.
  Keine weiteren Crates.
- **Tests:** LINEAR11 (Testvektoren), Parsen der Fixture-Blöcke E0/C1, Whitelist-Test,
  Echo-Prüfung.
- **Ressourcen nach dem Einbau neu messen** (Ziel unverändert: < 15 MB WS, ≈ 0 % CPU,
  0 GPU). Bei höherer Abfragerate nur Peaks mitführen und mit 2–4 Hz neu zeichnen.

## 8. Optional: `HwinfoSource` (`src/source/hwinfo.rs`)

- Shared Memory `Global\HWiNFO_SENS_SM2` lesen (`OpenFileMappingW` + `FILE_MAP_READ`), in
  HWiNFO muss „Shared Memory Support“ aktiv sein (beim Benutzer: `SensorsSM=1`).
- Header (öffentlich dokumentiert, beim Umsetzen gegen das HWiNFO-SDK prüfen): Signatur
  `'HWiS'`, Version 2, Revision 1, `poll_time` (i64), Offsets/Größen/Anzahl der Sensor-
  (hier 29 × 392 Byte) und Reading-Sektion (163 × 460 Byte), Polling-Periode (2000 ms).
- Reading: Typ, Sensorindex, Reading-ID, Label (ANSI und UTF-8), Einheit,
  `Value/Min/Max/Avg` als f64.
- Sensor „MSI MPG Ai1300TS“ (ID `0xF5000917`). Pins z. B. Reading-IDs
  `0x04000010…15` (Label „+12V-2x6_1 Pin1…6 Current“). **Per Sensorname und Label/ID
  suchen, nicht per Index.** Stale-Erkennung über `poll_time`, wie bei MAHM.
- Liefert Pins, Rails, Temperatur, Lüfter, P_Out, Effizienz und Total Runtime. **Keinen**
  Alarm- oder OCP-Status. Stecker 2 erscheint nur, wenn er belegt ist.
- In der **kostenlosen** HWiNFO-Version ist Shared Memory zeitlich begrenzt
  (vermutlich 12 h, nicht verifiziert). Beim Benutzer liegt eine Lizenz vor.

## 9. Anzeige-Erweiterungen (nach `HidSource`)

- Statuszeile: ALARM S1/S2 durch den echten Grund ersetzen bzw. ergänzen
  („PIN-OCP“, „SCHIEFLAST HW“, „18 A“). Gelatcht wie bisher.
- Neue Kennzahl „AC“: Eingangsspannung (V) und AC-Leistung (`P_Out / Eff`).
- Schutz-Alarme aus `E1` (OVP/UVP/OCP/OPP/OTP/Lüfter/AC) als Sammel-Chip, sobald das
  Bitlayout geklärt ist.
- Quelle im Hinweis/Tooltip anzeigen („HID“, „Afterburner“, „HWiNFO“).
- Bei schnellerer Abfrage: Peaks pro Pin aus allen Samples, Anzeige weiter mit ≤ 4 Hz.

## 10. Offene Fragen

| Frage | Klärung |
|---|---|
| Bedeutung der READ_ALL-Wörter 22–30 | `EnableLog = 1` (3., Schritt 1) |
| Aktualisierungsrate der Firmware | Messtest 6.2 |
| Formate von C1-Restbytes, E1, D0/D1, 0x42 | Messtest 6.3 |
| Layout von C0 (OCP-Schwellen); soll C0 in die Whitelist? | erst nach Klärung, wie MSI Center es liest |
| Firmware-Verhalten bei parallelen Anfragen | Messtest 6.4 |
| Gerätezugriff ohne Admin | Messtest 6.5 |
| Nutzt HWiNFO den MSI-Mutex? | Process Explorer / `handle.exe` als Admin |
| Monitor-Takt von MSI Center | nicht entscheidend (hält sich an Mutex) |

## 11. Risiken

| Risiko | Gegenmaßnahme |
|---|---|
| Schreibbefehl verändert Lüfter-/OCP-/Summer-Einstellungen oder speichert sie (`0xF1`) | Whitelist (5.6), keine generische Sende-API, Test |
| Afterburners Abfrage-Thread hängt bei einer Kollision (liest ohne Timeout) | niedrige Rate, gleiche Befehle wie Afterburner, Messtest mit Abbruchkriterium |
| Afterburner verliert gelegentlich einen Zyklus (Echo-Mismatch) | harmlos (Wert 1 s alt), nur E0/C1 senden |
| Firmware-Update ändert das Layout | Handshake/PID prüfen, Hinweis „ungetestet“, MAHM als Fallback |
| Kein Schreibrecht ohne Admin | dann HID-Quelle deaktivieren, MAHM/HWiNFO nutzen |
| MSI Center überschreibt beim Start Lüftereinstellungen | betrifft GpuMon nicht, nur zur Kenntnis |

## 12. Fertig, wenn

- [ ] GpuMon zeigt mit `--source hid` dieselben Werte wie Afterburner/HWiNFO (Abgleich wie in v1).
- [ ] Läuft ohne Afterburner, fällt bei fehlendem Gerät auf MAHM zurück und verbindet sich
      automatisch neu.
- [ ] Hardware-OCP-Status mit Grund wird angezeigt und gelatcht.
- [ ] Afterburner, MSI Center und HWiNFO laufen parallel ohne Aussetzer (Afterburner-Log sauber).
- [ ] Nur Whitelist-Befehle werden gesendet (Test vorhanden).
- [ ] Verbrauch gemessen und im README aktualisiert (< 15 MB WS, ≈ 0 % CPU, 0 GPU).

---

## 13. Kleinere offene Punkte (unabhängig davon)

- [ ] Wiederverbinden mit dem echten Afterburner testen: beenden → Hinweis, neu starten →
      Werte nach ca. 2 s wieder da. Bisher nur mit simulierter Section getestet (`--mahm-name`).
- [ ] Optional: Stecker 2 immer anzeigen (wie MSI Center), statt nur, wenn er Strom führt.

## 14. Quellen

- `docs/hid-protokoll-analyse.md`: vollständige Analyse mit Belegen (PSU.dll aus
  Afterburner 4.6.7, `API_Power Supply Unit.dll` 1.0.0.38 aus MSI Center, MSI-Center-Log,
  HWiNFO64 8.60).
- `HANDOVER.md`: MAHM-Layout und ursprüngliche Anforderungen.
- `README.md`: Architektur von v1, Entscheidung für GDI, Messwerte.
