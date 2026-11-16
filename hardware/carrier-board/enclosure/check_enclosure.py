#!/usr/bin/env python3
"""Checks on the enclosure: the board geometry file, the board model and the
printed parts, measured on the solids themselves.
Run with the CadQuery environment: make enclosure-check."""
import hashlib
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

BOARD = os.path.join(ROOT, "carrier-board.kicad_pcb")
TOL = 0.01


def near(a, b, tol=TOL):
    return abs(a - b) <= tol


def geometry():
    import board_model
    return board_model.load_geometry()


def test_geometry_sanity():
    g = geometry()
    xs = [p[0] for p in g["outline"]]
    ys = [p[1] for p in g["outline"]]
    w, h = max(xs) - min(xs), max(ys) - min(ys)
    assert near(w, 168) and near(h, 114), "outline is %.2f x %.2f, want 168 x 114" % (w, h)
    holes = {c["ref"]: (c["x"], c["y"]) for c in g["corner_holes"]}
    want = {"H1": (4, 4), "H2": (164, 4), "H3": (4, 110), "H4": (164, 110)}
    assert sorted(holes) == sorted(want), "corner holes %s" % sorted(holes)
    for ref, (x, y) in want.items():
        assert near(holes[ref][0], x) and near(holes[ref][1], y), "%s at %s, want %s" % (ref, holes[ref], (x, y))
    for key in ("zone_terminals", "zone_lights"):
        items = g[key]
        assert len(items) == 7, "%d %s, want 7" % (len(items), key)
        for a, b in zip(items, items[1:]):
            assert near(b["x"] - a["x"], 18.0), "%s %s to %s is %.3f apart, want 18" % (key, a["ref"], b["ref"], b["x"] - a["x"])
    for light in g["zone_lights"]:
        assert abs(light["y"] - 40) <= 1.5, "%s lens at y %.2f, not within 1.5 of 40" % (light["ref"], light["y"])
    with open(BOARD, "rb") as f:
        digest = hashlib.sha256(f.read()).hexdigest()
    assert g["pcb_sha256"] == digest, "board-geometry.json is from a different board (run make enclosure-geometry)"


def test_board_model():
    import board_model
    from params import P
    g = geometry()
    solids = board_model.board_solids(g, P)
    for key in ["board", "antenna_A1", "antenna_A2", "usb_plug", "A1", "A2", "K1", "RV1", "F1", "C1", "J2"] + \
            ["plug_J1%d" % n for n in range(1, 8)]:
        assert key in solids, "no solid %s" % key
    for ref, s in solids.items():
        if ref in ("board", "usb_plug") or ref.startswith(("antenna_", "plug_")):
            continue
        top = s.val().BoundingBox().zmax
        assert top <= 17 + 1e-6, "%s reaches z %.2f, above 17" % (ref, top)
    a1 = solids["antenna_A1"].val().BoundingBox()
    a2 = solids["antenna_A2"].val().BoundingBox()
    assert abs(-a1.xmin - 6.0) <= 0.1, "left antenna extends %.2f past the edge, want about 6.0" % -a1.xmin
    assert abs(a2.xmax - 168 - 5.05) <= 0.1, "right antenna extends %.2f past the edge, want about 5.05" % (a2.xmax - 168)
    assert near(a1.zmin, 11) and near(a1.zmax, 15), "antenna box z %.1f to %.1f, want 11 to 15" % (a1.zmin, a1.zmax)
    plug = solids["plug_J11"].val().BoundingBox()
    assert near(plug.xlen, 10.2) and near(plug.ymin, -15) and near(plug.zmax, 15), "plug J11 box is wrong"
    usb = solids["usb_plug"].val().BoundingBox()
    assert near(usb.ylen, 13) and near(usb.zmax, 8) and near(usb.xmin, -30), "USB plug box is wrong"
    assert near(solids["board"].val().BoundingBox().zmin, -1.6), "board is not 1.6 thick below z 0"


# --- Base -----------------------------------------------------------------

_CACHE = {}


def base_shape():
    """The base as one cq.Shape, built once per run."""
    if "base" not in _CACHE:
        import gen_enclosure
        from params import P
        _CACHE["base"] = gen_enclosure.make_base(geometry(), P).val()
    return _CACHE["base"]


def vol(shape):
    try:
        return shape.Volume()
    except Exception:
        return 0.0


def cyl(x, y, r, z0, z1):
    import cadquery as cq
    return cq.Solid.makeCylinder(r, z1 - z0, cq.Vector(x, y, z0))


