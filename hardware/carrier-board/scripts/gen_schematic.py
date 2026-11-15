#!/usr/bin/env python3
"""Write carrier-board.kicad_sch, the carrier board schematic.

Every part's symbol, footprint and value come from bom.csv. Library symbols
are read from KiCad's stock libraries and lib/, flattened (a symbol that
`extends` another is merged with its parent, as KiCad does) and embedded in
the schematic's lib_symbols. Pin positions are computed from those library
symbols, so wires land exactly on pins.

All coordinates here are in grid units of 1.27 mm (KiCad's default schematic
grid); they are converted to mm only when written.

Layout, one A2 sheet:
  left    power input (USB-C, 5 V terminal, fuse, bulk capacitors, power light)
  centre  the C6 (A1) and H2 (A2) dev boards with the inter-board nets as
          wires, and below them the 1-Wire header and the spare pads
  right   seven relay channels, each drawn by relay_channel(), reading left
          to right GPIO -> optocoupler -> transistor -> relay -> terminal

Wires join parts inside a block; net labels join blocks. Internal nets carry
a label on their wire as well, only to give the net its name.

Never imports expected.py: the checks compare this file's output with it.
"""
import copy
import csv
import os
import sys
import uuid

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import sexp  # noqa: E402
from sexp import Q  # noqa: E402

ROOT = os.path.dirname(HERE)
OUT = os.path.join(ROOT, "carrier-board.kicad_sch")
PROJECT = "carrier-board"
BOM = os.path.join(ROOT, "bom.csv")
KICAD_SYMBOLS = "/Applications/KiCad/KiCad.app/Contents/SharedSupport/symbols"
LOCAL_LIBS = {"emberheat": os.path.join(ROOT, "lib", "emberheat.kicad_sym")}

GRID = 1.27  # mm per grid unit
ROOT_UUID = "6f1c1e8a-3b0e-4c55-9d43-2a6c1f0e7b11"
_NS = uuid.UUID("0b9f5a52-7f36-4d8e-b1a4-7c2d4e5f6a70")

# Relay GPIO for zones 1-7, from the design ("Relay outputs").
ZONE_GPIOS = [0, 1, 2, 3, 10, 11, 22]

# Show the value on the drawing when it is short; otherwise show the
# manufacturer part number (the value stays in the netlist either way).
SHORT_VALUE = 12


# ---------------------------------------------------------------------------
# Output helpers
# ---------------------------------------------------------------------------

_uuid_count = [0]


def new_uuid():
    """Deterministic UUIDs, so regenerating gives an identical file."""
    _uuid_count[0] += 1
    return Q(str(uuid.uuid5(_NS, "carrier-board-%d" % _uuid_count[0])))


def mm(v):
    """Grid units -> mm atom."""
    s = "%.4f" % (v * GRID)
    s = s.rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def to_units(v_mm):
    u = float(v_mm) / GRID
    assert abs(u - round(u)) < 1e-6, "library pin at %s mm is off the 1.27 mm grid" % v_mm
    return int(round(u))


def effects(size=1.27, justify=None, hide=False, bold=False):
    font = ["font", ["size", str(size), str(size)]]
    if bold:
        font += [["thickness", str(round(size * 0.2, 3))], ["bold", "yes"]]
    e = ["effects", font]
    if justify:
        e.append(["justify"] + justify.split())
    if hide:
        e.append(["hide", "yes"])
    return e


# ---------------------------------------------------------------------------
# Library symbols
# ---------------------------------------------------------------------------

_lib_cache = {}


def _load_lib(lib):
    if lib not in _lib_cache:
        path = LOCAL_LIBS.get(lib, os.path.join(KICAD_SYMBOLS, lib + ".kicad_sym"))
        with open(path) as f:
            tree = sexp.loads(f.read())
        _lib_cache[lib] = {str(s[1]): s for s in tree if isinstance(s, list) and s and s[0] == "symbol"}
    return _lib_cache[lib]


def _kids(tree, name):
    return [c for c in tree if isinstance(c, list) and c and c[0] == name]


