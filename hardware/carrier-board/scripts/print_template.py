#!/usr/bin/env python3
"""Copy the board for the 1:1 print template (`make fab`): shift everything
so the board sits in the middle of the A4 sheet, clear of a printer's
unprintable margin, and add a ruler on the front silkscreen. The original is
at the top left corner of the sheet, where a printer would cut it off.
Only the copy is moved; nothing changes in carrier-board.kicad_pcb.
Usage: print_template.py IN.kicad_pcb OUT.kicad_pcb.
Run with the KiCad Python (needs pcbnew)."""
import sys

PAGE_W, PAGE_H = 297.0, 210.0   # mm, A4 landscape (the board's page setting)
RULER_MM = 100.0


def main(src, dst):
    import pcbnew
    mm = pcbnew.FromMM
    board = pcbnew.LoadBoard(src)
    box = board.GetBoardEdgesBoundingBox()
    w, h = pcbnew.ToMM(box.GetWidth()), pcbnew.ToMM(box.GetHeight())
    assert w < PAGE_W - 20 and h < PAGE_H - 60, "board does not fit the page with margins"
    left, top = (PAGE_W - w) / 2, 15.0           # 15 mm from the top edge
    v = pcbnew.VECTOR2I(mm(left - pcbnew.ToMM(box.GetX())), mm(top - pcbnew.ToMM(box.GetY())))
    items = list(board.GetFootprints()) + list(board.GetTracks()) + list(board.GetDrawings()) \
        + list(board.Zones())
    for it in items:
        it.Move(v)

    def line(x1, y1, x2, y2, width=0.2):
        s = pcbnew.PCB_SHAPE(board)
        s.SetShape(pcbnew.SHAPE_T_SEGMENT)
        s.SetLayer(pcbnew.F_SilkS)
        s.SetStart(pcbnew.VECTOR2I(mm(x1), mm(y1)))
        s.SetEnd(pcbnew.VECTOR2I(mm(x2), mm(y2)))
        s.SetWidth(mm(width))
        board.Add(s)

    def text(x, y, msg, size=2.0):
        t = pcbnew.PCB_TEXT(board)
        t.SetLayer(pcbnew.F_SilkS)
        t.SetText(msg)
        t.SetPosition(pcbnew.VECTOR2I(mm(x), mm(y)))
        t.SetTextSize(pcbnew.VECTOR2I(mm(size), mm(size)))
        t.SetTextThickness(mm(0.25))
        t.SetHorizJustify(pcbnew.GR_TEXT_H_ALIGN_LEFT)
        board.Add(t)

    # A 100 mm ruler with a tick every 10 mm, below the board. The board's own
    # width is the second thing to measure.
    y0 = top + h + 18
    x0 = left
    line(x0, y0, x0 + RULER_MM, y0)
    for i in range(0, int(RULER_MM) + 1, 10):
        line(x0 + i, y0 - 2, x0 + i, y0 + 2)
        text(x0 + i - 1.5, y0 + 5, str(i), 2.0)
    text(x0 + RULER_MM + 4, y0 + 1, "mm", 2.0)
    text(x0, y0 - 9, "PRINT AT 100 % (ACTUAL SIZE, NOT FIT TO PAGE). RULER MUST MEASURE 100 mm.", 2.4)
    text(x0, y0 + 11, "Board outline: %.0f x %.0f mm. Measure the width: it should read 168 mm." % (w, h), 2.4)
    text(x0, y0 + 16, "Dev boards sit with their pin rows on the round pads; USB ends face the notch.", 2.4)
    pcbnew.SaveBoard(dst, board)
    return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]))