def cube(x0, y0, x1, y1, z0, z1):
    import board_model
    return board_model.box(x0, y0, x1, y1, z0, z1).val()


def voids(region, shape):
    """The empty pockets of shape inside region, as solids."""
    return [s for s in region.cut(shape).Solids() if s.Volume() > 1e-6]


def void_at(region, shape, x, y):
    """The void solid of region that holds the point (x, y) at region's mid-height."""
    import cadquery as cq
    rb = region.BoundingBox()
    pt = cq.Vector(x, y, (rb.zmin + rb.zmax) / 2)
    found = [s for s in voids(region, shape) if s.isInside(pt)]
    assert len(found) == 1, "%d voids at (%.2f, %.2f), want 1" % (len(found), x, y)
    return found[0]


def board_prism(g, z0, z1):
    """The board outline extruded over z0..z1."""
    import cadquery as cq
    return (cq.Workplane("XY").polyline([tuple(pt) for pt in g["outline"]]).close()
            .extrude(z1 - z0).translate((0, 0, z0)).val())


def test_standoffs():
    from params import P
    g = geometry()
    base = base_shape()
    z_back = -g["thickness"]
    r_hole = P.insert_hole_d / 2
    keep_out = 1e-3
    for h in g["corner_holes"]:
        x, y = h["x"], h["y"]
        # Hole axis: the insert hole as a void in a slab through the standoff.
        hole = void_at(cube(x - 6, y - 6, x + 6, y + 6, z_back - 3, z_back - 1), base, x, y)
        b = hole.BoundingBox()
        assert b.xlen < 6 and b.ylen < 6, "%s: no closed insert hole in the standoff" % h["ref"]
        cx, cy = (b.xmin + b.xmax) / 2, (b.ymin + b.ymax) / 2
        assert abs(cx - x) <= 0.05 and abs(cy - y) <= 0.05, \
            "%s: insert hole at (%.3f, %.3f), board hole at (%.3f, %.3f)" % (h["ref"], cx, cy, x, y)
        assert abs(b.xlen - P.insert_hole_d) <= 0.02, "%s: insert hole %.2f across" % (h["ref"], b.xlen)
        # Top face at the board's underside.
        top = base.intersect(cube(x - 3, y - 3, x + 3, y + 3, -50, 50)).BoundingBox().zmax
        assert abs(top - z_back) <= 1e-4, "%s: standoff top at z %.3f, board back at %.3f" % (h["ref"], top, z_back)
        # Plastic round the insert hole: a 0.9 mm ring over the hole's whole depth.
        z0 = z_back - P.insert_hole_depth
        ring = cyl(x, y, r_hole + 0.9, z0, z_back).cut(cyl(x, y, r_hole, z0, z_back))
        got = vol(base.intersect(ring))
        assert got >= vol(ring) * (1 - 1e-4), \
            "%s: only %.1f of %.1f mm3 of plastic in the 0.9 mm ring round the insert" % (h["ref"], got, vol(ring))
        assert vol(base.intersect(cyl(x, y, r_hole - 0.01, z0 + 0.01, z_back))) < 1e-6, \
            "%s: insert hole is not open to depth %.1f" % (h["ref"], P.insert_hole_depth)
    # Top 2 mm: over the board, nothing beyond 3.5 mm from a hole centre.
    slab = board_prism(g, z_back - 2, z_back)
    for h in g["corner_holes"]:
        slab = slab.cut(cyl(h["x"], h["y"], P.standoff_top_radius_max + keep_out, z_back - 3, z_back + 1))
    over = vol(base.intersect(slab))
    assert over < 1e-4, "%.3f mm3 of base under the board beyond 3.5 mm of a hole in the top 2 mm" % over


def test_base_clear_of_board():
    import board_model
    import cadquery as cq
    import gen_enclosure
    from params import P
    g = geometry()
    base = base_shape()
    z_back = -g["thickness"]
    # The standoff tops touch the board on purpose: take off their top 1 mm.
    trimmed = base
    for h in g["corner_holes"]:
        trimmed = trimmed.cut(cyl(h["x"], h["y"], P.standoff_top_radius_max + 0.01, z_back - 1, z_back + 1))
    for ref, s in board_model.board_solids(g, P).items():
        s = s.val()
        v = vol(base.intersect(s))
        assert v < 1e-6, "base intersects %s (%.3f mm3)" % (ref, v)
        need = 2.0 if ref.startswith("antenna_") else 1.0
        d = trimmed.distance(s)
        assert d >= need - 1e-6, "base is %.3f mm from %s, want %.1f" % (d, ref, need)
    # Room for the cover's walls: above the floor, the base stays lip_clear
    # inside the cover's inside face (corners rounded with it).
    f = gen_enclosure.frame(g, P)
    x0, y0, x1, y1 = f.inside
    c = P.lip_clear
    room = (cq.Workplane("XY").workplane(offset=f.z_split).center((x0 + x1) / 2, (y0 + y1) / 2)
            .rect(x1 - x0 - 2 * c, y1 - y0 - 2 * c).extrude(100).edges("|Z").fillet(f.inside_r - c).val())
    above = cube(x0 - 50, y0 - 50, x1 + 50, y1 + 50, f.z_split, 100)
    out = vol(base.intersect(above.cut(room)))
    assert out < 1e-6, "%.3f mm3 of base above the floor is where the cover's walls go" % out


