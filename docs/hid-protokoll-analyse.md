# Analyse: Netzteil MSI MPG Ai1300TS PCIE5 direkt per USB-HID auslesen

Stand: 12.11.2026. Grundlage ist eine rein statische Analyse. **An das Netzteil wurde
nichts gesendet**, weder Output- noch Feature-Reports. Die einzige Berührung mit dem
Gerät war eine Abfrage der HID-Capabilities über einen Handle mit Zugriffsrecht 0
(`HidD_GetPreparsedData` / `HidP_GetCaps`). Dabei wird nichts übertragen.

Untersuchte Artefakte:

| Datei | Version / SHA-256 | Rolle |
|---|---|---|
| `MSI Afterburner\Plugins\Monitoring\PSU.dll` | AB 4.6.7, `5fe8c4e8…37fd8` | Afterburner-Plugin (C++/MFC, x86), nutzt `hidapi.dll` 0.15.0 |
| `MSI Center\Power Supply Unit\API_Power Supply Unit.dll` | 1.0.0.38, `f7833895…74bb` | MSI-Center-Modul (.NET), nutzt `MsiHid.dll` |
| MSI-Center-Log `Log\Power Supply Unit\Library_11_12_2026.log` | – | Laufzeitverhalten von MSI Center |

Kennzeichnung im Text: **[beobachtet]** = direkt im Code bzw. in den Metadaten
gesehen, **[abgeleitet]** = Schlussfolgerung aus mehreren Beobachtungen, **[offen]**
= nicht geklärt.

---

## 1. Ergebnis in Kürze

- Das Protokoll ist einfach und für das Lesen vollständig verstanden: 64-Byte-Output-Report
  `[0x51, Register]` (READ), Antwort als 64-Byte-Input-Report `[0x51, Register, Daten…]`.
  Alle Messwerte, die Afterburner anzeigt, kommen aus **einer einzigen Abfrage** (`0x51 0xE0`,
  „READ_ALL“) und liegen als PMBus-LINEAR11-Wörter vor.
- **Lesen und Schreiben sind sauber getrennt.** Byte 1 ist der Befehlstyp:
  `0x51` = READ, `0x50` = WRITE, `0xFA` = HANDSHAKE. Feature-Reports hat das Gerät
  gar nicht. Wenn GpuMon nur `0x51` und `0xFA 0x51` sendet, kann es keine Einstellung
  verändern [abgeleitet, siehe 2.6].
- Der Mutex `Global\MSI_PSU_Mutex` wird **von MSI Center um jede Transaktion** gesperrt,
  **von Afterburners PSU.dll aber nur beim Handshake**. Das zyklische Abfragen von
  Afterburner läuft ohne Mutex.
- Das Netzteil liefert mehr als Afterburner zeigt: AC-Eingangsspannung, Betriebsstunden,
  detaillierte Schutz-/Alarm-Bits (OVP/UVP/OCP/OPP/OTP/Lüfter/AC, OCP pro Pin),
  OCP-Schwellen, Lüfter-Duty, Hersteller-/Seriennummer und 9 weitere, unbekannte Wörter im
  READ_ALL-Block.
- Ob das Netzteil seine Werte schneller als 1 Hz aktualisiert, lässt sich statisch nicht
  klären. Dafür braucht es einen kurzen, rein lesenden Messtest (Abschnitt 6).
- **Empfehlung: umsetzen, aber in Stufen** und erst nach dem Messtest. Details in Abschnitt 8.

---

## 2. Protokoll

### 2.1 Gerät und Transport [beobachtet]

- USB `VID_0DB0 & PID_AA6F`, HID-Interface `HID\VID_0DB0&PID_AA6F\…`.
- HID-Caps: UsagePage `0xFF00` (herstellerdefiniert), Usage `0x0001`,
  **InputReportByteLength = 65, OutputReportByteLength = 65, FeatureReportByteLength = 0**.
  Es gibt also nur Report-ID 0 mit 64 Nutzbytes in beide Richtungen und keine Feature-Reports.
- PSU.dll findet das Gerät per `hid_enumerate(0,0)` und filtert auf VID `0x0DB0` und eine
  der PIDs unten. Geöffnet wird es mit `hid_open_path` (hidapi öffnet unter Windows per
  `CreateFileW` mit Lese- und Schreibzugriff). MSI Center öffnet es dagegen **für jede
  Transaktion neu** (`openMyDevice_Overlapped` → Write → Read → `CloseDevice`).

Unterstützte Modelle (Konstanten aus beiden DLLs, identisch):

| PID | Modell | PSU.dll-Layout |
|---|---|---|
| `0xE749` | MEG Ai1000P | 1 (Multi-Rail 12V1…12V5) |
| `0x56D4` | MEG Ai1300P | 1 |
| `0xC9EB` | MEG Ai1600T | 2 (nur Sammelwerte) |
| **`0xAA6F`** | **MPG Ai1300TS** | **3 (12V-2x6-Pinströme)** |
| `0x808C` | MPG Ai1600TS | 3 |

