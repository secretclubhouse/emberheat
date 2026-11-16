KICAD_CLI ?= /Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli
KICAD_PY  ?= /Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/Current/bin/python3
MAGICK    ?= /opt/homebrew/bin/magick

.PHONY: all lib sch board route autoroute erc render render-board fab check clean enclosure-venv enclosure-geometry enclosure-check enclosure-export enclosure-render enclosure

# From scratch: library, schematic, placed board, routed board, fab outputs, checks.
all: lib sch route fab check

# Write lib/ (dev board symbols and footprints) and the project library tables.
lib:
	$(KICAD_PY) -I scripts/gen_lib.py

# Write the schematic, then the netlist Task 4 builds the board from.
sch:
	$(KICAD_PY) -I scripts/gen_schematic.py
	mkdir -p build
	rm -f build/netlist.xml
	scripts/quiet.sh $(KICAD_CLI) sch export netlist --format kicadxml -o build/netlist.xml carrier-board.kicad_sch

# Place every part on the circuit board, draw the outline and route the zone
# side (24 V AC); the logic side is left for Task 5's router.
board: build/netlist.xml
	rm -f carrier-board.kicad_pcb
	scripts/quiet.sh $(KICAD_PY) -I scripts/gen_board.py

# Route the logic side and pour ground. scripts/route.py takes the board
# `make board` wrote and writes the routed board OVER carrier-board.kicad_pcb.
# No unrouted copy is kept: `make route` always regenerates the placed board
# first, so it repeats from the same start, and route.py refuses a board that
# already has logic-side copper. After `make board` alone the board is
# unrouted and `make check` fails (test_fully_routed, test_drc_clean,
# test_ground_pour) until `make route` has run.
#
# `make route` replays the committed Freerouting session
# routing/carrier-board.ses (Freerouting does not repeat itself run to run),
# and refuses if the router's input has changed since that session was made.
# `make autoroute` runs Freerouting (tools/freerouting-2.5.0.jar, Java; see
# README.md) headless to make a new session, then routes as `make route`.
route: board
	$(KICAD_PY) -I scripts/route.py

autoroute: board
	$(KICAD_PY) -I scripts/route.py --autoroute

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

# renders/board-*.png from the board: 3D views of both sides; each copper
# layer with its silkscreen and the outline as a flat plot (white
# background), once as built (*-layers) and once without the ground pours
# so the tracks can be seen (*-tracks).
render-board:
	rm -rf build/svg-board
	mkdir -p build/svg-board renders
	scripts/quiet.sh $(KICAD_CLI) pcb render --side top --width 2400 --height 1700 --quality high --background opaque -o renders/board-top.png carrier-board.kicad_pcb
	scripts/quiet.sh $(KICAD_CLI) pcb render --side bottom --width 2400 --height 1700 --quality high --background opaque -o renders/board-bottom.png carrier-board.kicad_pcb
	scripts/quiet.sh $(KICAD_CLI) pcb export svg --mode-single --page-size-mode 2 --exclude-drawing-sheet --layers F.Cu,F.Silkscreen,Edge.Cuts -o build/svg-board/front.svg carrier-board.kicad_pcb
	scripts/quiet.sh $(KICAD_CLI) pcb export svg --mode-single --page-size-mode 2 --exclude-drawing-sheet --mirror --layers B.Cu,B.Silkscreen,Edge.Cuts -o build/svg-board/back.svg carrier-board.kicad_pcb
	$(KICAD_PY) -I scripts/svg_for_magick.py build/svg-board/front.svg build/svg-board/front-clean.svg
	$(KICAD_PY) -I scripts/svg_for_magick.py build/svg-board/back.svg build/svg-board/back-clean.svg
	$(MAGICK) -density 400 build/svg-board/front-clean.svg -background white -flatten -depth 8 renders/board-front-layers.png
	$(MAGICK) -density 400 build/svg-board/back-clean.svg -background white -flatten -depth 8 renders/board-back-layers.png
	scripts/quiet.sh $(KICAD_PY) -I scripts/without_pours.py carrier-board.kicad_pcb build/svg-board/no-pours.kicad_pcb
	scripts/quiet.sh $(KICAD_CLI) pcb export svg --mode-single --page-size-mode 2 --exclude-drawing-sheet --layers F.Cu,F.Silkscreen,Edge.Cuts -o build/svg-board/front-tracks.svg build/svg-board/no-pours.kicad_pcb
	scripts/quiet.sh $(KICAD_CLI) pcb export svg --mode-single --page-size-mode 2 --exclude-drawing-sheet --mirror --layers B.Cu,B.Silkscreen,Edge.Cuts -o build/svg-board/back-tracks.svg build/svg-board/no-pours.kicad_pcb
	$(KICAD_PY) -I scripts/svg_for_magick.py build/svg-board/front-tracks.svg build/svg-board/front-tracks-clean.svg
	$(KICAD_PY) -I scripts/svg_for_magick.py build/svg-board/back-tracks.svg build/svg-board/back-tracks-clean.svg
	$(MAGICK) -density 400 build/svg-board/front-tracks-clean.svg -background white -flatten -depth 8 renders/board-front-tracks.png
	$(MAGICK) -density 400 build/svg-board/back-tracks-clean.svg -background white -flatten -depth 8 renders/board-back-tracks.png

