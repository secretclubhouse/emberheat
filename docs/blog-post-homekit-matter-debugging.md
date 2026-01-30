# Three Days of "Not Supported": A Matter Thermostat Debugging Adventure

*Hopefully someone will find this helpful. I will definitely find it helpful in a few months when I've forgotten everything and am staring at the same error message wondering why I didn't write this down.*

## The Project

I've been building a DIY smart thermostat using Rust on an ESP32-C6. It's a hydronic heating system—hot water runs through pipes to heat the house, and my thermostat controls when that happens. The whole thing uses the Matter protocol, which is supposed to be the magical standard that makes all smart home devices play nice together.

Key word: *supposed to*.

## Day 1: "This Should Be Easy"

After getting the basic thermostat logic working and adding MQTT for debugging, I started integrating Matter using rs-matter-embassy. The plan was simple: add the Matter stack, implement a thermostat cluster handler, commission to HomeKit, done.

The ESP32-C6 has both WiFi and BLE on a single radio. Matter uses BLE for commissioning and WiFi for normal operation, so in theory this chip should handle everything. I wrote the handler, started the Matter task, and tried to commission.

It... sort of worked? Sometimes. The commissioning would start, then stall, then timeout. Home Assistant could occasionally connect, but it was flaky. Something about the single-radio time-sharing between WiFi and BLE wasn't happy during the commissioning dance.

## Day 2: The Dual-Radio Detour

So of course I decided to go full yak-shave and add an ESP32-H2 as a dedicated BLE co-processor. The H2 would handle all the Bluetooth stuff over UART, leaving the C6 free to focus on WiFi.

This involved:
- Flashing the H2 with NimBLE HCI firmware (after finding out the example I initially tried doesn't even support the H2)
- Wiring up UART between the two chips
- Writing a DualRadioDriver that bridges rs-matter to the external BLE controller
- Discovering that 921600 baud was too fast and the UART RX FIFO would overflow during commissioning
- Adding hardware flow control
- Lowering the baud rate to 115200 because apparently my async UART read task wasn't getting polled fast enough

By the end of day 2, commissioning worked reliably. I could add the thermostat to Home Assistant and control it. Temperature readings, setpoint changes, heating on/off—all working great.

Then I tried HomeKit.

## The Problem

After commissioning to Apple Home (which worked fine), I was greeted with this lovely message:

> "Not Supported"

No controls. No temperature display. Just... nothing useful. The device was there, HomeKit acknowledged its existence, but apparently decided it wasn't worth talking to.

## Day 3: The Investigation

### "Maybe the temperature is NULL?"

First thing I checked: was the LocalTemperature attribute returning a valid value? Turns out it was stuck at 0x8000 (the Matter "null" value for temperatures). The zone control loop was reading sensors and updating local state, but nobody was pushing those values to the Matter state container.

Fixed that. Temperature now showed up in Home Assistant. HomeKit? Still "Not Supported."

### "Maybe it's a fabric limit thing?"

I noticed that when I tried to add chip-tool as a third fabric (after HomeKit's two), the device would panic with "No space." The default MAX_FABRICS in rs-matter is 3, and apparently HomeKit uses 2 fabrics when commissioning via BLE+WiFi.

Enabled the max-fabrics-5 feature. No more panic. HomeKit? Still "Not Supported."

### "Maybe Apple wants cooling support?"

Back when I worked with HomeKit Accessory Protocol directly, I remembered that supporting both heat and cool modes made things work better. Maybe Matter HomeKit had the same quirk?

I spent a few hours adding full cooling support. New attributes, new handlers, changed the FeatureMap from 0x01 (heat only) to 0x03 (heat + cool), updated ControlSequenceOfOperation to CoolingAndHeating. Six commits worth of changes.

Flashed the firmware. Tested with HomeKit.

Still "Not Supported."

Cool. Cool cool cool.

### "Let me just check what's actually happening"

This is where I should have started. I used chip-tool to query the thermostat directly:

```bash
chip-tool any read-by-id 0x0201 0xFFFC 1 1
```

That's asking for the FeatureMap attribute from the Thermostat cluster. The response?

```
status = 0x86 (UNSUPPORTED_ATTRIBUTE)
```

Wait, what? I literally have code that handles this attribute. I can see it right there in my handler function. Why is it returning "unsupported"?

## The Actual Problem

Here's where the ESP32 serial logs saved me. While chip-tool was complaining about unsupported attributes, my device was logging:

```
ERROR - Error processing attribute read: AttrStatus {
  cluster: Some(513), attr: Some(65532),
  status: UnsupportedAttribute
}
```

The error wasn't coming from my handler code. It was coming from somewhere upstream in rs-matter. My handler was never even being called.

After some head-scratching, I found it: rs-matter checks if an attribute exists in the cluster's attribute array *before* it calls your handler. If the attribute isn't listed, it rejects the request immediately.

And guess what wasn't in my `THERMOSTAT_ATTRIBUTES` array? The global attributes. You know, the ones every single Matter cluster needs to support:

- FeatureMap (0xFFFC)
- ClusterRevision (0xFFFD)
- AttributeList (0xFFFB)
- GeneratedCommandList (0xFFF8)
- AcceptedCommandList (0xFFF9)

I had handlers for all of them. I just forgot to put them in the array that tells rs-matter "hey, these attributes exist."

## The Fix

Five lines of code:

```rust
// --- Global Attributes (required by Matter spec) ---
Attribute::new(0xFFF8, Access::RV, Quality::NONE),
Attribute::new(0xFFF9, Access::RV, Quality::NONE),
Attribute::new(0xFFFB, Access::RV, Quality::NONE),
Attribute::new(0xFFFC, Access::RV, Quality::NONE),
Attribute::new(0xFFFD, Access::RV, Quality::F),
```

Flashed. Tested. HomeKit now shows a fully functional thermostat with temperature display and controls.

Three days of debugging. Dual-radio architecture. Full cooling support implementation. And the fix was five lines adding attributes to an array.

## What I Learned

1. **Read the device logs, not just the client logs.** chip-tool told me "unsupported attribute" but the ESP32 told me *why*—the attribute wasn't reaching my handler at all.

2. **rs-matter requires explicit attribute listing.** Unlike some Matter implementations that auto-include global attributes, rs-matter needs them in your attribute array. Makes sense for an embedded-focused library where you want full control, but it bit me hard.

3. **Start with the simplest debugging first.** I went down rabbit holes about dual-radio architectures and cooling support before I even verified that basic attribute reads were working. Should have started there. Of course it also helps to have all the pieces in place to do that debugging. I didn't want to spend the time commissioning chip-tool originally and then also took a bit to realize I needed to install a matter developer profile from Apple so BLE would work correctly during commissioning.

4. **When HomeKit says "Not Supported," it could be one broken attribute.** The error message makes it sound like the whole device type is wrong, but it might be something as simple as a missing FeatureMap response.

5. **Single-radio WiFi+BLE time-sharing on ESP32-C6 is flaky.** The dual-radio setup with an H2 co-processor does work, but that's a lot of complexity. I'm keeping it for now since it's already working.

6. **Always sync your state before Matter starts.** NULL_TEMPERATURE during commissioning can cause ecosystems to cache "not supported" status.

All that cooling support I added? Probably wasn't necessary for HomeKit. But hey, at least my heating-only thermostat can now pretend it has AC.

Now if you'll excuse me, I need to go adjust my thermostat. From my phone. Because it finally works.
