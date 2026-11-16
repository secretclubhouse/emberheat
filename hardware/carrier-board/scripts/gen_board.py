#!/usr/bin/env python3
"""Build carrier-board.kicad_pcb from the schematic's netlist: place every
part, draw the outline (with the notch for the dev boards' USB plugs) and
the silkscreen, route the zone side (24 V AC, locked tracks) and mark the
zone strip with a named rule area. The logic side is left unrouted for
route.py (`make route`). Also writes the net classes into
carrier-board.kicad_pro and the custom rules into carrier-board.kicad_dru.

Board frame: x to the right, y down, origin at the top-left corner, as the
board hangs on the wall (zone terminals at the top). Units are mm.
Run with the KiCad Python (needs pcbnew)."""
import csv
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import netlist  # noqa: E402

ROOT = os.path.dirname(HERE)
PCB = os.path.join(ROOT, "carrier-board.kicad_pcb")
PRO = os.path.join(ROOT, "carrier-board.kicad_pro")
DRU = os.path.join(ROOT, "carrier-board.kicad_dru")
BOM = os.path.join(ROOT, "bom.csv")
STOCK = "/Applications/KiCad/KiCad.app/Contents/SharedSupport/footprints"
DATE = "2026-11-15"

# --------------------------------------------------------------------------
# Band positions. Everything else is placed relative to these.

W, H = 168.0, 114.0

# Top strip (zone side): terminals at the top edge, varistor, relay contacts.
TERMINAL_PAD_Y = 10.6      # header pins; the plug opening is 10.1 mm above, at the edge
VARISTOR_PAD_Y = 15.2      # varistor pad 1
RELAY_A1_Y = 41.45         # relay coil pad A1; contact pad 14 is 21.25 mm above it
# The strip's lower limit is the middle of the relay's coil-to-contact gap
# (pad 12 bottom edge at RELAY_A1_Y - 11.25 + 1.2, pad A1 top edge at RELAY_A1_Y - 1.0).
ZONE_LINE_Y = ((RELAY_A1_Y - 11.25 + 1.2) + (RELAY_A1_Y - 1.0)) / 2

# Driver band: one column per zone under its relay.
COL_X0 = 32.6              # left edge of zone 1's column
COL_PITCH = 18.0           # also the terminal pitch
LED_PAD1_Y = 40.03         # zone lights, beside the relay coils
OPTO_PAD1_Y = 37.4
DIODE_Y = 48.0
RESISTOR_Y = (50.9, 54.6, 57.9, 61.6)   # Rn1, Rn2, Rn3, Rn4. Rows 1-2 and 3-4 are 3.7 mm apart so a 1.0 mm
                                        # supply track passes between them over 0.5 mm clear of the pads
                                        # (2-3 is 3.3: the diodes and transistors leave no more room)
TRANSISTOR_Y = 66.0

# Middle, above the notch: pull-up resistors, then the headers in one row.
PULLUP_Y = 70.0
HEADER_Y = 75.4

# Bottom band: dev boards lying horizontally, their pin-row centre line at
# DEV_Y. 8 mm below them is left for the corner holes H3, H4.
DEV_Y = H - 8.0 - 12.7
DEV_INSET = 0.35           # dev board PCB edge inside the board edge, antenna end
C6_ORIGIN_X = DEV_INSET + 20.625          # origin is 20.625 mm from the antenna-end PCB edge
H2_ORIGIN_X = W - DEV_INSET - 19.355
NOTCH_LEFT = DEV_INSET + 51.8             # C6 USB end
NOTCH_RIGHT = W - DEV_INSET - 48.26       # H2 USB end
NOTCH_TOP = DEV_Y - 12.7                  # as deep as the bottom band

# Mounting: corner holes 4 mm in; DIN clips on the rail line behind the
# board centre, hook end up, in the two lanes kept free of through-hole pads.
RAIL_Y = H / 2
CLIP_RAIL_FROM_ROUND = 16.14             # part-checks.md: rail centre at x 16.14 in the clip frame
CLIP_X = (28.2, 162.4)

