#!/usr/bin/env python3
"""Generates the CloudDeck app icon SVGs and the export manifest.

    python3 make_icon.py            # writes the svgs, export.html, export.json, preview.html
    # then render PNGs and build the .icns (see README.md)

Concept: a deck of clouds. Four clouds (the Lucide `cloud` shape) spread like
cards on a table, one tint per cloud provider; the front one is white and
sliced into stacked units. No logos, no lettering, and no green (green means
"running" inside the app). The squircle is a superellipse (n=5) on the 1024
canvas with the macOS 100px margin and a baked-in drop shadow. Sizes <= 32px
use a simplified version: three clouds, wider steps, no slices.
"""
import json, math

CLOUD = "M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z"  # Lucide cloud, 24 grid
TILE_TOP, TILE_BOTTOM = "#2a3a78", "#0f1838"   # night-navy squircle
HALO = "#1c2a5c"                               # gap between overlapping clouds
VIOLET, ORANGE, TEAL, FRONT = "#8f6cf5", "#ff9a3d", "#22b8a6", "#ffffff"


def squircle(cx=512, cy=512, half=412, n=5, steps=200):
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, s = math.cos(t), math.sin(t)
        pts.append((cx + half * math.copysign(abs(c) ** (2 / n), c),
                    cy + half * math.copysign(abs(s) ** (2 / n), s)))
    return "M" + " L".join(f"{x:.1f},{y:.1f}" for x, y in pts) + "Z"


def place(cx, cy, s):
    """Transform that puts the 24-grid cloud's centre at (cx, cy), s canvas px per grid unit."""
    return f"translate({cx-12*s:.1f},{cy-10.5*s:.1f}) scale({s})"


def cloud(cx, cy, s, fill, halo=None, slices=()):
    o = ""
    if halo:   # tile-coloured outline so each overlapping cloud keeps its own silhouette
        o += f'<path transform="{place(cx, cy, s)}" d="{CLOUD}" fill="{halo}" stroke="{halo}" stroke-width="{14/s+1.5:.2f}" stroke-linejoin="round"/>'
    mask = ""
    if slices:
        mid = f"m{int(cx)}{int(cy)}"
        o += f'<mask id="{mid}" maskUnits="userSpaceOnUse" x="0" y="0" width="1024" height="1024"><rect width="1024" height="1024" fill="#fff"/>'
        for gy in slices:
            o += f'<rect x="0" y="{cy-10.5*s+gy*s-s*.42:.1f}" width="1024" height="{s*.84:.1f}" fill="#000"/>'
        o += '</mask>'
        mask = f' mask="url(#{mid})"'
    return o + f'<g{mask}><path transform="{place(cx, cy, s)}" d="{CLOUD}" fill="{fill}" stroke="{fill}" stroke-width="1.5" stroke-linejoin="round"/></g>'


SQ = squircle()


def svg(body):
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024"><defs><linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{TILE_TOP}"/><stop offset="1" stop-color="{TILE_BOTTOM}"/></linearGradient>
<linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".2"/><stop offset=".5" stop-color="#fff" stop-opacity="0"/></linearGradient>
<filter id="sh" x="-20%" y="-20%" width="140%" height="150%"><feDropShadow dx="0" dy="14" stdDeviation="16" flood-color="#000" flood-opacity=".28"/></filter>
<filter id="cs" x="-30%" y="-30%" width="160%" height="170%"><feDropShadow dx="0" dy="8" stdDeviation="9" flood-color="#000" flood-opacity=".30"/></filter></defs>
<path d="{SQ}" fill="url(#bg)" filter="url(#sh)"/><path d="{SQ}" fill="url(#sheen)"/><path d="{SQ}" fill="none" stroke="#fff" stroke-opacity=".16" stroke-width="3"/>
<g filter="url(#cs)">{body}</g></svg>'''


def full():
    o = cloud(435, 400, 14, VIOLET)
    o += cloud(485, 455, 14, ORANGE, HALO)
    o += cloud(535, 510, 14, TEAL, HALO)
    return o + cloud(585, 565, 14, FRONT, HALO, slices=(11.6, 14.4))


def small():   # three clouds, bigger steps, no slices: survives 16px
    o = cloud(440, 410, 16, ORANGE)
    o += cloud(520, 495, 16, TEAL, HALO)
    return o + cloud(600, 580, 16, FRONT, HALO)


SIZES = [16, 32, 64, 128, 256, 512, 1024]

if __name__ == "__main__":
    open("clouddeck-icon.svg", "w").write(svg(full()))
    open("clouddeck-icon-small.svg", "w").write(svg(small()))

    imgs = "".join(
        f'<div id="s{s}" style="width:{s}px;height:{s}px;margin-bottom:10px"><img src="{"clouddeck-icon-small" if s <= 32 else "clouddeck-icon"}.svg" width="{s}" height="{s}" style="display:block"></div>'
        for s in SIZES)
    open("export.html", "w").write(f'<!doctype html><meta charset="utf-8"><body style="margin:0;background:transparent">{imgs}')
    json.dump({"prelude": "", "shots": [
        {"name": f"icon_{s}px", "description": f"{s}px export", "setup": "await new Promise(r=>setTimeout(r,150));",
         "clip": f"#s{s}", "scale": 1} for s in SIZES]}, open("export.json", "w"), indent=1)

    # preview board: the sizes people actually see, on light and dark
    def row(bg, fg, label):
        cells = "".join(f'<div class="c"><img src="{"clouddeck-icon-small" if s <= 32 else "clouddeck-icon"}.svg" width="{s}" height="{s}"><span>{s}</span></div>' for s in (256, 128, 64, 32, 16))
        return f'<div class="p" style="background:{bg};color:{fg}"><div class="t">{label}</div><div class="cells">{cells}</div></div>'
    open("preview.html", "w").write(
        '<!doctype html><meta charset="utf-8"><style>body{margin:0;padding:20px;background:#c9c9cf;font:13px -apple-system,sans-serif}#board{width:640px;display:flex;flex-direction:column;gap:12px}'
        '.p{border-radius:12px;padding:12px 16px;display:flex;flex-direction:column;gap:6px}.t{font-weight:600}.cells{display:flex;align-items:flex-end;gap:20px}.c{display:flex;flex-direction:column;align-items:center;gap:4px}.c span{font-size:11px;opacity:.7}</style>'
        f'<div id="board">{row("#f4f4f6", "#1c1c1f", "Light")}{row("#1e1f23", "#e8e8ea", "Dark")}</div>')
