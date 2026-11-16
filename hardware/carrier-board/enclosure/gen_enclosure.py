"""The enclosure's printed parts, built with CadQuery.

Frame: mm, x from the board's left edge, y down from its top edge, z up from
the board's top face (z = 0), as in board_model.py. Board positions come from
board-geometry.json; sizes come from params.py.

Handedness: this frame (x right, y down, z toward the viewer) is a mirror of
the physical world. A solid built here is the mirror image of the part to
print, so the exports must mirror y (y -> -y) before writing files. The
lettering on the back is drawn so that it reads correctly after that mirror.

The base is a floor plate the full outside size with, on top, a locating lip
just inside the cover's walls, four magnet pads in the lip line, and four
standoffs; on the back, the two DIN clips' screw holes and the HOOK / UP
markings. The cover's walls stand on the floor's rim (z_split); its magnet
pads come down onto the base's pads (z_magnet). Nothing of the base rises
above z_magnet except the standoffs, so the cover needs nothing under the
board."""
import functools
from dataclasses import dataclass

import cadquery as cq

from board_model import box

# Hammond 1427DINCLIP, in the clip's own frame: latch-end (round) hole at 0,
# slot hole at +29.75 toward the hook end (part-checks.md, "DIN rail clip").
CLIP_BODY_LATCH = 4.50     # body runs this far past the round hole, latch end
CLIP_BODY_HOOK = 40.90     # and this far toward the hook end
CLIP_BODY_W = 10.0
CLIP_RAIL = 16.14          # rail centre line, from the round hole toward the hook


@dataclass(frozen=True)
class Frame:
    """Shared layout of base and cover. Rectangles are (x0, y0, x1, y1) in
    board coordinates; z levels are absolute.

    inside / outside: the cover's inside and outside faces in plan (the base
    floor is the outside rectangle). inside_r / outside_r: their corner radii
    (walls stay an even thickness round the corners). lip_outer / lip_inner:
    the lip's faces in plan, corner radii inside_r - lip_clear and that minus
    lip_t. lip_breaks: where the lip stops for the cover's openings,
    ("top", x0, x1) or ("left", y0, y1). notch: the board's bottom notch.
    magnet_pad: the side of each square magnet pad."""
    board: tuple
    notch: tuple
    inside: tuple
    outside: tuple
    inside_r: float
    outside_r: float
    lip_outer: tuple
    lip_inner: tuple
    lip_breaks: tuple
    magnet_pad: float
    z_bottom: float        # floor underside, on the bed
    z_floor_top: float     # floor top; standoffs and lip stand on it
    z_split: float         # where the cover's walls sit on the base
    z_lip_top: float
    z_magnet: float        # base pad tops: the base's magnet pockets open here
    z_face_inside: float   # underside of the cover's face
    z_top: float           # outside of the cover's face


def board_notch(geom):
    """(x0, y0, x1, y1) of the notch in the board's bottom edge: the two
    outline points strictly inside the board's bounding box."""
    xs = [pt[0] for pt in geom["outline"]]
    ys = [pt[1] for pt in geom["outline"]]
    inner = [pt for pt in geom["outline"]
             if min(xs) < pt[0] < max(xs) and min(ys) < pt[1] < max(ys)]
    assert len(inner) == 2 and inner[0][1] == inner[1][1], "outline has no single bottom notch: %s" % inner
    x0, x1 = sorted(pt[0] for pt in inner)
    return (x0, inner[0][1], x1, max(ys))


def frame(geom, p):
    xs = [pt[0] for pt in geom["outline"]]
    ys = [pt[1] for pt in geom["outline"]]
    board = (min(xs), min(ys), max(xs), max(ys))
    inside = (board[0] - p.gap_side, board[1] - p.gap_top, board[2] + p.gap_side, board[3] + p.gap_bottom)
    assert abs(inside[2] - inside[0] - p.inside_w) < 1e-6 and abs(inside[3] - inside[1] - p.inside_h) < 1e-6, \
        "gaps round the board do not give the %.0f x %.0f inside" % (p.inside_w, p.inside_h)
    outside = grow(inside, p.wall)
    inside_r = p.outside_edge_radius - p.wall
    lip_outer = grow(inside, -p.lip_clear)
    lip_inner = grow(lip_outer, -p.lip_t)

    breaks = [("top", t["x"] - p.top_notch_w / 2, t["x"] + p.top_notch_w / 2) for t in geom["zone_terminals"]]
    breaks.append(("left", p.left_notch_y0, p.left_notch_y1))

    z_floor_top = -(geom["thickness"] + p.standoff_height)
    z_lip_top = z_floor_top + p.lip_height
    return Frame(
        board=board, notch=board_notch(geom), inside=inside, outside=outside,
        inside_r=inside_r, outside_r=p.outside_edge_radius,
        lip_outer=lip_outer, lip_inner=lip_inner, lip_breaks=tuple(breaks),
        magnet_pad=p.magnet_d + p.fit_clearance + 2 * p.magnet_pad_wall,
        z_bottom=z_floor_top - p.floor, z_floor_top=z_floor_top, z_split=z_floor_top,
        z_lip_top=z_lip_top, z_magnet=z_lip_top,
        z_face_inside=p.clear_height, z_top=p.clear_height + p.cover_face)


