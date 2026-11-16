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
    # Pockets as deep as the magnet: magnets sit flush, so the faces of a
    # pair are only the cover's seating gap apart (test_cover_seats).
    width, depth = 6.0 + P.fit_clearance, 3.0
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


def test_lip():
    """The base's locating lip: plastic in the lip band on every side, none in
    each break, and lip_clear to the cover's wall."""
    import gen_enclosure
    from params import P
    g = geometry()
    base = base_shape()
    cover = parts()["cover"]
    f = gen_enclosure.frame(g, P)
    ox0, oy0, ox1, oy1 = f.lip_outer
    ix0, iy0, ix1, iy1 = f.lip_inner
    z0, z1 = f.z_split + 0.3, f.z_lip_top - 0.3
    tops = sorted((a, b) for side, a, b in f.lip_breaks if side == "top")
    lefts = [(a, b) for side, a, b in f.lip_breaks if side == "left"]
    assert len(tops) == 7 and len(lefts) == 1, "lip breaks %s" % (f.lip_breaks,)
    # One probe per side, away from the breaks and the magnet pads.
    xt = (tops[0][1] + tops[1][0]) / 2
    ym = (f.outside[1] + f.outside[3]) / 2
    yl = (lefts[0][1] + iy1) / 2
    xm = (f.outside[0] + f.outside[2]) / 2
    probes = {
        "top": (xt - 1, oy0 + 0.3, xt + 1, iy0 - 0.3),
        "bottom": (xm - 1, iy1 + 0.3, xm + 1, oy1 - 0.3),
        "left": (ox0 + 0.3, yl - 1, ix0 - 0.3, yl + 1),
        "right": (ix1 + 0.3, ym - 1, ox1 - 0.3, ym + 1),
    }
    for side, r in probes.items():
        probe = cube(*r, z0, z1)
        got = vol(base.intersect(probe))
        assert got >= vol(probe) * 0.999, "%s lip: only %.2f of %.2f mm3 of plastic in the lip band" % (side, got, vol(probe))
        # Clearance to the cover's wall, across this stretch of lip.
        x0, y0, x1, y1 = r
        reach = cube(min(x0, f.inside[0]) - 3 if side == "left" else x0,
                     min(y0, f.inside[1]) - 3 if side == "top" else y0,
                     max(x1, f.inside[2]) + 3 if side == "right" else x1,
                     max(y1, f.inside[3]) + 3 if side == "bottom" else y1, f.z_split + 0.01, f.z_lip_top + 0.5)
        lip = base.intersect(reach)
        d = cover.distance(lip)
        assert abs(d - 0.25) <= 0.01, "%s lip is %.3f from the cover, want 0.25" % (side, d)
    for side, a, b in f.lip_breaks:
        if side == "top":
            probe = cube(a + 0.1, oy0 - 0.2, b - 0.1, iy0 + 0.2, f.z_split + 0.01, f.z_lip_top + 0.5)
        else:
            probe = cube(ox0 - 0.2, a + 0.1, ix0 + 0.2, b - 0.1, f.z_split + 0.01, f.z_lip_top + 0.5)
        got = vol(base.intersect(probe))
        assert got < 1e-6, "lip break %s %.2f..%.2f holds %.2f mm3 of plastic" % (side, a, b, got)


# --- Cover, dial strip, pane -------------------------------------------------

def parts():
    """build() as cq Shapes, built once per run."""
    if "parts" not in _CACHE:
        import gen_enclosure
        from params import P
        built = gen_enclosure.build(geometry(), P)
        _CACHE["parts"] = {k: v.val() for k, v in built.items()}
    return _CACHE["parts"]


def top_parts():
    """The parts that lift off with the cover."""
    s = parts()
    return {k: s[k] for k in ("cover", "dial_strip", "dial_lettering", "pane")}


def wire_path(g):
    """A 6 mm bundle of wires into J2 from the left, centred on the
    terminal, from beyond the box to the terminal's side."""
    x0, y0, x1, y1 = g["screw_terminal"]["bbox"]
    cy = (y0 + y1) / 2
    return cube(-40, cy - 3, x0, cy + 3, 2, 8)


def obstacles(g):
    """Everything that stays put when the cover lifts: board model, plugs,
    USB plug, J2's wires."""
    import board_model
    from params import P
    out = {k: v.val() for k, v in board_model.board_solids(g, P).items()}
    out["wires_J2"] = wire_path(g)
    return out


def lights_centre(g):
    xs = [d["x"] for d in g["zone_lights"]]
    ys = [d["y"] for d in g["zone_lights"]]
    return (min(xs) + max(xs)) / 2, sum(ys) / len(ys)


def face_rings(face, step=0.01):
    """The boundary of a planar face (normal along z) as polygons (x, y)."""
    import numpy as np
    rings = []
    for w in [face.outerWire()] + face.innerWires():
        pts = []
        for e in w.Edges():
            n = max(4, int(e.Length() / step))
            ps = [e.positionAt(t) for t in np.linspace(0, 1, n + 1)]
            if pts and (abs(ps[0].x - pts[-1][0]) > 1e-6 or abs(ps[0].y - pts[-1][1]) > 1e-6):
                # Edge runs against the previous one: join it end first.
                if abs(ps[-1].x - pts[-1][0]) < 1e-6 and abs(ps[-1].y - pts[-1][1]) < 1e-6:
                    ps = ps[::-1]
            pts.extend((p.x, p.y) for p in ps)
        rings.append(np.array(pts))
    return rings


