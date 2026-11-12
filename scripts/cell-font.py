#!/usr/bin/env python3
"""Convert Terminus Font BDF strikes into the cell bitmaps embedded by src/ui/cell.rs.

Usage: cell-font.py <dir with ter-u{16,20,24,28,32}{n,b}.bdf> <assets/cell>

File layout, little-endian: b"CELL1", u8 width, u8 height, u16 glyph count,
count x u32 code points (ascending), then count x height rows of ceil(width/8)
bytes, most significant bit leftmost.
"""
import struct
import sys
from pathlib import Path


def read_bdf(path):
    glyphs, code, rows, bbx, in_bitmap = {}, None, [], None, False
    box = None
    for line in path.read_text(encoding="latin-1").splitlines():
        part = line.split()
        if not part:
            continue
        if part[0] == "FONTBOUNDINGBOX":
            box = tuple(int(v) for v in part[1:5])
        elif part[0] == "ENCODING":
            code = int(part[1])
        elif part[0] == "BBX":
            bbx = tuple(int(v) for v in part[1:5])
        elif part[0] == "BITMAP":
            in_bitmap, rows = True, []
        elif part[0] == "ENDCHAR":
            in_bitmap = False
            width, height, _, descent = box
            assert bbx == box, f"{path.name}: U+{code:04X} is not a full cell"
            stride = (width + 7) // 8
            data = b"".join(int(r, 16).to_bytes(len(r) // 2, "big")[:stride] for r in rows)
            assert len(data) == stride * height
            if code is not None and code >= 0x20:
                glyphs[code] = data
        elif in_bitmap:
            rows.append(part[0])
    return box[0], box[1], glyphs


def main(source, target):
    source, target = Path(source), Path(target)
    for size in (16, 20, 24, 28, 32):
        for weight in "nb":
            width, height, glyphs = read_bdf(source / f"ter-u{size}{weight}.bdf")
            assert (width, height) == (size // 2, size)
            codes = sorted(glyphs)
            blob = b"CELL1" + struct.pack("<BBH", width, height, len(codes))
            blob += b"".join(struct.pack("<I", c) for c in codes)
            blob += b"".join(glyphs[c] for c in codes)
            (target / f"cell-{size}{weight}.bin").write_bytes(blob)
            print(f"cell-{size}{weight}.bin  {width}x{height}  {len(codes)} glyphs  {len(blob)} bytes")


if __name__ == "__main__":
    main(*sys.argv[1:3])
