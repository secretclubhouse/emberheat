#!/usr/bin/env python3
"""Writes lib/emberheat.kicad_sym, lib/emberheat.pretty/ and the two project
library tables. Run with the KiCad Python (`make lib`). Standard library only.

Geometry facts (Espressif ESP32-C6-DevKitC-1 and ESP32-H2-DevKitM-1 user
guides and dimension drawings, recorded in part-checks.md):
  - the two header rows are 22.86 mm apart, 2.54 mm pitch, J1 on the left and
    J3 on the right with the antenna at the top, seen from the component side;
  - pin 1 of each row is at the antenna end, 1.575 mm in from the PCB edge;
  - the module antenna overhangs the PCB by 6.35 mm (C6) or 5.40 mm (H2).
In a footprint the antenna end points toward -y (up on screen).

The pin lists are the dev boards' own and live here as the generator's input;
check_lib.py compares the result with expected.py, which this file never reads.
"""
import os
import sys
import uuid

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from sexp import Q, dumps  # noqa: E402

ROOT = os.path.dirname(HERE)
LIB = os.path.join(ROOT, "lib")
PRETTY = os.path.join(LIB, "emberheat.pretty")

ROW_X = 11.43
PITCH = 2.54
PAD_SIZE = 1.7
PAD_DRILL = 1.0
WIDTH = 25.4
RULE_END_CLEARANCE = 1.0  # rule area stops this far short of pin 1's centre


def split(s):
    return s.split()


BOARDS = [
    dict(
        name="ESP32-C6-DevKitC-1",
        j1=split("3V3 RST GPIO4 GPIO5 GPIO6 GPIO7 GPIO0 GPIO1 GPIO8 GPIO10 GPIO11 GPIO2 GPIO3 5V GND NC"),
        j3=split("GND GPIO16_TX GPIO17_RX GPIO15 GPIO23 GPIO22 GPIO21 GPIO20 GPIO19 GPIO18 GPIO9 GND GPIO13 GPIO12 GND NC"),
        length=51.8, edge_to_pin1=1.575, overhang=6.35, ant_width=18.0,
        desc="Espressif ESP32-C6-DevKitC-1 on two 1x16 female sockets, 22.86 mm apart, antenna toward -y",
        url="https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32c6/esp32-c6-devkitc-1/user_guide.html",
    ),
    dict(
        name="ESP32-H2-DevKitM-1",
        j1=split("3V3 RST GPIO0 GPIO1 GPIO2 GPIO3 GPIO13 GPIO14 GPIO4 GPIO5 NC VBAT GND 5V GND"),
        j3=split("GND TX RX GPIO10 GPIO11 GPIO25 GPIO12 GPIO8 GPIO22 GND GPIO9 GND GPIO27 GPIO26 GND"),
        length=48.26, edge_to_pin1=1.575, overhang=5.40, ant_width=13.2,
        desc="Espressif ESP32-H2-DevKitM-1 on two 1x15 female sockets, 22.86 mm apart, antenna toward -y",
        url="https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32h2/esp32-h2-devkitm-1/user_guide.html",
    ),
]


def num(v):
    s = ("%.4f" % v).rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def uid(*parts):
    return Q(str(uuid.uuid5(uuid.NAMESPACE_URL, "emberheat/" + "/".join(parts))))


def xy(*v):
    return [num(x) for x in v]


FONT = ["effects", ["font", ["size", "1", "1"], ["thickness", "0.15"]]]


# ---------------------------------------------------------------- footprints

def stroke(width):
    return ["stroke", ["width", num(width)], ["type", "solid"]]


def rect(name, tag, layer, x0, y0, x1, y1, width):
    return ["fp_rect", ["start"] + xy(x0, y0), ["end"] + xy(x1, y1), stroke(width),
            ["fill", "no"], ["layer", Q(layer)], ["uuid", uid(name, tag)]]


def text(name, tag, s, x, y, layer="F.SilkS"):
    return ["fp_text", "user", Q(s), ["at"] + xy(x, y), ["layer", Q(layer)], ["uuid", uid(name, tag)], FONT]


def prop(name, value, x, y, layer):
    return ["property", Q(name), Q(value), ["at"] + xy(x, y, 0), ["layer", Q(layer)], ["uuid", uid(name, "prop", name)], FONT]