def flat_symbol(lib_id):
    """The library symbol as KiCad embeds it in a schematic: named
    "Lib:Name", with any `extends` merged into one stand-alone symbol."""
    lib, name = lib_id.split(":", 1)
    syms = _load_lib(lib)
    sym = syms[name]
    ext = _kids(sym, "extends")
    if not ext:
        out = copy.deepcopy(sym)
        out[1] = Q(lib_id)
        return out
    parent_name = str(ext[0][1])
    parent = syms[parent_name]
    assert not _kids(parent, "extends"), "two-level extends not handled: %s" % lib_id
    own = {str(p[1]): p for p in _kids(sym, "property")}
    out = ["symbol", Q(lib_id)]
    used = set()
    for item in parent[2:]:
        if isinstance(item, list) and item and item[0] == "property":
            key = str(item[1])
            out.append(copy.deepcopy(own.get(key, item)))
            used.add(key)
        elif isinstance(item, list) and item and item[0] == "symbol":
            unit = copy.deepcopy(item)
            uname = str(unit[1])
            assert uname.startswith(parent_name + "_"), uname
            unit[1] = Q(name + uname[len(parent_name):])
            out.append(unit)
        else:
            out.append(copy.deepcopy(item))
    # Derived-only properties go after the inherited ones, before the units.
    extra = [copy.deepcopy(p) for k, p in own.items() if k not in used]
    if extra:
        at = max(i for i, x in enumerate(out) if isinstance(x, list) and x and x[0] == "property") + 1
        out[at:at] = extra
    return out


def symbol_pins(flat):
    """{pin number: (x, y)} in grid units, library coordinates (y up)."""
    pins = {}

    def walk(t):
        for c in t:
            if isinstance(c, list) and c:
                if c[0] == "pin":
                    d = {x[0]: x for x in c if isinstance(x, list) and x}
                    num = str(d["number"][1])
                    pins.setdefault(num, (to_units(d["at"][1]), to_units(d["at"][2])))
                elif c[0] == "symbol":
                    walk(c)
    walk(flat)
    return pins


# ---------------------------------------------------------------------------
# Schematic model
# ---------------------------------------------------------------------------

def read_bom():
    rows = {}
    with open(BOM, newline="") as f:
        for row in csv.DictReader(f):
            for ref in row["refs"].split():
                rows[ref] = row
    return rows


BOM_ROWS = read_bom()


class Placed:
    """A symbol on the sheet. `at` in grid units, `rot` degrees counter-
    clockwise as KiCad draws it, `mirror` "x" (flip top-bottom) or "y" (flip
    left-right), only with rot 0."""

    def __init__(self, lib_id, ref, at, rot, mirror, pins):
        self.lib_id, self.ref, self.at, self.rot, self.mirror = lib_id, ref, at, rot, mirror
        self._pins = pins

    def offset(self, x, y):
        """Library coordinates (y up) -> sheet offset (y down)."""
        dx, dy = x, -y
        if self.mirror == "x":
            dy = -dy
        elif self.mirror == "y":
            dx = -dx
        r = self.rot % 360
        if r == 90:
            dx, dy = dy, -dx
        elif r == 180:
            dx, dy = -dx, -dy
        elif r == 270:
            dx, dy = -dy, dx
        return dx, dy

    def pin(self, number):
        x, y = self._pins[str(number)]
        dx, dy = self.offset(x, y)
        return (self.at[0] + dx, self.at[1] + dy)

    def pin_points(self):
        return [self.pin(n) for n in self._pins]


