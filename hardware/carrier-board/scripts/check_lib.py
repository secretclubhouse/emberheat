#!/usr/bin/env python3
"""Checks on lib/: dev board footprints and symbols agree with expected.py,
the sockets are not mirrored or reversed, KiCad loads every file. Run with the
KiCad Python (needs pcbnew)."""
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import expected as exp  # noqa: E402
import sexp  # noqa: E402

ROOT = os.path.dirname(HERE)
LIB = os.path.join(ROOT, "lib")
SYM = os.path.join(LIB, "emberheat.kicad_sym")
PRETTY = os.path.join(LIB, "emberheat.pretty")
KICAD_CLI = os.environ.get("KICAD_CLI", "/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli")
KICAD_SHARE = "/Applications/KiCad/KiCad.app/Contents/SharedSupport"
TOL = 1e-4


def close(a, b):
    return abs(a - b) <= TOL


def read_tree(path):
    assert os.path.exists(path), "%s not found (run make lib)" % path
    with open(path) as f:
        return sexp.loads(f.read())


def kids(tree, name):
    return [c for c in tree if isinstance(c, list) and c and c[0] == name]


def one(tree, name):
    found = kids(tree, name)
    assert len(found) == 1, "expected one (%s ...), found %d" % (name, len(found))
    return found[0]


def footprint_pads(name):
    """{pad number: (x_mm, y_mm)} for numbered pads, read by KiCad itself."""
    import pcbnew
    fp = pcbnew.FootprintLoad(PRETTY, name)
    assert fp is not None, "KiCad cannot load footprint %s" % name
    pads = {}
    for p in fp.Pads():
        if p.GetNumber():
            pos = p.GetPosition()
            pads[p.GetNumber()] = (pcbnew.ToMM(pos.x), pcbnew.ToMM(pos.y))
    return pads


def footprint_tree(name):
    return read_tree(os.path.join(PRETTY, name + ".kicad_mod"))


def test_pad_counts():
    for name, board in exp.BOARDS.items():
        pads = footprint_pads(name)
        assert len(pads) == board["pads"], "%s has %d pads, want %d" % (name, len(pads), board["pads"])
        assert set(pads) == set(board["pins"]), "%s pad numbers differ from expected.py" % name


def test_pad_shape():
    for name in exp.BOARDS:
        tree = footprint_tree(name)
        pads = kids(tree, "pad")
        assert pads, "%s has no pads" % name
        for pad in pads:
            assert pad[2] == "thru_hole" and pad[3] == "circle", "%s pad %s is not a round through-hole" % (name, pad[1])
            size = [float(v) for v in one(pad, "size")[1:]]
            drill = float(one(pad, "drill")[1])
            assert close(size[0], exp.PAD_SIZE) and close(size[1], exp.PAD_SIZE), "%s pad %s size %s" % (name, pad[1], size)
            assert close(drill, exp.PAD_DRILL), "%s pad %s drill %s" % (name, pad[1], drill)


def test_row_geometry():
    for name, board in exp.BOARDS.items():
        pads = footprint_pads(name)
        for row, x in (("J1", -exp.ROW_X), ("J3", exp.ROW_X)):
            ys = []
            for num in sorted((n for n in pads if n.startswith(row + "_")), key=lambda n: int(n.split("_")[1])):
                px, py = pads[num]
                assert close(px, x), "%s %s at x=%s, want %s" % (name, num, px, x)
                ys.append(py)
            assert len(ys) == len(board["pins"]) // 2, "%s %s has %d pads" % (name, row, len(ys))
            for a, b in zip(ys, ys[1:]):
                assert close(b - a, exp.PITCH), "%s %s pitch %s, want %s" % (name, row, b - a, exp.PITCH)
        # The two rows are level: pad k of J1 and J3 share a y.
        for k in range(1, 16):
            a, b = pads.get("J1_%d" % k), pads.get("J3_%d" % k)
            if a and b:
                assert close(a[1], b[1]), "%s J1_%d and J3_%d not level" % (name, k, k)
        # Origin at the centre of the rows.
        ys = [p[1] for p in pads.values()]
        assert close(min(ys) + max(ys), 0.0), "%s rows are not centred on the origin" % name


def silk_rects(tree):
    """[(x0, y0, x1, y1)] of rectangles on F.SilkS."""
    rects = []
    for r in kids(tree, "fp_rect"):
        if one(r, "layer")[1] != "F.SilkS":
            continue
        s, e = one(r, "start"), one(r, "end")
        xs = sorted([float(s[1]), float(e[1])])
        ys = sorted([float(s[2]), float(e[2])])
        rects.append((xs[0], ys[0], xs[1], ys[1]))
    return rects


