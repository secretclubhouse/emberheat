#!/usr/bin/env python3
"""Checks on the enclosure: the board geometry file and the board model.
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


TESTS = [test_geometry_sanity, test_board_model]


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
