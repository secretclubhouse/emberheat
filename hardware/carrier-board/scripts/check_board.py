#!/usr/bin/env python3
"""Checks on the circuit board (carrier-board.kicad_pcb, written by
`make board` and routed by `make route`) against the netlist, bom.csv,
part-checks.md and the placement and routing rules of the design, ending
with KiCad's own design rule check. Run with the KiCad Python (needs pcbnew).
"""
import contextlib
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
# Mounting hardware (part-checks.md, "Mounting hardware"): no track, via or
# pour within HOLE_KEEPOUT of a hole's centre line; no pad within
# HOLE_PAD_MIN (largest hardware radius, an M3 washer 7 mm across, + 0.5).
HOLE_KEEPOUT = 4.5        # mm
HOLE_PAD_MIN = 4.0        # mm
VARISTOR_BODY_GAP = 0.5   # mm, varistor body to terminal header and relay bodies
# Board silkscreen that must be present, verbatim.
SILK_TEXT = ["ZONE WIRING 24 V AC", "rev 1", "5V ⎓ 3A USB-C", "+5V", "GND",
             "H2 3V3 — REMOVE BEFORE USING H2 USB", "1-WIRE", "3V3", "DQ",
             "IO20", "TX", "RX", "EmberHeat"]

ZONE_NET = re.compile(r"Z\d_[AB]")
RELAY_CONTACTS = {"11", "12", "14"}

# Track widths by net (mm): zone side 1.5, supply nets 1.0, signals 0.4.
POWER_NETS = {"+5V", "GND", "+3V3", "VIN", "H2_3V3"}
POWER_WIDTH = 1.0
SIGNAL_WIDTH = 0.4
ZONE_STRIP = "zone_strip"   # rule area over the zone strip (gen_board.py)
POUR_NET = "GND"
POUR_FILL_TOL = 0.001       # relative: saved fill area against a fresh refill
HAND_SOLDER_GAP = 0.5       # mm, track to a through-hole pad of another net
VBUS_PADS = ("A9", "B9")    # J1 (USB-C) VBUS pads, net VIN
VBUS_NECK_WIDTH = 0.5       # mm, track leaving them
VBUS_NECK_MAX = 1.1         # mm, longest such neck


@contextlib.contextmanager
def quiet_stderr():
    """Drop wxWidgets' harmless start-up chatter that pcbnew writes straight
    to file descriptor 2 (no wxApp assert, duplicate image handlers); pass
    every other line through."""
    noise = re.compile(r"assert \"\"traits\"\" failed|Debug: Adding duplicate image handler")
    sys.stderr.flush()
    saved = os.dup(2)
    with tempfile.TemporaryFile(mode="w+b") as buf:
        os.dup2(buf.fileno(), 2)
        try:
            yield
        finally:
            sys.stderr.flush()
            os.dup2(saved, 2)
            os.close(saved)
            buf.seek(0)
            for line in buf.read().decode(errors="replace").splitlines():
                if not noise.search(line):
                    print(line, file=sys.stderr)


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


def _hole_centre_lines(fp):
    """Each non-plated hole of a mounting footprint as its centre line (mm):
    a point for a round hole, the segment between the end-circle centres for
    a slot (a screw head can sit anywhere along it)."""
    import pcbnew
    out = []
    for p in fp.Pads():
        if p.GetAttribute() != pcbnew.PAD_ATTRIB_NPTH:
            continue
        cx, cy = mm(p.GetPosition().x), mm(p.GetPosition().y)
        dx, dy = mm(p.GetDrillSize().x), mm(p.GetDrillSize().y)
        half = abs(dx - dy) / 2
        # The slot's long axis, turned with the pad (pad frame x if dx > dy).
        ux, uy = (1.0, 0.0) if dx >= dy else (0.0, 1.0)
        rx, ry = rotate(ux, uy, p.GetOrientation().AsDegrees())
        out.append(((cx - rx * half, cy - ry * half), (cx + rx * half, cy + ry * half)))
    return out


def _distance_to_line(ps, a, b):
    """Smallest distance (mm) from a polygon set to the segment a-b (mm),
    0 if the segment touches or is inside it. Sampled every 0.01 mm."""
    import pcbnew
    n = max(1, int(math.hypot(b[0] - a[0], b[1] - a[1]) / 0.01))
    best = None
    for i in range(n + 1):
        x = a[0] + (b[0] - a[0]) * i / n
        y = a[1] + (b[1] - a[1]) * i / n
        v = pcbnew.VECTOR2I(nm(x), nm(y))
        d = 0 if ps.Contains(v) else ps.SquaredDistance(v)
        best = d if best is None or d < best else best
    return mm(math.sqrt(best))