### 2.2 Rahmen einer Transaktion [beobachtet]

Quelle: PSU.dll `FUN_10001b30` (Transaktion), `FUN_10001bd0` (Transaktion unter Mutex).

```
HidD_FlushQueue(handle)                    // alte Input-Reports verwerfen
out[0..64] = 0
out[0] = 0x00                              // Report-ID
out[1] = typ                               // 0x51 READ, 0xFA HANDSHAKE (0x50 WRITE nur MSI Center)
out[2] = register
hid_write(handle, out, 64)
n = hid_read(handle, in, 64)               // blockierend, ohne Timeout (!)
gültig, wenn in[0] == typ && in[1] == register
Nutzdaten ab in[2]
```

`hid_read` liefert unter Windows die Daten ohne Report-ID-Byte. Das Echo steht deshalb
in `in[0]` und `in[1]`.

### 2.3 Befehle (Register) [beobachtet, Enum `cPSU.CMD` aus MSI Center]

| Typ/Byte 1 | Wert | Bedeutung |
|---|---|---|
| `HANDSHAKE` | `0xFA` | mit Register `0x51` – liefert ab Byte 2 einen ASCII-String (Kennung/Version) |
| `READ` | `0x51` | Lesen eines Registers |
| `WRITE` | `0x50` | Schreiben eines Registers (**nie senden**) |

| Register | Name in MSI Center | Inhalt | von PSU.dll genutzt |
|---|---|---|---|
| `0x10` | MFR_ID | Hersteller („MSI“) | nein |
| `0x11` | MFR_MODEL | „MPG Ai1300TS“ | nein |
| `0x12` | MFR_REVISION | „1.0“ | nein |
| `0x13` | MFR_SERIAL | Seriennummer | nein |
| `0x20` | VIN | **AC-Eingangsspannung** | nein |
| `0x22`/`0x23` | VOUT/IOUT_12V5 | Einzel-Rail | nein |
| `0x24`/`0x25` | VOUT/IOUT_12 | 12 V Spannung/Strom | nein (kommt aus E0) |
| `0x26`/`0x27` | VOUT/IOUT_5 | 5 V | nein (E0) |
| `0x28`/`0x29` | VOUT/IOUT_3_3 | 3,3 V | nein (E0) |
| `0x2A` | POUT | Ausgangsleistung | nein (E0) |
| `0x2B` | EFFICIENCY | Wirkungsgrad | nein (E0) |
| `0x2C` | IOUT_12V2x6 | Pinströme | nein (E0) |
| `0x30` | TEMPERATURE | Temperatur | nein (E0) |
| `0x40` | FAN_SPEED | Lüfterdrehzahl | nein (E0) |
| `0x41` | FAN_SETTING | Lüftermodus (lesen ok, schreiben = Einstellung) | nein |
| `0x42` | READ_FANDUTY | Lüfter-Duty | nein |
| `0x43` | ZERO_FAN_ENABLE | Zero-Fan an/aus | nein |
| `0xC0` | OCP12V2x6_CONFIG / MULTIRAIL_OCP | OCP-Schwellen und Zeiten (lesen ok) | nein |
| `0xC1` | OCP12V2x6_STATE | **Hardware-Alarmstatus 12V-2x6** | ja |
| `0xC2` | BUZZER_SWITCH | Summer | nein |
| `0xD0` | RUNTIME_NOW | Laufzeit seit Einschalten | nein |
| `0xD1` | RUNTIME_TOTAL | Gesamtlaufzeit | nein |
| **`0xE0`** | **READ_ALL** | **alle Messwerte in einem Block** | **ja** |
| `0xE1` | READ_ALERTSTATUS | Schutz-/Alarm-Bitfeld | nein |
| `0xF1` | SAVE_SETTING | Einstellungen dauerhaft speichern (**gefährlich**) | – |

PSU.dll enthält zusätzlich eine Funktion für das Lesen eines einzelnen Registers
(`[0x51, reg]` → ein LINEAR11-Wort ab Byte 2, `FUN_10001ab0`), die aber nirgends
aufgerufen wird.

### 2.4 Antwort auf `0x51 0xE0` (READ_ALL) beim Ai1300TS [beobachtet]

Die Antwort wird als 64-Byte-Block zwischengespeichert. Wert *i* ist das
Little-Endian-Wort an `in[2 + 2·i]` (`FUN_10001850`). Zuordnung für Layout 3 (aus
`GetSourceDesc` und `GetSourceData`):

| Wort | Byte | Afterburner-Quelle | Einheit |
|---|---|---|---|
| 0 | 2–3 | PSU 12V current | A |
| 1 | 4–5 | PSU 5V current | A |
| 2 | 6–7 | PSU 3.3V current | A |
| 3–8 | 8–19 | PSU 12VHPWR1_1 … 1_6 current | A |
| 9–14 | 20–31 | PSU 12VHPWR2_1 … 2_6 current | A |
| 15 | 32–33 | PSU power | W |
| 16 | 34–35 | PSU efficiency | % |
| 17 | 36–37 | PSU temperature | °C |
| 18 | 38–39 | PSU fan speed | RPM |
| 19 | 40–41 | PSU 12V voltage | V |
| 20 | 42–43 | PSU 5V voltage | V |
| 21 | 44–45 | PSU 3.3V voltage | V |
| 22–30 | 46–63 | **ungenutzt** (das Debug-Log gibt Wort 0–29 einmalig aus) | [offen] |

