#!/usr/bin/env python3
"""Checks on the schematic's netlist (build/netlist.xml, written by `make sch`)
against expected.py, the firmware and the datasheet pinouts in part-checks.md.
Run with the KiCad Python (needs pcbnew)."""
import csv
import os
import re
import sys
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import expected as exp  # noqa: E402
import netlist  # noqa: E402
import sexp  # noqa: E402

ROOT = os.path.dirname(HERE)
REPO = os.path.dirname(os.path.dirname(ROOT))
NETLIST = os.path.join(ROOT, "build", "netlist.xml")
SCH = os.path.join(ROOT, "carrier-board.kicad_sch")
BOM = os.path.join(ROOT, "bom.csv")
PART_CHECKS = os.path.join(ROOT, "part-checks.md")
MAIN_RS = os.path.join(REPO, "emberheat-esp32", "src", "main.rs")
KICAD_SHARE = "/Applications/KiCad/KiCad.app/Contents/SharedSupport"
ZONES = range(1, 8)


def read_netlist():
    """(nets, components): nets is {name: set of "REF.PAD"} for every net
    with a real connection; KiCad's single-pin "unconnected-(...)" nets and
    power-symbol pins (#PWR, #FLG) are left out. components is {ref: set of
    pads} for every placed part, from the netlist's own part list."""
    n = netlist.Netlist(NETLIST)
    return n.nets, n.pins


def bom_rows():
    rows = {}
    with open(BOM, newline="") as f:
        for row in csv.DictReader(f):
            for ref in row["refs"].split():
                rows[ref] = row
    return rows


def net_of(nets, pad):
    found = [name for name, pads in nets.items() if pad in pads]
    return found[0] if len(found) == 1 else (None if not found else found)


def test_nets_match_expected(nets, components):
    extra = sorted(set(nets) - set(exp.NETS))
    missing = sorted(set(exp.NETS) - set(nets))
    assert not extra, "nets not in expected.NETS: %s" % extra
    assert not missing, "expected nets missing from the schematic: %s" % missing
    for name in sorted(exp.NETS):
        want, got = exp.NETS[name], nets[name]
        assert got == want, "net %s: extra %s, missing %s" % (name, sorted(got - want), sorted(want - got))
    seen = {}
    for name, pads in nets.items():
        for p in pads:
            assert p not in seen, "pad %s is in both %s and %s" % (p, seen[p], name)
            seen[p] = name


def test_unconnected(nets, components):
    used = set().union(*nets.values())
    wrong = sorted(exp.UNCONNECTED & used)
    assert not wrong, "pads that must connect to nothing are in a net: %s" % wrong
    # Every pad of every part is accounted for: in a net, or listed unconnected.
    want_used = set().union(*exp.NETS.values())
    for ref, pins in sorted(components.items()):
        for pin in pins:
            pad = "%s.%s" % (ref, pin)
            assert pad in want_used or pad in exp.UNCONNECTED, "pad %s is in neither NETS nor UNCONNECTED" % pad


def test_no_connect_flags(nets, components):
    assert os.path.exists(SCH), "%s not found (run make sch)" % SCH
    with open(SCH) as f:
        tree = sexp.loads(f.read())
    flags = [c for c in tree if isinstance(c, list) and c and c[0] == "no_connect"]
    assert len(flags) == len(exp.UNCONNECTED), \
        "%d no-connect flags in the schematic, want one per unconnected pad (%d)" % (len(flags), len(exp.UNCONNECTED))


def test_relay_gpios_match_firmware(nets, components):
    with open(MAIN_RS) as f:
        text = f.read()
    m = re.search(r"HARDWIRED_RELAY_GPIOS:\s*\[u8;\s*7\]\s*=\s*\[([^\]]*)\]", text)
    assert m, "HARDWIRED_RELAY_GPIOS not found in %s" % MAIN_RS
    firmware = [int(x) for x in m.group(1).split(",") if x.strip()]
    assert firmware == [0, 1, 2, 3, 10, 11, 22], "firmware relay GPIOs are %s" % firmware
    drawn = []
    for n in ZONES:
        pads = [p for p in nets.get("Z%d_N" % n, set()) if p.startswith("A1.")]
        assert len(pads) == 1, "Z%d_N has A1 pads %s, want one" % (n, pads)
        name = exp.C6_PINS[pads[0].split(".", 1)[1]]
        assert re.fullmatch(r"GPIO\d+", name), "Z%d_N is on A1 %s (%s), not a GPIO" % (n, pads[0], name)
        drawn.append(int(name[4:]))
    assert drawn == firmware, "Z1_N..Z7_N are on GPIOs %s, firmware drives %s" % (drawn, firmware)


