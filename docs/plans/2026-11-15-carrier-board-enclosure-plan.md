# Carrier Board Enclosure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A script-generated, 3D-printable box (base, cover, dial strip, pane) for the rev 1 carrier board, with checks that prove it fits the board, and images for the owner to judge.

**Architecture:** The KiCad Python reads `carrier-board.kicad_pcb` and writes a small JSON file of board geometry. A CadQuery script in its own virtual environment reads that JSON plus a parameters module and builds the four printed parts and a box model of the populated board. A check script measures the solids against each other. Exports and images are committed.

**Tech Stack:** KiCad 9 Python (`pcbnew`) for the export only; Python 3.12 + CadQuery (pinned) in `tools/cad-venv` (git-ignored) made with `uv`; ImageMagick for images if needed; `make`.

**Spec:** `docs/plans/2026-11-15-carrier-board-enclosure-design.md`. Read it first; it is the authority. Board facts: `hardware/carrier-board/README.md`, `part-checks.md`, `scripts/gen_board.py`.

All paths below are relative to `hardware/carrier-board/` unless they start with `docs/`.

## Global Constraints

- No change to the board, its scripts' outputs, `fab/`, or anything outside `hardware/carrier-board/` except the two docs named in Task 4. `make check` must still exit 0 with its existing 53 checks passing.
- Board positions are never typed into the enclosure code. They come from `enclosure/board-geometry.json`, which is written only by `scripts/export_board_geometry.py`.
- Every size in the spec's Design section is a named field in `enclosure/params.py` with the spec's value as its default: inside 186 x 117 mm (side gaps 9, top 1, bottom 2); walls 2.4; floor 3; cover face 2; lip 4 high, 0.25 clear; standoff height 6; standoff top radius at most 3.5 on board-facing sides; insert M3 5 mm OD x 4 mm long; clip holes one round 4.0 and one slot 4.0 x 5.5 at 29.75; clip counterbores leaving 1.6 mm of floor; clear height above board 25; top notches 13 wide to 17 above the board's top face; left notch y 34 to 64 to 12 above; window 122 x 12; pane 126 x 16 x 1.5; dial strip 2 thick, lettering 0.4 proud; magnets 6 x 3; fit clearance 0.25; screw holes 0.4 over thread; outside vertical edge radius 8.
- Part heights for the clearance model are the spec's: relay 15, varistor 12, fuse 14, C1 11.5, J2 10, terminal header and plug 15, dev boards underside 11 and top 17, antenna boxes 11 to 15. Anything else on the board: 12 mm.
- Coordinates: board frame, x from the board's left edge, y down from its top edge, z up from the board's top face (z = 0), mm. The wall-mounted "top" is y = 0.
- Printed without supports: no overhang steeper than 45 degrees except bridges up to 20 mm.
- Checks follow the existing style: `test_*` functions, a runner that prints `ok` or `FAIL` per test and exits non-zero on any failure (copy the runner from `scripts/check_lib.py`).
- The CadQuery version and the font are pinned so a rebuild gives the same files.
- Commit after each task on `feat/carrier-board`, messages in the repo's `feat:` / `fix:` / `docs:` style. Do not push.

## Review Focus

1. A cover that cannot lift straight off with every plug and cable still connected. Expected: it lifts in +z with nothing in the way.
2. A USB-C plug whose moulded body is larger than assumed. Expected: the left notch passes a body 13 mm wide and 8 mm tall centred on the socket.
3. A standoff top that sits on solder joints, or an insert wall too thin to survive pressing. Expected: at most 3.5 mm from the hole centre on board-facing sides, and at least 0.9 mm of plastic round the insert hole everywhere.
4. A clip mounted hook end down, or holes that miss the clip's +/- 0.75 mm spacing tolerance. Expected: slot at the hook (upper) end, round hole at the latch end, `HOOK` and `UP` marked.
5. Lettering too fine to print. Expected: every stroke of the numerals and the wordmark at least 0.5 mm wide.