def test_clips():
    import cadquery as cq
    import gen_enclosure
    from params import P
    g = geometry()
    base = base_shape()
    bb = base.BoundingBox()
    z_bot = bb.zmin
    mid_y = (bb.ymin + bb.ymax) / 2
    clips = gen_enclosure.clip_positions(g, P)
    assert len(clips) == 2, "%d clips, want 2" % len(clips)
    for i, c in enumerate(clips):
        rx, ry = c["round_hole"]
        sx, sy = c["slot_centre"]
        # Holes measured in the 1.6 mm of floor left under the heads (above the markings).
        slab = (z_bot + P.mark_depth + 0.1, z_bot + P.clip_floor_left - 0.1)
        rh = void_at(cube(rx - 6, ry - 6, rx + 6, ry + 6, *slab), base, rx, ry).BoundingBox()
        sh = void_at(cube(sx - 6, sy - 6, sx + 6, sy + 6, *slab), base, sx, sy).BoundingBox()
        assert abs(rh.xlen - P.clip_hole_round) <= 0.02 and abs(rh.ylen - P.clip_hole_round) <= 0.02, \
            "clip %d round hole %.2f x %.2f" % (i, rh.xlen, rh.ylen)
        assert abs(sh.xlen - P.clip_slot_w) <= 0.02 and abs(sh.ylen - P.clip_slot_l) <= 0.02, \
            "clip %d slot %.2f across x %.2f along the hole line, want 4.0 x 5.5" % (i, sh.xlen, sh.ylen)
        rcx, rcy = rh.center.x, rh.center.y
        scx, scy = sh.center.x, sh.center.y
        assert abs(rcx - scx) <= 0.01, "clip %d holes not on one line across x" % i
        assert abs((rcy - scy) - 29.75) <= 0.01, "clip %d hole spacing %.3f, want 29.75" % (i, rcy - scy)
        assert scy < rcy, "clip %d slot is not at the upper (hook) end" % i
        rail = rcy - 16.14
        assert abs(c["rail_y"] - rail) <= 0.01, "clip %d rail_y %.2f, holes put it at %.2f" % (i, c["rail_y"], rail)
        assert abs(rail - mid_y) <= 0.01, "clip %d rail at y %.2f, box middle %.2f" % (i, rail, mid_y)
        # Floor under each head: material in the head's ring stops 1.6 above the back.
        for (hx, hy), r_in in (((rx, ry), P.clip_hole_round / 2), ((sx, sy), P.clip_slot_l / 2)):
            ring = cyl(hx, hy, 6.86 / 2 + 0.05, z_bot - 1, z_bot + 20).cut(cyl(hx, hy, r_in + 0.2, z_bot - 1, z_bot + 20))
            under = base.intersect(ring).BoundingBox()
            assert abs(under.zmax - (z_bot + 1.6)) <= 0.01, \
                "clip %d: %.2f mm of floor under the screw head at (%.1f, %.1f), want 1.6" % (i, under.zmax - z_bot, hx, hy)
        # Body on the back, inside the outline.
        x0, y0, x1, y1 = c["body_bbox"]
        assert abs((x1 - x0) - 10.0) <= 0.01 and abs((y1 - y0) - 45.4) <= 0.01, "clip %d body is not 10 x 45.4" % i
        assert abs(y1 - (ry + 4.50)) <= 0.01 and abs(y0 - (ry - 40.90)) <= 0.01, "clip %d body not latch end on the round hole" % i
        for px, py in ((x0, y0), (x1, y0), (x0, y1), (x1, y1)):
            assert base.isInside(cq.Vector(px, py, z_bot + P.mark_depth + 0.2)), \
                "clip %d body corner (%.1f, %.1f) is off the base" % (i, px, py)
    a, b = clips[0]["body_bbox"], clips[1]["body_bbox"]
    assert a[2] < b[0] or b[2] < a[0] or a[3] < b[1] or b[3] < a[1], "clip bodies overlap"
    # Markings: debossed into the back, by each clip, clear of its body.
    marks = gen_enclosure.marking_solids(g, P)
    assert len(marks) == 2, "markings for %d clips, want 2" % len(marks)
    for i, (c, m) in enumerate(zip(clips, marks)):
        x0, y0, x1, y1 = c["body_bbox"]
        for word in ("HOOK", "UP"):
            assert word in m, "clip %d has no %s marking" % (i, word)
            s = m[word].val()
            mb = s.BoundingBox()
            assert vol(s) > 1.0, "clip %d %s marking is empty" % (i, word)
            assert vol(base.intersect(s)) < 1e-6, "clip %d %s is not cut into the base" % (i, word)
            assert abs(mb.zmin - z_bot) <= 1e-6, "clip %d %s is not on the back" % (i, word)
            above = s.translate(cq.Vector(0, 0, mb.zlen))
            assert vol(base.intersect(above)) >= vol(above) * 0.999, "clip %d %s is a hole, not a deboss" % (i, word)
            assert mb.ymax < y0 and mb.xmin > x0 - 10 and mb.xmax < x1 + 10 and mb.ymax > y0 - 25, \
                "clip %d %s is not beside the clip's hook end" % (i, word)