Hinweise:
- Die Rails 12V1…12V5 aus der Quellenliste gibt es nur bei Layout 1 (Ai1000P/Ai1300P).
  Beim Ai1300TS fragt das Plugin sie nicht ab.
- „PSU power“ heißt in MSI Center `P_Out`. Es ist also mit großer Wahrscheinlichkeit die
  **DC-Ausgangsleistung** [abgeleitet aus dem Feldnamen]. Die AC-Aufnahme wäre dann
  `P_Out / Eff`. Das beantwortet die offene Frage aus `HANDOVER.md` vorläufig.

### 2.5 Umrechnung (LINEAR11) [beobachtet, `FUN_10001390`, Assembly geprüft]

```
exp  = (i16)wort >> 11          // 5-Bit-Exponent mit Vorzeichen
mant = wort & 0x7FF             // 11-Bit-Mantisse, OHNE Vorzeichenerweiterung
wert = mant * 2^exp
```

Das entspricht PMBus LINEAR11 bis auf die Mantisse: PSU.dll behandelt sie immer als
vorzeichenlos. Der Code sieht zwar ein Flag für „signed“ vor, beide Zweige maskieren aber
identisch mit `0x7FF`. Für die Messwerte (alle ≥ 0) ist das egal. GpuMon sollte die
Mantisse trotzdem standardkonform mit Vorzeichen auswerten und negative Werte auf 0 klemmen.

Afterburner skaliert nichts weiter. Prozent, °C und RPM kommen direkt aus LINEAR11.

### 2.6 Alarm 12VHPWR1/2 [beobachtet]

Die Einstellung `SoftwareAlarm` in `PSU.cfg` (Standard 1, beim Benutzer **0**) entscheidet:

- **SoftwareAlarm = 0 (Benutzer):** Das Plugin liest zusätzlich `0x51 0xC1` (OCP12V2x6_STATE).
  Alarm 1 = `in[2] != 0`, Alarm 2 = `in[2 + 0x15] = in[23] != 0`.
- **SoftwareAlarm = 1:** Es wird berechnet. Alarm = (max. Pinstrom > `MaxCurrent`) oder
  (max − min > `CurrentDif`). Fehlt der Schlüssel, gelten die Vorgaben `MaxCurrent` 9,5 und
  `CurrentDif` 5,0.

Das Statusbyte ist in MSI Center das Enum `OCP_12V2x6_Status`: `0` Normal, `1` OCP_Point
(Pin-Überstrom), `2` Current_Diff (Schieflast), `3` OCP_18A. Laut MSI-Center-Struct
`OCP_State_12V2x6` folgen auf jedes Statusbyte `RunTime` (u32), `TotalTime` (u32) und
6 Pinströme. Bei 1 + 4 + 4 + 6·2 = 21 = 0x15 Byte passt das genau zum Abstand, den PSU.dll
verwendet [abgeleitet]. Die genaue Bedeutung von RunTime/TotalTime und der Pinströme in
diesem Block (Momentanwert oder Wert beim Auslösen) ist **[offen]**.

`0xE1` (READ_ALERTSTATUS) ist ein Bitfeld mit den Indizes aus dem Enum `AlertStatus`
(Ai1300TS-Variante): OVP 12/5/3,3 V, UVP 12/5/3,3 V, OCP 12/5/3,3 V, OCP 12V1…12V5, OPP,
OTP, Fan, AC, OCP_12V2x6_1_1…2_6. Bitreihenfolge und Länge des Felds sind **[offen]**.
MSI Center benutzt modellabhängige Index-Tabellen (`Alert_Index_P/T/TS`).

### 2.7 Lesen und Schreiben sind getrennt [beobachtet und abgeleitet]

- Der Befehlstyp steht explizit in Byte 1: `0x51` READ, `0x50` WRITE. Afterburner sendet
  ausschließlich `0x51 0xE0`, `0x51 0xC1` und den Handshake `0xFA 0x51`.
- MSI Center hat getrennte Methoden `Get` und `Set`. Schreibende Funktionen
  (`SET_FAN_SETTING`, `SET_ZERO_FAN`, `SET_OCP12V2x6`, `SET_MULTIRAIL_OCP`,
  `BUZZER_SWITCH`, `SAVE_SETTING`) laufen über `Set`.
- Es gibt keine Feature-Reports. Schreibzugriffe sind also nur über Output-Reports mit
  `0x50` (und vermutlich `0xF1`) möglich.