# fab/: Gerbers (both copper layers, both masks, both silkscreens, outline),
# Excellon drill files (plated and unplated apart), a zip of those for the
# board house, the 1:1 print template and the schematic as PDF. fab/ is
# committed so the board can be ordered without KiCad; run `make fab` after
# any change to the board. fab/source.sha256 records which board it came
# from; fab/print-template.source.sha256 records the board the template was
# plotted from; check_board.py compares both.
#
# print-template.pdf is a copy of the board moved to the middle of an A4
# landscape page (the board itself sits at the page's corner, where a printer
# would cut it off), with a 100 mm ruler added by scripts/print_template.py,
# plotted at scale 1 by kicad-cli: the pad openings (solder mask), front silkscreen
# (which carries the dev board outlines) and the outline, black and white,
# with drill holes drawn at their actual size.
fab:
	rm -rf fab
	mkdir -p fab
	scripts/quiet.sh $(KICAD_CLI) pcb export gerbers --layers F.Cu,B.Cu,F.Mask,B.Mask,F.Silkscreen,B.Silkscreen,Edge.Cuts -o fab carrier-board.kicad_pcb
	scripts/quiet.sh $(KICAD_CLI) pcb export drill --format excellon --excellon-separate-th --excellon-units mm -o fab/ carrier-board.kicad_pcb
	rm -f fab/*.gbrjob
	cd fab && zip -q -X carrier-board-gerbers.zip *.gtl *.gbl *.gts *.gbs *.gto *.gbo *.gm1 *.drl
	mkdir -p build
	rm -f build/print-template.kicad_pcb
	scripts/quiet.sh $(KICAD_PY) -I scripts/print_template.py carrier-board.kicad_pcb build/print-template.kicad_pcb
	scripts/quiet.sh $(KICAD_CLI) pcb export pdf --mode-single --layers F.Mask,F.Silkscreen,Edge.Cuts --black-and-white --drill-shape-opt 2 -o fab/print-template.pdf build/print-template.kicad_pcb
	shasum -a 256 carrier-board.kicad_pcb > fab/print-template.source.sha256
	scripts/quiet.sh $(KICAD_CLI) sch export pdf -o fab/schematic.pdf carrier-board.kicad_sch
	shasum -a 256 carrier-board.kicad_pcb > fab/source.sha256

# Run every scripts/check_*.py that exists; fail if any fails.
check:
	@status=0; \
	for s in scripts/check_*.py; do \
	  [ -e "$$s" ] || continue; \
	  echo "== $$s"; \
	  $(KICAD_PY) -I $$s || status=1; \
	done; \
	exit $$status

# Enclosure (enclosure/): CadQuery runs in its own Python 3.12 virtual
# environment (tools/cad-venv, not committed); only the geometry export needs
# KiCad's Python.
CAD_PY ?= tools/cad-venv/bin/python

enclosure-venv:
	uv venv --python 3.12 tools/cad-venv
	uv pip install --python $(CAD_PY) -r enclosure/requirements.txt

# enclosure/board-geometry.json from the board; make check fails if it is stale.
enclosure-geometry:
	scripts/quiet.sh $(KICAD_PY) -I scripts/export_board_geometry.py

enclosure-check:
	$(CAD_PY) enclosure/check_enclosure.py

# enclosure/out/ (STEP and STL in print orientation, hashes.txt) from the
# board geometry and the parts.
enclosure-export:
	$(CAD_PY) enclosure/export_enclosure.py

# renders/enclosure-*.png, shaded.
enclosure-render:
	$(CAD_PY) enclosure/render_enclosure.py

# Everything: geometry from the board, exports, images, then the checks.
enclosure:
	$(MAKE) enclosure-geometry
	$(MAKE) enclosure-export
	$(MAKE) enclosure-render
	$(MAKE) enclosure-check

clean:
	rm -rf *-backups fp-info-cache build