def raster(face, px):
    """The face as a boolean image, px mm per pixel, one pixel of margin."""
    import numpy as np
    from matplotlib.path import Path
    rings = face_rings(face, step=min(0.05, px))
    allp = np.vstack(rings)
    x0, y0 = allp.min(0) - 2 * px
    x1, y1 = allp.max(0) + 2 * px
    nx, ny = int((x1 - x0) / px) + 1, int((y1 - y0) / px) + 1
    xs, ys = np.meshgrid(x0 + (np.arange(nx) + 0.5) * px, y0 + (np.arange(ny) + 0.5) * px)
    pts = np.column_stack([xs.ravel(), ys.ravel()])
    img = np.zeros(nx * ny, bool)
    for r in rings:
        img ^= Path(r).contains_points(pts)
    return img.reshape(ny, nx)


def thin_parts(face, r=0.25, px=0.01):
    """Opening by a disk of radius r keeps every stroke at least 2r wide; what
    it removes is thinner than 2r. Returns the largest extent (bbox diagonal,
    mm) of a removed piece. Corner tips and the notches where strokes join
    lose pieces a few tenths long; a thin stroke loses its whole length."""
    import numpy as np
    from scipy import ndimage
    img = raster(face, px)
    core = ndimage.distance_transform_edt(img) * px > r
    opened = ndimage.distance_transform_edt(~core) * px <= r
    lost = img & ~opened
    lab, _ = ndimage.label(lost)
    ext = 0.0
    for sl in ndimage.find_objects(lab):
        ext = max(ext, float(np.hypot(sl[0].stop - sl[0].start, sl[1].stop - sl[1].start)) * px)
    return ext


def inscribed_width(face, px=0.1):
    """Diameter of the largest circle inside a planar face: the span a bridge
    over it has to cross when its edges are held."""
    from scipy import ndimage
    return 2 * float(ndimage.distance_transform_edt(raster(face, px)).max()) * px


def top_face(solid):
    zmax = solid.BoundingBox().zmax
    fs = [fc for fc in solid.Faces() if fc.geomType() == "PLANE" and abs(fc.Center().z - zmax) < 1e-6]
    assert len(fs) == 1, "%d top faces" % len(fs)
    return fs[0]


def lettering_rows():
    """The lettering's glyph solids in rows (top row first), each row in x order."""
    sols = sorted(parts()["dial_lettering"].Solids(), key=lambda s: s.BoundingBox().ymax)
    rows = [[sols[0]]]
    for s in sols[1:]:
        if s.BoundingBox().ymax - rows[-1][-1].BoundingBox().ymax > 1.0:
            rows.append([])
        rows[-1].append(s)
    return [sorted(r, key=lambda s: s.BoundingBox().center.x) for r in rows]


def test_cover_clear_of_board():
    g = geometry()
    for name, part in top_parts().items():
        for ref, s in obstacles(g).items():
            v = vol(part.intersect(s))
            assert v < 1e-6, "%s intersects %s (%.3f mm3)" % (name, ref, v)
            need = 2.0 if ref.startswith("antenna_") else 1.0
            d = part.distance(s)
            assert d >= need - 1e-6, "%s is %.3f mm from %s, want %.1f" % (name, d, ref, need)


def prism_up(face, h):
    import cadquery as cq
    from OCP.BRepPrimAPI import BRepPrimAPI_MakePrism
    from OCP.gp import gp_Vec
    return cq.Shape.cast(BRepPrimAPI_MakePrism(face.wrapped, gp_Vec(0, 0, h)).Shape())


def test_cover_lifts_off():
    """Sweep every face of the lifting parts 60 mm up; the swept volume must
    miss the base, the board model, the plugs and the wires."""
    g = geometry()
    still = dict(obstacles(g))
    still["base"] = base_shape()
    boxes = {k: v.BoundingBox() for k, v in still.items()}
    import numpy as np
    for name, part in top_parts().items():
        for fc in part.Faces():
            b = fc.BoundingBox()
            u0, u1, v0, v1 = fc._uvBounds()
            us, vs = np.meshgrid(np.linspace(u0, u1, 5), np.linspace(v0, v1, 5))
            if max(abs(n.z) for n in fc.normals(us.ravel(), vs.ravel())[0]) < 1e-9:
                continue                                 # vertical: sweeps nothing new
            sweep = None
            for k, ob in boxes.items():
                if ob.xmax < b.xmin or ob.xmin > b.xmax or ob.ymax < b.ymin or ob.ymin > b.ymax \
                        or ob.zmax < b.zmin or ob.zmin > b.zmax + 60:
                    continue
                if sweep is None:
                    sweep = prism_up(fc, 60)
                v = vol(sweep.intersect(still[k]))
                assert v < 1e-6, "%s face at z %.2f..%.2f hits %s when lifted (%.3f mm3)" % (name, b.zmin, b.zmax, k, v)