def board_footprint(b):
    name, n = b["name"], len(b["j1"])
    y0 = -(n - 1) / 2.0 * PITCH
    ys = [y0 + i * PITCH for i in range(n)]
    top = ys[0] - b["edge_to_pin1"]
    bottom = ys[-1] + (b["length"] - b["edge_to_pin1"] - (n - 1) * PITCH)
    tip = top - b["overhang"]
    half = WIDTH / 2.0
    rule_end = ys[0] - RULE_END_CLEARANCE

    fp = ["footprint", Q(name), ["version", "20241229"], ["generator", Q("gen_lib")],
          ["generator_version", Q("1")], ["layer", Q("F.Cu")], ["descr", Q(b["desc"])],
          ["tags", Q("ESP32 devkit socket")],
          prop("Reference", "REF**", 0, -3, "F.Fab"), prop("Value", name, 0, 3, "F.Fab"),
          ["attr", "through_hole"]]
    # Board outline and antenna overhang on the front silkscreen.
    fp.append(rect(name, "pcb", "F.SilkS", -half, top, half, bottom, 0.15))
    fp.append(rect(name, "antenna", "F.SilkS", -b["ant_width"] / 2.0, tip, b["ant_width"] / 2.0, top, 0.15))
    fp.append(text(name, "t-antenna", "ANTENNA", 0, tip + b["overhang"] / 2.0))
    fp.append(text(name, "t-usb", "USB", 0, bottom - 2.5))
    fp.append(text(name, "t-j1", "J1", -ROW_X, ys[0] - 2.54))
    fp.append(text(name, "t-j3", "J3", ROW_X, ys[0] - 2.54))
    # Courtyard around PCB and antenna.
    fp.append(["fp_rect", ["start"] + xy(-half - 0.25, tip - 0.25), ["end"] + xy(half + 0.25, bottom + 0.25),
               stroke(0.05), ["fill", "no"], ["layer", Q("F.CrtYd")], ["uuid", uid(name, "crtyd")]])
    # Pads.
    for row, x, names in (("J1", -ROW_X, b["j1"]), ("J3", ROW_X, b["j3"])):
        for i in range(n):
            fp.append(["pad", Q("%s_%d" % (row, i + 1)), "thru_hole", "circle", ["at"] + xy(x, ys[i]),
                       ["size", num(PAD_SIZE), num(PAD_SIZE)], ["drill", num(PAD_DRILL)],
                       ["layers", Q("*.Cu"), Q("*.Mask")], ["remove_unused_layers", "no"],
                       ["uuid", uid(name, "pad", row, str(i + 1))]])
    # No copper under the antenna overhang, on either layer.
    corners = [(-half, tip), (half, tip), (half, rule_end), (-half, rule_end)]
    fp.append(["zone", ["net", "0"], ["net_name", Q("")], ["layers", Q("F.Cu"), Q("B.Cu")],
               ["uuid", uid(name, "rule")], ["name", Q("ANTENNA_KEEPOUT")], ["hatch", "edge", "0.5"],
               ["connect_pads", ["clearance", "0"]], ["min_thickness", "0.25"], ["filled_areas_thickness", "no"],
               ["keepout", ["tracks", "not_allowed"], ["vias", "not_allowed"], ["pads", "not_allowed"],
                ["copperpour", "not_allowed"], ["footprints", "allowed"]],
               ["placement", ["enabled", "no"], ["sheetname", Q("")]],
               ["fill", ["thermal_gap", "0.5"], ["thermal_bridge_width", "0.5"]],
               ["polygon", ["pts"] + [["xy"] + xy(x, y) for x, y in corners]]])
    return fp


def din_clip():
    name = "DIN_Clip"
    d, slot_w, slot_l, spacing = 3.5, 3.5, 5.0, 29.75
    fp = ["footprint", Q(name), ["version", "20241229"], ["generator", Q("gen_lib")],
          ["generator_version", Q("1")], ["layer", Q("F.Cu")],
          ["descr", Q("Hammond 1427DINCLIP mounting holes: round hole at the origin, slot 29.75 mm away along +x. No copper.")],
          ["tags", Q("DIN rail clip mounting")],
          prop("Reference", "REF**", spacing / 2.0, -6, "F.Fab"), prop("Value", name, spacing / 2.0, 6, "F.Fab"),
          ["attr", "exclude_from_pos_files", "exclude_from_bom"]]
    # 8 mm of clearance for each screw head: 8 mm across, centred on the hole.
    # The slot allows the head +/- 0.75 mm along x.
    r = 4.0
    slide = (slot_l - slot_w) / 2.0
    fp.append(["fp_circle", ["center"] + xy(0, 0), ["end"] + xy(r, 0), stroke(0.05), ["fill", "no"],
               ["layer", Q("F.CrtYd")], ["uuid", uid(name, "crtyd-round")]])
    fp.append(["fp_rect", ["start"] + xy(spacing - slide - r, -r), ["end"] + xy(spacing + slide + r, r),
               stroke(0.05), ["fill", "no"], ["layer", Q("F.CrtYd")], ["uuid", uid(name, "crtyd-slot")]])
    fp.append(["pad", Q(""), "np_thru_hole", "circle", ["at"] + xy(0, 0), ["size", num(d), num(d)],
               ["drill", num(d)], ["layers", Q("*.Cu"), Q("*.Mask")], ["uuid", uid(name, "hole")]])
    fp.append(["pad", Q(""), "np_thru_hole", "oval", ["at"] + xy(spacing, 0), ["size", num(slot_l), num(slot_w)],
               ["drill", "oval", num(slot_l), num(slot_w)], ["layers", Q("*.Cu"), Q("*.Mask")],
               ["uuid", uid(name, "slot")]])
    return fp


