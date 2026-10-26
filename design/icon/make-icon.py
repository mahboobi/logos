#!/usr/bin/env python3
"""Render olb_ui/src/icons/icon.png from the source artwork.

Kept so the app icon can be regenerated rather than only edited. Run from the
repo root:  python3 design/icon/make-icon.py
"""
from PIL import Image, ImageDraw

SRC = "design/icon/microscope-source.jpg"
OUT = "olb_ui/src/icons/icon.png"
SIZE = 256
SS = 4                          # supersample, then downsample for clean edges
S = SIZE * SS
ACCENT = (11, 110, 79, 255)     # Theme.accent #0B6E4F — "verified / anchored"
GLYPH = (255, 255, 255, 255)    # Theme.textOnAccent
RADIUS = round(SIZE * 0.22) * SS
GLYPH_H = 0.60                  # glyph height as a fraction of the tile

# Alpha from darkness: the source is a black glyph on white, so 255 - luma
# keeps the antialiased edge instead of hard-thresholding it.
src = Image.open(SRC).convert("L")
alpha = src.point(lambda v: 255 - v)
alpha = alpha.crop(alpha.getbbox())

target_h = int(S * GLYPH_H)
target_w = max(1, round(alpha.width * target_h / alpha.height))
alpha = alpha.resize((target_w, target_h), Image.LANCZOS)

tile = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(tile).rounded_rectangle([0, 0, S - 1, S - 1], radius=RADIUS, fill=ACCENT)

glyph = Image.new("RGBA", (S, S), (0, 0, 0, 0))
glyph.paste(GLYPH, ((S - target_w) // 2, (S - target_h) // 2), alpha)
tile = Image.alpha_composite(tile, glyph)

tile.resize((SIZE, SIZE), Image.LANCZOS).save(OUT)
print("wrote", OUT)