def test_cover_seats():
    import board_model
    import gen_enclosure
    from params import P
    g = geometry()
    base = base_shape()
    cover = parts()["cover"]
    f = gen_enclosure.frame(g, P)
    v = vol(cover.intersect(base))
    assert v < 1e-6, "cover intersects the base (%.3f mm3)" % v
    # The walls stand on the floor's rim.
    cb = cover.BoundingBox()
    assert abs(cb.zmin - f.z_split) <= 1e-6, "cover bottom at z %.3f, rim at %.3f" % (cb.zmin, f.z_split)
    assert cover.distance(base) <= 1e-6, "cover does not rest on the base"
    # Above the rim, nothing of the base comes nearer than the fit clearance.
    above = base.intersect(cube(-100, -100, 300, 300, f.z_split + 0.01, 100))
    d = cover.distance(above)
    assert abs(d - P.fit_clearance) <= 0.01, "cover is %.3f from the base's lip and pads, want 0.25" % d
    # Magnet pockets: coaxial pairs facing across the split, faces 0.3 at most apart.
    solids = board_model.board_solids(g, P)
    for x, y in gen_enclosure.magnet_positions(g, P):
        bp = void_at(cube(x - 5, y - 5, x + 5, y + 5, f.z_magnet - 6, f.z_magnet), base, x, y).BoundingBox()
        # The cover's pad bottom: lowest cover material round the axis.
        pad_bottom = cover.intersect(cyl(x, y, 3.8, f.z_split, 30)).BoundingBox().zmin
        cv = void_at(cube(x - 3.5, y - 3.5, x + 3.5, y + 3.5, pad_bottom, pad_bottom + 6), cover, x, y)
        cp = cv.BoundingBox()
        assert abs(cp.center.x - bp.center.x) <= 0.05 and abs(cp.center.y - bp.center.y) <= 0.05, \
            "cover pocket at (%.2f, %.2f), base pocket at (%.2f, %.2f)" % (cp.center.x, cp.center.y, bp.center.x, bp.center.y)
        assert abs(cp.xlen - bp.xlen) <= 0.02 and abs(cp.zmin - pad_bottom) <= 1e-6, \
            "cover pocket at (%.1f, %.1f) is not the base's size or does not open down" % (x, y)
        assert vol(cover.intersect(cyl(x, y, bp.xlen / 2 - 0.05, f.z_magnet, cp.zmin))) < 1e-6, \
            "cover pocket at (%.1f, %.1f) does not face the base's" % (x, y)
        gap = (cp.zmax - 3.0) - (bp.zmin + 3.0)      # 6 x 3 discs pushed to the pocket ends
        assert 0 <= gap <= 0.3 + 1e-6, "magnets at (%.1f, %.1f) are %.2f apart when seated, want 0 to 0.3" % (x, y, gap)
        for a in ("antenna_A1", "antenna_A2"):
            d = cv.distance(solids[a].val())
            assert d >= 20, "cover pocket at (%.1f, %.1f) is %.1f mm from %s" % (x, y, d, a)


def test_lights():
    from params import P
    g = geometry()
    s = parts()
    for d in g["zone_lights"]:
        c = cyl(d["x"], d["y"], 1.5, 0, 40)            # the 3 mm lens, straight up
        for k in ("cover", "dial_strip", "dial_lettering"):
            v = vol(s[k].intersect(c))
            assert v < 1e-6, "%s blocks light %s (%.3f mm3)" % (k, d["ref"], v)
    pl = g["power_light"]
    cover = s["cover"]
    z = cover.BoundingBox().zmax
    hole = void_at(cube(pl["x"] - 4, pl["y"] - 4, pl["x"] + 4, pl["y"] + 4, z - 1.5, z - 0.5), cover, pl["x"], pl["y"])
    hb = hole.BoundingBox()
    assert abs(hb.xlen - 3.0) <= 0.02 and abs(hb.ylen - 3.0) <= 0.02, "power light hole %.2f x %.2f, want 3" % (hb.xlen, hb.ylen)
    assert abs(hb.center.x - pl["x"]) <= 0.05 and abs(hb.center.y - pl["y"]) <= 0.05, "power light hole is not over D8"
    for k in ("dial_strip", "dial_lettering", "pane"):
        assert vol(s[k].intersect(cyl(pl["x"], pl["y"], 1.5, 0, 40))) < 1e-6, "%s covers the power light" % k
    # Numerals: one per light, centred on it in x, below the window, in order.
    import gen_enclosure
    rows = lettering_rows()
    assert len(rows) == 2, "lettering in %d rows, want 2 (numerals, wordmark)" % len(rows)
    nums = rows[0]
    assert len(nums) == len(g["zone_lights"]), "%d numerals for %d lights" % (len(nums), len(g["zone_lights"]))
    size = gen_enclosure.label_size(P)
    for i, (n, d) in enumerate(zip(nums, sorted(g["zone_lights"], key=lambda d: d["x"]))):
        nb = n.BoundingBox()
        assert abs(nb.center.x - d["x"]) <= 0.3, "numeral %d centre x %.2f, light %s at %.2f" % (i + 1, nb.center.x, d["ref"], d["x"])
        assert nb.ymin > d["y"] + P.window_h / 2, "numeral %d is not below the window" % (i + 1)
        want = vol(gen_enclosure.text_solid(str(i + 1), size, 0, 0, 0, P.letter_proud).val())
        assert abs(vol(n) - want) <= 0.01 * want, "the numeral under %s is not a %d" % (d["ref"], i + 1)


