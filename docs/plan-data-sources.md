# Plan: Own data sources for PinGuard (reading the PSU without Afterburner)

As of 2026-11-12. This file collects everything needed to extend PinGuard with its own data
sources later. It can be used on its own. Evidence and derivations (function addresses, hashes,
log excerpts) are covered in detail in `docs/hid-protocol-analysis.md`.

Labels: **[confirmed]** = seen in code or observed live, **[derived]** = conclusion,
**[open]** = still to be clarified.

---

## 1. Starting point (v1)

- PinGuard reads the values from Afterburner's shared memory `MAHMSharedMemory`. It is filled
  by the Afterburner plugin `Plugins\Monitoring\PSU.dll`, which polls the PSU via USB HID.
  Layout, source names and connection logic: `src/source/mahm.rs`, `README.md`.
- The data source is swappable via the trait `PsuSource` (`src/source/mod.rs`):
  ```rust
  pub enum Reading { Data(PsuSnapshot), NoPsu, Offline }
  pub trait PsuSource { fn poll(&mut self) -> Reading; fn period_ms(&self) -> u32; fn name(&self) -> &'static str; }
  ```
  `MahmSource` and `MockSource` (`--mock`, `--mock2`) exist. New sources only need this
  trait; nothing changes in the display.
- `PsuSnapshot` (`src/model.rs`) consists of one `Option<f32>` per value (indices in
  `model::idx`). New values (AC voltage, OCP status with reason) get new fields there.
- Constraint: resource usage must stay minimal (today 14 MB WS, 3 MB private, 0.03 % CPU,
  no GPU). Do not load D3D (otherwise RTSS injects itself), redraw only on new data.

## 2. Options at a glance

| | A: Lower Afterburner period | B: `HwinfoSource` | C: `HidSource` (own USB access) |
|---|---|---|---|
| Effort | setting, 0 code | ~200–250 lines, ½ day | ~300–400 lines, ~1.5 days incl. measurement test |
| Without Afterburner | no | yes (needs HWiNFO) | yes |
| Risk to the PSU | none | none | low (whitelist, see 5.6) |
| Rate | configurable (default 1 s) | 2 s (configurable in HWiNFO) | free, sensible 1–5 Hz |
| OCP/alarm status | 0/1 per connector | **none** | **with reason** (pin OCP/imbalance/18 A) |
| Extra values | – | total runtime, min/max/avg | AC voltage, protection alarms, OCP thresholds, runtime, serial no. |
| Pitfalls | – | free version: shared memory time-limited | 4th reader on the device, firmware updates |

**Recommendation (from the analysis):** implement C, but in stages and as an **additional**
source with MAHM as fallback. B is a cheap, risk-free fallback but does not replace C.
Try A once beforehand; it costs nothing.

## 3. Order of implementation

1. **Without own access (risk-free)**
   - [ ] Test option A: in Afterburner, Settings → Monitoring → set the hardware polling
         period to 250 ms. Do the PSU values in PinGuard change more often than once per second?
   - [ ] In `MSI Afterburner\Plugins\Monitoring\PSU.cfg`, set `EnableLog = 1` under `[Debug]`
         and restart Afterburner. `PSU.log` then contains the device list, the handshake
         string and, **once, all 30 decoded words of the READ_ALL block**. Use this to
         interpret the unknown words 22–30 (5.4). Afterwards set `EnableLog = 0` again.
   - [ ] As administrator, check in Process Explorer or with `handle.exe -a -p HWiNFO64`
         whether HWiNFO holds a handle to `\BaseNamedObjects\MSI_PSU_Mutex` and to the HID device.
2. **Measurement test (only after explicit approval by the user)**, see section 6.
3. **Implement `HidSource`** (section 7). Rate depending on the measurement test: 2–5 Hz if
   the firmware updates faster than 1 Hz and no collisions occur, otherwise 1 Hz.
4. **Optional `HwinfoSource`** as another fallback (section 8).
5. **Extend the display** (section 9).

---

## 4. Device and access

- USB `VID_0DB0 & PID_AA6F` ("MPG Ai1300TS"), HID interface `HID\VID_0DB0&PID_AA6F\…`
  **[confirmed]**.
