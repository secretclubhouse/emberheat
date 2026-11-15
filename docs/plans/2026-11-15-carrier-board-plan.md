# EmberHeat Carrier Board (rev 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a checked, orderable KiCad design for the DIN-rail carrier board that holds the ESP32-C6 and ESP32-H2 dev boards and seven relay outputs.

**Architecture:** The schematic is the source of truth and is written by a Python generator (`gen_schematic.py`). KiCad exports its netlist; a second generator (`gen_board.py`, run with KiCad's bundled Python and `pcbnew`) builds the board from that netlist, places parts, and routes the zone side. Freerouting routes the logic side. Independent check scripts compare the netlist and the board against a hand-written table of what the design requires, and KiCad's own ERC, DRC and parity checks run on top.

**Tech Stack:** KiCad 9.0.7 (`kicad-cli`, bundled Python 3.9 with `pcbnew`), Python standard library only, Freerouting (Java, downloaded once), `make`.

**Spec:** `docs/plans/2026-11-15-carrier-board-design.md`

## Global Constraints

- Everything lives in `hardware/carrier-board/`. Nothing is copied from `~/Code/emberheat-pcb`. No firmware file is changed.
- Tools: `KICAD_CLI=/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli`, `KICAD_PY=/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/Current/bin/python3`. All scripts run with `KICAD_PY` and use only the standard library plus `pcbnew`.
- Relay GPIOs, zones 1–7: 0, 1, 2, 3, 10, 11, 22. Active-low: GPIO LOW closes the relay.
- Inter-board nets: C6 GPIO18 → H2 GPIO0, C6 GPIO19 ← H2 GPIO1, C6 GPIO6 → H2 GPIO2, C6 GPIO7 ← H2 GPIO3, C6 GPIO23 → H2 RST, C6 3V3 → jumper → H2 3V3, grounds joined.
- C6 strapping pins (4, 5, 8, 9, 15), USB pins (12, 13), RST and both NC pins connect to nothing.
- Hand-solderable: through-hole parts, or 0805 and larger. The only surface-mount part allowed is the power-only USB-C socket.
- Two-layer board, 1.6 mm, all parts on the front.
- At least 3.0 mm between any zone-side copper and any logic-side copper on both layers; no copper pour in the zone-side strip.
- Zone-side nets are exactly `Z1_A`…`Z7_A` and `Z1_B`…`Z7_B`. Every other net is logic-side.
- Track widths: zone-side 1.5 mm, `+5V`/`GND`/`+3V3` 1.0 mm, signals 0.4 mm. Minimum clearance 0.25 mm.
- Relay: Finder 34.51.7.005.5010 (AgNi + Au contacts, minimum load 5 V / 2 mA, 6 A 250 V AC, 5 V 170 mW coil, 8 mm coil-to-contact clearance; figures from Finder's 34 series datasheet, edition II-2026). KiCad symbol `Relay:FINDER-34.51`, footprint `Relay_THT:Relay_SPDT_Finder_34.51_Vertical` (pads A1, A2 coil; 11 common, 14 normally open, 12 normally closed).
- Commit messages follow the repo style (`feat:`, `docs:`). Work on branch `feat/carrier-board`.

## Review Focus

Mistakes that pass ERC and DRC but ruin the board, most likely first. Each has a named check in the task that owns it.

1. A dev board socket footprint mirrored or reversed, so every pin lands on the wrong signal. Check `test_socket_orientation` (Task 2) and Josh's paper print.
2. Zone terminal wired to the relay's normally-closed contact, so a valve is open whenever the relay is off. Check `test_zone_uses_normally_open` (Task 3).
3. Flyback diode or zone light fitted backwards in the design. Check `test_diode_polarity` (Task 3).
4. Transistor symbol pins not matching the bought part's legs. Check `test_transistor_pinout` (Task 3).
5. USB-C supply never turns on because the two CC pins share one resistor or have none. Check `test_usb_cc_resistors` (Task 3).

---

## File Structure

```
hardware/carrier-board/
  README.md                 what it is, assembly, wiring, pre-order checks
  Makefile                  targets: lib, sch, board, route, check, fab, clean
  bom.csv                   one row per distinct part, orderable numbers
  part-checks.md            datasheet figures that justify each part
  carrier-board.kicad_pro   project, net classes
  carrier-board.kicad_sch   generated
  carrier-board.kicad_pcb   generated, then routed
  carrier-board.kicad_dru   custom rule: 3 mm zone/logic separation
  sym-lib-table, fp-lib-table
  lib/emberheat.kicad_sym   dev board symbols
  lib/emberheat.pretty/     dev board and DIN clip footprints
  scripts/sexp.py           read and write KiCad s-expressions
  scripts/expected.py       what the design requires, written by hand from the spec
  scripts/gen_lib.py        writes lib/
  scripts/gen_schematic.py  writes the schematic
  scripts/gen_board.py      netlist → placed board with zone side routed
  scripts/route.py          Freerouting round trip, ground pour
  scripts/check_bom.py  check_lib.py  check_netlist.py  check_board.py
  renders/                  PNG images of schematic and board, for review
  fab/                      Gerbers, drill files, 1:1 print template (generated)
```

`expected.py` is never imported by a generator. It is the independent statement of the spec that the checks compare against.

---

### Task 1: Project folder, parts and bill of materials

**Files:**
- Create: `hardware/carrier-board/{README.md,Makefile,.gitignore,bom.csv,part-checks.md}`, `scripts/check_bom.py`

**Interfaces:**
- Produces: `bom.csv` with columns `refs,qty,value,manufacturer,mpn,symbol,footprint,note`. `refs` is a space-separated list. Later tasks take every symbol and footprint name from this file.
- Produces: `make check` (runs every `scripts/check_*.py` that exists; exits non-zero on any failure).

Reference designators, fixed for all tasks (n = zone 1–7):

| Ref | Part | Value / requirement |
|---|---|---|
| A1 | C6 dev board socket pair | 2 × 16-way 2.54 mm female strip |
| A2 | H2 dev board socket pair | 2 × 15-way 2.54 mm female strip |
| K*n* | Relay | Finder 34.51.7.005.5010 |
| U*n* | Optocoupler, DIP-4 | current transfer ratio ≥ 50 % at 5 mA (candidate LTV-817) |
| Q*n* | NPN, TO-92 | 2N3904 |
| D*n* | Flyback diode | 1N4148 |
| D1*n* | Zone light | 3 mm LED |
| R*n*1, R*n*2, R*n*3, R*n*4 | 330 Ω, 1 kΩ, 10 kΩ, 1 kΩ | opto LED, base, base pull-down, zone light |
| RV*n* | Varistor across contact | working voltage ≥ 30 V AC, clamping voltage < 400 V (candidate Littelfuse V56ZA2P) |
| J1*n* | Zone terminal | Phoenix MSTBA 2,5/2-G-5,08 header and MSTB 2,5/2-ST-5,08 plug |
| J1 | USB-C, power only | GCT USB4125-GF-A |
| J2 | 5 V screw terminal | Phoenix MKDS 1,5/2-5,08 |
| F1 | Resettable fuse | hold current 1.85–2.5 A, radial |
| C1, C2 | 470 µF ≥ 10 V radial, 100 nF | |
| D8, R3 | Power light, 1 kΩ | |
| R1, R2 | 5.1 kΩ | one per CC pin |
| R4 | 10 kΩ | pull-up on C6 GPIO18 |
| R5 | 4.7 kΩ | 1-Wire pull-up |
| JP1 | 2-pin header and shunt | H2 3.3 V feed |
| J3 | 3-pin header | 1-Wire: 3V3, data, GND |
| J4 | 5 pads, unpopulated | 3V3, GPIO20, TX, RX, GND |
| H1–H4 | M3 corner holes | |
| H5… | DIN clip holes | per chosen clip |

- [ ] **Step 1: Write `scripts/check_bom.py`** with these tests (plain functions, a `main()` that runs all and exits 1 on failure):
  - `test_every_ref_listed`: the refs in the table above (expanded for n = 1–7) all appear exactly once in `bom.csv`.
  - `test_symbols_and_footprints_exist`: every `symbol` (`Lib:Name`) is found in KiCad's `SharedSupport/symbols/Lib.kicad_sym` or `lib/emberheat.kicad_sym`; every `footprint` loads with `pcbnew.FootprintLoad`. Rows whose library is `emberheat` are skipped until `lib/` exists.
  - `test_orderable`: `manufacturer` and `mpn` are non-empty for every row except H1–H4 and J4.
- [ ] **Step 2: Run it.** `make check` → FAIL, `bom.csv` missing.
- [ ] **Step 3: Choose and record the open parts.** For each, open the manufacturer datasheet and write the figures into `part-checks.md`:
  - Optocoupler: current transfer ratio at 5 mA; pinout matches `Isolator:PC817`.
  - Varistor: maximum continuous AC voltage, clamping voltage, lead pitch and disc size (pick the matching `Varistor:RV_Disc_…` footprint).
  - Fuse: hold and trip current, lead pitch (pick a footprint from `Fuse.pretty` whose pitch matches).
  - DIN clip: a screw-on clip for 35 mm rail with a published dimensioned drawing, stocked by a US distributor. Record screw size, hole positions and the part's footprint on the board. Two clips are used.
  - Drive arithmetic, with the chosen parts' figures: opto LED current at 3.3 V through 330 Ω; transistor base current through 1 kΩ; coil current (34 mA) plus zone light. The transistor must be saturated with a gain of 20 or less.
  - Dev board power: from Espressif's ESP32-C6-DevKitC-1 schematic, confirm a diode sits between each USB port's supply and the 5V header pin. From the ESP32-H2-DevKitM-1 schematic, confirm the 3V3 header pin is the regulator output. If either is not so, stop and report: the spec's power section depends on it.
- [ ] **Step 4: Write `bom.csv`, `Makefile`, `.gitignore`** (ignore `*-backups/`, `*.lck`, `.DS_Store`, `fp-info-cache`, `tools/`) **and a README stub.**
- [ ] **Step 5: Run.** `make check` → PASS (emberheat-library rows skipped).
- [ ] **Step 6: Commit.** `feat: carrier board project folder and bill of materials`

### Task 2: Dev board symbols and footprints

**Files:**
- Create: `scripts/sexp.py`, `scripts/expected.py`, `scripts/gen_lib.py`, `scripts/check_lib.py`, `lib/` (generated), `sym-lib-table`, `fp-lib-table`

**Interfaces:**
- Produces: `sexp.loads(text) -> list`, `sexp.dumps(tree) -> str` (nested lists; atoms are `str`, quoted strings are a `sexp.Q` subclass of `str`).
- Produces: symbols `emberheat:ESP32-C6-DevKitC-1` (32 pins) and `emberheat:ESP32-H2-DevKitM-1` (30 pins); footprints of the same names plus `emberheat:DIN_Clip` holes. Pad numbers are `J1_1`…`J1_16`, `J3_1`…`J3_16` (15 for the H2), named after the dev boards' own header names and unrelated to the carrier's connector refs J1–J4; symbol pin numbers match pad numbers and pin names are the signal names below.
- Produces in `expected.py`: `C6_PINS`, `H2_PINS` (`dict[str, str]`, pad number → signal name).

Footprint geometry, both boards: origin at the centre of the two rows; rows at x = −11.43 mm (J1) and x = +11.43 mm (J3); pitch 2.54 mm; pin 1 of each row at the antenna end, which points toward −y; seen from the front of the carrier. Pads: 1.7 mm round, 1.0 mm drill. Front silkscreen: board outline (25.4 mm wide; 51.8 mm long for the C6, 48.26 mm for the H2), the word `ANTENNA` at the antenna end and `USB` at the other. A rule area under the last 7 mm at the antenna end forbids copper on both layers.

Signal names, pin 1 first:
- C6 J1: 3V3, RST, GPIO4, GPIO5, GPIO6, GPIO7, GPIO0, GPIO1, GPIO8, GPIO10, GPIO11, GPIO2, GPIO3, 5V, GND, NC
- C6 J3: GND, GPIO16_TX, GPIO17_RX, GPIO15, GPIO23, GPIO22, GPIO21, GPIO20, GPIO19, GPIO18, GPIO9, GND, GPIO13, GPIO12, GND, NC
- H2 J1: 3V3, RST, GPIO0, GPIO1, GPIO2, GPIO3, GPIO13, GPIO14, GPIO4, GPIO5, NC, VBAT, GND, 5V, GND
- H2 J3: GND, TX, RX, GPIO10, GPIO11, GPIO25, GPIO12, GPIO8, GPIO22, GND, GPIO9, GND, GPIO27, GPIO26, GND

- [ ] **Step 1: Confirm the tables.** Open Espressif's user guides for both boards (header tables and dimension drawings). Confirm each list above, the 22.86 mm row spacing, the board lengths, and that J1 is on the left with the antenna at the top when viewed from the component side. Record the document versions in `part-checks.md`. A mismatch is corrected here and in the spec, and reported.
- [ ] **Step 2: Write `expected.py` (`C6_PINS`, `H2_PINS`) and `check_lib.py`:**
  - `test_pad_counts`: 32 and 30 pads.
  - `test_row_geometry`: row x positions ±11.43, consecutive pads 2.54 mm apart.
  - `test_socket_orientation`: pad `J1_1` is at negative x and is the most negative y of its row; the antenna rule area lies beyond `J1_1` in −y; pad names follow `expected.py`.
  - `test_symbol_matches_footprint`: every symbol pin number has a pad and its pin name equals `expected.py`.
  - `test_sexp_round_trip`: `dumps(loads(x))` re-parses equal for KiCad's own `Relay.kicad_sym`.
- [ ] **Step 3: Run.** `make check` → FAIL, `lib/` missing.
- [ ] **Step 4: Write `sexp.py` and `gen_lib.py`; run `make lib`.**
- [ ] **Step 5: Run.** `make check` → PASS, including `check_bom.py` with no skipped rows.
- [ ] **Step 6: Commit.** `feat: carrier board dev board symbols and footprints`

### Task 3: Schematic

**Files:**
- Create: `scripts/gen_schematic.py`, `scripts/check_netlist.py`, `carrier-board.kicad_pro`, `carrier-board.kicad_sch` (generated), `renders/schematic.png`
- Modify: `scripts/expected.py`

**Interfaces:**
- Consumes: `bom.csv` (symbol, footprint, value per ref), `lib/`, `sexp`.
- Produces: `make sch` writes the schematic, then `build/netlist.xml` via `kicad-cli sch export netlist --format kicadxml`. Task 4 reads that file.
- Produces in `expected.py`: `NETS: dict[str, set[str]]`, net name → set of `"REF.PAD"`; and `UNCONNECTED: set[str]`.

Net names and connections (n = 1–7, GPIO for zone n from Global Constraints):

| Net | Connects |
|---|---|
| `+5V` | F1 output, C1+, C2, A1 5V, K*n* A1, D*n* cathode, D1*n* anode, U*n* collector, R3 |
| `PWR_LED` | R3, D8 anode (D8 cathode on `GND`) |
| `VIN` | J1 VBUS, J2 pin 1, F1 input |
| `GND` | all GND pins of A1 and A2, J1 GND and shell, J2 pin 2, C1−, C2, Q*n* emitter, R*n*3, R1, R2, D8 cathode, J3, J4 |
| `+3V3` | A1 3V3, R*n*1, R4, R5, JP1 pin 1, J3, J4 |
| `H2_3V3` | JP1 pin 2, A2 3V3 |
| `CC1`, `CC2` | J1 CC1–R1, J1 CC2–R2 |
| `Zn_N` | A1 GPIO for zone n, U*n* LED cathode |
| `Zn_LED` | R*n*1, U*n* LED anode |
| `Zn_DRV` | U*n* emitter, R*n*2 |
| `Zn_BASE` | R*n*2, R*n*3, Q*n* base |
| `Zn_COIL` | Q*n* collector, K*n* A2, D*n* anode, R*n*4 |
| `Zn_LAMP` | D1*n* cathode, R*n*4 |
| `Zn_A`, `Zn_B` | K*n* 11 – J1*n* pin 1 – RV*n*; K*n* 14 – J1*n* pin 2 – RV*n* |
| `UART_C6_TX` | A1 GPIO18, A2 GPIO0, R4 |
| `UART_H2_TX` | A1 GPIO19, A2 GPIO1 |
| `H2_RTS`, `H2_CTS` | A1 GPIO6 – A2 GPIO2; A1 GPIO7 – A2 GPIO3 |
| `H2_RST` | A1 GPIO23, A2 RST |
| `ONEWIRE` | A1 GPIO21, R5, J3 pin 2 |
| `SPARE_IO20`, `SPARE_TX`, `SPARE_RX` | A1 GPIO20, GPIO16_TX, GPIO17_RX to J4 |

Unconnected, each with a no-connect flag: K*n* 12; A1 RST, GPIO4, GPIO5, GPIO8, GPIO9, GPIO15, GPIO12, GPIO13, both NC; every A2 pin not named above.

Layout of the drawing: one A2 sheet. Left: power input block. Centre: A1 and A2 with the inter-board nets as wires. Right: the seven relay channels, each the same wired drawing stamped by one function, reading left to right GPIO → optocoupler → transistor → relay → terminal, with the channel's zone number as a text heading. Wires inside a block; net labels only between blocks. A dashed box and the text `ZONE WIRING — 24 V AC` around the contacts, varistors and terminals.

- [ ] **Step 1: Write `expected.NETS` and `expected.UNCONNECTED` from the table above**, by hand, pad by pad, using the pad numbers of the symbols named in `bom.csv`.
- [ ] **Step 2: Write `check_netlist.py`** (reads `build/netlist.xml`):
  - `test_nets_match_expected`: every net's pad set equals `expected.NETS`; no extra nets; no pad in two nets.
  - `test_unconnected`: every pad in `UNCONNECTED` is in no net.
  - `test_relay_gpios_match_firmware`: the relay GPIO list read out of `emberheat-esp32/src/main.rs` equals `[0, 1, 2, 3, 10, 11, 22]` and equals the A1 pads on `Z1_N`…`Z7_N`.
  - `test_zone_uses_normally_open`: for every n, J1*n* connects to K*n* pads 11 and 14 and K*n* pad 12 is unconnected.
  - `test_diode_polarity`: D*n* cathode pad is on `+5V`, anode on `Zn_COIL`; D1*n* anode is on `+5V`, cathode on `Zn_LAMP`; D8 cathode on `GND`.
  - `test_transistor_pinout`: for the footprint and symbol in `bom.csv`, Q*n*'s emitter pad number equals the emitter leg position in the datasheet pinout recorded in `part-checks.md`, and is on `GND`.
  - `test_usb_cc_resistors`: `CC1` and `CC2` are different nets, each with exactly one 5.1 kΩ resistor whose other pad is on `GND`.
  - `test_no_strapping_pins_used`: A1 pads for GPIO 4, 5, 8, 9, 15, 12, 13 are in no net.
  - `test_zone_nets_isolated`: no part other than K*n*, RV*n*, J1*n* touches a `Zn_A`/`Zn_B` net.
- [ ] **Step 3: Run.** `make sch check` → FAIL, no schematic.
- [ ] **Step 4: Write `gen_schematic.py`.** It needs: `place(lib_id, ref, at, rot) -> Placed` with `Placed.pin(number) -> (x, y)` computed from the library symbol; `wire(*points)`; `label(net, at)`; `no_connect(at)`; power symbols from KiCad's `power` library. All coordinates on the 1.27 mm grid. Embed each used library symbol in the schematic's `lib_symbols`, as KiCad does.
- [ ] **Step 5: Run.** `make sch check` → PASS. Then `kicad-cli sch erc --severity-all --exit-code-violations carrier-board.kicad_sch` → 0 errors, 0 warnings. Any exclusion is listed with its reason in the README.
- [ ] **Step 6: Render and read it.** `kicad-cli sch export svg`, convert to `renders/schematic.png` (`magick`), open the image. Follow one relay channel from GPIO to terminal, the power input, and the inter-board wires. Fix overlapping text or wires that cross through symbols.
- [ ] **Step 7: Commit.** `feat: carrier board schematic`

### Task 4: Board outline, placement and zone-side routing

**Files:**
- Create: `scripts/gen_board.py`, `scripts/check_board.py`, `carrier-board.kicad_dru`, `carrier-board.kicad_pcb` (generated)

**Interfaces:**
- Consumes: `build/netlist.xml`, `bom.csv`, `lib/`, the DIN clip dimensions in `part-checks.md`.
- Produces: `make board` writes an unrouted-logic board. Task 5's `route.py` opens it.
- Produces in `check_board.py`: `zone_side_items(board)` and `logic_side_items(board)`, reused in Task 5.

Board frame: x to the right, y downward, origin at the top-left corner, wall orientation as mounted. Target 160 × 100 mm; may grow to 170 × 115 mm if placement needs it.

- **Top strip (zone side), y from 0:** the seven zone terminals along the top edge, plug openings facing up, Z1 at the left, evenly pitched. Directly below each, its relay with the long axis vertical and contact pads toward the top, and its varistor between relay and terminal. A silkscreen line and the text `ZONE WIRING 24 V AC` mark the lower limit of the strip, which falls in the 9 mm gap between each relay's contact pads and coil pads.
- **Driver band, below the strip:** each channel's optocoupler, transistor, diode, resistors and zone light in the column under its relay. Zone lights in one row, each labelled `Z1`…`Z7`.
- **Bottom band:** a notch is cut into the bottom edge between the two dev boards, as deep as the bottom band. The C6 sits bottom-left, lying horizontally, antenna end overhanging the left board edge by 6 mm, USB end flush with the left side of the notch. The H2 sits bottom-right, mirrored: antenna overhanging the right edge, USB end at the right side of the notch. The notch is at least 60 mm wide so both USB cables can be plugged in.
- **Power input:** J1 (USB-C) and J2 on the left edge in the driver band, opening leftward, then F1, C1, C2, D8 beside them. C1 lies within 30 mm of the relay coil row.
- **Middle, above the notch:** JP1 with silkscreen `H2 3V3 — REMOVE BEFORE USING H2 USB`, J3 labelled `1-WIRE 3V3 DQ GND`, J4 with each pad labelled.
- **Mounting:** H1–H4 M3 holes 4 mm in from the corners of the full rectangle. Two DIN clips on the back, on the vertical centre line of the rail, which runs horizontally behind the board centre; their holes clear all pads by 3 mm.
- **Zone-side tracks**, drawn by this script on the front layer, 1.5 mm wide: K*n* pad 11 → J1*n* pin 1, K*n* pad 14 → J1*n* pin 2, with RV*n* across them.
- **Silkscreen also carries:** board name, `rev 1`, date, `5V ⎓ 3A USB-C` at J1, `+5V`/`GND` at J2, relay and terminal zone numbers.
- **`carrier-board.kicad_dru`:** one rule, clearance ≥ 3.0 mm between net class `Zone` and every other net class. Net classes `Zone`, `Power`, `Default` with the widths in Global Constraints are set in `carrier-board.kicad_pro`.

- [ ] **Step 1: Write `check_board.py`:**
  - `test_all_parts_present`: every ref in the netlist has a footprint on the board with the footprint named in `bom.csv`, on the front side.
  - `test_pad_nets_match_netlist`: every pad's net equals the netlist.
  - `test_zone_separation`: the smallest copper-to-copper distance between `zone_side_items` and `logic_side_items` (pads, tracks, vias, zone fills, both layers) is ≥ 3.0 mm. Print the smallest distance and the two items.
  - `test_no_pour_in_zone_strip`: no copper zone outline intersects the zone strip.
  - `test_inside_outline`: every pad is ≥ 1.0 mm inside the outline; the dev board silkscreen outlines may cross the board edge only at their antenna ends.
  - `test_dev_board_placement`: each dev board's `USB` end lies on a notch edge (±1 mm); each antenna rule area lies wholly outside the outline or over bare board; the two dev board outlines do not overlap any other footprint's courtyard except their own sockets.
  - `test_notch_width`: ≥ 60 mm.
  - `test_terminal_order`: J11…J17 left to right along the top edge, openings at the edge.
  - `test_mounting_holes`: four M3 corner holes; DIN clip holes match `part-checks.md` and are ≥ 3 mm from any pad.
  - `test_zone_tracks`: every `Zn_A`/`Zn_B` net is fully routed, on the front layer only, width ≥ 1.5 mm.
- [ ] **Step 2: Run.** `make board check` → FAIL, no board.
- [ ] **Step 3: Write `gen_board.py`.** Load each footprint with `pcbnew.FootprintLoad`, set reference, value and pad nets from the netlist, place by coordinates computed from a small table of band positions at the top of the file. Draw the outline with the notch on `Edge.Cuts`.
- [ ] **Step 4: Run.** `make board check` → PASS.
- [ ] **Step 5: Render and look.** `kicad-cli pcb render` (top and bottom views) and `kicad-cli pcb export svg` of the front copper, silkscreen and outline into `renders/`. Open them. Check that labels are readable and not under parts, the zone line is where the text says, and nothing sits under the dev boards that is taller than 8 mm.
- [ ] **Step 6: Commit.** `feat: carrier board outline and placement`

### Task 5: Logic-side routing and design rule check

**Files:**
- Create: `scripts/route.py`
- Modify: `carrier-board.kicad_pcb`, `Makefile`, `scripts/check_board.py`

**Interfaces:**
- Consumes: the placed board, `zone_side_items`.
- Produces: `make route` leaves a fully routed board with ground pours; `make check` now includes KiCad DRC.

- [ ] **Step 1: Add to `check_board.py`:**
  - `test_fully_routed`: `board.GetConnectivity().GetUnconnectedCount(True) == 0` after filling zones.
  - `test_track_widths`: every track meets its net class width.
  - `test_drc_clean`: `kicad-cli pcb drc --schematic-parity --severity-all --exit-code-violations` reports 0 violations, 0 unconnected items, 0 parity problems.
  - `test_zone_tracks_unchanged`: zone-side tracks are identical to those `gen_board.py` drew.
- [ ] **Step 2: Run.** `make check` → FAIL on `test_fully_routed`.
- [ ] **Step 3: Get Freerouting.** Download the current release jar from the Freerouting project's GitHub releases page into `hardware/carrier-board/tools/` (git-ignored); record the version and SHA-256 in the README. This step needs Josh's go-ahead, given when the plan is approved.
- [ ] **Step 4: Write `route.py`:** lock the zone-side tracks; `pcbnew.ExportSpecctraDSN`; run Freerouting headless (`java -jar … -de board.dsn -do board.ses`); `pcbnew.ImportSpecctraSES`; add a `GND` pour on both layers covering the logic side only (outside the zone strip and the antenna rule areas); fill; save.
- [ ] **Step 5: Run.** `make route check` → PASS. If Freerouting leaves nets unrouted or breaks the 3 mm rule, adjust placement in `gen_board.py` (not the rule) and re-run. If it cannot be run at all, route the logic side in `route.py` with explicit track coordinates instead; the checks are the same.
- [ ] **Step 6: Render and look** at both copper layers in `renders/`. Check for tracks squeezed between pads, needless detours on power nets, and vias under the relays.
- [ ] **Step 7: Commit.** `feat: route carrier board`

### Task 6: Fabrication outputs, print template and README

**Files:**
- Create: `fab/` (generated), `renders/` final images
- Modify: `Makefile`, `README.md`, `bom.csv` (if Task 5 changed anything), root `CLAUDE.md` (one line in Project Structure for `hardware/carrier-board/`)

**Interfaces:**
- Produces: `make fab` regenerates everything in `fab/`; `make -C hardware/carrier-board check` is the single command that proves the design.

- [ ] **Step 1: Add `make fab`:** Gerbers for both copper layers, both masks, both silkscreens and the outline; Excellon drill files; a zip of those; `fab/print-template.pdf` at 1:1 showing outline, pads, drill holes and front silkscreen (`kicad-cli pcb export pdf`, scale 1); `fab/schematic.pdf`.
- [ ] **Step 2: Add `test_fab_outputs` to `check_board.py`:** every expected file exists, is newer than `carrier-board.kicad_pcb`, and the drill file's hole count equals the board's.
- [ ] **Step 3: Run.** `make fab check` → PASS, from a clean checkout state (`make clean` first).
- [ ] **Step 4: Write `README.md`:**
  - what the board is and a rendered image;
  - power: a 5 V USB-C supply rated 3 A, and why a weaker one resets the C6;
  - the H2 jumper rule;
  - wiring table: terminal Z*n* → relay → C6 GPIO, and that zone lights show drive, not contact closure;
  - assembly order (lowest parts first) and that the relays are flux-proof, not washable;
  - the three check lists from the spec's Verification section, word for word in substance: automated, before ordering, after assembly;
  - how to regenerate (`make`), tool versions, Freerouting version and hash;
  - what is not covered (from the spec) and that enabling 7 zones is a firmware change.
- [ ] **Step 5: Final read against the spec.** Go through the spec section by section and tick each requirement against the board, netlist or README. List anything unmet at the end of the README under "Known gaps"; an empty list is stated as such.
- [ ] **Step 6: Commit.** `feat: carrier board fabrication outputs and docs`
