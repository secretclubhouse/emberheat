"""What the design requires of the dev board library, written by hand from the
spec and Espressif's user guides. Never imported by gen_lib.py: the checks
compare the generated library against this file.

Pad numbers are the dev boards' own header names (J1, J3), unrelated to the
carrier's connector references. Pin 1 of each row is at the antenna end.
Signal names follow the brief; Espressif prints the J3 serial pair as TX / RX
(GPIO16 / GPIO17 on the C6, GPIO24 / GPIO23 on the H2).
"""


def _pads(j1, j3):
    pins = {}
    for row, names in (("J1", j1), ("J3", j3)):
        for i, name in enumerate(names, start=1):
            pins["%s_%d" % (row, i)] = name
    return pins


C6_PINS = _pads(
    "3V3 RST GPIO4 GPIO5 GPIO6 GPIO7 GPIO0 GPIO1 GPIO8 GPIO10 GPIO11 GPIO2 GPIO3 5V GND NC".split(),
    "GND GPIO16_TX GPIO17_RX GPIO15 GPIO23 GPIO22 GPIO21 GPIO20 GPIO19 GPIO18 GPIO9 GND GPIO13 GPIO12 GND NC".split(),
)

H2_PINS = _pads(
    "3V3 RST GPIO0 GPIO1 GPIO2 GPIO3 GPIO13 GPIO14 GPIO4 GPIO5 NC VBAT GND 5V GND".split(),
    "GND TX RX GPIO10 GPIO11 GPIO25 GPIO12 GPIO8 GPIO22 GND GPIO9 GND GPIO27 GPIO26 GND".split(),
)

# Geometry, mm. Rows at x = -ROW_X (J1) and +ROW_X (J3), pin 1 at the
# antenna end, which points toward -y.
ROW_X = 11.43
PITCH = 2.54
PAD_SIZE = 1.7
PAD_DRILL = 1.0
BOARD_WIDTH = 25.4

# Per board: PCB length, PCB edge to pin 1 centre (1.575 on both, from
# Espressif's dimension drawings: 51.8 - 12.125 - 15 * 2.54 and
# 48.26 - 11.125 - 14 * 2.54), antenna overhang beyond the PCB edge and the
# overhang's width.
BOARDS = {
    "ESP32-C6-DevKitC-1": dict(pins=C6_PINS, pads=32, length=51.8, edge_to_pin1=1.575,
                               overhang=6.35, ant_width=18.0),
    "ESP32-H2-DevKitM-1": dict(pins=H2_PINS, pads=30, length=48.26, edge_to_pin1=1.575,
                               overhang=5.40, ant_width=13.2),
}

# DIN clip (Hammond 1427DINCLIP), see part-checks.md.
DIN_HOLE_DIA = 3.5
DIN_SLOT_W = 3.5
DIN_SLOT_L = 5.0
DIN_SPACING = 29.75
