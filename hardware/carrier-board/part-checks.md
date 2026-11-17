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
- Pins against the footprint's holes (same datasheet; page 3 "Copper side view" drilling pattern, page 9 outline drawing, both read as images): the drilling pattern gives Ø1 holes for the coil pins and Ø1.3 for the contact pins, the footprint's 1.0 and 1.3 mm drills. The outline gives the coil pins as 0.5 x 0.5 mm (diagonal 0.71 mm, 0.29 mm smaller than its hole) and the contact pins as 1 x 0.4 mm (diagonal 1.08 mm, 0.22 mm smaller than its hole). Both fit. Body 28 x 5 x 15 mm, pins 3.5 mm long.

## Optocoupler U1-U7: Lite-On LTV-817

Source: Lite-On LTV-817 Series datasheet (package page and electrical table).

- Current transfer ratio, whole series: minimum 50 %, maximum 600 % at IF = 5 mA, VCE = 5 V. Rank letters only narrow the range (L 50-100 %, A 80-160 %, B 130-260 %, C 200-400 %, D 300-600 %). Buying the unranked part guarantees only the 50 % minimum, which is what the arithmetic uses.
- LED forward voltage: typical 1.2 V, maximum 1.4 V.
- Collector-emitter saturation voltage: maximum 0.2 V (IF = 20 mA, IC = 1 mA).
- Pinout from the package drawing: 1 anode, 2 cathode, 3 emitter, 4 collector. `Isolator:PC817` has pins 1 and 2 on the LED side and 3 (emitter), 4 (collector) on the transistor side, with the same numbering. Package: 4-pin DIP, 7.62 mm row spacing, which is `Package_DIP:DIP-4_W7.62mm`.

## Transistor Q1-Q7: 2N3904

Source: onsemi 2N3903/2N3904 datasheet.

Ordered as 2N3904TF (Digi-Key 2N3904TFCT-ND, 2026-11-16): the same onsemi part on tape with formed leads, which Digi-Key names as the replacement for its discontinued plain 2N3904 listing. The formed lead spacing was not read from a drawing; the footprint is `TO-92_Inline_Wide` (2.54 mm between holes) and the leads bend to suit either way.

- hFE minimum at VCE = 1 V: 70 at 1 mA, 100 at 10 mA, 60 at 50 mA.
- VCE(sat) maximum: 0.2 V (IC 10 mA, IB 1 mA); 0.3 V (IC 50 mA, IB 5 mA).
- VBE(sat): 0.65-0.85 V (10 mA / 1 mA); up to 0.95 V (50 mA / 5 mA).
- Collector current 200 mA continuous.
- TO-92 pinout in this datasheet: 1 emitter, 2 base, 3 collector. `Transistor_BJT:2N3904` in KiCad 9.0.7 extends `Q_NPN_EBC`, which is pin 1 emitter, 2 base, 3 collector. They match. Some other makers' 2N3904 are CBE, so the bill of materials names onsemi.

## Varistor RV1-RV7: Littelfuse V56ZA2P

Source: Littelfuse, Radial Lead Varistors Datasheet, ZA Series, "Revised: GD. 01/09/24" (ratings table page 2, Product Dimensions table page 11; the copy read was downloaded from Arrow). Cross-checked against Littelfuse's ZA Varistor Series datasheet of 2011-11-13, whose dimension table has the same figures. In both copies the dimension drawing itself renders as a black box, so the letters are read from the table only.