class Sheet:
    def __init__(self):
        self.lib = {}
        self.symbols = []      # (Placed, tree)
        self.wires = []        # ((x1, y1), (x2, y2))
        self.labels = []
        self.no_connects = []
        self.items = []        # texts, rectangles
        self.power_count = {"#PWR": 0, "#FLG": 0}

    # -- symbols ----------------------------------------------------------

    def place(self, lib_id, ref, at, rot=0, mirror=None, show=None):
        """Place `lib_id` as `ref`. Parts in bom.csv take their footprint and
        value from it (and must use its symbol); power symbols get a #PWR /
        #FLG reference. `show` maps field name -> (dx, dy[, justify]) in grid
        units from `at`: those fields are visible there, the rest hidden."""
        assert at[0] == int(at[0]) and at[1] == int(at[1]), "%s placed off grid" % ref
        assert mirror is None or rot == 0, "mirror only with rot 0"
        if lib_id not in self.lib:
            self.lib[lib_id] = flat_symbol(lib_id)
        flat = self.lib[lib_id]
        p = Placed(lib_id, ref, at, rot, mirror, symbol_pins(flat))
        lib_props = {str(c[1]): str(c[2]) for c in _kids(flat, "property")}
        power = bool(_kids(flat, "power"))
        if power:
            prefix = "#FLG" if lib_id == "power:PWR_FLAG" else "#PWR"
            self.power_count[prefix] += 1
            ref = p.ref = "%s%02d" % (prefix, self.power_count[prefix])
            fields = [("Reference", ref), ("Value", lib_props["Value"]), ("Footprint", ""),
                      ("Datasheet", lib_props.get("Datasheet", "")),
                      ("Description", lib_props.get("Description", ""))]
        else:
            row = BOM_ROWS[ref]
            assert row["symbol"] == lib_id, "%s: bom.csv symbol %s, placed %s" % (ref, row["symbol"], lib_id)
            fields = [("Reference", ref), ("Value", row["value"]), ("Footprint", row["footprint"]),
                      ("Datasheet", lib_props.get("Datasheet", "~")),
                      ("Description", lib_props.get("Description", "")),
                      ("Manufacturer", row["manufacturer"]), ("MPN", row["mpn"])]
        show = show or {}
        tree = ["symbol", ["lib_id", Q(lib_id)], ["at", mm(at[0]), mm(at[1]), str(rot)]]
        if mirror:
            tree.append(["mirror", mirror])
        # bom.csv marks parts that keep their pads but are not fitted ("Not fitted.").
        dnp = not power and BOM_ROWS[ref]["note"].startswith("Not fitted")
        tree += [["unit", "1"], ["exclude_from_sim", "no"], ["in_bom", "no" if power else "yes"],
                 ["on_board", "no" if power else "yes"], ["dnp", "yes" if dnp else "no"], ["uuid", new_uuid()]]
        # A field's stored angle is turned by the symbol's own rotation when
        # drawn; store the symbol's angle back so every label reads level.
        fangle = str(rot % 180)
        plain = rot == 0 and not mirror
        for name, value in fields:
            if name in show:
                spec = show[name]
                fx, fy = at[0] + spec[0], at[1] + spec[1]
                just = spec[2] if len(spec) > 2 and plain else None
                eff = effects(justify=just)
            else:
                fx, fy = at
                eff = effects(hide=True)
            tree.append(["property", Q(name), Q(value), ["at", mm(fx), mm(fy), fangle], eff])
        for num in sorted(p._pins):
            tree.append(["pin", Q(num), ["uuid", new_uuid()]])
        tree.append(["instances", ["project", Q(PROJECT),
                                   ["path", Q("/" + ROOT_UUID), ["reference", Q(ref)], ["unit", "1"]]]])
        self.symbols.append((p, tree))
        return p

    def part(self, lib_id, ref, at, rot=0, mirror=None, ref_at=None, val_at=None):
        """A bom.csv part with its reference and either its value (if short)
        or its MPN shown at the given offsets."""
        row = BOM_ROWS[ref]
        show = {}
        if ref_at:
            show["Reference"] = ref_at
        if val_at:
            if len(row["value"]) <= SHORT_VALUE or not row["mpn"]:
                if len(row["value"]) <= SHORT_VALUE:
                    show["Value"] = val_at
            else:
                show["MPN"] = val_at
        return self.place(lib_id, ref, at, rot, mirror, show)

    def power(self, name, at, rot=0):
        """A power symbol (+5V, +3V3, GND, PWR_FLAG) whose pin is at `at`,
        pointing up (rot 0), left (90), down (180) or right (270) for +5V /
        +3V3 / PWR_FLAG; GND points the opposite way."""
        down = name == "GND"
        # Unit vector from the pin toward the symbol's text.
        vec = {0: (0, -1), 90: (-1, 0), 180: (0, 1), 270: (1, 0)}[rot % 360]
        if down:
            vec = (-vec[0], -vec[1])
        dist = 3.2 if vec[1] else 4.5
        if name == "PWR_FLAG":
            dist = 3.4 if vec[1] else 7.0
        return self.place("power:" + name, None, at, rot,
                          show={"Value": (vec[0] * dist, vec[1] * dist)})

    # -- connections ------------------------------------------------------

    def wire(self, *points):
        for a, b in zip(points, points[1:]):
            assert a != b, "zero-length wire at %s" % (a,)
            assert a[0] == b[0] or a[1] == b[1], "wire %s-%s not horizontal or vertical" % (a, b)
            for p in (a, b):
                assert p[0] == int(p[0]) and p[1] == int(p[1]), "wire point %s off grid" % (p,)
            self.wires.append((a, b))

    def label(self, net, at, rot=0):
        just = {0: "left bottom", 90: "left bottom", 180: "right bottom", 270: "right bottom"}[rot]
        self.labels.append(["label", Q(net), ["at", mm(at[0]), mm(at[1]), str(rot)],
                            ["fields_autoplaced", "yes"], effects(justify=just), ["uuid", new_uuid()]])

    def no_connect(self, at):
        self.no_connects.append(["no_connect", ["at", mm(at[0]), mm(at[1])], ["uuid", new_uuid()]])

    def text(self, s, at, size=1.27, bold=False, justify="left bottom"):
        self.items.append(["text", Q(s), ["exclude_from_sim", "no"], ["at", mm(at[0]), mm(at[1]), "0"],
                           effects(size=size, justify=justify, bold=bold), ["uuid", new_uuid()]])

    def dashed_box(self, a, b):
        self.items.append(["rectangle", ["start", mm(a[0]), mm(a[1])], ["end", mm(b[0]), mm(b[1])],
                           ["stroke", ["width", "0.254"], ["type", "dash"]], ["fill", ["type", "none"]],
                           ["uuid", new_uuid()]])

    # -- checks and output ------------------------------------------------

    def _on_segment(self, p, a, b):
        """p strictly inside segment a-b."""
        if a[0] == b[0] == p[0]:
            return min(a[1], b[1]) < p[1] < max(a[1], b[1])
        if a[1] == b[1] == p[1]:
            return min(a[0], b[0]) < p[0] < max(a[0], b[0])
        return False

    def junctions(self):
        pins = {}
        for p, _ in self.symbols:
            for pt in p.pin_points():
                pins[pt] = pins.get(pt, 0) + 1
        ends = {}
        for a, b in self.wires:
            for pt in (a, b):
                ends[pt] = ends.get(pt, 0) + 1
        out = []
        for pt in sorted(set(ends) | set(pins)):
            n = ends.get(pt, 0) + pins.get(pt, 0)
            n += sum(1 for a, b in self.wires if self._on_segment(pt, a, b))
            if n >= 3 and (pt in ends or any(self._on_segment(pt, a, b) for a, b in self.wires)):
                out.append(pt)
        return out

    def sanity(self):
        # Collinear wires must not overlap (that would short two nets).
        for i, (a, b) in enumerate(self.wires):
            for c, d in self.wires[i + 1:]:
                if a[0] == b[0] == c[0] == d[0]:
                    lo, hi = sorted((a[1], b[1])), sorted((c[1], d[1]))
                    assert min(lo[1], hi[1]) <= max(lo[0], hi[0]), "vertical wires overlap at x=%s" % a[0]
                if a[1] == b[1] == c[1] == d[1]:
                    lo, hi = sorted((a[0], b[0])), sorted((c[0], d[0]))
                    assert min(lo[1], hi[1]) <= max(lo[0], hi[0]), "horizontal wires overlap at y=%s" % a[1]
        # No wire passes through a pin it is not meant to end on.
        for p, _ in self.symbols:
            for pt in p.pin_points():
                for a, b in self.wires:
                    assert not self._on_segment(pt, a, b), "wire %s-%s runs over pin of %s at %s" % (a, b, p.ref, pt)

    def write(self, path):
        self.sanity()
        tree = ["kicad_sch", ["version", "20250114"], ["generator", Q("eeschema")],
                ["generator_version", Q("9.0")], ["uuid", Q(ROOT_UUID)], ["paper", Q("A2")],
                ["title_block", ["title", Q("EmberHeat carrier board")], ["date", Q("2026-11-15")],
                 ["rev", Q("1")],
                 ["comment", "1", Q("ESP32-C6-DevKitC-1 + ESP32-H2-DevKitM-1, seven 24 V AC zone relays")],
                 ["comment", "2", Q("Generated by scripts/gen_schematic.py; do not edit by hand")]]]
        tree.append(["lib_symbols"] + [self.lib[k] for k in sorted(self.lib)])
        for pt in self.junctions():
            tree.append(["junction", ["at", mm(pt[0]), mm(pt[1])], ["diameter", "0"],
                         ["color", "0", "0", "0", "0"], ["uuid", new_uuid()]])
        tree += self.no_connects
        for a, b in self.wires:
            tree.append(["wire", ["pts", ["xy", mm(a[0]), mm(a[1])], ["xy", mm(b[0]), mm(b[1])]],
                         ["stroke", ["width", "0"], ["type", "default"]], ["uuid", new_uuid()]])
        tree += self.items
        tree += self.labels
        tree += [t for _, t in self.symbols]
        tree.append(["sheet_instances", ["path", Q("/"), ["page", Q("1")]]])
        tree.append(["embedded_fonts", "no"])
        with open(path, "w") as f:
            f.write(sexp.dumps(tree) + "\n")


