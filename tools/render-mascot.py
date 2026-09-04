#!/usr/bin/env python3
"""Render the mascot into the image assets an editor needs.

The pixels live in `crates/rqx-cli/assets/mascot.txt` and the colours in
`crates/rqx-cli/src/brand.rs`; both are read here rather than restated, so an owl
edited in the terminal art is the same owl on the marketplace listing.

    python3 tools/render-mascot.py
"""

import re
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "crates/rqx-cli/assets/mascot.txt"
BRAND = ROOT / "crates/rqx-cli/src/brand.rs"
VSCODE = ROOT / "editors/vscode"

TRANSPARENT = (0, 0, 0, 0)


def inks() -> dict[str, tuple[int, int, int]]:
    """The `rgb` table out of brand.rs, so a colour changed there lands here too."""
    source = BRAND.read_text()
    table = re.search(r"const fn rgb\(self\).*?\n    \}", source, re.S)
    if not table:
        sys.exit("brand.rs no longer has an `rgb` table in the shape this expects")
    found = {
        name: (int(r, 16), int(g, 16), int(b, 16))
        for name, r, g, b in re.findall(
            r"Self::(\w+) => \(0x([0-9A-Fa-f]{2}), 0x([0-9A-Fa-f]{2}), 0x([0-9A-Fa-f]{2})\)",
            table.group(0),
        )
    }
    missing = [name for name in ("Signal", "Text", "Ground") if name not in found]
    if missing:
        sys.exit(f"brand.rs has no colour for {', '.join(missing)}")
    return found


def darken(channel: int) -> int:
    """`brand.rs::darken` — the shade is the signal colour, three fifths of it."""
    return channel * 3 // 5


def frames() -> dict[str, list[str]]:
    """`#: name` opens a frame; every other non-comment line is a row of pixels."""
    out: dict[str, list[str]] = {}
    current = None
    for line in ART.read_text().splitlines():
        if line.startswith("#:"):
            current = line[2:].strip()
            out[current] = []
        elif line.startswith("#") or not line.strip():
            continue
        elif current:
            out[current].append(line)
    return out


def palette() -> dict[str, tuple[int, int, int, int]]:
    ink = inks()
    signal = ink["Signal"]
    return {
        ".": TRANSPARENT,
        "S": (*signal, 255),
        "H": (*tuple(darken(c) for c in signal), 255),
        "T": (*ink["Text"], 255),
        "K": (*ink["Ground"], 255),
    }


def render(rows: list[str], colours: dict[str, tuple[int, int, int, int]]) -> Image.Image:
    width = max(len(row) for row in rows)
    image = Image.new("RGBA", (width, len(rows)), TRANSPARENT)
    for y, row in enumerate(rows):
        for x, character in enumerate(row):
            image.putpixel((x, y), colours.get(character, TRANSPARENT))
    return image