- **Regel für GpuMon:** Eine fest einkompilierte Whitelist, die nur
  `[0xFA, 0x51]` und `[0x51, r]` mit `r ∈ {0xE0, 0xC1, 0xE1, 0x20, 0xD0, 0xD1, 0x10–0x13, 0x42}`
  erlaubt. Alle anderen Bytes des Reports sind 0. Mehr ist nicht nötig.

Restrisiko: Ob die Firmware auf einen *Lese*befehl mit unbekanntem Register eine
Nebenwirkung hat, ist nicht prüfbar. Die Whitelist enthält deshalb nur Register, die
Afterburner oder MSI Center selbst lesen.

---

## 3. Mutex und Koexistenz

### 3.1 Wer greift zu [beobachtet]

Auf dem Rechner laufen gleichzeitig **MSI Afterburner** (PSU.dll) und **MSI Center**
(`MSI_Central_Service`, `MSI.CentralServer`, Modul „Power Supply Unit“, SDK 3.2026.1013.01,
am 12.11.2026 um 13:31 installiert). MSI Center hat das Netzteil erkannt („MPG Ai1300TS PCIE5“,
Seriennummer im Log) und fragt es zyklisch ab. Als dritter Leser kommt **HWiNFO64** hinzu
(Abschnitt 9).

### 3.2 Mutex-Nutzung

| Programm | Mutex `Global\MSI_PSU_Mutex` |
|---|---|
| PSU.dll | `CreateMutexA` + `WaitForSingleObject(INFINITE)` **nur beim Handshake** (`FUN_10001bd0`). Die zyklischen Abfragen E0/C1 (`FUN_10001ce0`) laufen **ohne Mutex**. |
| MSI Center | `new Mutex` + `WaitOne` um **jede** Transaktion in `Get`, `Set`, `CONNECT_PSU`, `DISCONNECT` und `CheckConnectedDevice` (Öffnen → Write → Read → Schließen). |

### 3.3 Was bei gleichzeitigen Abfragen passiert [abgeleitet]

- Windows stellt jeden Input-Report **jedem** offenen Handle zu. Jede Antwort landet also
  bei allen Programmen, die das Gerät offen haben.
- PSU.dll verwirft vor jedem Write alte Reports (`HidD_FlushQueue`) und prüft das Echo
  `in[0..1]`. Erwischt es die Antwort eines anderen Programms auf **denselben** Befehl,
  übernimmt es diese Daten. Das ist harmlos, weil der Inhalt gleich ist. Bei einem
  **anderen** Befehl schlägt die Echo-Prüfung fehl, und Afterburner verwirft diesen Zyklus
  („Failed to update data“). Die eigene Antwort wird beim nächsten Flush entsorgt.
- **Kritisch:** `hid_read` in PSU.dll blockiert ohne Timeout. Sollte die Firmware bei zwei
  fast gleichzeitigen Anfragen eine verwerfen, hängt der Abfrage-Thread von Afterburner
  dauerhaft. Ob die Firmware Anfragen puffert, ist **[offen]**. MSI Center und Afterburner
  laufen heute schon parallel, ohne dass das sichtbar aufgefallen ist. Das spricht dafür,
  dass die Firmware Anfragen nacheinander beantwortet. Im MSI-Center-Log stehen allerdings
  vereinzelte Fehler wie „GET_RUNTIME_TOTAL is failed“ (13:58:50, 14:15:50). Das könnten
  solche Kollisionen sein.

### 3.4 Folgerungen für GpuMon

1. Jede Transaktion unter `Global\MSI_PSU_Mutex` ausführen (`CreateMutexW`, `WaitForSingleObject`
   mit Timeout von z. B. 200 ms statt INFINITE, danach `ReleaseMutex`). Das schützt gegen MSI Center.
2. Gegen Afterburner hilft der Mutex nicht. Deshalb: Echo prüfen, `ReadFile` mit Timeout
   (overlapped, ~100 ms), bei Fehlschlag diesen Zyklus verwerfen und den letzten Wert halten.
3. Möglichst dieselben Befehle senden wie Afterburner (`E0`, `C1`). Dann sind fremde
   Antworten in der eigenen Queue und umgekehrt inhaltlich gleich und unschädlich.
4. Das Gerät dauerhaft offen halten (wie PSU.dll), nicht pro Transaktion öffnen
   (wie MSI Center). Das spart Systemaufrufe und Zeit.
5. Afterburner fragt im Takt `HwPollPeriod=1000` ms ab. Das Plugin holt E0/C1 nur, wenn sich
   Afterburners Zeitstempel ändert (`GetTimestamp`), also einmal pro Abfrageperiode.

---

## 4. Abfragerate und kurze Stromspitzen

- **Bekannt:** Afterburner fragt einmal pro Sekunde ab (Einstellung 1000 ms). Jede Abfrage ist
  eine HID-Transaktion von 64 Byte hin und 64 Byte zurück.
- **Unbekannt [offen]:** Wie oft die Firmware die Werte intern aktualisiert. Typische
  PMBus-Netzteil-Controller mitteln über einige 10–100 ms. Der Mikrocontroller liest den
  Leistungsteil über PMBus/I²C aus. Pro Abfrage sind also etliche Millisekunden realistisch.
