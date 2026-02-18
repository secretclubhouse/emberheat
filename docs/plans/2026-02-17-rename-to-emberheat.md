# Rename thermos-rust → EmberHeat: Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Rename the project from "thermos-rust" to "emberheat" across all crates, source files, configs, and docs.

**Architecture:** Mechanical find-and-replace with careful attention to the distinction between "thermos" (project name → rename) and "thermostat" (domain term → keep). Work bottom-up: update file contents first, rename directories last.

**Tech Stack:** Rust workspace, ESP32-C6, Matter protocol

---

### Task 1: Delete stale root-level docs

**Files:**
- Delete: `WEEK2_STATUS.md`
- Delete: `WEEK2_FINAL_STATUS.md`
- Delete: `ESP32_STATUS.md`
- Delete: `ESP32_TESTS.md`
- Delete: `COMMIT_SUMMARY.md`
- Delete: `NO_STD_MIGRATION_COMPLETE.md`
- Delete: `PROJECT_SUMMARY.md`
- Delete: `QUICK_START.md`
- Delete: `ESP32_ADAPTATION_ANALYSIS.md`
- Delete: `ESP32_RISCV_ADAPTATION_PLAN.md`
- Delete: `GITHUB_ACTIONS_SETUP.md`

**Step 1: Delete the files**

```bash
git rm WEEK2_STATUS.md WEEK2_FINAL_STATUS.md ESP32_STATUS.md ESP32_TESTS.md \
  COMMIT_SUMMARY.md NO_STD_MIGRATION_COMPLETE.md PROJECT_SUMMARY.md QUICK_START.md \
  ESP32_ADAPTATION_ANALYSIS.md ESP32_RISCV_ADAPTATION_PLAN.md GITHUB_ACTIONS_SETUP.md
```

**Step 2: Commit**

```bash
git commit -m "chore: delete stale status and migration docs"
```

---

### Task 2: Delete stale thermos-esp32 docs

**Files:**
- Delete: `thermos-esp32/WEEK3_STATUS.md`
- Delete: `thermos-esp32/WEEK3_COMPLETE.md`
- Delete: `thermos-esp32/WEEK3_TESTING.md`
- Delete: `thermos-esp32/WEEK3_TESTS_SUMMARY.md`

**Step 1: Delete the files**

```bash
git rm thermos-esp32/WEEK3_STATUS.md thermos-esp32/WEEK3_COMPLETE.md \
  thermos-esp32/WEEK3_TESTING.md thermos-esp32/WEEK3_TESTS_SUMMARY.md
```

**Step 2: Commit**

```bash
git commit -m "chore: delete stale ESP32 week 3 status docs"
```

---

### Task 3: Delete pre-workspace root code and Linux-era configs

**Files:**
- Delete: `src/` (entire directory)
- Delete: `tests/` (entire directory)
- Delete: `build.rs` (root-level)
- Delete: `prometheus.yml`
- Delete: `grafana/` (entire directory)

**Step 1: Delete files**

```bash
git rm -r src/ tests/ build.rs prometheus.yml grafana/
```

**Step 2: Commit**

```bash
git commit -m "chore: delete pre-workspace root src/, tests/, and Linux-era monitoring configs"
```

---

### Task 4: Update workspace root Cargo.toml

**Files:**
- Modify: `Cargo.toml`

**Step 1: Update all thermos references**

Replace in `Cargo.toml`:
- `"thermos-core"` → `"emberheat-core"` (in members and default-members)
- `"thermos-matter"` → `"emberheat-matter"`
- `"thermos-linux"` → `"emberheat-linux"`
- `"thermos-esp32"` → `"emberheat-esp32"`
- Comment: `# Default to ESP32 development - thermos-linux is kept but not actively developed`
  → `# Default to ESP32 development - emberheat-linux is kept but not actively developed`
- Comment: `# in thermos-esp32/Cargo.toml to expose...`
  → `# in emberheat-esp32/Cargo.toml to expose...`

**Step 2: Verify syntax**

No build yet — directories haven't been renamed.

---

### Task 5: Update thermos-core/Cargo.toml

