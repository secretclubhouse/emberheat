# Understanding the 1-Wire Search Algorithm

This document explains how the 1-Wire search algorithm works, the bug we encountered, and how we fixed it. Written for developers who are new to embedded systems or the 1-Wire protocol.

## Table of Contents

1. [What is 1-Wire?](#what-is-1-wire)
2. [The Problem: Finding Multiple Devices](#the-problem-finding-multiple-devices)
3. [How the Search Algorithm Works](#how-the-search-algorithm-works)
4. [The Bug We Found](#the-bug-we-found)
5. [How We Fixed It](#how-we-fixed-it)
6. [Code Walkthrough](#code-walkthrough)

---

## What is 1-Wire?

1-Wire is a communication protocol invented by Dallas Semiconductor (now Maxim). It's called "1-Wire" because it only needs one data wire (plus ground) to communicate with devices.

**Key facts about 1-Wire:**
- Each device has a unique 64-bit address (like a serial number)
- Multiple devices can share the same wire
- The DS18B20 temperature sensor uses 1-Wire

**Our setup:**
```
ESP32 GPIO Pin ────┬──── DS18B20 Sensor #1 (address: 28-240100008fd4)
                   │
                   ├──── DS18B20 Sensor #2 (address: 28-240100007b6e)
                   │
                   └──── DS18B20 Sensor #3 (address: 28-240100006f7f)
                   │
              4.7kΩ Resistor to 3.3V (pull-up)
```

All three sensors share the same wire! So how does the ESP32 know which sensor is which?

---

## The Problem: Finding Multiple Devices

When you have multiple devices on the same wire, you need to **discover** each device's unique address before you can talk to it individually.

**The naive approach doesn't work:**

If we just said "Hey, everyone tell me your address!", all three sensors would respond at the same time. Their signals would collide and we'd get garbage data.

```
ESP32: "What's your address?"

Sensor 1: "28-D4-8F-00-00-01-24-24"  ─┐
Sensor 2: "28-6E-7B-00-00-01-24-55"  ─┼──► COLLISION! Garbage data received
Sensor 3: "28-7F-6F-00-00-01-24-5A"  ─┘
```

**The solution: Search Algorithm**

The 1-Wire search algorithm cleverly finds one device at a time by having devices "drop out" of the conversation based on their address bits.

---

## How the Search Algorithm Works

### The Basic Idea

Think of it like a game of "20 Questions" but with binary (0 or 1):

1. ESP32 asks: "Does your address have a 0 or 1 in bit position 0?"
2. Devices with 0 stay in the conversation, devices with 1 drop out (or vice versa)
3. Repeat for all 64 bits
4. At the end, only ONE device is still "talking" - we've found its address!

### Step-by-Step Example

Let's use simplified 8-bit addresses to make this easier:

```
Sensor A: 11010100 (D4 in hex)
Sensor B: 01101110 (6E in hex)
Sensor C: 01111111 (7F in hex)
```

**Iteration 1 - Finding Sensor A:**

| Bit | Sensor A | Sensor B | Sensor C | Both Values Present? | We Choose | Who Stays? |
|-----|----------|----------|----------|---------------------|-----------|------------|
| 0   | 0        | 0        | 1        | Yes (0 and 1)       | 0         | A, B       |
| 1   | 0        | 1        | 1        | Yes (0 and 1)       | 0         | A only     |
| 2   | 1        | 1        | 1        | No (all 1)          | 1         | A only     |
| ... | ...      | ...      | ...      | ...                 | ...       | ...        |

After going through all 8 bits, we've found Sensor A's address: `11010100`

**Key insight:** At bits 0 and 1, we saw BOTH 0 and 1 values (a "discrepancy"). We chose 0 both times. We need to remember these positions so we can go back and try 1 later!

**Iteration 2 - Finding Sensor B:**

We "backtrack" to bit 1 (the last place we chose 0 when both were available) and this time choose 1:

| Bit | What We Do | Who Stays? |
|-----|------------|------------|
| 0   | Choose 0 (same as before) | A, B, C... wait, C has 1 here |
| 0   | Actually, let's check... | B stays (has 0), C drops out (has 1) |
| 1   | Choose 1 (backtrack!) | B stays, A drops out |
| ... | Follow B's bits | B only |

We find Sensor B's address: `01101110`

**Iteration 3 - Finding Sensor C:**

Now we backtrack to bit 0 (we chose 0 before, now try 1):

| Bit | What We Do | Who Stays? |
|-----|------------|------------|
| 0   | Choose 1 (backtrack!) | C only (A and B have 0) |
| ... | Follow C's bits | C only |

We find Sensor C's address: `01111111`

**Iteration 4 - No more devices:**

We've explored all branches of the "tree". Search complete!

### Visualizing as a Tree

The search explores a binary tree where each branch is a bit choice:

```
                    Start
                   /     \
              bit0=0     bit0=1
               /  \          \
          bit1=0  bit1=1     ...
            |       |
         Sensor A  Sensor B    Sensor C
```

---

## The Bug We Found

### The Symptom

When we connected 3 specific sensors, only 2 were found:

```
Sensor 1: 28-240100008fd4 ✓ Found
Sensor 2: 28-240100007b6e ✓ Found  
Sensor 3: 28-240100006f7f ✗ NOT FOUND!
```

But when we swapped sensor 2 for a different one, all 3 were found! The bug only appeared with certain combinations of addresses.

### Debug Output

We added logging to see what was happening:

```
Search iteration 1...
  Bit 8: DISCREPANCY
  Bit 9: DISCREPANCY
  Found: 28-240100008fd4, last_zero=Some(9)

Search iteration 2...
  Bit 8: DISCREPANCY  
  Bit 9: DISCREPANCY
  Found: 28-240100007b6e, last_zero=None  ← BUG! Should not be None!

Search iteration 3...
  Search complete (no more devices)  ← We stopped too early!
```

### The Problem

Look at iteration 2: `last_zero=None`

This means "we didn't find any new places to backtrack to", so the algorithm thought it was done. But there was still sensor 3 waiting to be found!

**What should have happened:**

In iteration 2, at bit 8, we saw a discrepancy (both 0 and 1 present). We followed the 0 path (because that's what we did in iteration 1). But we should have recorded bit 8 as a place to backtrack to later!

### The Root Cause

Here's the buggy logic (simplified):

```rust
// At a discrepancy (both 0 and 1 present on bus)
if bit_position == backtrack_point {
    // We're at our backtrack point - take the 1 path
    choose 1
} else if bit_position < backtrack_point {
    // Before backtrack point - follow same path as last time
    follow_previous_path()  // ← BUG: We didn't record this!
} else {
    // After backtrack point - take 0 path and remember it
    last_zero = bit_position
    choose 0
}
```

The bug: When we're BEFORE the backtrack point and we follow the 0 path again, we weren't recording it as a potential future backtrack point!

---

## How We Fixed It

The fix was simple - when following the previous path, check if it was 0, and if so, record it:

```rust
// At a discrepancy (both 0 and 1 present on bus)
if bit_position == backtrack_point {
    // We're at our backtrack point - take the 1 path
    choose 1
} else if bit_position < backtrack_point {
    // Before backtrack point - follow same path as last time
    let previous_choice = get_previous_bit(bit_position);
    
    // ✅ FIX: If we're taking 0, remember this spot!
    if previous_choice == 0 {
        last_zero = bit_position
    }
    
    choose previous_choice
} else {
    // After backtrack point - take 0 path and remember it
    last_zero = bit_position
    choose 0
}
```

### Why This Works

**Before the fix:**
- Iteration 1: Discrepancy at bits 8,9. Take 0 at both. Record `last_zero=9`. Find sensor 1.
- Iteration 2: Backtrack to bit 9, take 1. At bit 8, follow previous (0) but DON'T record it. `last_zero=None`. Find sensor 2. Think we're done!

**After the fix:**
- Iteration 1: Discrepancy at bits 8,9. Take 0 at both. Record `last_zero=9`. Find sensor 1.
- Iteration 2: Backtrack to bit 9, take 1. At bit 8, follow previous (0) AND record `last_zero=8`. Find sensor 2. NOT done yet!
- Iteration 3: Backtrack to bit 8, take 1. Find sensor 3. Now `last_zero=None`. Done!

---

## Code Walkthrough

Here's the actual fixed code with detailed comments:

```rust
pub async fn next(&mut self, ow: &mut OneWire) -> Result<Address, SearchError> {
    // If we already found all devices, stop
    if self.complete {
        return Err(SearchError::SearchComplete);
    }

    // Send reset pulse - all devices wake up and respond
    let have_devices = ow.reset().await?;
    if !have_devices {
        return Err(SearchError::NoDevicesPresent);
    }

    // This will track the LAST bit position where we took the 0 path
    // at a discrepancy. This is where we'll backtrack next iteration.
    let mut last_zero_discrepancy: Option<usize> = None;

    // Tell all devices "we're doing a search"
    ow.send_byte(0xF0).await?; // SEARCH ROM command

    // Check each of the 64 address bits
    for bit_position in 0..64 {
        // Read two bits from the bus:
        // - First bit: the actual bit value
        // - Second bit: the complement (opposite)
        //
        // What the combinations mean:
        // [0, 1] = All remaining devices have 0 at this position
        // [1, 0] = All remaining devices have 1 at this position
        // [0, 0] = DISCREPANCY! Some devices have 0, some have 1
        // [1, 1] = No devices responding (error)
        let id_bits = ow.exchange_bits([true, true]).await?;

        let search_direction = match id_bits {
            // All devices have 0 - no choice, go with 0
            [false, true] => false,

            // All devices have 1 - no choice, go with 1
            [true, false] => true,

            // No devices - something went wrong
            [true, true] => {
                return Err(SearchError::NoDevicesPresent);
            }

            // DISCREPANCY - this is where the interesting logic happens!
            [false, false] => {
                if self.last_discrepancy == Some(bit_position) {
                    // Case 1: This is our backtrack point
                    // Last time we took 0 here, now take 1
                    // Don't record as last_zero (we're done with this branch)
                    true
                } else if let Some(backtrack_pos) = self.last_discrepancy {
                    if bit_position < backtrack_pos {
                        // Case 2: Before our backtrack point
                        // Follow the same path we took last time
                        let previous_bit = (self.address & (1 << bit_position)) != 0;
                        
                        // ✅ THE FIX IS HERE!
                        // If we're following a 0 path, record this position
                        // so we can come back and try 1 later
                        if !previous_bit {
                            last_zero_discrepancy = Some(bit_position);
                        }
                        
                        previous_bit
                    } else {
                        // Case 3: After our backtrack point
                        // This is new territory - take 0 first, explore 1 later
                        last_zero_discrepancy = Some(bit_position);
                        false
                    }
                } else {
                    // Case 4: First search, no backtrack point yet
                    // Take 0 first, we'll come back for 1 later
                    last_zero_discrepancy = Some(bit_position);
                    false
                }
            }
        };

        // Update our running address with this bit
        if search_direction {
            self.address |= 1 << bit_position;   // Set bit to 1
        } else {
            self.address &= !(1 << bit_position); // Set bit to 0
        }

        // Tell devices which path we're taking
        // Devices that don't match will stop responding
        ow.exchange_bits([search_direction]).await?;
    }

    // Save the last discrepancy for next iteration
    self.last_discrepancy = last_zero_discrepancy;
    
    // If there's no place to backtrack to, we've found all devices
    self.complete = last_zero_discrepancy.is_none();

    // Return the address we found
    Ok(Address(self.address))
}
```

---

## Summary

**The 1-Wire search algorithm:**
1. Explores a binary tree of possible addresses
2. At each "discrepancy" (where devices differ), it picks 0 first
3. Records where it picked 0 so it can backtrack and try 1 later
4. Repeats until all branches are explored

**The bug:**
- When following a previous 0-path at a discrepancy BEFORE the backtrack point, we weren't recording it as a future backtrack point

**The fix:**
- Always record the position when taking a 0-path at a discrepancy, regardless of whether it's before or after the backtrack point

**The lesson:**
- Binary search trees are tricky! The standard 1-Wire search algorithm from Maxim's application notes has this subtlety, and it's easy to miss when implementing from scratch.

---

## References

- [Maxim/Dallas 1-Wire Search Algorithm Application Note](https://www.analog.com/en/technical-articles/1wire-search-algorithm.html)
- [DS18B20 Datasheet](https://www.analog.com/media/en/technical-documentation/data-sheets/DS18B20.pdf)
- Our implementation: `emberheat-esp32/src/onewire_rmt.rs`