def test_mounting_hole_keepout(board, nl):
    """No track, via or pour within HOLE_KEEPOUT of any mounting hole's
    centre line (the corner holes H1-H4 and both holes of each DIN clip,
    along the whole slot), on either layer: screw heads and washers sit
    there, and a cut in the solder mask under one must not bridge copper.
    Pads (fixed by the parts) must be at least HOLE_PAD_MIN away: the
    largest hardware radius plus 0.5 mm (part-checks.md)."""
    import pcbnew
    worst_copper = (float("inf"), None)
    worst_pad = (float("inf"), None)
    report = []
    items = copper_items(board)
    for ref in ("H1", "H2", "H3", "H4", "H5", "H6"):
        fp = board.FindFootprintByReference(ref)
        assert fp, "%s missing" % ref
        lines = _hole_centre_lines(fp)
        assert lines, "%s has no holes" % ref
        here = (float("inf"), None)
        for k, (a, b) in enumerate(lines):
            box = (nm(min(a[0], b[0]) - HOLE_KEEPOUT - 1), nm(min(a[1], b[1]) - HOLE_KEEPOUT - 1),
                   nm(max(a[0], b[0]) + HOLE_KEEPOUT + 1), nm(max(a[1], b[1]) + HOLE_KEEPOUT + 1))
            for it in items:
                if _box_gap(box, it.box) > 0:
                    continue
                d = _distance_to_line(it.poly, a, b)
                what = "%s %s on %s" % (ref, "slot" if a != b else "hole", it.desc + " " +
                                        pcbnew.BOARD.GetStandardLayerName(it.layer))
                if it.pad is not None:
                    if d < worst_pad[0]:
                        worst_pad = (d, what)
                else:
                    if d < worst_copper[0]:
                        worst_copper = (d, what)
                    if d < here[0]:
                        here = (d, it.desc)
        report.append("%s %s" % (ref, "%.2f" % here[0] if here[1] else "clear"))
    print("     mounting holes, nearest track/via/pour to a hole centre line (keepout %.1f mm): %s"
          % (HOLE_KEEPOUT, "; ".join(report)))
    print("     smallest copper-to-mounting-hole distance: %.3f mm (%s); nearest pad %.3f mm (%s, minimum %.1f)"
          % (worst_copper[0], worst_copper[1], worst_pad[0], worst_pad[1], HOLE_PAD_MIN))
    assert worst_copper[0] >= HOLE_KEEPOUT - 1e-6, "%s is %.3f mm from the hole centre line" % (worst_copper[1], worst_copper[0])
    assert worst_pad[0] >= HOLE_PAD_MIN - 1e-6, "%s is %.3f mm from the hole centre line" % (worst_pad[1], worst_pad[0])


def _fab_box(fp):
    """Board box (mm) of a footprint's F.Fab drawing: the part's body."""
    import pcbnew
    box = None
    for g in fp.GraphicalItems():
        if g.GetLayer() == pcbnew.F_Fab and g.GetClass() == "PCB_SHAPE":
            bb = g.GetBoundingBox()
            if box is None:
                box = pcbnew.BOX2I(bb.GetOrigin(), bb.GetSize())
            else:
                box.Merge(bb)
    assert box is not None, "%s has no F.Fab outline" % fp.GetReference()
    w = 0.05     # half the fab line width the box includes
    return (mm(box.GetLeft()) + w, mm(box.GetTop()) + w, mm(box.GetRight()) - w, mm(box.GetBottom()) - w)


def test_varistor_fit(board, nl):
    """Each varistor's largest body (its F.Fab outline, the datasheet's
    maximum E x D) is at least VARISTOR_BODY_GAP from the bodies of its
    terminal header and of every relay, and inside the zone strip."""
    worst = (float("inf"), None)
    y_line = zone_line_y(board)
    for n in ZONES:
        rv = board.FindFootprintByReference("RV%d" % n)
        assert str(rv.GetFPID().GetLibItemName()) == "Varistor_Littelfuse_ZA_7mm", "RV%d footprint" % n
        vb = _fab_box(rv)
        assert vb[3] < y_line, "RV%d body reaches below the zone line" % n
        for ref in ["J1%d" % m for m in ZONES] + ["K%d" % m for m in ZONES]:
            ob = _fab_box(board.FindFootprintByReference(ref))
            d = _box_gap(vb, ob)
            if d < worst[0]:
                worst = (d, "RV%d and %s" % (n, ref))
    print("     varistor bodies: closest terminal or relay body %.2f mm (%s)" % worst)
    assert worst[0] >= VARISTOR_BODY_GAP - 1e-6, "%s bodies %.2f mm apart" % (worst[1], worst[0])


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


