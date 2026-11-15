#!/usr/bin/env python3
"""Checks on the circuit board (carrier-board.kicad_pcb, written by
`make board`) against the netlist, bom.csv, part-checks.md and the placement
rules of the design. Run with the KiCad Python (needs pcbnew).

zone_side_items() and logic_side_items() are also used after routing (Task 5).
"""
import csv
import json
import math
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import netlist  # noqa: E402

ROOT = os.path.dirname(HERE)
BOARD = os.path.join(ROOT, "carrier-board.kicad_pcb")
BOM = os.path.join(ROOT, "bom.csv")
PART_CHECKS = os.path.join(ROOT, "part-checks.md")
KICAD_CLI = "/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli"
ZONES = range(1, 8)

SEPARATION = 3.0          # mm, zone-side copper to logic-side copper
PAD_EDGE_MARGIN = 1.0     # mm, every pad inside the outline by at least this
ZONE_TRACK_WIDTH = 1.5    # mm
NOTCH_MIN = 60.0          # mm
CORNER_HOLE_INSET = 4.0   # mm
CLIP_BODY_MARGIN = 0.75   # mm round the clip body: +/-0.25 width tolerance, 0.5 for solder fillets
USB_EDGE_TOL = 1.0        # mm
# Board silkscreen that must be present, verbatim.
SILK_TEXT = ["ZONE WIRING 24 V AC", "rev 1", "5V ⎓ 3A USB-C", "+5V", "GND",
             "H2 3V3 — REMOVE BEFORE USING H2 USB", "1-WIRE", "3V3", "DQ",
             "IO20", "TX", "RX", "EmberHeat"]

ZONE_NET = re.compile(r"Z\d_[AB]")
RELAY_CONTACTS = {"11", "12", "14"}


def mm(v):
    import pcbnew
    return pcbnew.ToMM(v)


def nm(v):
    import pcbnew
    return pcbnew.FromMM(v)


# --------------------------------------------------------------------------
# Copper items and distances

class Item:
    """One piece of copper on one layer: a pad, track, via or zone fill."""

    def __init__(self, desc, layer, poly, net, pad=None):
        self.desc, self.layer, self.poly, self.net, self.pad = desc, layer, poly, net, pad
        bb = poly.BBox()
        self.box = (bb.GetLeft(), bb.GetTop(), bb.GetRight(), bb.GetBottom())


def _pad_poly(pad, layer):
    import pcbnew
    ps = pcbnew.SHAPE_POLY_SET()
    pad.TransformShapeToPolygon(ps, layer, 0, nm(0.002), pcbnew.ERROR_OUTSIDE)
    return ps


def _pad_name(pad):
    return "%s.%s" % (pad.GetParentFootprint().GetReference(), pad.GetNumber() or "hole")


def copper_items(board):
    """Every piece of copper on F.Cu and B.Cu. Non-plated holes have none."""
    import pcbnew
    items = []
    layers = (pcbnew.F_Cu, pcbnew.B_Cu)
    for fp in board.GetFootprints():
        for pad in fp.Pads():
            if pad.GetAttribute() == pcbnew.PAD_ATTRIB_NPTH:
                continue
            for layer in layers:
                if pad.IsOnLayer(layer):
                    items.append(Item("pad %s" % _pad_name(pad), layer, _pad_poly(pad, layer),
                                      netlist.bare(pad.GetNetname()), pad))
    for t in board.GetTracks():
        net = netlist.bare(t.GetNetname())
        for layer in layers:
            if t.IsOnLayer(layer):
                ps = pcbnew.SHAPE_POLY_SET()
                t.TransformShapeToPolygon(ps, layer, 0, nm(0.002), pcbnew.ERROR_OUTSIDE)
                kind = "via" if t.GetClass() == "PCB_VIA" else "track"
                items.append(Item("%s %s at (%.2f, %.2f)" % (kind, net, mm(t.GetStart().x), mm(t.GetStart().y)),
                                  layer, ps, net))
    for z in board.Zones():
        if z.GetIsRuleArea():
            continue
        net = netlist.bare(z.GetNetname())
        for layer in layers:
            if z.IsOnLayer(layer) and z.HasFilledPolysForLayer(layer):
                ps = z.GetFilledPolysList(layer)
                if ps.OutlineCount():
                    items.append(Item("zone fill %s" % net, layer, ps, net))
    return items


def is_zone_side(item):
    """Zone side: copper on Z1_A..Z7_B, plus every relay contact pad (11, 12,
    14). Pad 12 has no net but is joined to the common contact whenever the
    relay is off, so it carries zone voltage."""
    if ZONE_NET.fullmatch(item.net):
        return True
    p = item.pad
    return p is not None and re.fullmatch(r"K\d+", p.GetParentFootprint().GetReference()) is not None \
        and p.GetNumber() in RELAY_CONTACTS