ZONE_STRIP = "zone_strip"  # name of the rule area over the zone strip

SILK_W = 0.15
REF_SIZE = 0.8


# --------------------------------------------------------------------------

def mm(v):
    """mm to KiCad's nm, rounded to the nearest nm (pcbnew.FromMM truncates,
    so 78.76 became 78759999 nm, which the router's session file then
    rounded back: a 1 nm "move" of the part)."""
    return int(round(v * 1e6))


def pt(x, y):
    import pcbnew
    return pcbnew.VECTOR2I(mm(x), mm(y))


def load_footprint(spec):
    import pcbnew
    lib, name = spec.split(":", 1)
    path = os.path.join(ROOT, "lib", "emberheat.pretty") if lib == "emberheat" else \
        os.path.join(STOCK, lib + ".pretty")
    fp = pcbnew.FootprintLoad(path, name)
    assert fp is not None, "footprint %s does not load" % spec
    fp.SetFPID(pcbnew.LIB_ID(lib, name))
    return fp


class Board:
    def __init__(self, nl, path):
        import pcbnew
        self.pcbnew = pcbnew
        # NewBoard writes an empty board (and project) at path.
        self.board = pcbnew.NewBoard(path)
        # Fixed seed: the same netlist gives the same file, UUIDs included.
        # Seeded after NewBoard, whose project loading uses up a varying
        # number of ids (more when another board is already open, as in
        # check_board.py).
        pcbnew.KIID.SeedGenerator(4)
        self.nl = nl
        self.nets = {}
        for bare, full in sorted(nl.full_names.items()):
            ni = pcbnew.NETINFO_ITEM(self.board, full)
            self.board.Add(ni)
            self.nets[bare] = ni
        self.pad_net = nl.net_of_pad()
        # Pads that connect to nothing get KiCad's own single-pad net, as
        # "Update PCB from Schematic" would give them.
        for pad, full in sorted(nl.unconnected.items()):
            ni = pcbnew.NETINFO_ITEM(self.board, full)
            self.board.Add(ni)
            self.nets[full] = ni
            self.pad_net[pad] = full
        self.fps = {}

    def place(self, ref, x, y, rot, ref_at=None, ref_hidden=False):
        """Put ref's footprint origin at (x, y), turned rot degrees
        (counter-clockwise as seen from the front), and its reference text
        at ref_at (board coordinates), upright."""
        part = self.nl.parts[ref]
        fp = load_footprint(part["footprint"])
        fp.SetReference(ref)
        fp.SetValue(part["value"])
        # Link to the schematic symbol, as "Update PCB from Schematic" would.
        fp.SetPath(self.pcbnew.KIID_PATH(part["path"]))
        fp.SetSheetname("")
        fp.SetSheetfile("carrier-board.kicad_sch")
        # Attributes follow the symbol: every part is in the bill of
        # materials (the stock mounting-hole footprints say otherwise).
        fp.SetAttributes(fp.GetAttributes() & ~self.pcbnew.FP_EXCLUDE_FROM_BOM)
        self.board.Add(fp)
        fp.SetOrientationDegrees(rot)
        fp.SetPosition(pt(x, y))
        for pad in fp.Pads():
            net = self.pad_net.get("%s.%s" % (ref, pad.GetNumber()))
            if net:
                pad.SetNet(self.nets[net])
        r = fp.Reference()
        r.SetTextSize(pt(REF_SIZE, REF_SIZE))
        r.SetTextThickness(mm(0.12))
        r.SetTextAngleDegrees(0)
        if ref_at:
            r.SetPosition(pt(*ref_at))
        r.SetVisible(not ref_hidden)
        self.fps[ref] = fp
        return fp

    def pad(self, ref, number):
        p = [p for p in self.fps[ref].Pads() if p.GetNumber() == number]
        assert len(p) == 1, (ref, number)
        return self.pcbnew.ToMM(p[0].GetPosition().x), self.pcbnew.ToMM(p[0].GetPosition().y)

    def text(self, s, x, y, size=1.0, justify="center", layer=None, bold=False):
        pcbnew = self.pcbnew
        t = pcbnew.PCB_TEXT(self.board)
        t.SetText(s)
        t.SetLayer(pcbnew.F_SilkS if layer is None else layer)
        t.SetTextSize(pt(size, size))
        t.SetTextThickness(mm(size * (0.2 if bold else 0.15)))
        t.SetHorizJustify({"left": pcbnew.GR_TEXT_H_ALIGN_LEFT, "center": pcbnew.GR_TEXT_H_ALIGN_CENTER,
                           "right": pcbnew.GR_TEXT_H_ALIGN_RIGHT}[justify])
        if layer == pcbnew.B_SilkS:
            t.SetMirrored(True)
        t.SetPosition(pt(x, y))
        self.board.Add(t)
        return t

    def line(self, x1, y1, x2, y2, layer, width=SILK_W):
        pcbnew = self.pcbnew
        s = pcbnew.PCB_SHAPE(self.board)
        s.SetShape(pcbnew.SHAPE_T_SEGMENT)
        s.SetStart(pt(x1, y1))
        s.SetEnd(pt(x2, y2))
        s.SetLayer(layer)
        s.SetWidth(mm(width))
        self.board.Add(s)

    def rect(self, l, t, r, b, layer, width=SILK_W):
        pcbnew = self.pcbnew
        s = pcbnew.PCB_SHAPE(self.board)
        s.SetShape(pcbnew.SHAPE_T_RECT)
        s.SetStart(pt(l, t))
        s.SetEnd(pt(r, b))
        s.SetLayer(layer)
        s.SetWidth(mm(width))
        self.board.Add(s)

    def track(self, net, points, width):
        """Zone-side track, locked: the logic-side router and anyone editing
        the board by hand must not move it."""
        pcbnew = self.pcbnew
        for (x1, y1), (x2, y2) in zip(points, points[1:]):
            t = pcbnew.PCB_TRACK(self.board)
            t.SetStart(pt(x1, y1))
            t.SetEnd(pt(x2, y2))
            t.SetWidth(mm(width))
            t.SetLayer(pcbnew.F_Cu)
            t.SetNet(self.nets[net])
            t.SetLocked(True)
            self.board.Add(t)