- **Technisch machbar:** USB-Full-Speed-HID schafft pro Interrupt-Endpoint bis zu
  1000 Reports/s, praktisch begrenzt durch `bInterval` (nicht ausgelesen) und die Antwortzeit
  der Firmware. 5–10 Hz sollten problemlos gehen.
- **Was man damit sieht:** Lastspitzen moderner GPUs dauern 10 µs bis einige ms. Die sieht
  man über einen gemittelten Sensor auch mit 10 Hz nicht. Erkennbar wären Lastsprünge und
  Plateaus über mehr als ca. 100 ms, etwa Szenenwechsel, Shader-Kompilierung oder
  Boost-Spitzen. Für die eigentlichen Transienten ist die PSU-Telemetrie nicht gebaut.
  Dafür bräuchte es ein Messgerät wie WireView mit eigenem ADC.
- **Hardware-Alarm:** Das OCP-Statusbyte (`C1`) wird von der Firmware gesetzt und gilt bis
  zur Rücksetzung [abgeleitet aus RunTime/TotalTime]. Kurze Überströme, die die Firmware
  erkennt, sieht GpuMon damit auch bei 1 Hz. Das ist wahrscheinlich wertvoller als eine
  hohe Abfragerate.
- **Ressourcen:** Eine Transaktion besteht aus einem `WriteFile` und einem `ReadFile`
  (overlapped) plus `WaitForSingleObject` auf Mutex und Event. Selbst bei 10 Hz ist das im
  Bereich von 0,0x % CPU und ändert am Arbeitsspeicher praktisch nichts (keine zusätzliche
  DLL, `hid.dll`/`setupapi.dll` sind klein). GpuMon zeichnet ohnehin nur bei neuen Daten neu.
  Ein Neuzeichnen mit 10 Hz kostet mehr als das HID-Lesen. Für höhere Raten also nur die
  Peak-Werte mitführen und die Anzeige bei 2–4 Hz belassen.

---

## 5. Zusätzliche Werte, die Afterburner nicht weitergibt

| Wert | Quelle | Sicherheit |
|---|---|---|
| AC-Eingangsspannung | `0x51 0x20` (VIN), LINEAR11 | Register sicher, Format [abgeleitet] |
| AC-Leistung | berechnet `P_Out / Eff` | [abgeleitet] |
| OCP-Status pro Stecker (Normal / Pin-OCP / Schieflast / 18 A) | `C1` Byte 2 und 23 | Status sicher, Werte [beobachtet] |
| OCP-Laufzeiten / Pinströme im OCP-Block | `C1` Bytes 3–22 / 24–43 | [abgeleitet], Bedeutung [offen] |
| Schutz-Alarme OVP/UVP/OCP/OPP/OTP/Lüfter/AC | `0x51 0xE1` | Register sicher, Bitlayout [offen] |
| OCP-Schwellen und Auslösezeiten der Firmware | `0x51 0xC0` | Register sicher, Layout [offen] (Struct: IsEnable, Point Low/High, Diff Low/High, Trigger-/Warnzeiten, je u8) |
| Lüfter-Duty, Lüftermodus, Zero-Fan | `0x42`, `0x41`, `0x43` | Register sicher, Format [offen] |
| Laufzeit seit Einschalten / gesamt | `0xD0` / `0xD1` | Register sicher, Format [offen] |
| Hersteller, Modell, Revision, Seriennummer | `0x10`–`0x13` | ASCII [abgeleitet aus Log] |
| 9 unbekannte Wörter im READ_ALL-Block | `E0` Bytes 46–63 | [offen] |

---

## 6. Offene Fragen und wie man sie ohne Risiko klärt

1. **Inhalt der Wörter 22–30 in E0.** *Ohne eigene HID-Zugriffe:* In
   `Plugins\Monitoring\PSU.cfg` `[Debug] EnableLog = 1` setzen und Afterburner neu starten.
   Das Plugin schreibt dann in `Plugins\Monitoring\PSU.log` die Geräte-Liste, den
   Handshake-String und **einmalig alle 30 dekodierten Wörter** von E0. Danach wieder auf 0
   setzen.
2. **Aktualisierungsrate der Firmware.** Ein kleines, separates Testprogramm sendet nur
   `[0x51, 0xE0]` unter dem Mutex, z. B. 30 s lang mit 10 Hz, und protokolliert, wie oft sich
   die Rohwörter ändern. Das ist eine reine Leseabfrage, genau wie Afterburner sie sendet.
   **Erst nach ausdrücklicher Freigabe durch den Benutzer ausführen.**
3. **Formate von C0/C1/E1/D0/D1/0x42.** Im selben Testprogramm, ebenfalls nur `0x51`-Lesebefehle.
   Bytes roh protokollieren und gegen MSI-Center-Anzeige bzw. -Log vergleichen.
4. **Firmware-Verhalten bei parallelen Anfragen.** Lässt sich nur durch Beobachten klären,
   z. B. Afterburner-Log auf „Failed to update data“ prüfen, während das Testprogramm läuft.
