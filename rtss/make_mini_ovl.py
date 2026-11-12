# Minimalistische 12V-2x6-Zeile im Stil des klassischen Afterburner-OSD:
#   12V   6.9 A   ▮▮▮▮▮▮   Diff 0.4 A
# Datenquelle: MSI-Afterburner-PSU-Plugin (MSI MPG Ai1300TS PCIE5)
#
# Aufruf:  python make_mini_ovl.py [--stecker 1|2] [--x PX] [--y PX]
#                                  [--nur-warnung] [--out DATEI]
#   --nur-warnung: Zeile erscheint nur bei Auffaelligkeiten
#                  (ein Pin >= 8 A, Diff >= 1.5 A oder Netzteil-Alarm)
import argparse

ap = argparse.ArgumentParser()
ap.add_argument("--stecker", default="1", choices=["1", "2"])
ap.add_argument("--x", type=int, default=0)
ap.add_argument("--y", type=int, default=0)
ap.add_argument("--nur-warnung", action="store_true")
ap.add_argument("--out")
args = ap.parse_args()
CONN, OFF_X, OFF_Y = args.stecker, args.x, args.y
OUT = args.out or ("12V Mini Warnung.ovl" if args.nur_warnung else "12V Mini.ovl")

SCALE_MAX = 12.0
WARN, LIMIT = 8.0, 9.2        # gelb / rot pro Pin
DIFF_WARN = 1.5               # Schieflast-Schwelle fuer --nur-warnung

LABEL = "E0C000"              # Label-Farbe (Gold), analog zu GPU/MEM/D3D9
VALUE = "FF9900"              # Werte-Orange wie im Afterburner-OSD
GREEN, YELLOW, RED = "3CDC78", "FFC400", "FF3B3B"
DIM = "A0FFFFFF"

# Geometrie (Pixel) - eine Textzeile
ROW_H = 24
COL_VALUE = 95                # Spalte der Werte (wie "92 %" hinter "GPU")
BAR_X0, BAR_W, BAR_STEP, BAR_H = 175, 5, 8, 18
COL_DIFF = BAR_X0 + 5 * BAR_STEP + BAR_W + 20

sources, layers = [], []


def src_ab(name, srcname, units, fmt="%.1f"):
    sources.append(dict(Name=name, Units=units, Format=fmt, Formula="",
                        Provider="MSI Afterburner", SrcId="000000F6",
                        Gpu="00000000", SrcName=srcname))


def src_hal(name, formula, units="", fmt="%.1f"):
    sources.append(dict(Name=name, Units=units, Format=fmt, Formula=formula,
                        Provider="HAL", ID="Stub"))


def layer(name, text, x, y, ex, ey, color, size=100, origin=0):
    layers.append(dict(Name=name, VisibilitySource="Show", Text=text,
                       PositionX=x + OFF_X, PositionY=y + OFF_Y, ExtentX=ex,
                       ExtentY=ey, ExtentOrigin=origin, FixedAlignment=1,
                       Size=size, TextColor=color))


def pin_colors(src):
    return f"{GREEN},,{WARN},{YELLOW},{WARN},{LIMIT},{RED},{LIMIT},({src},0)"


# ---- Quellen -------------------------------------------------------------
pins = [f"Pin{i}" for i in range(1, 7)]
for i, p in enumerate(pins, 1):
    src_ab(p, f"PSU 12VHPWR{CONN}_{i} current", "A")
src_ab("PinAlarm", f"PSU 12VHPWR{CONN} alarm", "", "%.0f")
src_hal("BarBg", "1", fmt="%.0f")
mx = mn = pins[0]
for p in pins[1:]:
    mx, mn = f"max({mx},{p})", f"min({mn},{p})"
src_hal("PinMax", mx, "A")
src_hal("PinMin", mn, "A")
src_hal("Delta", "PinMax-PinMin", "A")
if args.nur_warnung:
    src_hal("Show", f"(PinMax>={WARN})||(Delta>={DIFF_WARN})||(PinAlarm>0)",
            fmt="%.0f")
else:
    src_hal("Show", "1", fmt="%.0f")

# ---- Layer ---------------------------------------------------------------
layer("Label", "12V", 0, 0, 80, ROW_H, LABEL)
layer("Max Pin", "%PinMax%<S=50> A<S>", COL_VALUE, 0, 75, ROW_H,
      f"{VALUE},,{LIMIT},{RED},{LIMIT},(PinMax,0)")

bar_y = (ROW_H - BAR_H) // 2 + 1
for i, p in enumerate(pins):
    x = BAR_X0 + i * BAR_STEP
    layer(f"{p} Hintergrund", f"<G=BarBg,{BAR_W},{BAR_H},0,0,1,28>",
          x, bar_y, BAR_W, BAR_H, "30FFFFFF")
    layer(f"{p} Balken", f"<G={p},{BAR_W},{BAR_H},0,0,{SCALE_MAX:g},28>",
          x, bar_y, BAR_W, BAR_H, pin_colors(p))

layer("Diff", f"<C={DIM}><S=60>Diff <S><C>%Delta%<S=50> A<S>",
      COL_DIFF, 0, 110, ROW_H,
      f"{VALUE},,{DIFF_WARN},{RED},{DIFF_WARN},(Delta,0)")

# ---- Datei schreiben -----------------------------------------------------
out = ["[Master]", "Implementation=2", "FontFace=Arial", "FontHeight=-20",
       "FontWeight=700", "ZoomRatio=1",
       "[Settings]", "Name=12V Mini", "EnvVars=", "RefreshPeriod=250",
       "LockUserSettings=0", "EmbeddedImage=", "PingAddr=",
       "[General]", f"Sources={len(sources)}", "Tables=0",
       f"Layers={len(layers)}"]
for i, s in enumerate(sources):
    out.append(f"[Source{i}]")
    out += [f"{k}={v}" for k, v in s.items()]
for i, l in enumerate(layers):
    out.append(f"[Layer{i}]")
    out += [f"{k}={v}" for k, v in l.items()]

with open(OUT, "w", encoding="cp1252", newline="\r\n") as f:
    f.write("\n".join(out) + "\n")
print(f"{OUT}: {len(sources)} Quellen, {len(layers)} Layer")
