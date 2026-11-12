#!/usr/bin/env python3
"""Write the 1024×1024 PNG that `npx tauri icon` turns into the app's icons.

Handed-over reason: a Windows installer without an icon is an installer people distrust, and this box has no
image library — so the source art is drawn here, in pixels, from a description. It is not a placeholder to be
replaced later; it is the mark, and re-running this script reproduces it byte for byte:

    python3 scripts/make-icon.py && npx tauri icon branding/jott-icon.png -o apps/shell/icons

The mark: a page, folded at the top right, on a slate tile with rounded corners. Three lines of amber text
say "notes" without spelling it; the third is shorter, because a note that fills its last line is not a note
anybody stopped writing in.
"""

import struct
import zlib
from pathlib import Path

SIZE = 1024

# The palette, as decided in the design conversation: slate for the tile, paper for the page, amber for what
# is written on it.
SLATE = (20, 32, 43, 255)
SLATE_EDGE = (32, 48, 62, 255)
PAPER = (246, 243, 236, 255)
FOLD = (214, 208, 196, 255)
AMBER = (232, 176, 75, 255)
CLEAR = (0, 0, 0, 0)

# The page, and its folded corner.
PAGE_LEFT, PAGE_TOP, PAGE_RIGHT, PAGE_BOTTOM = 292, 208, 732, 816
FOLD_SIZE = 150

# Three lines of text inside the page: left edge, width, height, top.
LINES = [
    (356, 316, 46, 344),
    (356, 316, 46, 460),
    (356, 208, 46, 576),
]

CORNER_RADIUS = 190


def rounded_tile(x: int, y: int) -> bool:
    """Whether a point is inside the tile's rounded corners."""
    if x < 0 or y < 0 or x >= SIZE or y >= SIZE:
        return False
    r = CORNER_RADIUS
    if x < r and y < r:
        return (x - r) ** 2 + (y - r) ** 2 <= r * r
    if x >= SIZE - r and y < r:
        return (x - (SIZE - r)) ** 2 + (y - r) ** 2 <= r * r
    if x < r and y >= SIZE - r:
        return (x - r) ** 2 + (y - (SIZE - r)) ** 2 <= r * r
    if x >= SIZE - r and y >= SIZE - r:
        return (x - (SIZE - r)) ** 2 + (y - (SIZE - r)) ** 2 <= r * r
    return True


def on_page(x: int, y: int) -> bool:
    """Inside the page, minus the corner that has been folded back."""
    if not (PAGE_LEFT <= x < PAGE_RIGHT and PAGE_TOP <= y < PAGE_BOTTOM):
        return False
    fold_x = x - (PAGE_RIGHT - FOLD_SIZE)
    fold_y = (PAGE_TOP + FOLD_SIZE) - y
    return not (fold_x > 0 and fold_y > 0 and fold_x + fold_y > FOLD_SIZE)


def in_fold(x: int, y: int) -> bool:
    """The folded flap itself: the triangle that reads as a turned-down corner."""
    if not (PAGE_RIGHT - FOLD_SIZE <= x < PAGE_RIGHT and PAGE_TOP <= y < PAGE_TOP + FOLD_SIZE):
        return False
    fold_x = x - (PAGE_RIGHT - FOLD_SIZE)
    fold_y = (PAGE_TOP + FOLD_SIZE) - y
    return 0 < fold_x + FOLD_SIZE - fold_y < 58 and fold_x + fold_y > FOLD_SIZE


def on_a_line(x: int, y: int) -> bool:
    for left, width, height, top in LINES:
        if left <= x < left + width and top <= y < top + height:
            return True
    return False


def pixel(x: int, y: int):
    if not rounded_tile(x, y):
        return CLEAR
    # A one-pixel lighter rim, so the tile does not read as a hole cut in the desktop.
    edge = 6
    inside = rounded_tile(x - edge, y - edge) and rounded_tile(x + edge, y + edge)
    if on_a_line(x, y):
        return AMBER
    if in_fold(x, y):
        return FOLD
    if on_page(x, y):
        return PAPER
    return SLATE if inside else SLATE_EDGE


def write_png(path: Path) -> None:
    raw = bytearray()
    for y in range(SIZE):
        raw.append(0)  # filter type 0: every row is stored as it comes
        for x in range(SIZE):
            raw.extend(pixel(x, y))

    def chunk(kind: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)  # 8-bit RGBA
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


if __name__ == "__main__":
    out = Path(__file__).resolve().parent.parent / "branding" / "jott-icon.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    write_png(out)
    print(f"{out} ({out.stat().st_size:,} bytes)")