def test_socket_orientation():
    for name, board in exp.BOARDS.items():
        pads = footprint_pads(name)
        pins = board["pins"]
        j1 = {n: p for n, p in pads.items() if n.startswith("J1_")}
        j3 = {n: p for n, p in pads.items() if n.startswith("J3_")}
        # J1 is the left row (negative x), J3 the right row.
        assert pads["J1_1"][0] < 0, "%s: J1_1 is not at negative x" % name
        assert pads["J3_1"][0] > 0, "%s: J3_1 is not at positive x" % name
        # Pin 1 is the most negative y of its row (antenna end toward -y) and
        # pin numbers rise toward +y.
        for row in (j1, j3):
            first = min(row, key=lambda n: row[n][1])
            assert first.endswith("_1"), "%s: %s is nearest the antenna, not pin 1" % (name, first)
            n = len(row)
            for k in range(2, n + 1):
                prefix = first.split("_")[0]
                assert pads["%s_%d" % (prefix, k)][1] > pads["%s_%d" % (prefix, k - 1)][1], "%s: pad numbers do not rise toward +y" % name
        # Pad names against expected.py: pad number -> signal is checked via the
        # symbol; here pin 1 of J1 must be 3V3 and pin 1 of J3 must be GND.
        assert pins["J1_1"] == "3V3" and pins["J3_1"] == "GND"
        # The antenna rule area lies beyond J1_1 in -y.
        tree = footprint_tree(name)
        zones = kids(tree, "zone")
        assert len(zones) == 1, "%s: expected one rule area, found %d" % (name, len(zones))
        z = zones[0]
        assert [str(l) for l in one(z, "layers")[1:]] == ["F.Cu", "B.Cu"], "%s: rule area must cover both copper layers" % name
        keep = one(z, "keepout")
        for item in ("tracks", "vias", "copperpour", "pads"):
            assert one(keep, item)[1] == "not_allowed", "%s: rule area allows %s" % (name, item)
        pts = [(float(p[1]), float(p[2])) for p in one(one(z, "polygon"), "pts")[1:]]
        y1 = pads["J1_1"][1]
        assert max(y for _, y in pts) < y1, "%s: rule area reaches J1_1 (y=%s)" % (name, y1)
        # It starts at the antenna tip and ends 1.0 mm short of pin 1's centre.
        tip = y1 - board["edge_to_pin1"] - board["overhang"]
        assert close(min(y for _, y in pts), tip), "%s: rule area does not start at the antenna tip" % name
        depth = max(y for _, y in pts) - min(y for _, y in pts)
        want = board["edge_to_pin1"] + board["overhang"] - 1.0
        assert close(depth, want), "%s: rule area depth %s, want %s" % (name, depth, want)
        # Silkscreen outline: the PCB rectangle sits the right way round, the
        # antenna overhang lies at the -y end, ANTENNA is there, USB at the other end.
        rects = silk_rects(tree)
        pcb = [r for r in rects if close(r[2] - r[0], exp.BOARD_WIDTH) and close(r[3] - r[1], board["length"])]
        assert len(pcb) == 1, "%s: no %s x %s silk outline" % (name, exp.BOARD_WIDTH, board["length"])
        x0, top, x1, bottom = pcb[0]
        assert close(x0, -x1), "%s: outline not centred on the rows" % name
        assert close(top, y1 - board["edge_to_pin1"]), "%s: PCB edge to pin 1 is not %s" % (name, board["edge_to_pin1"])
        assert bottom > max(p[1] for p in pads.values()), "%s: outline ends before the last pad" % name
        ant = [r for r in rects if close(r[2] - r[0], board["ant_width"])]
        assert len(ant) == 1 and close(ant[0][1], tip) and close(ant[0][3], top), "%s: antenna overhang outline misplaced" % name
        texts = {str(t[2]): t for t in kids(tree, "fp_text") if t[1] == "user"}
        assert "ANTENNA" in texts and "USB" in texts, "%s: ANTENNA / USB labels missing" % name
        ay = float(one(texts["ANTENNA"], "at")[2])
        uy = float(one(texts["USB"], "at")[2])
        assert ay < y1 and uy > max(p[1] for p in pads.values()) - 10, "%s: labels at wrong ends" % name
        assert ay < uy, "%s: ANTENNA label is not at the -y end" % name


def symbol_pins(tree, name):
    sym = [s for s in kids(tree, "symbol") if s[1] == name]
    assert len(sym) == 1, "symbol %s not found once in lib" % name
    pins = {}
    for unit in kids(sym[0], "symbol"):
        for pin in kids(unit, "pin"):
            pins[str(one(pin, "number")[1])] = (str(one(pin, "name")[1]), pin)
    return pins