def test_inter_board_nets(nets, components):
    """Each inter-board net joins the C6 and H2 pins the design names, looked
    up by signal name in expected.C6_PINS / H2_PINS."""
    c6 = {v: k for k, v in exp.C6_PINS.items() if v not in ("GND", "NC")}
    h2 = {v: k for k, v in exp.H2_PINS.items() if v not in ("GND", "NC")}
    for net, a, b in (("UART_C6_TX", "GPIO18", "GPIO0"), ("UART_H2_TX", "GPIO19", "GPIO1"),
                      ("H2_RTS", "GPIO6", "GPIO2"), ("H2_CTS", "GPIO7", "GPIO3"),
                      ("H2_RST", "GPIO23", "RST")):
        pads = nets.get(net, set())
        assert "A1." + c6[a] in pads and "A2." + h2[b] in pads, \
            "%s should join C6 %s and H2 %s, has %s" % (net, a, b, sorted(pads))
    assert net_of(nets, "A1." + c6["3V3"]) == "+3V3", "C6 3V3 not on +3V3"
    assert net_of(nets, "A2." + h2["3V3"]) == "H2_3V3", "H2 3V3 not on H2_3V3"
    assert nets["H2_3V3"] == {"JP1.2", "A2." + h2["3V3"]}, "H2 3V3 must reach +3V3 only through JP1"
    assert net_of(nets, "JP1.1") == "+3V3", "JP1 pin 1 not on +3V3"
    for board, pins in (("A1", exp.C6_PINS), ("A2", exp.H2_PINS)):
        for pad, name in pins.items():
            if name == "GND":
                assert net_of(nets, "%s.%s" % (board, pad)) == "GND", "%s %s (GND) not on GND" % (board, pad)
    assert net_of(nets, "A1." + c6["5V"]) == "+5V", "C6 5V not on +5V"
    assert net_of(nets, "A2." + h2["5V"]) is None, "H2 5V must connect to nothing"


def test_zone_uses_normally_open(nets, components):
    for n in ZONES:
        j = "J1%d" % n
        k = "K%d" % n
        on = {net_of(nets, "%s.1" % j), net_of(nets, "%s.2" % j)}
        assert on == {"Z%d_A" % n, "Z%d_B" % n}, "%s pads are on %s" % (j, on)
        for net in on:
            kpads = {p for p in nets[net] if p.startswith(k + ".")}
            assert len(kpads) == 1, "%s has relay pads %s" % (net, kpads)
        relay = {p for net in on for p in nets[net] if p.startswith(k + ".")}
        assert relay == {k + ".11", k + ".14"}, "%s reaches relay pads %s, want 11 (common) and 14 (NO)" % (j, relay)
        assert net_of(nets, k + ".12") is None, "%s pad 12 (normally closed) is connected" % k


def test_diode_polarity(nets, components):
    # Device:D and Device:LED: pin 1 = K, pin 2 = A. Confirm from the library.
    for name in ("D", "LED"):
        pins = lib_pins("Device", name)
        assert pins.get("1") == "K" and pins.get("2") == "A", "Device:%s pins are %s" % (name, pins)
    for n in ZONES:
        assert net_of(nets, "D%d.1" % n) == "+5V", "D%d cathode not on +5V" % n
        assert net_of(nets, "D%d.2" % n) == "Z%d_COIL" % n, "D%d anode not on Z%d_COIL" % (n, n)
        assert net_of(nets, "D1%d.2" % n) == "+5V", "D1%d anode not on +5V" % n
        assert net_of(nets, "D1%d.1" % n) == "Z%d_LAMP" % n, "D1%d cathode not on Z%d_LAMP" % (n, n)
    assert net_of(nets, "D8.1") == "GND", "D8 cathode not on GND"
    assert net_of(nets, "D8.2") == "PWR_LED", "D8 anode not on PWR_LED"
    # C1 is polarised: pin 1 (+) on +5V.
    assert net_of(nets, "C1.1") == "+5V" and net_of(nets, "C1.2") == "GND", "C1 polarity"


