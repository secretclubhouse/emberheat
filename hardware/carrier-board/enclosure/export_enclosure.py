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
  test-piece  two bits cut from the base for a first test print (see TEST_REGIONS)
  test-piece-cover  the same two corners of the cover (they mate with the base
              test piece) and one end of the window with its rim and recess,
              face down

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


# Regions of the base (board frame, x0, y0, x1, y1; full height) for the test
# print: the bottom-left corner (standoff H3's post and insert hole, the lip's
# corner, the wall's rim) and the bottom magnet pad nearest the notch with its
# pocket and a stretch of lip. Cut from the real base so they cannot drift.
TEST_REGIONS = [(-11.4, 94.0, 16.0, 118.4), (58.0, 100.0, 80.0, 118.4)]


# One end of the cover's window with its rim, the strip's recess and a pane rail
# (board frame, x0, y0, x1, y1), for the cover's test piece.
WINDOW_REGION = (14.0, 22.0, 50.0, 72.0)


def real_part(shape):
    """The real part from a board-frame shape: mirrored in y."""
    return shape.mirror("XZ")


def sources():
    """The files the exports depend on: (key, path)."""
    return [("board-geometry.json", os.path.join(HERE, "board-geometry.json")),
            ("params.py", os.path.join(HERE, "params.py")),
            ("gen_enclosure.py", os.path.join(HERE, "gen_enclosure.py")),
            ("font", os.path.join(HERE, "fonts", "Inter-SemiBold.ttf")),
            ("export_enclosure.py", os.path.join(HERE, "export_enclosure.py")),
            ("render_enclosure.py", os.path.join(HERE, "render_enclosure.py")),
            ("board_model.py", os.path.join(HERE, "board_model.py"))]


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
    base = shapes["base"]
    bb = base.BoundingBox()
    pieces = [base.intersect(cq.Solid.makeBox(x1 - x0, y1 - y0, bb.zlen + 2, cq.Vector(x0, y0, bb.zmin - 1)))
              for x0, y0, x1, y1 in TEST_REGIONS]
    shapes["test-piece"] = cq.Compound.makeCompound([sol for pc in pieces for sol in pc.Solids()])
    cover = shapes["cover"]
    cb = cover.BoundingBox()
    regions = TEST_REGIONS + [WINDOW_REGION]
    cpieces = [cover.intersect(cq.Solid.makeBox(x1 - x0, y1 - y0, cb.zlen + 2, cq.Vector(x0, y0, cb.zmin - 1)))
               for x0, y0, x1, y1 in regions]
    shapes["test-piece-cover"] = cq.Compound.makeCompound([sol for pc in cpieces for sol in pc.Solids()])
    out = {}
    for name, s in shapes.items():
        s = real_part(s)
        if name in ("cover", "test-piece-cover"):
            s = s.rotate(cq.Vector(0, 0, 0), cq.Vector(1, 0, 0), 180)
        bb = s.BoundingBox()
        out[name] = s.translate(cq.Vector(0, 0, -bb.zmin))
    # The strip's back is on the bed and its front strip_t above it, so the
    # lettering starts strip_t above the bed.
    return out, p.strip_t


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
