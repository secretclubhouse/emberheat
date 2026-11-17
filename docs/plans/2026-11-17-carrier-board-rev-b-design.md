# Carrier board rev B: design

Date: 2026-11-17. Follows `2026-11-15-carrier-board-design.md` (rev A, `hardware/carrier-board/`). Status: draft, awaiting the owner's approval. Nothing here has been drawn, laid out or ordered.

**Do not start rev B until rev A has been assembled and bench-tested.** Rev A's results (zone current, relay behaviour at power-up, radio range) feed several decisions below.

## Intent

A smaller, cheaper controller board that drives off-the-shelf DIN-rail relay modules, in place of one large board that carries the relays, the zone terminals and two dev boards.

Why: rev A is 168 x 114 mm and cost about $149 for three boards at OSH Park, which prices by area ($5 per square inch for three). Most of that area is the two dev boards on sockets, the seven relay channels with their zone terminals, and the 24 V AC isolation strip. Moving the relays off the board and the radios onto modules removes most of it.

## Goals

- The board fits inside 100 x 100 mm, the size below which most board houses charge a few dollars for five boards. Target about 50 x 60 mm (an estimate, not laid out).
- No 24 V AC on the board at all. The zone circuits are wired only to the relay modules.
- A failed relay is replaced by unplugging it, with no soldering.
- The zone count is not fixed by the board's relay positions: up to seven coil outputs, with unused ones left unwired.
- BLE commissioning keeps working as it does today (C6 for Wi-Fi, H2 for BLE over UART).
- Thread stays possible later without a board change.
- One USB-C port flashes both radios.
- Every relay stays open in every start-up, reset and download state, as rev A requires.

## Non-goals

- Thread firmware. The board only keeps the wiring that Thread would need.
- A custom relay PCB or a custom relay housing.
- More than seven zones. The firmware has seven static zone tasks.
- Changing the hysteresis or control logic.
- A new printed enclosure. It is a follow-up once the board's size is known (see Open questions).

## Anti-goals

- A bare chip in place of a certified module. The module keeps the antenna, the matching network and the radio approval.
- A design that can only be built by a board house's assembly service, unless the owner decides that (see Open questions).
- A driver stage that can close a relay while a C6 pin floats or sits on a weak pull-up.
- Two antennas side by side on the same edge.

## Approaches

| | A. Rev A with fewer parts | B. Modules, relays on board | C. Modules, DIN relays off board |
|---|---|---|---|
| Radios | dev boards on sockets | C6 and H2 modules | C6 and H2 modules |
| Relays | soldered to the board | soldered to the board | DIN interface modules on the rail |
| 24 V AC on the board | yes | yes | no |
| Board size (estimate) | about 150 x 100 mm | about 100 x 60 mm | about 50 x 60 mm |
| OSH Park, three boards (estimate) | about $115 | about $46 | about $23 |
| Replace a relay | desolder | desolder | unplug |

**Chosen: C.** It gives the smallest board, takes the 24 V AC and its isolation rules off the board, and makes relays serviceable. Its cost is in the relay modules and the wiring between the board and them.

Rev A's design weighed three options: the carrier board, a fully integrated board, and no custom board with a hobby relay module on jumper wires. It did not weigh this one, a small custom controller wired to DIN interface relays, which is why it appears only now.