# ---------------------------------------------------------------------------
# Drawing
# ---------------------------------------------------------------------------

def relay_channel(s, n, x0, y0):
    """Zone n's channel, origin (x0, y0). Reads left to right:
    Zn_N label -> optocoupler U -> Rn2 -> transistor Q -> relay coil, then
    inside the zone-wiring box the contacts, varistor and terminal."""
    def p(x, y):
        return (x0 + x, y0 + y)

    gpio = ZONE_GPIOS[n - 1]
    s.text("ZONE %d   GPIO%d" % (n, gpio), p(0, -11), size=2.54, bold=True)

    # Optocoupler LED: +3V3 -> Rn1 -> anode; cathode -> GPIO (LOW = on).
    r1 = s.part("Device:R", "R%d1" % n, p(8, -2), rot=90, ref_at=(0, -1.6), val_at=(0, 1.6))
    u = s.part("Isolator:PC817", "U%d" % n, p(26, 0), ref_at=(0, -4.4), val_at=(0, 4.4))
    s.wire(r1.pin(1), p(2, -2), p(2, -4))
    s.power("+3V3", p(2, -4))
    s.wire(r1.pin(2), u.pin(1))
    s.label("Z%d_LED" % n, p(12, -2))
    s.wire(p(0, 2), u.pin(2))
    s.label("Z%d_N" % n, p(0, 2))

    # Optocoupler transistor: +5V -> collector; emitter -> Rn2 -> base.
    s.wire(u.pin(4), p(34, -2), p(34, -5))
    s.power("+5V", p(34, -5))
    r2 = s.part("Device:R", "R%d2" % n, p(44, 2), rot=90, ref_at=(0, -1.6), val_at=(0, 1.6))
    s.wire(u.pin(3), r2.pin(1))
    s.label("Z%d_DRV" % n, p(34, 2))

    # Transistor base: Rn2 -> base, Rn3 pull-down to GND.
    q = s.part("Transistor_BJT:2N3904", "Q%d" % n, p(64, 2), ref_at=(4, 1, "left"), val_at=(4, 2.6, "left"))
    r3 = s.part("Device:R", "R%d3" % n, p(57, 7), ref_at=(1.5, -0.8, "left"), val_at=(1.5, 0.8, "left"))
    s.wire(r2.pin(2), p(57, 2), q.pin(2))
    s.wire(p(57, 2), r3.pin(1))
    s.label("Z%d_BASE" % n, p(48, 2))
    s.power("GND", r3.pin(2))
    s.wire(q.pin(1), p(66, 10))
    s.power("GND", p(66, 10))

    # Coil side: +5V bus on top (y -18), Zn_COIL bus below (y -2) from the
    # collector. Between them: flyback diode, zone light, relay coil.
    k = s.part("Relay:FINDER-34.51", "K%d" % n, p(102, -10), ref_at=(-5, 11, "right"), val_at=(-5, 12.6, "right"))
    d = s.part("Device:D", "D%d" % n, p(72, -10), rot=270, ref_at=(-3.5, -1), val_at=(-4, 1))
    led = s.part("Device:LED", "D1%d" % n, p(80, -15), rot=90, ref_at=(5, -1), val_at=(6.5, 1))
    r4 = s.part("Device:R", "R%d4" % n, p(90, -6), ref_at=(-1.5, -0.8, "right"), val_at=(-1.5, 0.8, "right"))
    assert q.pin(3) == p(66, -2)
    s.wire(q.pin(3), p(72, -2), p(90, -2), p(98, -2))
    s.label("Z%d_COIL" % n, p(74, -2))
    s.wire(p(70, -18), p(72, -18), p(80, -18), p(98, -18))
    s.power("+5V", p(70, -18))
    s.wire(k.pin("A1"), p(98, -18))
    s.wire(k.pin("A2"), p(98, -2))
    s.wire(p(72, -18), d.pin(1))          # cathode to +5V
    s.wire(d.pin(2), p(72, -2))           # anode to Zn_COIL
    assert led.pin(2) == p(80, -18)       # LED anode on the +5V bus
    s.wire(led.pin(1), p(80, -9), r4.pin(1))
    s.label("Z%d_LAMP" % n, p(82, -9))
    s.wire(r4.pin(2), p(90, -2))

    # Zone side: common (11) and normally open (14) to the varistor and the
    # terminal. 12 (normally closed) is unused.
    s.no_connect(k.pin("12"))
    rv = s.part("Device:Varistor", "RV%d" % n, p(118, -10), ref_at=(1.6, -0.8, "left"), val_at=(1.6, 0.8, "left"))
    j = s.part("Connector:Screw_Terminal_01x02", "J1%d" % n, p(136, -9), mirror="x",
               ref_at=(4, -2), val_at=(5, 0))
    assert j.pin(2) == p(132, -11) and j.pin(1) == p(132, -9)
    s.wire(k.pin("14"), p(108, -19), p(118, -19), p(128, -19), p(128, -11), j.pin(2))
    s.wire(p(118, -19), rv.pin(1))
    s.label("Z%d_B" % n, p(120, -19))
    s.wire(k.pin("11"), p(106, -1), p(118, -1), p(128, -1), p(128, -9), j.pin(1))
    s.wire(p(118, -1), rv.pin(2))
    s.label("Z%d_A" % n, p(120, -1))