Each has a test in the task that owns the code (named below).

---

### Task 1: Environment, board geometry export, board model

**Files:**
- Create: `scripts/export_board_geometry.py`, `enclosure/board-geometry.json`, `enclosure/requirements.txt`, `enclosure/params.py`, `enclosure/board_model.py`, `enclosure/check_enclosure.py`
- Modify: `Makefile`, `scripts/check_board.py` (one new test)

**Interfaces:**
- Produces `enclosure/board-geometry.json`:
  - `pcb_sha256`: hash of `carrier-board.kicad_pcb`
  - `outline`: list of `[x, y]` (the notched outline), `thickness`: 1.6
  - `corner_holes`: `[{ref, x, y, drill}]` for H1 to H4
  - `zone_terminals`: `[{ref, x, mouth_y, width}]` for J11 to J17, `x` the body centre
  - `usb_c`: `{x_mouth, y, width}`; `screw_terminal`: `{bbox}`
  - `zone_lights`: `[{ref, x, y}]` and `power_light`: `{x, y}`, all **lens centres** (midpoint of the LED's two pads), not footprint origins
  - `dev_boards`: `[{ref, bbox, antenna_bbox}]`, `antenna_bbox` being the part past or nearest the board edge
  - `parts`: `[{ref, bbox}]`, every footprint's courtyard as `[x0, y0, x1, y1]`
- Produces `enclosure/params.py`: a frozen dataclass `Params` with the Global Constraints defaults, and `P = Params()`.
- Produces `enclosure/board_model.py`:
  - `load_geometry(path=...) -> dict`
  - `part_height(ref: str) -> float` (by reference prefix, per Global Constraints)
  - `board_solids(geom, p) -> dict[str, cq.Workplane]` with keys `board`, each part ref, `antenna_A1`, `antenna_A2`, `plug_J11` ... `plug_J17` (10.2 wide, z 0 to 15, from the mouth to 15 mm past the top edge), `usb_plug` (13 x 8 body, centred on the socket, from the mouth to 30 mm outside the left edge)
- Make targets: `enclosure-venv` (creates `tools/cad-venv` with `uv venv --python 3.12` and installs `enclosure/requirements.txt`), `enclosure-geometry` (KiCad Python), `enclosure-check` (venv Python).

- [ ] **Step 1:** Create the environment. `uv venv --python 3.12 tools/cad-venv`, install CadQuery, confirm `import cadquery` works and a 10 mm box exports to STL. Pin the installed version in `enclosure/requirements.txt`. If CadQuery will not install on this Mac with Python 3.12, try 3.10, then report BLOCKED with the error; do not switch CAD library.
- [ ] **Step 2:** Write `test_geometry_matches_board` in `scripts/check_board.py`: it re-runs the export in memory and fails unless it equals the committed JSON (so the JSON cannot go stale). Run `make check`; expect that one test to FAIL (no JSON yet).
- [ ] **Step 3:** Write `scripts/export_board_geometry.py` and the `enclosure-geometry` target; write the JSON. Run `make check`; expect exit 0, 54 ok.
- [ ] **Step 4:** In `enclosure/check_enclosure.py` write `test_geometry_sanity`: outline bounding box 168 x 114; four corner holes at 4 mm insets; seven terminals 18 mm apart; seven zone lights 18 mm apart, each within 1.5 mm of y 40; `pcb_sha256` equals the board file's hash. And `test_board_model`: no part box reaches above z 17; both antenna boxes extend past the board edge (left about 6.0, right about 5.05). Run; expect FAIL (no model).
- [ ] **Step 5:** Write `params.py` and `board_model.py`. Run `make enclosure-check`; expect all ok.
- [ ] **Step 6:** Commit: `feat: carrier board geometry export and enclosure board model`.

### Task 2: Base

**Files:**
- Create: `enclosure/gen_enclosure.py`
- Modify: `enclosure/check_enclosure.py`, `enclosure/params.py` (new fields only)

**Interfaces:**
- Consumes: `load_geometry`, `board_solids`, `P`.
- Produces in `gen_enclosure.py`:
  - `frame(geom, p) -> Frame`: the inside and outside rectangles in board coordinates and the z levels (`z_floor_top = -(1.6 + p.standoff_height)`, `z_split`, `z_face_inside = p.clear_height`), used by both parts
  - `magnet_positions(geom, p) -> list[(x, y)]`: four, per the spec
  - `clip_positions(geom, p) -> list[dict]`: per clip `round_hole`, `slot_centre`, `body_bbox`, `rail_y`
  - `make_base(geom, p) -> cq.Workplane`

- [ ] **Step 1:** Write the failing tests:
  - `test_standoffs`: a hole axis under each board hole (within 0.05 mm); the standoff's top face at the board's underside; in the top 2 mm, no base material beyond 3.5 mm from the hole centre on the two board-facing sides; at least 0.9 mm of plastic round the insert hole at every height (Review Focus 3).
  - `test_base_clear_of_board`: base and every `board_solids` item do not intersect, and are at least 1 mm apart except at the four standoff tops.
  - `test_clips`: per clip, hole spacing 29.75; slot 4.0 x 5.5 with its long axis on the hole line, at the upper (smaller y) end; rail centre 16.14 mm above the round hole and on the box's mid-height line for both clips; 1.6 mm of floor under each counterbore; clip bodies inside the base outline and not overlapping; `HOOK` and `UP` text bodies exist on the back (Review Focus 4).
  - `test_magnets_base`: four pockets, 6.0 + fit wide and 3.0 + 0.2 deep, opening toward the cover; none within 20 mm of an antenna box; none under the board outline.
  - `test_printable_base`: bounding box fits 300 x 300; the floor's outside face is one flat plane.
  Run; expect FAIL.
- [ ] **Step 2:** Implement `frame`, `magnet_positions`, `clip_positions`, `make_base` (floor, lip with breaks, standoffs with insert holes, clip holes and counterbores, magnet pads, back markings, rounded corners).
- [ ] **Step 3:** Run `make enclosure-check`; expect all ok.
- [ ] **Step 4:** Commit: `feat: carrier board enclosure base`.

### Task 3: Cover, dial strip, pane

**Files:**
- Create: `enclosure/fonts/Inter-SemiBold.ttf` and its `OFL.txt` (from the Inter project's official release; record the version and source URL in `enclosure/fonts/README.md`)
- Modify: `enclosure/gen_enclosure.py`, `enclosure/check_enclosure.py`, `enclosure/params.py` (new fields only)

**Interfaces:**
- Consumes: `frame`, `magnet_positions`, `make_base`, `board_solids`.
- Produces: `make_cover(geom, p)`, `make_dial_strip(geom, p) -> (plate, lettering)` (two solids, so the lettering can be coloured and its start height reported), `make_pane(geom, p)`, and `build(geom, p) -> dict` with keys `base`, `cover`, `dial_strip`, `dial_lettering`, `pane`, all in the assembled position.

- [ ] **Step 1:** Write the failing tests:
  - `test_cover_clear_of_board`: cover, strip and pane are at least 1 mm from every board part and at least 2 mm from each antenna box; the plugs, the USB plug body and a 6 mm wire path left of J2 pass through their notches without touching (Review Focus 2).
  - `test_cover_lifts_off`: sweeping cover + strip + pane 60 mm in +z intersects nothing in the base or in `board_solids`, plugs and USB plug included (Review Focus 1).
  - `test_cover_seats`: the cover does not intersect the base; lip clearance 0.25; each cover magnet pocket is coaxial with a base pocket and they face each other across the split.
  - `test_lights`: each zone light's centre is inside the window slot in plan, through cover and strip; the power light is under a 3 mm hole; each numeral's bounding-box centre is within 0.3 mm in x of its light.
  - `test_dial_strip`: the strip fits its recess with 0.25 clearance; its front is flush with the cover's face (within 0.05); lettering is 0.4 proud; the strip's window matches the cover's; the wordmark reads `emberheat`; the narrowest stroke of any glyph is at least 0.5 mm (Review Focus 5; measure by eroding each glyph outline by 0.25 mm and requiring it to stay non-empty and connected for `1`, or by sampling section widths).
  - `test_pane`: the pane slides in along x to its stop without intersecting the cover; it covers the whole window.
  - `test_styling`: the four outside vertical edges are arcs of radius 8; vent openings are evenly pitched and centred on the bottom wall.
  - `test_printable_cover`: every part fits 300 x 300 in its print orientation; the cover's face, the strip's back and the pane lie flat on the bed; no face in print orientation overhangs by more than 45 degrees, bridges up to 20 mm excepted.
  Run; expect FAIL.
- [ ] **Step 2:** Implement the cover (shell, notches, vents, window, pane rails, strip recess, power-light hole, magnet pads), the dial strip with lettering in Inter SemiBold, and the pane. If a 3.5 mm numeral in SemiBold fails the stroke test, raise the size, not the tolerance.
- [ ] **Step 3:** Run `make enclosure-check`; expect all ok. Run `make check`; expect exit 0.
- [ ] **Step 4:** Commit: `feat: carrier board enclosure cover, dial strip and pane`.

### Task 4: Exports, images, docs

**Files:**
- Create: `enclosure/out/` (`base`, `cover`, `dial-strip`, `pane` as `.step` and `.stl`, each in its print orientation, and `hashes.txt`), `renders/enclosure-closed.png`, `renders/enclosure-open.png`, `renders/enclosure-front.png`, `renders/enclosure-back.png`
- Modify: `Makefile` (target `enclosure`: geometry, build, export, images, check), `enclosure/check_enclosure.py`, `README.md` (new "Enclosure" section, and the DIN clip, rail-gap and Known gaps text made consistent with it), `docs/plans/2026-11-15-carrier-board-enclosure-design.md` ("As built" section), root `CLAUDE.md` (the one Project Structure line for `hardware/carrier-board/`)

**Interfaces:**
- Consumes: `build(geom, p)`.

- [ ] **Step 1:** Write `test_exports_current`: `enclosure/out/hashes.txt` records the hash of `board-geometry.json`, `params.py`, `gen_enclosure.py` and the font; the test fails if any differs from the file on disk, or if any of the eight exports is missing or empty. Run; expect FAIL.
- [ ] **Step 2:** Write the export step and the `enclosure` target. STL in print orientation (base floor down, cover face down, strip lettering up, pane flat). The dial strip's STL is one body; print the lettering start height (strip thickness, in mm from the bed) to the console and record it in `hashes.txt`.
- [ ] **Step 3:** Render the four images, shaded and in colour (body light grey, lettering black, pane translucent, board green with parts as grey boxes in the open view), at least 2000 px wide, white background. `enclosure-front` is straight on, so the grid and lettering can be judged. If shaded offscreen rendering cannot be made to work, fall back to hidden-line drawings and say so in the report.
- [ ] **Step 4:** Look at each image. Fix anything visibly wrong (lettering mirrored, strip not flush, notch missing) before going on.
- [ ] **Step 5:** Write the README "Enclosure" section: what to print and in what orientation; material and colours; the filament change height; hardware (4 M3 inserts 5 x 4, 4 M3 screws, 8 magnets 6 x 3, the Hammond clip pair and its screws); assembly order (inserts, magnets with polarity checked pair by pair before gluing, clips hook end up, board, strip, pane); a first test print of one standoff, one magnet pocket and a lip corner; what was not verified (the spec's Known limits, plus anything found while building); nylon screws in the bottom corners if radio range disappoints. Update the existing clip and rail-gap text so the README does not tell the owner to do both.
- [ ] **Step 6:** Append "As built" to the design doc: final outside size, every parameter that changed from the spec and why.
- [ ] **Step 7:** Run `make enclosure` then `make check`; expect both exit 0. Commit: `feat: carrier board enclosure exports, images and docs`.