def run_drc(pcb, parity=True):
    """kicad-cli DRC, all severities, exit code set on any violation or
    unconnected item. Returns (exit code, report dict)."""
    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, "drc.json")
        cmd = [KICAD_CLI, "pcb", "drc", "--format", "json", "--severity-all", "--exit-code-violations"]
        if parity:
            cmd.append("--schematic-parity")
        r = subprocess.run(cmd + ["-o", out, pcb], capture_output=True, text=True)
        assert os.path.exists(out), "kicad-cli drc failed: %s" % r.stderr[-500:]
        with open(out) as f:
            return r.returncode, json.load(f)


def test_drc_clean(board, nl):
    """kicad-cli DRC with the schematic parity check: no violations of any
    severity, no unconnected items, no parity problems, and kicad-cli's own
    exit code agrees. The only relaxations are the custom rules in
    carrier-board.kicad_dru (dev board outlines on the silkscreen may meet
    the board edge); nothing is excluded."""
    code, rep = run_drc(BOARD)
    viol = rep.get("violations", [])
    kinds = {}
    for v in viol:
        kinds.setdefault(v["type"], []).append("; ".join(i["description"] for i in v.get("items", [])))
    unconnected = rep.get("unconnected_items", [])
    parity = rep.get("schematic_parity", [])
    print("     DRC: %d violations %s; %d unconnected items; %d schematic parity issues; kicad-cli exit code %d"
          % (len(viol), {k: len(v) for k, v in kinds.items()}, len(unconnected), len(parity), code))
    assert not viol, "DRC violations: %s" % {k: v[:3] for k, v in kinds.items()}
    assert not unconnected, "unconnected: %s" % ["; ".join(i["description"] for i in u.get("items", []))
                                                  for u in unconnected[:3]]
    assert not parity, "board differs from the schematic: %s" % [p["description"] for p in parity[:3]]
    assert code == 0, "kicad-cli drc exit code %d" % code


def _fresh_board():
    import pcbnew
    return pcbnew.LoadBoard(BOARD)


def _fill(board):
    import pcbnew
    zones = board.Zones()
    filler = pcbnew.ZONE_FILLER(board)
    assert filler.Fill(zones), "zone fill failed"
    board.BuildConnectivity()


def test_fully_routed(board, nl):
    """Every connection made, counted by KiCad after filling the pours (on a
    fresh copy of the board, so the other tests see the saved fill)."""
    b = _fresh_board()
    _fill(b)
    n = b.GetConnectivity().GetUnconnectedCount(True)
    print("     %d unconnected connections after filling zones" % n)
    assert n == 0, "%d connections unrouted" % n


def _width_for(net):
    if ZONE_NET.fullmatch(net):
        return ZONE_TRACK_WIDTH
    return POWER_WIDTH if net in POWER_NETS else SIGNAL_WIDTH


def _vbus_neck(board, t, net, width):
    """The one allowed narrowing: VIN leaving a USB-C VBUS pad (J1 A9, B9).
    Those pads are 0.76 mm across with the CC pads 1.0 mm away, so a 1.0 mm
    track cannot reach them with clearance; a VBUS_NECK_WIDTH track no
    longer than VBUS_NECK_MAX from the pad centre can."""
    if net != "VIN" or abs(width - VBUS_NECK_WIDTH) > 1e-6 or mm(t.GetLength()) > VBUS_NECK_MAX + 1e-6:
        return False
    j1 = board.FindFootprintByReference("J1")
    centres = [p.GetPosition() for p in j1.Pads() if p.GetNumber() in VBUS_PADS]
    return t.GetStart() in centres or t.GetEnd() in centres


