#!/usr/bin/env python3
"""Copy a board without its copper pours, for the track-only renders
(`make render-board`): with the pours drawn, tracks on the same layer are
hard to see. Usage: without_pours.py IN.kicad_pcb OUT.kicad_pcb.
Run with the KiCad Python (needs pcbnew)."""
import sys


def main(src, dst):
    import pcbnew
    board = pcbnew.LoadBoard(src)
    for z in [z for z in board.Zones() if not z.GetIsRuleArea()]:
        board.Remove(z)
    pcbnew.SaveBoard(dst, board)
    return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]))
