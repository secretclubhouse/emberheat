"""The populated board as boxes, for clearance checks and for the pictures.
Frame: mm, x from the board's left edge, y down from its top edge, z up from
the board's top face (z = 0). All geometry comes from board-geometry.json."""
import json
import os

import cadquery as cq

HERE = os.path.dirname(os.path.abspath(__file__))
GEOMETRY = os.path.join(HERE, "board-geometry.json")

# Heights above the board's top face (design, "Constraints from the board").
DEFAULT_HEIGHT = 12.0
DEV_UNDERSIDE = 11.0
DEV_TOP = 17.0
ANTENNA_Z = (11.0, 15.0)
PLUG_WIDTH = 10.2
PLUG_HEIGHT = 15.0
PLUG_OUT = 15.0        # plug body runs this far past the board's top edge
USB_BODY_W = 13.0      # moulded plug body, across (y)
USB_BODY_H = 8.0       # and up from the board face (z)
USB_OUT = 30.0         # plug and cable run this far past the left edge


def load_geometry(path=GEOMETRY):
    with open(path) as f:
        return json.load(f)


def part_height(ref):
    """Height above the board's top face by reference."""
    if ref.startswith("RV"):
        return 12.0
    if ref == "C1":
        return 11.5
    if ref == "J2":
        return 10.0
    if ref in ("J11", "J12", "J13", "J14", "J15", "J16", "J17"):
        return 15.0
    if ref.startswith("K"):
        return 15.0
    if ref.startswith("F"):
        return 14.0
    if ref in ("A1", "A2"):
        return DEV_TOP
    return DEFAULT_HEIGHT


def box(x0, y0, x1, y1, z0, z1):
    """A solid from two corners and a z range."""
    return (cq.Workplane("XY").box(x1 - x0, y1 - y0, z1 - z0, centered=False)
            .translate((x0, y0, z0)))


def board_solids(geom, p):
    """Named solids: board, every part ref, antenna_A1/A2, plug_J11..J17 and
    usb_plug. p (Params) is accepted so later parts share one call shape."""
    solids = {}
    xs = [pt[0] for pt in geom["outline"]]
    ys = [pt[1] for pt in geom["outline"]]
    outline = cq.Workplane("XY").polyline([tuple(pt) for pt in geom["outline"]]).close()
    solids["board"] = outline.extrude(-geom["thickness"])
    assert min(xs) == 0 and min(ys) == 0

    dev = {d["ref"]: d for d in geom["dev_boards"]}
    for part in geom["parts"]:
        ref, bb = part["ref"], part["bbox"]
        if ref in dev:
            # Module only; the antenna overhang is its own solid.
            bb = dev[ref]["bbox"]
            solids[ref] = box(*bb, DEV_UNDERSIDE, part_height(ref))
        else:
            solids[ref] = box(*bb, 0, part_height(ref))
    for ref, d in dev.items():
        solids["antenna_" + ref] = box(*d["antenna_bbox"], *ANTENNA_Z)

    for t in geom["zone_terminals"]:
        solids["plug_" + t["ref"]] = box(t["x"] - PLUG_WIDTH / 2, -PLUG_OUT,
                                         t["x"] + PLUG_WIDTH / 2, t["mouth_y"], 0, PLUG_HEIGHT)
    u = geom["usb_c"]
    solids["usb_plug"] = box(-USB_OUT, u["y"] - USB_BODY_W / 2, u["x_mouth"],
                             u["y"] + USB_BODY_W / 2, 0, USB_BODY_H)
    return solids