def lib_pins(lib, name):
    """{pin number: pin name} of a stock library symbol, following extends."""
    path = os.path.join(KICAD_SHARE, "symbols", lib + ".kicad_sym")
    with open(path) as f:
        tree = sexp.loads(f.read())
    syms = {s[1]: s for s in tree if isinstance(s, list) and s and s[0] == "symbol"}
    sym = syms[name]
    ext = [c for c in sym if isinstance(c, list) and c and c[0] == "extends"]
    if ext:
        sym = syms[ext[0][1]]
    pins = {}

    def walk(t):
        for c in t:
            if isinstance(c, list) and c:
                if c[0] == "pin":
                    d = {x[0]: x for x in c if isinstance(x, list) and x}
                    pins[str(d["number"][1])] = str(d["name"][1])
                elif c[0] == "symbol":
                    walk(c)
    walk(sym)
    return pins


def footprint_pads(spec):
    import pcbnew
    lib, name = spec.split(":", 1)
    path = os.path.join(ROOT, "lib", "emberheat.pretty") if lib == "emberheat" else \
        os.path.join(KICAD_SHARE, "footprints", lib + ".pretty")
    fp = pcbnew.FootprintLoad(path, name)
    assert fp is not None, "footprint %s does not load" % spec
    return {p.GetNumber() for p in fp.Pads() if p.GetNumber()}


def test_transistor_pinout(nets, components):
    with open(PART_CHECKS) as f:
        text = f.read()
    section = text.split("## Transistor Q1-Q7: 2N3904", 1)[1].split("\n## ", 1)[0]
    m = re.search(r"TO-92 pinout in this datasheet: 1 (\w+), 2 (\w+), 3 (\w+)", section)
    assert m, "datasheet TO-92 pinout not found in part-checks.md"
    datasheet = {m.group(i).lower(): str(i) for i in (1, 2, 3)}
    rows = bom_rows()
    sym, fp = rows["Q1"]["symbol"], rows["Q1"]["footprint"]
    assert all(rows["Q%d" % n]["symbol"] == sym and rows["Q%d" % n]["footprint"] == fp for n in ZONES)
    pins = lib_pins(*sym.split(":", 1))
    by_name = {v: k for k, v in pins.items()}
    assert by_name["E"] == datasheet["emitter"], "symbol emitter is pin %s, datasheet leg %s" % (by_name["E"], datasheet["emitter"])
    assert by_name["B"] == datasheet["base"], "symbol base is pin %s, datasheet leg %s" % (by_name["B"], datasheet["base"])
    assert by_name["C"] == datasheet["collector"], "symbol collector is pin %s, datasheet leg %s" % (by_name["C"], datasheet["collector"])
    assert footprint_pads(fp) == {"1", "2", "3"}, "%s pads are not 1, 2, 3" % fp
    for n in ZONES:
        assert net_of(nets, "Q%d.%s" % (n, datasheet["emitter"])) == "GND", "Q%d emitter not on GND" % n
        assert net_of(nets, "Q%d.%s" % (n, datasheet["base"])) == "Z%d_BASE" % n, "Q%d base" % n
        assert net_of(nets, "Q%d.%s" % (n, datasheet["collector"])) == "Z%d_COIL" % n, "Q%d collector" % n


def test_optocoupler_pinout(nets, components):
    with open(PART_CHECKS) as f:
        text = f.read()
    m = re.search(r"Pinout from the package drawing: 1 (\w+), 2 (\w+), 3 (\w+), 4 (\w+)", text)
    assert m, "LTV-817 pinout not found in part-checks.md"
    pin = {m.group(i): str(i) for i in (1, 2, 3, 4)}
    for n in ZONES:
        assert net_of(nets, "U%d.%s" % (n, pin["anode"])) == "Z%d_LED" % n, "U%d anode" % n
        assert net_of(nets, "U%d.%s" % (n, pin["cathode"])) == "Z%d_N" % n, "U%d cathode" % n
        assert net_of(nets, "U%d.%s" % (n, pin["collector"])) == "+5V", "U%d collector" % n
        assert net_of(nets, "U%d.%s" % (n, pin["emitter"])) == "Z%d_DRV" % n, "U%d emitter" % n


