#!/usr/bin/env python3
"""KiCad's electrical rules check on the schematic: every severity, no
violations, no exclusions. Same as `make erc`, but part of `make check`.
Run with the KiCad Python."""
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCH = os.path.join(ROOT, "carrier-board.kicad_sch")
KICAD_CLI = "/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli"


def test_erc_clean():
    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, "erc.json")
        r = subprocess.run([KICAD_CLI, "sch", "erc", "--format", "json", "--severity-all",
                            "--exit-code-violations", "-o", out, SCH], capture_output=True, text=True)
        assert os.path.exists(out), "kicad-cli erc failed: %s" % r.stderr[-500:]
        with open(out) as f:
            rep = json.load(f)
    viol = [v for sheet in rep.get("sheets", []) for v in sheet.get("violations", [])]
    print("     ERC: %d violations, kicad-cli exit code %d" % (len(viol), r.returncode))
    assert not viol, "ERC violations: %s" % [v.get("description") for v in viol[:3]]
    assert r.returncode == 0, "kicad-cli erc exit code %d" % r.returncode


def main():
    failed = 0
    for t in (test_erc_clean,):
        try:
            t()
            print("ok   %s" % t.__name__)
        except Exception as e:
            failed += 1
            print("FAIL %s: %s" % (t.__name__, e))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