def _classify(board):
    zone, logic = [], []
    for it in copper_items(board):
        (zone if is_zone_side(it) else logic).append(it)
    return zone, logic


def zone_side_items(board):
    return _classify(board)[0]


def logic_side_items(board):
    return _classify(board)[1]


def _points(ps, box=None):
    """Vertices of a polygon set (outlines and holes), optionally only those
    inside box (left, top, right, bottom)."""
    for oi in range(ps.OutlineCount()):
        chains = [ps.COutline(oi)] + [ps.CHole(oi, h) for h in range(ps.HoleCount(oi))]
        for c in chains:
            for i in range(c.PointCount()):
                p = c.CPoint(i)
                if box is None or (box[0] <= p.x <= box[2] and box[1] <= p.y <= box[3]):
                    yield p


def _grow(box, d):
    return (box[0] - d, box[1] - d, box[2] + d, box[3] + d)


def _box_gap(a, b):
    dx = max(a[0] - b[2], b[0] - a[2], 0)
    dy = max(a[1] - b[3], b[1] - a[3], 0)
    return math.hypot(dx, dy)


def poly_distance(a, b, a_box=None, b_box=None, reach=None):
    """Smallest distance (nm) between two polygon sets; 0 if they touch.
    For disjoint polygons the minimum is from a vertex of one to the other."""
    import pcbnew
    inter = pcbnew.SHAPE_POLY_SET(a)
    inter.BooleanIntersection(b)
    if inter.OutlineCount():
        return 0
    a_box = a_box or _bbox(a)
    b_box = b_box or _bbox(b)
    r = reach if reach is not None else nm(20)
    best = None
    for p in _points(a, _grow(b_box, r)):
        d = b.SquaredDistance(p)
        best = d if best is None or d < best else best
    for p in _points(b, _grow(a_box, r)):
        d = a.SquaredDistance(p)
        best = d if best is None or d < best else best
    return math.sqrt(best) if best is not None else float("inf")


def _bbox(ps):
    bb = ps.BBox()
    return (bb.GetLeft(), bb.GetTop(), bb.GetRight(), bb.GetBottom())


def smallest_separation(zone, logic, reach_mm=10.0):
    """(distance mm, zone item, logic item) for the closest pair on a shared
    layer, looking only at pairs whose boxes are within reach_mm."""
    best = (float("inf"), None, None)
    reach = nm(reach_mm)
    for z in zone:
        for g in logic:
            if g.layer != z.layer or _box_gap(z.box, g.box) > reach:
                continue
            d = poly_distance(z.poly, g.poly, z.box, g.box, reach)
            if d < best[0]:
                best = (d, z, g)
    return (mm(best[0]) if best[1] else float("inf"), best[1], best[2])


# --------------------------------------------------------------------------
# Board geometry

def outline(board):
    import pcbnew
    ps = pcbnew.SHAPE_POLY_SET()
    assert board.GetBoardPolygonOutlines(ps), "board outline is not closed"
    assert ps.OutlineCount() == 1, "board outline has %d parts" % ps.OutlineCount()
    return ps


def outline_box(board):
    """(left, top, right, bottom) in mm."""
    return tuple(mm(v) for v in _bbox(outline(board)))


def notch_edges(board):
    """x (mm) of the outline's two vertical edges that are neither the left
    nor the right side of the board, and the y of the notch's top edge."""
    ol = outline(board)
    left, top, right, bottom = outline_box(board)
    chain = ol.COutline(0)
    n = chain.PointCount()
    xs, tops = [], []
    for i in range(n):
        a, b = chain.CPoint(i), chain.CPoint((i + 1) % n)
        if a.x == b.x and abs(mm(a.x) - left) > 0.01 and abs(mm(a.x) - right) > 0.01:
            xs.append(mm(a.x))
            tops.append(min(mm(a.y), mm(b.y)))
    assert len(xs) == 2, "expected two notch sides in the outline, found %s" % xs
    return sorted(xs), min(tops)


def zone_line_y(board):
    """y (mm) of the silkscreen line that marks the lower limit of the zone
    strip: the board-level horizontal F.SilkS segments."""
    import pcbnew
    ys = set()
    for d in board.GetDrawings():
        if d.GetClass() == "PCB_SHAPE" and d.GetLayer() == pcbnew.F_SilkS \
                and d.GetShape() == pcbnew.SHAPE_T_SEGMENT \
                and d.GetStart().y == d.GetEnd().y and abs(d.GetEnd().x - d.GetStart().x) > nm(2):
            ys.add(round(mm(d.GetStart().y), 4))
    assert len(ys) == 1, "expected the zone line at one y, found board silk lines at %s" % sorted(ys)
    return ys.pop()