**Files:**
- Modify: `thermos-core/Cargo.toml`

**Step 1: Update package metadata**

- `name = "thermos-core"` → `name = "emberheat-core"`
- `authors = ["Thermos Contributors"]` → `authors = ["Josh Tambunga"]`
- `description = "Platform-agnostic thermostat control logic"` → keep as-is ("thermostat" is domain term)

---

### Task 6: Update thermos-matter/Cargo.toml

**Files:**
- Modify: `thermos-matter/Cargo.toml`

**Step 1: Update package metadata**

- `name = "thermos-matter"` → `name = "emberheat-matter"`
- Update description if it mentions "thermos" (keep "thermostat" references)

---

### Task 7: Update thermos-esp32/Cargo.toml

**Files:**
- Modify: `thermos-esp32/Cargo.toml`

**Step 1: Update package metadata and dependencies**

- `name = "thermos-esp32"` → `name = "emberheat-esp32"`
- `thermos-core = { path = "../thermos-core"...` → `emberheat-core = { path = "../emberheat-core"...`
- `thermos-matter = { path = "../thermos-matter" }` → `emberheat-matter = { path = "../emberheat-matter" }`
- All comments: `-p thermos-esp32` → `-p emberheat-esp32`
- Comment: `# Matter thermostat cluster implementation` → keep ("thermostat" is domain)

---

### Task 8: Update thermos-linux/Cargo.toml

**Files:**
- Modify: `thermos-linux/Cargo.toml`

**Step 1: Update package metadata and dependencies**

- `name = "thermos-linux"` → `name = "emberheat-linux"`
- `authors = ["Thermos Contributors"]` → `authors = ["Josh Tambunga"]`
- `repository = "https://github.com/josht/thermos-rust"` → `repository = "https://github.com/secretclubhouse/emberheat"`
- `thermos-core = { path = "../thermos-core"...` → `emberheat-core = { path = "../emberheat-core"...`

**Step 2: Commit all Cargo.toml changes**

```bash
git add Cargo.toml thermos-core/Cargo.toml thermos-matter/Cargo.toml thermos-esp32/Cargo.toml thermos-linux/Cargo.toml
git commit -m "chore: rename crates from thermos-* to emberheat-* in Cargo.toml files"
```

---

### Task 9: Update thermos-core Rust source files

**Files:**
- Modify: `thermos-core/src/lib.rs` — update doc comment example `use thermos_core::` → `use emberheat_core::`

**Step 1: Update imports in doc comments**

In `thermos-core/src/lib.rs`, lines 23-25: replace `use thermos_core::` with `use emberheat_core::` in the example code.

**Important:** Do NOT rename any type names containing "thermostat" (e.g., `ThermostatState`). Only rename project/crate references.

**Note:** `thermos-core/src/types.rs`, `control.rs`, `config.rs`, `persistence.rs` only contain "thermostat" (domain term), not "thermos" (project name). No changes needed in those files.

---

### Task 10: Update thermos-matter Rust source files

**Files:**
- Modify: `thermos-matter/src/lib.rs` — doc comment example imports
- Modify: `thermos-matter/src/device.rs` — comment referencing `thermos-esp32`, device name, serial number
- Modify: `thermos-matter/src/state.rs` — comments referencing `thermos-core`

**Step 1: Update lib.rs**

In doc comment example code, replace `use thermos_matter::` with `use emberheat_matter::`.

**Step 2: Update device.rs**

- Line 4 comment: `thermos-esp32` → `emberheat-esp32`
- Line 88: `device_name: "Thermos Multi-Zone"` → `device_name: "EmberHeat Multi-Zone"`
- Line 89: `serial_number: "THERMOS-001"` → `serial_number: "EMBERHEAT-001"`
- Line 162 test: `assert_eq!(info.device_name, "Thermos Multi-Zone")` → `"EmberHeat Multi-Zone"`

**Step 3: Update state.rs**

- Line 10 comment: `thermos-core control loop` → `emberheat-core control loop`
- Line 21 comment: `thermos-core control loop` → `emberheat-core control loop`
- Doc comment examples: `use thermos_matter::` → `use emberheat_matter::`