def scaled(image: Image.Image, size: int, pad: int = 0) -> Image.Image:
    """Nearest neighbour, and only whole multiples: a pixel that lands on a fraction of
    a pixel is what makes upscaled pixel art look blurred rather than sharp."""
    inner = size - pad * 2
    factor = max(1, inner // max(image.size))
    grown = image.resize((image.width * factor, image.height * factor), Image.NEAREST)
    canvas = Image.new("RGBA", (size, size), TRANSPARENT)
    canvas.paste(grown, ((size - grown.width) // 2, (size - grown.height) // 2))
    return canvas


def on_ground(image: Image.Image, ground: tuple[int, int, int]) -> Image.Image:
    """A marketplace tile is shown against whatever the page is, so the icon carries its
    own background rather than punching a transparent hole in someone else's."""
    canvas = Image.new("RGBA", image.size, (*ground, 255))
    canvas.alpha_composite(image)
    return canvas


def silhouette(rows: list[str]) -> str:
    """An activity-bar icon is masked to one colour, so only the shape survives. The
    irises are left as holes and the pupils kept solid, which is what still reads as an
    owl once the colour is gone.

    A flat fill rather than `currentColor`: the mask is taken from the alpha channel, and
    `currentColor` outside a document inherits nothing and renders the icon empty.
    """
    width = max(len(row) for row in rows)
    runs = []
    for y, row in enumerate(rows):
        start = None
        for x in range(width + 1):
            solid = x < len(row) and row[x] in "SHK"
            if solid and start is None:
                start = x
            elif not solid and start is not None:
                runs.append((start, y, x - start))
                start = None
    squares = "\n".join(
        f'  <rect x="{x}" y="{y}" width="{w}" height="1"/>' for x, y, w in runs
    )
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {len(rows)}" '
        f'width="{width * 2}" height="{len(rows) * 2}" fill="#C5C5C5" '
        f'shape-rendering="crispEdges">\n{squares}\n</svg>\n'
    )


def woff(rows: list[str], path: Path) -> None:
    """A status-bar entry renders codicons, not images, so the only way the owl reaches
    it is as a glyph in a contributed icon font.

    Pixels become one rectangle each in a single glyph, drawn on the font's em square
    with the y axis flipped — font coordinates count up from the baseline while the art
    counts down from the top row.
    """
    from fontTools.fontBuilder import FontBuilder
    from fontTools.pens.ttGlyphPen import TTGlyphPen

    units, height = 1000, len(rows)
    width = max(len(row) for row in rows)
    cell = units // max(width, height)
    # Sat on the baseline the owl reads low against the text beside it; a codicon sits
    # slightly above it, which is what this offset buys.
    lift = int(units * 0.12)

    pen = TTGlyphPen(None)
    for y, row in enumerate(rows):
        start = None
        for x in range(width + 1):
            solid = x < len(row) and row[x] in "SHK"
            if solid and start is None:
                start = x
            elif not solid and start is not None:
                left, right = start * cell, x * cell
                top = (height - y) * cell + lift
                bottom = top - cell
                pen.moveTo((left, bottom))
                pen.lineTo((left, top))
                pen.lineTo((right, top))
                pen.lineTo((right, bottom))
                pen.closePath()
                start = None

    builder = FontBuilder(units, isTTF=True)
    order = [".notdef", "owl"]
    builder.setupGlyphOrder(order)
    builder.setupCharacterMap({0xE900: "owl"})
    empty = TTGlyphPen(None).glyph()
    builder.setupGlyf({".notdef": empty, "owl": pen.glyph()})
    advance = width * cell
    builder.setupHorizontalMetrics({name: (advance, 0) for name in order})
    builder.setupHorizontalHeader(ascent=units, descent=0)
    builder.setupNameTable({"familyName": "rqx-icons", "styleName": "Regular"})
    builder.setupOS2(sTypoAscender=units, usWinAscent=units, usWinDescent=0)
    builder.setupPost()
    builder.font.flavor = "woff"
    builder.save(str(path))


def main() -> None:
    art = frames()
    if "idle" not in art:
        sys.exit("mascot.txt has no `idle` frame")
    colours = palette()
    ground = inks()["Ground"]
    idle = render(art["idle"], colours)

    written = []
    # The marketplace requires 128x128; the padding keeps the owl off the tile's edge.
    icon = on_ground(scaled(idle, 128, pad=12), ground)
    icon.save(VSCODE / "icon.png")
    written.append(VSCODE / "icon.png")

    media = VSCODE / "media"
    media.mkdir(exist_ok=True)
    for size in (16, 32, 48, 64, 128, 256):
        path = media / f"mascot-{size}.png"
        scaled(idle, size).save(path)
        written.append(path)

    for name, rows in art.items():
        path = media / f"mascot-{name}@8x.png"
        scaled(render(rows, colours), max(len(rows), max(len(r) for r in rows)) * 8).save(path)
        written.append(path)

    (media / "rqx.svg").write_text(silhouette(art["idle"]))
    written.append(media / "rqx.svg")

    woff(art["idle"], media / "rqx-icons.woff")
    written.append(media / "rqx-icons.woff")

    for path in written:
        print(f"  {path.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