def rect_poly(l, t, r, b):
    import pcbnew
    ps = pcbnew.SHAPE_POLY_SET()
    ps.NewOutline()
    for x, y in ((l, t), (r, t), (r, b), (l, b)):
        ps.Append(nm(x), nm(y))
    return ps


def rotate(x, y, deg):
    """A footprint-frame offset turned as KiCad turns a footprint by deg
    (counter-clockwise on screen, y down)."""
    t = math.radians(deg)
    return (x * math.cos(t) + y * math.sin(t), -x * math.sin(t) + y * math.cos(t))


def to_board(fp, x, y):
    dx, dy = rotate(x, y, fp.GetOrientationDegrees())
    return (mm(fp.GetPosition().x) + dx, mm(fp.GetPosition().y) + dy)


def pad_xy(fp, number):
    pads = [p for p in fp.Pads() if p.GetNumber() == number]
    assert len(pads) == 1, "%s has %d pads %s" % (fp.GetReference(), len(pads), number)
    return mm(pads[0].GetPosition().x), mm(pads[0].GetPosition().y)


def silk_rects(fp):
    """Axis-aligned boxes (mm) of a footprint's F.SilkS rectangles."""
    import pcbnew
    out = []
    for g in fp.GraphicalItems():
        if g.GetClass() == "PCB_SHAPE" and g.GetLayer() == pcbnew.F_SilkS and g.GetShape() == pcbnew.SHAPE_T_RECT:
            bb = g.GetBoundingBox()
            # The box includes half the line width; take the line's centre.
            w = mm(g.GetWidth()) / 2
            out.append((mm(bb.GetLeft()) + w, mm(bb.GetTop()) + w, mm(bb.GetRight()) - w, mm(bb.GetBottom()) - w))
    return out


def dev_board(fp):
    """(pcb_rect, antenna_rect, antenna_dir): the dev board's PCB outline and
    antenna overhang as drawn on its silkscreen, and the unit vector the
    antenna points along (footprint -y, turned)."""
    rects = sorted(silk_rects(fp), key=lambda r: (r[2] - r[0]) * (r[3] - r[1]))
    assert len(rects) == 2, "%s: expected 2 silkscreen rectangles, found %d" % (fp.GetReference(), len(rects))
    ant, pcb = rects
    d = rotate(0, -1, fp.GetOrientationDegrees())
    return pcb, ant, (round(d[0], 6), round(d[1], 6))


def part_checks_clip():
    """DIN clip figures from part-checks.md."""
    with open(PART_CHECKS) as f:
        text = f.read()
    sec = text.split("## DIN rail clip H5, H6", 1)[1].split("\n## ", 1)[0]
    m = re.search(r"round hole ([\d.]+) mm at the origin and a slot ([\d.]+) mm wide by ([\d.]+) mm long.*?centred ([\d.]+) mm from it", sec)
    assert m, "DIN clip hole figures not found in part-checks.md"
    hole, slot_w, slot_l, spacing = (float(g) for g in m.groups())
    m2 = re.search(r"([\d.]+) \+/- [\d.]+ mm apart", sec)
    assert m2 and float(m2.group(1)) == spacing, "clip hole spacing in part-checks.md disagrees with the footprint line"
    num = r"(-?\d+(?:\.\d+)?)"
    m = re.search(r"body x %s to %s, y %s to %s; rail centre at x %s" % ((num,) * 5), sec)
    assert m, "DIN clip body outline not found in part-checks.md"
    x0, x1, y0, y1, rail = (float(g) for g in m.groups())
    return dict(hole=hole, slot_w=slot_w, slot_l=slot_l, spacing=spacing, body=(x0, x1, y0, y1), rail=rail)


def clip_body_box(fp, body):
    """Board box (mm) of a clip body given in the footprint frame."""
    x0, x1, y0, y1 = body
    pts = [to_board(fp, x, y) for x in (x0, x1) for y in (y0, y1)]
    return (min(p[0] for p in pts), min(p[1] for p in pts), max(p[0] for p in pts), max(p[1] for p in pts))


def bom_rows():
    rows = {}
    with open(BOM, newline="") as f:
        for row in csv.DictReader(f):
            for ref in row["refs"].split():
                rows[ref] = row
    return rows


def silk_texts(board):
    import pcbnew
    return [d for d in board.GetDrawings() if d.GetClass() == "PCB_TEXT" and d.GetLayer() == pcbnew.F_SilkS]


# --------------------------------------------------------------------------
# Tests