def test_track_widths(board, nl):
    """Every track is exactly its net's width (zone 1.5, supply 1.0, signal
    0.4 mm), which is also its net class's width in the project, except one
    short neck from each USB-C VBUS pad (see _vbus_neck); vias are at least
    the net class via. Prints the routing statistics."""
    import pcbnew
    length = {}
    vias = []
    necks = []
    for t in board.GetTracks():
        net = netlist.bare(t.GetNetname())
        assert net, "track or via with no net at (%.2f, %.2f)" % (mm(t.GetStart().x), mm(t.GetStart().y))
        nc = board.GetDesignSettings().m_NetSettings.GetEffectiveNetClass(t.GetNetname())
        if t.GetClass() == "PCB_VIA":
            vias.append(t)
            assert t.GetWidth(pcbnew.F_Cu) >= nc.GetViaDiameter() and t.GetDrillValue() >= nc.GetViaDrill(), \
                "via %s at (%.2f, %.2f) is %.2f/%.2f mm, net class %s wants %.2f/%.2f" % (
                    net, mm(t.GetStart().x), mm(t.GetStart().y), mm(t.GetWidth(pcbnew.F_Cu)), mm(t.GetDrillValue()),
                    nc.GetName(), mm(nc.GetViaDiameter()), mm(nc.GetViaDrill()))
            continue
        w = mm(t.GetWidth())
        want = _width_for(net)
        where = "%s track on %s at (%.2f, %.2f)" % (net, t.GetLayerName(), mm(t.GetStart().x), mm(t.GetStart().y))
        length[t.GetLayerName()] = length.get(t.GetLayerName(), 0.0) + mm(t.GetLength())
        if _vbus_neck(board, t, net, w):
            necks.append(t)
            continue
        assert abs(w - want) < 1e-6, "%s is %.3f mm wide, want %.2f" % (where, w, want)
        assert abs(w - mm(nc.GetTrackWidth())) < 1e-6, "%s is %.3f mm wide, net class %s says %.2f" % (
            where, w, nc.GetName(), mm(nc.GetTrackWidth()))
    assert len(necks) == len(VBUS_PADS), "%d VBUS necks, want one per pad %s" % (len(necks), VBUS_PADS)
    print("     track length: %s; %d vias; %d VBUS necks of %.1f mm at J1"
          % (", ".join("%s %.0f mm" % kv for kv in sorted(length.items())), len(vias), len(necks), VBUS_NECK_WIDTH))


def test_track_to_through_hole_pads(board, nl):
    """Hand soldering: every track is at least HAND_SOLDER_GAP (copper edge
    to copper edge) from every through-hole pad of another net, on each
    layer, so a solder bridge at a joint does not short it. Unconnected pads
    count as another net. Computed here from the geometry, independently of
    KiCad's DRC (which checks the same through carrier-board.kicad_dru)."""
    import pcbnew
    pads = []
    for fp in board.GetFootprints():
        for p in fp.Pads():
            if p.GetAttribute() != pcbnew.PAD_ATTRIB_PTH:
                continue
            for layer in (pcbnew.F_Cu, pcbnew.B_Cu):
                if p.IsOnLayer(layer):
                    ps = _pad_poly(p, layer)
                    pads.append((layer, p.GetNetCode(), ps, _bbox(ps), _pad_name(p)))
    reach = nm(HAND_SOLDER_GAP + 1.0)
    worst = (float("inf"), None, None)
    short = 0
    for t in board.GetTracks():
        if t.GetClass() == "PCB_VIA":
            continue
        layer = t.GetLayer()
        ps = pcbnew.SHAPE_POLY_SET()
        t.TransformShapeToPolygon(ps, layer, 0, nm(0.002), pcbnew.ERROR_OUTSIDE)
        box = _bbox(ps)
        for pl, net, pps, pbox, name in pads:
            if pl != layer or net == t.GetNetCode() or _box_gap(box, pbox) > reach:
                continue
            d = mm(poly_distance(ps, pps, box, pbox, reach))
            if d < HAND_SOLDER_GAP - 1e-6:
                short += 1
            if d < worst[0]:
                worst = (d, "%s track on %s at (%.2f, %.2f)-(%.2f, %.2f)" % (
                    netlist.bare(t.GetNetname()), t.GetLayerName(), mm(t.GetStart().x), mm(t.GetStart().y),
                    mm(t.GetEnd().x), mm(t.GetEnd().y)), name)
    print("     smallest track to other-net through-hole pad: %.3f mm (%s, pad %s); %d gaps under %.1f mm"
          % (worst[0], worst[1], worst[2], short, HAND_SOLDER_GAP))
    assert worst[0] >= HAND_SOLDER_GAP - 1e-6, "%s is %.3f mm from pad %s" % (worst[1], worst[0], worst[2])
    # The ground pour too (its own pads connect by thermal reliefs and are
    # the same net, so they are left out).
    pworst = (float("inf"), None)
    for z in _pours(board):
        for layer in (pcbnew.F_Cu, pcbnew.B_Cu):
            if not (z.IsOnLayer(layer) and z.HasFilledPolysForLayer(layer)):
                continue
            fill = z.GetFilledPolysList(layer)
            for pl, net, pps, pbox, name in pads:
                if pl != layer or net == z.GetNetCode():
                    continue
                near = pcbnew.SHAPE_POLY_SET(pps)
                near.Inflate(reach, pcbnew.CORNER_STRATEGY_ROUND_ALL_CORNERS, nm(0.002))
                near.BooleanIntersection(fill)
                if not near.OutlineCount():
                    continue
                d = mm(poly_distance(pps, near, pbox, _bbox(near), reach))
                if d < pworst[0]:
                    pworst = (d, "%s, pad %s" % (pcbnew.BOARD.GetStandardLayerName(layer), name))
    print("     smallest ground pour to other-net through-hole pad: %.3f mm (%s)" % pworst)
    assert pworst[0] >= HAND_SOLDER_GAP - 1e-6, "ground pour %.3f mm from %s" % pworst