- Maximum continuous AC voltage VM(AC): 35 V. Required: at least 30 V.
- Maximum clamping voltage VC: 110 V at 8/20 us test current 2.5 A. Required: well below the relay's 400 V maximum switching voltage (rated 250 V).
- Model size: the ratings table lists V56ZA2P as model size 7 (mm). Also VNOM 50.4-61.6 V at 1 mA DC, surge 250 A (8/20 us), energy 2.3 J, typical capacitance 1035 pF at 1 MHz.
- Product Dimensions, "7mm Size" column, for V8ZA-V56ZA where the row depends on voltage:
  - ØD, body diameter: maximum **9 mm** (not 7 mm: "7mm" is the model size, the coated disc is larger). The previous version of this file said "maximum disc diameter 7 mm"; that was wrong.
  - E, thickness: maximum **5 mm**.
  - A, seated height: maximum 12 mm.
  - e, lead spacing: 4 to 6 mm.
  - e1, lead offset: 1 to 3 mm. Taken as the leads' offset across the disc's thickness (the leads leave the two faces of the disc), which is how KiCad's stock `RV_Disc` footprints use it (pad 2 1.3 mm off the line of pad 1). The drawing that defines e1 could not be read (see above).
  - Øb, lead diameter: **0.585 to 0.685 mm**.
- The old footprint `Varistor:RV_Disc_D7mm_W3.4mm_P5mm` had 0.6 mm drills (smaller than a 0.685 mm lead) and a 7 x 3.4 mm body. It is replaced by `emberheat:Varistor_Littelfuse_ZA_7mm` (scripts/gen_lib.py, checked by `check_lib.py test_varistor`): drill 1.0 mm (0.315 mm over the largest lead, which also takes up the e1 spread), round pads 2.0 mm (0.5 mm ring), pad 2 at 2.0 mm across and 5.0 mm along from pad 1 (the middle of e1 and inside e), courtyard the maximum body 5 x 9 mm centred between the leads, plus 0.25 mm.
- Fit: between the terminal header body (to y 12.65 mm) and the relay body (from y 18.3 mm) there are about 5.6 mm, and a 5 mm thick body needs 6.0 mm with 0.5 mm each side. So the varistor no longer sits in that slot. It stands to the right of its relay, its diameter vertical: pad 1 level with relay pad 14, pad 2 level with pad 11, body x 44.1-49.1 mm and y 18.2-27.2 mm for zone 1 (18 mm further right per zone). Its body is 1.2 mm from the relay body and 5.5 mm below the terminal header body; `check_board.py test_varistor_fit` measures the bodies (F.Fab outlines) and requires 0.5 mm. The zone tracks were redrawn in `gen_board.py` to reach it (front layer, 1.5 mm). The part did not need replacing.
- Through an open contact (24 V AC across the varistor): its capacitance passes 2 pi x 60 Hz x 1035 pF x 24 V = 9.4 uA (7.8 uA at 50 Hz; 13.7 uA at its 35 V maximum). The datasheet gives no leakage (standby) current; it only defines VNOM at 1 mA DC, and 24 V RMS (34 V peak) is well under the 50.4 V minimum, so the resistive part is below 1 mA by definition and in practice far smaller, but its value is not published. A few tens of microamps at most is far below what holds a zone valve or a control board's relay input on.

## Resettable fuse F1: Bourns MF-RHT200

Source: Bourns, MF-RHT series datasheet.

- Hold current 2.0 A and trip current 3.8 A at 23 C. Required hold: 1.85-2.5 A.
- Hold current derates to 1.77 A at 40 C and 1.34 A at 70 C. Expected load is about 1 A or less (7 coils at 38.4 mA, zone lights, both radios).
- Vmax 16 V, Imax 100 A. The rail is 5 V.
- Lead spacing 5.1 mm (dimension C of the product dimensions table). Footprint `Fuse:Fuse_Bourns_MF-RHT200` (pad 1 at 0, 0; pad 2 at 5.1, 1.2 mm).
- Lead diameter: 0.51 mm nominal for the MF-RHT200 (lead diameter column of the same Product Dimensions table, "Physical Characteristics", 0.020 in; no tolerance given). The footprint's drill is 0.71 mm, 0.2 mm over. Fits. Body at most 9.4 mm wide (A) and 14.0 mm high (B).
- Ordering: `MF-RHT200-0` is the bulk-packed form from the How to Order page.

