# Game Support Matrix

> Auto-generated reference. Run `python scripts/validate_game_matrix.py` to
> validate adapter coverage and test status.

## Overview

OpenRacing supports **61 game adapters** covering sim-racing, rally, truck simulation,
and arcade titles. Each adapter translates game-specific telemetry into the
normalised `NormalizedTelemetry` format consumed by the force-feedback engine.

Adapters are registered in
[`crates/telemetry-adapters/src/lib.rs`](../crates/telemetry-adapters/src/lib.rs)
via `adapter_factories()`.

## Support Status Key

| Status | Meaning |
|--------|---------|
| **Verified** | Full protocol implementation with dedicated crate tests and snapshot coverage |
| **Tested** | Adapter implemented with integration-level test coverage |
| **Experimental** | Adapter implemented but limited real-world validation |
| **Stub** | Placeholder — no native telemetry protocol documented yet |

## Game Support Matrix

### Sim Racing

| Game | Adapter ID | Data Source | Status | Dedicated Crate |
|------|-----------|-------------|--------|-----------------|
| Assetto Corsa Competizione | `acc` | UDP | Verified | — |
| Assetto Corsa Competizione 2 | `acc2` | UDP | Tested | — |
| Assetto Corsa EVO | `ac_evo` | UDP | Tested | — |
| Assetto Corsa | `assetto_corsa` | UDP | Tested | — |
| Automobilista 2 (AMS2) | `ams2` | Shared Memory | Verified | `telemetry-ams2` |
| Automobilista 1 | `automobilista` | Shared Memory | Tested | — |
| iRacing | `iracing` | Shared Memory | Verified | — |
| Le Mans Ultimate | `le_mans_ultimate` | UDP | Tested | — |
| Project CARS 2 | `project_cars_2` | Shared Memory | Verified | — |
| Project CARS 3 | `project_cars_3` | UDP | Tested | — |
| RaceRoom Racing Experience | `raceroom` | Shared Memory | Verified | `telemetry-raceroom` |
| Rennsport | `rennsport` | UDP | Verified | `telemetry-rennsport` |
| rFactor 2 | `rfactor2` | Shared Memory | Verified | — |
| rFactor 1 | `rfactor1` | UDP | Tested | — |
| GTR 2 | `gtr2` | UDP (rFactor 1) | Experimental | — |
| RACE 07 | `race_07` | UDP (rFactor 1) | Experimental | — |
| Game Stock Car | `gsc` | UDP (rFactor 1) | Experimental | — |
| NASCAR Heat | `nascar` | UDP | Tested | — |
| NASCAR 21: Ignition | `nascar_21` | UDP | Tested | — |
| WTCR | `wtcr` | UDP | Tested | — |
| Wreckfest | `wreckfest` | UDP | Tested | — |
| Ride 5 | `ride5` | UDP (JSON) | Experimental | — |
| MotoGP | `motogp` | UDP (JSON) | Experimental | — |

### Formula / Open Wheel

| Game | Adapter ID | Data Source | Status | Dedicated Crate |
|------|-----------|-------------|--------|-----------------|
| EA Sports F1 (2023) | `f1` | UDP | Verified | `telemetry-f1` |
| EA Sports F1 25 | `f1_25` | UDP | Tested | — |
| F1 Manager | `f1_manager` | UDP | Tested | — |
| F1 (Native) | `f1_native` | UDP | Tested | — |

### Open World / Arcade

| Game | Adapter ID | Data Source | Status | Dedicated Crate |
|------|-----------|-------------|--------|-----------------|
| Forza Motorsport | `forza_motorsport` | UDP | Verified | `telemetry-forza` |
| Forza Horizon 4 | `forza_horizon_4` | UDP | Tested | — |
| Forza Horizon 5 | `forza_horizon_5` | UDP | Tested | — |
| Gran Turismo 7 | `gran_turismo_7` | UDP (Encrypted) | Verified | — |
| Gran Turismo Sport | `gran_turismo_sport` | UDP (Encrypted) | Tested | — |
| BeamNG.drive | `beamng_drive` | UDP (OutGauge) | Verified | — |
| FlatOut | `flatout` | UDP | Tested | — |
| TrackMania | `trackmania` | UDP (JSON) | Tested | — |
| KartKraft | `kartkraft` | UDP (FlatBuffers) | Verified | `telemetry-kartkraft` |
| Live for Speed | `live_for_speed` | UDP (OutGauge) | Verified | `telemetry-lfs` |

