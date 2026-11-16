#!/usr/bin/env python3
"""Route the logic side of carrier-board.kicad_pcb and add the ground pours.
Takes the board `make board` wrote (placed, zone side routed and locked,
logic side unrouted) and writes the routed board over it.

    route.py              import the committed Freerouting session
                          (routing/carrier-board.ses); no Java needed
    route.py --autoroute  run Freerouting first and commit its session

Freerouting 2.5.0 does not give the same routing twice from the same input
(even with one thread), so the session it produced is kept in the
repository and `make route` replays it: the same board every time.
`make autoroute` makes a new one; it is needed whenever the placement or
anything else in the router's input changes, which route.py detects from
the SHA-256 of the exported DSN recorded beside the session.

1. Refuse a board that already has logic-side copper.
2. Draw the pre-routed tracks (USB-C VBUS, see pre_route). Router's copy:
   router-only keepouts (zone strip less a moat round the zone copper, the
   board edges, vias under relays) and through-hole pads grown by PAD_GROW
   a side, so the router's 0.25 mm clearance keeps every wire 0.5 mm from
   the real pads of other nets (hand soldering); export Specctra DSN (zone and
   pre-routed tracks go out as fixed wires; the antenna rule areas as
   keepouts).
3. With --autoroute: Freerouting headless (no window, no dialogs, bounded
   passes, wall-clock timeout), AUTOROUTE_RUNS times, keeping the session
   with the fewest vias, then the shortest wire.
4. Import the session into a fresh copy of the board (no router-only
   keepouts); set logic tracks to their net class width; confirm the zone
   tracks are exactly as gen_board.py drew them and no part moved.
5. GND pour on both layers over the logic side (below the zone line), thermal
   reliefs on through-hole pads; fill; fix thermal reliefs KiCad's DRC
   calls starved (fix_thermals); save, reload, confirm the fill survived.

Run with the KiCad Python (needs pcbnew); --autoroute also needs Java."""
import contextlib
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import netlist  # noqa: E402

ROOT = os.path.dirname(HERE)
PCB = os.path.join(ROOT, "carrier-board.kicad_pcb")
WORK = os.path.join(ROOT, "build", "route")
DSN = os.path.join(WORK, "carrier-board.dsn")
SESSION = os.path.join(ROOT, "routing", "carrier-board.ses")          # committed
SESSION_INPUT = os.path.join(ROOT, "routing", "carrier-board.dsn.sha256")

FREEROUTING_VERSION = "2.5.0"
JAR = os.path.join(ROOT, "tools", "freerouting-%s.jar" % FREEROUTING_VERSION)
JAR_SHA256 = "f6f51bb02245e8e717f9359bd260cc9c5c0b1bc0acc8b7cb2cd5b8ffeb5de3c7"
MAX_PASSES = 100
TIMEOUT_S = 900            # whole Freerouting run, wall clock
JOB_TIMEOUT = "00:12:00"   # Freerouting's own budget, inside TIMEOUT_S
AUTOROUTE_RUNS = 3

ZONE_STRIP = "zone_strip"
ZONE_NET = re.compile(r"Z\d_[AB]")
RELAY_CONTACTS = {"11", "12", "14"}
STRIP_MOAT = 0.6           # mm of strip left open round zone copper (see router_keepouts)
POWER_NETS = {"+5V", "GND", "+3V3", "VIN", "H2_3V3"}
WIDTH = {"power": 1.0, "signal": 0.4}

# Ground pour: clearance to other nets, thermal relief gap and spoke width,
# smallest copper neck (mm). Spokes as wide as a signal track, gap wide
# enough that an iron heats the pad, not the plane.
POUR_NET = "GND"
POUR_CLEARANCE = 0.52      # 0.5 for hand soldering, plus the fill's arc approximation
POUR_MIN_WIDTH = 0.3
THERMAL_GAP = 0.5
THERMAL_SPOKE = 0.5

KICAD_CLI = "/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli"
THERMAL_ROUNDS = 4
HAND_SOLDER_GAP = 0.5      # mm, wire to through-hole pin of another net (hand soldering)
CLEARANCE = 0.25           # mm, the net classes' clearance: the router's only clearance
PAD_GROW = HAND_SOLDER_GAP - CLEARANCE   # router's copy: through-hole pads grow this much a side
EDGE_KEEP = 0.5           # copper to board edge (board rule min_copper_edge_clearance)
VIA = (0.8, 0.4)           # diameter, drill: the net classes' via

# USB-C VBUS (net VIN) at J1, drawn by pre_route(); see there.
VBUS_NECK = 0.5            # track width leaving pads A9/B9
VBUS_NECK_LEN = 1.07       # from the pad centre to where the track widens (pad edge + 0.47)
VBUS_RUN_X = 7.8           # A9's run down the front to J2 (0.6 mm clear of J1's shield pin)
VBUS_VIA_X = 8.4           # B9's via to the back


def need(ok, message):
    """Raise unless ok. Not an assert: these guard the board and must hold
    even under python -O."""
    if not ok:
        raise RuntimeError(message)


def mm(v):
    import pcbnew
    return pcbnew.FromMM(v)


def to_mm(v):
    import pcbnew
    return pcbnew.ToMM(v)