def test_usb_cc_resistors(nets, components):
    rows = bom_rows()
    cc1, cc2 = net_of(nets, "J1.A5"), net_of(nets, "J1.B5")
    assert cc1 and cc2 and cc1 != cc2, "CC1 and CC2 are on %s and %s" % (cc1, cc2)
    for net in (cc1, cc2):
        others = nets[net] - {"J1.A5", "J1.B5"}
        assert len(others) == 1, "%s has %s besides the socket, want one resistor" % (net, sorted(others))
        ref, pad = others.pop().split(".")
        assert re.fullmatch(r"R\d+", ref) and rows[ref]["value"] == "5.1 k", "%s goes to %s (%s)" % (net, ref, rows[ref]["value"])
        other = "%s.%s" % (ref, "2" if pad == "1" else "1")
        assert net_of(nets, other) == "GND", "%s other end is on %s, not GND" % (ref, net_of(nets, other))


def test_no_strapping_pins_used(nets, components):
    for name, pad in exp.A1_FORBIDDEN.items():
        assert exp.C6_PINS[pad] == name, "expected.A1_FORBIDDEN %s is pad %s, C6_PINS says %s" % (name, pad, exp.C6_PINS[pad])
    for pad, name in exp.C6_PINS.items():
        if name in ("RST", "NC") or name in exp.A1_FORBIDDEN:
            assert net_of(nets, "A1." + pad) is None, "A1 %s (%s) is connected" % (pad, name)


def test_zone_nets_isolated(nets, components):
    zone = {"Z%d_%s" % (n, s) for n in ZONES for s in "AB"}
    assert {n for n in nets if re.fullmatch(r"Z\d_[AB]", n)} == zone, "zone-side nets are not exactly Z1_A..Z7_B"
    for n in ZONES:
        allowed = {"K%d" % n, "RV%d" % n, "J1%d" % n}
        for s in "AB":
            refs = {p.split(".")[0] for p in nets["Z%d_%s" % (n, s)]}
            assert refs <= allowed, "Z%d_%s touches %s" % (n, s, sorted(refs - allowed))
    # And the zone parts' other pads stay off the logic side: relay contacts,
    # varistor and terminal pads are on zone nets or unconnected only.
    for n in ZONES:
        for p in ("K%d.11" % n, "K%d.14" % n, "RV%d.1" % n, "RV%d.2" % n, "J1%d.1" % n, "J1%d.2" % n):
            assert net_of(nets, p) in zone, "%s is on %s" % (p, net_of(nets, p))


def test_parts_match_bom(nets, components):
    """Every ref in bom.csv is placed once, and every pad used in a net exists
    on that part's footprint."""
    rows = bom_rows()
    assert set(components) == set(rows), "placed %s, bom.csv has %s" % (
        sorted(set(components) - set(rows)), sorted(set(rows) - set(components)))
    root = ET.parse(NETLIST).getroot()
    for c in root.find("components"):
        ref = c.get("ref")
        if ref.startswith("#"):
            continue
        ls = c.find("libsource")
        row = rows[ref]
        assert "%s:%s" % (ls.get("lib"), ls.get("part")) == row["symbol"], "%s symbol %s:%s" % (ref, ls.get("lib"), ls.get("part"))
        assert c.findtext("footprint") == row["footprint"], "%s footprint %s" % (ref, c.findtext("footprint"))
        assert c.findtext("value") == row["value"], "%s value %r" % (ref, c.findtext("value"))
    pads_of = {}
    for ref, row in rows.items():
        fp = row["footprint"]
        if fp not in pads_of:
            pads_of[fp] = footprint_pads(fp)
    for name, pads in nets.items():
        for p in pads:
            ref, pad = p.split(".", 1)
            assert pad in pads_of[rows[ref]["footprint"]], "%s: %s has no pad %s" % (name, rows[ref]["footprint"], pad)


TESTS = [test_nets_match_expected, test_unconnected, test_no_connect_flags,
         test_relay_gpios_match_firmware, test_inter_board_nets, test_zone_uses_normally_open,
         test_diode_polarity, test_transistor_pinout, test_optocoupler_pinout,
         test_usb_cc_resistors, test_no_strapping_pins_used, test_zone_nets_isolated,
         test_parts_match_bom]


def main():
    try:
        nets, components = read_netlist()
    except (OSError, AssertionError, ET.ParseError) as e:
        print("FAIL netlist: %s" % e)
        return 1
    failed = 0
    for t in TESTS:
        try:
            t(nets, components)
            print("ok   %s" % t.__name__)
        except Exception as e:  # report every failure, not just the first
            failed += 1
            print("FAIL %s: %s" % (t.__name__, e))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
