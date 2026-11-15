#!/usr/bin/env python3
"""Checks on bom.csv: every ref listed once, every library name real, every
row orderable. Run with the KiCad Python (needs pcbnew)."""
import csv
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
BOM = os.path.join(ROOT, "bom.csv")
LIB_DIR = os.path.join(ROOT, "lib")
KICAD_SHARE = "/Applications/KiCad/KiCad.app/Contents/SharedSupport"
COLUMNS = ["refs", "qty", "value", "manufacturer", "mpn", "symbol", "footprint", "note"]
# Rows with no orderable number: mounting holes and the unpopulated pad row.
NO_MPN = {"H1", "H2", "H3", "H4", "J4"}


def read_bom():
    with open(BOM, newline="") as f:
        reader = csv.DictReader(f)
        if reader.fieldnames != COLUMNS:
            raise AssertionError(f"bom.csv columns are {reader.fieldnames}, want {COLUMNS}")
        return list(reader)


def fixed_refs():
    refs = ["A1", "A2"]
    for n in range(1, 8):
        refs += [f"K{n}", f"U{n}", f"Q{n}", f"D{n}", f"D1{n}",
                 f"R{n}1", f"R{n}2", f"R{n}3", f"R{n}4", f"RV{n}", f"J1{n}"]
    refs += ["J1", "J2", "F1", "C1", "C2", "D8", "R3", "R1", "R2", "R4", "R5",
             "JP1", "J3", "J4", "H1", "H2", "H3", "H4"]
    return refs


def test_every_ref_listed(rows):
    seen = []
    for row in rows:
        refs = row["refs"].split()
        assert int(row["qty"]) == len(refs), f"qty {row['qty']} != {len(refs)} refs in {row['refs']!r}"
        seen += refs
    dup = sorted({r for r in seen if seen.count(r) > 1})
    assert not dup, f"refs listed more than once: {dup}"
    missing = [r for r in fixed_refs() if r not in seen]
    assert not missing, f"refs missing from bom.csv: {missing}"
    # DIN clip holes are H5 upward, contiguous, at least two (two clips).
    holes = sorted(int(r[1:]) for r in seen if re.fullmatch(r"H\d+", r))
    assert holes[:4] == [1, 2, 3, 4], "H1-H4 corner holes missing"
    din = [h for h in holes if h >= 5]
    assert len(din) >= 2 and din == list(range(5, 5 + len(din))), f"DIN clip holes H5.. not contiguous: {din}"
    extra = [r for r in seen if r not in fixed_refs() and r not in {f"H{h}" for h in din}]
    assert not extra, f"refs not in the fixed table: {extra}"


def _symbol_exists(lib, name):
    path = os.path.join(KICAD_SHARE, "symbols", lib + ".kicad_sym")
    if lib == "emberheat":
        path = os.path.join(LIB_DIR, "emberheat.kicad_sym")
    assert os.path.exists(path), f"symbol library {lib} not found at {path}"
    with open(path) as f:
        text = f.read()
    # Top-level symbols are indented one tab or two spaces; unit symbols
    # (name_1_1) are not matched because the name is quoted whole.
    return re.search(r'^\s{1,2}\(symbol "' + re.escape(name) + '"', text, re.M) is not None


def test_symbols_and_footprints_exist(rows):
    import pcbnew
    have_lib = os.path.isdir(LIB_DIR)
    for row in rows:
        sym, fp = row["symbol"], row["footprint"]
        for kind, spec in (("symbol", sym), ("footprint", fp)):
            assert ":" in spec, f"{row['refs']}: {kind} {spec!r} is not Lib:Name"
        slib, sname = sym.split(":", 1)
        flib, fname = fp.split(":", 1)
        if slib == "emberheat":
            if have_lib:
                assert _symbol_exists(slib, sname), f"{row['refs']}: symbol {sym} not in lib/"
        else:
            assert _symbol_exists(slib, sname), f"{row['refs']}: symbol {sym} not found"
        if flib == "emberheat":
            if have_lib:
                path = os.path.join(LIB_DIR, "emberheat.pretty")
                assert pcbnew.FootprintLoad(path, fname) is not None, f"{row['refs']}: footprint {fp} not in lib/"
        else:
            path = os.path.join(KICAD_SHARE, "footprints", flib + ".pretty")
            assert os.path.isdir(path), f"{row['refs']}: footprint library {flib} not found"
            assert pcbnew.FootprintLoad(path, fname) is not None, f"{row['refs']}: footprint {fp} does not load"


def test_orderable(rows):
    for row in rows:
        refs = set(row["refs"].split())
        if refs <= NO_MPN:
            continue
        assert row["manufacturer"].strip(), f"{row['refs']}: manufacturer empty"
        assert row["mpn"].strip(), f"{row['refs']}: mpn empty"


def main():
    tests = [test_every_ref_listed, test_symbols_and_footprints_exist, test_orderable]
    failed = 0
    try:
        rows = read_bom()
    except (OSError, AssertionError) as e:
        print(f"FAIL bom.csv: {e}")
        return 1
    for t in tests:
        try:
            t(rows)
            print(f"ok   {t.__name__}")
        except Exception as e:  # report every failure, not just the first
            failed += 1
            print(f"FAIL {t.__name__}: {e}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