def power_block(s):
    s.text("POWER INPUT", (12, 28), size=2.54, bold=True)
    # 5 V screw terminal, flipped left-right so pin 1 is on top.
    j2 = s.part("Connector:Screw_Terminal_01x02", "J2", (20, 36), mirror="y", ref_at=(-2, -3), val_at=None)
    s.text("5 V in", (12, 40))
    usb = s.part("Connector:USB_C_Receptacle_PowerOnly_6P", "J1", (36, 60),
                 ref_at=(-8, -14, "left"), val_at=(-8, -12.4, "left"))
    assert j2.pin(1) == (24, 36) and usb.pin("A9") == (48, 54)
    f1 = s.part("Device:Polyfuse", "F1", (68, 54), rot=90, ref_at=(0, -2), val_at=(0, 2))
    s.wire(j2.pin(1), (60, 36), (60, 54))
    s.wire(usb.pin("A9"), (60, 54), f1.pin(1))
    s.label("VIN", (50, 54))
    s.wire(j2.pin(2), (27, 38), (27, 41))
    s.power("GND", (27, 41))

    # USB-C CC pull-downs, one per CC pin.
    r2 = s.part("Device:R", "R2", (52, 70), ref_at=(-1.5, -0.8, "right"), val_at=(-1.5, 0.8, "right"))
    r1 = s.part("Device:R", "R1", (58, 70), ref_at=(1.5, -0.8, "left"), val_at=(1.5, 0.8, "left"))
    s.wire(usb.pin("B5"), (52, 66), r2.pin(1))
    s.label("CC2", (49, 66))
    s.wire(usb.pin("A5"), (58, 64), r1.pin(1))
    s.label("CC1", (49, 64))
    s.power("GND", r2.pin(2))
    s.power("GND", r1.pin(2))
    s.wire(usb.pin("S1"), (30, 76), (36, 76))
    s.wire(usb.pin("A12"), (36, 76), (42, 76))
    s.power("GND", (36, 76))
    s.power("PWR_FLAG", (42, 76), rot=180)

    # +5V rail: fuse output, bulk and decoupling capacitors, power light.
    c1 = s.part("Device:C_Polarized", "C1", (86, 59), ref_at=(-1.6, -0.8, "right"), val_at=(-1.6, 0.8, "right"))
    c2 = s.part("Device:C", "C2", (96, 59), ref_at=(1.6, -0.8, "left"), val_at=(1.6, 0.8, "left"))
    r3 = s.part("Device:R", "R3", (110, 58), ref_at=(1.5, -0.8, "left"), val_at=(1.5, 0.8, "left"))
    d8 = s.part("Device:LED", "D8", (110, 67), rot=90, ref_at=(5, -1), val_at=(6.5, 1))
    s.wire(f1.pin(2), (74, 54), (80, 54), (86, 54), (96, 54), (110, 54))
    s.power("+5V", (74, 54))
    s.power("PWR_FLAG", (80, 54))
    s.wire((86, 54), c1.pin(1))
    s.wire((96, 54), c2.pin(1))
    s.wire((110, 54), r3.pin(1))
    s.power("GND", c1.pin(2))
    s.power("GND", c2.pin(2))
    s.wire(r3.pin(2), d8.pin(2))
    s.label("PWR_LED", (110, 62))
    s.power("GND", d8.pin(1))


