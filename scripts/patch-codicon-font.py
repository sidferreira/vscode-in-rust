#!/usr/bin/env python3
"""Make the Codicons font loadable by gpui on macOS.

gpui's CoreText loader (gpui-pre-apple text_system.rs `load_family`) skips any font
that has no glyph for 'm' (it measures text with 'm'). Codicons is a pure icon font
with no 'm', so gpui silently fell back to the system font and every icon rendered
as a missing-glyph box.

Fix: map U+006D ('m') to the existing `blank` codicon glyph (U+EC03) in every
Unicode cmap subtable. No icon glyph changes. The result is a modified version of
@vscode/codicons (CC BY 4.0): see crates/vsr-app/assets/codicon-NOTICE.md.

Needs fontTools (not stdlib), e.g.:
    python3 -m venv "$(mktemp -d)/venv" && <venv>/bin/pip install fonttools
    <venv>/bin/python -I scripts/patch-codicon-font.py <in.ttf> <out.ttf>
"""
import sys

from fontTools.ttLib import TTFont

M = 0x6D
BLANK = 0xEC03


def main(src: str, dst: str) -> None:
    font = TTFont(src)
    patched = 0
    for table in font["cmap"].tables:
        if not table.isUnicode():
            continue
        if BLANK not in table.cmap:
            continue
        table.cmap[M] = table.cmap[BLANK]
        patched += 1
    if patched == 0:
        sys.exit("no Unicode cmap subtable contains the blank glyph; nothing patched")
    font.save(dst)
    print(f"patched {patched} cmap subtable(s): U+006D -> glyph '{font.getBestCmap()[M]}' -> {dst}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit("usage: patch-codicon-font.py <in.ttf> <out.ttf>")
    main(sys.argv[1], sys.argv[2])