# --------------------------------------------------------------------------

def outline(b):
    pcbnew = b.pcbnew
    pts = [(0, 0), (W, 0), (W, H), (NOTCH_RIGHT, H), (NOTCH_RIGHT, NOTCH_TOP),
           (NOTCH_LEFT, NOTCH_TOP), (NOTCH_LEFT, H), (0, H)]
    for (x1, y1), (x2, y2) in zip(pts, pts[1:] + pts[:1]):
        b.line(x1, y1, x2, y2, pcbnew.Edge_Cuts, 0.1)


def zone_column(b, n):
    """Zone n: terminal, varistor, relay (zone side), then the driver parts
    under the relay. x0 is the column's left edge."""
    x0 = COL_X0 + (n - 1) * COL_PITCH
    rx = x0 + 6.5                                  # relay pad line
    # Zone side. Terminal turned 180 so the plug enters from the top edge;
    # pin 1 is then on the right.
    b.place("J1%d" % n, x0 + 10.16, TERMINAL_PAD_Y, 180, ref_at=(x0 + 13.6, 14.6))
    b.place("RV%d" % n, x0 + 5.08, VARISTOR_PAD_Y, 0, ref_at=(x0 + 13.6, 16.6))
    b.place("K%d" % n, rx, RELAY_A1_Y, 90, ref_at=(x0 + 13.6, 26.0))
    b.text("Z%d" % n, x0 + 2.2, 15.4, 1.2, bold=True)            # terminal
    b.text("Z%d" % n, x0 + 2.5, 26.0, 1.2, bold=True)            # relay
    # Driver band.
    b.place("D1%d" % n, x0 + 2.5, LED_PAD1_Y, 90, ref_at=(x0 + 2.5, 44.4))
    b.text("Z%d" % n, x0 + 2.5, 42.4, 1.2, bold=True)            # zone light
    b.place("U%d" % n, x0 + 15.12, OPTO_PAD1_Y, 270, ref_at=(x0 + 14.9, DIODE_Y))
    b.place("D%d" % n, x0 + 4.4, DIODE_Y, 0, ref_at=(x0 + 1.7, DIODE_Y))
    for i, y in enumerate(RESISTOR_Y, start=1):
        b.place("R%d%d" % (n, i), x0 + 4.4, y, 0, ref_at=(x0 + 1.7, y))
    b.place("Q%d" % n, x0 + 4.4, TRANSISTOR_Y, 0, ref_at=(x0 + 1.7, TRANSISTOR_Y))
    # Zone-side tracks: K pad 14 -> J pin 2 through varistor pad 1 (Zn_B),
    # K pad 11 -> J pin 1 through varistor pad 2 (Zn_A). 45-degree bends.
    j1, j2 = b.pad("J1%d" % n, "1"), b.pad("J1%d" % n, "2")
    v1, v2 = b.pad("RV%d" % n, "1"), b.pad("RV%d" % n, "2")
    k11, k14 = b.pad("K%d" % n, "11"), b.pad("K%d" % n, "14")
    b.track("Z%d_B" % n, [j2, v1, (v1[0], k14[1] - (k14[0] - v1[0])), k14], 1.5)
    b.track("Z%d_A" % n, [j1, v2, (v2[0], k11[1] - (v2[0] - k11[0])), k11], 1.5)
    return x0, rx


