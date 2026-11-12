# Analysis: Reading the MSI MPG Ai1300TS PCIE5 PSU directly over USB-HID

As of: 2026-11-12. This is based purely on static analysis. **Nothing was sent to the
PSU**, neither output nor feature reports. The only contact with the device was a query
of the HID capabilities through a handle with access right 0
(`HidD_GetPreparsedData` / `HidP_GetCaps`). Nothing is transmitted in that case.

Artifacts examined:

| File | Version / SHA-256 | Role |
|---|---|---|
| `MSI Afterburner\Plugins\Monitoring\PSU.dll` | AB 4.6.7, `5fe8c4e8…37fd8` | Afterburner plugin (C++/MFC, x86), uses `hidapi.dll` 0.15.0 |
| `MSI Center\Power Supply Unit\API_Power Supply Unit.dll` | 1.0.0.38, `f7833895…74bb` | MSI Center module (.NET), uses `MsiHid.dll` |
| MSI Center log `Log\Power Supply Unit\Library_11_12_2026.log` | – | Runtime behavior of MSI Center |

Labels used in the text: **[observed]** = seen directly in the code or the metadata,
**[derived]** = conclusion drawn from several observations, **[open]** = not resolved.

---

## 1. Summary

- The protocol is simple and fully understood for reading: 64-byte output report
  `[0x51, register]` (READ), response as a 64-byte input report `[0x51, register, data…]`.
  All measurements Afterburner displays come from **a single query** (`0x51 0xE0`,
  "READ_ALL") and are PMBus LINEAR11 words.
- **Reading and writing are cleanly separated.** Byte 1 is the command type:
  `0x51` = READ, `0x50` = WRITE, `0xFA` = HANDSHAKE. The device has no feature reports
  at all. If PinGuard only sends `0x51` and `0xFA 0x51`, it cannot change any setting
  [derived, see 2.6].
- The mutex `Global\MSI_PSU_Mutex` is locked **by MSI Center around every transaction**,
  **but by Afterburner's PSU.dll only during the handshake**. Afterburner's cyclic polling
  runs without the mutex.
- The PSU provides more than Afterburner shows: AC input voltage, operating hours,
  detailed protection/alarm bits (OVP/UVP/OCP/OPP/OTP/fan/AC, OCP per pin),
  OCP thresholds, fan duty, manufacturer/serial number and 9 further, unknown words in the
  READ_ALL block.
- Whether the PSU updates its values faster than 1 Hz cannot be determined statically.
  That requires a short, read-only measurement test (section 6).
- **Recommendation: implement it, but in stages** and only after the measurement test.
  Details in section 8.

---

## 2. Protocol

### 2.1 Device and transport [observed]

- USB `VID_0DB0 & PID_AA6F`, HID interface `HID\VID_0DB0&PID_AA6F\…`.
- HID caps: UsagePage `0xFF00` (vendor-defined), Usage `0x0001`,
  **InputReportByteLength = 65, OutputReportByteLength = 65, FeatureReportByteLength = 0**.
  So there is only report ID 0 with 64 payload bytes in both directions and no feature reports.
- PSU.dll finds the device via `hid_enumerate(0,0)` and filters on VID `0x0DB0` and one
  of the PIDs below. It is opened with `hid_open_path` (on Windows, hidapi opens via
  `CreateFileW` with read and write access). MSI Center, by contrast, opens it **anew for
  every transaction** (`openMyDevice_Overlapped` → Write → Read → `CloseDevice`).

Supported models (constants from both DLLs, identical):

| PID | Model | PSU.dll layout |
|---|---|---|
| `0xE749` | MEG Ai1000P | 1 (multi-rail 12V1…12V5) |
| `0x56D4` | MEG Ai1300P | 1 |
| `0xC9EB` | MEG Ai1600T | 2 (aggregate values only) |
| **`0xAA6F`** | **MPG Ai1300TS** | **3 (12V-2x6 pin currents)** |
| `0x808C` | MPG Ai1600TS | 3 |

### 2.2 Structure of a transaction [observed]

Source: PSU.dll `FUN_10001b30` (transaction), `FUN_10001bd0` (transaction under mutex).