- HID caps: UsagePage `0xFF00` (vendor-defined), Usage `0x0001`,
  Input = Output = **65 bytes** (report ID 0 + 64 payload bytes), **no feature reports**
  **[confirmed]**.
- Other models with the same protocol: `0xE749` MEG Ai1000P, `0x56D4` MEG Ai1300P
  (multi-rail layout), `0xC9EB` MEG Ai1600T (totals only), `0x808C` MPG Ai1600TS
  (same layout as Ai1300TS) **[confirmed]**.
- Who accesses it today **[confirmed/derived]**:

  | Program | Rate | Mutex `Global\MSI_PSU_Mutex` |
  |---|---|---|
  | Afterburner PSU.dll | 1 s (`HwPollPeriod`) | **only during handshake**, cyclic E0/C1 queries **without** mutex; `hid_read` blocks **without timeout** |
  | MSI Center (module "Power Supply Unit") | profile timer 60 s, monitor rate [open] | around **every** transaction, reopens the device per transaction |
  | HWiNFO64 8.60 | 2 s | [open] |

  PinGuard would be the fourth reader.
- Windows delivers every input report to **every** open handle. Responses to other
  programs therefore also land in PinGuard's queue (and vice versa).

## 5. Protocol

### 5.1 Transaction [confirmed]

```
HidD_FlushQueue(handle)                 // discard old input reports
out = [0u8; 65]
out[0] = 0x00                           // report ID
out[1] = type                           // 0x51 READ | 0xFA HANDSHAKE   (0x50 WRITE: NEVER)
out[2] = register
WriteFile(handle, out, 65)
ReadFile(handle, in, 65)                // with timeout (overlapped)!
// With Windows WriteFile+ReadFile, in[0] = report ID (0); hidapi strips it.
// Check echo: in[1] == type && in[2] == register  (with hidapi: in[0], in[1])
// Payload from in[3]                              (with hidapi: from in[2])
```

The analysis gives offsets from hidapi's point of view (without the report ID byte).
**All byte offsets below are also counted without the report ID.** With raw `ReadFile`,
add 1.

### 5.2 Command types and registers [confirmed, names from MSI Center `cPSU.CMD`]

Type (byte 1): `0x51` READ, `0x50` WRITE (**never send**), `0xFA` HANDSHAKE (with register
`0x51`; the response from byte 2 is an ASCII string with identifier/version).

| Register | Name | Content | For PinGuard |
|---|---|---|---|
| `0xE0` | READ_ALL | all measurements in one block (5.3) | **cyclic** |
| `0xC1` | OCP12V2x6_STATE | hardware OCP status per connector (5.5) | **cyclic** |
| `0xE1` | READ_ALERTSTATUS | protection alarms as bit field (layout [open]) | rarely (10 s) |
| `0x20` | VIN | AC input voltage, LINEAR11 | rarely (10 s) |
| `0xD0` / `0xD1` | RUNTIME_NOW / TOTAL | runtime (format [open]) | optional |
| `0x10`–`0x13` | MFR_ID / MODEL / REVISION / SERIAL | ASCII | once |
| `0x42` | READ_FANDUTY | fan duty (format [open]) | optional |
| `0xC0` | OCP configuration | thresholds and times, one u8 each (layout [open]) | not for now (not in the whitelist) |
| `0x41`, `0x43`, `0xC2` | fan mode, zero fan, buzzer | settings | **not needed** |
| `0xF1` | SAVE_SETTING | store settings permanently | **never** |

The individual registers `0x24`–`0x2C`, `0x30`, `0x40` return the same as READ_ALL and are
not needed.

### 5.3 READ_ALL (`0x51 0xE0`) on the Ai1300TS [confirmed]

Word *i* = little-endian u16 at byte `2 + 2·i`, each LINEAR11 (5.4).

