# Rename: thermos-rust → EmberHeat

## Context

The project originated as a Rust port of a Python thermostat project called "Thermos" that ran on Raspberry Pi. It has since evolved into a Matter-enabled embedded thermostat for ESP32-C6 — a fundamentally different project. Before making the repo public, rename everything from "thermos" to "emberheat" to give it a clean, independent identity.

## Naming Convention

| Context | Old | New |
|---------|-----|-----|
| Repository | `thermos-rust` | `emberheat` |
| GitHub | `josht/thermos-rust` | `secretclubhouse/emberheat` |
| Prose | Thermos-Rust | EmberHeat |
| Workspace crates | `thermos-core`, `thermos-matter`, `thermos-esp32`, `thermos-linux` | `emberheat-core`, `emberheat-matter`, `emberheat-esp32`, `emberheat-linux` |
| Rust imports | `thermos_core`, `thermos_matter` | `emberheat_core`, `emberheat_matter` |
| Binary | `thermos-esp32` | `emberheat-esp32` |
| Service | `thermos-rust.service` | `emberheat.service` |
| Author | `Thermos Contributors` | `Josh Tambunga` |
| Matter device name | `Thermos Multi-Zone` | `EmberHeat Multi-Zone` |

## What NOT to Rename

- **`ThermostatState`**, **`ZoneThermostatState`**, **`Thermostat`** (type names) — "thermostat" is the domain term
- **Thermostat Cluster** references — this is the Matter spec name (Cluster 0x0201)
- **MQTT topic `thermostat/`** — generic topic name, not project branding
- **`prometheus.yml` job name `thermostat`** — describes function (moot: file being deleted)
- **Grafana dashboard title** — describes function (moot: directory being deleted)

## Phases

Work bottom-up: update file contents first, rename directories last for cleaner diffs.

### Phase 1 — Delete stale files

Remove old status docs, pre-workspace root `src/` and `tests/`, Linux-era monitoring configs.

**Root-level stale docs:**
- `WEEK2_STATUS.md`, `WEEK2_FINAL_STATUS.md`
- `ESP32_STATUS.md`, `ESP32_TESTS.md`
- `COMMIT_SUMMARY.md`, `NO_STD_MIGRATION_COMPLETE.md`
- `PROJECT_SUMMARY.md`, `QUICK_START.md`
- `ESP32_ADAPTATION_ANALYSIS.md`, `ESP32_RISCV_ADAPTATION_PLAN.md`
- `GITHUB_ACTIONS_SETUP.md`

**thermos-esp32/ stale docs:**
- `WEEK3_STATUS.md`, `WEEK3_COMPLETE.md`, `WEEK3_TESTING.md`, `WEEK3_TESTS_SUMMARY.md`

**Pre-workspace root code (no longer used):**
- `src/` directory (entire tree)
- `tests/` directory (entire tree)
- `build.rs` (root-level, pre-workspace)
- `prometheus.yml`
- `grafana/` directory

### Phase 2 — Update Cargo.toml files

All 5 Cargo.toml files:
- `name = "thermos-*"` → `name = "emberheat-*"`
- `path = "../thermos-*"` → `path = "../emberheat-*"`
- `authors = ["Thermos Contributors"]` → `authors = ["Josh Tambunga"]`
- `repository` URL → `https://github.com/secretclubhouse/emberheat`
- `description` text — update project name references
- Comments referencing package names

### Phase 3 — Update Rust source files

Search-and-replace across all `.rs` files:
- `use thermos_core::` → `use emberheat_core::`
- `use thermos_matter::` → `use emberheat_matter::`
- `thermos_core::` → `emberheat_core::` (in any path context)
- `thermos_matter::` → `emberheat_matter::` (in any path context)

**User-visible strings to update:**
- `thermos-esp32/src/config.rs`: `MATTER_DEVICE_NAME` → `"EmberHeat Multi-Zone"`
- `thermos-esp32/src/config.rs`: MQTT device ID default → `"emberheat-esp32"`
- `thermos-matter/src/device.rs`: default device name → `"EmberHeat Multi-Zone"`
- `thermos-matter/src/device.rs`: test assertion for device name

**Comments** referencing "thermos-esp32", "thermos-core", etc. in `.rs` files should be updated.

### Phase 4 — Update Makefile

- Header: "Thermos-Rust" → "EmberHeat"
- Echo: "Thermos-Rust Build System" → "EmberHeat Build System"
- All `-p thermos-core` → `-p emberheat-core`
- All `-p thermos-matter` → `-p emberheat-matter`
- All `-p thermos-esp32` → `-p emberheat-esp32`
- Binary path: `target/.../thermos-esp32` → `target/.../emberheat-esp32`
- MQTT_DEVICE_ID default: `thermos-esp32` → `emberheat-esp32`
- `cd thermos-esp32` → `cd emberheat-esp32`

### Phase 5 — Update CI/CD and config files

- `.github/workflows/ci.yml` — package references (`-p thermos-core`)
- `.github/workflows/release.yml` — package refs, binary names, archive names, release title
- `.github/workflows/dependencies.yml` — package refs
- `scripts/pre-commit` — header comment, package references
- `thermos-esp32/.cargo/config.toml` — comment
- `.env.example` — header comment, MQTT_DEVICE_ID example
- `.devcontainer/devcontainer.json` — container name (if applicable)

### Phase 6 — Update documentation

- `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `TESTING.md`
- `MATTER_INTEGRATION.md`
- `docs/*.md` (all docs files)
- `thermos-esp32/README.md`, `thermos-esp32/DS18B20_IMPLEMENTATION.md`, `thermos-esp32/MQTT_TESTING.md`
- `.devcontainer/README.md`, `.github/workflows/README.md`
- `docs/plans/*.md` (plan docs)
- `.claude/CLAUDE.md` — full rewrite of project references

### Phase 7 — Update LICENSE files

- Update `"Thermos Contributors"` → `"Josh Tambunga"` in any license files

### Phase 8 — Rename service file

- `thermos-rust.service` → `emberheat.service`
- Update Description, WorkingDirectory, binary paths, SyslogIdentifier inside

### Phase 9 — Rename crate directories

Do this last, after all file contents are updated:

```bash
git mv thermos-core emberheat-core
git mv thermos-matter emberheat-matter
git mv thermos-esp32 emberheat-esp32
git mv thermos-linux emberheat-linux
```

### Phase 10 — Regenerate Cargo.lock and verify

1. `cargo generate-lockfile` (or just run build/test)
2. `make test` — all tests pass
3. `make build` — thermos-core builds
4. `make lint` — no clippy warnings
5. `grep -ri "thermos" --include="*.rs" --include="*.toml" --include="*.yml"` — should return zero hits (Cargo.lock auto-content excepted)
6. Verify Cargo.lock has new crate names

### Phase 11 — Update git remote

```bash
git remote set-url origin git@github.com:secretclubhouse/emberheat.git
```

(After creating the new repo on GitHub.)