5. **MSI-Center-Abfragetakt.** Der Profil-Timer läuft alle 60 s (Log). Der Monitor-Timer ist
   statisch ohne IL-Decompiler nicht ablesbar. Für GpuMon ist er nicht entscheidend, weil
   MSI Center sich an den Mutex hält.

---

## 7. Risiken

| Risiko | Bewertung | Gegenmaßnahme |
|---|---|---|
| Versehentlicher Schreibbefehl ändert Lüfter-, OCP- oder Summer-Einstellungen bzw. speichert sie (`0xF1`) | gering bei Whitelist, Folgen mittel | feste Whitelist, Report wird nur aus `(0x51, reg)` gebaut, keine generische Sende-API |
| Afterburner-Thread hängt, weil die Firmware bei Kollision eine Anfrage verliert (blockierendes `hid_read` ohne Timeout) | [offen], heute schon mit MSI Center möglich | geringe Rate, gleiche Befehle wie Afterburner, Messtest mit Afterburner-Log |
| Afterburner verliert gelegentlich einen Zyklus (Echo-Mismatch) | wahrscheinlich, harmlos (Wert bleibt 1 s alt) | nur E0/C1 senden |
| MSI Center sendet beim Start selbst Schreibbefehle (Log: „Set ZeroFan On/Off“, „Set fan mode: 1“, „Set fan speed: 50“) | betrifft GpuMon nicht direkt | nur zur Kenntnis: Einstellungen kann MSI Center überschreiben |
| Firmware-Update ändert Layout | gering | Handshake-String und PID prüfen, unbekannte Version → Hinweis „ungetestet“ |
| Rechte: Gerätezugriff ohne Admin | Vendor-Page-HID-Geräte sind normalerweise für Benutzer les- und schreibbar [abgeleitet]. Afterburner läuft aber erhöht, GpuMon nicht. | beim Messtest prüfen. Ohne Schreibrecht kein Request möglich, dann bleibt nur die MAHM-Quelle |

---

## 8. Aufwand für `HidSource` und Empfehlung

### 8.1 Umfang

Neue Datei `src/source/hid.rs` (geschätzt 300–400 Zeilen) hinter dem bestehenden Trait
`PsuSource` (`src/source/mod.rs`):

- Gerätesuche: `SetupDiGetClassDevsW` + `SetupDiEnumDeviceInterfaces` +
  `HidD_GetAttributes` → VID `0x0DB0`, PID `0xAA6F` (optional `0x808C`).
  Alternativ den Pfad aus `CM_Get_Device_Interface_ListW` bauen.
- Öffnen: `CreateFileW(GENERIC_READ|GENERIC_WRITE, FILE_SHARE_READ|FILE_SHARE_WRITE,
  FILE_FLAG_OVERLAPPED)`. Handle offen halten, bei Fehlern neu verbinden (Backoff 2–5 s).
- Transaktion: Mutex (Timeout) → `HidD_FlushQueue` → `WriteFile` 65 Byte → `ReadFile` 65 Byte
  mit Timeout → Echo prüfen → Mutex freigeben.
- Handshake `[0xFA, 0x51]` einmal nach dem Öffnen (String loggen/anzeigen), dann zyklisch
  `E0` und `C1`, seltener (z. B. alle 10 s) `0x20` und `E1`.
- LINEAR11-Dekoder plus Zuordnung nach Tabelle 2.4 in `PsuSnapshot`. `model.rs` bekommt
  ggf. neue optionale Felder (AC-Spannung, OCP-Status als Enum statt 0/1).
- `windows`-Features ergänzen: `Win32_Devices_HumanInterfaceDevice`,
  `Win32_Devices_DeviceAndDriverInstallation`, `Win32_Storage_FileSystem`, `Win32_System_IO`.
- Quellwahl in `main.rs`: Auto (HID, Fallback MAHM) bzw. per Einstellung.
- Unit-Tests für LINEAR11 und das Parsen eines aufgezeichneten E0-Blocks (Fixture aus dem
  Messtest), der Mock bleibt.

Aufwand: **ca. 1 Tag** für die Quelle mit Tests, plus **½ Tag** für Messtest, Feinschliff
und Doku. Neue Abhängigkeiten außer `windows`-Features sind nicht nötig.

### 8.2 Empfehlung

**Ja, umsetzen – aber gestuft und als zusätzliche Quelle, nicht als Ersatz.**

Begründung:
- Das Leseprotokoll ist klein, eindeutig und vom Schreiben sauber getrennt. Das Risiko,
  Einstellungen zu verändern, lässt sich durch die Whitelist praktisch ausschließen.
- Gewinn: GpuMon läuft auch ohne Afterburner, sieht den **echten Hardware-OCP-Status mit
  Grund** (Pin-Überstrom / Schieflast / 18 A) statt nur 0/1, die AC-Spannung und die
  Schutz-Alarme. Außerdem lässt sich die Rate erhöhen, falls die Firmware das hergibt.