def test_all_parts_present(board, nl):
    import pcbnew
    rows = bom_rows()
    placed = {fp.GetReference(): fp for fp in board.GetFootprints()}
    assert set(placed) == set(nl.parts), "board has extra %s, missing %s" % (
        sorted(set(placed) - set(nl.parts)), sorted(set(nl.parts) - set(placed)))
    for ref, fp in sorted(placed.items()):
        fpid = fp.GetFPID()
        name = "%s:%s" % (fpid.GetLibNickname(), fpid.GetLibItemName())
        assert name == rows[ref]["footprint"], "%s footprint %s, bom.csv says %s" % (ref, name, rows[ref]["footprint"])
        assert fp.GetLayer() == pcbnew.F_Cu, "%s is on the back" % ref
        assert fp.GetValue() == nl.parts[ref]["value"], "%s value %r" % (ref, fp.GetValue())


def test_pad_nets_match_netlist(board, nl):
    want = nl.net_of_pad()
    seen = set()
    for fp in board.GetFootprints():
        ref = fp.GetReference()
        for pad in fp.Pads():
            if not pad.GetNumber():
                assert not pad.GetNetname(), "%s unnumbered pad on %s" % (ref, pad.GetNetname())
                continue
            key = "%s.%s" % (ref, pad.GetNumber())
            seen.add(key)
            got = netlist.bare(pad.GetNetname()) or None
            if got and got.startswith("unconnected-("):
                assert pad.GetNetname() == nl.unconnected.get(key), "%s is on %s, netlist says %s" \
                    % (key, pad.GetNetname(), nl.unconnected.get(key) or want.get(key))
                got = None
            assert got == want.get(key), "%s is on %s, netlist says %s" % (key, got, want.get(key))
    missing = sorted(set(want) - seen)
    assert not missing, "netlist pads missing from the board: %s" % missing


def test_zone_separation(board, nl):
    zone, logic = _classify(board)
    assert zone and logic, "no zone-side or no logic-side copper found"
    d, z, g = smallest_separation(zone, logic)
    import pcbnew
    print("     smallest zone-to-logic copper distance %.3f mm: %s and %s on %s"
          % (d, z.desc if z else "-", g.desc if g else "-",
             pcbnew.BOARD.GetStandardLayerName(z.layer) if z else "-"))
    assert d >= SEPARATION - 1e-6, "zone and logic copper %.3f mm apart (%s, %s)" % (d, z.desc, g.desc)


def test_zone_line(board, nl):
    """The silkscreen line falls in every relay's coil-to-contact gap, the
    label sits next to it, and no logic copper is above it."""
    y = zone_line_y(board)
    for n in ZONES:
        fp = board.FindFootprintByReference("K%d" % n)
        assert fp, "K%d missing" % n
        import pcbnew
        pads = {p.GetNumber(): _bbox(_pad_poly(p, pcbnew.F_Cu)) for p in fp.Pads()}
        contacts_bottom = max(mm(pads[k][3]) for k in RELAY_CONTACTS)
        coil_top = min(mm(pads[k][1]) for k in ("A1", "A2"))
        assert contacts_bottom < y < coil_top, "K%d: zone line at %.2f is not between contact pads (to %.2f) and coil pads (from %.2f)" \
            % (n, y, contacts_bottom, coil_top)
        assert min(mm(pads[k][1]) for k in RELAY_CONTACTS) < y, "K%d contacts are not at the top" % n
    labels = [t for t in silk_texts(board) if t.GetText() == "ZONE WIRING 24 V AC"]
    assert len(labels) >= 1, "zone line label missing"
    for t in labels:
        assert abs(mm(t.GetPosition().y) - y) < 3.0, "zone label at y %.2f is not next to the line at %.2f" % (mm(t.GetPosition().y), y)
    left, top, right, bottom = outline_box(board)
    strip = rect_poly(left, top, right, y)
    for it in logic_side_items(board):
        inter = strip.__class__(strip)
        inter.BooleanIntersection(it.poly)
        assert not inter.OutlineCount(), "logic copper %s is in the zone strip" % it.desc


def test_no_pour_in_zone_strip(board, nl):
    y = zone_line_y(board)
    left, top, right, bottom = outline_box(board)
    strip = rect_poly(left, top, right, y)
    for z in board.Zones():
        if z.GetIsRuleArea():
            continue
        inter = strip.__class__(strip)
        inter.BooleanIntersection(z.Outline())
        assert not inter.OutlineCount(), "copper zone %s reaches into the zone strip" % z.GetNetname()