def test_dial_strip():
    import cadquery as cq
    import gen_enclosure
    from params import P
    g = geometry()
    s = parts()
    cover, plate, letters = s["cover"], s["dial_strip"], s["dial_lettering"]
    pb, cb, lb = plate.BoundingBox(), cover.BoundingBox(), letters.BoundingBox()
    assert vol(plate.intersect(cover)) < 1e-6 and vol(letters.intersect(cover)) < 1e-6, "strip intersects the cover"
    # Seated on the recess floor, 0.25 clear on every side (recess and window rim).
    assert vol(plate.translate(cq.Vector(0, 0, -0.02)).intersect(cover)) > 1e-4, "strip does not sit on the recess floor"
    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        near_ = plate.translate(cq.Vector(dx * 0.23, dy * 0.23, 0.01))
        far_ = plate.translate(cq.Vector(dx * 0.27, dy * 0.27, 0.01))
        assert vol(near_.intersect(cover)) < 1e-6, "strip has under 0.23 clearance toward (%d, %d)" % (dx, dy)
        assert vol(far_.intersect(cover)) > 1e-6, "strip has over 0.27 clearance toward (%d, %d)" % (dx, dy)
    # The plate is 1.8 thick in the 2.0 deep recess: its front 0.2 below the
    # cover's face, the lettering (0.4 tall) standing 0.2 above it.
    assert abs(cb.zmax - pb.zmax - 0.2) <= 0.01, "strip front at z %.3f, cover face at %.3f, want 0.2 below" % (pb.zmax, cb.zmax)
    assert abs(pb.zlen - 1.8) <= 1e-6, "strip is %.2f thick, want 1.8" % pb.zlen
    assert abs(lb.zmin - pb.zmax) <= 1e-6 and abs(lb.zlen - 0.4) <= 1e-6, \
        "lettering z %.3f..%.3f, strip front %.3f, want 0.4 tall" % (lb.zmin, lb.zmax, pb.zmax)
    assert abs(lb.zmax - cb.zmax - 0.2) <= 0.01, "lettering is not 0.2 proud of the cover's face"
    # Windows: concentric; the cover's is the design size, the strip's is the
    # cover's rim plus the fit clearance.
    cx, cy = lights_centre(g)
    win = cube(cx - 70, cy - 12, cx + 70, cy + 12, cb.zmax - 1.5, cb.zmax - 0.5)
    cw = void_at(win, cover, cx, cy).BoundingBox()
    sw = void_at(cube(cx - 70, cy - 12, cx + 70, cy + 12, pb.zmin + 0.5, pb.zmax - 0.5), plate, cx, cy).BoundingBox()
    assert abs(cw.xlen - 122) <= 0.02 and abs(cw.ylen - 12) <= 0.02, "cover window %.2f x %.2f" % (cw.xlen, cw.ylen)
    for a, b in ((cw.center.x, cx), (cw.center.y, cy), (sw.center.x, cx), (sw.center.y, cy)):
        assert abs(a - b) <= 0.05, "windows not centred on the lights"
    grow = 2 * (P.window_collar + P.fit_clearance)
    assert abs(sw.xlen - cw.xlen - grow) <= 0.02 and abs(sw.ylen - cw.ylen - grow) <= 0.02, \
        "strip window %.2f x %.2f does not fit round the cover's window rim" % (sw.xlen, sw.ylen)
    # Equal margins: window to strip edge on three sides, wordmark to the
    # bottom edge; the wordmark ends on the window's right margin.
    rows = lettering_rows()
    word = rows[1]
    wx1 = max(c.BoundingBox().xmax for c in word)
    wy1 = max(c.BoundingBox().ymax for c in word)
    lay = (cw.xmin - P.strip_reach, cw.ymin - P.strip_reach, cw.xmax + P.strip_reach)   # the layout rectangle
    m = [lay[0] - pb.xmin, pb.xmax - lay[2], lay[1] - pb.ymin, pb.ymax - wy1]
    assert max(m) - min(m) <= 0.01, "strip margins %s are not equal" % ["%.2f" % v for v in m]
    assert abs(wx1 - lay[2]) <= 0.01, "wordmark ends at x %.2f, margin at %.2f" % (wx1, lay[2])
    assert sw.xmin - pb.xmin > 2.0 and pb.xmax - sw.xmax > 2.0 and sw.ymin - pb.ymin > 2.0, "strip is thin round its window"
    # The wordmark reads `emberheat`: each glyph, in x order, is that letter.
    size = gen_enclosure.label_size(P)
    assert len(word) == 9, "wordmark has %d glyphs" % len(word)
    for ch, sol in zip("emberheat", word):
        want = vol(gen_enclosure.text_solid(ch, size, 0, 0, 0, P.letter_proud).val())
        assert abs(vol(sol) - want) <= 0.01 * want, "wordmark glyph at x %.1f is not %r" % (sol.BoundingBox().center.x, ch)
    # Read from the front after the export's y mirror: text right is +x, up is -y.
    e, b = word[0].BoundingBox(), word[2]
    bb = b.BoundingBox()
    assert bb.ymin < e.ymin - 0.5 and abs(bb.ymax - e.ymax) < 0.2, "b's ascender does not rise toward -y"
    asc = b.intersect(cube(bb.xmin - 1, bb.ymin - 1, bb.xmax + 1, e.ymin - 0.3, -100, 100)).BoundingBox()
    assert asc.center.x < bb.center.x, "b's stem is not on its left: the lettering is mirrored"
    seven = rows[0][6]
    sb = seven.BoundingBox()
    bar = seven.intersect(cube(sb.xmin - 1, sb.ymin - 1, sb.xmax + 1, sb.ymin + 0.15 * sb.ylen, -100, 100)).BoundingBox()
    foot = seven.intersect(cube(sb.xmin - 1, sb.ymax - 0.15 * sb.ylen, sb.xmax + 1, sb.ymax + 1, -100, 100)).BoundingBox()
    assert bar.xlen >= 0.8 * sb.xlen and foot.xlen < 0.6 * sb.xlen, "7's bar is not at its top (-y)"
    assert foot.center.x < sb.center.x, "7's foot is not left of centre: the lettering is mirrored"
    # Every stroke at least 0.5 mm (Review Focus 5).
    for row in rows:
        for gl in row:
            ext = thin_parts(top_face(gl))
            assert ext <= 0.8, "glyph at x %.1f has a part under 0.5 mm wide over %.2f mm" % (gl.BoundingBox().center.x, ext)


