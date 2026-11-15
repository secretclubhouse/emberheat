KICAD_CLI ?= /Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli
KICAD_PY  ?= /Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/Current/bin/python3
MAGICK    ?= /opt/homebrew/bin/magick

.PHONY: lib sch board erc render render-board check clean

# Write lib/ (dev board symbols and footprints) and the project library tables.
lib:
	$(KICAD_PY) -I scripts/gen_lib.py

# Write the schematic, then the netlist Task 4 builds the board from.
sch:
	$(KICAD_PY) -I scripts/gen_schematic.py
	mkdir -p build
	$(KICAD_CLI) sch export netlist --format kicadxml -o build/netlist.xml carrier-board.kicad_sch

# Place every part on the circuit board, draw the outline and route the zone
# side (24 V AC); the logic side is left for Task 5's router.
board: build/netlist.xml
	$(KICAD_PY) -I scripts/gen_board.py

build/netlist.xml:
	$(MAKE) sch

# Electrical rules check: fails on any error or warning.
erc:
	mkdir -p build
	$(KICAD_CLI) sch erc --severity-all --exit-code-violations -o build/erc.rpt carrier-board.kicad_sch

# renders/schematic.png from the schematic.
render:
	rm -rf build/svg
	mkdir -p build/svg renders
	$(KICAD_CLI) sch export svg -o build/svg carrier-board.kicad_sch
	$(KICAD_PY) -I scripts/svg_for_magick.py build/svg/carrier-board.svg build/svg/clean.svg
	$(MAGICK) -density 300 build/svg/clean.svg -background white -flatten -depth 8 renders/schematic.png

# renders/board-*.png from the board: 3D views of both sides, and the front
# copper, silkscreen and outline as a flat plot (white background).
render-board:
	rm -rf build/svg-board
	mkdir -p build/svg-board renders
	$(KICAD_CLI) pcb render --side top --width 2400 --height 1700 --quality high --background opaque -o renders/board-top.png carrier-board.kicad_pcb 2>&1 | grep -v Fontconfig || true
	$(KICAD_CLI) pcb render --side bottom --width 2400 --height 1700 --quality high --background opaque -o renders/board-bottom.png carrier-board.kicad_pcb 2>&1 | grep -v Fontconfig || true
	$(KICAD_CLI) pcb export svg --mode-single --page-size-mode 2 --exclude-drawing-sheet --layers F.Cu,F.Silkscreen,Edge.Cuts -o build/svg-board/front.svg carrier-board.kicad_pcb 2>&1 | grep -v Fontconfig || true
	$(KICAD_CLI) pcb export svg --mode-single --page-size-mode 2 --exclude-drawing-sheet --mirror --layers B.Cu,B.Silkscreen,Edge.Cuts -o build/svg-board/back.svg carrier-board.kicad_pcb 2>&1 | grep -v Fontconfig || true
	$(KICAD_PY) -I scripts/svg_for_magick.py build/svg-board/front.svg build/svg-board/front-clean.svg
	$(KICAD_PY) -I scripts/svg_for_magick.py build/svg-board/back.svg build/svg-board/back-clean.svg
	$(MAGICK) -density 400 build/svg-board/front-clean.svg -background white -flatten -depth 8 renders/board-front-layers.png
	$(MAGICK) -density 400 build/svg-board/back-clean.svg -background white -flatten -depth 8 renders/board-back-layers.png

# Run every scripts/check_*.py that exists; fail if any fails.
check:
	@status=0; \
	for s in scripts/check_*.py; do \
	  [ -e "$$s" ] || continue; \
	  echo "== $$s"; \
	  $(KICAD_PY) -I $$s || status=1; \
	done; \
	exit $$status

clean:
	rm -rf *-backups fp-info-cache build