- Kosten: vernachlässigbarer Ressourcenverbrauch, rund 1,5 Tage Arbeit.
- Afterburner bleibt für die RTSS-Overlays nötig. Solange dessen PSU-Plugin aktiv ist, fragen
  zwei Programme ohne gemeinsamen Schutz ab. Deshalb GpuMon mit niedriger Rate und denselben
  Befehlen betreiben.

Reihenfolge:
1. `EnableLog = 1` im Plugin, um die unbekannten Wörter zu sehen (keine eigenen Zugriffe).
2. Nach Freigabe: lesender Messtest (Abschnitt 6, Punkt 2–3) mit Whitelist und Afterburner-Log.
3. Wenn die Firmware schneller als 1 Hz aktualisiert und keine Kollisionsprobleme auftreten:
   `HidSource` mit 2–5 Hz, Peaks pro Pin, Anzeige weiter sparsam.
   Wenn nicht: `HidSource` trotzdem mit 1 Hz für OCP-Status, AC-Spannung und Unabhängigkeit
   von Afterburner. Die MAHM-Quelle bleibt als Fallback.

---

## 9. HWiNFO

Untersucht: HWiNFO64 8.60-6120 (`C:\Program Files\HWiNFO64`). HWiNFO läuft mit
erhöhten Rechten, gestartet per Autostart nur mit dem Sensorfenster. Laut `HWiNFO64.INI`
ist `SensorsSM=1`, Shared Memory ist also aktiv. Eine Lizenzdatei liegt neben der exe, ihr
Inhalt wurde nicht gelesen. HWiNFO-Einstellungen wurden nicht verändert. Ich habe nur das
Shared Memory `Global\HWiNFO_SENS_SM2` mit `FILE_MAP_READ` gelesen.

### 9.1 Zeigt HWiNFO das Netzteil? [beobachtet]

Ja. HWiNFO hat einen eigenen Sensor **„MSI MPG Ai1300TS“** (Sensor-ID `0xF5000917`) mit
diesen Werten (Momentaufnahme unter Last):

| Wert | Beispiel | bei Afterburner? |
|---|---|---|
| PSU Temperature | 53 °C | ja |
| +3.3V / +5V / +12V (Spannung) | 3,320 / 5,039 / 12,078 V | ja |
| PSU Fan | 384 RPM | ja |
| +3.3V / +5V / +12V Current | 0 / 0 / 43,5 A | ja |
| +12V-2x6_1 Pin1 … Pin6 Current | 6,56 / 6,81 / 7,06 / 6,81 / 6,56 / 6,63 A | ja |
| +3.3V / +5V / +12V Power | berechnet U·I, z. B. 525,4 W | nein (berechnet) |
| PSU Power (sum) | 525,4 W, Summe der Rail-Leistungen | nein (berechnet) |
| PSU Power Out | 524 W, vom Netzteil gemeldet | ja („PSU power“) |
| PSU Efficiency | 94,25 % | ja |
| **Total Runtime** | 2,076 h | **nein** |

Gegenüber Afterburner fehlen der zweite Stecker (12V-2x6_2, beim Benutzer unbelegt;
HWiNFO blendet ihn offenbar aus) und **jeder Alarm/OCP-Status**. Neu ist nur
„Total Runtime“, ein Wert, den Afterburner nicht liefert. Er stammt mit hoher
Wahrscheinlichkeit aus Register `0xD0`/`0xD1` (RUNTIME_NOW/TOTAL, abgeleitet). Dass der
Minimalwert 1,023 h beträgt, deutet eher auf die Laufzeit seit dem Einschalten bzw. seit
dem HWiNFO-Start als auf die Lebensdauer des Netzteils hin [offen]. AC-Spannung,
Schutz-Alarme und OCP-Schwellen zeigt HWiNFO nicht.

Die Werte sind auf 1/16 A (Pins, z. B. 6,5625 = 105/16) bzw. 1/256 V (12,078 ≈ 3092/256)
gerastert. Das passt genau zu LINEAR11 mit den Exponenten −4 bzw. −8 und bestätigt
unabhängig die Umrechnung aus Abschnitt 2.5 [abgeleitet].

### 9.2 Liest HWiNFO selbst per HID?

- **Sehr wahrscheinlich ja [abgeleitet]:** Der Sensor gehört zu HWiNFOs eigener Familie für
  Geräte mit eigener Schnittstelle (`0xF5…`, wie „NUC126“). Afterburner ist kein eigener
  Sensor. Der vorhandene „RTSS“-Sensor stammt aus RTSS, nicht aus Afterburners
  MAHM-Speicher. „Total Runtime“ gibt Afterburner gar nicht weiter. Zudem fragt HWiNFO in
  seinem eigenen 2-s-Takt ab (9.3), nicht in Afterburners 1-s-Takt.