def power(b):
    """Left edge of the driver band: USB-C and screw terminal opening left."""
    b.place("J1", 2.85, 42.0, 270, ref_at=(10.3, 39.0))           # USB-C, mouth 0.6 mm past the edge
    b.place("C1", 16.4, 41.0, 0, ref_at=(10.3, 43.5))
    b.text("5V ⎓ 3A USB-C", 0.9, 49.6, 0.9, justify="left")
    b.place("J2", 5.4, 55.0, 270, ref_at=(13.4, 57.55))
    vin, gnd = b.pad("J2", "1"), b.pad("J2", "2")
    b.text("+5V", 11.4, vin[1], 1.0, justify="left")
    b.text("GND", 11.4, gnd[1], 1.0, justify="left")
    # CC1 (J1 pad A5) is above CC2 (B5) at the socket, so CC1's resistor is
    # the right-hand one: the two tracks leave rightwards without crossing.
    b.place("R1", 20.95, 50.6, 270, ref_at=(20.95, 63.1))
    b.place("R2", 17.55, 50.6, 270, ref_at=(17.55, 63.1))
    b.place("F1", 2.9, 65.6, 0, ref_at=(5.5, 69.25))
    b.place("C2", 13.1, 66.0, 0, ref_at=(20.8, 66.0))
    b.place("D8", 2.2, 72.3, 0, ref_at=(3.5, 75.7))
    b.place("R3", 8.4, 72.3, 0, ref_at=(21.0, 72.3))
    b.text("PWR", 3.5, 77.3, 0.9)