| Word | Bytes | Value | Unit | MAHM name |
|---|---|---|---|---|
| 0 | 2–3 | 12 V current | A | PSU 12V current |
| 1 | 4–5 | 5 V current | A | PSU 5V current |
| 2 | 6–7 | 3.3 V current | A | PSU 3.3V current |
| 3–8 | 8–19 | connector 1, pins 1–6 | A | PSU 12VHPWR1_1…6 current |
| 9–14 | 20–31 | connector 2, pins 1–6 | A | PSU 12VHPWR2_1…6 current |
| 15 | 32–33 | output power (DC, `P_Out`) | W | PSU power |
| 16 | 34–35 | efficiency | % | PSU efficiency |
| 17 | 36–37 | temperature | °C | PSU temperature |
| 18 | 38–39 | fan | RPM | PSU fan speed |
| 19 | 40–41 | 12 V voltage | V | PSU 12V voltage |
| 20 | 42–43 | 5 V voltage | V | PSU 5V voltage |
| 21 | 44–45 | 3.3 V voltage | V | PSU 3.3V voltage |
| 22–30 | 46–63 | unknown | [open] | – (interpret with `EnableLog`) |

`PSU power` is the DC output power. AC input is ≈ `P_Out / (Eff / 100)`.

### 5.4 LINEAR11 [confirmed]

```rust
fn linear11(w: u16) -> f32 {
    let exp = (w as i16) >> 11;                     // 5 bits, signed
    let mant = ((w << 5) as i16) >> 5;              // 11 bits, signed (PMBus-compliant)
    (mant as f32 * (exp as f32).exp2()).max(0.0)    // measurements are >= 0
}
```

PSU.dll always evaluates the mantissa unsigned (`w & 0x7FF`). For values ≥ 0 this is
identical. The resolution confirms the exponent: pins in 1/16 A (−4), voltages in
1/256 V (−8), also visible live in HWiNFO.

Test vectors from live values: 6.5625 A = 105 × 2⁻⁴ → `0xE069`; 12.078 V ≈ 3092 × 2⁻⁸.
Save real raw words as fixtures during the measurement test.

### 5.5 OCP status (`0x51 0xC1`) [confirmed/derived]

- Connector 1: status byte at **byte 2**, connector 2: at **byte 23** (`2 + 0x15`) [confirmed].
- Values (enum `OCP_12V2x6_Status`): `0` Normal, `1` OCP_Point (pin overcurrent),
  `2` Current_Diff (imbalance), `3` OCP_18A [confirmed].
- Each connector block continues with `RunTime` (u32), `TotalTime` (u32) and 6 pin currents
  (u16), 21 bytes per block in total [derived]. Whether these are live values or values at
  the time of tripping is [open].
- The status is set by the firmware and presumably stays until reset [derived]. Short
  overcurrents are therefore visible even at 1 Hz. That is more valuable than a high
  polling rate.