- **Nicht direkt prüfbar:** `HWiNFO64.exe` ist gepackt bzw. verschlüsselt (3 Sektionen,
  praktisch keine lesbaren Strings, keine Treffer für `0DB0`, `AA6F`, `MSI_PSU_Mutex`,
  `hidapi`, `Ai1300`). Statische Analyse ist ohne Entpacken nicht möglich. Weil HWiNFO
  erhöht läuft und diese Sitzung nicht, ließen sich auch geöffnete Handles und geladene
  Module (`hid.dll`) nicht auflisten. **Ob HWiNFO `Global\MSI_PSU_Mutex` verwendet, ist
  [offen].** Prüfbar wäre das mit Sysinternals `handle.exe -a -p HWiNFO64` bzw. Process
  Explorer als Administrator. Gesucht ist ein Handle auf `\Device\…HID…VID_0DB0&PID_AA6F…`
  und auf `\BaseNamedObjects\MSI_PSU_Mutex`.
- Der öffentliche Änderungsverlauf (hwinfo.com) blockiert automatisierte Abrufe (HTTP 403).
  Er wurde deshalb nicht ausgewertet.

### 9.3 Rate und Protokoll

- **Rate [beobachtet]:** Das Feld „Polling Period“ im SM-Header steht auf **2000 ms**. Der
  Zeitstempel `poll_time` ändert sich über 12 s gemessen exakt alle ~2,0 s. HWiNFO fragt
  also standardmäßig alle 2 s ab. In HWiNFO ist das pro Sensor einstellbar.
- **Protokoll:** Die gerasterten Werte bestätigen LINEAR11 (9.1). Welche Befehle HWiNFO
  sendet, ließ sich wegen des Packers nicht ermitteln. Eine **unabhängige Bestätigung,
  welche Befehle rein lesend sind, liefert HWiNFO daher nicht**. Diese Aussage stützt sich
  weiter auf die beiden MSI-Programme (Abschnitt 2.7).
- **Folge für die Koexistenz [abgeleitet]:** Es greifen bereits **drei** Programme
  zyklisch zu: Afterburner (1 s, ohne Mutex), HWiNFO (2 s, Mutex unbekannt) und
  MSI Center (mit Mutex). Ein eigener HID-Zugriff von GpuMon wäre der vierte. Das
  verstärkt das Kollisionsrisiko aus 3.3. Die Firmware scheint parallele Leser
  bisher zu verkraften. Die sporadischen „GET_RUNTIME_TOTAL is failed“ im MSI-Center-Log
  passen allerdings gut dazu, dass HWiNFO denselben Runtime-Wert liest.

### 9.4 HWiNFO-Shared-Memory als Quelle für GpuMon

Machbar und technisch einfach:

- **Layout ist öffentlich und stabil** (`HWiNFO_SENS_SM2`, Signatur `'HWiS'`, Version 2,
  Revision 1). Header mit Offsets und Größen der Sensor- (hier 29 × 392 Byte) und
  Reading-Sektion (163 × 460 Byte). Pro Reading: Typ, Sensorindex, ID, Label (ANSI und
  UTF-8), Einheit, `Value/Min/Max/Avg` als f64. Das Auslesen ähnelt der vorhandenen
  `mahm.rs`, ca. 200–250 Zeilen. Einträge per Sensorname und Reading-ID suchen
  (z. B. `0x04000010…15` für die Pins), nicht per Index.
- **Vorteile:** kein eigener HID-Zugriff, also kein zusätzliches Kollisionsrisiko und
  kein Risiko für Einstellungen. Die Pinströme sind enthalten, HWiNFO liefert gleich
  Min/Max/Avg mit, und GpuMon funktioniert auch ohne Afterburner.
- **Nachteile:** Nur 2-s-Takt (einstellbar, aber dann für alle Sensoren bzw. pro Sensor in
  HWiNFO). **Kein Alarm/OCP-Status**, der zweite Stecker taucht nur bei Belegung auf.
  Außerdem gilt **in der kostenlosen Version** von HWiNFO das Shared Memory nur zeitlich
  begrenzt (nach meinem Kenntnisstand 12 h, danach muss es neu aktiviert werden; nicht am
  Changelog verifiziert). Auf diesem Rechner liegt eine Lizenzdatei, die Grenze greift
  hier also vermutlich nicht. Für andere Nutzer von GpuMon wäre das ein Stolperstein.
- **Ressourcen:** wie bei MAHM vernachlässigbar (eine Map, ein `memcpy` pro Takt).

**Bewertung:** Als *weitere* Quelle hinter `PsuSource` sinnvoll und risikolos, z. B.
`HwinfoSource` als Fallback, wenn Afterburner nicht läuft. Gegenüber Afterburner/MAHM bringt
sie aber kaum neue Werte und einen langsameren Takt. Den Gewinn eines eigenen HID-Zugriffs
(OCP-Status mit Grund, AC-Spannung, Schutz-Alarme, ggf. höhere Rate) ersetzt sie nicht. An
der Empfehlung aus Abschnitt 8 ändert sich deshalb nichts. Das Kollisionsrisiko spricht aber
noch deutlicher dafür, GpuMon mit niedriger Rate, unter dem Mutex und nur mit E0/C1 abfragen
zu lassen.
