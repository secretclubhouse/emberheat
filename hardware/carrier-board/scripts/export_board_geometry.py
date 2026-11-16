#!/usr/bin/env python3
"""Write enclosure/board-geometry.json: the board facts the enclosure is built
from, read from carrier-board.kicad_pcb. Run with the KiCad Python (needs
pcbnew): make enclosure-geometry. check_board.py re-runs build_geometry() and
fails if the committed JSON differs, so the file cannot go stale.

Frame: mm, x from the board's left edge, y down from its top edge."""
import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
BOARD = os.path.join(ROOT, "carrier-board.kicad_pcb")
OUT = os.path.join(ROOT, "enclosure", "board-geometry.json")

THICKNESS = 1.6
# part-checks.md "Orientation / What the drawings add": the module antenna
# overhangs the dev board PCB by this much, over this width. The PCB's
# antenna-end edge is a fixed distance from the footprint origin (gen_board.py
# C6_ORIGIN_X / H2_ORIGIN_X).
ANTENNA = {
    "A1": {"side": "left", "overhang": 6.35, "width": 18.00, "edge_from_origin": -20.625},
    "A2": {"side": "right", "overhang": 5.40, "width": 13.20, "edge_from_origin": 19.355},
}
PCB_LENGTH = {"A1": 51.80, "A2": 48.26}   # dev board PCB, antenna end to USB end
COURTYARD_SLACK = 0.5     # the footprint courtyard must hold the antenna to within this


def r(v):
    return round(v, 3)


def mm_box(box):
    import pcbnew
    return [r(pcbnew.ToMM(box.GetLeft())), r(pcbnew.ToMM(box.GetTop())),
            r(pcbnew.ToMM(box.GetRight())), r(pcbnew.ToMM(box.GetBottom()))]


def courtyard(fp):
    import pcbnew
    cy = fp.GetCourtyard(pcbnew.F_CrtYd)
    assert cy.OutlineCount(), "%s has no courtyard" % fp.GetReference()
    return mm_box(cy.BBox())


def fab_box(fp):
    """Bounding box of the footprint's F.Fab drawing (the part's body outline)."""
    import pcbnew
    boxes = [mm_box(g.GetBoundingBox()) for g in fp.GraphicalItems() if g.GetLayer() == pcbnew.F_Fab]
    assert boxes, "%s has no F.Fab outline" % fp.GetReference()
    return [min(b[0] for b in boxes), min(b[1] for b in boxes),
            max(b[2] for b in boxes), max(b[3] for b in boxes)]


def pad_mm(fp, number):
    import pcbnew
    for p in fp.Pads():
        if p.GetNumber() == number:
            pos = p.GetPosition()
            return pcbnew.ToMM(pos.x), pcbnew.ToMM(pos.y)
    raise AssertionError("%s has no pad %s" % (fp.GetReference(), number))


def lens_centre(fp):
    """Midpoint of the LED's two pads (the lens sits between them; the
    footprint origin is pad 1)."""
    (x1, y1), (x2, y2) = pad_mm(fp, "1"), pad_mm(fp, "2")
    return r((x1 + x2) / 2), r((y1 + y2) / 2)


def outline(board):
    """The Edge.Cuts lines chained into one closed polygon, as [x, y]."""
    import pcbnew
    segs = []
    for d in board.GetDrawings():
        if d.GetLayer() == pcbnew.Edge_Cuts:
            assert d.GetShapeStr() == "Line", "outline holds a %s, only lines are handled" % d.GetShapeStr()
            segs.append(((r(pcbnew.ToMM(d.GetStart().x)), r(pcbnew.ToMM(d.GetStart().y))),
                         (r(pcbnew.ToMM(d.GetEnd().x)), r(pcbnew.ToMM(d.GetEnd().y)))))
    assert segs, "no Edge.Cuts outline"
    # Start at the top-left corner and walk, so the list is the same every run.
    start = min(p for s in segs for p in s)
    pts, cur, used = [start], start, set()
    while True:
        nxt = None
        for i, (a, b) in enumerate(segs):
            if i in used:
                continue
            if a == cur:
                nxt = (i, b)
            elif b == cur:
                nxt = (i, a)
            if nxt:
                break
        if nxt is None:
            break
        used.add(nxt[0])
        cur = nxt[1]
        if cur == start:
            break
        pts.append(cur)
    assert cur == start and len(used) == len(segs), "Edge.Cuts is not one closed loop"
    return [list(p) for p in pts]


