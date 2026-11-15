# EmberHeat carrier board

A KiCad board that holds an ESP32-C6 and an ESP32-H2 dev board, seven relay outputs for zone valves, and a USB-C power input. It mounts on a DIN rail with two clips.

Status: parts chosen (`bom.csv`, `part-checks.md`). The schematic and board are not drawn yet.

## Files

- `bom.csv`: one row per distinct part, with the exact KiCad symbol and footprint names.
- `part-checks.md`: datasheet figures that justify each part, and the drive arithmetic.
- `scripts/check_bom.py`: checks the bill of materials.

## Checks

```
make check
```

Runs every `scripts/check_*.py`. Needs KiCad 9 at `/Applications/KiCad/KiCad.app`.

## Power

Use a 5 V USB-C supply rated 3 A. A supply that offers only 0.5-0.9 A will sag when several relays close and reset the C6, which opens the relays. That is safe but a nuisance.
