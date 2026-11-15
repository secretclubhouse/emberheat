#!/usr/bin/env python3
"""Copy a KiCad SVG export into a form ImageMagick's built-in SVG renderer
draws: drop the invisible <text> copies and <desc> tags KiCad adds for
searching, and the "stroked-text" group class (magick stops with "vector
graphics nested too deeply" on it). The visible text is stroked paths, so
nothing drawn is lost. Usage: svg_for_magick.py IN.svg OUT.svg"""
import re
import sys


def main(src, dst):
    with open(src) as f:
        t = f.read()
    t = re.sub(r"<text[^>]*>.*?</text>", "", t, flags=re.S)
    t = re.sub(r"<desc>.*?</desc>", "", t, flags=re.S)
    t = t.replace('<g class="stroked-text">', "<g>")
    with open(dst, "w") as f:
        f.write(t)
    return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:3]))
