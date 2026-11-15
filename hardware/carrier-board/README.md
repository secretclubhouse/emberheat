# EmberHeat carrier board

A KiCad board that holds an ESP32-C6 and an ESP32-H2 dev board, seven relay outputs for zone valves, and a USB-C power input. It mounts on a DIN rail with two clips.

Status: parts chosen (`bom.csv`, `part-checks.md`), schematic drawn (`carrier-board.kicad_sch`, rendered in `renders/schematic.png`), board placed and routed (`carrier-board.kicad_pcb`, rendered in `renders/board-*.png`). KiCad's design rule check passes with no violations, no unconnected items and no schematic parity problems.

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
- `carrier-board.kicad_pcb`: the board. `make board` (`scripts/gen_board.py`) places every part, draws the outline and silkscreen and routes the zone side (24 V AC, locked tracks); `make route` (`scripts/route.py`) routes the logic side and adds the ground pours, writing over the same file. Do not edit it by hand.
- `carrier-board.kicad_dru`: custom design rules: 3.0 mm between zone-side and logic-side copper; no logic-side track, via or pour in the zone strip (the rule area named `zone_strip`); the dev boards' outlines on the silkscreen may meet the board edge.
- `routing/carrier-board.ses`: the Freerouting session `make route` replays, and `routing/carrier-board.dsn.sha256`, the router input it was made from.
- `make render-board`: writes `renders/board-*.png`: 3D views of both sides, each copper layer with its silkscreen as built (`*-layers.png`), and again without the ground pours so the tracks can be seen (`*-tracks.png`).
- `scripts/check_board.py`: checks the board against the netlist and the design, and runs KiCad's design rule check.

## Routing

The logic side is routed by [Freerouting](https://github.com/freerouting/freerouting), run headless by `scripts/route.py`.

- Version 2.5.0, from https://github.com/freerouting/freerouting/releases/download/v2.5.0/freerouting-2.5.0.jar
- SHA-256 `f6f51bb02245e8e717f9359bd260cc9c5c0b1bc0acc8b7cb2cd5b8ffeb5de3c7` (route.py refuses any other file)
- Put it at `tools/freerouting-2.5.0.jar` (`tools/` is not committed). It needs Java (tested with OpenJDK 27).

Freerouting does not give the same routing twice from the same input, so the session it made is committed and `make route` replays it: no Java needed, the same board every time. `make autoroute` runs Freerouting three times (headless, at most 100 passes, 15 minutes each), keeps the session with the fewest vias, and then routes as `make route`. Run it after any change to placement, net classes, rules or the pre-routed tracks; `make route` refuses a session made from a different router input.

What route.py does around the router:

- Zone strip: the router sees a keepout over the strip, less a 0.6 mm moat round the zone copper (too narrow for any track or via), so no logic track can enter it. The board itself keeps a plain named rule area there (`zone_strip`) and the rule in `carrier-board.kicad_dru` forbids logic tracks, vias and pours in it: a keepout on the board would also flag the zone tracks.
- Board edges: 0.5 mm track and via keepouts, as Freerouting knows only the 0.25 mm net clearance. No vias under the relays.
- USB-C VBUS (net VIN) is drawn by hand: its pads are too narrow for a 1.0 mm track, so it leaves each pad at 0.5 mm for 1.07 mm and then runs at 1.0 mm (A9 down the front to J2, B9 through one via down the back), and on to the fuse.
- Ground pours on both layers below the zone line, with thermal reliefs (0.5 mm gap and spokes) on through-hole pads for hand soldering. A thermal relief whose spokes reach only an island of fill that connects the pad to nothing is a design rule error; route.py removes such islands with small pour keepouts (`no_pour_*` rule areas), or turns a pad's spokes 45 degrees where fewer than two reach the pour.

## Checks

```
make check
```

Runs every `scripts/check_*.py`. Needs KiCad 9 at `/Applications/KiCad/KiCad.app`.

## Power

Use a 5 V USB-C supply rated 3 A. A supply that offers only 0.5-0.9 A will sag when several relays close and reset the C6, which opens the relays. That is safe but a nuisance.