def test_vias(board, nl):
    """No via under a relay body or a dev board's antenna, none on the zone
    side, none in a dev board socket row (the strip of a header row's pads,
    end to end)."""
    import pcbnew
    relays = [board.FindFootprintByReference("K%d" % n) for n in ZONES]
    rows = []
    for ref in ("A1", "A2"):
        by_y = {}
        for p in board.FindFootprintByReference(ref).Pads():
            if p.GetAttribute() == pcbnew.PAD_ATTRIB_PTH:
                by_y.setdefault(p.GetPosition().y, []).append(p)
        assert len(by_y) == 2, "%s: %d pad rows" % (ref, len(by_y))
        for row in by_y.values():
            bb = row[0].GetBoundingBox()
            for p in row[1:]:
                bb.Merge(p.GetBoundingBox())
            rows.append((ref, (bb.GetLeft(), bb.GetTop(), bb.GetRight(), bb.GetBottom())))
    where = []
    for v in board.GetTracks():
        if v.GetClass() != "PCB_VIA":
            continue
        net = netlist.bare(v.GetNetname())
        assert not ZONE_NET.fullmatch(net), "via on zone net %s" % net
        p = v.GetPosition()
        where.append("%s (%.1f, %.1f)" % (net, mm(p.x), mm(p.y)))
        ps = pcbnew.SHAPE_POLY_SET()
        v.TransformShapeToPolygon(ps, pcbnew.F_Cu, 0, nm(0.002), pcbnew.ERROR_OUTSIDE)
        for k in relays:
            inter = pcbnew.SHAPE_POLY_SET(k.GetCourtyard(pcbnew.F_CrtYd))
            inter.BooleanIntersection(ps)
            assert not inter.OutlineCount(), "via %s at (%.2f, %.2f) is under relay %s" % (
                net, mm(p.x), mm(p.y), k.GetReference())
        for ref, box in rows:
            assert _box_gap(_bbox(ps), box) > 0, "via %s at (%.2f, %.2f) is in a socket row of %s" % (
                net, mm(p.x), mm(p.y), ref)
    print("     vias: %s" % ("; ".join(where) or "none"))


def _pours(board):
    return [z for z in board.Zones() if not z.GetIsRuleArea()]


def test_ground_pour(board, nl):
    """A GND pour on each copper layer, logic side only (outline below the
    zone line), thermal reliefs on through-hole pads for hand soldering,
    and the fill saved in the file is the fill KiCad makes now (not stale)."""
    import pcbnew
    pours = _pours(board)
    y = zone_line_y(board)
    layers = {}
    for z in pours:
        assert netlist.bare(z.GetNetname()) == POUR_NET, "pour on net %s" % z.GetNetname()
        assert z.GetPadConnection() in (pcbnew.ZONE_CONNECTION_THERMAL, pcbnew.ZONE_CONNECTION_THT_THERMAL), \
            "%s pour connects pads solid" % z.GetLayerName()
        assert mm(z.Outline().BBox().GetTop()) >= y - 1e-6, "pour reaches above the zone line"
        for layer in (pcbnew.F_Cu, pcbnew.B_Cu):
            if z.IsOnLayer(layer):
                layers[layer] = z
    assert set(layers) == {pcbnew.F_Cu, pcbnew.B_Cu}, "GND pour missing on %s" % sorted(
        pcbnew.BOARD.GetStandardLayerName(l) for l in {pcbnew.F_Cu, pcbnew.B_Cu} - set(layers))
    for fp in board.GetFootprints():
        for pad in fp.Pads():
            if netlist.bare(pad.GetNetname()) == POUR_NET and pad.GetAttribute() == pcbnew.PAD_ATTRIB_PTH:
                assert pad.GetLocalZoneConnection() in (pcbnew.ZONE_CONNECTION_INHERITED, pcbnew.ZONE_CONNECTION_THERMAL), \
                    "pad %s overrides the thermal relief" % _pad_name(pad)
                assert fp.GetLocalZoneConnection() in (pcbnew.ZONE_CONNECTION_INHERITED, pcbnew.ZONE_CONNECTION_THERMAL), \
                    "footprint %s overrides the thermal relief" % fp.GetReference()
    names = [z.GetZoneName() for z in board.Zones() if z.GetIsRuleArea()]
    dup = sorted({n for n in names if names.count(n) > 1})
    assert not dup, "rule areas named more than once: %s" % dup
    for z in board.Zones():
        if not z.GetIsRuleArea() or z.GetZoneName() == ZONE_STRIP:
            continue
        if z.GetZoneName().startswith("hole_keepout_"):
            # gen_board.py's keepouts round the mounting holes: no track,
            # via or pour, on both layers (test_mounting_hole_keepout).
            assert z.GetDoNotAllowTracks() and z.GetDoNotAllowVias() and z.GetDoNotAllowCopperPour() \
                and z.IsOnLayer(pcbnew.F_Cu) and z.IsOnLayer(pcbnew.B_Cu), "%s does not keep copper out" % z.GetZoneName()
            continue
        # route.py's pour keepouts over fill islands that would connect a
        # pad to nothing (see fix_thermals there): pour only, one layer.
        assert z.GetZoneName().startswith("no_pour_") and z.GetDoNotAllowCopperPour() \
            and not (z.GetDoNotAllowTracks() or z.GetDoNotAllowVias() or z.GetDoNotAllowPads()
                     or z.GetDoNotAllowFootprints()) and z.GetLayerSet().CuStack().size() == 1, \
            "unexpected rule area %r" % z.GetZoneName()
    saved = {}
    for layer, z in layers.items():
        assert z.HasFilledPolysForLayer(layer) and z.GetFilledPolysList(layer).OutlineCount(), \
            "%s pour is not filled" % pcbnew.BOARD.GetStandardLayerName(layer)
        saved[layer] = z.GetFilledPolysList(layer).Area()
    fresh = _fresh_board()
    _fill(fresh)
    for z in _pours(fresh):
        for layer in saved:
            if z.IsOnLayer(layer):
                a = z.GetFilledPolysList(layer).Area()
                assert abs(a - saved[layer]) <= POUR_FILL_TOL * saved[layer], \
                    "%s fill in the file (%.0f mm2) differs from a refill (%.0f mm2)" % (
                        pcbnew.BOARD.GetStandardLayerName(layer), saved[layer] / 1e12, a / 1e12)
    print("     GND pour fill: %s" % ", ".join("%s %.0f mm2" % (pcbnew.BOARD.GetStandardLayerName(l), a / 1e12)
                                             for l, a in sorted(saved.items())))