**Important:** Do NOT rename type names (`ZoneThermostatState`, `ThermostatState`), function names (`thermostat_running_state`), or Matter cluster references. These use "thermostat" as a domain term.

---

### Task 11: Update thermos-esp32 Rust source files — imports

**Files:**
- Modify: All `.rs` files in `thermos-esp32/src/` that have `use thermos_core::` or `use thermos_matter::`

**Step 1: Replace crate imports across all thermos-esp32 .rs files**

Find and replace in all `.rs` files under `thermos-esp32/`:
- `use thermos_core::` → `use emberheat_core::`
- `thermos_core::` → `emberheat_core::` (in path context, e.g., `thermos_core::ZoneConfig`)
- `use thermos_matter::` → `use emberheat_matter::`
- `thermos_matter::` → `emberheat_matter::` (in path context)
- `pub use thermos_matter::` → `pub use emberheat_matter::`
- `use thermos_esp32::` → `use emberheat_esp32::` (in test files)

**Specific files with imports:**
- `src/main.rs:70` — `use thermos_core::{...}`
- `src/gpio_impl.rs:7` — `use thermos_core::{GpioError, OutputPin};`
- `src/sensor_onewire.rs:18` — `use thermos_core::{...}`
- `src/sensor_i2c.rs:18` — `use thermos_core::{...}`
- `src/sensors.rs:33` — `use thermos_core::{...}`
- `src/zone.rs:130` — `thermos_matter::MAX_ZONES`
- `src/controller.rs:46` — `use thermos_core::{...}`
- `src/persistence_nvs.rs:17` — `use thermos_core::{...}`
- `src/persistence/types.rs:5` — `use thermos_matter::types::SystemMode;`
- `src/persistence/reset.rs:11` — `use thermos_matter::types::{...}`
- `src/matter/mod.rs:23` — `pub use thermos_matter::*;`
- `src/matter/bridge.rs:47` — `use thermos_matter::types::SystemMode;`
- `src/matter/zones.rs:6` — `use thermos_matter::state::ZoneThermostatState;`
- `src/matter/zones.rs:42` — `thermos_matter::endpoint_to_zone_index(...)`
- `src/mocks.rs:11` — `use thermos_core::{...}`
- `tests/integration_tests.rs` — `use thermos_core::{...}`
- `tests/mock_tests.rs` — `use thermos_core::{...}`
- `tests/wifi_tests.rs` — `use thermos_esp32::WifiConfig;`

---

### Task 12: Update thermos-esp32 Rust source files — comments and strings

**Files:**
- Modify: Various `.rs` files in `thermos-esp32/src/`

**Step 1: Update doc comments referencing project crate names**

Replace `thermos-core` → `emberheat-core` and `thermos-esp32` → `emberheat-esp32` and `thermos-matter` → `emberheat-matter` in doc comments (NOT in "thermostat" domain references).

Specific files:
- `src/lib.rs:6` — `thermos-core traits` → `emberheat-core traits`
- `src/gpio_impl.rs:3` — `thermos-core OutputPin trait` → `emberheat-core OutputPin trait`
- `src/gpio_impl.rs:9` — `thermos-core OutputPin trait` → `emberheat-core OutputPin trait`
- `src/sensor_onewire.rs:3` — `thermos-core TemperatureSensor trait` → `emberheat-core TemperatureSensor trait`
- `src/sensor_i2c.rs:3` — `thermos-core TemperatureSensor trait` → `emberheat-core TemperatureSensor trait`
- `src/zone.rs:9` — `thermos_matter::MAX_ZONES` → `emberheat_matter::MAX_ZONES`
- `src/matter/mod.rs:1,3,7,9` — all `thermos-esp32`, `thermos-matter`, `thermos-core` → `emberheat-*`
- `src/matter/bridge.rs:1,5` — `thermos-core` → `emberheat-core`
- `src/matter/stack.rs:1` — `thermos-esp32` → `emberheat-esp32`
- `src/matter/stack.rs:9` — `thermos-matter` → `emberheat-matter`

**Step 2: Update user-visible strings**

