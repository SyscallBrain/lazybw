#!/usr/bin/env python3
"""Generates the lazybw logo files in book/src/images.

The mark is a padlock whose body is a terminal window with a prompt and a
cursor. The wordmark is drawn from a 5x7 pixel font as rectangles, so the
SVGs need no font. Colours come from the tokyonight-night theme.

    python3 scripts/make-logo.py
"""
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "book" / "src" / "images"

BG = "#1a1b26"      # bg
PANEL = "#24283b"   # bg.float
BORDER = "#414868"  # border
ACCENT = "#7aa2f7"  # accent (blue)
GREEN = "#9ece6a"   # ok
CYAN = "#7dcfff"    # info
FG = "#c0caf5"      # fg

# 5x7 glyphs, '#' = pixel.
GLYPHS = {
    "l": ["##...", ".#...", ".#...", ".#...", ".#...", ".#...", "###.."],
    "a": [".....", ".....", ".###.", "....#", ".####", "#...#", ".####"],
    "z": [".....", ".....", "#####", "...#.", "..#..", ".#...", "#####"],
    "y": [".....", ".....", "#...#", "#...#", ".####", "....#", ".###."],
    "b": ["#....", "#....", "####.", "#...#", "#...#", "#...#", "####."],
    "w": [".....", ".....", "#...#", "#...#", "#.#.#", "#.#.#", ".#.#."],
}


def mark(x: float, y: float, s: float) -> str:
    """The padlock-terminal mark in a s×s box at (x, y)."""
    u = s / 64  # design grid: 64 units
    def r(v):
        return round(v * u, 3)
    return f"""
  <g transform="translate({x},{y})">
    <!-- shackle -->
    <path d="M{r(20)},{r(30)} V{r(20)} a{r(12)},{r(12)} 0 0 1 {r(24)},0 V{r(30)}"
          fill="none" stroke="{ACCENT}" stroke-width="{r(6)}" stroke-linecap="round"/>
    <!-- body: a terminal window -->
    <rect x="{r(8)}" y="{r(28)}" width="{r(48)}" height="{r(32)}" rx="{r(6)}"
          fill="{PANEL}" stroke="{ACCENT}" stroke-width="{r(3)}"/>
    <!-- title bar dots -->
    <circle cx="{r(15)}" cy="{r(34)}" r="{r(1.6)}" fill="{BORDER}"/>
    <circle cx="{r(20.5)}" cy="{r(34)}" r="{r(1.6)}" fill="{BORDER}"/>
    <circle cx="{r(26)}" cy="{r(34)}" r="{r(1.6)}" fill="{BORDER}"/>
    <!-- prompt chevron -->
    <path d="M{r(16)},{r(41)} L{r(23)},{r(46)} L{r(16)},{r(51)}"
          fill="none" stroke="{GREEN}" stroke-width="{r(3.4)}"
          stroke-linecap="round" stroke-linejoin="round"/>
    <!-- block cursor -->
    <rect x="{r(28)}" y="{r(48)}" width="{r(12)}" height="{r(4)}" rx="{r(1)}" fill="{CYAN}"/>
  </g>"""


def wordmark(x: float, y: float, px: float, colour: str) -> tuple[str, float]:
    """The word 'lazybw' as pixel rectangles; returns (svg, width)."""
    parts = []
    cursor = x
    for ch in "lazybw":
        rows = GLYPHS[ch]
        width = max(len(row.rstrip(".")) for row in rows)
        for j, row in enumerate(rows):
            for i, cell in enumerate(row):
                if cell == "#":
                    parts.append(
                        f'<rect x="{cursor + i * px:.2f}" y="{y + j * px:.2f}" '
                        f'width="{px:.2f}" height="{px:.2f}"/>'
                    )
        cursor += (width + 1) * px
    svg = f'  <g fill="{colour}">\n    ' + "\n    ".join(parts) + "\n  </g>"
    return svg, cursor - x - px


def write(name: str, width: float, height: float, body: str, background: bool) -> None:
    bg = (
        f'  <rect width="{width}" height="{height}" rx="{round(height * 0.22, 2)}" fill="{BG}"/>\n'
        if background
        else ""
    )
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0f}" height="{height:.0f}" '
        f'viewBox="0 0 {width:.2f} {height:.2f}" role="img" aria-label="lazybw">\n'
        f"  <title>lazybw</title>\n{bg}{body}\n</svg>\n"
    )
    (OUT / name).write_text(svg)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    # App icon: the mark on a rounded dark tile.
    write("logo.svg", 256, 256, mark(32, 26, 192), background=True)
    # Mark only, transparent, for favicons and small sizes.
    write("logo-mark.svg", 64, 64, mark(0, 0, 64), background=False)
    # Wordmark for dark backgrounds (README banner, book header).
    px = 9
    text, text_width = wordmark(0, 0, px, FG)
    height = 120
    mark_size = 96
    gap = 28
    body = mark(24, 12, mark_size) + "\n" + text.replace(
        "<g ", f'<g transform="translate({24 + mark_size + gap},{(height - 7 * px) / 2 + 6})" ', 1
    )
    write("logo-wordmark.svg", 24 + mark_size + gap + text_width + 32, height, body, background=True)
    # Same wordmark, transparent with dark text, for light pages.
    text_light, _ = wordmark(0, 0, px, BG)
    body_light = mark(24, 12, mark_size) + "\n" + text_light.replace(
        "<g ", f'<g transform="translate({24 + mark_size + gap},{(height - 7 * px) / 2 + 6})" ', 1
    )
    write(
        "logo-wordmark-light.svg",
        24 + mark_size + gap + text_width + 32,
        height,
        body_light,
        background=False,
    )


if __name__ == "__main__":
    main()