### Rally / Off-Road

| Game | Adapter ID | Data Source | Status | Dedicated Crate |
|------|-----------|-------------|--------|-----------------|
| EA Sports WRC | `eawrc` | UDP | Tested | — |
| WRC Generations | `wrc_generations` | UDP | Verified | `telemetry-wrc-generations` |
| WRC 9 | `wrc_9` | UDP | Tested | — |
| WRC 10 | `wrc_10` | UDP | Tested | — |
| DiRT Rally 2.0 | `dirt_rally_2` | UDP | Verified | — |
| DiRT 3 | `dirt3` | UDP | Tested | — |
| DiRT 4 | `dirt4` | UDP | Tested | — |
| DiRT 5 | `dirt5` | UDP | Tested | — |
| DiRT Showdown | `dirt_showdown` | UDP | Tested | — |
| Dakar Desert Rally | `dakar_desert_rally` | UDP | Tested | — |
| Sébastien Loeb Rally EVO | `seb_loeb_rally` | — | Stub | — |
| V-Rally 4 | `v_rally_4` | UDP | Tested | — |
| Gravel | `gravel` | UDP (JSON) | Experimental | — |
| AC Rally | `ac_rally` | UDP | Tested | — |
| MudRunner | `mudrunner` | UDP (JSON) | Verified | `telemetry-mudrunner` |
| SnowRunner | `snowrunner` | UDP (JSON) | Tested | — |

### GRID / Codemasters Racing

| Game | Adapter ID | Data Source | Status | Dedicated Crate |
|------|-----------|-------------|--------|-----------------|
| GRID (2019) | `grid_2019` | UDP | Tested | — |
| GRID Autosport | `grid_autosport` | UDP | Tested | — |
| GRID Legends | `grid_legends` | UDP | Tested | — |
| Race Driver: GRID | `race_driver_grid` | UDP | Tested | — |

### Truck Simulation

| Game | Adapter ID | Data Source | Status | Dedicated Crate |
|------|-----------|-------------|--------|-----------------|
| Euro Truck Simulator 2 | `ets2` | Shared Memory | Tested | — |
| American Truck Simulator | `ats` | Shared Memory | Tested | — |

### Bridge / Generic

| Game | Adapter ID | Data Source | Status | Dedicated Crate |
|------|-----------|-------------|--------|-----------------|
| SimHub (Generic Bridge) | `simhub` | UDP (JSON) | Verified | `telemetry-simhub` |

## Data Sources

| Type | Description | Platform |
|------|-------------|----------|
| **UDP** | Game sends telemetry packets over localhost UDP | Cross-platform |
| **UDP (Encrypted)** | Encrypted UDP packets (Salsa20) | Cross-platform |
| **UDP (OutGauge)** | LFS OutGauge protocol over UDP | Cross-platform |
| **UDP (FlatBuffers)** | FlatBuffers-serialised UDP packets | Cross-platform |
| **UDP (JSON)** | JSON-formatted telemetry over UDP | Cross-platform |
| **Shared Memory** | Windows memory-mapped file (or platform shim) | Windows primary |

## Test Coverage

Every registered adapter is validated by the CI-integrated script
`scripts/validate_game_matrix.py`. Games with dedicated crates
(`telemetry-ams2`, `telemetry-f1`, etc.) carry additional unit and
snapshot tests. The main `telemetry-adapters` crate contains 35+
integration and snapshot test files covering all adapters.

## Protocol Documentation

Detailed protocol specs are maintained under `docs/protocols/`:

- [F1 Telemetry Protocol](protocols/F1_TELEMETRY.md)
- [DiRT 5 Telemetry Protocol](protocols/DIRT5_TELEMETRY.md)

For hardware protocol documentation, see the [Protocols README](protocols/README.md).
