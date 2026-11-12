# CLAUDE.md

PinGuard is a compact Windows desktop monitor for the 12V-2x6 (12VHPWR) connector and the PSU
values of the **MSI MPG Ai1300TS PCIE5**. It reads MSI Afterburner's shared memory
(`MAHMSharedMemory`, PSU plugin `PSU.dll`) read-only and shows per-pin currents, faults and metrics.
It runs on a second monitor while gaming, so **minimal RAM/CPU/GPU usage is the top requirement**.

Detailed docs (German): `README.md` (features, INI keys, design decisions, measurements),
`HANDOVER.md` (original brief, MAHM layout, thresholds), `PLAN-Datenquellen.md` and
`docs/hid-protokoll-analyse.md` (planned direct HID / HWiNFO data sources).

## Build and test

```bash
cargo build --release      # target\release\pinguard.exe (~190 KB, no console window)
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
rtss/              RTSS overlays and Python generators, not part of the app
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
  Nothing may ever be sent to the PSU outside the whitelist in `PLAN-Datenquellen.md`.
- Keep dependencies minimal (currently only the `windows` crate); release profile is size-optimized.

## Conventions

- Every user-visible string goes through `src/i18n.rs` with both German and English text; numbers
  use the language's decimal separator. The settings dialog accepts both `,` and `.`.
- New settings get an INI key with a default; invalid or missing values fall back to the default.
  Document new keys and features in `README.md`.
- Code comments and docs are in German. Commit messages are in English, imperative summary line plus
  a bullet list body.
- The user communicates in German.
