KICAD_CLI ?= /Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli
KICAD_PY  ?= /Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/Current/bin/python3
MAGICK    ?= /opt/homebrew/bin/magick

.PHONY: lib sch erc render check clean

# Write lib/ (dev board symbols and footprints) and the project library tables.
lib:
	$(KICAD_PY) -I scripts/gen_lib.py

# Write the schematic, then the netlist Task 4 builds the board from.
sch:
	$(KICAD_PY) -I scripts/gen_schematic.py
	mkdir -p build
	$(KICAD_CLI) sch export netlist --format kicadxml -o build/netlist.xml carrier-board.kicad_sch

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