def test_zone_strip_rule(board, nl):
    """The zone strip is a named rule area from the top edge to the zone
    line across the whole board, and the custom rule that keeps logic copper
    out of it fires: a logic track and via put in the strip of a copy of the
    board are DRC violations, while the zone tracks there are not."""
    import pcbnew
    areas = [z for z in board.Zones() if z.GetIsRuleArea() and z.GetZoneName() == ZONE_STRIP]
    assert len(areas) == 1, "%d rule areas named %s" % (len(areas), ZONE_STRIP)
    area = areas[0]
    assert area.IsOnLayer(pcbnew.F_Cu) and area.IsOnLayer(pcbnew.B_Cu), "zone strip area is not on both copper layers"
    left, top, right, bottom = outline_box(board)
    y = zone_line_y(board)
    strip = rect_poly(left, top, right, y)
    missing = pcbnew.SHAPE_POLY_SET(strip)
    missing.BooleanSubtract(area.Outline())
    assert not missing.OutlineCount() or missing.Area() < 1e6, "zone strip area does not cover the strip"
    beyond = pcbnew.SHAPE_POLY_SET(area.Outline())
    beyond.BooleanSubtract(strip)
    assert not beyond.OutlineCount() or beyond.Area() < 1e6, "zone strip area reaches below the zone line"
    with tempfile.TemporaryDirectory() as tmp:
        for f in ("carrier-board.kicad_pro", "carrier-board.kicad_dru"):
            with open(os.path.join(ROOT, f)) as src, open(os.path.join(tmp, f), "w") as dst:
                dst.write(src.read())
        b = _fresh_board()
        gnd = b.FindNet(nl.full_names[POUR_NET])
        # Bare board in the strip, far (> 3 mm) from any zone copper.
        x0, y0 = 2.0, y - 8.0
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(pcbnew.VECTOR2I(nm(x0), nm(y0)))
        t.SetEnd(pcbnew.VECTOR2I(nm(x0 + 10), nm(y0)))
        t.SetWidth(nm(POWER_WIDTH))
        t.SetLayer(pcbnew.B_Cu)
        t.SetNet(gnd)
        b.Add(t)
        v = pcbnew.PCB_VIA(b)
        v.SetPosition(pcbnew.VECTOR2I(nm(x0 + 15), nm(y0)))
        v.SetWidth(nm(0.8))
        v.SetDrill(nm(0.4))
        v.SetNet(gnd)
        b.Add(v)
        pcb = os.path.join(tmp, "carrier-board.kicad_pcb")
        pcbnew.SaveBoard(pcb, b)
        code, rep = run_drc(pcb, parity=False)
    hits = [v for v in rep.get("violations", []) if v["type"] == "items_not_allowed"]
    descs = ["; ".join(i["description"] for i in v.get("items", [])) for v in hits]
    assert any(d.startswith("Track [GND]") for d in descs) and any(d.startswith("Via [GND]") for d in descs), \
        "a GND track and via in the zone strip are not DRC violations: %s" % descs
    assert not any(re.search(r"\[/?Z\d_[AB]\]", d) for d in descs), "zone tracks flagged in the strip: %s" % descs
    print("     zone strip rule: test track and via in the strip flagged (%d items_not_allowed)" % len(hits))