def test_pane():
    import cadquery as cq
    from params import P
    g = geometry()
    s = parts()
    cover, pane = s["cover"], s["pane"]
    pb = pane.BoundingBox()
    assert abs(vol(pane) - pb.xlen * pb.ylen * pb.zlen) < 1e-3, "pane is not a plain plate"
    assert near(pb.xlen, P.pane_w) and near(pb.ylen, P.pane_h) and near(pb.zlen, P.pane_t), "pane is %.1f x %.1f x %.1f" % (pb.xlen, pb.ylen, pb.zlen)
    assert vol(pane.intersect(cover)) < 1e-6, "pane intersects the cover"
    cx, cy = lights_centre(g)
    w2, h2 = P.window_w / 2, P.window_h / 2
    assert pb.xmin < cx - w2 and pb.xmax > cx + w2 and pb.ymin < cy - h2 and pb.ymax > cy + h2, "pane does not cover the window"
    # Slides in from above (-y) to its stop: the swept path misses the cover,
    # and from the start of the slide it lifts out of the open back.
    up = pb.ylen + 1
    path = cube(pb.xmin, pb.ymin - up, pb.xmax, pb.ymax, pb.zmin, pb.zmax)
    assert vol(path.intersect(cover)) < 1e-6, "pane's slide path hits the cover"
    out = cube(pb.xmin, pb.ymin - up, pb.xmax, pb.ymax - up, pb.zmin - 40, pb.zmax)
    assert vol(out.intersect(cover)) < 1e-6, "pane cannot be put in from the open back"
    # Held: it cannot move away from the face, past its stop, or sideways.
    for v, what in (((0, 0, -0.5), "away from the face"), ((0, 0.3, 0), "past its stop"),
                    ((0.4, 0, 0), "sideways (+x)"), ((-0.4, 0, 0), "sideways (-x)")):
        assert vol(pane.translate(cq.Vector(*v)).intersect(cover)) > 1e-6, "pane is free to move %s" % what


def test_styling():
    import gen_enclosure
    from params import P
    g = geometry()
    cover = parts()["cover"]
    f = gen_enclosure.frame(g, P)
    corners = []
    for fc in cover.Faces():
        if fc.geomType() != "CYLINDER":
            continue
        c = fc._geomAdaptor().Cylinder()
        ax = c.Axis().Direction()
        if abs(abs(ax.Z()) - 1) < 1e-9 and abs(c.Radius() - 8.0) < 1e-6:
            corners.append((round(c.Location().X(), 3), round(c.Location().Y(), 3)))
    ox0, oy0, ox1, oy1 = f.outside
    want = sorted({(round(x, 3), round(y, 3)) for x in (ox0 + 8, ox1 - 8) for y in (oy0 + 8, oy1 - 8)})
    assert sorted(set(corners)) == want, "R8 vertical corner faces at %s, want %s" % (sorted(set(corners)), want)
    # Vents: a row of equal openings through the bottom wall, evenly pitched,
    # centred on the wall, above the lip and not blocked behind.
    ix0, iy0, ix1, iy1 = f.inside
    r = f.outside_r
    slab = cube(ox0 + r, iy1 + 0.3, ox1 - r, oy1 - 0.3, f.z_split + 0.5, f.z_face_inside - 0.5)
    vents = sorted(voids(slab, cover), key=lambda v: v.BoundingBox().center.x)
    assert len(vents) >= 5, "%d vent openings in the bottom wall" % len(vents)
    bbs = [v.BoundingBox() for v in vents]
    xs = [b.center.x for b in bbs]
    pitch = [b - a for a, b in zip(xs, xs[1:])]
    assert max(pitch) - min(pitch) <= 0.01, "vent pitch varies: %.3f to %.3f" % (min(pitch), max(pitch))
    assert abs((xs[0] + xs[-1]) / 2 - (ox0 + ox1) / 2) <= 0.01, "vent row is not centred on the wall"
    for b in bbs:
        assert abs(b.xlen - bbs[0].xlen) <= 0.01 and abs(b.zlen - bbs[0].zlen) <= 0.01, "vents differ in size"
        assert b.zmin > f.z_lip_top, "vent at x %.1f starts below the lip top" % b.center.x
        behind = cube(b.xmin + 0.01, iy1 - 10, b.xmax - 0.01, iy1 + 0.3, b.zmin + 0.01, b.zmax - 0.01)
        assert vol(cover.intersect(behind)) < 1e-6, "vent at x %.1f is blocked inside" % b.center.x


def edge_faces(shape, edge):
    """The faces of shape that meet at edge."""
    import cadquery as cq
    from OCP.TopAbs import TopAbs_EDGE, TopAbs_FACE
    from OCP.TopExp import TopExp
    from OCP.TopTools import TopTools_IndexedDataMapOfShapeListOfShape
    key = id(shape)
    if _CACHE.get("edge_map_key") != key:
        m = TopTools_IndexedDataMapOfShapeListOfShape()
        TopExp.MapShapesAndAncestors_s(shape.wrapped, TopAbs_EDGE, TopAbs_FACE, m)
        _CACHE["edge_map_key"], _CACHE["edge_map"] = key, m
    m = _CACHE["edge_map"]
    i = m.FindIndex(edge.wrapped)
    return [cq.Face(f) for f in m.FindFromIndex(i)] if i else []