## DIN rail clip H5, H6: Hammond 1427DINCLIP

Source: Hammond 1427DINCLIP drawing PDF (hammfg.com, revision 2, 2022-03-31).

- Plastic clip for 35 mm DIN rail. Sold as a pair, with 4 panhead #6 x 1/4 in screws. Two clips per board, so one pair.
- Clip body 45.40 +/- 1.25 mm long (across the rail), 10.00 +/- 0.50 mm wide, 14.00 +/- 0.50 mm tall.
- Screw holes on the mounting face: two, diameter 3.00 mm, on the clip's centre line (5.00 mm from each long edge), 29.75 +/- 0.75 mm apart, the first 4.50 mm from the end.
- Screw: #6 x 1/4 in pan head (6.35 mm long), self-tapping into the plastic. With 1.6 mm of board, about 4.7 mm goes into the clip. Hammond does not say which thread form; for a #6 type AB (or B, the same thread) tapping screw the major diameter is 0.139 in (3.53 mm) maximum, 0.132 in (3.35 mm) minimum (Tanner "Threads for Self-Tapping Screws Type AB" table, citing ASME B18.6.3-2013); a #6 type A is 0.141 in (3.58 mm) maximum (the same source's type A table, as quoted by a search result, not read). The pan head is 0.270 in (6.86 mm) across at most, 0.256 in (6.50 mm) at least (American Fastener, "Self Tapping Screws, Pan Head" table, #6).
- The board's holes must be clearance holes: the earlier 3.5 mm hole was smaller than the screw's thread, so the screw would have cut into the board and the slot could not slide. Now 4.0 mm (0.42 mm over a 3.58 mm thread).
- Board footprint `emberheat:DIN_Clip`, one per clip: a round hole 4.0 mm at the origin and a slot 4.0 mm wide by 5.5 mm long (long axis along the hole line) centred 29.75 mm from it. The slot takes the +/- 0.75 mm spacing tolerance (5.5 = 4.0 + 2 x 0.75). The clips mount on the back of the board, so the screws go in from the front. The clip bodies and their positions are unchanged (the footprint origin and the 29.75 mm spacing did not move); `check_board.py test_mounting_holes` still finds no through-hole pad under either clip body.
- Body outline and rail position, measured on the same drawing (side and bottom views share one horizontal scale, 45.40 mm body = 35.00 mm rail span x 1.297): the fixed hook is at one end of the body, the spring latch at the other. The hole 4.50 mm from the end is at the latch end; the other hole is 11.15 mm from the hook end (45.40 - 4.50 - 29.75). The 35 mm rail starts 7.26 mm in from the hook end, so its centre line is 24.76 mm from the hook end and 20.64 mm from the latch end.
- In the footprint's frame (round hole at the origin, slot at +29.75 mm on x), with the latch-end hole on the round hole: body x -4.50 to 40.90, y -5.00 to 5.00; rail centre at x 16.14. Screwed on end for end (hook end on the round hole) it would be body x -11.15 to 34.25, rail centre at x 13.61. The board is laid out for the first way, hook end up on the wall (hooked over the top of the rail, latch snapped on at the bottom), and its back silkscreen draws the clip outline with the hook end marked.
- Gap from the back of the board to the metal rail: NOT ESTABLISHED. The same drawing (revision 2, 2022-03-31, downloaded from hammfg.com, text extracted and the page read as an image) dimensions the clip only: 14.00 +/- 0.50 mm overall height from the mounting face to the top of the hook and latch, and 13.75 mm to the top of the hook end. It gives no dimension for where the rail's flanges seat in the clip. Which part of the rail is nearest the board: an EN 60715 35 x 7.5 rail (OBO Bettermann 2069 T60 FS data sheet, item 1115380, dated 02.09.2026: 35 mm wide, 27 mm across the crown, 7.5 mm high, 1 mm steel) is mounted with its crown, the 27 mm raised section, against the wall. Its two flanges, each (35 - 27) / 2 = 4 mm wide at the outer edges, stand 7.5 mm off the wall and face the device; the clip grips them. So the nearest metal to the board is the two flange strips, and the channel between them is 7.5 mm further back. (An earlier version of this file had the rail the wrong way round, with the hat standing toward the board.) With the rail centre at board y 57.0 mm, the flanges lie under board y 39.5-43.5 mm (top) and 70.5-74.5 mm (bottom). Counted from the routed board (pads whose copper reaches into the strip): 16 plated joints over the top flange (every relay's coil pin A1, the zone lights' pad 1, C1) and 6 over the bottom flange (D8, R4, R5). The clip's seat and the rail position along the clip are read from a drawing, so allowing 2 mm either way (strips 8 mm wide): 60 over the top flange, including every relay coil pin and every optocoupler pin, and 16 over the bottom one. The reviewer counted 37 and 8 with a strip definition not recorded; the figures here are from the definition just given. Relay pins are stiff and are usually left untrimmed, so they stand furthest out; they sit over the top flange. The flange's distance from the board is the unknown: it cannot be more than the clip's 14.5 mm and the drawing gives no lower bound. The README therefore says the gap could not be established, treats it as under 5 mm, and tells the owner to measure it with a clip on a rail and to fit an insulating sheet between board and rail. Zone-voltage pads (relay contacts 11, 12, 14, varistors, terminals) reach down to y 31.4 mm (the bottom of relay pad 12, at y 30.2 mm, which carries zone voltage while the relay is off), 8 mm above the top flange strip.
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
- J1: GCT USB4125-GF-A, 6-pin power-only USB-C, 3 A (GCT category page and Digi-Key listing). Datasheet: GCT drawing USB4125 (gct.co/files/drawings/usb4125.pdf, downloaded 2026-11-15, read as an image; its revision block was not legible). Current rating 3.00 A collectively on the VBUS pins, 1.25 A on A5/B5; voltage rating 48 V DC.
  - Shell stakes against the footprint: the drawing's "Recommended PCB Layout" gives four plated slots 0.60 x 1.10 mm with 1.20 x 1.70 mm pads, 8.64 mm apart along the socket and 3.80 mm apart front to back. KiCad's `USB_C_Receptacle_GCT_USB4125-xx-x_6P_TopMnt_Horizontal` has slots 0.6 x 1.2 mm (0.1 mm longer than recommended) and pads 1.1 x 1.7 mm (0.1 mm narrower), at 8.64 mm and 3.80 mm on the board. The stakes go into slots at least as large as the maker's own layout, so they fit. The stake's own cross-section is not legible on the drawing.
  - The six signal pads (A5, A9, A12, B5, B9, B12) match the drawing's pin table: A5 CC1, A9 VBUS, A12 GND, B5 CC2, B9 VBUS, B12 GND, as on the board.

