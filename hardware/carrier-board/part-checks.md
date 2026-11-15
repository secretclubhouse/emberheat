# Part checks

Datasheet figures behind each part in `bom.csv`. Every figure names its source. Where a figure could not be found it says so.

## Relay K1-K7: Finder 34.51.7.005.5010

Source: Finder, 34 series datasheet, edition II-2026 (the "II-2026" footer).

- Ordering code 34.51.7.005.5010: sensitive DC coil, 5 V, contact material 5 = AgNi + Au, contact circuit 0 = changeover (so pin 12, normally closed, stays unused), option 1 = flux proof, standard version.
- Contacts 6 A, 250 V AC. Maximum switching voltage 250 V AC.
- Minimum switching load 50 mW (5 V / 2 mA).
- Clearance/creepage 8 mm / 8 mm, coil to contacts.
- Coil: rated power 0.17 W. The coil table gives, for the 5 V coil: minimum pick-up 3.5 V, maximum 7.5 V, resistance 130 ohm, current at 5 V 38.4 mA.
- Discrepancy: the plan quotes 34 mA (170 mW / 5 V). The coil table says 38.4 mA (5 V / 130 ohm). The arithmetic below uses the larger figure.

## Optocoupler U1-U7: Lite-On LTV-817

Source: Lite-On LTV-817 Series datasheet (package page and electrical table).

- Current transfer ratio, whole series: minimum 50 %, maximum 600 % at IF = 5 mA, VCE = 5 V. Rank letters only narrow the range (L 50-100 %, A 80-160 %, B 130-260 %, C 200-400 %, D 300-600 %). Buying the unranked part guarantees only the 50 % minimum, which is what the arithmetic uses.
- LED forward voltage: typical 1.2 V, maximum 1.4 V.
- Collector-emitter saturation voltage: maximum 0.2 V (IF = 20 mA, IC = 1 mA).
- Pinout from the package drawing: 1 anode, 2 cathode, 3 emitter, 4 collector. `Isolator:PC817` has pins 1 and 2 on the LED side and 3 (emitter), 4 (collector) on the transistor side, with the same numbering. Package: 4-pin DIP, 7.62 mm row spacing, which is `Package_DIP:DIP-4_W7.62mm`.

## Transistor Q1-Q7: 2N3904

Source: onsemi 2N3903/2N3904 datasheet.

- hFE minimum at VCE = 1 V: 70 at 1 mA, 100 at 10 mA, 60 at 50 mA.
- VCE(sat) maximum: 0.2 V (IC 10 mA, IB 1 mA); 0.3 V (IC 50 mA, IB 5 mA).
- VBE(sat): 0.65-0.85 V (10 mA / 1 mA); up to 0.95 V (50 mA / 5 mA).
- Collector current 200 mA continuous.
- TO-92 pinout in this datasheet: 1 emitter, 2 base, 3 collector. `Transistor_BJT:2N3904` in KiCad 9.0.7 extends `Q_NPN_EBC`, which is pin 1 emitter, 2 base, 3 collector. They match. Some other makers' 2N3904 are CBE, so the bill of materials names onsemi.

## Varistor RV1-RV7: Littelfuse V56ZA2P

Source: Littelfuse, Radial Lead Varistors (ZA series) datasheet, 2024.

- Maximum continuous AC voltage VM(AC): 35 V. Required: at least 30 V.
- Maximum clamping voltage VC: 110 V at 8/20 us test current 2.5 A. Required: below 400 V. The relay's contacts are rated 250 V.
- Disc size: 7 mm, maximum disc diameter 7 mm. Lead spacing e: 4 to 6 mm, so 5 mm.
- Footprint `Varistor:RV_Disc_D7mm_W3.4mm_P5mm`: D7 mm, pitch 5 mm. The thickness the datasheet gives for this part was not extracted, so the W3.4 mm body width in the footprint is not confirmed; it does not move the holes.
- Also from the same table: VNOM 50.4-61.6 V, surge 250 A (8/20 us), energy 2.3 J.

## Resettable fuse F1: Bourns MF-RHT200

Source: Bourns, MF-RHT series datasheet.

- Hold current 2.0 A and trip current 3.8 A at 23 C. Required hold: 1.85-2.5 A.
- Hold current derates to 1.77 A at 40 C and 1.34 A at 70 C. Expected load is about 1 A or less (7 coils at 38.4 mA, zone lights, both radios).
- Vmax 16 V, Imax 100 A. The rail is 5 V.
- Lead spacing 5.1 mm (dimension C of the product dimensions table). Footprint `Fuse:Fuse_Bourns_MF-RHT200` (pad 1 at 0, 0; pad 2 at 5.1, 1.2 mm).
- Ordering: `MF-RHT200-0` is the bulk-packed form from the How to Order page.

## DIN rail clip H5, H6: Hammond 1427DINCLIP

Source: Hammond 1427DINCLIP drawing PDF (hammfg.com, revision 2, 2022-03-31).