```
HidD_FlushQueue(handle)                    // discard old input reports
out[0..64] = 0
out[0] = 0x00                              // report ID
out[1] = type                              // 0x51 READ, 0xFA HANDSHAKE (0x50 WRITE only MSI Center)
out[2] = register
hid_write(handle, out, 64)
n = hid_read(handle, in, 64)               // blocking, without timeout (!)
valid if in[0] == type && in[1] == register
payload starts at in[2]
```

On Windows, `hid_read` returns the data without the report ID byte. The echo is therefore
in `in[0]` and `in[1]`.

### 2.3 Commands (registers) [observed, enum `cPSU.CMD` from MSI Center]

| Type/byte 1 | Value | Meaning |
|---|---|---|
| `HANDSHAKE` | `0xFA` | with register `0x51` – returns an ASCII string (identifier/version) from byte 2 |
| `READ` | `0x51` | Read a register |
| `WRITE` | `0x50` | Write a register (**never send**) |

| Register | Name in MSI Center | Content | used by PSU.dll |
|---|---|---|---|
| `0x10` | MFR_ID | Manufacturer ("MSI") | no |
| `0x11` | MFR_MODEL | "MPG Ai1300TS" | no |
| `0x12` | MFR_REVISION | "1.0" | no |
| `0x13` | MFR_SERIAL | Serial number | no |
| `0x20` | VIN | **AC input voltage** | no |
| `0x22`/`0x23` | VOUT/IOUT_12V5 | Single rail | no |
| `0x24`/`0x25` | VOUT/IOUT_12 | 12 V voltage/current | no (comes from E0) |
| `0x26`/`0x27` | VOUT/IOUT_5 | 5 V | no (E0) |
| `0x28`/`0x29` | VOUT/IOUT_3_3 | 3.3 V | no (E0) |
| `0x2A` | POUT | Output power | no (E0) |
| `0x2B` | EFFICIENCY | Efficiency | no (E0) |
| `0x2C` | IOUT_12V2x6 | Pin currents | no (E0) |
| `0x30` | TEMPERATURE | Temperature | no (E0) |
| `0x40` | FAN_SPEED | Fan speed | no (E0) |
| `0x41` | FAN_SETTING | Fan mode (reading ok, writing = setting) | no |
| `0x42` | READ_FANDUTY | Fan duty | no |
| `0x43` | ZERO_FAN_ENABLE | Zero fan on/off | no |
| `0xC0` | OCP12V2x6_CONFIG / MULTIRAIL_OCP | OCP thresholds and times (reading ok) | no |
| `0xC1` | OCP12V2x6_STATE | **Hardware alarm status 12V-2x6** | yes |
| `0xC2` | BUZZER_SWITCH | Buzzer | no |
| `0xD0` | RUNTIME_NOW | Runtime since power-on | no |
| `0xD1` | RUNTIME_TOTAL | Total runtime | no |
| **`0xE0`** | **READ_ALL** | **all measurements in one block** | **yes** |
| `0xE1` | READ_ALERTSTATUS | Protection/alarm bit field | no |
| `0xF1` | SAVE_SETTING | Save settings permanently (**dangerous**) | – |

PSU.dll additionally contains a function for reading a single register
(`[0x51, reg]` → one LINEAR11 word from byte 2, `FUN_10001ab0`), but it is never
called.

### 2.4 Response to `0x51 0xE0` (READ_ALL) on the Ai1300TS [observed]

The response is cached as a 64-byte block. Value *i* is the
little-endian word at `in[2 + 2·i]` (`FUN_10001850`). Mapping for layout 3 (from
`GetSourceDesc` and `GetSourceData`):

| Word | Byte | Afterburner source | Unit |
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
| 22–30 | 46–63 | **unused** (the debug log prints words 0–29 once) | [open] |

Notes:
- The rails 12V1…12V5 from the source list only exist in layout 1 (Ai1000P/Ai1300P).
  On the Ai1300TS the plugin does not query them.
- "PSU power" is called `P_Out` in MSI Center. So it is very likely the
  **DC output power** [derived from the field name]. AC input power would then be
  `P_Out / Eff`. This tentatively answers the earlier open question about this value.

### 2.5 Conversion (LINEAR11) [observed, `FUN_10001390`, assembly checked]

```
exp  = (i16)word >> 11          // 5-bit signed exponent
mant = word & 0x7FF             // 11-bit mantissa, WITHOUT sign extension
value = mant * 2^exp
```

