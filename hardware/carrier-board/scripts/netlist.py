"""Read build/netlist.xml (KiCad XML netlist, written by `make sch`).

Shared by check_netlist.py, check_board.py and gen_board.py, so there is one
parser. It knows nothing about what the design should contain."""
import os
import xml.etree.ElementTree as ET

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NETLIST = os.path.join(ROOT, "build", "netlist.xml")


def bare(name):
    """Net name without the "/" KiCad puts in front of labelled nets on the
    root sheet ("/Z7_A" -> "Z7_A"; "+5V" stays "+5V")."""
    return name[1:] if name.startswith("/") else name


class Netlist:
    """nets: {bare name: set of "REF.PAD"} for every net with a real
    connection; KiCad's single-pin "unconnected-(...)" nets and power-symbol
    pins (#PWR, #FLG) are left out.
    full_names: {bare name: name as KiCad writes it}.
    unconnected: {"REF.PAD": "unconnected-(...)"}, the single-pad nets KiCad
    gives pads that connect to nothing (the board carries them too, so it
    matches the schematic).
    parts: {ref: dict(value, footprint, symbol, path)} for every placed
    part; path is the symbol's sheet path and UUID, which the board's
    footprint carries so KiCad can match the two.
    pins: {ref: set of symbol pin numbers}."""

    def __init__(self, path=NETLIST):
        assert os.path.exists(path), "%s not found (run make sch)" % path
        root = ET.parse(path).getroot()
        self.nets, self.full_names, self.unconnected = {}, {}, {}
        for net in root.find("nets"):
            name = net.get("name")
            if name.startswith("unconnected-("):
                for n in net:
                    self.unconnected["%s.%s" % (n.get("ref"), n.get("pin"))] = name
                continue
            nodes = {"%s.%s" % (n.get("ref"), n.get("pin")) for n in net if not n.get("ref").startswith("#")}
            assert bare(name) not in self.nets, "net %s appears twice" % bare(name)
            self.nets[bare(name)] = nodes
            self.full_names[bare(name)] = name
        libparts = {}
        for lp in root.find("libparts"):
            pins = lp.find("pins")
            libparts[(lp.get("lib"), lp.get("part"))] = {p.get("num") for p in pins} if pins is not None else set()
        self.parts, self.pins = {}, {}
        for c in root.find("components"):
            ref = c.get("ref")
            if ref.startswith("#"):
                continue
            ls = c.find("libsource")
            self.parts[ref] = dict(value=c.findtext("value"), footprint=c.findtext("footprint"),
                                   symbol="%s:%s" % (ls.get("lib"), ls.get("part")),
                                   path=c.find("sheetpath").get("tstamps") + c.findtext("tstamps"))
            self.pins[ref] = libparts[(ls.get("lib"), ls.get("part"))]

    def net_of_pad(self):
        """{"REF.PAD": bare net name} for every connected pad."""
        return {p: name for name, pads in self.nets.items() for p in pads}