@contextlib.contextmanager
def quiet_stderr():
    """Drop wxWidgets' harmless start-up chatter that pcbnew writes straight
    to file descriptor 2; pass every other line through."""
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


def load():
    import pcbnew
    with quiet_stderr():
        return pcbnew.LoadBoard(PCB)


def is_zone_net(item):
    return ZONE_NET.fullmatch(netlist.bare(item.GetNetname())) is not None


def strip_area(board):
    areas = [z for z in board.Zones() if z.GetIsRuleArea() and z.GetZoneName() == ZONE_STRIP]
    need(len(areas) == 1, "expected one rule area named %s, found %d" % (ZONE_STRIP, len(areas)))
    return areas[0]


def snapshot(board):
    """Zone tracks as (uuid, start, end, width, layer, net, locked) and
    footprints as ref -> (position, rotation, layer)."""
    tracks = sorted((t.m_Uuid.AsString(), t.GetStart().x, t.GetStart().y, t.GetEnd().x, t.GetEnd().y,
                     t.GetWidth(), t.GetLayer(), t.GetNetname(), t.IsLocked())
                    for t in board.GetTracks() if is_zone_net(t))
    fps = {fp.GetReference(): (fp.GetPosition().x, fp.GetPosition().y, fp.GetOrientationDegrees(), fp.GetLayer())
           for fp in board.GetFootprints()}
    return tracks, fps


def check_start(board):
    logic = [t for t in board.GetTracks() if not is_zone_net(t)]
    pours = [z for z in board.Zones() if not z.GetIsRuleArea()]
    need(not logic and not pours,
        "%s already has %d logic-side tracks/vias and %d pours: run `make board` first (`make route` does)"
        % (os.path.basename(PCB), len(logic), len(pours)))
    zone = [t for t in board.GetTracks() if is_zone_net(t)]
    need(zone and all(t.IsLocked() for t in zone), "zone tracks missing or not locked")


def keepout(board, name, chain, tracks=True, vias=True):
    """A router-only rule area on both copper layers from one closed chain."""
    import pcbnew
    layers = pcbnew.LSET()
    layers.AddLayer(pcbnew.F_Cu)
    layers.AddLayer(pcbnew.B_Cu)
    z = pcbnew.ZONE(board)
    z.SetIsRuleArea(True)
    z.SetZoneName(name)
    z.SetLayerSet(layers)
    z.SetDoNotAllowTracks(tracks)
    z.SetDoNotAllowVias(vias)
    for setter in (z.SetDoNotAllowCopperPour, z.SetDoNotAllowPads, z.SetDoNotAllowFootprints):
        setter(False)
    z.Outline().AddOutline(chain)
    board.Add(z)


def zone_copper(board, grow):
    """Union of the zone-side copper (zone-net tracks and pads, relay contact
    pads 11, 12, 14) on both layers, grown by grow (nm)."""
    import pcbnew
    ps = pcbnew.SHAPE_POLY_SET()
    for t in board.GetTracks():
        if is_zone_net(t):
            t.TransformShapeToPolygon(ps, t.GetLayer(), grow, mm(0.005), pcbnew.ERROR_OUTSIDE)
    for fp in board.GetFootprints():
        relay = re.fullmatch(r"K\d+", fp.GetReference()) is not None
        for p in fp.Pads():
            if is_zone_net(p) or (relay and p.GetNumber() in RELAY_CONTACTS):
                for layer in (pcbnew.F_Cu, pcbnew.B_Cu):
                    if p.IsOnLayer(layer):
                        p.TransformShapeToPolygon(ps, layer, grow, mm(0.005), pcbnew.ERROR_OUTSIDE)
    ps.Simplify()
    return ps


