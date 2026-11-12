# PinGuard

A tiny monitor for the 12V-2x6 (12VHPWR) GPU power connector and the PSU values of the
**MSI MPG Ai1300TS PCIE5**. It shows the current on every pin, warns about overcurrent and
imbalance, and keeps an eye on power, efficiency, temperature and fan.

PinGuard is made to sit on a second monitor while you game, so it is built to cost you
**practically nothing**:

- **~14 MB RAM** (only **3 MB** of it private; the rest is shared Windows DLLs)
- **0.03 % CPU** on average (about 4 ms of work per second, 4 threads)
- **No GPU load at all**: no Direct3D, no overlay hooks, no animation loop. It only redraws when new data arrives
- A single **~340 KB** EXE, with no installer and no runtime

![PinGuard: live pin currents rising until pin 3 triggers an overcurrent alarm, which stays latched](docs/demo.gif)

## Requirements

- Windows 11
- **MSI MPG Ai1300TS PCIE5** connected over USB
- **MSI Afterburner** (tested with 4.6.7) with its bundled **PSU monitoring plugin** enabled
  (`PSU.dll`, enabled in Afterburner's monitoring settings). PinGuard reads the values
  Afterburner publishes. The order of the monitoring sources in Afterburner doesn't matter.

If Afterburner isn't running, PinGuard shows a notice and connects automatically as soon as
Afterburner is back.

## Installation

Put `pinguard.exe` anywhere and start it. To run it at logon, right-click → *Start with Windows*.

Settings are stored in `%APPDATA%\PinGuard\pinguard.ini`.

## What you see

| Area | Content |
|---|---|
| Pin bars | The 6 pins of the active connector. Green below 8 A, yellow from 8 A, red from 9.2 A (the 12V-2x6 per-pin limit). A small line marks each pin's peak. An optional dashed line shows the pin average. Connector 2 only appears once it carries current. |
| Connector | Power, total current, highest pin, imbalance. Optional: load relative to 600 W and GPU board power. |
| PSU | DC output power, efficiency, temperature, fan speed. Can be replaced by a **2-minute history chart** of connector power. |
| Rails | 12 V / 5 V / 3.3 V voltage and current. |
| Connection dot | Next to the PSU heading. Green = data is coming in, red = no data, orange ring = the connection was lost in the meantime. |
| Status line | **ALARM C1/C2** (alarm reported by the PSU itself), **OVERCURRENT** (a pin above the red limit), **IMBALANCE**. Red = active, orange outline = was active (latched until you click it). |

With the history chart enabled, the PSU group shows connector power over the last two minutes,
with the peak marked:

![History chart following a load cycle: power climbs to a 690 W peak and drops again, then the imbalance alarm fires](docs/history.gif)

Hover over almost anything for a tooltip with exact values and limits. While an alarm is active or
latched, its tooltip shows when it fired, how long it lasted and the worst value, e.g.
"Triggered 14:32, pin 3: 9.6 A, duration 4 s".

Note: PSU power is the DC output power. The draw from the wall is roughly PSU power ÷ efficiency.

## Controls

- **Drag** anywhere to move the window. **Resize** from the edges; the aspect ratio is kept.
- **Minimal view / minimize / close:** buttons appear at the top right while the mouse is over the
  window.
- **M key:** switch between the full and the minimal view (while the window is active).
- **Click the status line or the connection dot:** clear latched alarms.
- **Double-click:** reset the peaks.
- **Right-click:** menu with reset, *Limits and alarms …*, *Minimal view*, *Always on top*,
  *Minimize to notification area*, *Start with Windows*, *Language*, *Event log* and *Exit*.
- Only one instance runs. Starting it again brings the existing window to the front.

### Minimal view

The view button at the top right, the **M** key or right-click → *Minimal view* shrink the window
to the pin bars. Below them are the connector power
and a single status field: **ALL OK** with a green dot, red with the alarm name while an alarm is
active (further alarms as "+1", "+2" …), or an orange outline while an alarm is latched. The link
to Afterburner is part of this field, so a lost connection shows up there. Hover over the field to
see every alarm's state, and click it to clear latched alarms. The same button, key or menu item
returns to the full view. The window keeps its height, and the choice is remembered.

### Tray

With *Minimize to notification area* enabled, minimizing hides the window from the taskbar. A round icon in
the notification area shows the state: **green** = OK, **yellow** = warning level or a latched
alarm, **red** = alarm active, **grey** = no data. Its tooltip shows connector power, the highest
pin and active alarms. Left-click brings the window back, right-click opens the menu.

### Limits and alarms

Right-click → *Limits and alarms …* (changes apply immediately):

| Setting | Default |
|---|---|
| Pin yellow from | 8 A |
| Pin red / overcurrent from | 9.2 A |
| Bar full scale | 12 A |
| Imbalance measured as | amps |
| Imbalance limit (highest − lowest pin), amps mode | 3 A |
| Imbalance limit (largest deviation of a pin from the average), percent mode | 30 % |
| Percent mode only above a connector current of | 20 A |
| Rated power per connector (for the load figure) | 600 W |

**Amps or percent?** *Amps* compares the strongest with the weakest pin. It is meaningful under
high load but can miss even a failed pin at low load. *Percent* scales with the load: a failed pin
is 100 % deviation. Below the minimum current it isn't evaluated, because measurement noise alone
causes large percentages there. Either way the imbalance figure turns yellow at half the limit and
red at the limit.

For every alarm (connector 1, connector 2, overcurrent, imbalance, no data) you can choose:

- **Active:** evaluate and show it
- **Latch:** keep it marked in orange until you click the status line
- **Sound:** play a warning sound when it fires
- **Show:** bring the window to the front (without stealing focus) and flash it in the taskbar

By default all alarms are active and latching, without sound and without *Show*.

The same dialog lets you show or hide each metric, the rails, the yellow warning line, the average
line and the history chart.

### Event log

PinGuard appends a line to `%APPDATA%\PinGuard\events.csv` for every new alarm: time, alarm type,
all 12 pin currents, 12 V voltage and connector power (semicolon-separated, decimal point). At
1 MB the file is renamed to `events.old.csv` and a new one is started. Turn it off or open the
folder via right-click → *Event log*.

### Language

English and German. By default PinGuard follows the Windows display language. You can change it
via right-click → *Language*; it applies immediately.

## Command line

```
pinguard.exe [--topmost] [--mock]
```

- `--topmost` starts the window always on top.
- `--mock` shows simulated data (including overcurrent, imbalance and a short dropout) so you can
  try PinGuard without the hardware.

## Building from source

```bash
cargo build --release
```

Requires Rust with the MSVC toolchain. The result is `target\release\pinguard.exe`.

## License

MIT, see [LICENSE](LICENSE).