def grow(r, d):
    """Rectangle r grown by d on every side (shrunk if d < 0)."""
    return (r[0] - d, r[1] - d, r[2] + d, r[3] + d)


def magnet_pads(geom, p):
    """Plan rectangles of the four base magnet pads, outer faces on the lip's
    outer face. Two in the side gaps beside the board's top corners, just below
    where the inside corner's round ends; two against the bottom wall inside
    the board's notch, at its quarter points. All four are off the board, so
    the cover's matching pads come straight down beside it."""
    f = frame(geom, p)
    s = f.magnet_pad
    lx0, ly0, lx1, ly1 = f.lip_outer
    y_top = f.inside[1] + f.inside_r          # below the inside corner's round
    nx0, _, nx1, _ = f.notch
    q = (nx1 - nx0) / 4
    pads = [(lx0, y_top, lx0 + s, y_top + s), (lx1 - s, y_top, lx1, y_top + s)]
    for cx in (nx0 + q, nx1 - q):
        pads.append((cx - s / 2, ly1 - s, cx + s / 2, ly1))
    bx0, _, bx1, _ = f.board
    for x0, y0, x1, y1 in pads[:2]:
        assert x1 <= bx0 - 1 or x0 >= bx1 + 1, "side magnet pad is within 1 mm of the board"
    return pads


def magnet_positions(geom, p):
    """Centres (x, y) of the four magnet pockets; the cover's pockets are on
    the same axes."""
    return [((x0 + x1) / 2, (y0 + y1) / 2) for x0, y0, x1, y1 in magnet_pads(geom, p)]


def clip_positions(geom, p):
    """Per DIN clip: x, round_hole (x, y), slot_centre (x, y), body_bbox
    (x0, y0, x1, y1) on the back, rail_y. Hook end up (smaller y): the slot is
    above the round hole. The rail centre line is the box's mid-height line;
    the clips' centre lines are clip_inset in from the outside edges."""
    f = frame(geom, p)
    ox0, oy0, ox1, oy1 = f.outside
    rail_y = (oy0 + oy1) / 2
    ry = rail_y + CLIP_RAIL
    clips = []
    for x in (ox0 + p.clip_inset, ox1 - p.clip_inset):
        clips.append({
            "x": x,
            "round_hole": (x, ry),
            "slot_centre": (x, ry - p.clip_hole_spacing),
            "body_bbox": (x - CLIP_BODY_W / 2, ry - CLIP_BODY_HOOK, x + CLIP_BODY_W / 2, ry + CLIP_BODY_LATCH),
            "rail_y": rail_y,
        })
    return clips


def rounded(r, radius, z0, z1):
    """A rounded rectangle prism over z0..z1."""
    x0, y0, x1, y1 = r
    wp = box(x0, y0, x1, y1, z0, z1)
    return wp.edges("|Z").fillet(radius) if radius > 0 else wp


def slot(x, y, length, width, z0, z1):
    """An obround along y, length end to end."""
    return (cq.Workplane("XY").workplane(offset=z0).center(x, y)
            .slot2D(length, width, angle=90).extrude(z1 - z0))


def cylinder(x, y, d, z0, z1):
    return cq.Workplane("XY").add(cq.Solid.makeCylinder(d / 2, z1 - z0, cq.Vector(x, y, z0)))


def union_all(parts):
    return functools.reduce(lambda a, b: a.union(b), parts)


# Block capitals for the back markings, as centreline strokes in a unit cell
# (u right, v up, as read). Square caps, so joints close.
STROKES = {
    "H": [((0, 0), (0, 1)), ((1, 0), (1, 1)), ((0, 0.5), (1, 0.5))],
    "O": [((0, 0), (1, 0)), ((1, 0), (1, 1)), ((1, 1), (0, 1)), ((0, 1), (0, 0))],
    "K": [((0, 0), (0, 1)), ((0, 0.5), (1, 1)), ((0, 0.5), (1, 0))],
    "U": [((0, 1), (0, 0)), ((0, 0), (1, 0)), ((1, 0), (1, 1))],
    "P": [((0, 0), (0, 1)), ((0, 1), (1, 1)), ((1, 1), (1, 0.5)), ((1, 0.5), (0, 0.5))],
}