The fully integrated board from February 2026 (`emberheat-pcb`, named in rev A's design as the natural second revision) already covers the module, regulator, USB and antenna work. It was not read for this document. Read it before drawing rev B: it may be the starting point.

A fourth idea, a small custom PCB per relay, was dropped: it is what a DIN interface relay already is, without the housing, the screw terminals or the approvals.

## Design

### Relays

Slim DIN-rail interface relays, about 6.2 mm wide each, one per zone. The Finder 38.51 is the reference part: a socket with screw terminals, a coil indicator and coil suppression, carrying a plug-in 34.51 relay, the same relay series rev A uses. Phoenix Contact's PLC-RSC series is the equivalent. Seven of them take about 44 mm of rail.

- Coil voltage: 12 V DC or 24 V DC. Distributor listings show 12 V and 24 V versions of the 38.51; a 5 V or 6 V version was not found (searched 2026-11-17, not checked against Finder's catalogue).
- Coil commons are joined with the maker's jumper comb, so the board needs one wire per zone plus one common.
- The zone lights, varistors and zone terminals leave the board. The module's own indicator shows that the coil is driven. As on rev A, that is not proof the contact has closed.
- Contact protection across the 24 V AC contact (rev A's varistor) is not part of the module. Whether it is needed, and where it goes, depends on rev A's bench results.

### Relay drive

One ULN2003A (seven Darlington sinks, built-in flyback diodes) replaces rev A's seven optocouplers, seven transistors and their resistors. The relay's own coil-to-contact insulation is the isolation barrier, so the optocouplers add nothing once the 24 V AC is off the board.

**This flips the drive to active-high**: a C6 pin HIGH closes a relay. Rev A and the firmware are active-low.

- A floating or high-impedance pin leaves the relay open, which is the safe state at reset.
- A weak pull-up is the risk. Rev A's notes record that GPIO22 is weakly pulled up at reset. Through the ULN2003A's input network that is probably too little current to turn a channel on (my arithmetic, not measured), but the margin is small. So: fit a 10 k pull-down on every driver input, and do not use GPIO22 or any strapping pin for a relay.
- The alternative is to keep rev A's optocoupler stage and stay active-low, with no firmware change and about 35 more parts. Not chosen, but it is the fallback if the start-up tests fail.

### Radios

- **C6:** an ESP32-C6-WROOM-1 module (18 x 25.5 mm, 29 castellated pads). The ESP32-C6-MINI-1 is smaller (13.2 x 16.6 mm) but has 53 pads and is harder to solder by hand.
- **H2:** an ESP32-H2-MINI-1 module (13.2 x 16.6 mm). Espressif also lists an ESP32-H2-WROOM-02C; check whether it is easier to hand-solder.
- The H2 footprint is optional to populate. If BLE commissioning on the C6 alone works in a later esp-radio, the H2 can be left off.
- The two antennas go on different edges of the board, each with the keepout its datasheet asks for. This, not the part count, is expected to set the minimum board size.

### C6 to H2 link

Rev A links the boards on the H2's GPIO0 to GPIO3 and gives the H2 its own USB port, with a jumper (JP1) that must be pulled before that port is used. Rev B lets the C6 flash the H2 and drops the port and the jumper.

| C6 | H2 | Purpose |
|---|---|---|
| UART TX | GPIO23 (U0RXD) | data to the H2; also the H2's bootloader input |
| UART RX | GPIO24 (U0TXD) | data from the H2; also the H2's bootloader output |
| RTS, CTS | two free GPIOs | flow control, as today |
| GPIO | RST (EN) | C6 resets the H2, as today |
| GPIO | GPIO9 | held LOW at reset to put the H2 in its serial bootloader |

- The H2's GPIO8 must be HIGH at reset for the bootloader to start reliably (Espressif's esptool documentation); GPIO8 and GPIO9 both LOW is an invalid combination. Fit a pull-up on GPIO8.
- The H2 firmware must move its UART to GPIO23 and GPIO24.
- Test pads on the H2's USB pins stay on the board as a recovery path if the C6 cannot flash it. Check the pin numbers in the H2 datasheet.

For Thread, the same UART would carry the radio co-processor protocol with no wiring change. The C6 also has its own 802.15.4 radio, so the H2 is only needed for Thread if Wi-Fi and Thread must run at once. Whether the Rust stack supports the H2 as a co-processor has not been checked.

### Power

The dev boards supplied the 3.3 V regulator, the USB port, the reset and boot buttons and the decoupling. Rev B must carry them itself.

- Input: 12 V or 24 V DC on a pluggable screw terminal, matching the relay coil voltage, with reverse-polarity protection (rev A's J2 has none) and a fuse sized below the supply's limit (rev A's is above it).
- A buck regulator to 3.3 V for both modules.
- USB-C for flashing and logs, wired to the C6's USB pins (GPIO12, GPIO13) with ESD protection. Whether USB can also power the logic with the relay supply absent, as on rev A, is undecided.
- Reset and boot buttons for the C6. The factory-reset button stays on GPIO9.

### Connectors

- Coil outputs: one pluggable terminal, seven outputs plus the coil supply common.
- Power in: one two-position pluggable terminal.
- 1-Wire: three positions (3.3 V, data, ground) with the 4.7 k pull-up, as rev A's J3.
- USB-C.

### Mounting

On the same 35 mm rail as the relay modules. A board this small may fit a bought DIN-rail enclosure, which would also give a flame rating that printed plastic does not have. Undecided.

## Firmware consequences

None of this is done, and none of it should be until rev A is tested.

- `gpio_impl.rs`: relay polarity becomes a board setting (active-low for rev A, active-high for rev B).
- `failsafe.rs`: the panic handler's single register write must drive the relay pins to the OFF level for the board in use. On rev B that is LOW.
- `main.rs`: the first-thing-in-`main` write and the build-time check against `ZONE_CONFIGS[n].relay_pin` change with the new pin assignment.
- `dual_radio.rs`: new H2 pins, and a way to hold the H2 in its bootloader and pass a firmware image through.
- The safety notes in `CLAUDE.md` describe active-low relays and hardware pull-ups to HIGH. They must be rewritten for rev B.

## Checks after assembly

The rev A list applies, with these changes:

- With the modules unpowered, held in reset, during boot, in download mode and through ten power cycles: no relay module's indicator lights and no relay clicks.
- The same while the C6 is flashing the H2.
- Each zone switched from Apple Home, checked with a meter across the relay module's contact terminals.

## Open questions

1. **Hand solder or assembled?** The modules have a ground pad underneath, so they need hot air or a board house's assembly service. Assembly in practice means JLCPCB, which brings back the import duties that rev A avoided by using OSH Park.
2. **12 V or 24 V DC for the coils**, and what supplies it. A DIN-rail supply sits neatly beside the relays. Taking it from the system's 24 V AC transformer would put that circuit back on the board, which rev A deliberately avoided.
3. **Can the eight 5 V relays already bought be reused?** They are 34.51 relays, the type the 38.51 socket carries, but a socket that accepts a 5 V coil was not found. If one exists, only sockets need buying.
4. **Module prices.** The per-channel cost of the DIN modules was not looked up. My rough expectation is $15 to $25 each, against $14.40 for rev A's bare relay.
5. **Contact protection** across the 24 V AC contacts, pending rev A's measurements.
6. **Enclosure:** a bought DIN box, or a new printed one.
7. **C6 pin assignment.** Not drawn up. It needs seven relay pins that are neither strapping pins nor pulled up at reset, six pins for the H2, one for 1-Wire, the two USB pins and GPIO9 for the button.

## Not verified

- Every size and price estimate in the Approaches table. None was laid out or quoted.
- That a weak pull-up cannot turn on a ULN2003A channel. The pull-downs are there because this was not established.
- The H2's USB pin numbers and the land patterns of all three modules. Read the Espressif datasheets before drawing footprints.
- The Finder 38.51's coil current, indicator and suppression details. Read the Finder datasheet before choosing a part number.

## Sources

Read 2026-11-17 through web search summaries, not the documents themselves:

- [esptool: boot mode selection, ESP32-H2](https://docs.espressif.com/projects/esptool/en/latest/esp32h2/advanced-topics/boot-mode-selection.html) (GPIO9 LOW with GPIO8 HIGH for the serial bootloader)
- [RIOT OS: ESP32-H2](https://doc.riot-os.org/group__cpu__esp32__esp32h2.html) (UART0 on GPIO23 RX and GPIO24 TX)
- [Espressif ESP32-C6-MINI-1 datasheet](https://www.espressif.com/sites/default/files/documentation/esp32-c6-mini-1_datasheet_en.pdf) and [atomic14 module sizes](https://www.atomic14.com/esp32/sizes/) (module dimensions and pad counts)
- [RS: Finder 38.51, 12 V coil](https://uk.rs-online.com/web/p/electromechanical-interface-relays/0653002) and [24 V coil](https://uk.rs-online.com/web/p/electromechanical-interface-relays/0653003) (coil voltages on sale)