- Afterburner only reads `C1` with `SoftwareAlarm = 0` in `PSU.cfg` (user's setting: 0).
  With `1`, the plugin computes the alarm itself (`MaxCurrent` / `CurrentDif`).

### 5.6 Safety rule: whitelist [binding]

Reads and writes differ in the type byte. There are no feature reports. Therefore:

- Hard-compiled whitelist, **no generic send API**:
  `[0xFA, 0x51]` and `[0x51, r]` with `r ∈ {0xE0, 0xC1, 0xE1, 0x20, 0xD0, 0xD1, 0x10, 0x11, 0x12, 0x13, 0x42}` (as in the analysis).
- All other bytes of the report are 0.
- Only registers that Afterburner or MSI Center read themselves. Whether a read command on
  an unknown register has side effects cannot be verified.
- Unit test ensuring that the report builder only produces whitelisted combinations.

---

## 6. Measurement test (before implementation, only with approval)

Small separate test program, e.g. `examples/hid_probe.rs` or a `--hid-probe` switch.
It sends only whitelisted commands, under the mutex.

1. Open the device, handshake, read `0x10`–`0x13` once and log them.
2. For 30 s, send `0x51 0xE0` at 10 Hz. Log raw words with timestamps and count how often
   the pin words change → actual update rate of the firmware.
3. Log `C1`, `E1`, `D0`, `D1`, `0x20`, `0x42` raw once each and compare with the display in
   MSI Center / HWiNFO → clarify formats.
4. In parallel, check whether Afterburner reports errors: watch Afterburner monitoring for
   gaps, or set `EnableLog = 1` and search `PSU.log` for "Failed to update data".
   Caution: PSU.dll reads without a timeout. If the firmware dropped a request on a
   collision, Afterburner's polling thread would hang. Abort criterion: Afterburner's PSU
   values freeze → stop the test immediately, restart Afterburner.
5. Check whether `CreateFileW` with read and write access works **without admin**
   (Afterburner runs elevated, PinGuard does not). Without write access no request is
   possible; then only MAHM or HWiNFO remain.
6. Save the results as fixtures (`tests/fixtures/e0_*.bin`, `c1_*.bin`).

## 7. Implementing `HidSource` (`src/source/hid.rs`)

- **Device discovery:** `SetupDiGetClassDevsW(&GUID_DEVINTERFACE_HID, …, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)`
  → `SetupDiEnumDeviceInterfaces` → `SetupDiGetDeviceInterfaceDetailW` → open path →
  `HidD_GetAttributes` (VID `0x0DB0`, PID `0xAA6F`, optionally `0x808C`) and `HidD_GetPreparsedData`
  + `HidP_GetCaps` (UsagePage `0xFF00`, report lengths 65).
- **Opening:** `CreateFileW(GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
  OPEN_EXISTING, FILE_FLAG_OVERLAPPED)`. **Keep the handle open** (like PSU.dll). On errors,
  close it and search again with a 2–5 s backoff (like `MahmSource`).
- **Transaction:**
  1. `CreateMutexW(None, false, "Global\\MSI_PSU_Mutex")` once; per transaction
     `WaitForSingleObject(mutex, 200)`. On `WAIT_TIMEOUT` skip the cycle, on
     `WAIT_ABANDONED` continue normally. Then `ReleaseMutex`.
  2. `HidD_FlushQueue` → `WriteFile` (overlapped, timeout ~100 ms) → `ReadFile` (overlapped,
     timeout ~100 ms, otherwise `CancelIoEx`).
  3. Check the echo. Discard foreign responses (different register) and read at most 2
     more reports, then give up. On failure, keep the last value.
- **Rate:** cyclic `E0` + `C1` (1 Hz, possibly 2–5 Hz after the measurement test), `0x20`
  and `E1` every 10 s. Use the same commands as Afterburner where possible; then foreign
  responses in our own queue have the same content and are harmless.
- **Mapping:** LINEAR11 → `PsuSnapshot` per table 5.3. `GPU_W` is not available via HID;
  if needed, keep taking it from MAHM or drop it.
- **Source selection** in `main.rs`: `--source auto|hid|mahm|hwinfo` (default `auto`: HID,
  otherwise MAHM). Store it in the INI. `name()` shows the active source in the notice.
- **Firmware protection:** check the handshake string and PID. With an unknown version keep
  reading, but show "untested firmware" in the UI.
- **Cargo features:** `Win32_Devices_HumanInterfaceDevice`,
  `Win32_Devices_DeviceAndDriverInstallation`, `Win32_Storage_FileSystem`, `Win32_System_IO`.
  No additional crates.
- **Tests:** LINEAR11 (test vectors), parsing of the E0/C1 fixture blocks, whitelist test,
  echo check.
- **Measure resources again after integration** (target unchanged: < 15 MB WS, ≈ 0 % CPU,
  0 GPU). With a higher polling rate, only track peaks and redraw at 2–4 Hz.

## 8. Optional: `HwinfoSource` (`src/source/hwinfo.rs`)

- Read shared memory `Global\HWiNFO_SENS_SM2` (`OpenFileMappingW` + `FILE_MAP_READ`);
  "Shared Memory Support" must be enabled in HWiNFO (user's setting: `SensorsSM=1`).
- Header (publicly documented, check against the HWiNFO SDK when implementing): signature
  `'HWiS'`, version 2, revision 1, `poll_time` (i64), offsets/sizes/counts of the sensor
  section (here 29 × 392 bytes) and the reading section (163 × 460 bytes), polling period
  (2000 ms).
- Reading: type, sensor index, reading ID, label (ANSI and UTF-8), unit,
  `Value/Min/Max/Avg` as f64.
- Sensor "MSI MPG Ai1300TS" (ID `0xF5000917`). Pins e.g. reading IDs
  `0x04000010…15` (label "+12V-2x6_1 Pin1…6 Current"). **Look up by sensor name and
  label/ID, not by index.** Stale detection via `poll_time`, as with MAHM.
- Provides pins, rails, temperature, fan, P_Out, efficiency and total runtime. **No**
  alarm or OCP status. Connector 2 only appears when it is in use.
- In the **free** HWiNFO version, shared memory is time-limited (presumably 12 h, not
  verified). The user has a license.

## 9. Display extensions (after `HidSource`)

- Status line: replace or supplement ALARM C1/C2 with the actual reason
  ("PIN OCP", "IMBALANCE HW", "18 A"). Latched as before.
- New metric "AC": input voltage (V) and AC power (`P_Out / Eff`).
- Protection alarms from `E1` (OVP/UVP/OCP/OPP/OTP/fan/AC) as a combined chip once the
  bit layout is clarified.
- Show the source in the notice/tooltip ("HID", "Afterburner", "HWiNFO").
- With faster polling: per-pin peaks from all samples, display still at ≤ 4 Hz.

## 10. Open questions

| Question | How to clarify |
|---|---|
| Meaning of READ_ALL words 22–30 | `EnableLog = 1` (section 3, step 1) |
| Firmware update rate | measurement test 6.2 |
| Formats of C1 remaining bytes, E1, D0/D1, 0x42 | measurement test 6.3 |
| Layout of C0 (OCP thresholds); should C0 go into the whitelist? | only after clarifying how MSI Center reads it |
| Firmware behavior on parallel requests | measurement test 6.4 |
| Device access without admin | measurement test 6.5 |
| Does HWiNFO use the MSI mutex? | Process Explorer / `handle.exe` as admin |
| Monitor rate of MSI Center | not decisive (respects the mutex) |

## 11. Risks

| Risk | Mitigation |
|---|---|
| A write command changes fan/OCP/buzzer settings or stores them (`0xF1`) | whitelist (5.6), no generic send API, test |
| Afterburner's polling thread hangs on a collision (reads without timeout) | low rate, same commands as Afterburner, measurement test with abort criterion |
| Afterburner occasionally loses a cycle (echo mismatch) | harmless (value 1 s old), only send E0/C1 |
| A firmware update changes the layout | check handshake/PID, "untested" notice, MAHM as fallback |
| No write access without admin | then disable the HID source, use MAHM/HWiNFO |
| MSI Center overwrites fan settings on startup | does not affect PinGuard, for information only |

## 12. Done when

- [ ] With `--source hid`, PinGuard shows the same values as Afterburner/HWiNFO (comparison as in v1).
- [ ] Runs without Afterburner, falls back to MAHM when the device is missing and reconnects
      automatically.
- [ ] Hardware OCP status with reason is displayed and latched.
- [ ] Afterburner, MSI Center and HWiNFO run in parallel without dropouts (Afterburner log clean).
- [ ] Only whitelisted commands are sent (test exists).
- [ ] Resource usage measured and updated in the README (< 15 MB WS, ≈ 0 % CPU, 0 GPU).

---

## 13. Smaller open items (independent of the above)

- [ ] Test reconnecting with the real Afterburner: quit → notice, restart → values back
      after about 2 s. So far only tested with a simulated section (`--mahm-name`).
- [ ] Optional: always show connector 2 (like MSI Center) instead of only when it carries current.

## 14. Sources

- `docs/hid-protocol-analysis.md`: full analysis with evidence (PSU.dll from
  Afterburner 4.6.7, `API_Power Supply Unit.dll` 1.0.0.38 from MSI Center, MSI Center log,
  HWiNFO64 8.60).
- `README.md`: v1 architecture, decision for GDI, measurements.
- `src/source/mahm.rs`: MAHM layout and source names.
