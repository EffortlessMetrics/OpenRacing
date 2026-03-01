# Getting Started with OpenRacing

This guide covers everything you need to get OpenRacing up and running with your racing wheel and simulator of choice — from installation through first use.

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Installation](#2-installation)
3. [Device Setup](#3-device-setup)
4. [Game Setup](#4-game-setup)
5. [Troubleshooting](#5-troubleshooting)
6. [CLI Reference](#6-cli-reference)

---

## 1. Prerequisites

### Rust (latest stable)

OpenRacing is built in Rust. Install the latest stable toolchain from [rustup.rs](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Windows: download and run https://win.rustup.rs
```

Minimum supported version: **Rust 1.89**.

### Git

```bash
git --version   # must be present to clone the repository
```

### Platform requirements

| Platform | Minimum version | Notes |
|----------|----------------|-------|
| **Windows** | Windows 10 (build 1903+) | Visual C++ Redistributable required |
| **Linux** | Kernel 4.0+ | udev rules required for USB device access |
| **macOS** | macOS 10.15 (Catalina)+ | IOKit HID access required |

---

## 2. Installation

### From the releases page (recommended)

Pre-built binaries for Windows, Linux, and macOS are available on the
[GitHub releases page](https://github.com/EffortlessMetrics/OpenRacing/releases).

**Windows** — run the `.msi` installer; it installs the service and CLI automatically.

**Linux** — install the `.deb` or `.rpm` package, or extract the tarball:

```bash
# Install udev rules (required for USB access without root)
sudo cp packaging/linux/99-racing-wheel-suite.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger
```

**macOS** — open the `.dmg` and drag OpenRacing to `/Applications`.

### From source

```bash
# 1. Clone the repository
git clone https://github.com/EffortlessMetrics/OpenRacing.git
cd OpenRacing

# 2. Build in release mode
cargo build --release --workspace

# 3. Install the CLI into your PATH
cargo install --path crates/cli

# 4. (Linux only) install udev rules
sudo cp packaging/linux/99-racing-wheel-suite.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

### Verifying the installation

```bash
openracing --version
openracing service start
openracing devices list
```

### Configuration locations

| Platform | Path |
|----------|------|
| Windows | `%LOCALAPPDATA%\wheel\` |
| Linux | `~/.config/wheel/` |
| macOS | `~/Library/Application Support/wheel/` |

---

## 3. Device Setup

### Plug-and-play auto-detection

Connect your wheel base via USB. The OpenRacing service detects it automatically at startup and on hot-plug. No driver installation is required on Windows 10+ or recent Linux kernels.

```bash
# Verify your device appears
openracing devices list
```

Successful output looks like:

```
ID          VENDOR          MODEL                   STATUS
dev-0       Moza Racing     R9 V2                   connected
```

### Supported devices

OpenRacing supports the following 14 vendors and their product lines out of the box.

| Vendor | USB VID | Representative models | FFB support |
|--------|---------|----------------------|-------------|
| **Moza Racing** | `0x346E` | R3, R5 V1/V2, R9 V1/V2, R12 V1/V2, R16, R21 | ✅ Serial / HID PIDFF |
| **Fanatec** | `0x0EB7` | CSL DD, GT DD Pro, Podium DD1/DD2, CSW v2.5 | ✅ Custom HID |
| **Logitech** | `0x046D` | G27, G29, G923, G Pro | ✅ HID PIDFF + TrueForce |
| **Thrustmaster** | `0x044F` | T150/Pro, TMX, T300RS/GT, TX, T500RS, T248/X, T-GT/II, TS-PC, TS-XW, T818 | ✅ HID PIDFF |
| **Simagic** | `0x0483` / `0x3670` / `0x16D0` | Alpha, Alpha Mini/EVO, M10, Neo/Mini | ✅ HID PIDFF |
| **VRS DirectForce Pro** | `0x0483` | DirectForce Pro V1/V2 (20/25 Nm) | ✅ HID PIDFF |
| **Simucube** | `0x16D0` / `0x1D50` | Simucube 2 Sport/Pro/Ultimate; Simucube 1 (IONI / ARGON) | ✅ HID PIDFF / SimpleMotion V2 |
| **Heusinkveld** | `0x04D8` | Sprint, Ultimate+, Pro pedals | Input only |
| **Cammus** | `0x3416` | C5, C12 direct drive | ✅ HID PIDFF |
| **Leo Bodnar** | `0x1DD2` | USB sim racing interfaces, load-cell brake controllers | Input only |
| **Asetek SimSports** | `0x2433` | Forte (20 Nm), Invicta (15 Nm), LaPrima (10 Nm) | ✅ HID PIDFF |
| **OpenFFBoard** | `0x1209` | All production firmware variants | ✅ HID PIDFF |
| **FFBeast** | `0x045B` | Joystick, rudder, wheel builds | ✅ HID PIDFF |
| **AccuForce** | `0x1FC9` | SimExperience AccuForce Pro | ✅ HID PIDFF |

Any USB HID device that advertises standard USB HID PID force-feedback capabilities (`Usage Page 0x000F`) is also supported as a generic device.

### Device not appearing?

1. Run `openracing devices list` — if your device is absent, check the troubleshooting section below.
2. Confirm the USB Vendor ID and Product ID match the table above using Device Manager (Windows), `lsusb` (Linux), or System Information (macOS).
3. See [Section 5 — Troubleshooting](#5-troubleshooting) for common fixes.

---

## 4. Game Setup

### Auto-detection

OpenRacing monitors running processes and automatically recognises supported simulators by their executable name. When a game is detected:

1. OpenRacing connects to the game's telemetry stream.
2. On first run it writes the required telemetry configuration to the game's documents/config folder.
3. Force feedback and telemetry data begin flowing immediately.

No manual steps are required for most games.

### Applying config manually

If auto-detection does not write the config file (e.g. the game was already running, or the config was deleted):

```bash
openracing config apply <game_id>
# Example:
openracing config apply iracing
openracing config apply acc
openracing config apply forza_motorsport
```

Run `openracing games list` to see the full list of `game_id` values.

### Supported games

| Game | ID | Status | Integration method |
|------|----|--------|--------------------|
| iRacing | `iracing` | ✅ Stable | Shared memory |
| Assetto Corsa | `assetto_corsa` | ✅ Stable | UDP OutGauge (port 9996) |
| Assetto Corsa Competizione | `acc` | ✅ Stable | UDP broadcast (port 9000) |
| DiRT Rally 2.0 | `dirt_rally_2` | ✅ Stable | Codemasters UDP mode 1 |
| Forza Motorsport / Forza Horizon | `forza_motorsport` | ✅ Stable | Forza Data Out UDP (port 5300) |
| BeamNG.drive | `beamng_drive` | ✅ Stable | UDP OutGauge (port 4444) |
| Project CARS 2 | `project_cars_2` | ✅ Stable | Shared memory |
| Automobilista 2 | `ams2` | 🧪 Experimental | Shared memory |
| rFactor 2 | `rfactor2` | 🧪 Experimental | Shared memory |
| F1 24 / F1 25 (Codemasters bridge) | `f1` | 🧪 Experimental | Codemasters UDP (port 20777) |
| F1 25 (native UDP) | `f1_25` | 🧪 Experimental | Native UDP format 2025 (port 20777) |
| EA SPORTS WRC | `eawrc` | 🧪 Experimental | UDP schema (port 20778) |
| Dirt 5 | `dirt5` | 🧪 Experimental | Codemasters UDP (port 20777) |
| Dirt 4 | `dirt4` | 🧪 Experimental | Codemasters UDP mode 1 |
| WRC Generations | `wrc_generations` | 🧪 Experimental | Codemasters UDP mode 1 |
| Gran Turismo 7 | `gran_turismo_7` | 🧪 Experimental | Salsa20-encrypted UDP (port 33740) |
| Assetto Corsa Rally | `ac_rally` | 🧪 Experimental | Probe discovery |
| Richard Burns Rally | `rbr` | 🧪 Experimental | UDP live data (port 6776) |
| RaceRoom Racing Experience | `raceroom` | 🧪 Experimental | R3E shared memory |
| Live For Speed | `live_for_speed` | 🧪 Experimental | OutSim / OutGauge UDP |
| Euro Truck Simulator 2 | `euro_truck_simulator_2` | 🧪 Experimental | SCS SDK shared memory |
| American Truck Simulator | `american_truck_simulator` | 🧪 Experimental | SCS SDK shared memory |
| Wreckfest | `wreckfest` | 🧪 Experimental | UDP telemetry |
| Rennsport | `rennsport` | 🧪 Experimental | UDP telemetry |
| GRID Autosport | `grid_autosport` | 🧪 Experimental | Codemasters UDP |
| GRID (2019) | `grid_2019` | 🧪 Experimental | Codemasters UDP |
| GRID Legends | `grid_legends` | 🧪 Experimental | Codemasters UDP |
| Automobilista 1 | `automobilista_1` | 🧪 Experimental | UDP / shared memory |
| KartKraft | `kartkraft` | 🧪 Experimental | UDP telemetry |

> **Note:** Experimental games receive telemetry and display data but may have limited or no force feedback output until the integration matures. Check the [CHANGELOG](../CHANGELOG.md) for updates.

### Game-specific notes

**iRacing** — OpenRacing writes `app.ini` changes to enable the shared memory API. iRacing must be restarted if it was already running when the config was applied.

**Forza Motorsport / Forza Horizon** — Enable "Data Out" in the HUD & Gameplay settings, set the IP to `127.0.0.1` and port to `5300`.

**Gran Turismo 7** — Runs on PlayStation; the PC must be on the same network. Enable "Send Data" in GT7's settings and point it at your PC's IP address. Process auto-detection is not available for console titles.

**ACC** — OpenRacing writes `broadcasting.json`. Restart ACC after first-time setup.

---

## 5. Troubleshooting

### Device not found

**Linux — permission denied / device absent**

The most common cause is missing udev rules:

```bash
# Install udev rules
sudo cp packaging/linux/99-racing-wheel-suite.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger

# Verify your user is in the 'plugdev' group
groups $USER
sudo usermod -aG plugdev $USER   # log out and back in after this
```

**Windows — device absent**

1. Open Device Manager and check for unknown devices under "Human Interface Devices".
2. For Fanatec wheels, install the official Fanatec driver first — it is required before OpenRacing can open the device.
3. If the device shows a yellow warning icon, right-click → "Update driver" → "Search automatically".

**All platforms — VID/PID not in supported list**

Run `openracing devices scan --all` to list every HID device with its VID and PID. Cross-reference with the table in [Section 3](#3-device-setup). If your device is missing, please open an issue on GitHub with the VID, PID, and device name.

---

### Game not detected

1. Confirm the game is running: `openracing games status`
2. Check that the executable name matches what is expected:

```bash
openracing games list --verbose   # shows expected process names
```

3. If the process name differs (e.g. a non-Steam install path), edit the game entry in the support matrix or open an issue.

4. Apply config manually and restart the game:

```bash
openracing config apply <game_id>
```

---

### No force feedback

1. **In-game FFB must be enabled.** Most simulators have a dedicated Force Feedback setting (strength or percentage). Set it to a non-zero value.
2. Check the service is running: `openracing service status`
3. Check the device is connected and shows `connected` in `openracing devices list`.
4. Check diagnostics for fault codes: `openracing diag report`
5. Review service logs:
   - Windows: `%LOCALAPPDATA%\wheel\logs\`
   - Linux/macOS: `~/.config/wheel/logs/`

---

### Service fails to start

```bash
# Check for port conflicts or permission errors
openracing service start --foreground

# Reset to defaults if config is corrupt
openracing service reset-config
```

---

### Generating a support bundle

If you need help from the community or want to file a bug report, generate a support bundle:

```bash
openracing diag bundle --output ~/openracing-bundle.zip
```

The bundle contains sanitised logs, device enumeration, and system info. No personal or game data is included.

---

## 6. CLI Reference

The `openracing` CLI communicates with the background service over a local IPC socket. The service must be running for most commands.

### Service management

```bash
openracing service start          # start the background service
openracing service stop           # stop the background service
openracing service status         # show running/stopped and uptime
openracing service restart        # stop then start
```

### Device commands

```bash
openracing devices list                    # list all connected and known devices
openracing devices list --verbose          # include VID/PID and firmware version
openracing devices scan --all              # enumerate all HID devices (for debugging)
openracing devices status <device-id>      # detailed status for one device
openracing devices calibrate <device-id>   # run interactive calibration wizard
```

### Game commands

```bash
openracing games list                      # list all supported games with status
openracing games list --verbose            # include process names and config paths
openracing games status                    # show currently detected/active game
```

### Configuration commands

```bash
openracing config apply <game_id>          # write telemetry config to game folder
openracing config apply --all              # apply config for all installed games
openracing config show <game_id>           # print the config that would be written
openracing config verify <game_id>         # check if config is present and valid
```

### Profile commands

```bash
openracing profile list                    # list available FFB profiles
openracing profile apply <device-id> <profile.json>   # apply a profile
openracing profile export <device-id>      # export current settings as a profile
```

### Diagnostics

```bash
openracing diag test                       # run built-in hardware self-test
openracing diag report                     # print fault log and current health
openracing diag bundle --output <file.zip> # create a support bundle
openracing health                          # quick one-line health summary
```

---

## Further reading

- [README](../README.md) — project overview and feature summary
- [User Guide](USER_GUIDE.md) — in-depth usage and profile editing
- [System Integration](SYSTEM_INTEGRATION.md) — detailed game and hardware integration notes
- [Plugin Development](PLUGIN_DEVELOPMENT.md) — writing WASM or native plugins
- [Power Management Guide](POWER_MANAGEMENT_GUIDE.md) — suspend/resume and USB power settings
- [Contributing](CONTRIBUTING.md) — how to report issues and submit patches