def word_solid(word, cx, cy, p, z0):
    """word debossed into the back, centred on (cx, cy), as the solid to cut.

    Seen from behind the box, text right is -x and text up is -y in this
    frame; after the y mirror of the export (see the module docstring) text up
    is up the wall and the word reads left to right from behind."""
    h, s = p.mark_h, p.mark_stroke
    w = 0.6 * h
    gap = 0.25 * h
    total = len(word) * w + (len(word) - 1) * gap
    letters = []
    for i, ch in enumerate(word):
        strokes = []
        u0 = -total / 2 + i * (w + gap) + s / 2
        for (a, b) in STROKES[ch]:
            # Centreline in text coordinates, then a rectangle s wide with caps.
            ua, va = u0 + a[0] * (w - s), -h / 2 + s / 2 + a[1] * (h - s)
            ub, vb = u0 + b[0] * (w - s), -h / 2 + s / 2 + b[1] * (h - s)
            du, dv = ub - ua, vb - va
            n = (du * du + dv * dv) ** 0.5
            tu, tv = du / n * s / 2, dv / n * s / 2       # half cap along
            nu, nv = -tv, tu                             # half width across
            pts = [(ua - tu + nu, va - tv + nv), (ub + tu + nu, vb + tv + nv),
                   (ub + tu - nu, vb + tv - nv), (ua - tu - nu, va - tv - nv)]
            xy = [(cx - u, cy - v) for u, v in pts]
            strokes.append(cq.Workplane("XY").workplane(offset=z0).polyline(xy).close().extrude(p.mark_depth))
        # Trim the diagonals' caps to the letter's cell (w x h).
        ul = u0 - s / 2
        cell = box(cx - (ul + w), cy - h / 2, cx - ul, cy + h / 2, z0, z0 + p.mark_depth)
        letters.append(union_all(strokes).intersect(cell))
    return union_all(letters)


def marking_solids(geom, p):
    """Per clip, {"HOOK": solid, "UP": solid}: the debossed words on the back,
    above the clip's hook end and centred on its centre line, UP uppermost."""
    f = frame(geom, p)
    out = []
    for c in clip_positions(geom, p):
        y_hook = c["body_bbox"][1] - 2.0 - p.mark_h / 2
        y_up = y_hook - p.mark_h - 2.0
        out.append({"HOOK": word_solid("HOOK", c["x"], y_hook, p, f.z_bottom),
                    "UP": word_solid("UP", c["x"], y_up, p, f.z_bottom)})
    return out


def make_base(geom, p):
    f = frame(geom, p)
    z_back = -geom["thickness"]

    # Floor: full outside size, rounded vertical edges, small chamfer round
    # the bed face and the rim the cover stands on.
    base = (rounded(f.outside, f.outside_r, f.z_bottom, f.z_floor_top)
            .edges("not |Z").chamfer(p.edge_chamfer))

    # Lip, broken at the cover's openings.
    lip = (rounded(f.lip_outer, f.inside_r - p.lip_clear, f.z_split, f.z_lip_top)
           .cut(rounded(f.lip_inner, f.inside_r - p.lip_clear - p.lip_t, f.z_split, f.z_lip_top)))
    ix0, iy0, _, _ = f.inside
    for side, a, b in f.lip_breaks:
        if side == "top":
            lip = lip.cut(box(a, iy0 - 1, b, f.lip_inner[1] + 1, f.z_split, f.z_lip_top + 1))
        else:
            lip = lip.cut(box(ix0 - 1, a, f.lip_inner[0] + 1, b, f.z_split, f.z_lip_top + 1))
    base = base.union(lip)

    # Magnet pads in the lip line, pockets opening up toward the cover.
    for r in magnet_pads(geom, p):
        base = base.union(box(*r, f.z_split, f.z_magnet))

    # Standoffs: the top radius all the way up, a 45 degree flare at the foot.
    for h in geom["corner_holes"]:
        x, y = h["x"], h["y"]
        r_top = p.standoff_top_radius_max
        flare = p.standoff_foot_r - r_top
        post = cylinder(x, y, 2 * r_top, f.z_floor_top, z_back)
        foot = cq.Workplane("XY").add(cq.Solid.makeCone(p.standoff_foot_r, r_top, flare,
                                                        cq.Vector(x, y, f.z_floor_top)))
        base = base.union(post).union(foot)

    # Cuts: insert holes, magnet pockets, clip holes and counterbores, markings.
    for h in geom["corner_holes"]:
        base = base.cut(cylinder(h["x"], h["y"], p.insert_hole_d, z_back - p.insert_hole_depth, z_back + 1))
    for x, y in magnet_positions(geom, p):
        d = p.magnet_d + p.fit_clearance
        base = base.cut(cylinder(x, y, d, f.z_magnet - p.magnet_t - p.magnet_recess, f.z_magnet + 1))
    z_cb = f.z_bottom + p.clip_floor_left
    travel = p.clip_slot_l - p.clip_slot_w
    for c in clip_positions(geom, p):
        rx, ry = c["round_hole"]
        sx, sy = c["slot_centre"]
        base = base.cut(cylinder(rx, ry, p.clip_hole_round, f.z_bottom - 1, f.z_floor_top + 1))
        base = base.cut(slot(sx, sy, p.clip_slot_l, p.clip_slot_w, f.z_bottom - 1, f.z_floor_top + 1))
        base = base.cut(cylinder(rx, ry, p.clip_head_clear_d, z_cb, f.z_floor_top + 1))
        base = base.cut(slot(sx, sy, p.clip_head_clear_d + travel, p.clip_head_clear_d, z_cb, f.z_floor_top + 1))
    for m in marking_solids(geom, p):
        for word in m.values():
            base = base.cut(word)
    return base
