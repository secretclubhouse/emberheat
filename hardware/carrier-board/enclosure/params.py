"""Every size of the enclosure, in mm. Defaults are the design's values
(docs/plans/2026-11-15-carrier-board-enclosure-design.md). Board positions are
not here: they come from board-geometry.json."""
from dataclasses import dataclass


@dataclass(frozen=True)
class Params:
    # Envelope: inside size, and the gaps round the 168 x 114 board
    inside_w: float = 186.0
    inside_h: float = 117.0
    gap_side: float = 9.0          # board edge to inside wall, left and right
    gap_top: float = 1.0
    gap_bottom: float = 2.0
    wall: float = 2.4
    floor: float = 3.0
    cover_face: float = 2.0
    outside_edge_radius: float = 8.0   # outside vertical edges
    edge_chamfer: float = 0.6          # every other outside edge

    # Locating lip on the base
    lip_height: float = 4.0
    lip_clear: float = 0.25
    lip_t: float = 2.0                 # lip thickness, wall to board side

    # Standoffs and heat-set inserts
    standoff_height: float = 6.0       # floor to board back
    standoff_top_radius_max: float = 3.5   # on board-facing sides (copper-free zone is 4.0)
    # Flare at the floor, 45 degrees up to the top radius. At most 4.75: the
    # top holes are 5 mm from the inside wall and the cover needs lip_clear.
    standoff_foot_r: float = 4.5
    insert_od: float = 5.0
    insert_len: float = 4.0
    # Hole for the insert, from the top. 4.4 mm is the plan's ruling for an
    # M3 5 x 4 insert (about 0.6 under the knurl); the README's test piece
    # settles it. Deeper than the insert so an M3 x 8 screw through the 1.6 mm
    # board does not bottom out (8 - 1.6 = 6.4 < 7).
    insert_hole_d: float = 4.4
    insert_hole_depth: float = 7.0

    # DIN clip screws
    clip_hole_round: float = 4.0
    clip_slot_w: float = 4.0
    clip_slot_l: float = 5.5
    clip_hole_spacing: float = 29.75
    clip_floor_left: float = 1.6       # floor left under each screw head
    clip_inset: float = 40.0           # outside edge to each clip's centre line
    clip_head_clear_d: float = 7.6     # counterbore; #6 pan head is 6.86 at most

    # Debossed HOOK / UP on the back
    mark_h: float = 5.0                # capital height
    mark_stroke: float = 0.8
    mark_depth: float = 0.6

    # Cover
    clear_height: float = 25.0         # above the board's top face
    top_notch_w: float = 13.0
    top_notch_h: float = 17.0          # above the board's top face
    left_notch_y0: float = 34.0
    left_notch_y1: float = 64.0
    left_notch_h: float = 12.0         # above the board's top face

    # Window, pane, dial strip
    window_w: float = 122.0
    window_h: float = 12.0
    pane_w: float = 126.0
    pane_h: float = 16.0
    pane_t: float = 1.5
    dial_t: float = 2.0
    letter_proud: float = 0.4
    # The cover's window has a rim this wide, flush with the face, inside the
    # strip's window: printed face down, the plate behind the strip then
    # bridges between held edges instead of ending free at the window.
    window_collar: float = 1.0
    backing_t: float = 1.6             # plate behind the strip's recess
    backing_lap: float = 2.0           # how far it runs past the recess
    strip_margin: float = 4.0          # strip edge to its window, and to the wordmark
    # Cap height of the numerals (the wordmark is set at the same size). The
    # design's ~3 mm gives strokes of 0.3 to 0.45 mm in Inter SemiBold; 5.5 is
    # the smallest that keeps every stroke at 0.5 or more.
    label_cap: float = 5.5
    label_gap: float = 3.0             # strip window to the numerals' tops
    word_gap: float = 5.5              # numerals' baseline to the wordmark's top: one cap height
    power_hole_d: float = 3.0

    # Pane rails: on the plate behind the strip, along y at the pane's ends;
    # the pane slides down between them onto two stops.
    rail_w: float = 2.0                # rail bar, beside the pane's end
    rail_lip: float = 1.5              # lip over the pane, its underside at 45 degrees
    rail_foot: float = 0.6             # rail below the lip
    stop_t: float = 2.0
    stop_reach: float = 3.5            # stop runs this far in from the rail

    # Vents: slots through the bottom wall
    vent_w: float = 2.0
    vent_h: float = 10.0
    vent_pitch: float = 4.5            # a quarter of the 18 mm light pitch
    vent_count: int = 25

    # Magnets and fits
    magnet_d: float = 6.0
    magnet_t: float = 3.0
    # Pocket depth over the magnet's thickness. 0: magnets sit flush, so a
    # pair's faces are only the cover's seating gap (fit_clearance) apart.
    magnet_recess: float = 0.0
    pad_roof: float = 1.0              # cover pad above its pocket
    magnet_pad_wall: float = 0.7       # plastic round each pocket in the pad
    fit_clearance: float = 0.25
    screw_over: float = 0.4            # hole diameter over thread


P = Params()