- Plastic clip for 35 mm DIN rail. Sold as a pair, with 4 panhead #6 x 1/4 in screws. Two clips per board, so one pair.
- Clip body 45.40 +/- 1.25 mm long (across the rail), 10.00 +/- 0.50 mm wide, 14.00 +/- 0.50 mm tall.
- Screw holes on the mounting face: two, diameter 3.00 mm, on the clip's centre line (5.00 mm from each long edge), 29.75 +/- 0.75 mm apart, the first 4.50 mm from the end.
- Screw: #6 x 1/4 in (about 3.5 mm thread, 6.35 mm long), self-tapping into the plastic. With 1.6 mm of board, about 4.7 mm goes into the clip.
- Board footprint `emberheat:DIN_Clip`, one per clip: a round hole 3.5 mm at the origin and a slot 3.5 mm wide by 5.0 mm long (long axis along the hole line) centred 29.75 mm from it. The slot takes the +/- 0.75 mm spacing tolerance. Keep 8 mm clear around each hole for the screw head. The clips mount on the back of the board, so the screws go in from the front.
- Stock, US distributor: Digi-Key (digikey.com) part number HM1489-ND, listed with manufacturer standard lead time 4 weeks. Search-result snapshots of the digikey.com pages showed "In-Stock: 340" (product page), "332 In Stock, 990 Factory" (1427 base-product page), "854 In Stock, 5,183 Factory" and "453 In Stock, 4,440 Factory" (Box Accessories category listings). The snapshots differ in date, so the quantity is not exact, but every one shows the part in stock. The live pages (digikey.com, Mouser and Newark, which also lists it as "DIN Clip, Nylon, Pair", 50H5703) block automated reading from this environment, so no live count was read. Hammond's own page says it is sold in packages of 2.

## Zone terminal J11-J17, J1, J2

- Phoenix Contact 1757242 is MSTBA 2,5/2-G-5,08, the right-angle header (confirmed by Phoenix and distributor listings). Mating plug MSTB 2,5/2-ST-5,08 is Phoenix 1757019: 12 A, 250 V, 5.08 mm, 0.2-2.5 mm2 wire (Farnell copy of the Phoenix datasheet). Order 7 plugs.
- J2: Phoenix 1729128 is listed by Phoenix as MKDSN 1,5/2-5,08 (Phoenix datasheet, 2017-03-20, product version 07). The KiCad footprint is MKDS 1,5/2-5,08. Compared:
  - Pitch: Phoenix dimension a 5.08 mm; footprint pads 5.08 mm apart. Same.
  - Drill: Phoenix PCB-design hole diameter 1.3 mm; footprint drill 1.3 mm. Same.
  - Pin: Phoenix pin dimensions 0.5 x 1 mm (diagonal about 1.12 mm), which fits the 1.3 mm hole.
  - Pad: the footprint pad is 2.6 mm; the Phoenix datasheet gives no pad size, so that is KiCad's choice and fine for hand soldering.
  - Body: Phoenix length 8.1 mm, width 10.16 mm (two positions), height 10 mm; footprint outline 10.2 x 9.8 mm. Close enough for keep-out; Task 3 should keep the outline as is.
  - The MKDS (not N) variant's own datasheet was not compared; the footprint matches the MKDSN part that is on order.
- J1: GCT USB4125-GF-A, 6-pin power-only USB-C, 3 A (GCT category page and Digi-Key listing). The datasheet itself was not retrieved.

## Drive arithmetic (one channel)

Figures: LTV-817 and 2N3904 above, relay coil 38.4 mA at 5 V.

- Optocoupler LED current, GPIO at 0 V, 3.3 V through 330 ohm: (3.3 - 1.2) / 330 = 6.4 mA typical. With the 1.4 V maximum VF it is 5.8 mA. The CTR minimum is given at 5 mA, so at least 5.8 mA is above the test point.
- Optocoupler transistor can pass at least 0.5 x 5.8 = 2.9 mA.
- Base current: the 1 k resistor alone would allow (5 - 0.2 - 0.85) / 1 k = 3.95 mA. The 10 k pull-down takes 0.85 / 10 k = 0.085 mA. So base current is between 2.8 mA (limited by the optocoupler at the minimum CTR) and 3.9 mA.
- Collector current: coil 38.4 mA, plus zone light (5 - Vf - 0.2) / 1 k. For a red LED taking Vf = 1.6-2.4 V (an assumption; the Kingbright WP7113ID datasheet was not checked) that is 2.4-3.2 mA. Worst case 38.4 + 3.2 = 41.6 mA. At 5.25 V supply, 5.25 / 130 + 3.5 mA = 43.9 mA.
- Forced gain: 41.6 / 2.8 = 14.9 at the worst case, 15.7 with a 5.25 V supply. Both are 20 or less, so the transistor is saturated (hFE minimum is 60 at 50 mA).
- Coil voltage when on: 5 - 0.3 = 4.7 V, above the 3.5 V pick-up and below the 7.5 V maximum.
- With the plan's 34 mA coil figure the gain would be 13.3 ((34 + 3.2) / 2.8), so the choice holds either way.
- A GPIO that is high or floating gives no LED current, so the relay stays open.

