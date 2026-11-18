"""Generates the app icon (original artwork, no third-party assets).

Writes assets/icon.ico (16/32/48/256 PNG entries) and gui/icon64.rgba
(raw 64x64 RGBA for the GUI window icon). Pure standard library.
"""
import math, os, struct, zlib

HERE = os.path.dirname(os.path.abspath(__file__))


def rrect(px, py, cx, cy, hw, hh, r):
    qx, qy = abs(px - cx) - (hw - r), abs(py - cy) - (hh - r)
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r


def shade(x, y):
    """Colour at a point of the 256x256 design; returns (r,g,b,a) 0..255."""
    # background rounded square with a vertical blue gradient
    if rrect(x, y, 128, 128, 124, 124, 52) > 0:
        return (0, 0, 0, 0)
    t = y / 256
    bg = (int(30 + (13 - 30) * t), int(136 + (71 - 136) * t), int(229 + (161 - 229) * t))
    # mouse body
    if rrect(x, y, 128, 136, 48, 84, 46) <= 0:
        dark = (21, 66, 140)
        # split between buttons: horizontal line then vertical line up
        if abs(y - 108) <= 3 and abs(x - 128) <= 48:
            return (*dark, 255)
        if abs(x - 128) <= 3 and y <= 108:
            return (*dark, 255)
        # scroll wheel
        if rrect(x, y, 128, 84, 8, 17, 8) <= 0:
            return (*dark, 255)
        return (255, 255, 255, 255)
    return (*bg, 255)


def render(size):
    ss = 4
    out = bytearray()
    for j in range(size):
        for i in range(size):
            acc = [0, 0, 0, 0]
            for sy in range(ss):
                for sx in range(ss):
                    x = (i + (sx + 0.5) / ss) * 256 / size
                    y = (j + (sy + 0.5) / ss) * 256 / size
                    r, g, b, a = shade(x, y)
                    acc[0] += r * a
                    acc[1] += g * a
                    acc[2] += b * a
                    acc[3] += a
            n = ss * ss
            a = acc[3] / n
            if acc[3] == 0:
                out += bytes((0, 0, 0, 0))
            else:
                out += bytes((round(acc[0] / acc[3]), round(acc[1] / acc[3]), round(acc[2] / acc[3]), round(a)))
    return bytes(out)


def png(size, rgba):
    raw = b"".join(b"\x00" + rgba[y * size * 4:(y + 1) * size * 4] for y in range(size))

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


sizes = [16, 32, 48, 256]
images = [png(s, render(s)) for s in sizes]
header = struct.pack("<HHH", 0, 1, len(sizes))
offset = 6 + 16 * len(sizes)
entries = b""
for s, im in zip(sizes, images):
    entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(im), offset)
    offset += len(im)
with open(os.path.join(HERE, "icon.ico"), "wb") as f:
    f.write(header + entries + b"".join(images))
with open(os.path.join(HERE, "..", "gui", "icon64.rgba"), "wb") as f:
    f.write(render(64))
with open(os.path.join(HERE, "icon256.png"), "wb") as f:
    f.write(images[-1])
print("ok")
