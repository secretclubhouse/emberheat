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

    # Locating lip on the base
    lip_height: float = 4.0
    lip_clear: float = 0.25

    # Standoffs and heat-set inserts
    standoff_height: float = 6.0       # floor to board back
    standoff_top_radius_max: float = 3.5   # on board-facing sides (copper-free zone is 4.0)
    insert_od: float = 5.0
    insert_len: float = 4.0

    # DIN clip screws
    clip_hole_round: float = 4.0
    clip_slot_w: float = 4.0
    clip_slot_l: float = 5.5
    clip_hole_spacing: float = 29.75
    clip_floor_left: float = 1.6       # floor left under each screw head

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

    # Magnets and fits
    magnet_d: float = 6.0
    magnet_t: float = 3.0
    fit_clearance: float = 0.25
    screw_over: float = 0.4            # hole diameter over thread


P = Params()