def test_inside_outline(board, nl):
    import pcbnew
    ol = outline(board)
    chain = ol.COutline(0)
    worst = (float("inf"), None)
    for fp in board.GetFootprints():
        for pad in fp.Pads():
            layer = pcbnew.F_Cu if pad.IsOnLayer(pcbnew.F_Cu) else pcbnew.B_Cu
            ps = _pad_poly(pad, layer)
            if pad.GetAttribute() == pcbnew.PAD_ATTRIB_NPTH:
                ps = pcbnew.SHAPE_POLY_SET()
                pad.TransformHoleToPolygon(ps, 0, nm(0.002), pcbnew.ERROR_OUTSIDE)
            for p in _points(ps):
                assert ol.Contains(p), "pad %s is outside the outline" % _pad_name(pad)
                d = mm(math.sqrt(chain.SquaredDistance(p, True)))
                if d < worst[0]:
                    worst = (d, _pad_name(pad))
    print("     closest pad to the board edge: %s, %.3f mm" % (worst[1], worst[0]))
    assert worst[0] >= PAD_EDGE_MARGIN - 1e-6, "pad %s is %.3f mm from the edge" % (worst[1], worst[0])
    for ref in ("A1", "A2"):
        fp = board.FindFootprintByReference(ref)
        pcb, ant, d = dev_board(fp)
        ox, oy = mm(fp.GetPosition().x), mm(fp.GetPosition().y)
        pcb_ant_end = max((x - ox) * d[0] + (y - oy) * d[1] for x in (pcb[0], pcb[2]) for y in (pcb[1], pcb[3]))
        for r in (pcb, ant):
            for x in (r[0], r[2]):
                for y in (r[1], r[3]):
                    # A corner on the edge (the USB end, flush with the notch) is on the board.
                    if not ol.Contains(pcbnew.VECTOR2I(nm(x), nm(y)), -1, nm(0.01)):
                        along = (x - ox) * d[0] + (y - oy) * d[1]
                        assert along >= pcb_ant_end - 1e-6, \
                            "%s silkscreen corner (%.2f, %.2f) is off the board away from the antenna end" % (ref, x, y)


def test_dev_board_placement(board, nl):
    import pcbnew
    (notch_l, notch_r), notch_top = notch_edges(board)
    left, top, right, bottom = outline_box(board)
    ol = outline(board)
    copper = copper_items(board)
    for ref, want_dir, usb_x, last in (("A1", (-1, 0), notch_l, "J1_16"), ("A2", (1, 0), notch_r, "J1_15")):
        fp = board.FindFootprintByReference(ref)
        pcb, ant, d = dev_board(fp)
        assert d == want_dir, "%s antenna points %s, want %s" % (ref, d, want_dir)
        # Read the rotation back from the pads, not from the angle.
        x1, _ = pad_xy(fp, "J1_1")
        xn, _ = pad_xy(fp, last)
        edge = left if want_dir[0] < 0 else right
        assert abs(x1 - edge) < abs(xn - edge), "%s pin J1_1 (x %.2f) is not nearer the %s edge than %s (x %.2f)" \
            % (ref, x1, "left" if want_dir[0] < 0 else "right", last, xn)
        # Antenna overhang beyond the board edge.
        tip = ant[0] if want_dir[0] < 0 else ant[2]
        over = (left - tip) if want_dir[0] < 0 else (tip - right)
        print("     %s antenna overhangs the %s edge by %.2f mm; USB end at x %.2f, notch side at %.2f"
              % (ref, "left" if want_dir[0] < 0 else "right", over,
                 pcb[2] if want_dir[0] < 0 else pcb[0], usb_x))
        assert over > 0, "%s antenna does not overhang the board edge" % ref
        usb_end = pcb[2] if want_dir[0] < 0 else pcb[0]
        assert abs(usb_end - usb_x) <= USB_EDGE_TOL, "%s USB end at x %.2f, notch side at %.2f" % (ref, usb_end, usb_x)
        # Antenna rule area: off the board, or over bare board.
        areas = [z for z in fp.Zones() if z.GetIsRuleArea()]
        assert areas, "%s has no antenna rule area" % ref
        for z in areas:
            on_board = pcbnew.SHAPE_POLY_SET(z.Outline())
            on_board.BooleanIntersection(ol)
            if not on_board.OutlineCount():
                continue
            for it in copper:
                inter = pcbnew.SHAPE_POLY_SET(on_board)
                inter.BooleanIntersection(it.poly)
                assert not inter.OutlineCount(), "%s antenna area has copper: %s" % (ref, it.desc)
        # Nothing else's courtyard under the dev board or its antenna.
        body = rect_poly(*pcb)
        body.BooleanAdd(rect_poly(*ant))
        for other in board.GetFootprints():
            if other.GetReference() == ref:
                continue
            for layer in (pcbnew.F_CrtYd, pcbnew.B_CrtYd):
                cy = other.GetCourtyard(layer)
                if not cy.OutlineCount():
                    continue
                inter = pcbnew.SHAPE_POLY_SET(body)
                inter.BooleanIntersection(cy)
                assert not inter.OutlineCount(), "%s courtyard is under dev board %s" % (other.GetReference(), ref)
    # The notch is as deep as the bottom band: it reaches up to the dev boards' tops.
    tops = [dev_board(board.FindFootprintByReference(r))[0][1] for r in ("A1", "A2")]
    assert abs(notch_top - min(tops)) <= 1.0, "notch top at y %.2f, dev boards start at %.2f" % (notch_top, min(tops))