def _regenerated():
    """The board as gen_board.py draws it now (placed, zone side routed),
    saved to a temporary file and read back as s-expressions; also the
    in-memory board."""
    import pcbnew
    import gen_board
    import sexp
    with tempfile.TemporaryDirectory() as tmp:
        # The same project as `make board` sees: loading it uses up the same
        # ids, so the regenerated tracks get the same uuids.
        for f in ("carrier-board.kicad_pro", "carrier-board.kicad_dru"):
            with open(os.path.join(ROOT, f)) as src, open(os.path.join(tmp, f), "w") as dst:
                dst.write(src.read())
        pcb = os.path.join(tmp, "carrier-board.kicad_pcb")
        with quiet_stderr():
            b = gen_board.build(netlist.Netlist(), pcb)
            pcbnew.SaveBoard(pcb, b)
        with open(pcb) as f:
            return b, sexp.loads(f.read())


def _zone_segments(tree):
    """dumps() of every zone-net segment, the net number replaced by its
    name, in file order."""
    import sexp
    names = {}
    for node in tree:
        if isinstance(node, list) and node and node[0] == "net" and len(node) == 3:
            names[node[1]] = netlist.bare(node[2])
    out = []
    for node in tree:
        if isinstance(node, list) and node and node[0] in ("segment", "arc", "via"):
            net = [c for c in node if isinstance(c, list) and c and c[0] == "net"]
            name = names.get(net[0][1]) if net else None
            if name and ZONE_NET.fullmatch(name):
                node = [["net", sexp.Q(name)] if (isinstance(c, list) and c and c[0] == "net") else c for c in node]
                out.append(sexp.dumps(node))
    return out


_REGEN = []


def regenerated():
    if not _REGEN:
        _REGEN.append(_regenerated())
    return _REGEN[0]


def test_zone_tracks_unchanged(board, nl):
    """The zone-side tracks in the routed board are, text for text (uuid,
    lock, width, ends), the ones gen_board.py draws."""
    import sexp
    _, want_tree = regenerated()
    with open(BOARD) as f:
        have_tree = sexp.loads(f.read())
    want, have = _zone_segments(want_tree), _zone_segments(have_tree)
    assert want, "gen_board.py drew no zone tracks"
    extra = sorted(set(have) - set(want))
    lost = sorted(set(want) - set(have))
    assert not extra and not lost and len(have) == len(want), \
        "zone tracks changed: %d added, %d lost, e.g. %s" % (len(extra), len(lost), (extra or lost)[:1])
    print("     %d zone track segments identical to gen_board.py's" % len(have))


def test_footprints_unmoved(board, nl):
    """Routing moved no part: position, rotation and side as gen_board.py
    places them."""
    import pcbnew
    want, _ = regenerated()
    for fp in want.GetFootprints():
        ref = fp.GetReference()
        got = board.FindFootprintByReference(ref)
        assert got, "%s missing" % ref
        assert got.GetPosition() == fp.GetPosition() and got.GetOrientationDegrees() == fp.GetOrientationDegrees() \
            and got.GetLayer() == fp.GetLayer(), "%s moved: (%.3f, %.3f) %g, gen_board.py has (%.3f, %.3f) %g" % (
                ref, mm(got.GetPosition().x), mm(got.GetPosition().y), got.GetOrientationDegrees(),
                mm(fp.GetPosition().x), mm(fp.GetPosition().y), fp.GetOrientationDegrees())


FAB = os.path.join(ROOT, "fab")
FAB_FILES = ["carrier-board-F_Cu.gtl", "carrier-board-B_Cu.gbl", "carrier-board-F_Mask.gts",
             "carrier-board-B_Mask.gbs", "carrier-board-F_Silkscreen.gto", "carrier-board-B_Silkscreen.gbo",
             "carrier-board-Edge_Cuts.gm1", "carrier-board-PTH.drl", "carrier-board-NPTH.drl",
             "carrier-board-gerbers.zip", "print-template.pdf", "schematic.pdf"]
FAB_ZIP_MEMBERS = [n for n in FAB_FILES if n.endswith((".gtl", ".gbl", ".gts", ".gbs", ".gto", ".gbo", ".gm1", ".drl"))]


