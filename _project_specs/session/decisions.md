<!--
LOG DECISIONS WHEN:
- Choosing between architectural approaches
- Selecting libraries or tools
- Making security-related choices
- Deviating from standard patterns

This is append-only. Never delete entries.
-->

# Decision Log

Track key architectural and implementation decisions.

## Format
```
## [YYYY-MM-DD] Decision Title

**Decision**: What was decided
**Context**: Why this decision was needed
**Options Considered**: What alternatives existed
**Choice**: Which option was chosen
**Reasoning**: Why this choice was made
**Trade-offs**: What we gave up
**References**: Related code/docs
```

---

## [2026-01-07] Skills-Based Architecture Adoption

**Decision**: Adopt skills-based Claude project structure while preserving existing CLAUDE.md

**Context**: Project had comprehensive CLAUDE.md with embedded systems specifics but lacked:
- Modular skills system for reusable patterns
- Session state tracking for context preservation
- Atomic todo tracking system

**Options Considered**:
1. Replace CLAUDE.md entirely with skills
2. Keep CLAUDE.md as-is, add skills alongside
3. Integrate skills into CLAUDE.md, preserve project-specific content

**Choice**: Option 3 - Integrate skills while preserving project context

**Reasoning**:
- Existing CLAUDE.md has valuable Rust/embedded systems guidance
- Skills provide reusable patterns (security, session management)
- Hybrid approach gets benefits of both systems
- Project-specific content (GPIO, hysteresis, hardware safety) stays intact

**Trade-offs**:
- Slightly more complex structure (skills + custom instructions)
- Need to maintain both skill references and custom content

**References**: .claude/CLAUDE.md, .claude/skills/

---

## [2025-01-07] DS18B20 1-Wire Implementation

**Decision**: Use esp-hal-rmt-onewire for 1-Wire protocol

**Context**: Needed reliable 1-Wire communication for DS18B20 temperature sensors on Raspberry Pi

**Options Considered**:
1. Direct sysfs read from /sys/bus/w1/devices/
2. Custom bit-bang implementation
3. esp-hal-rmt-onewire library

**Choice**: Option 3 - esp-hal-rmt-onewire

**Reasoning**:
- Proven library with ESP32 heritage
- Handles timing-critical protocol correctly
- Better error handling than sysfs approach
- Type-safe Rust interface

**Trade-offs**:
- Additional dependency
- ESP32-focused but works on Linux GPIO

**References**: src/sensors/onewire.rs, PR #37

---

## [2024-12-25] Rust Reimplementation

**Decision**: Rewrite Python thermostat in Rust

**Context**: Python version had memory leaks, occasional crashes, and no compile-time safety

**Options Considered**:
1. Fix Python version
2. Rewrite in Go
3. Rewrite in Rust

**Choice**: Option 3 - Rust

**Reasoning**:
- Memory safety prevents GPIO corruption
- Zero-cost abstractions for embedded constraints
- Async/await for concurrent zone control
- Strong type system catches errors at compile time
- No GC pauses for real-time control loop

**Trade-offs**:
- Longer development time (type system learning curve)
- More complex dependency management (Cargo vs pip)
- GPIO library (rppal) is Linux-specific

**References**: ARCHITECTURE.md