def middle(b):
    """Above the notch: 1-Wire header, spare pads, H2 3.3 V jumper."""
    pcbnew = b.pcbnew
    # Pull-ups above the headers: R5 for the 1-Wire line, R4 for the C6 -> H2 UART line.
    b.place("R5", 55.6, PULLUP_Y, 0, ref_at=(68.4, PULLUP_Y))
    b.place("R4", 87.6, PULLUP_Y, 0, ref_at=(100.4, PULLUP_Y))
    x = 57.5
    b.place("J3", x, HEADER_Y, 90, ref_at=(54.2, HEADER_Y))
    b.text("1-WIRE", x + 2.54, HEADER_Y - 3.0, 1.0)
    for i, s in enumerate(("3V3", "DQ", "GND")):
        b.text(s, x + 2.54 * i, HEADER_Y + 3.2, 0.8)
    x = 70.0
    j4 = b.place("J4", x, HEADER_Y, 90, ref_at=(66.4, HEADER_Y))
    j4.SetDNP(True)
    j4.SetAttributes(j4.GetAttributes() | pcbnew.FP_EXCLUDE_FROM_POS_FILES)
    for i, s in enumerate(("3V3", "IO20", "TX", "RX", "GND")):
        # Alternate above and below so 4-letter labels fit a 2.54 mm pitch.
        b.text(s, x + 2.54 * i, HEADER_Y + (-2.8 if i % 2 == 0 else 3.2), 0.8)
    x = 101.0
    b.place("JP1", x, HEADER_Y, 90, ref_at=(x + 5.6, HEADER_Y))
    b.text("H2 3V3 — REMOVE BEFORE USING H2 USB", x + 1.27, HEADER_Y - 3.0, 0.9)


def dev_boards(b):
    # C6: antenna (footprint -y) to the left, so turned 90; H2 mirrored, 270.
    b.place("A1", C6_ORIGIN_X, DEV_Y, 90, ref_at=(NOTCH_LEFT - 6.0, DEV_Y))
    b.place("A2", H2_ORIGIN_X, DEV_Y, 270, ref_at=(NOTCH_RIGHT + 6.0, DEV_Y))
    # The footprints label their header rows "J1" / "J3" just beyond pin 1,
    # which here is past the board edge. Put each label inside its row,
    # beside pin 1, instead.
    for ref in ("A1", "A2"):
        fp = b.fps[ref]
        for item in fp.GraphicalItems():
            if item.GetClass() == "PCB_TEXT" and item.GetText() in ("J1", "J3"):
                row = "J1_1" if item.GetText() == "J1" else "J3_1"
                px, py = b.pad(ref, row)
                inward = 1 if py < DEV_Y else -1
                item.SetPosition(pt(px, py + inward * 2.2))
                item.SetTextAngleDegrees(0)


def mounting(b):
    pcbnew = b.pcbnew
    inset = 4.0
    for ref, (x, y) in zip(("H1", "H2", "H3", "H4"),
                           ((inset, inset), (W - inset, inset), (inset, H - inset), (W - inset, H - inset))):
        b.place(ref, x, y, 0, ref_hidden=True)
    # DIN clips: round hole on the clip's latch-end hole, slot (hook end) up.
    for ref, x in zip(("H5", "H6"), CLIP_X):
        ry = RAIL_Y + CLIP_RAIL_FROM_ROUND
        b.place(ref, x, ry, 90, ref_hidden=True)
        # Clip body on the back silkscreen (part-checks.md: x -4.50..40.90, y +/-5 in the clip frame).
        b.rect(x - 5.0, ry - 40.9, x + 5.0, ry + 4.5, pcbnew.B_SilkS)
        b.text("HOOK", x, ry - 37.5, 1.0, layer=pcbnew.B_SilkS)
        b.text("RAIL", x, RAIL_Y, 1.0, layer=pcbnew.B_SilkS)