def hole_free(region):
    """Closed chains covering region, none with holes: cut it with a
    vertical line through the middle of every hole (each hole is then open
    to the outside of both halves)."""
    import pcbnew
    bb = region.BBox()
    xs = set()
    for i in range(region.OutlineCount()):
        for h in range(region.HoleCount(i)):
            hb = region.CHole(i, h).BBox()
            xs.add((hb.GetLeft() + hb.GetRight()) // 2)
    cuts = [bb.GetLeft()] + sorted(xs) + [bb.GetRight()]
    out = []
    for x0, x1 in zip(cuts, cuts[1:]):
        slab = pcbnew.SHAPE_POLY_SET()
        slab.NewOutline()
        for x, y in ((x0, bb.GetTop()), (x1, bb.GetTop()), (x1, bb.GetBottom()), (x0, bb.GetBottom())):
            slab.Append(x, y)
        slab.BooleanIntersection(region)
        for i in range(slab.OutlineCount()):
            need(slab.HoleCount(i) == 0, "keepout piece between x %.2f and %.2f has a hole" % (to_mm(x0), to_mm(x1)))
            out.append(pcbnew.SHAPE_LINE_CHAIN(slab.COutline(i)))   # a copy: slab is freed
    return out


def router_keepouts(board):
    """Keepouts only the router sees.

    The zone strip: no track or via of any net. A keepout over the whole
    strip would also cover the zone tracks (fixed wires), which Freerouting
    then counts as violations and their nets as unrouted, and it stops
    without running its optimizer. So the keepout is the strip less a moat
    STRIP_MOAT wide round every piece of zone copper: too narrow for a track
    or via with clearance on both sides, so nothing can reach the zone
    copper through it. Split into hole-free pieces, as Specctra keepouts
    have no holes.

    And no via under a relay body (its courtyard)."""
    import pcbnew
    # KiCad exports every rule area as a keepout, whatever it forbids: the
    # named strip area itself must not reach the router.
    area = strip_area(board)
    region = pcbnew.SHAPE_POLY_SET(area.Outline())
    board.Remove(area)
    copper = zone_copper(board, mm(STRIP_MOAT))
    region.BooleanSubtract(copper)
    for i, chain in enumerate(hole_free(region)):
        keepout(board, "router_strip_%d" % i, chain)
    for n in range(1, 8):
        k = board.FindFootprintByReference("K%d" % n)
        cy = k.GetCourtyard(pcbnew.F_CrtYd)
        need(cy.OutlineCount() == 1, "K%d courtyard" % n)
        keepout(board, "router_no_via_K%d" % n, cy.COutline(0), tracks=False)


def edge_keepouts(board):
    """Router-only track/via keepout strips EDGE_KEEP wide along the inside
    of every board edge, and a square of side 2 EDGE_KEEP on every corner
    (the notch's inner corners would otherwise let a track cut close).
    Freerouting keeps only the net clearance (0.25 mm) to the outline; the
    board's copper-to-edge rule is 0.5 mm."""
    import pcbnew
    outline = pcbnew.SHAPE_POLY_SET()
    need(board.GetBoardPolygonOutlines(outline), "board outline is not closed")
    edge = outline.COutline(0)
    n = edge.PointCount()
    d = mm(EDGE_KEEP)
    rects = []
    for i in range(n):
        a, b = edge.CPoint(i), edge.CPoint((i + 1) % n)
        if a == b:
            continue
        need(a.x == b.x or a.y == b.y, "outline edge not axis-aligned")
        mx, my = (a.x + b.x) // 2, (a.y + b.y) // 2
        if a.x == b.x:      # vertical: inward is the side the board is on
            sx = 1 if outline.Contains(pcbnew.VECTOR2I(mx + d, my)) else -1
            rects.append((min(a.x, a.x + sx * d), min(a.y, b.y), max(a.x, a.x + sx * d), max(a.y, b.y)))
        else:
            sy = 1 if outline.Contains(pcbnew.VECTOR2I(mx, my + d)) else -1
            rects.append((min(a.x, b.x), min(a.y, a.y + sy * d), max(a.x, b.x), max(a.y, a.y + sy * d)))
        rects.append((a.x - d, a.y - d, a.x + d, a.y + d))
    for i, (x0, y0, x1, y1) in enumerate(rects):
        chain = pcbnew.SHAPE_LINE_CHAIN()
        for x, y in ((x0, y0), (x1, y0), (x1, y1), (x0, y1)):
            chain.Append(x, y)
        chain.SetClosed(True)
        keepout(board, "router_edge_%d" % i, chain)


def pre_route(board):
    """Tracks drawn here, with explicit coordinates, before the router runs;
    locked, so they go to the router as fixed wires.

    VIN from the USB-C socket J1: its VBUS pads A9 and B9 are 0.76 mm across
    with the CC pads 1.0 mm away, so a 1.0 mm track cannot reach them with
    0.25 mm clearance. Each leaves its pad at VBUS_NECK (0.5 mm) for at most
    VBUS_NECK_LEN and then runs at the Power width. A9 goes down the front to
    J2; B9 drops through one via and goes down the back to J2, so the two
    never close a loop round the CC pads, which leave to the right between
    them. Then J2 pin 1 to the fuse F1 down the back, left of J2's GND pin."""
    import pcbnew
    net = board.FindNet(netlist.Netlist().full_names["VIN"])
    a9 = pad_xy(board, "J1", "A9")
    b9 = pad_xy(board, "J1", "B9")
    j2 = pad_xy(board, "J2", "1")
    f1 = pad_xy(board, "F1", "1")
    neck_x = a9[0] + VBUS_NECK_LEN
    via = (VBUS_VIA_X, b9[1])
    segs = [
        (pcbnew.F_Cu, VBUS_NECK, [a9, (neck_x, a9[1])]),
        (pcbnew.F_Cu, WIDTH["power"], [(neck_x, a9[1]), (VBUS_RUN_X, a9[1]), (VBUS_RUN_X, j2[1] - (VBUS_RUN_X - j2[0])), j2]),
        (pcbnew.F_Cu, VBUS_NECK, [b9, (neck_x, b9[1])]),
        (pcbnew.F_Cu, WIDTH["power"], [(neck_x, b9[1]), via]),
        (pcbnew.B_Cu, WIDTH["power"], [via, (via[0], j2[1] - (via[0] - j2[0])), j2]),
        (pcbnew.B_Cu, WIDTH["power"], [j2, (f1[0], j2[1] + (j2[0] - f1[0])), f1]),
    ]
    for layer, width, pts in segs:
        for (x1, y1), (x2, y2) in zip(pts, pts[1:]):
            t = pcbnew.PCB_TRACK(board)
            t.SetStart(pcbnew.VECTOR2I(mm(x1), mm(y1)))
            t.SetEnd(pcbnew.VECTOR2I(mm(x2), mm(y2)))
            t.SetWidth(mm(width))
            t.SetLayer(layer)
            t.SetNet(net)
            t.SetLocked(True)
            board.Add(t)
    v = pcbnew.PCB_VIA(board)
    v.SetPosition(pcbnew.VECTOR2I(mm(via[0]), mm(via[1])))
    v.SetWidth(mm(VIA[0]))
    v.SetDrill(mm(VIA[1]))
    v.SetNet(net)
    v.SetLocked(True)
    board.Add(v)


def pad_xy(board, ref, number):
    fp = board.FindFootprintByReference(ref)
    pads = [p for p in fp.Pads() if p.GetNumber() == number]
    need(len(pads) == 1, (ref, number))
    return to_mm(pads[0].GetPosition().x), to_mm(pads[0].GetPosition().y)


FIXED_WIRE = re.compile(r"\(wire \(path (\S+) (\S+)((?:\s+-?[\d.]+\s+-?[\d.]+)+)\s*\)\s*\(net ([^)]+)\)\s*\(type fix\)\)")


def split_fixed_wires(dsn):
    """One fixed wire per segment in the DSN. KiCad writes a run of locked
    tracks as one multi-point wire; a pad met at an inner point of it (the
    varistor on each Zn_B run, where the run is straight) is then not
    connected for Freerouting, which reports the net unrouted and so never
    runs its optimizer. Returns how many wires were split."""
    with open(dsn) as f:
        text = f.read()
    count = [0]

    def split(m):
        layer, width, coords, net = m.group(1), m.group(2), m.group(3).split(), m.group(4)
        pts = list(zip(coords[0::2], coords[1::2]))
        if len(pts) <= 2:
            return m.group(0)
        count[0] += 1
        return "\n    ".join("(wire (path %s %s  %s %s  %s %s)(net %s)(type fix))" % (layer, width, a[0], a[1], b[0], b[1], net)
                               for a, b in zip(pts, pts[1:]))
    text = FIXED_WIRE.sub(split, text)
    with open(dsn, "w") as f:
        f.write(text)
    return count[0]


def enlarge_through_hole_pads(board):
    """Hand-soldering clearance for the router, by geometry: on the router's
    copy only, every through-hole pad's copper grows by PAD_GROW on every
    side (diameter or both dimensions + 2 PAD_GROW), so a wire the router
    keeps its ordinary CLEARANCE from the grown pad is HAND_SOLDER_GAP from
    the real one. Wires end on pad centres, so the real board's connections
    are the same. The USB-C socket's surface-mount pads are not grown.

    Two pads of a footprint so close that their grown copper would be nearer
    than CLEARANCE (the 3 mm LEDs: 1.8 mm pads on 2.54 mm pitch) grow less
    on the side facing each other only: GROW_FACING there, PAD_GROW
    elsewhere, by a pad offset away from the partner. No wire fits between
    them anyway (0.74 mm). Returns the references whose pads grew that way."""
    import math
    import pcbnew
    grow = mm(PAD_GROW)
    trimmed = set()
    for fp in board.GetFootprints():
        pth = [p for p in fp.Pads() if p.GetAttribute() == pcbnew.PAD_ATTRIB_PTH]
        for p in pth:
            need(p.GetShape(pcbnew.F_Cu) in (pcbnew.PAD_SHAPE_CIRCLE, pcbnew.PAD_SHAPE_RECTANGLE,
                                             pcbnew.PAD_SHAPE_OVAL, pcbnew.PAD_SHAPE_ROUNDRECT),
                 "%s.%s: pad shape %d not handled" % (fp.GetReference(), p.GetNumber(), p.GetShape(pcbnew.F_Cu)))
            need(p.GetOffset(pcbnew.F_Cu) == pcbnew.VECTOR2I(0, 0), "%s.%s has a pad offset" % (fp.GetReference(), p.GetNumber()))
        # Partners: another PTH pad of the footprint, different net, whose
        # grown copper would come within CLEARANCE of this pad's.
        facing = {}
        for p in pth:
            for q in pth:
                if q is p or q.GetNetCode() == p.GetNetCode():
                    continue
                a, b = p.GetFPRelativePosition(), q.GetFPRelativePosition()
                dx, dy = b.x - a.x, b.y - a.y
                if dx and dy:
                    continue        # not side by side on an axis: grown_pad_gaps() decides
                gap = math.hypot(dx, dy) - (p.GetSize(pcbnew.F_Cu).x if dy == 0 else p.GetSize(pcbnew.F_Cu).y) / 2 \
                    - (q.GetSize(pcbnew.F_Cu).x if dy == 0 else q.GetSize(pcbnew.F_Cu).y) / 2
                if gap - 2 * grow < mm(CLEARANCE + 0.05):
                    need(p.GetFPRelativeOrientation().AsDegrees() % 180 == 0,
                         "%s.%s is turned in its footprint" % (fp.GetReference(), p.GetNumber()))
                    facing[p.m_Uuid.AsString()] = (dx, dy, gap)
        for p in pth:
            size = p.GetSize(pcbnew.F_Cu)
            if p.m_Uuid.AsString() in facing:
                dx, dy, gap = facing[p.m_Uuid.AsString()]
                facing_grow = min(grow, int(gap - mm(CLEARANCE + 0.05)) // 2)
                need(facing_grow >= 0, "%s.%s: pads too close to grow" % (fp.GetReference(), p.GetNumber()))
                shift = (grow - facing_grow) // 2          # away from the partner
                if dy == 0:
                    p.SetSize(pcbnew.F_Cu, pcbnew.VECTOR2I(size.x + grow + facing_grow, size.y + 2 * grow))
                    p.SetOffset(pcbnew.F_Cu, pcbnew.VECTOR2I(-shift if dx > 0 else shift, 0))
                else:
                    p.SetSize(pcbnew.F_Cu, pcbnew.VECTOR2I(size.x + 2 * grow, size.y + grow + facing_grow))
                    p.SetOffset(pcbnew.F_Cu, pcbnew.VECTOR2I(0, -shift if dy > 0 else shift))
                if p.GetShape(pcbnew.F_Cu) == pcbnew.PAD_SHAPE_CIRCLE:
                    p.SetShape(pcbnew.F_Cu, pcbnew.PAD_SHAPE_OVAL)   # no longer round; an oval still covers the circle
                trimmed.add(fp.GetReference())
            else:
                p.SetSize(pcbnew.F_Cu, pcbnew.VECTOR2I(size.x + 2 * grow, size.y + 2 * grow))
        for p in pth:
            p.BuildEffectiveShapes()
    return sorted(trimmed)


def grown_pad_gaps(board):
    """Smallest gap (mm) between grown copper of two different-net pads on
    the router's copy, and where: must be at least CLEARANCE, or the router
    starts with violations it cannot fix."""
    import pcbnew
    items = []
    for fp in board.GetFootprints():
        for p in fp.Pads():
            for layer in (pcbnew.F_Cu, pcbnew.B_Cu):
                if p.IsOnLayer(layer) and p.GetAttribute() in (pcbnew.PAD_ATTRIB_PTH, pcbnew.PAD_ATTRIB_SMD):
                    ps = pcbnew.SHAPE_POLY_SET()
                    p.TransformShapeToPolygon(ps, layer, 0, mm(0.002), pcbnew.ERROR_OUTSIDE)
                    need(ps.OutlineCount() == 1, "%s.%s copper is %d pieces" % (fp.GetReference(), p.GetNumber(), ps.OutlineCount()))
                    items.append((layer, p.GetNetCode(), "%s.%s" % (fp.GetReference(), p.GetNumber()),
                                  pcbnew.SHAPE_LINE_CHAIN(ps.COutline(0)), ps.BBox()))   # a copy: ps is freed
    worst = (float("inf"), None)
    near = mm(CLEARANCE + 1.0)
    for i, (la, na, ida, ca, ba) in enumerate(items):
        for lb, nb, idb, cb, bb in items[i + 1:]:
            if la != lb or na == nb:
                continue
            if ba.GetRight() + near < bb.GetLeft() or bb.GetRight() + near < ba.GetLeft() \
                    or ba.GetBottom() + near < bb.GetTop() or bb.GetBottom() + near < ba.GetTop():
                continue
            # Closest approach of two disjoint polygons: a vertex of one to
            # an edge of the other (0 if they overlap at a vertex).
            d2 = min(min(ca.SquaredDistance(cb.CPoint(k)) for k in range(cb.PointCount())),
                     min(cb.SquaredDistance(ca.CPoint(k)) for k in range(ca.PointCount())))
            d = to_mm(int(round(d2 ** 0.5)))
            if d < worst[0]:
                worst = (d, "%s and %s" % (ida, idb))
    return worst


def sha256(path):
    import hashlib
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def freerouting_command(dsn, ses):
    return ["java", "-Djava.awt.headless=true", "-jar", JAR,
            "-de", dsn, "-do", ses,
            "--gui.enabled=false",          # headless: no window, no dialogs
            "-da",                          # no usage data sent
            "--user_data_path=%s" % os.path.join(WORK, "freerouting-data"),
            "--logging.file.enabled=false",
            "--logging.console.level=INFO",
            "--router.autorouter.max_passes=%d" % MAX_PASSES,
            "--router.job_timeout=%s" % JOB_TIMEOUT,
            "--router.fanout.enabled=false",       # no SMD fanout vias (the only SMD part is J1)
            "--router.automatic_neckdown=false",   # keep net class widths up to the pads
            "--router.optimizer.improvement_threshold=0.5",   # keep optimising while it removes vias
            "--router.optimizer.max_threads=1"]


def session_stats(ses):
    """(vias, wire length mm) of a Freerouting session."""
    import math
    with open(ses) as f:
        text = f.read()
    res = float(re.search(r"\(resolution um (\d+)\)", text).group(1))
    vias = len(re.findall(r"\(via ", text))
    length = 0.0
    for m in re.finditer(r"\(path \S+ \S+((?:\s+-?\d+)+)\s*\)", text):
        c = [float(v) for v in m.group(1).split()]
        pts = list(zip(c[0::2], c[1::2]))
        length += sum(math.hypot(q[0] - p[0], q[1] - p[1]) for p, q in zip(pts, pts[1:]))
    return vias, length / res / 1000.0


def run_freerouting(run):
    """One headless Freerouting run on DSN. Returns (unrouted connections,
    vias, wire mm, session path, log path)."""
    need(os.path.exists(JAR), "%s missing: see README.md (Freerouting) for the download" % JAR)
    got = sha256(JAR)
    need(got == JAR_SHA256, "%s has SHA-256 %s, README.md records %s" % (JAR, got, JAR_SHA256))
    ses = os.path.join(WORK, "run%d.ses" % run)
    log_path = os.path.join(WORK, "run%d.log" % run)
    if os.path.exists(ses):
        os.remove(ses)
    cmd = freerouting_command(DSN, ses)
    with open(log_path, "w") as log:
        log.write(" ".join(cmd) + "\n")
        log.flush()
        r = subprocess.run(cmd, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                           timeout=TIMEOUT_S, cwd=WORK)
    need(r.returncode == 0, "Freerouting exited %d; see %s" % (r.returncode, log_path))
    need(os.path.exists(ses), "Freerouting wrote no session; see %s" % log_path)
    with open(log_path) as f:
        left = sum(int(n) for n in re.findall(r"Net '[^']+' \((\d+) unrouted", f.read()))
    vias, length = session_stats(ses)
    print("Freerouting %s run %d: %d connections unrouted, %d vias, %.0f mm of wire"
          % (FREEROUTING_VERSION, run, left, vias, length))
    return left, vias, length, ses, log_path


def dsn_hash():
    """SHA-256 of the exported DSN, its first line (which names the file's
    own path) left out."""
    import hashlib
    with open(DSN, "rb") as f:
        text = f.read()
    return hashlib.sha256(text.split(b"\n", 1)[1]).hexdigest()


def autoroute():
    """AUTOROUTE_RUNS Freerouting runs; the best session (all routed, fewest
    vias, then shortest) becomes routing/carrier-board.ses, with the DSN's
    hash beside it."""
    import shutil
    runs = [run_freerouting(i) for i in range(1, AUTOROUTE_RUNS + 1)]
    best = min(runs, key=lambda r: (r[0], r[1], r[2]))
    need(best[0] == 0, "no Freerouting run routed everything; see %s" % best[4])
    os.makedirs(os.path.dirname(SESSION), exist_ok=True)
    shutil.copy(best[3], SESSION)
    with open(SESSION_INPUT, "w") as f:
        f.write("%s  carrier-board.dsn (the router input %s was made from, first line left out; Freerouting %s)\n"
                % (dsn_hash(), os.path.basename(SESSION), FREEROUTING_VERSION))
    print("kept %s as %s" % (os.path.basename(best[3]), os.path.relpath(SESSION, ROOT)))


def check_session():
    need(os.path.exists(SESSION) and os.path.exists(SESSION_INPUT),
        "%s missing: run `make autoroute`" % os.path.relpath(SESSION, ROOT))
    with open(SESSION_INPUT) as f:
        recorded = f.read().split()[0]
    need(recorded == dsn_hash(),
        "the router's input changed since %s was made (placement, rules or pre-routed tracks): "
        "run `make autoroute`" % os.path.relpath(SESSION, ROOT))


def export_dsn(board):
    """The router's copy of board, with the router-only keepouts, as
    build/route/carrier-board.dsn."""
    import pcbnew
    router_keepouts(board)
    edge_keepouts(board)
    trimmed = enlarge_through_hole_pads(board)
    gap, where = grown_pad_gaps(board)
    need(gap >= CLEARANCE, "grown pads %s are %.3f mm apart, under the router's %.2f mm" % (where, gap, CLEARANCE))
    with quiet_stderr():
        need(pcbnew.ExportSpecctraDSN(board, DSN), "DSN export failed")
    split = split_fixed_wires(DSN)
    print("exported %s: through-hole pads grown by %.2f mm a side (less between the pads of %s), smallest "
          "grown pad-to-pad gap %.3f mm (%s); %d fixed wires split into segments"
          % (os.path.relpath(DSN, ROOT), PAD_GROW, ", ".join(trimmed) or "none", gap, where, split))


def routed_board(ses):
    """Fresh copy of the placed board with the pre-routed tracks and the
    session's wires (the router-only keepouts never reach it). KiCad keeps
    locked tracks (zone side, pre-routed) through a session import and
    replaces the rest."""
    import pcbnew
    board = load()
    pcbnew.KIID.SeedGenerator(5)     # new tracks get the same uuids every run
    pre_route(board)
    with quiet_stderr():
        need(pcbnew.ImportSpecctraSES(board, ses), "SES import failed: %s" % ses)
    return board


def logic_widths(board):
    """Every logic track at its net class width. Returns how many changed."""
    changed = 0
    for t in board.GetTracks():
        if t.GetClass() == "PCB_VIA" or is_zone_net(t) or t.IsLocked():
            continue
        want = mm(WIDTH["power"] if netlist.bare(t.GetNetname()) in POWER_NETS else WIDTH["signal"])
        if t.GetWidth() != want:
            t.SetWidth(want)
            changed += 1
    return changed


def ground_pours(board, line_y):
    """GND on F.Cu and B.Cu over the logic side: the board outline below the
    zone line. The fill keeps out of the antenna rule areas by itself."""
    import pcbnew
    outline = pcbnew.SHAPE_POLY_SET()
    need(board.GetBoardPolygonOutlines(outline), "board outline is not closed")
    bb = outline.BBox()
    below = pcbnew.SHAPE_POLY_SET()
    below.NewOutline()
    for x, y in ((bb.GetLeft(), mm(line_y)), (bb.GetRight(), mm(line_y)),
                 (bb.GetRight(), bb.GetBottom()), (bb.GetLeft(), bb.GetBottom())):
        below.Append(x, y)
    below.BooleanIntersection(outline)
    need(below.OutlineCount() == 1, "logic side is %d pieces" % below.OutlineCount())
    gnd = board.FindNet(netlist.Netlist().full_names[POUR_NET])
    need(gnd, "no %s net" % POUR_NET)
    pours = []
    for layer in (pcbnew.F_Cu, pcbnew.B_Cu):
        z = pcbnew.ZONE(board)
        z.SetLayer(layer)
        z.SetNet(gnd)
        z.SetZoneName("GND_%s" % pcbnew.BOARD.GetStandardLayerName(layer).split(".")[0])
        z.Outline().AddOutline(below.COutline(0))
        z.SetPadConnection(pcbnew.ZONE_CONNECTION_THERMAL)
        z.SetThermalReliefGap(mm(THERMAL_GAP))
        z.SetThermalReliefSpokeWidth(mm(THERMAL_SPOKE))
        z.SetLocalClearance(mm(POUR_CLEARANCE))
        z.SetMinThickness(mm(POUR_MIN_WIDTH))
        z.SetIslandRemovalMode(pcbnew.ISLAND_REMOVAL_MODE_ALWAYS)
        z.SetAssignedPriority(0)
        board.Add(z)
        pours.append(z)
    return pours


def fill(board):
    import pcbnew
    filler = pcbnew.ZONE_FILLER(board)
    need(filler.Fill(board.Zones()), "zone fill failed")
    board.BuildConnectivity()


def fill_areas(board):
    import pcbnew
    out = {}
    for z in board.Zones():
        if z.GetIsRuleArea():
            continue
        for layer in (pcbnew.F_Cu, pcbnew.B_Cu):
            if z.IsOnLayer(layer):
                need(z.HasFilledPolysForLayer(layer), "%s not filled" % z.GetZoneName())
                out[pcbnew.BOARD.GetStandardLayerName(layer)] = z.GetFilledPolysList(layer).Area()
    return out


def zone_line_y(board):
    """y (mm) of the zone strip's lower edge: the bottom of its rule area."""
    return to_mm(strip_area(board).Outline().BBox().GetBottom())


def drc(board):
    """kicad-cli DRC of board as it is now (saved to build/route with the
    project's rules). Returns the report."""
    import json
    import pcbnew
    import shutil
    pcb = os.path.join(WORK, "carrier-board.kicad_pcb")
    for f in ("carrier-board.kicad_pro", "carrier-board.kicad_dru"):
        shutil.copy(os.path.join(ROOT, f), WORK)
    with quiet_stderr():
        pcbnew.SaveBoard(pcb, board)
    out = os.path.join(WORK, "drc.json")
    if os.path.exists(out):
        os.remove(out)      # never read a report from an earlier run
    r = subprocess.run([KICAD_CLI, "pcb", "drc", "--format", "json", "--severity-all", "-o", out, pcb],
                       capture_output=True, text=True)
    # kicad-cli drc exits 0 without --exit-code-violations unless it failed.
    need(r.returncode == 0 and os.path.exists(out),
         "kicad-cli drc failed (exit %d): %s" % (r.returncode, r.stderr.strip()[-500:]))
    with open(out) as f:
        return json.load(f)


def circle(x, y, r):
    import math
    import pcbnew
    ps = pcbnew.SHAPE_POLY_SET()
    ps.NewOutline()
    for i in range(32):
        a = 2 * math.pi * i / 32
        # Circumscribed polygon: it covers the circle.
        ps.Append(int(x + r / math.cos(math.pi / 32) * math.cos(a)), int(y + r / math.cos(math.pi / 32) * math.sin(a)))
    return ps


def pour_keepout(board, name, layer, chain):
    """A rule area on one layer that only keeps the pour out."""
    import pcbnew
    z = pcbnew.ZONE(board)
    z.SetIsRuleArea(True)
    z.SetZoneName(name)
    z.SetLayer(layer)
    z.SetDoNotAllowCopperPour(True)
    for setter in (z.SetDoNotAllowTracks, z.SetDoNotAllowVias, z.SetDoNotAllowPads, z.SetDoNotAllowFootprints):
        setter(False)
    z.Outline().AddOutline(chain)
    board.Add(z)


def starved_pad(board, v):
    """(pad, layer) of a starved_thermal violation."""
    import pcbnew
    m = re.search(r"layer ([FB])\.Cu", v["description"])
    layer = pcbnew.F_Cu if m.group(1) == "F" else pcbnew.B_Cu
    item = [i for i in v["items"] if re.match(r"(PTH )?pad ", i["description"])]
    need(item, "starved thermal without a pad: %s" % v)
    pos = pcbnew.VECTOR2I(mm(item[0]["pos"]["x"]), mm(item[0]["pos"]["y"]))
    pads = [p for fp in board.GetFootprints() for p in fp.Pads() if p.GetPosition() == pos]
    need(len(pads) == 1, "no single pad at %s" % item[0])
    return pads[0], layer


def fix_thermals(board):
    """Thermal reliefs KiCad's DRC calls starved, fixed without relaxing the
    two-spoke rule, until none is left (at most THERMAL_ROUNDS refills):

    - spokes that reach only an isolated island of fill (cut off from the
      pour by passing tracks): the island connects the pad to nothing, so a
      pour keepout removes it;
    - fewer than two spokes reach the pour: turn the pad's spokes 45
      degrees; if that does not help, keep the pour clear of the pad on that
      layer (it is connected by its tracks and on the other layer).

    The connectivity check after routing confirms nothing relied on them.
    Returns what was done, for the log."""
    import pcbnew
    done = []
    turned = set()
    for _ in range(THERMAL_ROUNDS):
        starved = [v for v in drc(board).get("violations", []) if v["type"] == "starved_thermal"]
        if not starved:
            return done
        for v in starved:
            pad, layer = starved_pad(board, v)
            name = "%s.%s" % (pad.GetParentFootprint().GetReference(), pad.GetNumber())
            lname = pcbnew.BOARD.GetStandardLayerName(layer)
            if "isolated island" in v["description"]:
                z = [z for z in board.Zones() if not z.GetIsRuleArea() and z.GetLayer() == layer][0]
                fill_ps = z.GetFilledPolysList(layer)
                reach = pcbnew.SHAPE_POLY_SET()
                pad.TransformShapeToPolygon(reach, layer, mm(THERMAL_GAP + 0.1), mm(0.005), pcbnew.ERROR_OUTSIDE)
                for i in range(fill_ps.OutlineCount()):
                    island = fill_ps.UnitSet(i)
                    touch = pcbnew.SHAPE_POLY_SET(island)
                    touch.BooleanIntersection(reach)
                    if not touch.OutlineCount():
                        continue
                    island.Inflate(mm(0.1), pcbnew.CORNER_STRATEGY_ROUND_ALL_CORNERS, mm(0.005))
                    pour_keepout(board, "no_pour_island_%s_%s" % (name, lname.split(".")[0]), layer,
                                 pcbnew.SHAPE_LINE_CHAIN(island.COutline(0)))
                    done.append("removed the isolated fill island at %s on %s" % (name, lname))
            elif (name, lname) not in turned:
                turned.add((name, lname))
                pad.SetThermalSpokeAngleDegrees(45 if pad.GetThermalSpokeAngleDegrees() == 90 else 90)
                done.append("turned %s's thermal spokes to %g degrees (%s)" % (name, pad.GetThermalSpokeAngleDegrees(), lname))
            else:
                r = mm(to_mm(max(pad.GetSize(layer).x, pad.GetSize(layer).y)) / 2 + THERMAL_GAP + POUR_CLEARANCE)
                c = circle(pad.GetPosition().x, pad.GetPosition().y, r)
                pour_keepout(board, "no_pour_at_%s_%s" % (name, lname.split(".")[0]), layer, pcbnew.SHAPE_LINE_CHAIN(c.COutline(0)))
                done.append("kept the pour clear of %s on %s" % (name, lname))
        fill(board)
    left = [v["description"] for v in drc(board).get("violations", []) if v["type"] == "starved_thermal"]
    need(not left, "thermal reliefs still starved: %s" % left)
    return done


def main(argv):
    import pcbnew
    os.makedirs(WORK, exist_ok=True)

    board = load()
    check_start(board)
    zone_tracks, fps = snapshot(board)
    pre_route(board)
    export_dsn(board)
    if "--autoroute" in argv:
        autoroute()
    check_session()

    board = routed_board(SESSION)
    widened = logic_widths(board)
    tracks2, fps2 = snapshot(board)
    need(tracks2 == zone_tracks, "zone tracks differ after the session import")
    moved = sorted(r for r in fps if fps2.get(r) != fps[r])
    need(not moved, "parts moved by the session import: %s" % moved)

    ground_pours(board, zone_line_y(board))
    fill(board)
    for line in fix_thermals(board):
        print("thermal relief: %s" % line)
    unrouted = board.GetConnectivity().GetUnconnectedCount(True)
    areas = fill_areas(board)
    with quiet_stderr():
        pcbnew.SaveBoard(PCB, board)

    again = load()
    again_areas = fill_areas(again)
    need(again_areas == areas, "fill changed on reload: %s, saved %s" % (again_areas, areas))

    tracks = [t for t in again.GetTracks() if t.GetClass() == "PCB_TRACK" and not is_zone_net(t)]
    vias = [t for t in again.GetTracks() if t.GetClass() == "PCB_VIA"]
    print("routed: %d logic track segments, %d vias, %d tracks set to net class width, zone tracks unchanged; "
          "%d unconnected; GND fill %s"
          % (len(tracks), len(vias), widened, unrouted,
             ", ".join("%s %.0f mm2" % (k, v / 1e12) for k, v in sorted(again_areas.items()))))
    return 0 if unrouted == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