This matches PMBus LINEAR11 except for the mantissa: PSU.dll always treats it as
unsigned. The code does provide a flag for "signed", but both branches mask
identically with `0x7FF`. For the measurements (all ≥ 0) this makes no difference. PinGuard
should nevertheless evaluate the mantissa as signed per the standard and clamp negative values to 0.

Afterburner does no further scaling. Percent, °C and RPM come directly from LINEAR11.

### 2.6 Alarm 12VHPWR1/2 [observed]

The setting `SoftwareAlarm` in `PSU.cfg` (default 1, on the user's system **0**) decides:

- **SoftwareAlarm = 0 (user):** The plugin additionally reads `0x51 0xC1` (OCP12V2x6_STATE).
  Alarm 1 = `in[2] != 0`, alarm 2 = `in[2 + 0x15] = in[23] != 0`.
- **SoftwareAlarm = 1:** It is computed. Alarm = (max. pin current > `MaxCurrent`) or
  (max − min > `CurrentDif`). If the key is missing, the defaults `MaxCurrent` 9.5 and
  `CurrentDif` 5.0 apply.

In MSI Center the status byte is the enum `OCP_12V2x6_Status`: `0` Normal, `1` OCP_Point
(pin overcurrent), `2` Current_Diff (load imbalance), `3` OCP_18A. According to the MSI Center struct
`OCP_State_12V2x6`, each status byte is followed by `RunTime` (u32), `TotalTime` (u32) and
6 pin currents. At 1 + 4 + 4 + 6·2 = 21 = 0x15 bytes this matches exactly the offset PSU.dll
uses [derived]. The exact meaning of RunTime/TotalTime and of the pin currents in
this block (instantaneous value or value at the time of tripping) is **[open]**.

`0xE1` (READ_ALERTSTATUS) is a bit field with the indices from the enum `AlertStatus`
(Ai1300TS variant): OVP 12/5/3.3 V, UVP 12/5/3.3 V, OCP 12/5/3.3 V, OCP 12V1…12V5, OPP,
OTP, Fan, AC, OCP_12V2x6_1_1…2_6. Bit order and length of the field are **[open]**.
MSI Center uses model-dependent index tables (`Alert_Index_P/T/TS`).

### 2.7 Reading and writing are separated [observed and derived]

- The command type is explicitly in byte 1: `0x51` READ, `0x50` WRITE. Afterburner sends
  exclusively `0x51 0xE0`, `0x51 0xC1` and the handshake `0xFA 0x51`.
- MSI Center has separate methods `Get` and `Set`. Writing functions
  (`SET_FAN_SETTING`, `SET_ZERO_FAN`, `SET_OCP12V2x6`, `SET_MULTIRAIL_OCP`,
  `BUZZER_SWITCH`, `SAVE_SETTING`) go through `Set`.
- There are no feature reports. Write access is therefore only possible via output reports with
  `0x50` (and presumably `0xF1`).
- **Rule for PinGuard:** A hard-compiled whitelist that only allows
  `[0xFA, 0x51]` and `[0x51, r]` with `r ∈ {0xE0, 0xC1, 0xE1, 0x20, 0xD0, 0xD1, 0x10–0x13, 0x42}`.
  All other bytes of the report are 0. Nothing more is needed.

Residual risk: Whether the firmware has a side effect on a *read* command with an unknown
register cannot be verified. The whitelist therefore only contains registers that
Afterburner or MSI Center read themselves.

---

## 3. Mutex and coexistence

### 3.1 Who accesses the device [observed]

The machine runs **MSI Afterburner** (PSU.dll) and **MSI Center**
(`MSI_Central_Service`, `MSI.CentralServer`, module "Power Supply Unit", SDK 3.2026.1013.01,
installed on 2026-11-12 at 13:31) at the same time. MSI Center has detected the PSU ("MPG Ai1300TS PCIE5",
serial number in the log) and polls it cyclically. **HWiNFO64** is a third reader
(section 9).

### 3.2 Mutex usage

| Program | Mutex `Global\MSI_PSU_Mutex` |
|---|---|
| PSU.dll | `CreateMutexA` + `WaitForSingleObject(INFINITE)` **only during the handshake** (`FUN_10001bd0`). The cyclic E0/C1 queries (`FUN_10001ce0`) run **without the mutex**. |
| MSI Center | `new Mutex` + `WaitOne` around **every** transaction in `Get`, `Set`, `CONNECT_PSU`, `DISCONNECT` and `CheckConnectedDevice` (open → Write → Read → close). |

### 3.3 What happens with simultaneous queries [derived]

- Windows delivers every input report to **every** open handle. So each response ends up
  with all programs that have the device open.
- PSU.dll discards old reports before each write (`HidD_FlushQueue`) and checks the echo
  `in[0..1]`. If it catches another program's response to the **same** command,
  it takes that data. This is harmless because the content is identical. With a
  **different** command the echo check fails, and Afterburner discards that cycle
  ("Failed to update data"). Its own response is thrown away at the next flush.
- **Critical:** `hid_read` in PSU.dll blocks without a timeout. If the firmware were to drop one of two
  almost simultaneous requests, Afterburner's polling thread would hang
  permanently. Whether the firmware buffers requests is **[open]**. MSI Center and Afterburner
  already run in parallel today without this having been noticeably observed. That suggests
  the firmware answers requests one after the other. However, the MSI Center log contains
  occasional errors such as "GET_RUNTIME_TOTAL is failed" (13:58:50, 14:15:50). These could be
  such collisions.

### 3.4 Consequences for PinGuard

1. Run every transaction under `Global\MSI_PSU_Mutex` (`CreateMutexW`, `WaitForSingleObject`
   with a timeout of e.g. 200 ms instead of INFINITE, then `ReleaseMutex`). This protects against MSI Center.
2. The mutex does not help against Afterburner. Therefore: check the echo, `ReadFile` with timeout
   (overlapped, ~100 ms), on failure discard this cycle and keep the last value.
3. Send the same commands as Afterburner where possible (`E0`, `C1`). Then foreign
   responses in our own queue, and vice versa, are identical in content and harmless.
4. Keep the device open permanently (like PSU.dll), do not open it per transaction
   (like MSI Center). This saves system calls and time.
5. Afterburner polls at `HwPollPeriod=1000` ms. The plugin only fetches E0/C1 when
   Afterburner's timestamp changes (`GetTimestamp`), i.e. once per polling period.

---

## 4. Polling rate and short current spikes

- **Known:** Afterburner polls once per second (setting 1000 ms). Each query is
  one HID transaction of 64 bytes out and 64 bytes back.
- **Unknown [open]:** How often the firmware updates the values internally. Typical
  PMBus PSU controllers average over a few 10–100 ms. The microcontroller reads the
  power stage via PMBus/I²C. So several milliseconds per query are realistic.
- **Technically feasible:** USB full-speed HID can handle up to
  1000 reports/s per interrupt endpoint, in practice limited by `bInterval` (not read out) and the response time
  of the firmware. 5–10 Hz should work without problems.
- **What you can see with it:** Load spikes of modern GPUs last 10 µs to a few ms. You
  will not see them through an averaged sensor even at 10 Hz. What would be detectable are load steps and
  plateaus longer than approx. 100 ms, such as scene changes, shader compilation or
  boost spikes. PSU telemetry is not built for the actual transients.
  That would require a measuring device like WireView with its own ADC.
- **Hardware alarm:** The OCP status byte (`C1`) is set by the firmware and stays valid until
  it is reset [derived from RunTime/TotalTime]. Short overcurrents that the firmware
  detects are therefore visible to PinGuard even at 1 Hz. This is probably more valuable than a
  high polling rate.
- **Resources:** A transaction consists of one `WriteFile` and one `ReadFile`
  (overlapped) plus `WaitForSingleObject` on mutex and event. Even at 10 Hz this is in the
  range of 0.0x % CPU and practically changes nothing in memory usage (no additional
  DLL, `hid.dll`/`setupapi.dll` are small). PinGuard only redraws on new data anyway.
  Redrawing at 10 Hz costs more than the HID reads. For higher rates, therefore, only track the
  peak values and keep the display at 2–4 Hz.

---

## 5. Additional values Afterburner does not pass on

| Value | Source | Certainty |
|---|---|---|
| AC input voltage | `0x51 0x20` (VIN), LINEAR11 | Register certain, format [derived] |
| AC power | computed `P_Out / Eff` | [derived] |
| OCP status per connector (Normal / pin OCP / imbalance / 18 A) | `C1` byte 2 and 23 | Status certain, values [observed] |
| OCP runtimes / pin currents in the OCP block | `C1` bytes 3–22 / 24–43 | [derived], meaning [open] |
| Protection alarms OVP/UVP/OCP/OPP/OTP/fan/AC | `0x51 0xE1` | Register certain, bit layout [open] |
| Firmware OCP thresholds and trip times | `0x51 0xC0` | Register certain, layout [open] (struct: IsEnable, Point Low/High, Diff Low/High, trigger/warning times, each u8) |
| Fan duty, fan mode, zero fan | `0x42`, `0x41`, `0x43` | Register certain, format [open] |
| Runtime since power-on / total | `0xD0` / `0xD1` | Register certain, format [open] |
| Manufacturer, model, revision, serial number | `0x10`–`0x13` | ASCII [derived from log] |
| 9 unknown words in the READ_ALL block | `E0` bytes 46–63 | [open] |

---

## 6. Open questions and how to resolve them without risk

1. **Content of words 22–30 in E0.** *Without our own HID accesses:* In
   `Plugins\Monitoring\PSU.cfg` set `[Debug] EnableLog = 1` and restart Afterburner.
   The plugin then writes the device list, the
   handshake string and **all 30 decoded words** of E0 **once** to `Plugins\Monitoring\PSU.log`.
   Set it back to 0 afterwards.
2. **Firmware update rate.** A small, separate test program sends only
   `[0x51, 0xE0]` under the mutex, e.g. for 30 s at 10 Hz, and logs how often
   the raw words change. This is a pure read query, exactly as Afterburner sends it.
   **Only run after explicit approval by the user.**
3. **Formats of C0/C1/E1/D0/D1/0x42.** In the same test program, likewise only `0x51` read commands.
   Log the raw bytes and compare against the MSI Center display or log.
4. **Firmware behavior with parallel requests.** Can only be resolved by observation,
   e.g. checking the Afterburner log for "Failed to update data" while the test program runs.
5. **MSI Center polling interval.** The profile timer runs every 60 s (log). The monitor timer
   cannot be read statically without an IL decompiler. It is not decisive for PinGuard because
   MSI Center respects the mutex.

---

## 7. Risks

| Risk | Assessment | Countermeasure |
|---|---|---|
| An accidental write command changes fan, OCP or buzzer settings or saves them (`0xF1`) | low with whitelist, moderate consequences | fixed whitelist, report is built only from `(0x51, reg)`, no generic send API |
| Afterburner thread hangs because the firmware loses a request on collision (blocking `hid_read` without timeout) | [open], already possible today with MSI Center | low rate, same commands as Afterburner, measurement test with Afterburner log |
| Afterburner occasionally loses a cycle (echo mismatch) | likely, harmless (value stays 1 s old) | send only E0/C1 |
| MSI Center itself sends write commands at startup (log: "Set ZeroFan On/Off", "Set fan mode: 1", "Set fan speed: 50") | does not affect PinGuard directly | for information only: MSI Center can overwrite settings |
| Firmware update changes the layout | low | check handshake string and PID, unknown version → "untested" notice |
| Permissions: device access without admin | Vendor-page HID devices are normally readable and writable by users [derived]. Afterburner, however, runs elevated, PinGuard does not. | check during the measurement test. Without write access no request is possible; then only the MAHM source remains |

---

## 8. Effort for `HidSource` and recommendation

### 8.1 Scope

New file `src/source/hid.rs` (estimated 300–400 lines) behind the existing trait
`PsuSource` (`src/source/mod.rs`):

- Device discovery: `SetupDiGetClassDevsW` + `SetupDiEnumDeviceInterfaces` +
  `HidD_GetAttributes` → VID `0x0DB0`, PID `0xAA6F` (optionally `0x808C`).
  Alternatively build the path from `CM_Get_Device_Interface_ListW`.
- Opening: `CreateFileW(GENERIC_READ|GENERIC_WRITE, FILE_SHARE_READ|FILE_SHARE_WRITE,
  FILE_FLAG_OVERLAPPED)`. Keep the handle open, reconnect on errors (backoff 2–5 s).
- Transaction: mutex (timeout) → `HidD_FlushQueue` → `WriteFile` 65 bytes → `ReadFile` 65 bytes
  with timeout → check echo → release mutex.
- Handshake `[0xFA, 0x51]` once after opening (log/display the string), then cyclically
  `E0` and `C1`, less often (e.g. every 10 s) `0x20` and `E1`.
- LINEAR11 decoder plus mapping per table 2.4 into `PsuSnapshot`. `model.rs` may get
  new optional fields (AC voltage, OCP status as an enum instead of 0/1).
- Add `windows` features: `Win32_Devices_HumanInterfaceDevice`,
  `Win32_Devices_DeviceAndDriverInstallation`, `Win32_Storage_FileSystem`, `Win32_System_IO`.
- Source selection in `main.rs`: Auto (HID, fallback MAHM) or via setting.
- Unit tests for LINEAR11 and parsing a recorded E0 block (fixture from the
  measurement test); the mock stays.

Effort: **approx. 1 day** for the source with tests, plus **½ day** for the measurement test, polish
and documentation. No new dependencies besides `windows` features are needed.

### 8.2 Recommendation

**Yes, implement it – but in stages and as an additional source, not as a replacement.**

Rationale:
- The read protocol is small, unambiguous and cleanly separated from writing. The risk of
  changing settings can be practically ruled out by the whitelist.
- Benefit: PinGuard also works without Afterburner, sees the **real hardware OCP status with
  its cause** (pin overcurrent / imbalance / 18 A) instead of just 0/1, the AC voltage and the
  protection alarms. In addition, the rate can be increased if the firmware allows it.
- Cost: negligible resource usage, roughly 1.5 days of work.
- Afterburner remains necessary for the RTSS overlays. As long as its PSU plugin is active,
  two programs poll without shared protection. Therefore run PinGuard at a low rate and with the same
  commands.

Order:
1. `EnableLog = 1` in the plugin to see the unknown words (no accesses of our own).
2. After approval: read-only measurement test (section 6, items 2–3) with whitelist and Afterburner log.
3. If the firmware updates faster than 1 Hz and no collision problems occur:
   `HidSource` at 2–5 Hz, peaks per pin, display still economical.
   If not: `HidSource` anyway at 1 Hz for OCP status, AC voltage and independence
   from Afterburner. The MAHM source remains as a fallback.

---

## 9. HWiNFO

Examined: HWiNFO64 8.60-6120 (`C:\Program Files\HWiNFO64`). HWiNFO runs with
elevated rights, started via autostart with only the sensor window. According to `HWiNFO64.INI`,
`SensorsSM=1`, so shared memory is active. A license file sits next to the exe; its
content was not read. HWiNFO settings were not changed. I only read the
shared memory `Global\HWiNFO_SENS_SM2` with `FILE_MAP_READ`.

### 9.1 Does HWiNFO show the PSU? [observed]

Yes. HWiNFO has its own sensor **"MSI MPG Ai1300TS"** (sensor ID `0xF5000917`) with
these values (snapshot under load):

| Value | Example | in Afterburner? |
|---|---|---|
| PSU Temperature | 53 °C | yes |
| +3.3V / +5V / +12V (voltage) | 3.320 / 5.039 / 12.078 V | yes |
| PSU Fan | 384 RPM | yes |
| +3.3V / +5V / +12V Current | 0 / 0 / 43.5 A | yes |
| +12V-2x6_1 Pin1 … Pin6 Current | 6.56 / 6.81 / 7.06 / 6.81 / 6.56 / 6.63 A | yes |
| +3.3V / +5V / +12V Power | computed U·I, e.g. 525.4 W | no (computed) |
| PSU Power (sum) | 525.4 W, sum of the rail powers | no (computed) |
| PSU Power Out | 524 W, reported by the PSU | yes ("PSU power") |
| PSU Efficiency | 94.25 % | yes |
| **Total Runtime** | 2.076 h | **no** |

Compared to Afterburner, the second connector is missing (12V-2x6_2, unused on the user's system;
HWiNFO apparently hides it) as well as **any alarm/OCP status**. The only new item is
"Total Runtime", a value Afterburner does not provide. It very likely comes
from register `0xD0`/`0xD1` (RUNTIME_NOW/TOTAL, derived). The fact that the
minimum value is 1.023 h points more to the runtime since power-on or since
HWiNFO started than to the lifetime of the PSU [open]. HWiNFO does not show AC voltage,
protection alarms or OCP thresholds.

The values are quantized to 1/16 A (pins, e.g. 6.5625 = 105/16) and 1/256 V (12.078 ≈ 3092/256)
respectively. This matches LINEAR11 with exponents −4 and −8 exactly and independently confirms
the conversion from section 2.5 [derived].

### 9.2 Does HWiNFO read via HID itself?

- **Very likely yes [derived]:** The sensor belongs to HWiNFO's own family for
  devices with their own interface (`0xF5…`, like "NUC126"). Afterburner is not a separate
  sensor. The existing "RTSS" sensor comes from RTSS, not from Afterburner's
  MAHM memory. Afterburner does not pass on "Total Runtime" at all. In addition, HWiNFO polls at
  its own 2 s interval (9.3), not at Afterburner's 1 s interval.
- **Not directly verifiable:** `HWiNFO64.exe` is packed or encrypted (3 sections,
  practically no readable strings, no hits for `0DB0`, `AA6F`, `MSI_PSU_Mutex`,
  `hidapi`, `Ai1300`). Static analysis is not possible without unpacking. Because HWiNFO
  runs elevated and this session does not, open handles and loaded
  modules (`hid.dll`) could not be listed either. **Whether HWiNFO uses `Global\MSI_PSU_Mutex` is
  [open].** This could be checked with Sysinternals `handle.exe -a -p HWiNFO64` or Process
  Explorer as administrator. Look for a handle to `\Device\…HID…VID_0DB0&PID_AA6F…`
  and to `\BaseNamedObjects\MSI_PSU_Mutex`.
- The public change log (hwinfo.com) blocks automated requests (HTTP 403).
  It was therefore not evaluated.

### 9.3 Rate and protocol

- **Rate [observed]:** The "Polling Period" field in the SM header is set to **2000 ms**. The
  timestamp `poll_time`, measured over 12 s, changes exactly every ~2.0 s. So HWiNFO polls
  every 2 s by default. In HWiNFO this is configurable per sensor.
- **Protocol:** The quantized values confirm LINEAR11 (9.1). Which commands HWiNFO
  sends could not be determined because of the packer. **HWiNFO therefore does not provide an independent
  confirmation of which commands are purely read-only**. That statement continues to rest
  on the two MSI programs (section 2.7).
- **Consequence for coexistence [derived]:** **Three** programs already access the device
  cyclically: Afterburner (1 s, without mutex), HWiNFO (2 s, mutex unknown) and
  MSI Center (with mutex). PinGuard's own HID access would be the fourth. This
  increases the collision risk from 3.3. The firmware seems to cope with parallel readers
  so far. However, the sporadic "GET_RUNTIME_TOTAL is failed" entries in the MSI Center log
  fit well with HWiNFO reading the same runtime value.

### 9.4 HWiNFO shared memory as a source for PinGuard

Feasible and technically simple:

- **The layout is public and stable** (`HWiNFO_SENS_SM2`, signature `'HWiS'`, version 2,
  revision 1). Header with offsets and sizes of the sensor section (here 29 × 392 bytes) and the
  reading section (163 × 460 bytes). Per reading: type, sensor index, ID, label (ANSI and
  UTF-8), unit, `Value/Min/Max/Avg` as f64. Reading it out is similar to the existing
  `mahm.rs`, approx. 200–250 lines. Look up entries by sensor name and reading ID
  (e.g. `0x04000010…15` for the pins), not by index.
- **Advantages:** no HID access of our own, so no additional collision risk and
  no risk to settings. The pin currents are included, HWiNFO supplies
  Min/Max/Avg along with them, and PinGuard also works without Afterburner.
- **Disadvantages:** Only a 2 s interval (configurable, but then for all sensors or per sensor in
  HWiNFO). **No alarm/OCP status**; the second connector only appears when it is in use.
  In addition, **in the free version** of HWiNFO shared memory is only available for a limited
  time (to my knowledge 12 h, after which it has to be re-enabled; not verified against the
  changelog). There is a license file on this machine, so the limit presumably does not apply
  here. For other users of PinGuard it would be a stumbling block.
- **Resources:** negligible, as with MAHM (one mapping, one `memcpy` per cycle).

**Assessment:** Useful and risk-free as an *additional* source behind `PsuSource`, e.g.
`HwinfoSource` as a fallback when Afterburner is not running. Compared to Afterburner/MAHM, however,
it brings hardly any new values and a slower interval. It does not replace the benefit of our own HID access
(OCP status with cause, AC voltage, protection alarms, possibly a higher rate). The
recommendation from section 8 therefore stands. The collision risk, however, argues
even more clearly for having PinGuard poll at a low rate, under the mutex and only with E0/C1.