def zone_strip_area(b):
    """A named rule area over the zone strip (board top to the zone line, full
    width, both copper layers). It forbids nothing itself: a keepout would
    also flag the zone tracks drawn inside it. The custom rule "no logic
    copper in the zone strip" in carrier-board.kicad_dru refers to it by name
    and forbids logic-side tracks, vias and pours there; route.py turns its
    keepouts on only for the router's copy of the board."""
    pcbnew = b.pcbnew
    z = pcbnew.ZONE(b.board)
    z.SetIsRuleArea(True)
    z.SetZoneName(ZONE_STRIP)
    layers = pcbnew.LSET()
    layers.AddLayer(pcbnew.F_Cu)
    layers.AddLayer(pcbnew.B_Cu)
    z.SetLayerSet(layers)
    for setter in (z.SetDoNotAllowTracks, z.SetDoNotAllowVias, z.SetDoNotAllowCopperPour,
                   z.SetDoNotAllowPads, z.SetDoNotAllowFootprints):
        setter(False)
    o = z.Outline()
    o.NewOutline()
    for x, y in ((0, 0), (W, 0), (W, ZONE_LINE_Y), (0, ZONE_LINE_Y)):
        o.Append(mm(x), mm(y))
    z.SetLocked(True)
    b.board.Add(z)


def silkscreen(b, relay_x):
    pcbnew = b.pcbnew
    # Zone line, broken where it passes under each relay body.
    gaps = sorted((x - 1.9, x + 4.3) for x in relay_x)
    x = 0.6
    for g0, g1 in gaps:
        b.line(x, ZONE_LINE_Y, g0, ZONE_LINE_Y, pcbnew.F_SilkS, 0.2)
        x = g1
    b.line(x, ZONE_LINE_Y, W - 0.6, ZONE_LINE_Y, pcbnew.F_SilkS, 0.2)
    b.text("ZONE WIRING 24 V AC", 1.0, ZONE_LINE_Y - 1.6, 1.0, justify="left", bold=True)
    b.text("EmberHeat", 9.0, 7.0, 2.0, justify="left", bold=True)
    b.text("carrier board", 9.0, 10.6, 1.2, justify="left")
    b.text("rev 1", 9.0, 13.4, 1.0, justify="left")
    b.text(DATE, 9.0, 15.6, 1.0, justify="left")


def write_project(nl):
    """Net classes into carrier-board.kicad_pro (kept otherwise as it is)."""
    with open(PRO) as f:
        pro = json.load(f)

    def cls(name, width, clearance=0.25):
        return {"name": name, "clearance": clearance, "track_width": width, "via_diameter": 0.8,
                "via_drill": 0.4, "microvia_diameter": 0.3, "microvia_drill": 0.1,
                "diff_pair_gap": 0.25, "diff_pair_via_gap": 0.25, "diff_pair_width": 0.2,
                "line_style": 0, "wire_width": 6, "bus_width": 12,
                "pcb_color": "rgba(0, 0, 0, 0.000)", "schematic_color": "rgba(0, 0, 0, 0.000)",
                "priority": 2147483647 if name == "Default" else {"Zone": 0, "Power": 1}[name]}
    zone = ["Z%d_%s" % (n, s) for n in range(1, 8) for s in "AB"]
    power = ["+5V", "GND", "+3V3", "VIN", "H2_3V3"]
    patterns = [{"netclass": "Zone", "pattern": nl.full_names[n]} for n in zone] + \
               [{"netclass": "Power", "pattern": nl.full_names[n]} for n in power]
    pro["net_settings"] = {
        "classes": [cls("Default", 0.4), cls("Power", 1.0), cls("Zone", 1.5)],
        "meta": {"version": 4},
        "net_colors": None,
        "netclass_assignments": None,
        "netclass_patterns": patterns,
    }
    with open(PRO, "w") as f:
        json.dump(pro, f, indent=2)
        f.write("\n")