def test_symbol_matches_footprint():
    tree = read_tree(SYM)
    for name, board in exp.BOARDS.items():
        pads = footprint_pads(name)
        pins = symbol_pins(tree, name)
        assert len(pins) == board["pads"], "symbol %s has %d pins, want %d" % (name, len(pins), board["pads"])
        for num, (pname, _) in pins.items():
            assert num in pads, "symbol %s pin %s has no pad" % (name, num)
            assert pname == board["pins"][num], "symbol %s pin %s is %s, want %s" % (name, num, pname, board["pins"][num])
        assert set(pads) == set(pins), "pads without a symbol pin in %s" % name
        # Symbol layout mirrors the board: J1 on the left, J3 on the right,
        # pin 1 at the top.
        for num, (_, pin) in pins.items():
            x = float(one(pin, "at")[1])
            assert (x < 0) == num.startswith("J1_"), "symbol %s pin %s on the wrong side" % (name, num)
        for row in ("J1", "J3"):
            ys = [float(one(pins["%s_%d" % (row, k)][1], "at")[2]) for k in range(1, len([n for n in pins if n.startswith(row)]) + 1)]
            assert all(a > b for a, b in zip(ys, ys[1:])), "symbol %s %s pins do not run top to bottom" % (name, row)
        # The footprint field names the matching footprint.
        sym = [s for s in kids(tree, "symbol") if s[1] == name][0]
        fields = {str(p[1]): str(p[2]) for p in kids(sym, "property")}
        assert fields.get("Footprint") == "emberheat:" + name, "symbol %s Footprint field is %r" % (name, fields.get("Footprint"))


def test_din_clip():
    tree = footprint_tree("DIN_Clip")
    pads = kids(tree, "pad")
    assert len(pads) == 2, "DIN_Clip should have 2 holes, has %d" % len(pads)
    for p in pads:
        assert p[2] == "np_thru_hole", "DIN_Clip pad %s has copper" % p[1]
        assert not kids(tree, "zone")
    round_, slot = sorted(pads, key=lambda p: float(one(p, "at")[1]))
    assert p_at(round_) == (0.0, 0.0) and round_[3] == "circle"
    assert close(float(one(round_, "drill")[1]), exp.DIN_HOLE_DIA)
    assert close(p_at(slot)[0], exp.DIN_SPACING) and close(p_at(slot)[1], 0.0)
    drill = one(slot, "drill")
    assert drill[1] == "oval" and close(float(drill[2]), exp.DIN_SLOT_L) and close(float(drill[3]), exp.DIN_SLOT_W), "slot drill %s" % drill
    size = one(slot, "size")
    assert close(float(size[1]), exp.DIN_SLOT_L) and close(float(size[2]), exp.DIN_SLOT_W)


def p_at(pad):
    a = one(pad, "at")
    return (float(a[1]), float(a[2]))


def test_kicad_loads_every_footprint():
    import pcbnew
    names = sorted(f[:-len(".kicad_mod")] for f in os.listdir(PRETTY) if f.endswith(".kicad_mod"))
    assert names == sorted(list(exp.BOARDS) + ["DIN_Clip"]), "unexpected footprints %s" % names
    for n in names:
        assert pcbnew.FootprintLoad(PRETTY, n) is not None, "%s did not load" % n


def test_kicad_loads_symbols():
    assert os.path.exists(KICAD_CLI), "kicad-cli not found at %s" % KICAD_CLI
    with tempfile.TemporaryDirectory() as out:
        r = subprocess.run([KICAD_CLI, "sym", "export", "svg", "-o", out, SYM], capture_output=True, text=True)
        assert r.returncode == 0, "kicad-cli sym export svg failed: %s %s" % (r.stdout, r.stderr)
        svgs = sorted(os.listdir(out))
        assert len(svgs) == len(exp.BOARDS), "expected one SVG per symbol, got %s" % svgs


def test_lib_tables():
    sym = read_tree(os.path.join(ROOT, "sym-lib-table"))
    fp = read_tree(os.path.join(ROOT, "fp-lib-table"))
    for tree, uri in ((sym, "${KIPRJMOD}/lib/emberheat.kicad_sym"), (fp, "${KIPRJMOD}/lib/emberheat.pretty")):
        libs = kids(tree, "lib")
        assert len(libs) == 1
        assert str(one(libs[0], "name")[1]) == "emberheat" and str(one(libs[0], "uri")[1]) == uri


def test_sexp_round_trip():
    path = os.path.join(KICAD_SHARE, "symbols", "Relay.kicad_sym")
    with open(path) as f:
        text = f.read()
    tree = sexp.loads(text)
    again = sexp.loads(sexp.dumps(tree))
    assert sexp.shape(tree) == sexp.shape(again), "dumps(loads(Relay.kicad_sym)) does not re-parse equal"
    assert sexp.dumps(sexp.loads(sexp.dumps(tree))) == sexp.dumps(tree), "dumps is not stable"
    # Quoted strings stay quoted, bare atoms stay bare.
    assert any(isinstance(x, sexp.Q) for x in flat(tree)) and any(not isinstance(x, sexp.Q) for x in flat(tree))


def flat(tree):
    for x in tree:
        if isinstance(x, list):
            for y in flat(x):
                yield y
        else:
            yield x


def main():
    tests = [test_sexp_round_trip, test_pad_counts, test_pad_shape, test_row_geometry,
             test_socket_orientation, test_symbol_matches_footprint, test_din_clip,
             test_kicad_loads_every_footprint, test_kicad_loads_symbols, test_lib_tables]
    failed = 0
    for t in tests:
        try:
            t()
            print("ok   %s" % t.__name__)
        except Exception as e:  # report every failure, not just the first
            failed += 1
            print("FAIL %s: %s" % (t.__name__, e))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
