# EmberHeat carrier board

A KiCad board that holds an ESP32-C6 and an ESP32-H2 dev board, seven relay outputs for zone valves, and a USB-C power input. It mounts on a DIN rail with two clips.

Status: parts chosen (`bom.csv`, `part-checks.md`) and schematic drawn (`carrier-board.kicad_sch`, rendered in `renders/schematic.png`). The board is not laid out yet.

## Files

- `bom.csv`: one row per distinct part, with the exact KiCad symbol and footprint names.
- `part-checks.md`: datasheet figures that justify each part, and the drive arithmetic.
- `scripts/check_bom.py`: checks the bill of materials.
- `lib/`: dev board symbols and footprints and the DIN clip holes, written by `scripts/gen_lib.py` (`make lib`). `sym-lib-table` and `fp-lib-table` name it `emberheat`.
- `scripts/expected.py`: the pin lists and dimensions the checks compare the library against, written by hand.
- `scripts/check_lib.py`: checks the library, including that the sockets are not mirrored or reversed.
- `carrier-board.kicad_sch`: the schematic, written by `scripts/gen_schematic.py` (`make sch`, which also writes `build/netlist.xml`). Do not edit it by hand.
- `scripts/check_netlist.py`: checks the netlist against `expected.NETS`, the firmware's relay GPIOs and the datasheet pinouts.
- `make erc`: KiCad's electrical rules check, all severities; it must report 0 violations. No ERC exclusions are used.
- `make render`: writes `renders/schematic.png` (KiCad SVG export, cleaned by `scripts/svg_for_magick.py`, then ImageMagick).

## Checks

```
make check
```

Runs every `scripts/check_*.py`. Needs KiCad 9 at `/Applications/KiCad/KiCad.app`.

## Power

Use a 5 V USB-C supply rated 3 A. A supply that offers only 0.5-0.9 A will sag when several relays close and reset the C6, which opens the relays. That is safe but a nuisance.