- `src/config.rs:206` comment: `"thermos-esp32"` → `"emberheat-esp32"`
- `src/config.rs:209` default: `"thermos-esp32"` → `"emberheat-esp32"`
- `src/config.rs:247`: `"Thermos Multi-Zone"` → `"EmberHeat Multi-Zone"`
- `src/mqtt.rs:71`: `"thermos-{}"` → `"emberheat-{}"` (MQTT client ID)
- `src/matter/stack.rs:1105`: `serial_no: "thermos-001"` → `serial_no: "emberheat-001"`

**Important:** Do NOT change MQTT topics like `"thermostat/{}/temp/{}"` — "thermostat" is the domain term here.

---

### Task 13: Update thermos-linux Rust source files

**Files:**
- Modify: All `.rs` files in `thermos-linux/src/`

**Step 1: Replace crate imports**

- `thermos-linux/build.rs:1` — `thermos-rust` → `emberheat`
- `src/main.rs` — `use thermos_core::` → `use emberheat_core::`; `"Thermos-Rust"` → `"EmberHeat"` in strings; `/etc/thermos-rust/` → `/etc/emberheat/`; `~/.config/thermos-rust/` → `~/.config/emberheat/`
- `src/config/mod.rs:53` — `pub use thermos_core::ZoneConfig;` → `pub use emberheat_core::ZoneConfig;`
- `src/persistence/mod.rs:50` — `pub use thermos_core::` → `pub use emberheat_core::`
- `src/gpio/mod.rs:43` — `pub use thermos_core::` → `pub use emberheat_core::`
- `src/sensors/mod.rs:36` — `pub use thermos_core::` → `pub use emberheat_core::`
- `src/metrics/mod.rs` — `thermos-rust` references in config paths

**Important:** Keep `"thermostat:"` Redis key prefix and domain-term references.

**Step 2: Commit all Rust source changes**

```bash
git add -A '*.rs'
git commit -m "refactor: rename thermos imports and strings to emberheat across all crates"
```

---

### Task 14: Update Makefile

**Files:**
- Modify: `Makefile`

**Step 1: Replace all thermos references**

- Line 1: `Thermos-Rust (ESP32-focused)` → `EmberHeat (ESP32-focused)`
- Line 17: `"Thermos-Rust Build System (ESP32)"` → `"EmberHeat Build System (ESP32)"`
- All `-p thermos-core` → `-p emberheat-core`
- All `-p thermos-matter` → `-p emberheat-matter`
- All `-p thermos-esp32` → `-p emberheat-esp32`
- All `cd thermos-esp32` → `cd emberheat-esp32`
- All `target/riscv32imac-unknown-none-elf/release/thermos-esp32` → `target/riscv32imac-unknown-none-elf/release/emberheat-esp32`
- Line 54: `MQTT_DEVICE_ID=thermos-esp32` → `MQTT_DEVICE_ID=emberheat-esp32`
- Echo text: `thermos-core` → `emberheat-core`, `thermos-matter` → `emberheat-matter`

**Step 2: Commit**

```bash
git add Makefile
git commit -m "chore: rename thermos to emberheat in Makefile"
```

---

