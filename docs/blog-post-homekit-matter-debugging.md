# Why My Matter Thermostat Hated Apple (And How I Fixed It)

*Hopefully someone will find this helpful. I will definitely find it helpful in a few months when I've forgotten everything and am staring at the same error message wondering why I didn't write this down.*

## The Setup

I've been building a DIY smart thermostat using Rust on an ESP32-C6. It's a hydronic heating system - basically, hot water runs through pipes to heat the house, and my thermostat controls when that happens. The whole thing uses the Matter protocol, which is supposed to be the magical standard that makes all smart home devices play nice together.

Key word: *supposed to*.

The thermostat worked great with Home Assistant. Temperature readings, setpoint changes, heating on and off - everything just worked. So naturally, I figured adding it to Apple HomeKit would be equally straightforward.

I was wrong.

## The Problem

After commissioning the device to HomeKit (which worked fine, by the way), I was greeted with this lovely message:

> "This accessory is not currently supported by the Home"

No controls. No temperature display. Just... nothing useful. The device was there, HomeKit acknowledged its existence, but apparently decided it wasn't worth talking to.

## The Investigation

### Attempt 1: Maybe Apple Wants Cooling Too?

My first thought was that Apple might be picky about thermostats that only support heating. Back when I worked with HomeKit Accessory Protocol directly, I remembered that supporting both heat and cool modes made things work better.

So I spent a good chunk of time adding full cooling support. New attributes, new handlers, changed the FeatureMap from 0x01 (heat only) to 0x03 (heat + cool), updated ControlSequenceOfOperation - the works.

Flashed the firmware. Tested with HomeKit.

Still "Not Supported."

Cool. Cool cool cool.

### Attempt 2: Developer Profiles?

Some searching led me to discover that Apple might reject devices using test vendor IDs (like the 0xFFF1 I was using) unless you have the Matter Developer Profile installed on your iOS device and home hub.

I already had it installed. Dead end.

### Attempt 3: Actually Look at What's Happening

This is where I should have started. I used chip-tool to poke at the device directly:

```bash
chip-tool any read-by-id 0x0201 0xFFFC 1 1
```

That's asking for the FeatureMap attribute from the Thermostat cluster. The response?

```
status = 0x86 (UNSUPPORTED_ATTRIBUTE)
```

Wait, what? I literally have code that handles this. I can see it right there in my handler. Why is it returning "unsupported"?

## The Actual Problem

Here's where the ESP32 serial logs saved me. While chip-tool was complaining about unsupported attributes, my device was logging:

```
ERROR - Error processing attribute read: AttrStatus {
  cluster: Some(513), attr: Some(65532),
  status: UnsupportedAttribute
}
```

The error wasn't coming from my handler code. It was coming from somewhere upstream in rs-matter (the Rust Matter library I'm using). My handler was never even being called.

After digging through the code, I found it: rs-matter checks if an attribute exists in the cluster's attribute array *before* it calls your handler. If the attribute isn't listed, it rejects the request immediately.

And guess what wasn't in my `THERMOSTAT_ATTRIBUTES` array? The global attributes. You know, the ones every Matter cluster needs:

- FeatureMap (0xFFFC)
- ClusterRevision (0xFFFD)
- AttributeList (0xFFFB)
- And a couple others

I had handlers for all of them. I just forgot to put them in the array that tells rs-matter "hey, these attributes exist."

## The Fix

Five lines of code:

```rust
// --- Global Attributes (required by Matter spec) ---
Attribute::new(0xFFF8, Access::RV, Quality::NONE),  // GeneratedCommandList
Attribute::new(0xFFF9, Access::RV, Quality::NONE),  // AcceptedCommandList
Attribute::new(0xFFFB, Access::RV, Quality::NONE),  // AttributeList
Attribute::new(0xFFFC, Access::RV, Quality::NONE),  // FeatureMap
Attribute::new(0xFFFD, Access::RV, Quality::F),     // ClusterRevision
```

Flashed. Tested. HomeKit now shows a fully functional thermostat with temperature display and controls.

All that cooling support I added? Probably wasn't necessary. But hey, at least my heating-only thermostat can now pretend it has AC.

## What I Learned

1. **Read the device logs, not just the client logs.** chip-tool told me "unsupported attribute" but the ESP32 told me *why* - the attribute wasn't reaching my handler at all.

2. **rs-matter requires explicit attribute listing.** Unlike some Matter implementations that auto-include global attributes, rs-matter needs them in your attribute array. Makes sense for an embedded-focused library where you want full control, but it's easy to miss.

3. **Start with the simplest debugging first.** I went down rabbit holes about Apple profiles and cooling support before I even verified that basic attribute reads were working. Should have started there.

4. **When HomeKit says "Not Supported," it might just mean one attribute is broken.** The error message makes it sound like the whole device type is wrong, but it could be something as simple as a missing FeatureMap response.

Now if you'll excuse me, I need to go adjust my thermostat. From my phone. Because it finally works.