## Dev board power

Sources: Espressif ESP32-C6-DevKitC-1 schematic V1.1 (2022-11-10) and ESP32-H2-DevKitM-1 schematic V1.3 (2024-03-19), read from the rendered PDFs.

- C6 board: each USB port has its own 1N5819HW Schottky diode between its VBUS and VCC_5V. D1 is on J2 (UART port, VBUSA) and D7 is on J4 (USB port, VBUSB). The 5V header pin (J1 pin 14) connects to VCC_5V, which is after the diodes. So a 5 V supply on the 5V pin cannot back-feed either USB port, and either USB cable cannot raise the 5V pin above its own VBUS less a diode drop. As the design assumes.
- C6 3V3 header pin: J1 pin 1 is labelled ESP_3V3, which comes from the SGM2212-3.3 regulator output (VCC_3V3) through the 2-pin jumper J5. With J5 fitted (as shipped) it is the regulator output.
- H2 board: the 3V3 header pin (J1 pin 1, ESP_3V3) is the SGM2212-3.3 regulator output (VCC_3V3) through jumper J5, as shipped. The H2 board has the same two Schottky diodes (D1, D7) between its USB ports and VCC_5V.
- Result: the power section of the design holds. One extra point: both boards' 3V3 header pins sit behind their own J5 jumper, so the carrier's 3V3 net works only while J5 is fitted on the C6 board. Do not remove it.
- The C6 schematic is V1.1; the user guide lists later revisions (up to v1.4) for newer boards. The later schematics were not fetched.

## Dev board headers and dimensions (A1, A2 footprints)

Sources, read on 2026-11-15 from docs.espressif.com (esp-dev-kits, "latest"):
- ESP32-C6-DevKitC-1 v1.2 user guide (header tables J1 and J3, pin layout figure) and `esp32-c6-devkitc-1-dimensions_v1.2.pdf`. The guide says its numbering is the same as the v1.2 schematic; v1.3 and v1.4 only change resistor values.
- ESP32-H2-DevKitM-1 user guide (header tables J1 and J3, pin layout figure v1.2) and `esp32-h2-devkitm-1_v1.2_dimension.pdf`. The guide lists schematics v1.2 and v1.3.

Result: every pin list, the row spacing, both board lengths and the orientation match the plan. No difference.

- Pin lists: C6 J1 and J3 (16 pins each) and H2 J1 and J3 (15 pins each) match `scripts/expected.py` pin for pin. Espressif names the grounds `G` and the serial pair `TX` / `RX` on J3 pins 2 and 3 (GPIO16 / GPIO17 on the C6, GPIO24 / GPIO23 on the H2); the library uses `GND`, `GPIO16_TX`, `GPIO17_RX` and, on the H2, `TX`, `RX` as the plan gives them. H2 J1 pins 7 and 8 are printed `13/N` and `14/N` (GPIO13 and GPIO14, with 32 kHz crystal functions); the library names them GPIO13 and GPIO14.
- Row spacing: 22.86 mm between the two rows (both drawings); header pitch 2.54 mm; board width 25.40 mm (22.86 + 2 x 1.27).
- Lengths: C6 PCB 51.80 mm, H2 PCB 48.26 mm.
- Orientation: both pin layout figures show the component side (module, USB connectors, buttons visible) with the antenna at the top. J1 (starts 3V3, RST) is the left row and J3 (starts G, TX, RX) the right row, and pin 1 of each row is at the antenna end. Seen from the front of the carrier (dev board plugged in, component side up) this is the same picture, which is what the footprint draws with the antenna toward -y.
- What the drawings add, used by the footprints: the 51.80 / 48.26 mm lengths are the PCB only. The module antenna overhangs the PCB by 6.35 mm (C6, 18.00 mm wide) or 5.40 mm (H2, 13.20 mm wide). Pin 1 is 1.575 mm from the PCB edge at the antenna end (C6: 51.80 - 12.125 - 15 x 2.54; H2: 48.26 - 11.125 - 14 x 2.54), so the rows are not centred in the PCB. The origin (centre of the rows) is 20.625 mm from the PCB edge toward the antenna and 31.175 mm toward the USB end for the C6 (19.355 and 28.905 mm for the H2). The plan did not say this; it only fixed the origin as the centre of the two rows.
- Antenna rule area: from the antenna tip to 1.0 mm short of pin 1's centre, full 25.4 mm width, no tracks, vias, pads or pours on either copper layer. That is 6.925 mm deep on the C6 and 5.975 mm on the H2, where the plan said 7 mm for both. A full 7 mm measured from the antenna tip would cover the H2's pin 1 pad.
- DIN clip keep-out: the DIN clip section says to keep 8 mm clear around each hole for the screw head. The footprint draws that as an 8 mm wide courtyard centred on the hole (a 9.5 x 8 mm box on the slot, covering the +/- 0.75 mm of slide).
