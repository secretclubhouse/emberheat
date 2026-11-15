# EmberHeat carrier board (hardware rev 1) — design

Date: 2026-11-15
Status: approved by Josh, 2026-11-15

## Intent

Josh's heating controller today is a Raspberry Pi and relay module on a piece
of wood taped to a cinder-block wall, with the zone wires twisted onto relay
terminals that are too small for them. The ESP32 firmware in this repo is ready
to replace the Pi, but only exists as a bench rig of two dev boards and jumper
wires.

This board turns the bench rig into something that can be mounted on a DIN rail
and wired properly: the two dev boards plug into it, and it carries the seven
relays, their drivers, and terminals sized for the real wire.

## The system it connects to

- A zone control board about 10 ft up the wall has wire pairs carrying 24 V AC.
  Closing the connection between the two wires of a pair opens that zone's
  valve and starts the circulation pump.
- There are 7 zones. The carrier closes each pair with a relay contact and does
  nothing else with those wires: it does not use, measure or share the 24 V AC.
- Water temperature is set by a manual dial on the heater and is not controlled
  here.
- The controller is mounted about 5 ft below the zone control board. Zone wires
  are 16 AWG today; Josh will replace them with 18 AWG two-conductor
  thermostat cable.

## Goals

- Mounts on a DIN rail.
- The existing ESP32-C6-DevKitC-1 and ESP32-H2-DevKitM-1 plug in; no jumper
  wires between them.
- 7 relay outputs, each a dry contact closing one wire pair.
- Zone terminals accept 18 AWG wire, solid or stranded with a ferrule.
- Runs the current firmware with no pin changes.
- Powered from a USB-C supply. Nothing on the board connects to the 24 V AC.
- A light per zone shows when its relay is closed.
- Can be soldered by hand.

## Non-goals

- Bare radio modules soldered to the board, on-board USB data, on-board 3.3 V
  regulator (a later, integrated revision).
- Powering the board from the 24 V AC.
- An eighth relay.
- A closed enclosure.
- Seven wired sensor terminals. Zones get temperature over MQTT; one optional
  1-Wire connector is kept.
- Safety certification. This is a one-off for Josh's own installation.

## Anti-goals

- **A relay that is closed when it should not be.** With the board unpowered,
  the C6 unplugged from its socket, the C6 held in reset, or the C6 booting,
  every relay must be open (valve closed).
- **24 V AC reaching the logic side.** Zone wiring must stay separated from
  everything else on the board.
- A board that cannot be checked before it is ordered. Josh must be able to
  confirm fit and wiring without trusting the layout blindly (see Verification).

## Approaches

| Approach | Outcome |
|---|---|
| **Carrier board for the existing dev boards** (picked) | Only relays, drivers and connectors are new. Firmware and inter-board wiring are already proven on these exact boards. Large hand-solderable parts. Taller and bigger than an integrated board. |
| Fully integrated board (the February 2026 design in `emberheat-pcb`) | Compact, but adds antenna clearance, USB data routing, a regulator and fine-pitch soldering to a first spin, none of which automated checks can judge. Natural second revision. |
| No custom board (DIN mounts plus an off-the-shelf relay module) | Cheapest, but keeps loose jumper wires. |

Josh picked the carrier board on 2026-11-15.

## Design