def write_rules():
    with open(DRU, "w") as f:
        f.write("""(version 1)

# Hand soldering: every track stays 0.5 mm from every through-hole pad of
# another net (unconnected pads included), so a solder bridge at a joint
# does not reach it. Before the zone rule: where both apply, the later rule
# (3.0 mm) wins.
(rule "tracks 0.5 mm from other-net through-hole pads"
\t(constraint clearance (min 0.5mm))
\t(condition "(A.Type == 'Track' && B.Type == 'Pad' && B.Pad_Type == 'Through-hole' && A.Net != B.Net) || (B.Type == 'Track' && A.Type == 'Pad' && A.Pad_Type == 'Through-hole' && A.Net != B.Net)"))

# Zone-side copper (net class Zone, Z1_A..Z7_B, and the relays' unconnected
# normally-closed pad 12, which carries zone voltage whenever the relay is
# off) stays 3.0 mm from all other copper, on both layers.
(rule "zone separation"
\t(constraint clearance (min 3.0mm))
\t(condition "(A.NetClass == 'Zone' || (A.Type == 'Pad' && A.Pad_Number == '12' && A.memberOfFootprint('K*'))) && B.NetClass != 'Zone' && !(B.Type == 'Pad' && B.Pad_Number == '12' && B.memberOfFootprint('K*'))"))

# Nothing of the logic side in the zone strip (the rule area named
# %(strip)s, from the board top down to the zone line): no track, via or
# copper pour of any net outside net class Zone. Written as a rule rather
# than as keepouts on the area, which would also flag the zone tracks.
(rule "no logic copper in the zone strip"
\t(constraint disallow track via zone)
\t(condition "A.NetClass != 'Zone' && A.intersectsArea('%(strip)s')"))

# The dev boards' outlines are drawn on the silkscreen as the boards sit: the
# antenna end crosses the board edge and the USB end is flush with the notch
# side. That is the placement the design asks for; the board maker clips
# silkscreen at the edge. Only those outlines against Edge.Cuts are exempt.
(rule "dev board outlines may meet the board edge"
\t(constraint silk_clearance)
\t(severity ignore)
\t(condition "(A.memberOfFootprint('A?') && A.Layer == 'F.Silkscreen' && B.Layer == 'Edge.Cuts') || (B.memberOfFootprint('A?') && B.Layer == 'F.Silkscreen' && A.Layer == 'Edge.Cuts')"))
""" % {"strip": ZONE_STRIP})


def bom_footprints():
    out = {}
    with open(BOM, newline="") as f:
        for row in csv.DictReader(f):
            for ref in row["refs"].split():
                out[ref] = row["footprint"]
    return out


def build(nl, path=PCB):
    """The placed board with the zone side routed, in memory (KiCad's
    NewBoard leaves an empty board file at path). Also used by check_board.py,
    with a temporary path, to confirm routing left the zone tracks and the
    placement as drawn here."""
    b = Board(nl, path)
    outline(b)
    relay_x = [zone_column(b, n)[1] for n in range(1, 8)]
    power(b)
    middle(b)
    dev_boards(b)
    mounting(b)
    silkscreen(b, relay_x)
    zone_strip_area(b)
    missing = sorted(set(nl.parts) - set(b.fps))
    assert not missing, "not placed: %s" % missing
    return b.board


def main():
    import pcbnew
    nl = netlist.Netlist()
    bom = bom_footprints()
    for ref, part in nl.parts.items():
        assert part["footprint"] == bom[ref], "%s: netlist footprint %s, bom.csv %s" % (ref, part["footprint"], bom[ref])
    write_project(nl)
    write_rules()
    board = build(nl)
    pcbnew.SaveBoard(PCB, board)
    # A board KiCad cannot read back is not done.
    again = pcbnew.LoadBoard(PCB)
    assert len(again.GetFootprints()) == len(nl.parts), "reloaded board has %d footprints" % len(again.GetFootprints())
    print("wrote %s (%.1f x %.1f mm, notch %.2f x %.2f mm)" % (os.path.basename(PCB), W, H,
                                                             NOTCH_RIGHT - NOTCH_LEFT, H - NOTCH_TOP))
    return 0


if __name__ == "__main__":
    sys.exit(main())
