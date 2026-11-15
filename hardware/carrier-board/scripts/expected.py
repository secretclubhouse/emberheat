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


# ---------------------------------------------------------------------------
# Schematic netlist (Task 3). Never imported by gen_schematic.py.
#
# Net name -> set of "REF.PAD". Pad numbers are the symbol pin numbers of the
# parts named in bom.csv, which are also their footprint pad numbers:
#   A1, A2 (dev boards)   J1_n / J3_n, see C6_PINS / H2_PINS above
#   Device:R, Varistor, Polyfuse, C, C_Polarized   1, 2 (C_Polarized 1 = +)
#   Device:D, Device:LED  1 = K (cathode), 2 = A (anode)
#   Isolator:PC817        1 anode, 2 cathode, 3 emitter, 4 collector
#   Transistor_BJT:2N3904 1 E, 2 B, 3 C
#   Relay:FINDER-34.51    A1, A2 coil; 11 common, 14 normally open, 12 NC
#   USB_C_Receptacle_PowerOnly_6P  A9 B9 VBUS, A12 B12 GND, S1 shield,
#                                  A5 CC1, B5 CC2
#   Screw_Terminal_01x02, Conn_01x0N_Pin   1..N
#
# Which end of a two-pin non-polar part (resistor, varistor, fuse) is pin 1 is
# a drawing choice; it is fixed here so the netlist is fully determined:
# Rn1 1 = +3V3; Rn2 1 = DRV; Rn3 1 = BASE; Rn4 1 = LAMP; R1/R2 1 = CC;
# R3 1 = +5V; R4 1 = UART_C6_TX; R5 1 = +3V3; RVn 1 = Zn_B; F1 1 = VIN.
# ---------------------------------------------------------------------------

# Zone n -> C6 (A1) pad of its relay GPIO: GPIO 0, 1, 2, 3, 10, 11, 22.
ZONE_GPIO = [0, 1, 2, 3, 10, 11, 22]
ZONE_A1_PAD = {1: "J1_7", 2: "J1_8", 3: "J1_12", 4: "J1_13", 5: "J1_10", 6: "J1_11", 7: "J3_6"}

NETS = {
    "VIN": {"J1.A9", "J1.B9", "J2.1", "F1.1"},
    "+5V": {"F1.2", "C1.1", "C2.1", "A1.J1_14", "R3.1"},
    "PWR_LED": {"R3.2", "D8.2"},
    "GND": {"A1.J1_15", "A1.J3_1", "A1.J3_12", "A1.J3_15",
            "A2.J1_13", "A2.J1_15", "A2.J3_1", "A2.J3_10", "A2.J3_12", "A2.J3_15",
            "J1.A12", "J1.B12", "J1.S1", "J2.2", "C1.2", "C2.2",
            "R1.2", "R2.2", "D8.1", "J3.3", "J4.5"},
    "+3V3": {"A1.J1_1", "R4.2", "R5.1", "JP1.1", "J3.1", "J4.1"},
    "H2_3V3": {"JP1.2", "A2.J1_1"},
    "CC1": {"J1.A5", "R1.1"},
    "CC2": {"J1.B5", "R2.1"},
    "UART_C6_TX": {"A1.J3_10", "A2.J1_3", "R4.1"},
    "UART_H2_TX": {"A1.J3_9", "A2.J1_4"},
    "H2_RTS": {"A1.J1_5", "A2.J1_5"},
    "H2_CTS": {"A1.J1_6", "A2.J1_6"},
    "H2_RST": {"A1.J3_5", "A2.J1_2"},
    "ONEWIRE": {"A1.J3_7", "R5.2", "J3.2"},
    "SPARE_IO20": {"A1.J3_8", "J4.2"},
    "SPARE_TX": {"A1.J3_2", "J4.3"},
    "SPARE_RX": {"A1.J3_3", "J4.4"},
}

for _n in range(1, 8):
    NETS["+5V"] |= {f"K{_n}.A1", f"D{_n}.1", f"D1{_n}.2", f"U{_n}.4"}
    NETS["GND"] |= {f"Q{_n}.1", f"R{_n}3.2"}
    NETS["+3V3"] |= {f"R{_n}1.1"}
    NETS[f"Z{_n}_N"] = {f"A1.{ZONE_A1_PAD[_n]}", f"U{_n}.2"}
    NETS[f"Z{_n}_LED"] = {f"R{_n}1.2", f"U{_n}.1"}
    NETS[f"Z{_n}_DRV"] = {f"U{_n}.3", f"R{_n}2.1"}
    NETS[f"Z{_n}_BASE"] = {f"R{_n}2.2", f"R{_n}3.1", f"Q{_n}.2"}
    NETS[f"Z{_n}_COIL"] = {f"Q{_n}.3", f"K{_n}.A2", f"D{_n}.2", f"R{_n}4.2"}
    NETS[f"Z{_n}_LAMP"] = {f"D1{_n}.1", f"R{_n}4.1"}
    NETS[f"Z{_n}_A"] = {f"K{_n}.11", f"J1{_n}.1", f"RV{_n}.2"}
    NETS[f"Z{_n}_B"] = {f"K{_n}.14", f"J1{_n}.2", f"RV{_n}.1"}

# Pads that connect to nothing, each carrying a no-connect flag.
UNCONNECTED = {f"K{_n}.12" for _n in range(1, 8)}
# C6: RST, strapping pins 4 5 8 9 15, USB pins 12 13, both NC.
UNCONNECTED |= {"A1.J1_2", "A1.J1_3", "A1.J1_4", "A1.J1_9", "A1.J3_11", "A1.J3_4",
                "A1.J3_14", "A1.J3_13", "A1.J1_16", "A1.J3_16"}
# H2: every pin not named above (3V3, RST, GPIO0-3 and the grounds are used).
UNCONNECTED |= {"A2.J1_7", "A2.J1_8", "A2.J1_9", "A2.J1_10", "A2.J1_11", "A2.J1_12",
                "A2.J1_14", "A2.J3_2", "A2.J3_3", "A2.J3_4", "A2.J3_5", "A2.J3_6",
                "A2.J3_7", "A2.J3_8", "A2.J3_9", "A2.J3_11", "A2.J3_13", "A2.J3_14"}

# C6 strapping (4, 5, 8, 9, 15) and USB (12, 13) pins, by pad.
A1_FORBIDDEN = {"GPIO4": "J1_3", "GPIO5": "J1_4", "GPIO8": "J1_9", "GPIO9": "J3_11",
                "GPIO15": "J3_4", "GPIO12": "J3_14", "GPIO13": "J3_13"}
