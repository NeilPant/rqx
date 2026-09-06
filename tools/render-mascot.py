#!/usr/bin/env python3
"""Render the mascot into the image assets an editor needs.

The pixels live in `crates/rqx-cli/assets/mascot.txt` and the colours in
`crates/rqx-cli/src/brand.rs`; both are read here rather than restated, so an owl
edited in the terminal art is the same owl everywhere else.

    python3 tools/render-mascot.py
"""

import re
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "crates/rqx-cli/assets/mascot.txt"
BRAND = ROOT / "crates/rqx-cli/src/brand.rs"
ASSETS = ROOT / "assets"

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





def social_preview(colours: dict[str, tuple[int, int, int]]) -> None:
    """The 1280x640 card GitHub shows when the repository is linked anywhere else.

    Written here rather than drawn by hand because it reads the same brand table and
    the same owl as every other asset, so none of them can drift from the others.
    GitHub has no API for uploading it: Settings -> General -> Social preview.
    """
    from PIL import ImageDraw, ImageFont

    width, height = 1280, 640
    ground = colours["Ground"]
    text = colours["Text"]
    signal = colours["Signal"]
    card = Image.new("RGB", (width, height), ground)
    draw = ImageDraw.Draw(card)

    owl = Image.open(ASSETS / "mascot-256.png").convert("RGBA")
    owl = owl.resize((320, 320), Image.LANCZOS)
    card.paste(owl, (104, 160), owl)

    def font(size: int, bold: bool = False):
        for name in (
            "/System/Library/Fonts/SFNSDisplay.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans%s.ttf" % ("-Bold" if bold else ""),
        ):
            try:
                return ImageFont.truetype(name, size)
            except OSError:
                continue
        return ImageFont.load_default(size)

    # The rule sits clear of the descender on `q`, which a bar under the baseline
    # would otherwise read as a strikethrough.
    left = 472
    draw.text((left, 138), "rqx", font=font(132, bold=True), fill=text)
    draw.rectangle([(left, 318), (left + 104, 325)], fill=signal)
    draw.text(
        (left, 358),
        "Static analysis that names the refactoring",
        font=font(40),
        fill=text,
    )
    draw.text((left, 414), "TypeScript · Python · Go · Java · C++", font=font(35), fill=signal)
    draw.text(
        (left, 478),
        "185 rules · applies the mechanical fixes · grades a diff",
        font=font(28),
        fill=tuple(channel * 3 // 5 + 60 for channel in text),
    )

    out = ASSETS / "social-preview.png"
    card.save(out)
    print(f"wrote {out.relative_to(ROOT)} ({width}x{height})")


def main() -> None:
    art = frames()
    if "idle" not in art:
        sys.exit("mascot.txt has no `idle` frame")
    colours = palette()
    idle = render(art["idle"], colours)

    written = []
    ASSETS.mkdir(exist_ok=True)
    for size in (128, 256):
        path = ASSETS / f"mascot-{size}.png"
        scaled(idle, size).save(path)
        written.append(path)

    social_preview(inks())

    for path in written:
        print(f"  {path.relative_to(ROOT)}")


if __name__ == "__main__":
    main()

