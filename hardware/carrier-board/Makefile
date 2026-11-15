KICAD_CLI ?= /Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli
KICAD_PY  ?= /Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/Current/bin/python3

.PHONY: check clean

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
	rm -rf *-backups fp-info-cache