## LEDs D11-D17 (zone lights) and D8 (power light): Kingbright WP710A10ID, WP710A10GD

Sources: Kingbright WP710A10ID datasheet (Spec No DSAL0505, Rev V.12A, 08/11/2023) and WP710A10GD datasheet (Spec No DSAL0501, Rev V.10B, 04/18/2020), both from kingbrightusa.com, page 1 read as an image and the text extracted.

- The parts first listed, WP7113ID and WP7113GD, are Kingbright's 5 mm (T-1 3/4) lamps and would not seat in `LED_THT:LED_D3.0mm` beside the relays. Replaced by the 3 mm parts of the same maker and colours.
- Both: "T-1 (3mm) Solid State Lamp". Lens Ø2.9 mm, flange Ø3.2 mm, which is the 3.1 mm body circle `LED_D3.0mm` draws on its fab layer plus 0.1 mm; the courtyard is 4.5 mm wide across the line of the leads, so the flange stays inside it. Leads 0.5 mm square, 2.54 mm apart; recommended PCB layout two Ø0.9 mm holes 2.54 mm apart, which is exactly the footprint (0.9 mm drills, 2.54 mm). The drawing marks the cathode on the lead by the flat; `LED_THT:LED_D3.0mm` puts the square pad 1 and the silkscreen flat on the same side, and `Device:LED` pin 1 is K, so the cathode goes in the square pad.
- Forward voltage at 10 mA: red (WP710A10ID) 1.9 V typical, 2.3 V maximum; green (WP710A10GD) 2.0 V typical, 2.4 V maximum (both ±0.1 V measurement tolerance). DC forward current at most 30 mA.

