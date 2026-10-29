"""Draw the DAW 388 app icon (1024 x 1024 PNG): the console's dark panel in a bordeaux rim, two
reels behind smoked glass, and "388" in the red of the name plate.

Run: python3 packaging/make_icon.py  (needs Pillow; writes packaging/icon-1024.png)
"""
import math
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).parent
FONT = HERE.parent / "crates/ui/assets/fonts/IBMPlexSansCondensed-SemiBold.ttf"
S = 4  # supersampling
N = 1024 * S


def rgb(h):
    return tuple(int(h[i:i + 2], 16) for i in (1, 3, 5))


def rounded(draw, box, r, fill, outline=None, width=0):
    draw.rounded_rectangle(box, radius=r, fill=fill, outline=outline, width=width)


def main():
    img = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    # The macOS grid: an 824-point square in a 1024 canvas, with a soft shadow under it.
    m, size = 100 * S, 824 * S
    box = (m, m, m + size, m + size)
    radius = 185 * S
    shadow = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle((m, m + 14 * S, m + size, m + size + 14 * S), radius=radius, fill=(0, 0, 0, 120))
    img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(18 * S)))
    d = ImageDraw.Draw(img)
    # Bordeaux wood rim and the dark panel inside it.
    rounded(d, box, radius, rgb("#4a1d18"))
    inset = 34 * S
    panel = (box[0] + inset, box[1] + inset, box[2] - inset, box[3] - inset)
    rounded(d, panel, radius - inset, rgb("#2c2c2e"))
    # A faint highlight along the top edge of the panel, composited so it stays faint.
    light = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    ImageDraw.Draw(light).rounded_rectangle(panel, radius=radius - inset, outline=(255, 255, 255, 26), width=3 * S)
    img.alpha_composite(light)

    # The smoked-glass window with the two reels.
    glass = (panel[0] + 70 * S, panel[1] + 70 * S, panel[2] - 70 * S, panel[1] + 70 * S + 330 * S)
    rounded(d, glass, 26 * S, rgb("#161211"), outline=rgb("#0e0c0b"), width=4 * S)
    cy = (glass[1] + glass[3]) // 2
    for cx, pack in ((glass[0] + (glass[2] - glass[0]) * 0.27, 120), (glass[0] + (glass[2] - glass[0]) * 0.73, 78)):
        cx = int(cx)
        flange = 135 * S
        d.ellipse((cx - flange, cy - flange, cx + flange, cy + flange), fill=rgb("#3a2a25"), outline=rgb("#6a4c40"), width=4 * S)
        p = pack * S
        d.ellipse((cx - p, cy - p, cx + p, cy + p), fill=rgb("#120e0c"), outline=rgb("#6a4c40"), width=3 * S)
        for k in range(3):
            a = math.radians(90 + k * 120)
            tip = (cx + math.cos(a) * flange * 0.95, cy - math.sin(a) * flange * 0.95)
            d.line((cx, cy, *tip), fill=rgb("#5e4238"), width=26 * S)
        hub = 34 * S
        d.ellipse((cx - hub, cy - hub, cx + hub, cy + hub), fill=rgb("#4a332b"))
        d.ellipse((cx - 11 * S, cy - 11 * S, cx + 11 * S, cy + 11 * S), fill=rgb("#161211"))
    # The tape between the packs.
    d.line((glass[0] + (glass[2] - glass[0]) * 0.27, cy + 120 * S, glass[0] + (glass[2] - glass[0]) * 0.73, cy + 78 * S), fill=rgb("#6a4c40"), width=4 * S)

    # "388" in the red of the name plate, slanted like its italic.
    text = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    font = ImageFont.truetype(str(FONT), 300 * S)
    td = ImageDraw.Draw(text)
    bbox = td.textbbox((0, 0), "388", font=font)
    w, h = bbox[2] - bbox[0], bbox[3] - bbox[1]
    x = (N - w) // 2 - bbox[0]
    y = glass[3] + 38 * S - bbox[1]
    td.text((x, y), "388", font=font, fill=rgb("#d43a26"))
    slant = 0.18
    text = text.transform(text.size, Image.AFFINE, (1, slant, -slant * (y + h), 0, 1, 0), resample=Image.BICUBIC)
    img.alpha_composite(text)

    out = img.resize((1024, 1024), Image.LANCZOS)
    out.save(HERE / "icon-1024.png")
    print("wrote", HERE / "icon-1024.png")


if __name__ == "__main__":
    main()