# ------------------------------------------------------------------- symbols

def sym_effects(hide=False, justify=None):
    e = ["effects", ["font", ["size", "1.27", "1.27"]]]
    if justify:
        e.append(["justify", justify])
    if hide:
        e.append(["hide", "yes"])
    return e


def sym_prop(name, value, x, y, hide=False, justify=None):
    return ["property", Q(name), Q(value), ["at"] + xy(x, y, 0), sym_effects(hide, justify)]


def pin_type(signal):
    if signal == "NC":
        return "no_connect"
    if signal == "RST":
        return "input"
    if signal in ("3V3", "5V", "VBAT", "GND"):
        return "passive"
    return "bidirectional"


def symbol(b):
    name, n = b["name"], len(b["j1"])
    top = 8 * PITCH if n == 16 else 7 * PITCH  # first pin y, on the 2.54 grid
    body_top = top + PITCH
    body_bottom = top - (n - 1) * PITCH - PITCH
    half = 10.16
    sym = ["symbol", Q(name), ["exclude_from_sim", "no"], ["in_bom", "yes"], ["on_board", "yes"],
           sym_prop("Reference", "A", -half, body_top + 1.27, justify="left"),
           sym_prop("Value", name, half, body_top + 1.27, justify="right"),
           sym_prop("Footprint", "emberheat:" + name, 0, 0, hide=True),
           sym_prop("Datasheet", b["url"], 0, 0, hide=True),
           sym_prop("Description", b["desc"], 0, 0, hide=True)]
    sym.append(["symbol", Q(name + "_0_1"),
                ["rectangle", ["start"] + xy(-half, body_top), ["end"] + xy(half, body_bottom),
                 ["stroke", ["width", "0.254"], ["type", "default"]], ["fill", ["type", "background"]]]])
    unit = ["symbol", Q(name + "_1_1")]
    for row, x, angle, names in (("J1", -(half + PITCH), 0, b["j1"]), ("J3", half + PITCH, 180, b["j3"])):
        for i, sig in enumerate(names):
            unit.append(["pin", pin_type(sig), "line", ["at"] + xy(x, top - i * PITCH, angle), ["length", num(PITCH)],
                         ["name", Q(sig), sym_effects()], ["number", Q("%s_%d" % (row, i + 1)), sym_effects()]])
    sym.append(unit)
    sym.append(["embedded_fonts", "no"])
    return sym


# --------------------------------------------------------------------- files

def write(path, text_):
    with open(path, "w") as f:
        f.write(text_ + "\n")


def main():
    os.makedirs(PRETTY, exist_ok=True)
    lib = ["kicad_symbol_lib", ["version", "20241209"], ["generator", Q("gen_lib")], ["generator_version", Q("1")]]
    lib += [symbol(b) for b in BOARDS]
    write(os.path.join(LIB, "emberheat.kicad_sym"), dumps(lib))
    for b in BOARDS:
        write(os.path.join(PRETTY, b["name"] + ".kicad_mod"), dumps(board_footprint(b)))
    write(os.path.join(PRETTY, "DIN_Clip.kicad_mod"), dumps(din_clip()))
    write(os.path.join(ROOT, "sym-lib-table"), dumps(
        ["sym_lib_table", ["version", "7"],
         ["lib", ["name", Q("emberheat")], ["type", Q("KiCad")], ["uri", Q("${KIPRJMOD}/lib/emberheat.kicad_sym")],
          ["options", Q("")], ["descr", Q("Dev board symbols for the EmberHeat carrier board")]]]))
    write(os.path.join(ROOT, "fp-lib-table"), dumps(
        ["fp_lib_table", ["version", "7"],
         ["lib", ["name", Q("emberheat")], ["type", Q("KiCad")], ["uri", Q("${KIPRJMOD}/lib/emberheat.pretty")],
          ["options", Q("")], ["descr", Q("Dev board and DIN clip footprints for the EmberHeat carrier board")]]]))
    print("wrote lib/emberheat.kicad_sym, lib/emberheat.pretty (%d footprints), sym-lib-table, fp-lib-table" % (len(BOARDS) + 1))


if __name__ == "__main__":
    main()
