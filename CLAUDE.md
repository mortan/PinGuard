# CLAUDE.md

PinGuard is a compact Windows desktop monitor for the 12V-2x6 (12VHPWR) connector and the PSU
values of the **MSI MPG Ai1300TS PCIE5**. It reads MSI Afterburner's shared memory
(`MAHMSharedMemory`, PSU plugin `PSU.dll`) read-only and shows per-pin currents, faults and metrics.
It runs on a second monitor while gaming, so **minimal RAM/CPU/GPU usage is the top requirement**.

Docs: `README.md` (English, user-facing only: features, controls, settings, resource usage),
`docs/plan-data-sources.md` and `docs/hid-protocol-analysis.md` (planned direct HID / HWiNFO data
sources). INI keys and defaults are defined in `src/settings.rs` and `src/model.rs`.

Measured (release build, live data, game at ~500 W GPU load, 120 s): 14.2 MB working set, 3.0 MB
private, 0.03 % total CPU (16 logical CPUs), 4 threads, no GPU engine usage. The first Direct2D
version used 43 MB WS, 74 MB private, 21 threads. Re-measure after changes to drawing or polling.

Thresholds: 12V-2x6 spec allows 9.2 A per pin (≈ 8.3 A/pin at 600 W). Default colors: green < 8 A,
yellow 8–9.2 A, red ≥ 9.2 A, full scale 12 A.

## Build and test

```bash
cargo build --release      # target\release\pinguard.exe (~340 KB, no console window)
cargo test
cargo run -- --mock        # fake data, no hardware needed (--mock2: both connectors in use)
```

- Windows only, MSVC toolchain. `build.rs` embeds `assets/pinguard.ico` by writing a `.res` file
  directly (no `rc.exe`, no extra crate). Regenerate the icon with `python assets/make_icon.py`.
- Other flags: `--topmost`, `--mahm-name NAME` (read a different shared memory section, tests only).
- Tests must not touch `%APPDATA%\PinGuard`, the registry or the notification area (temp dir only).

## Architecture

```
src/
  main.rs          window, message loop, timer, DPI, hit test, menu
  model.rs         PsuSnapshot, metrics, peaks, faults, alarm memory, history ring buffer, limits
  render.rs        layout + drawing (GDI+/GDI), colors
  i18n.rs          all visible texts (German/English), number format, language choice
  settings.rs      INI persistence (%APPDATA%\PinGuard\pinguard.ini)
  settings_dlg.rs  native Win32 settings dialog (in-memory template)
  tip.rs           custom tooltip popup
  tray.rs          tray icon (icons generated at runtime)
  events.rs        event log events.csv
  autostart.rs     HKCU\...\Run entry
  source/          trait PsuSource; mahm.rs (Afterburner), mock.rs (fake data)
```

## Hard constraints

- **No Direct2D / D3D / GPU APIs and no UI framework.** Direct2D loads D3D11/WARP, RTSS then injects
  into the process (43 MB WS vs. 14 MB now). Draw with GDI+/GDI into a backbuffer.
- **No continuous rendering.** Poll every 250 ms, redraw only when values change or on
  `WM_PAINT`/resize; nothing is drawn while minimized.
- Look up MAHM entries **by name, never by index**; rebuild the lookup when `dwNumEntries` changes.
  Handle `0xDEAD` signature and a frozen timestamp (10 s) by reconnecting.
- Snapshot values are `Option<f32>`; missing sources are normal.
- Data access stays behind `PsuSource` so a direct HID source can replace Afterburner later.
  Nothing may ever be sent to the PSU outside the whitelist in `docs/plan-data-sources.md`.
- Keep dependencies minimal (currently only the `windows` crate); release profile is size-optimized.

## Conventions

- Every user-visible string goes through `src/i18n.rs` with both German and English text; numbers
  use the language's decimal separator. The settings dialog accepts both `,` and `.`.
- New settings get an INI key with a default; invalid or missing values fall back to the default.
  Document user-visible features in `README.md` (English, no implementation details).
- Code comments are in German, docs in English. Commit messages are in English, imperative summary line plus
  a bullet list body.
- The user communicates in German.