def overhangs(shape, up, bed_z, bridge_limit):
    """Faces of shape that, printed with `up` (+1 or -1 along z) and the face
    at bed_z on the bed, overhang more than 45 degrees and are not bridges
    within bridge_limit(face). Returns descriptions."""
    import numpy as np
    bad = []
    for fc in shape.Faces():
        b = fc.BoundingBox()
        if fc.geomType() == "PLANE" and abs(b.zmin - bed_z) < 1e-6 and abs(b.zmax - bed_z) < 1e-6:
            continue                                     # on the bed
        u0, u1, v0, v1 = fc._uvBounds()
        us, vs = np.meshgrid(np.linspace(u0, u1, 7), np.linspace(v0, v1, 7))
        ns, _ = fc.normals(us.ravel(), vs.ravel())
        worst = min(n.z * up for n in ns)
        if worst >= -np.cos(np.radians(45)) - 1e-6:
            continue
        if fc.geomType() == "PLANE" and abs(fc.normalAt().z * up + 1) < 1e-9:
            span = inscribed_width(fc)
            if span > bridge_limit(fc):
                bad.append("bridge of %.1f mm at z %.2f" % (span, b.zmin))
            # A bridge is held at every edge: some face meeting it there runs
            # down toward the bed. An edge with nothing under it is a
            # cantilever.
            for e in fc.Edges():
                held = False
                for other in edge_faces(shape, e):
                    if other.isSame(fc):
                        continue
                    ob = other.BoundingBox()
                    below = (b.zmin - ob.zmin) if up > 0 else (ob.zmax - b.zmax)
                    held = held or below > 0.1
                if not held:
                    c = e.Center()
                    bad.append("bridge at z %.2f has a free edge at (%.1f, %.1f)" % (b.zmin, c.x, c.y))
                    break
        else:
            bad.append("%s face at (%.1f, %.1f, %.1f) overhangs" % (fc.geomType(), b.center.x, b.center.y, b.center.z))
    return bad


def test_printable_cover():
    import gen_enclosure
    from params import P
    g = geometry()
    s = parts()
    cover, plate, pane = s["cover"], s["dial_strip"], s["pane"]
    for name, part in (("cover", cover), ("dial_strip", plate), ("pane", pane)):
        b = part.BoundingBox()
        assert b.xlen <= 300 and b.ylen <= 300, "%s is %.1f x %.1f, bed is 300 x 300" % (name, b.xlen, b.ylen)
    # Bed faces: cover face down, strip back down (lettering up), pane flat.
    cb, pb, nb = cover.BoundingBox(), plate.BoundingBox(), pane.BoundingBox()
    for name, part, z, nz in (("cover", cover, cb.zmax, 1), ("dial_strip", plate, pb.zmin, -1), ("pane", pane, nb.zmin, -1)):
        bed = [fc for fc in part.Faces() if fc.geomType() == "PLANE" and abs(fc.BoundingBox().zmin - z) < 1e-6
               and abs(fc.BoundingBox().zmax - z) < 1e-6]
        assert bed and all(abs(fc.normalAt().z - nz) < 1e-9 for fc in bed), "%s has no flat face on the bed" % name
        b = part.BoundingBox()
        area = sum(fc.Area() for fc in bed)
        assert area >= 0.6 * b.xlen * b.ylen, "%s: %.0f mm2 on the bed of a %.0f mm2 footprint" % (name, area, b.xlen * b.ylen)
    # Overhangs. The strip's recess floor may bridge 30 mm (hidden behind
    # the strip); everything else 20.
    d = gen_enclosure.dial(g, P)
    rx0, ry0, rx1, ry1 = d["recess"]
    z_floor = cb.zmax - P.dial_t

    def limit(fc):
        fb = fc.BoundingBox()
        inside = fb.xmin >= rx0 - 1e-6 and fb.xmax <= rx1 + 1e-6 and fb.ymin >= ry0 - 1e-6 and fb.ymax <= ry1 + 1e-6
        return 30.0 if inside and abs(fb.zmin - z_floor) < 1e-6 else 20.0
    for name, part, up, z in (("cover", cover, -1, cb.zmax), ("dial_strip", plate, 1, pb.zmin), ("pane", pane, 1, nb.zmin)):
        bad = overhangs(part, up, z, limit)
        assert not bad, "%s: %s" % (name, "; ".join(bad[:5]))
    letters = s["dial_lettering"]
    assert not overhangs(letters, 1, letters.BoundingBox().zmin, lambda fc: 0.0), "lettering overhangs"

# --- Exports ------------------------------------------------------------------

OUT = os.path.join(HERE, "out")
EXPORT_NAMES = ("base", "cover", "dial-strip", "pane", "test-piece", "test-piece-cover")


def sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def hashes_file():
    """enclosure/out/hashes.txt as a dict: 'name: value' lines."""
    path = os.path.join(OUT, "hashes.txt")
    assert os.path.exists(path), "no out/hashes.txt (run make enclosure)"
    out = {}
    with open(path) as f:
        for line in f:
            if ":" in line and not line.startswith("#"):
                k, v = line.split(":", 1)
                out[k.strip()] = v.strip()
    return out


def test_exports_current():
    for name in EXPORT_NAMES:
        for ext in ("step", "stl"):
            path = os.path.join(OUT, "%s.%s" % (name, ext))
            assert os.path.exists(path) and os.path.getsize(path) > 500, "export %s missing or empty (run make enclosure)" % path
    h = hashes_file()
    sources = {"board-geometry.json": "board-geometry.json", "params.py": "params.py",
               "gen_enclosure.py": "gen_enclosure.py", "font": os.path.join("fonts", "Inter-SemiBold.ttf"),
               "export_enclosure.py": "export_enclosure.py", "render_enclosure.py": "render_enclosure.py",
               "board_model.py": "board_model.py"}
    for key, rel in sources.items():
        assert key in h, "hashes.txt has no %s" % key
        assert h[key] == sha256(os.path.join(HERE, rel)), "%s changed since the export (run make enclosure)" % key
    assert float(h["lettering_start_height_mm"]) > 0


def load_step(name):
    import cadquery as cq
    return cq.importers.importStep(os.path.join(OUT, name + ".step")).val()