def test_notch_width(board, nl):
    (l, r), top = notch_edges(board)
    print("     notch %.2f mm wide, %.2f mm deep" % (r - l, outline_box(board)[3] - top))
    assert r - l >= NOTCH_MIN, "notch is %.2f mm wide" % (r - l)


def test_terminal_order(board, nl):
    left, top, right, bottom = outline_box(board)
    xs = []
    for n in ZONES:
        fp = board.FindFootprintByReference("J1%d" % n)
        x1, y1 = pad_xy(fp, "1")
        x2, y2 = pad_xy(fp, "2")
        xs.append((x1 + x2) / 2)
        assert abs(y1 - y2) < 1e-6, "J1%d pads not level" % n
        # The plug enters along the footprint's +y; it must face up, off the top edge.
        d = rotate(0, 1, fp.GetOrientationDegrees())
        assert abs(d[0]) < 1e-6 and d[1] < 0, "J1%d opening faces %s, not up" % (n, d)
        cy = _bbox(fp.GetCourtyard(__import__("pcbnew").F_CrtYd))
        assert mm(cy[1]) - top <= 1.0, "J1%d body front is %.2f mm below the top edge" % (n, mm(cy[1]) - top)
        k = board.FindFootprintByReference("K%d" % n)
        kx, _ = pad_xy(k, "11")
        assert abs(kx - xs[-1]) < 5.0, "K%d is not below J1%d" % (n, n)
    steps = [b - a for a, b in zip(xs, xs[1:])]
    assert all(s > 0 for s in steps), "J11..J17 are not left to right: %s" % xs
    assert max(steps) - min(steps) < 0.01, "terminals not evenly pitched: %s" % steps


def test_mounting_holes(board, nl):
    import pcbnew
    left, top, right, bottom = outline_box(board)
    corners = {(left + CORNER_HOLE_INSET, top + CORNER_HOLE_INSET), (right - CORNER_HOLE_INSET, top + CORNER_HOLE_INSET),
               (left + CORNER_HOLE_INSET, bottom - CORNER_HOLE_INSET), (right - CORNER_HOLE_INSET, bottom - CORNER_HOLE_INSET)}
    got = set()
    for ref in ("H1", "H2", "H3", "H4"):
        fp = board.FindFootprintByReference(ref)
        assert str(fp.GetFPID().GetLibItemName()) == "MountingHole_3.2mm_M3", "%s is not an M3 hole" % ref
        pads = list(fp.Pads())
        assert len(pads) == 1 and pads[0].GetAttribute() == pcbnew.PAD_ATTRIB_NPTH, "%s is not one non-plated hole" % ref
        got.add((round(mm(fp.GetPosition().x), 3), round(mm(fp.GetPosition().y), 3)))
    want = {(round(x, 3), round(y, 3)) for x, y in corners}
    assert got == want, "corner holes at %s, want %s" % (sorted(got), sorted(want))
    clip = part_checks_clip()
    thru = [p for fp in board.GetFootprints() for p in fp.Pads()
            if p.GetAttribute() != pcbnew.PAD_ATTRIB_NPTH and p.GetDrillSize().x > 0]
    rails = []
    worst_hole = (float("inf"), None)
    worst_body = (float("inf"), None)
    for ref in ("H5", "H6"):
        fp = board.FindFootprintByReference(ref)
        pads = sorted(fp.Pads(), key=lambda p: p.GetDrillSize().x)
        assert len(pads) == 2 and all(p.GetAttribute() == pcbnew.PAD_ATTRIB_NPTH for p in pads), "%s: want two non-plated holes" % ref
        rnd, slot = pads
        assert abs(mm(rnd.GetDrillSize().x) - clip["hole"]) < 1e-3 and abs(mm(rnd.GetDrillSize().y) - clip["hole"]) < 1e-3, "%s round hole size" % ref
        assert {round(mm(slot.GetDrillSize().x), 3), round(mm(slot.GetDrillSize().y), 3)} == {clip["slot_w"], clip["slot_l"]}, "%s slot size" % ref
        a, b = rnd.GetPosition(), slot.GetPosition()
        assert abs(mm(math.hypot(a.x - b.x, a.y - b.y)) - clip["spacing"]) < 1e-3, "%s hole spacing" % ref
        assert a.x == b.x and b.y < a.y, "%s: holes not one above the other with the slot (hook end) up" % ref
        rails.append(to_board(fp, clip["rail"], 0)[1])
        for hole in pads:
            hp = pcbnew.SHAPE_POLY_SET()
            hole.TransformHoleToPolygon(hp, 0, nm(0.002), pcbnew.ERROR_OUTSIDE)
            for p in thru + [q for f in board.GetFootprints() for q in f.Pads() if q.GetAttribute() == pcbnew.PAD_ATTRIB_SMD]:
                d = mm(poly_distance(hp, _pad_poly(p, pcbnew.F_Cu if p.IsOnLayer(pcbnew.F_Cu) else pcbnew.B_Cu)))
                if d < worst_hole[0]:
                    worst_hole = (d, "%s hole and %s" % (ref, _pad_name(p)))
        # No through-hole pad (solder joint, pin end) under the clip body on the back.
        l, t, r, bt = clip_body_box(fp, clip["body"])
        body = rect_poly(l - CLIP_BODY_MARGIN, t - CLIP_BODY_MARGIN, r + CLIP_BODY_MARGIN, bt + CLIP_BODY_MARGIN)
        for p in thru:
            d = mm(poly_distance(body, _pad_poly(p, pcbnew.B_Cu)))
            if d < worst_body[0]:
                worst_body = (d, "%s body and %s" % (ref, _pad_name(p)))
    print("     DIN clip holes: closest pad %.2f mm (%s); clip bodies: closest through-hole pad %.2f mm beyond the %.2f mm margin (%s); rail at y %s, board centre %.2f"
          % (worst_hole[0], worst_hole[1], worst_body[0], CLIP_BODY_MARGIN, worst_body[1],
             ", ".join("%.2f" % r for r in rails), (top + bottom) / 2))
    assert worst_hole[0] >= 3.0 - 1e-6, "%s only %.2f mm apart" % (worst_hole[1], worst_hole[0])
    assert worst_body[0] > 0, "through-hole pad under a DIN clip body: %s" % worst_body[1]
    assert abs(rails[0] - rails[1]) < 1e-3, "the two clips are not on one rail line: %s" % rails
    assert abs(rails[0] - (top + bottom) / 2) <= 1.0, "rail at y %.2f is not behind the board centre %.2f" % (rails[0], (top + bottom) / 2)