Items marked (Claude's call) were not specified by Josh.

### Dev board sockets

Both boards are 25.4 mm wide with two header rows 22.86 mm apart on a 2.54 mm
pitch (from Espressif's dimension drawings).

- C6: ESP32-C6-DevKitC-1, 2 × 16 pins, 51.8 mm long.
- H2: ESP32-H2-DevKitM-1, 2 × 15 pins, 48.26 mm long. Josh identified it as an
  official Espressif board carrying an ESP32-H2-MINI-1 module; the DevKitM-1 is
  the Espressif board that matches. To be confirmed by the paper-template check
  before ordering.
- Each board sits in two single-row female socket strips. Both boards' USB
  ports stay reachable from a board edge, for flashing and serial output.
- Antenna ends point at a board edge, and the carrier has no copper under them
  (Claude's call; the boards sit about 11 mm above the carrier, so this is
  caution rather than necessity).

### Connections between the two boards

These are the bench rig's connections, as board traces:

| C6 pin | H2 pin | Purpose |
|---|---|---|
| GPIO18 | GPIO0 | C6 transmit to H2 |
| GPIO19 | GPIO1 | H2 transmit to C6 |
| GPIO6 | GPIO2 | C6 ready-to-send |
| GPIO7 | GPIO3 | C6 clear-to-send |
| GPIO23 | RST | C6 resets the H2 at boot |
| 3V3 | 3V3 | H2 is powered from the C6 board's 3.3 V, as on the rig, through a 2-pin jumper |
| G | G | Ground |

The jumper in the 3.3 V feed is removed before plugging a cable into the H2
board's own USB port (to flash it), so the two boards' 3.3 V regulators are
never connected together. It is labelled on the board (Claude's call).

A 10 kΩ pull-up from the C6 transmit line to 3.3 V keeps that line idle-high
while the C6 is in reset or being flashed (Claude's call, from the bench
debugging on 2026-11-14).

### Relay outputs

Zone to C6 pin mapping follows `ZONE_CONFIGS_FULL` in
`emberheat-esp32/src/config.rs`: zones 1–7 on GPIO 0, 1, 2, 3, 10, 11, 22, which
is also what `main.rs` wires up.

Each of the 7 channels:

```
3V3 ── 330 Ω ── optocoupler LED ── C6 GPIO         (GPIO LOW = on)

5V ── optocoupler transistor ── 1 kΩ ──┬── NPN base
                                         10 kΩ
                                         GND

5V ──┬── relay coil ──┬── NPN collector      NPN emitter ── GND
     ├── diode ───────┤   (flyback)
     └── LED + 1 kΩ ──┘   (zone light)

relay contact (normally open) ── 2-position terminal
```

- The firmware drives relays active-low: pin LOW closes the relay. A pin that
  is floating, or a C6 that is absent, in reset or unpowered, passes no current
  through the optocoupler LED, so the relay stays open. This is what meets the
  first anti-goal, and it is why the optocoupler stays even though the relay
  already isolates the contacts.
- Drive values are chosen so the transistor is fully on with an ordinary
  optocoupler (about 6 mA through its LED) and a coil of up to 80 mA. The
  builder confirms them against the chosen parts' datasheets.
- The optocoupler is not there for isolation (its output shares the logic
  ground); the relay isolates the contacts.
- Relay: sealed or flux-proof, through-hole, 5 V coil of at most 80 mA, single normally-open
  contact that is gold-clad or gold-plated (Josh's decision, see "What the
  contacts switch"). The datasheet must give a minimum switching load of
  10 mA at 5 V DC or lower, and a rating of at least 2 A at 30 V AC. A
  general-purpose power relay with plain silver-alloy contacts (the Omron
  G5NB-1A-E named in an earlier draft, or the Songle type on the Elegoo board)
  does not qualify. The builder picks the part and records the datasheet
  figures beside it.
- The zone light is across the coil, so it shows that the relay is being
  driven, not that the contact has physically closed.
- Terminals: pluggable 5.08 mm screw terminal blocks, one 2-position plug per
  zone, labelled Z1–Z7 on the board. They take wire up to about 2.5 mm², so a
  18 AWG wire fits bare or in a 1.0 mm² ferrule, and the existing 16 AWG wire
  would still fit. Pluggable so the wires can be terminated once
  and the board lifted off (Claude's call).
- A surge absorber (varistor) across each contact, fitted on every channel. It
  is rated for continuous use on a 24 V AC circuit (working voltage of at
  least 30 V AC) and clamps well below the relay's contact voltage rating.
- Parts are through-hole or 0805-and-larger so the board can be soldered by
  hand (Claude's call: optocoupler in DIP-4, transistor in TO-92, through-hole
  diodes and LEDs; resistors may be through-hole or 0805, builder's choice).

### Separation of zone wiring

- Relays and zone terminals sit along one edge. Nothing but relay contacts and
  terminals is on that side.
- At least 3 mm of bare board between any zone-side copper and any logic-side
  copper, on both layers, and no ground pour in the zone-side strip. A relay's
  contact pads count as zone-side and its coil pads as logic-side, so the
  relay footprint itself must have 3 mm between them.
- The board carries a printed line and label marking the zone-wiring side.

### Power

- A USB-C socket on the carrier, wired for power only (5 V, ground, and the two
  5.1 kΩ resistors that make a USB-C supply turn on). No data lines. It must be
  a power-only type with few, widely spaced pins so it can be soldered by hand.
  A 2-position screw terminal for 5 V sits beside it as an alternative input
  (Claude's calls).
- A resettable fuse (about 2 A hold) after the input, then the 5 V rail.
- The 5 V rail feeds the relay coils and zone lights directly and the C6 dev
  board through its 5V pin. The C6 board makes its own 3.3 V, which also powers
  the H2 and the optocoupler LEDs.
- Worst case is roughly 1 A at 5 V (7 relays at up to 80 mA each, zone
  lights, both radios), so a 5 V USB-C supply rated 3 A. A supply that only
  offers 0.5–0.9 A would sag when several relays close and reset the C6, which
  opens the relays: safe, but a nuisance. The README must say this.
- The C6 board's own USB port can be plugged into a computer at the same time
  for flashing; the dev board's input diode keeps the two 5 V sources apart.
  The builder must confirm that against the DevKitC-1 schematic.
- A bulk capacitor (about 470 µF) and a 100 nF capacitor on the 5 V rail near
  the relays, and a power light.

### Optional wired sensor connector

A 3-pin header (3.3 V, data on GPIO21, ground) with a 4.7 kΩ pull-up, for
DS18B20 sensors if wired sensing returns. Unused by the current firmware
default (Claude's call).

### Unused C6 pins

GPIO20 is no longer used to switch H2 power (the firmware resets the H2 through
GPIO23 instead). Spare C6 pins that the headers expose are brought to a row of
labelled pads for later use; strapping pins (4, 5, 8, 9, 15) and the USB pins
(12, 13) are left unconnected.

### Mechanical

- Two-layer board, 1.6 mm, one-sided assembly.
- Mounted with screw-on DIN-rail clips fixed to M3 holes in the board, not a
  slide-in extruded carrier, so all four edges stay free for terminals and
  cables (Claude's call). The builder picks a specific orderable clip, matches
  the hole pattern to it, and lists it in the bill of materials.
- Orientation on the wall: zone terminals along the top edge, because the zone
  wires come down from the control board above; power input and both dev
  boards' USB ports toward the bottom or sides.
- The two dev boards lie along the bottom, antennas overhanging the left and
  right board edges. Their USB ends face a notch cut into the bottom edge
  between them, so both USB ports are reached from a board edge. The power
  input sits on the left edge (Claude's call, made while planning: it is the
  only arrangement that puts both the antennas and the USB ports at an edge).
- Size is whatever the parts need, expected roughly 160 × 100 mm, with four
  further M3 holes at the corners.
- Open board, no lid. Height above the board is set by the socketed dev
  boards, about 25 mm.

### Where it lives

`hardware/carrier-board/` in this repo, because the board and the firmware's
pin assignments must change together:

- the KiCad project (schematic and board)
- `README.md`: what the board is, assembly notes, wiring, and the pre-order
  checks
- a bill of materials with orderable part numbers
- fabrication outputs (Gerbers and drill files), generated, in a subfolder
- any script used to generate or check the design

KiCad backup folders and `.DS_Store` are git-ignored. The February project in
`~/Code/emberheat-pcb` is left as it is; its notes are the starting reference
and it is not copied in.

### Not covered by this board

- **Firmware that is running but stuck with a relay pin held LOW.** The board
  has no hardware watchdog. This is handled in firmware: the crash fail-safe
  (on `main` as 1c06a5a) forces relays off and restarts the chip on a panic
  or hang. A board-level watchdog is left for a later revision.
- **Enabling 7 zones in firmware.** `config.rs` currently builds one zone
  (`NUM_ZONES = 1`) with the 7-zone table commented out; `main.rs` already
  wires GPIO 0, 1, 2, 3, 10, 11, 22 to relays 1–7. Switching to 7 zones is a
  firmware change made when the board is installed, and
  `test_relay_pins_are_valid_gpio` (which rejects GPIO22) needs correcting
  then. For this board, the pin source of truth is `main.rs` together with
  the commented `ZONE_CONFIGS_FULL` table.

### What the contacts switch

The current through a closed wire pair has not been measured. The Raspberry Pi
system uses an Elegoo 8-relay board; after about two years, 5 of the 7 relays
in use click and light but no longer close the circuit. That is contact
failure, with two likely causes that cannot be told apart from here: too
little current to keep plain silver contacts clean, or arcing from a valve
motor or relay coil on the pair.

Decision (Josh, 2026-11-15): mechanical relays with gold-clad contacts and a
surge absorber across each contact. Gold-clad contacts switch reliably from
about a milliamp up to several amps, so this covers both causes without the
measurement. Solid-state relays were considered and not chosen: they have no
contacts to fail, but a surge or shorted wire can destroy one, they tend to
fail stuck on, and sizing them needs the measured current.

Optional check by Josh: an AC current reading through one wire pair. A reading
above 2 A would mean revisiting the relay rating.

## Verification

Claude can write the design files and run KiCad's command-line checks (KiCad
9.0.7 is installed) but cannot look at the board. So "done" is defined by
checks that do not depend on looking, plus checks Josh does by hand:

Automated, all required:
- Schematic electrical rules check: no errors.
- Board design rules check: no errors and no unconnected nets.
- Schematic and board agree (KiCad's parity check).
- A scripted check that the net on each C6 socket pin matches the pin's role
  in `config.rs` and the table above, and that every zone-side copper item is
  at least 3 mm from every logic-side copper item.

By Josh, before ordering:
- Print the board at 1:1 and set both dev boards on the paper: every pin lands
  on a pad, USB ports clear the edge, antennas overhang as drawn.
- Check the relay, terminal block and DIN-rail clips against the printed
  footprints and hole pattern, or against the parts in hand.
- Open the schematic in KiCad and read one relay channel end to end.

By Josh, after assembly and before connecting zone wires:
- With no dev boards fitted, power the carrier: no relay closes, no zone light.
- With the C6 fitted and held in reset, and during boot: no relay closes.
- Run the firmware and switch each zone from Apple Home: the right relay and
  light respond, checked with a meter across each terminal.

## Left to the builder

- How the KiCad files are produced (hand-written, scripted, or generated from
  a netlist) and how routing is done, as long as the checks above pass and the
  schematic is readable by a person.
- Exact part numbers, subject to the constraints above.
- Placement and routing within the mechanical rules.
- Whether the resistors are through-hole or 0805.
- Fabrication output format details for the chosen board house.
