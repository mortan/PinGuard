"""Erzeugt assets/pinguard.ico (und optional eine PNG-Vorschau).

Aufruf: python assets/make_icon.py [vorschau.png]

Motiv: dunkle abgerundete Kachel, Pin-Balken in Grün, ein Balken in Gelb
kurz unter der roten Grenzlinie. Reines Python ohne Zusatzpakete; gezeichnet
wird mit 4-fachem Supersampling je Größe. Kleine Größen (< 32 px) bekommen
vier statt sechs Balken, damit sie lesbar bleiben.
"""
import struct
import sys
import zlib
from pathlib import Path

SIZES = [16, 20, 24, 32, 40, 48, 64, 256]
SS = 4  # Supersampling je Achse

PANEL = (0x1A, 0x1D, 0x23)
GREEN = (0x3C, 0xDC, 0x78)
YELLOW = (0xFF, 0xC4, 0x00)
RED = (0xFF, 0x3B, 0x3B)


def shapes(size):
    """Formen im 256er-Raster: (x0, y0, x1, y1, Radius, Farbe)."""
    out = [(0, 0, 256, 256, 54, PANEL)]
    if size < 32:
        heights = [0.50, 0.56, 0.84, 0.52]
        width, radius = 30, 6
    else:
        heights = [0.52, 0.57, 0.54, 0.84, 0.56, 0.50]
        width, radius = 19, 5
    x0, x1, bottom, full = 40, 216, 212, 150
    n = len(heights)
    step = (x1 - x0 - width) / (n - 1)
    for i, h in enumerate(heights):
        x = x0 + i * step
        col = YELLOW if h > 0.8 else GREEN
        out.append((x, bottom - h * full, x + width, bottom, radius, col))
    line = 14 if size < 32 else 9
    out.append((30, 50, 226, 50 + line, line / 2, RED))
    return out


def inside(px, py, s):
    x0, y0, x1, y1, r, _ = s
    if not (x0 <= px < x1 and y0 <= py < y1):
        return False
    cx = min(max(px, x0 + r), x1 - r)
    cy = min(max(py, y0 + r), y1 - r)
    return (px - cx) ** 2 + (py - cy) ** 2 <= r * r


def render(size):
    """RGBA-Zeilen (oben nach unten), nicht vormultipliziert."""
    sh = shapes(size)
    n = size * SS
    k = 256 / n
    rows = []
    for y in range(size):
        row = bytearray()
        for x in range(size):
            r = g = b = a = 0
            for sy in range(SS):
                py = (y * SS + sy + 0.5) * k
                for sx in range(SS):
                    px = (x * SS + sx + 0.5) * k
                    col = None
                    for s in sh:
                        if inside(px, py, s):
                            col = s[5]
                    # Balken und Linie nur innerhalb der Kachel
                    if col is not None and inside(px, py, sh[0]):
                        r += col[0]
                        g += col[1]
                        b += col[2]
                        a += 1
            if a:
                row += bytes((round(r / a), round(g / a), round(b / a), round(a * 255 / (SS * SS))))
            else:
                row += bytes(4)
        rows.append(bytes(row))
    return rows


def png(size, rows):
    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    raw = b"".join(b"\x00" + r for r in rows)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def bmp(size, rows):
    """Icon-DIB: 32 Bit BGRA von unten nach oben, danach leere AND-Maske."""
    head = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    xor = b"".join(bytes(v for i in range(0, len(r), 4) for v in (r[i + 2], r[i + 1], r[i], r[i + 3])) for r in reversed(rows))
    mask = bytes(((size + 31) // 32) * 4) * size
    return head + xor + mask


def main():
    images = []
    for size in SIZES:
        rows = render(size)
        images.append((size, png(size, rows) if size >= 256 else bmp(size, rows), rows))
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, data, _ in images:
        out += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    for _, data, _ in images:
        out += data
    here = Path(__file__).resolve().parent
    (here / "pinguard.ico").write_bytes(out)
    print(f"pinguard.ico: {len(out)} Bytes, Größen {SIZES}")
    if len(sys.argv) > 1:
        # Vorschau: 256er-Bild und daneben die kleinen Größen in Originalgröße
        big = images[-1][2]
        small = [im for im in images if im[0] in (48, 32, 24, 16)]
        w = 256 + 24 + 48
        canvas = [bytearray(bytes((0x10, 0x12, 0x16, 255)) * w) for _ in range(256)]
        for y, r in enumerate(big):
            blit(canvas[y], 0, r)
        y0 = 0
        for size, _, rows in reversed(small):
            for y, r in enumerate(rows):
                blit(canvas[y0 + y], 280, r)
            y0 += size + 16
        Path(sys.argv[1]).write_bytes(png_rect(w, 256, [bytes(c) for c in canvas]))


def blit(dst, x, row):
    for i in range(0, len(row), 4):
        a = row[i + 3] / 255
        o = (x + i // 4) * 4
        for c in range(3):
            dst[o + c] = round(row[i + c] * a + dst[o + c] * (1 - a))


def png_rect(w, h, rows):
    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    raw = b"".join(b"\x00" + r for r in rows)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


if __name__ == "__main__":
    main()