def dev_board(fp, ref):
    """PCB-and-sockets box (courtyard without the antenna overhang) and the
    antenna box, both built from the documented overhang, since the footprint
    courtyard only just holds the antenna."""
    import pcbnew
    a = ANTENNA[ref]
    x0, y0, x1, y1 = courtyard(fp)
    ox = pcbnew.ToMM(fp.GetPosition().x)
    edge = r(ox + a["edge_from_origin"])
    # The origin is the centre of the pin rows, so the antenna centre line is
    # the courtyard's middle in y.
    ymid = r((y0 + y1) / 2)
    half = a["width"] / 2
    if a["side"] == "left":
        tip = r(edge - a["overhang"])
        pcb_far = r(edge + PCB_LENGTH[ref])
        ant = [tip, r(ymid - half), edge, r(ymid + half)]
        body = [edge, y0, max(x1, pcb_far), y1]
        assert abs(x0 - tip) <= COURTYARD_SLACK, "%s courtyard %.2f is not at the antenna tip %.2f" % (ref, x0, tip)
    else:
        tip = r(edge + a["overhang"])
        pcb_far = r(edge - PCB_LENGTH[ref])
        ant = [edge, r(ymid - half), tip, r(ymid + half)]
        body = [min(x0, pcb_far), y0, edge, y1]
        assert abs(x1 - tip) <= COURTYARD_SLACK, "%s courtyard %.2f is not at the antenna tip %.2f" % (ref, x1, tip)
    return {"ref": ref, "bbox": [r(v) for v in body], "antenna_bbox": [r(v) for v in ant]}


def build_geometry(board):
    """The geometry dict for a loaded pcbnew board."""
    import pcbnew
    with open(BOARD, "rb") as f:
        digest = hashlib.sha256(f.read()).hexdigest()
    fps = {fp.GetReference(): fp for fp in board.GetFootprints()}

    corner = []
    for ref in ("H1", "H2", "H3", "H4"):
        fp = fps[ref]
        pos = fp.GetPosition()
        pad = list(fp.Pads())[0]
        corner.append({"ref": ref, "x": r(pcbnew.ToMM(pos.x)), "y": r(pcbnew.ToMM(pos.y)),
                       "drill": r(pcbnew.ToMM(pad.GetDrillSizeX()))})

    terminals = []
    for n in range(1, 8):
        ref = "J1%d" % n
        b = fab_box(fps[ref])
        # Body centre x; the plug mouth is the body outline's edge nearest the board edge.
        terminals.append({"ref": ref, "x": r((b[0] + b[2]) / 2), "mouth_y": b[1], "width": r(b[2] - b[0])})

    j1 = fps["J1"]
    jb = fab_box(j1)
    usb_c = {"x_mouth": jb[0], "y": r((jb[1] + jb[3]) / 2), "width": r(jb[3] - jb[1])}

    zone_lights = []
    for n in range(1, 8):
        x, y = lens_centre(fps["D1%d" % n])
        zone_lights.append({"ref": "D1%d" % n, "x": x, "y": y})
    px, py = lens_centre(fps["D8"])

    parts = [{"ref": ref, "bbox": courtyard(fp)} for ref, fp in sorted(fps.items())]

    return {
        "pcb_sha256": digest,
        "outline": outline(board),
        "thickness": THICKNESS,
        "corner_holes": corner,
        "zone_terminals": terminals,
        "usb_c": usb_c,
        "screw_terminal": {"bbox": courtyard(fps["J2"])},
        "zone_lights": zone_lights,
        "power_light": {"x": px, "y": py},
        "dev_boards": [dev_board(fps["A1"], "A1"), dev_board(fps["A2"], "A2")],
        "parts": parts,
    }


def main():
    import pcbnew
    board = pcbnew.LoadBoard(BOARD)
    geom = build_geometry(board)
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as f:
        json.dump(geom, f, indent=1)
        f.write("\n")
    print("wrote %s (%d parts)" % (os.path.relpath(OUT, ROOT), len(geom["parts"])))
    return 0


if __name__ == "__main__":
    sys.exit(main())