def test_magnets_base():
    import board_model
    import gen_enclosure
    from params import P
    g = geometry()
    base = base_shape()
    f = gen_enclosure.frame(g, P)
    solids = board_model.board_solids(g, P)
    board = board_prism(g, -100, 100)
    mags = gen_enclosure.magnet_positions(g, P)
    assert len(mags) == 4, "%d magnet positions, want 4" % len(mags)
    width, depth = 6.0 + P.fit_clearance, 3.0 + 0.2
    for x, y in mags:
        pocket = void_at(cube(x - 5, y - 5, x + 5, y + 5, f.z_magnet - 6, f.z_magnet), base, x, y)
        b = pocket.BoundingBox()
        assert abs(b.xlen - width) <= 0.02 and abs(b.ylen - width) <= 0.02, \
            "pocket at (%.1f, %.1f) is %.2f x %.2f, want %.2f" % (x, y, b.xlen, b.ylen, width)
        assert abs(b.zlen - depth) <= 0.01 and abs(b.zmax - f.z_magnet) <= 1e-6, \
            "pocket at (%.1f, %.1f) is %.2f deep, top z %.2f" % (x, y, b.zlen, b.zmax)
        assert vol(base.intersect(cyl(x, y, width / 2 - 0.05, f.z_magnet, f.z_magnet + 50))) < 1e-6, \
            "pocket at (%.1f, %.1f) is not open toward the cover" % (x, y)
        for a in ("antenna_A1", "antenna_A2"):
            d = pocket.distance(solids[a].val())
            assert d >= 20, "pocket at (%.1f, %.1f) is %.1f mm from %s" % (x, y, d, a)
        assert vol(pocket.intersect(board)) < 1e-6, "pocket at (%.1f, %.1f) is under the board" % (x, y)


def test_printable_base():
    base = base_shape()
    bb = base.BoundingBox()
    assert bb.xlen <= 300 and bb.ylen <= 300, "base is %.1f x %.1f, bed is 300 x 300" % (bb.xlen, bb.ylen)
    bottom = [fc for fc in base.Faces() if fc.BoundingBox().zmax <= bb.zmin + 1e-6]
    assert bottom, "no face on the bed"
    for fc in bottom:
        assert fc.geomType() == "PLANE", "a bed face is not flat"
    biggest = max(fc.Area() for fc in bottom)
    plan = bb.xlen * bb.ylen
    assert biggest >= 0.8 * plan, "largest bed face %.0f mm2 of a %.0f mm2 footprint" % (biggest, plan)
    assert all(abs(fc.normalAt().z + 1) < 1e-6 for fc in bottom), "a bed face does not face down"


TESTS = [test_geometry_sanity, test_board_model, test_standoffs, test_base_clear_of_board,
         test_clips, test_magnets_base, test_printable_base]


def main():
    failed = 0
    for t in TESTS:
        try:
            t()
            print("ok   %s" % t.__name__)
        except Exception as e:  # report every failure, not just the first
            failed += 1
            print("FAIL %s: %s" % (t.__name__, e))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