def test_zone_tracks(board, nl):
    import pcbnew
    for n in ZONES:
        for s in "AB":
            name = "Z%d_%s" % (n, s)
            tracks = [t for t in board.GetTracks() if netlist.bare(t.GetNetname()) == name]
            assert tracks, "%s has no tracks" % name
            for t in tracks:
                assert t.GetClass() == "PCB_TRACK", "%s has a %s" % (name, t.GetClass())
                assert t.GetLayer() == pcbnew.F_Cu, "%s has a track on %s" % (name, t.GetLayerName())
                assert mm(t.GetWidth()) >= ZONE_TRACK_WIDTH - 1e-6, "%s track %.2f mm wide" % (name, mm(t.GetWidth()))
            pads = [p for fp in board.GetFootprints() for p in fp.Pads() if netlist.bare(p.GetNetname()) == name]
            # Union-find over pads and tracks.
            parent = {}

            def find(k):
                while parent.setdefault(k, k) != k:
                    k = parent[k]
                return k

            def join(a, b):
                parent[find(a)] = find(b)
            for i, t in enumerate(tracks):
                for end in (t.GetStart(), t.GetEnd()):
                    for p in pads:
                        if p.HitTest(end):
                            join(("t", i), ("p", _pad_name(p)))
                    for j, u in enumerate(tracks):
                        if j != i and end in (u.GetStart(), u.GetEnd()):
                            join(("t", i), ("t", j))
            roots = {find(("p", _pad_name(p))) for p in pads}
            assert len(roots) == 1, "%s is not fully routed (%d separate pieces)" % (name, len(roots))


def test_power_and_middle(board, nl):
    """J1 (USB-C) and J2 on the left edge in the driver band, opening left;
    C1 within 30 mm of the relay coil row; JP1, J3, J4 above the notch."""
    import pcbnew
    left, top, right, bottom = outline_box(board)
    y_line = zone_line_y(board)
    for ref in ("J1", "J2"):
        fp = board.FindFootprintByReference(ref)
        d = rotate(0, 1, fp.GetOrientationDegrees())   # both footprints open toward their +y
        assert d[0] < 0 and abs(d[1]) < 1e-6, "%s opens toward %s, not left" % (ref, d)
        cy = [mm(v) for v in _bbox(fp.GetCourtyard(pcbnew.F_CrtYd))]
        assert cy[0] - left <= 1.0, "%s is %.2f mm in from the left edge" % (ref, cy[0] - left)
        assert cy[1] > y_line, "%s is in the zone strip" % ref
    c1 = board.FindFootprintByReference("C1")
    cx, cyy = mm(c1.GetPosition().x), mm(c1.GetPosition().y)
    coil = [pad_xy(board.FindFootprintByReference("K%d" % n), p) for n in ZONES for p in ("A1", "A2")]
    dist = min(math.hypot(cx - x, cyy - y) for x, y in coil)
    print("     C1 is %.1f mm from the nearest relay coil pad" % dist)
    assert dist <= 30.0, "C1 is %.1f mm from the relay coil row" % dist
    (nl_x, nr_x), notch_top = notch_edges(board)
    for ref in ("JP1", "J3", "J4"):
        cyb = [mm(v) for v in _bbox(board.FindFootprintByReference(ref).GetCourtyard(pcbnew.F_CrtYd))]
        assert nl_x <= cyb[0] and cyb[2] <= nr_x and cyb[3] < notch_top, "%s is not above the notch" % ref


