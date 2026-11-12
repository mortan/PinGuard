# Erzeugt ein RTSS-Overlay-Editor-Layout mit vertikalen Balken je 12V-2x6-Pin
# Datenquelle: MSI-Afterburner-PSU-Plugin (MSI MPG Ai1300TS PCIE5 per USB)
#
# Aufruf:  python make_ovl.py [--stecker 1|2] [--x PIXEL] [--y PIXEL] [--out DATEI]
#   --x/--y verschieben das komplette Panel (linke obere Ecke, in Pixeln).
#   So bleiben alle Layer exakt zueinander ausgerichtet.
import argparse

ap = argparse.ArgumentParser()
ap.add_argument("--stecker", default="1", choices=["1", "2"])
ap.add_argument("--x", type=int, default=0)
ap.add_argument("--y", type=int, default=0)
ap.add_argument("--out", default="12VHPWR Pins.ovl")
args = ap.parse_args()
CONN, OUT, OFF_X, OFF_Y = args.stecker, args.out, args.x, args.y

SCALE_MAX = 12.0      # Balken-Vollausschlag in A (wie MaxCurrent im PSU-Plugin)
WARN, LIMIT = 8.0, 9.2  # gelb ab 8 A, rot ab 9,2 A (Spezifikation 12V-2x6)

GREEN, YELLOW, RED = "3CDC78", "FFC400", "FF3B3B"
DIM, WHITE = "A0FFFFFF", "F0FFFFFF"

# Geometrie (Pixel)
W, H = 240, 226
BAR_W, BAR_H, BAR_TOP = 22, 110, 44
BAR_X0, BAR_STEP = 18, 36

sources, layers = [], []


def src_ab(name, srcname, units, fmt="%.1f"):
    sources.append(dict(Name=name, Units=units, Format=fmt, Formula="",
                        Provider="MSI Afterburner", SrcId="000000F6",
                        Gpu="00000000", SrcName=srcname))


def src_hal(name, formula, units="", fmt="%.1f"):
    sources.append(dict(Name=name, Units=units, Format=fmt, Formula=formula,
                        Provider="HAL", ID="Stub"))


def layer(name, text, x, y, ex, ey, size=100, color=WHITE, bgnd=None,
          origin=0, **extra):
    d = dict(Name=name, Text=text, PositionX=x + OFF_X,
             PositionY=y + OFF_Y, ExtentX=ex,
             ExtentY=ey, ExtentOrigin=origin, FixedAlignment=1, Size=size,
             TextColor=color)
    if bgnd:
        d["BgndColor"] = bgnd
    d.update(extra)
    layers.append(d)


def thresholds(src):
    return f"{GREEN},,{WARN},{YELLOW},{WARN},{LIMIT},{RED},{LIMIT},({src},0)"


# ---- Quellen -------------------------------------------------------------
pins = [f"Pin{i}" for i in range(1, 7)]
for i, p in enumerate(pins, 1):
    src_ab(p, f"PSU 12VHPWR{CONN}_{i} current", "A")
src_ab("V12", "PSU 12V voltage", "V", "%.2f")
src_ab("PinAlarm", f"PSU 12VHPWR{CONN} alarm", "", "%.0f")
src_hal("BarBg", "1", fmt="%.0f")
src_hal("Total", "+".join(pins), "A")
src_hal("Watt", "Total*V12", "W", "%.0f")
mx = mn = pins[0]
for p in pins[1:]:
    mx, mn = f"max({mx},{p})", f"min({mn},{p})"
src_hal("PinMax", mx, "A")
src_hal("PinMin", mn, "A")
src_hal("Delta", "PinMax-PinMin", "A")

# ---- Layer ---------------------------------------------------------------
layer("Panel", "", 0, 0, W, H, size=1, bgnd="D8101418")
layer("Titel", f"12V-2x6  #{CONN}<IF PinAlarm>  <C={RED}>ALARM<C><IF>",
      10, 6, 150, 22, size=90)
layer("Leistung", "%Watt%<S=60> W<S>", W - 90, 6, 80, 22, size=90, origin=2)
layer("Trenner oben", "", 8, 32, W - 16, 1, size=1, bgnd="40FFFFFF")

for i, p in enumerate(pins):
    x = BAR_X0 + i * BAR_STEP
    layer(f"{p} Hintergrund", f"<G=BarBg,{BAR_W},{BAR_H},0,0,1,28>",
          x, BAR_TOP, BAR_W, BAR_H, color="28FFFFFF")
    layer(f"{p} Balken", f"<G={p},{BAR_W},{BAR_H},0,0,{SCALE_MAX:g},28>",
          x, BAR_TOP, BAR_W, BAR_H, color=thresholds(p))

# Markierung der 9,2-A-Grenze quer ueber alle Balken
y_lim = BAR_TOP + round(BAR_H * (1 - LIMIT / SCALE_MAX))
layer("Grenze 9.2A", "", BAR_X0 - 4, y_lim, 5 * BAR_STEP + BAR_W + 8, 1,
      size=1, bgnd="90FF3B3B")

for i, p in enumerate(pins):
    cx = BAR_X0 + i * BAR_STEP - (BAR_STEP - BAR_W) // 2
    layer(f"{p} Wert", f"%{p}%", cx, BAR_TOP + BAR_H + 4, BAR_STEP, 16,
          size=70, origin=4, color=thresholds(p))
    layer(f"{p} Label", f"P{i + 1}", cx, BAR_TOP + BAR_H + 21, BAR_STEP, 14,
          size=55, origin=4, color=DIM)

layer("Trenner unten", "", 8, H - 34, W - 16, 1, size=1, bgnd="40FFFFFF")
layer("Summen",
      f"<C={DIM}>Sum<C> %Total%A   <C={DIM}>Max<C> %PinMax%A   "
      f"<C={DIM}>Diff<C> %Delta%A",
      10, H - 28, W - 20, 20, size=65)

# ---- Datei schreiben -----------------------------------------------------
out = ["[Master]", "Implementation=2", "FontFace=Segoe UI", "FontHeight=-20",
       "FontWeight=700", "ZoomRatio=1",
       "[Settings]", "Name=12V-2x6 Pins", "EnvVars=", "RefreshPeriod=250",
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