## Drive arithmetic (one channel)

Figures: LTV-817 and 2N3904 above, relay coil 38.4 mA at 5 V.

- Optocoupler LED current, GPIO at 0 V, 3.3 V through 330 ohm: (3.3 - 1.2) / 330 = 6.4 mA typical. With the 1.4 V maximum VF it is 5.8 mA. The CTR minimum is given at 5 mA, so at least 5.8 mA is above the test point.
- Optocoupler transistor can pass at least 0.5 x 5.8 = 2.9 mA.
- Base current: the 1 k resistor alone would allow (5 - 0.2 - 0.85) / 1 k = 3.95 mA. The 10 k pull-down takes 0.85 / 10 k = 0.085 mA. So base current is between 2.8 mA (limited by the optocoupler at the minimum CTR) and 3.9 mA.
- Collector current: coil 38.4 mA, plus zone light (5 - Vf - 0.2) / 1 k. With the WP710A10ID's Vf (1.9 V typical, 2.3 V maximum at 10 mA; at 2-3 mA it is lower, and 1.6 V is kept as the lower bound for the arithmetic) that is 2.5 mA at the maximum Vf, 2.9 mA typical and at most 3.2 mA. Worst case 38.4 + 3.2 = 41.6 mA. At 5.25 V supply, 5.25 / 130 + 3.5 mA = 43.9 mA.
- Power light D8 (WP710A10GD through R3, 1 k from +5 V): (5 - 2.0) / 1 k = 3.0 mA typical, (5 - 2.4) / 1 k = 2.6 mA at the maximum Vf. Both lights stay far below the 30 mA rating.
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

## C6 relay pins at and after reset