def dev_boards(s):
    """A1 (C6) and A2 (H2). Inter-board nets are wires; zone GPIOs and the
    connector nets leave as labels."""
    ax, ay = 145, 80
    s.text("DEV BOARDS", (112, 50), size=2.54, bold=True)
    a1 = s.place("emberheat:ESP32-C6-DevKitC-1", "A1", (ax, ay), show={"Reference": (0, -21.5)})
    s.text("ESP32-C6-DevKitC-1", (ax, ay - 19.4), justify="bottom")
    pl, pr = ax - 10, ax + 10

    # Left row (J1).
    assert a1.pin("J1_1") == (pl, ay - 16)
    s.wire(a1.pin("J1_1"), (pl - 3, ay - 16), (pl - 9, ay - 16))
    s.power("+3V3", (pl - 3, ay - 16))
    s.power("PWR_FLAG", (pl - 9, ay - 16))
    for pin in ("J1_2", "J1_3", "J1_4", "J1_9", "J1_16"):   # RST, GPIO4, GPIO5, GPIO8, NC
        s.no_connect(a1.pin(pin))
    for zone, pin in ((1, "J1_7"), (2, "J1_8"), (5, "J1_10"), (6, "J1_11"), (3, "J1_12"), (4, "J1_13")):
        x, y = a1.pin(pin)
        s.wire((x, y), (x - 3, y))
        s.label("Z%d_N" % zone, (x - 3, y), rot=180)
    s.wire(a1.pin("J1_14"), (pl - 3, ay + 10))
    s.power("+5V", (pl - 3, ay + 10), rot=90)
    s.wire(a1.pin("J1_15"), (pl - 3, ay + 12))
    s.power("GND", (pl - 3, ay + 12), rot=270)

    # Right row (J3).
    s.wire(a1.pin("J3_1"), (pr + 3, ay - 16), (pr + 3, ay - 18))
    s.power("GND", (pr + 3, ay - 18), rot=180)
    for pin in ("J3_4", "J3_11", "J3_13", "J3_14", "J3_16"):  # GPIO15, GPIO9, GPIO13, GPIO12, NC
        s.no_connect(a1.pin(pin))
    for net, pin in (("SPARE_TX", "J3_2"), ("SPARE_RX", "J3_3"), ("Z7_N", "J3_6"),
                     ("ONEWIRE", "J3_7"), ("SPARE_IO20", "J3_8")):
        x, y = a1.pin(pin)
        s.wire((x, y), (x + 3, y))
        s.label(net, (x + 3, y))
    s.wire(a1.pin("J3_12"), (pr + 3, ay + 6))
    s.power("GND", (pr + 3, ay + 6), rot=90)
    s.wire(a1.pin("J3_15"), (pr + 3, ay + 12))
    s.power("GND", (pr + 3, ay + 12), rot=90)

    # A2, its left row facing A1's right row.
    bx, by = ax + 56, ay + 3
    a2 = s.place("emberheat:ESP32-H2-DevKitM-1", "A2", (bx, by), show={"Reference": (0, -20.8)})
    s.text("ESP32-H2-DevKitM-1", (bx, by - 17.4), justify="bottom")
    ql, qr = bx - 10, bx + 10
    assert a2.pin("J1_2") == (ql, ay - 9)

    # H2_RST: C6 GPIO23 -> H2 RST.
    s.wire(a1.pin("J3_5"), (pr + 4, ay - 8), (pr + 4, ay - 9), a2.pin("J1_2"))
    s.label("H2_RST", (pr + 22, ay - 9))
    # UART: C6 GPIO18 (TX) -> H2 GPIO0; H2 GPIO1 (TX) -> C6 GPIO19. The two
    # cross once; no junction there.
    xc, xb = pr + 18, pr + 20
    s.wire(a1.pin("J3_10"), (pr + 12, ay + 2), (xc, ay + 2), (xc, ay - 7), a2.pin("J1_3"))
    s.label("UART_C6_TX", (xb + 1, ay - 7))
    s.wire(a1.pin("J3_9"), (xb, ay), (xb, ay - 5), a2.pin("J1_4"))
    s.label("UART_H2_TX", (xb + 1, ay - 5))
    # R4 pull-up keeps C6 TX idle-high while the C6 is in reset.
    r4 = s.part("Device:R", "R4", (pr + 12, ay + 7), ref_at=(1.5, -0.8, "left"), val_at=(1.5, 0.8, "left"))
    s.wire((pr + 12, ay + 2), r4.pin(1))
    s.power("+3V3", r4.pin(2), rot=180)

    # RTS / CTS: C6 GPIO6 / GPIO7 (left row) around the bottom of A1 to H2
    # GPIO2 / GPIO3. They cross once near A2.
    xv7, xv6 = pr + 24, pr + 26
    s.wire(a1.pin("J1_6"), (pl - 14, ay - 6), (pl - 14, ay + 24), (xv7, ay + 24), (xv7, ay - 1), a2.pin("J1_6"))
    s.wire(a1.pin("J1_5"), (pl - 16, ay - 8), (pl - 16, ay + 26), (xv6, ay + 26), (xv6, ay - 3), a2.pin("J1_5"))
    s.label("H2_CTS", (pl - 4, ay + 24))
    s.label("H2_RTS", (pl - 4, ay + 26))

    # H2 3V3 from the C6's 3V3 through JP1.
    jp = s.part("Connector:Conn_01x02_Pin", "JP1", (ql - 16, by - 16), ref_at=(-1, -2.2))
    assert jp.pin(2) == (ql - 12, by - 14) == (ql - 12, a2.pin("J1_1")[1])
    s.wire(jp.pin(2), a2.pin("J1_1"))
    s.label("H2_3V3", (ql - 11, by - 14))
    s.wire(jp.pin(1), (ql - 10, by - 16), (ql - 10, by - 18))
    s.power("+3V3", (ql - 10, by - 18))

    # A2 unused pins and grounds.
    for pin in ("J1_7", "J1_8", "J1_9", "J1_10", "J1_11", "J1_12", "J1_14",
                "J3_2", "J3_3", "J3_4", "J3_5", "J3_6", "J3_7", "J3_8", "J3_9", "J3_11", "J3_13", "J3_14"):
        s.no_connect(a2.pin(pin))
    for pin in ("J1_13", "J1_15"):
        x, y = a2.pin(pin)
        s.wire((x, y), (x - 3, y))
        s.power("GND", (x - 3, y), rot=270)
    for pin in ("J3_1", "J3_10", "J3_12", "J3_15"):
        x, y = a2.pin(pin)
        s.wire((x, y), (x + 3, y))
        s.power("GND", (x + 3, y), rot=90)