def stl_triangles(name):
    """Binary STL as an (n, 3, 3) array."""
    import numpy as np
    with open(os.path.join(OUT, name + ".stl"), "rb") as f:
        data = f.read()
    assert not data.startswith(b"solid") or len(data) == 84 + 50 * int.from_bytes(data[80:84], "little"), "%s.stl is not binary" % name
    n = int.from_bytes(data[80:84], "little")
    assert len(data) == 84 + 50 * n, "%s.stl size does not match its triangle count" % name
    rec = np.frombuffer(data, dtype=np.dtype([("n", "<f4", 3), ("v", "<f4", (3, 3)), ("a", "<u2")]), count=n, offset=84)
    return rec["v"].astype(float)


def inside(shape, x, y, z):
    import cadquery as cq
    return shape.isInside(cq.Vector(x, y, z))


def test_exports_stl_match_step():
    """Each STL is a closed, correctly wound copy of its STEP: same box, volume
    within 1 % (a mirrored mesh without flipped winding would be negative)."""
    for name in EXPORT_NAMES:
        s = load_step(name)
        bb = s.BoundingBox()
        tri = stl_triangles(name)
        lo, hi = tri.reshape(-1, 3).min(axis=0), tri.reshape(-1, 3).max(axis=0)
        for got, want, ax in ((lo[0], bb.xmin, "xmin"), (hi[0], bb.xmax, "xmax"), (lo[1], bb.ymin, "ymin"),
                              (hi[1], bb.ymax, "ymax"), (lo[2], bb.zmin, "zmin"), (hi[2], bb.zmax, "zmax")):
            assert abs(got - want) < 0.05, "%s.stl %s %.3f, STEP %.3f" % (name, ax, got, want)
        v = (tri[:, 0] * __import__("numpy").cross(tri[:, 1], tri[:, 2])).sum() / 6.0
        assert abs(v - s.Volume()) <= 0.01 * s.Volume(), "%s.stl volume %.0f, STEP %.0f" % (name, v, s.Volume())
        assert abs(bb.zmin) < 1e-6, "%s is not on the bed (zmin %.4f)" % (name, bb.zmin)
        assert bb.xlen <= 300 and bb.ylen <= 300, "%s does not fit the bed" % name