Source: Espressif, ESP32-C6 Hardware Design Guidelines, "Schematic Checklist", section GPIO, table "IO MUX Pin Functions" (https://docs.espressif.com/projects/esp-hardware-design-guidelines/en/latest/esp32c6/schematic-checklist.html, "latest", page footer "© Copyright 2023 - 2026", downloaded with curl on 2026-11-15). Legend: IE input enabled, WPU internal weak pull-up, WPD internal weak pull-down; an empty cell is high impedance with neither. The table names some pins by their other function; the GPIO numbers are from the ESP32-C6-DevKitC-1 user guide's header table (GPIO0 = XTAL_32K_P, GPIO22 = SDIO_DATA2; GPIO1 is XTAL_32K_N).

| Zone | GPIO | Table row | At reset | After reset |
|---|---|---|---|---|
| 1 | GPIO0 | XTAL_32K_P (pin 6) | high impedance | high impedance |
| 2 | GPIO1 | XTAL_32K_N (pin 7) | high impedance | high impedance |
| 3 | GPIO2 | GPIO2 (pin 8) | IE | IE |
| 4 | GPIO3 | GPIO3 (pin 9) | IE | IE |
| 5 | GPIO10 | GPIO10 (pin 16) | high impedance | IE |
| 6 | GPIO11 | GPIO11 (pin 17) | high impedance | IE |
| 7 | GPIO22 | SDIO_DATA2 (pin 35) | WPU | IE |

- No relay pin has WPD (pull-down) in either column, and none is driven: every documented state is high impedance, input or weakly pulled up. None of the seven is a strapping pin (4, 5, 8, 9, 15 per the same page) or a USB-JTAG pin (12, 13).
- My reading agrees with the reviewer's table.
- A relay closes only if current flows from 3.3 V through 330 ohm and the optocoupler LED into the pin (drive arithmetic above), that is only if the pin is driven or pulled low. A weak pull-up or an input passes no LED current.
- Not established: what the boot ROM does with the pins in download mode (holding BOOT at reset) beyond this table, notably GPIO22 (the SDIO pins); and how the pins behave in the first milliseconds of power-up while the supply rises. Espressif publishes no power-up glitch figures for these pins. README's after-assembly checks cover both on the real board.

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
- DIN clip keep-out: see "Mounting hardware" below. The footprint's courtyard is the 4.5 mm keepout radius (a 9 mm circle on the round hole, a 10.5 x 9 mm box on the slot, covering the +/- 0.75 mm of slide).

## Mounting hardware and the copper keepout round each hole

- Largest hardware at any hole: a #6 pan head, at most 6.86 mm across (radius 3.43 mm; DIN clip section above), at the clip holes; an M3 flat washer, ISO 7089 / DIN 125 A, 7 mm outside diameter (radius 3.5 mm; RS Components A2 washer datasheet and Wurth's ISO 7089 listings, as quoted by a search; not read in full), at the corner holes if the owner uses M3 screws and washers. An M3 pan head (smaller than the washer) is covered by it.
- Keepout: no track, via or ground pour within 4.5 mm of the hole's centre, on both layers; for a slot, within 4.5 mm of any point of the line the screw's centre can slide along (+/- 0.75 mm). That is the largest radius, 3.5 mm, plus 0.5 mm, rounded up to the 4.5 mm minimum the review set. It covers a large-OD washer only up to 8 mm across: do not use larger washers.
- Pads are the parts' and some sit closer: the nearest is a dev board socket pad 4.44 mm from the centre of a bottom corner hole (A1 at H3, A2 at H4). Pads must be at least 4.0 mm away (hardware radius 3.5 + 0.5), which they are.
- `gen_board.py` draws these keepouts as rule areas (`hole_keepout_H1_1` ... `hole_keepout_H6_2`; tracks, vias and pour not allowed), so KiCad's DRC enforces them and route.py's DSN export hands them to the router as keepouts. `check_board.py test_mounting_hole_keepout` measures the distance from each hole's centre line to every piece of copper on both layers and fails below 4.5 mm (pads below 4.0 mm). On the board before this change it failed: ground pour 1.85 mm from H4, 2.00 mm from the slot centre line at H5 and H6, a +3V3 track 2.81 mm from H3.

## Measured on the routed board

Printed by `make check` (check_board.py) on the board routed in the final-review fix (varistors moved beside the relays, hole keepouts, corner supply leads, R3 moved; routing/carrier-board.ses regenerated by `make autoroute`). They change if the board is re-routed.

- Smallest zone-side to logic-side copper distance: 4.525 mm (pad K1.12 and the F.Cu ground pour); the rule is at least 3.0 mm.
- Smallest track to other-net through-hole pad: 0.502 mm (rule: at least 0.5 mm); smallest ground pour to other-net through-hole pad: 0.519 mm.
- Smallest distance from a mounting hole's centre line (corner holes, DIN clip holes, along the whole slot) to any track, via or pour, either layer: 4.500 mm (keepout 4.5 mm); nearest pad 4.439 mm (A1 J1_2 at H3; minimum 4.0 mm).
- Varistor bodies: 1.29 mm from the nearest relay body (RV1 and K1); at least 0.5 mm required.
- 0 unconnected; DRC 0 violations, 0 unconnected items, 0 schematic parity issues, no exclusions; 40 vias.