def connectors(s):
    s.text("CONNECTORS", (112, 124), size=2.54, bold=True)
    # J3: 1-Wire sensors (3V3, data on GPIO21, GND), R5 pull-up.
    j3 = s.part("Connector:Conn_01x03_Pin", "J3", (140, 140), ref_at=(-1, -3.6))
    s.text("1-Wire sensor", (128, 146))
    assert j3.pin(1) == (144, 138)
    s.wire(j3.pin(1), (147, 138), (147, 136))
    s.power("+3V3", (147, 136))
    r5 = s.part("Device:R", "R5", (152, 135), ref_at=(1.5, -0.8, "left"), val_at=(1.5, 0.8, "left"))
    s.wire(j3.pin(2), (152, 140), (160, 140))
    s.wire(r5.pin(2), (152, 140))
    s.power("+3V3", r5.pin(1))
    s.label("ONEWIRE", (160, 140))
    s.wire(j3.pin(3), (147, 142), (147, 144))
    s.power("GND", (147, 144))

    # J4: spare pads, not fitted (3V3, GPIO20, TX, RX, GND).
    j4 = s.part("Connector:Conn_01x05_Pin", "J4", (140, 162), ref_at=(-1, -6.6))
    s.text("Spare pads, not fitted", (124, 170))
    s.wire(j4.pin(1), (147, 158), (147, 156))
    s.power("+3V3", (147, 156))
    for pin, net in ((2, "SPARE_IO20"), (3, "SPARE_TX"), (4, "SPARE_RX")):
        x, y = j4.pin(pin)
        s.wire((x, y), (x + 8, y))
        s.label(net, (x + 8, y))
    s.wire(j4.pin(5), (147, 166), (147, 168))
    s.power("GND", (147, 168))