def _drill_hits(path):
    """Holes in an Excellon file: one per coordinate line after a tool is
    selected; a slot (G85 line) is one hole."""
    n, in_body = 0, False
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line == "%":
                in_body = True
            elif in_body and re.match(r"X-?[\d.]+Y-?[\d.]+", line):
                n += 1
    return n


def test_fab_outputs(board, nl):
    """fab/ (make fab) has every expected file, is from this board (the
    board's SHA-256 is stored beside it, so a fresh clone cannot look stale;
    likewise for the print template), the zip holds exactly the Gerber and
    drill files and the same bytes as the loose files, and the drill files hold exactly the board's holes."""
    import hashlib
    assert os.path.isdir(FAB), "fab/ missing (run make fab)"
    missing = [n for n in FAB_FILES + ["source.sha256", "print-template.source.sha256"] if not os.path.isfile(os.path.join(FAB, n))]
    assert not missing, "fab/ lacks %s (run make fab)" % ", ".join(missing)
    empty = [n for n in FAB_FILES if os.path.getsize(os.path.join(FAB, n)) == 0]
    assert not empty, "empty: %s" % ", ".join(empty)
    with open(BOARD, "rb") as f:
        digest = hashlib.sha256(f.read()).hexdigest()
    with open(os.path.join(FAB, "source.sha256")) as f:
        stored = f.read().split()[0]
    assert stored == digest, "fab/ was made from a different carrier-board.kicad_pcb (run make fab)"
    with open(os.path.join(FAB, "print-template.source.sha256")) as f:
        stored = f.read().split()[0]
    assert stored == digest, "print-template.pdf was plotted from a different carrier-board.kicad_pcb (run make fab)"
    import zipfile
    with zipfile.ZipFile(os.path.join(FAB, "carrier-board-gerbers.zip")) as z:
        members = sorted(z.namelist())
        assert members == sorted(FAB_ZIP_MEMBERS), "gerbers.zip holds %s, want %s" % (members, sorted(FAB_ZIP_MEMBERS))
        for n in members:
            with open(os.path.join(FAB, n), "rb") as f:
                assert z.read(n) == f.read(), "gerbers.zip member %s differs from fab/%s" % (n, n)
    want = sum(1 for fp in board.GetFootprints() for p in fp.Pads() if p.GetDrillSizeX() > 0)
    want += sum(1 for t in board.GetTracks() if t.GetClass() == "PCB_VIA")
    have = _drill_hits(os.path.join(FAB, "carrier-board-PTH.drl")) + \
        _drill_hits(os.path.join(FAB, "carrier-board-NPTH.drl"))
    assert have == want, "drill files hold %d holes, board has %d" % (have, want)
    print("     %d holes in the drill files, as on the board" % have)


def test_geometry_matches_board(board, nl):
    """enclosure/board-geometry.json is what scripts/export_board_geometry.py
    reads from this board now, so the enclosure cannot be built from a stale
    copy (run make enclosure-geometry)."""
    import export_board_geometry
    path = os.path.join(ROOT, "enclosure", "board-geometry.json")
    assert os.path.isfile(path), "enclosure/board-geometry.json missing (run make enclosure-geometry)"
    with open(path) as f:
        committed = json.load(f)
    fresh = json.loads(json.dumps(export_board_geometry.build_geometry(board)))
    assert fresh == committed, "enclosure/board-geometry.json differs from the board (run make enclosure-geometry)"


TESTS = [test_all_parts_present, test_pad_nets_match_netlist, test_zone_separation, test_zone_line,
         test_no_pour_in_zone_strip, test_inside_outline, test_dev_board_placement, test_notch_width,
         test_terminal_order, test_mounting_holes, test_mounting_hole_keepout, test_varistor_fit, test_zone_tracks, test_power_and_middle,
         test_silkscreen, test_zone_strip_rule, test_zone_tracks_unchanged, test_footprints_unmoved,
         test_fully_routed, test_track_widths, test_track_to_through_hole_pads, test_vias, test_ground_pour,
         test_drc_clean, test_fab_outputs, test_geometry_matches_board]


def main():
    try:
        import pcbnew
        assert os.path.exists(BOARD), "%s not found (run make board)" % BOARD
        with quiet_stderr():
            board = pcbnew.LoadBoard(BOARD)
        nl = netlist.Netlist()
    except (OSError, AssertionError, ImportError) as e:
        print("FAIL board: %s" % e)
        return 1
    failed = 0
    for t in TESTS:
        try:
            with quiet_stderr():
                t(board, nl)
            print("ok   %s" % t.__name__)
        except Exception as e:  # report every failure, not just the first
            failed += 1
            print("FAIL %s: %s" % (t.__name__, e))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
