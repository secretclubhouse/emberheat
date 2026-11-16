#!/usr/bin/env python3
"""Write the four printed parts to out/ as STEP and STL, each in its print
orientation, and out/hashes.txt (which sources they came from).

Handedness: gen_enclosure builds in the board frame (x right, y down, z toward
the viewer), which is left-handed, so every solid there is the mirror image of
the real part. Every part is mirrored in y here (y -> -y) before anything else.
Print orientations then use rotations and translations only, never a second
mirror. In the exported files:

  base        floor on the bed, as assembled
  cover       face on the bed: turned 180 degrees about x
  dial-strip  back on the bed, lettering up, one body (plate and lettering)
  pane        flat

Run with the CadQuery environment: make enclosure."""
import hashlib
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, "out")

# STL tessellation: chordal deviation (mm) and angle (rad). 0.02 mm keeps the
# lettering's curves smooth to well under the printer's resolution.
STL_TOL = 0.02
STL_ANGLE = 0.2


def real_part(shape):
    """The real part from a board-frame shape: mirrored in y."""
    return shape.mirror("XZ")


def sources():
    """The files the exports depend on: (key, path)."""
    return [("board-geometry.json", os.path.join(HERE, "board-geometry.json")),
            ("params.py", os.path.join(HERE, "params.py")),
            ("gen_enclosure.py", os.path.join(HERE, "gen_enclosure.py")),
            ("font", os.path.join(HERE, "fonts", "Inter-SemiBold.ttf"))]


def sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def print_parts(geom, p):
    """({name: cq.Shape in print orientation}, lettering start height)."""
    import cadquery as cq
    import gen_enclosure
    built = gen_enclosure.build(geom, p)
    strip = built["dial_strip"].union(built["dial_lettering"]).val()
    shapes = {"base": built["base"].val(), "cover": built["cover"].val(), "dial-strip": strip,
              "pane": built["pane"].val()}
    out = {}
    for name, s in shapes.items():
        s = real_part(s)
        if name == "cover":
            s = s.rotate(cq.Vector(0, 0, 0), cq.Vector(1, 0, 0), 180)
        bb = s.BoundingBox()
        out[name] = s.translate(cq.Vector(0, 0, -bb.zmin))
    # The strip's back is on the bed and its front dial_t above it, so the
    # lettering starts dial_t above the bed.
    return out, p.dial_t


def write_step(shape, path):
    import cadquery as cq
    cq.exporters.export(cq.Workplane("XY").add(shape), path, "STEP")
    # The header carries the time of the export; fix it so a rebuild is byte-identical.
    with open(path, "rb") as f:
        text = f.read().decode("latin-1")
    text = re.sub(r"(FILE_NAME\('[^']*',\s*')[^']*(')", r"\g<1>2000-01-01T00:00:00\g<2>", text)
    with open(path, "wb") as f:
        f.write(text.encode("latin-1"))


def write_stl(shape, path):
    import cadquery as cq
    cq.exporters.export(cq.Workplane("XY").add(shape), path, "STL", tolerance=STL_TOL,
                        angularTolerance=STL_ANGLE)


def main():
    import cadquery as cq
    import board_model
    from params import P
    geom = board_model.load_geometry()
    parts, start = print_parts(geom, P)
    os.makedirs(OUT, exist_ok=True)
    for name, s in parts.items():
        bb = s.BoundingBox()
        write_step(s, os.path.join(OUT, name + ".step"))
        write_stl(s, os.path.join(OUT, name + ".stl"))
        print("%-10s %6.1f x %6.1f x %5.1f mm  %8.0f mm3" % (name, bb.xlen, bb.ylen, bb.zlen, s.Volume()))
    print("Dial strip: change filament at %.2f mm from the bed (lettering %.2f to %.2f)." % (start, start, start + P.letter_proud))
    with open(os.path.join(OUT, "hashes.txt"), "w") as f:
        f.write("# SHA-256 of the files the exports were made from; check_enclosure.py compares them.\n")
        for key, path in sources():
            f.write("%s: %s\n" % (key, sha256(path)))
        f.write("cadquery: %s\n" % cq.__version__)
        f.write("lettering_start_height_mm: %.3f\n" % start)


if __name__ == "__main__":
    main()