def test_exports_are_the_real_part():
    """The model is built in the board frame (x right, y down, z toward the
    viewer), which is left-handed: every export must be mirrored in y. Proved
    on the exported files, put back into the assembled position (z only for the
    base and strip; the cover is turned 180 degrees about x for printing, so
    turn it back)."""
    import cadquery as cq
    import gen_enclosure
    from params import P
    g = geometry()
    f = gen_enclosure.frame(g, P)

    # Base, as assembled: floor on the bed, so z = bed + (assembled z - z_bottom).
    base = load_step("base")
    zf = -9.5 - f.z_bottom    # a height in the 1.6 mm of floor under the clip holes, above the bed
    clips = gen_enclosure.clip_positions(g, P)
    assert len(clips) == 2
    for c in clips:
        x = c["x"]
        slot_y, hole_y = -c["slot_centre"][1], -c["round_hole"][1]   # mirrored
        assert slot_y > hole_y, "DIN clip slot (hook end) is not at larger Y than the round hole"
        assert not inside(base, x, slot_y + 2.4, zf) and not inside(base, x, slot_y - 2.4, zf), "no slot at its mirrored place"
        assert not inside(base, x, hole_y, zf), "no round hole at its mirrored place"
        assert inside(base, x + 5.5, slot_y, zf) and inside(base, x + 5.5, hole_y, zf), "no floor beside the clip holes"
        assert inside(base, x, hole_y + 2.4, zf) and inside(base, x, hole_y - 2.4, zf), "hole is a slot, not round"
    # Lip breaks: power input notch at smaller X, zone terminal notches at larger Y.
    zl = (f.z_split + f.z_lip_top) / 2 - f.z_bottom
    lx = (f.lip_outer[0] + f.lip_inner[0]) / 2
    ty = -(f.lip_outer[1] + f.lip_inner[1]) / 2
    by = -(f.lip_outer[3] + f.lip_inner[3]) / 2
    left = [b for b in f.lip_breaks if b[0] == "left"][0]
    top = [b for b in f.lip_breaks if b[0] == "top"]
    assert not inside(base, lx, -(left[1] + left[2]) / 2, zl), "no left lip break at the mirrored place"
    assert inside(base, lx, -20.0, zl), "left lip is not solid away from its break"
    for _, x0, x1 in top:
        assert not inside(base, (x0 + x1) / 2, ty, zl), "no top lip break at the mirrored place"
        assert inside(base, (x0 + x1) / 2, by, zl), "bottom lip is not solid under a zone break"
    assert ty > by, "zone terminals' side of the lip is not at larger Y"

    # Test piece: the same mirrored base, cut down. Standoff H3 at board (4, 110):
    # insert hole open at the centre, plastic round it; the magnet pocket of the
    # bottom pad at (68.96, 111.925) open, plastic round it.
    tp = load_step("test-piece")
    assert len(tp.Solids()) == 2, "test piece should be two bodies"
    zi = -4.0 - f.z_bottom
    hx, hy = 4.0, -110.0
    assert not inside(tp, hx, hy, zi) and inside(tp, hx + 3.0, hy, zi) and inside(tp, hx, hy - 3.0, zi), "test piece: standoff not at its mirrored place"
    mx, my = 68.96, -111.925
    assert not inside(tp, mx, my, zi) and inside(tp, mx + 3.5, my, zi), "test piece: magnet pocket not at its mirrored place"
    assert tp.Volume() < 0.2 * base.Volume()

    # Cover: turn back (rotate 180 about x, then z -> top 27).
    cover = load_step("cover").rotate(cq.Vector(0, 0, 0), cq.Vector(1, 0, 0), 180).translate(cq.Vector(0, 0, f.z_top))
    zc = 5.0
    wall_top = -(f.outside[1] + f.inside[1]) / 2
    wall_bot = -(f.outside[3] + f.inside[3]) / 2
    wall_left = (f.outside[0] + f.inside[0]) / 2
    for _, x0, x1 in top:
        xm = (x0 + x1) / 2
        assert not inside(cover, xm, wall_top, zc), "cover has no zone notch at its mirrored place"
        assert inside(cover, xm, wall_bot, zc), "cover bottom wall is cut under a zone notch"
        assert inside(cover, xm + 9, wall_top, zc), "cover wall is not solid between notches"
    assert not inside(cover, wall_left, -(left[1] + left[2]) / 2, zc), "cover has no power input notch at the mirrored place"
    assert inside(cover, wall_left, -20.0, zc), "cover left wall is not solid away from its notch"
    # Cover test piece (same turn-back): window end with its rim, bottom-left
    # corner wall, bottom pad pocket.
    ct = load_step("test-piece-cover").rotate(cq.Vector(0, 0, 0), cq.Vector(1, 0, 0), 180).translate(cq.Vector(0, 0, f.z_top))
    assert len(ct.Solids()) == 3, "cover test piece should be three bodies"
    wy = -lights_centre(g)[1]
    assert not inside(ct, 40.0, wy, 26.0) and inside(ct, 27.0, wy, 26.0), "cover test piece: window end not at its mirrored place"
    assert inside(ct, 0.0, -117.2, 5.0), "cover test piece: corner wall not at the mirrored place"
    assert not inside(ct, mx, my, -2.0) and inside(ct, mx + 3.5, my, -2.0), "cover test piece: magnet pocket not at its mirrored place"
    # The face is on the bed in the print file.
    cp = load_step("cover")
    assert abs(cp.BoundingBox().zmin) < 1e-6 and inside(cp, 84, 100, 1.0), "cover face is not on the bed"

    # Dial strip: viewed from +z (lettering up) the lettering reads left to right.
    strip = load_step("dial-strip")
    start = gen_enclosure.dial(g, P)["z_floor"]
    t0 = float(hashes_file()["lettering_start_height_mm"])
    assert near(t0, P.strip_t, 1e-3), "lettering start %.3f, strip is %.3f thick" % (t0, P.strip_t)
    z_shift = -start          # assembled z (recess floor, the strip back) -> bed z
    ref = gen_enclosure.build(g, P)["dial_lettering"].val()
    mirrored = ref.mirror("XZ").translate(cq.Vector(0, 0, z_shift))
    plain = ref.translate(cq.Vector(0, 0, z_shift))
    top_part = strip.intersect(cq.Solid.makeBox(400, 400, 5, cq.Vector(-100, -300, t0 + 0.001)))
    lv = top_part.Volume()
    assert lv > 0.98 * ref.Volume(), "lettering is missing from the strip body (%.1f of %.1f mm3)" % (lv, ref.Volume())
    common_ok = top_part.intersect(mirrored.intersect(cq.Solid.makeBox(400, 400, 5, cq.Vector(-100, -300, t0 + 0.001)))).Volume()
    common_bad = top_part.intersect(plain.intersect(cq.Solid.makeBox(400, 400, 5, cq.Vector(-100, -300, t0 + 0.001)))).Volume()
    assert common_ok > 0.98 * lv, "exported lettering is not the y-mirrored lettering (%.1f of %.1f mm3 match)" % (common_ok, lv)
    assert common_bad < 0.5 * lv, "exported lettering matches the unmirrored model (%.1f of %.1f mm3)" % (common_bad, lv)
    # Left to right: numeral 1 at smaller X than 7; in the wordmark e at smaller X than t.
    glyphs = sorted(top_part.Solids(), key=lambda s: s.Center().x)
    assert len(glyphs) == 16, "%d lettering glyphs, want 16" % len(glyphs)
    ref_g = {}
    for ch in "17et":
        ref_g[ch] = gen_enclosure.text_solid(ch, gen_enclosure.label_size(P), 0, 0, 0, P.letter_proud).val().Volume()
    nums = [s for s in glyphs if s.Center().y > -57]         # numeral row is above the wordmark (larger Y)
    word = [s for s in glyphs if s.Center().y <= -57]
    assert len(nums) == 7 and len(word) == 9, "%d numerals and %d wordmark glyphs" % (len(nums), len(word))
    assert abs(nums[0].Volume() - ref_g["1"]) < 0.01 * ref_g["1"], "first numeral is not a 1"
    assert abs(nums[-1].Volume() - ref_g["7"]) < 0.01 * ref_g["7"], "last numeral is not a 7"
    assert abs(word[0].Volume() - ref_g["e"]) < 0.01 * ref_g["e"], "wordmark does not start with e"
    assert abs(word[-1].Volume() - ref_g["t"]) < 0.01 * ref_g["t"], "wordmark does not end with t"
    assert nums[0].Center().x < nums[-1].Center().x and word[0].Center().x < word[-1].Center().x
    # The wordmark is under the numerals in the real part: lower on the face = smaller Y.
    assert max(s.Center().y for s in word) < min(s.Center().y for s in nums)


TESTS = [test_geometry_sanity, test_board_model, test_standoffs, test_base_clear_of_board,
         test_clips, test_magnets_base, test_printable_base, test_lip, test_cover_clear_of_board,
         test_cover_lifts_off, test_cover_seats, test_lights, test_dial_strip, test_pane,
         test_styling, test_printable_cover, test_exports_current,
         test_exports_stl_match_step, test_exports_are_the_real_part]


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