def test_silkscreen(board, nl):
    texts = silk_texts(board)
    have = [t.GetText() for t in texts]
    for s in SILK_TEXT:
        assert s in have, "silkscreen text %r missing" % s
    for n in ZONES:
        assert have.count("Z%d" % n) >= 3, "Z%d appears %d times; want terminal, relay and zone light" % (n, have.count("Z%d" % n))
    for t in texts:
        assert t.GetTextAngleDegrees() == 0 and not t.IsMirrored(), "text %r is turned or mirrored" % t.GetText()
    import pcbnew
    j4 = board.FindFootprintByReference("J4")
    assert j4.IsDNP(), "J4 is not marked do-not-populate"
    assert j4.GetAttributes() & pcbnew.FP_EXCLUDE_FROM_POS_FILES, "J4 is in the position files"


def test_drc(board, nl):
    """kicad-cli DRC, with the schematic parity check: no violations of any
    kind and the board matches the schematic. Unrouted logic nets are
    expected before Task 5; unrouted zone nets are not."""
    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, "drc.json")
        r = subprocess.run([KICAD_CLI, "pcb", "drc", "--format", "json", "--severity-all", "--schematic-parity",
                            "-o", out, BOARD], capture_output=True, text=True)
        assert os.path.exists(out), "kicad-cli drc failed: %s" % r.stderr[-500:]
        with open(out) as f:
            rep = json.load(f)
    viol = []
    allowed = []
    for v in rep.get("violations", []):
        # The dev board outlines are drawn on the silkscreen as the boards
        # sit: the antenna end crosses the board edge and the USB end is flush
        # with the notch side. That is the placement the design asks for; the
        # board maker clips silkscreen at the edge.
        descs = [i["description"] for i in v.get("items", [])]
        if v["type"] == "silk_edge_clearance" and all(
                d.startswith("Segment on Edge.Cuts") or re.fullmatch(r"Rectangle of A[12] on F.Silkscreen", d)
                for d in descs):
            allowed.append(v)
        else:
            viol.append(v)
    kinds = {}
    for v in viol:
        kinds.setdefault(v["type"], []).append("; ".join(i["description"] for i in v.get("items", [])))
    unconnected = rep.get("unconnected_items", [])
    zone_open = [u for u in unconnected if any(re.search(r"\[/?Z\d_[AB]\]", i["description"]) for i in u.get("items", []))]
    parity = rep.get("schematic_parity", [])
    print("     DRC: %d violations %s; %d dev board outlines past the edge (allowed); %d unconnected (logic nets, routed in Task 5); %d schematic parity issues"
          % (len(viol), {k: len(v) for k, v in kinds.items()}, len(allowed), len(unconnected), len(parity)))
    assert not viol, "DRC violations: %s" % {k: v[:3] for k, v in kinds.items()}
    assert not parity, "board differs from the schematic: %s" % [p["description"] for p in parity[:3]]
    assert not zone_open, "zone nets unconnected: %s" % zone_open[:3]


TESTS = [test_all_parts_present, test_pad_nets_match_netlist, test_zone_separation, test_zone_line,
         test_no_pour_in_zone_strip, test_inside_outline, test_dev_board_placement, test_notch_width,
         test_terminal_order, test_mounting_holes, test_zone_tracks, test_power_and_middle,
         test_silkscreen, test_drc]


def main():
    try:
        import pcbnew
        assert os.path.exists(BOARD), "%s not found (run make board)" % BOARD
        board = pcbnew.LoadBoard(BOARD)
        nl = netlist.Netlist()
    except (OSError, AssertionError, ImportError) as e:
        print("FAIL board: %s" % e)
        return 1
    failed = 0
    for t in TESTS:
        try:
            t(board, nl)
            print("ok   %s" % t.__name__)
        except Exception as e:  # report every failure, not just the first
            failed += 1
            print("FAIL %s: %s" % (t.__name__, e))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
