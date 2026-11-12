# Kompakte 12V-2x6-Variante: nur die 6 vertikalen Balken (+ 9,2-A-Linie
# und Werte darunter), gleiche Farben wie das grosse Panel, ohne Rahmen/Titel.
# Datenquelle: MSI-Afterburner-PSU-Plugin (MSI MPG Ai1300TS PCIE5)
#
# Aufruf:  python make_bars_ovl.py [--stecker 1|2] [--x PX] [--y PX]
#                                  [--ohne-werte] [--out DATEI]
import argparse

ap = argparse.ArgumentParser()
ap.add_argument("--stecker", default="1", choices=["1", "2"])
ap.add_argument("--x", type=int, default=0)
ap.add_argument("--y", type=int, default=0)
ap.add_argument("--ohne-werte", action="store_true")
ap.add_argument("--out")
args = ap.parse_args()
CONN, OFF_X, OFF_Y = args.stecker, args.x, args.y
OUT = args.out or ("12V Balken ohne Werte.ovl" if args.ohne_werte
                   else "12V Balken.ovl")

SCALE_MAX = 12.0
WARN, LIMIT = 8.0, 9.2
GREEN, YELLOW, RED = "3CDC78", "FFC400", "FF3B3B"

# Geometrie (Pixel) - ca. 60 % des grossen Panels
BAR_W, BAR_H, BAR_STEP = 14, 66, 22
BAR_X0, BAR_TOP = 4, 0

sources, layers = [], []


def src_ab(name, srcname, units, fmt="%.1f"):
    sources.append(dict(Name=name, Units=units, Format=fmt, Formula="",
                        Provider="MSI Afterburner", SrcId="000000F6",
                        Gpu="00000000", SrcName=srcname))


def src_hal(name, formula, units="", fmt="%.1f"):
    sources.append(dict(Name=name, Units=units, Format=fmt, Formula=formula,
                        Provider="HAL", ID="Stub"))


def layer(name, text, x, y, ex, ey, color, size=100, origin=0, bgnd=None):
    d = dict(Name=name, Text=text, PositionX=x + OFF_X, PositionY=y + OFF_Y,
             ExtentX=ex, ExtentY=ey, ExtentOrigin=origin, FixedAlignment=1,
             Size=size, TextColor=color)
    if bgnd:
        d["BgndColor"] = bgnd
    layers.append(d)


def pin_colors(src):
    return f"{GREEN},,{WARN},{YELLOW},{WARN},{LIMIT},{RED},{LIMIT},({src},0)"


pins = [f"Pin{i}" for i in range(1, 7)]
for i, p in enumerate(pins, 1):
    src_ab(p, f"PSU 12VHPWR{CONN}_{i} current", "A")
src_hal("BarBg", "1", fmt="%.0f")

for i, p in enumerate(pins):
    x = BAR_X0 + i * BAR_STEP
    layer(f"{p} Hintergrund", f"<G=BarBg,{BAR_W},{BAR_H},0,0,1,28>",
          x, BAR_TOP, BAR_W, BAR_H, "28FFFFFF")
    layer(f"{p} Balken", f"<G={p},{BAR_W},{BAR_H},0,0,{SCALE_MAX:g},28>",
          x, BAR_TOP, BAR_W, BAR_H, pin_colors(p))

y_lim = BAR_TOP + round(BAR_H * (1 - LIMIT / SCALE_MAX))
layer("Grenze 9.2A", "", BAR_X0 - 3, y_lim, 5 * BAR_STEP + BAR_W + 6, 1,
      "FFFFFF", size=1, bgnd="90FF3B3B")

if not args.ohne_werte:
    for i, p in enumerate(pins):
        cx = BAR_X0 + i * BAR_STEP - (BAR_STEP - BAR_W) // 2
        layer(f"{p} Wert", f"%{p}%", cx, BAR_TOP + BAR_H + 2, BAR_STEP, 14,
              pin_colors(p), size=55, origin=4)

out = ["[Master]", "Implementation=2", "FontFace=Adderley Bold", "FontHeight=-22",
       "FontWeight=700", "ZoomRatio=1",
       "[Settings]", "Name=12V Balken", "EnvVars=", "RefreshPeriod=250",
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