def mounting(s):
    s.text("MOUNTING", (12, 282), size=2.54, bold=True)
    for i in range(1, 7):
        s.place("Mechanical:MountingHole", "H%d" % i, (16 + 10 * (i - 1), 290), show={"Reference": (0, 3.5)})
    s.text("H1-H4 corner holes, H5 H6 DIN rail clips", (12, 298))


def notes(s):
    s.text("NOTES", (12, 214), size=2.54, bold=True)
    lines = [
        "Relays are active-low: GPIO LOW lights the optocoupler LED and closes the relay.",
        "A floating, reset or unpowered C6 passes no LED current, so every relay stays open.",
        "JP1 feeds the H2 from the C6 board's 3.3 V. Remove it before plugging a cable",
        "into the H2 board's own USB port.",
        "J4 (spare pads) is not fitted.",
        "Zone wiring (relay contacts 11 / 14, varistors, terminals) keeps 3 mm from all",
        "other copper on the board.",
    ]
    for i, line in enumerate(lines):
        s.text(line, (12, 220 + 2 * i))


def build():
    s = Sheet()
    power_block(s)
    dev_boards(s)
    connectors(s)
    mounting(s)
    notes(s)
    x0, y0, pitch = 240, 36, 40
    for n in range(1, 8):
        relay_channel(s, n, x0, y0 + (n - 1) * pitch)
    # Zone wiring box: contacts, varistors and terminals of all channels.
    s.dashed_box((x0 + 101.8, y0 - 21), (x0 + 145, y0 + 6 * pitch + 3))
    s.text("ZONE WIRING — 24 V AC", (x0 + 104, y0 - 22), size=2.54, bold=True)
    return s


def main():
    s = build()
    s.write(OUT)
    print("wrote %s" % os.path.relpath(OUT, ROOT))
    return 0


if __name__ == "__main__":
    sys.exit(main())