### Task 15: Update CI/CD workflows

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/dependencies.yml`

**Step 1: Update ci.yml**

- `thermos-core` → `emberheat-core` (in all `-p` flags and comments)
- `thermos-matter` → `emberheat-matter` (if referenced)

**Step 2: Update release.yml**

- `thermos-linux` → `emberheat-linux` (binary names, archive names, `-p` flags)
- `Thermos-Rust` → `EmberHeat` (release title)
- All download/install instructions mentioning `thermos-linux`

**Step 3: Update dependencies.yml**

- `thermos-core` → `emberheat-core`
- `thermos-linux` → `emberheat-linux`

**Step 4: Commit**

```bash
git add .github/workflows/
git commit -m "chore: rename thermos to emberheat in CI/CD workflows"
```

---

### Task 16: Update scripts and config files

**Files:**
- Modify: `scripts/pre-commit`
- Modify: `thermos-esp32/.cargo/config.toml`
- Modify: `.env.example`
- Modify: `.devcontainer/devcontainer.json`

**Step 1: Update pre-commit**

- Line 3: `Thermos-Rust` → `EmberHeat`
- Line 34 comment: `thermos-core` → `emberheat-core`
- Line 36: `-p thermos-core` → `-p emberheat-core`
- Line 45 comment: `thermos-core` → `emberheat-core`
- Line 47: `-p thermos-core` → `-p emberheat-core`

**Step 2: Update .cargo/config.toml**

- Line 1: `thermos-esp32` → `emberheat-esp32`

**Step 3: Update .env.example**

- Line 1: `Thermos-Rust` → `EmberHeat`
- Line 10: `thermos-esp32/src/config.rs` → `emberheat-esp32/src/config.rs`
- Line 13: `thermos-esp32` → `emberheat-esp32`

**Step 4: Update devcontainer.json**

- Line 2: `"name": "Thermos-Rust Development"` → `"name": "EmberHeat Development"`

**Step 5: Commit**

```bash
git add scripts/pre-commit thermos-esp32/.cargo/config.toml .env.example .devcontainer/devcontainer.json
git commit -m "chore: rename thermos to emberheat in scripts and config files"
```

---

### Task 17: Update service file

**Files:**
- Modify + Rename: `thermos-rust.service` → `emberheat.service`

**Step 1: Update contents**

- `Description=Thermos-Rust Multi-Zone Thermostat` → `Description=EmberHeat Multi-Zone Thermostat`
- `Documentation=https://github.com/josht/thermos-rust` → `Documentation=https://github.com/secretclubhouse/emberheat`
- `WorkingDirectory=/opt/thermos-rust` → `WorkingDirectory=/opt/emberheat`
- `ExecStart=/opt/thermos-rust/thermos-rust` → `ExecStart=/opt/emberheat/emberheat-linux`
- `SyslogIdentifier=thermos-rust` → `SyslogIdentifier=emberheat`
- Comments: `/etc/thermos-rust/` → `/etc/emberheat/`; `/opt/thermos-rust/thermos-rust` → `/opt/emberheat/emberheat-linux`

**Step 2: Rename file**

```bash
git mv thermos-rust.service emberheat.service
```

**Step 3: Commit**

```bash
git add emberheat.service
git commit -m "chore: rename thermos-rust.service to emberheat.service"
```

---

### Task 18: Update README.md

**Files:**
- Modify: `README.md`

**Step 1: Replace all project name references**

- `thermos-rust` → `emberheat`, `Thermos-Rust` → `EmberHeat`, `thermos-core` → `emberheat-core`, `thermos-esp32` → `emberheat-esp32`, `thermos-matter` → `emberheat-matter`, `thermos-linux` → `emberheat-linux`
- Repository URLs → `secretclubhouse/emberheat`
- Keep "thermostat" domain references unchanged

**Step 2: Commit**

```bash
git add README.md
git commit -m "docs: rename thermos to emberheat in README"
```

---

### Task 19: Update ARCHITECTURE.md, CONTRIBUTING.md, TESTING.md, MATTER_INTEGRATION.md

**Files:**
- Modify: `ARCHITECTURE.md`
- Modify: `CONTRIBUTING.md`
- Modify: `TESTING.md`
- Modify: `MATTER_INTEGRATION.md`

**Step 1: Replace all project name references in each file**

Same pattern as README: `thermos-*` → `emberheat-*`, `Thermos-Rust` → `EmberHeat`, repo URLs.

**Step 2: Commit**

```bash
git add ARCHITECTURE.md CONTRIBUTING.md TESTING.md MATTER_INTEGRATION.md
git commit -m "docs: rename thermos to emberheat in project documentation"
```

---

### Task 20: Update docs/ directory

**Files:**
- Modify: `docs/*.md` (all markdown files in docs/)
- Modify: `docs/debugging/*.md`
- Modify: `docs/plans/*.md`

**Step 1: Replace project name references in all docs**

Replace `thermos-*` → `emberheat-*`, `Thermos-Rust` → `EmberHeat` in all docs files. Keep "thermostat" domain references.

**Step 2: Commit**

```bash
git add docs/
git commit -m "docs: rename thermos to emberheat in docs directory"
```

---

### Task 21: Update ESP32 docs

**Files:**
- Modify: `thermos-esp32/README.md`
- Modify: `thermos-esp32/DS18B20_IMPLEMENTATION.md`
- Modify: `thermos-esp32/MQTT_TESTING.md`

**Step 1: Replace project name references**

`thermos-*` → `emberheat-*`, `Thermos-Rust` → `EmberHeat`.

**Step 2: Commit**

```bash
git add thermos-esp32/README.md thermos-esp32/DS18B20_IMPLEMENTATION.md thermos-esp32/MQTT_TESTING.md
git commit -m "docs: rename thermos to emberheat in ESP32 docs"
```

---

### Task 22: Update .devcontainer/README.md and .github/workflows/README.md

**Files:**
- Modify: `.devcontainer/README.md`
- Modify: `.github/workflows/README.md`

**Step 1: Replace project name references**

- `.devcontainer/README.md`: `Thermos-Rust` → `EmberHeat`, `thermos_rust` → `emberheat`
- `.github/workflows/README.md`: `thermos-*` → `emberheat-*`

**Step 2: Commit**

```bash
git add .devcontainer/README.md .github/workflows/README.md
git commit -m "docs: rename thermos to emberheat in devcontainer and workflow READMEs"
```

---

### Task 23: Update .claude/CLAUDE.md

**Files:**
- Modify: `.claude/CLAUDE.md`

**Step 1: Replace all project name references**

This file has ~26 occurrences. Replace:
- `Thermos-Rust` → `EmberHeat`
- `thermos-rust` → `emberheat`
- `thermos-core` → `emberheat-core`
- `thermos-matter` → `emberheat-matter`
- `thermos-esp32` → `emberheat-esp32`
- `thermos-linux` → `emberheat-linux`
- `thermos_core` → `emberheat_core`
- `thermos_matter` → `emberheat_matter`
- Directory tree paths: `thermos-core/` → `emberheat-core/`, etc.

Keep "thermostat" domain references (e.g., "thermostat system", "Thermostat Cluster").

**Step 2: Commit**

```bash
git add .claude/CLAUDE.md
git commit -m "docs: rename thermos to emberheat in CLAUDE.md"
```

---

### Task 24: Rename crate directories

**Step 1: Rename all four crate directories**

```bash
git mv thermos-core emberheat-core
git mv thermos-matter emberheat-matter
git mv thermos-esp32 emberheat-esp32
git mv thermos-linux emberheat-linux
```

**Step 2: Commit**

```bash
git commit -m "refactor: rename crate directories from thermos-* to emberheat-*"
```

---

### Task 25: Regenerate Cargo.lock and verify build

**Step 1: Regenerate lockfile**

```bash
cargo generate-lockfile
```

**Step 2: Run tests**

```bash
make test
```

Expected: All tests pass with new crate names.

**Step 3: Build core**

```bash
make build
```

Expected: Builds successfully.

**Step 4: Run linter**

```bash
make lint
```

Expected: No clippy warnings.

**Step 5: Commit updated lockfile**

```bash
git add Cargo.lock
git commit -m "chore: regenerate Cargo.lock with emberheat crate names"
```

---

### Task 26: Verify no remaining "thermos" references

**Step 1: Search for remaining references**

```bash
grep -ri "thermos" --include="*.rs" --include="*.toml" --include="*.yml" --include="*.json" --include="*.md" --include="*.service" --include="*.sh" .
```

Expected: Zero hits, except:
- `Cargo.lock` may have auto-generated content (acceptable)
- `docs/plans/2026-02-17-rename-to-emberheat-design.md` references old names in context (acceptable)
- Blog/debugging docs may reference the original Python "Thermos" project (acceptable)

**Step 2: Fix any remaining references found**

If any unexpected references remain, update them and amend the relevant commit.

---

### Task 27: Update git remote (manual)

This step is done manually after the new GitHub repo exists.

```bash
git remote set-url origin git@github.com:secretclubhouse/emberheat.git
```
