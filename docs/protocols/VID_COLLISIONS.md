# USB Vendor ID Collisions

## Overview

Several racing peripheral manufacturers share a single USB Vendor ID (VID)
because they use off-the-shelf USB chipsets instead of registering a
vendor-specific VID with the USB-IF. The OpenRacing dispatch layer
disambiguates these devices by **Product ID (PID)** — the second half of the
USB identity pair — so every device still routes to the correct protocol
handler.

This document catalogues the known collisions, the PID ranges each vendor
occupies, and the risks if a future device introduces an overlapping PID.

---

## Shared VID: `0x0483` — STMicroelectronics

The STM32 USB device library ships with the ST default VID `0x0483`.
Three vendors use this VID:

| Vendor | PID range | Known PIDs | Notes |
|--------|-----------|------------|-------|
| **VRS DirectForce** | `0xA355`–`0xA35A`, `0xA3BE`, `0xA44C` | DFP (`0xA355`), DFP V2 (`0xA356`), Pedals V1 (`0xA357`), Pedals V2 (`0xA358`), Handbrake (`0xA359`), Shifter (`0xA35A`), Pedals (`0xA3BE`), R295 (`0xA44C`) | Confirmed from hardware |
| **Cube Controls** | `0x0C73`–`0x0C75` | GT Pro (`0x0C73`), Formula Pro (`0x0C74`), CSX3 (`0x0C75`) | **Provisional** — not confirmed from real hardware |
| **Simagic (legacy)** | `0x0522` | Alpha / M10 (`0x0522`) | Legacy firmware; modern Simagic uses VID `0x3670` |

### Dispatch order

```text
0x0483 + PID
  ├─ is_vrs_product(pid)?        → VrsProtocolHandler
  ├─ is_cube_controls_product(pid)?  → CubeControlsProtocolHandler
  └─ (default)                   → SimagicProtocol (legacy fallback)
```

VRS PIDs are checked first, then Cube Controls. Any PID not claimed by
either is assumed to be a legacy Simagic device.

### Conflict risk

The Cube Controls PIDs are **provisional and unverified**. If Cube Controls
ships hardware with a different PID in the `0x0Cxx` range, the
`is_cube_controls_product()` guard must be updated. A new STM32-based
vendor whose PID overlaps `0xA3xx` or `0x0C7x` would collide and require
an additional guard.

---

## Shared VID: `0x16D0` — MCS Electronics / OpenMoko

VID `0x16D0` is resold by MCS Electronics (formerly OpenMoko) for
low-volume USB projects. Three vendors share it:

| Vendor | PID range | Known PIDs | Notes |
|--------|-----------|------------|-------|
| **Heusinkveld** | `0x1156`–`0x1158` | Sprint (`0x1156`), Ultimate+ (`0x1157`), Pro (`0x1158`) | Pedal devices (no FFB) — PIDs not externally verified |
| **Simucube** | `0x0D5A`–`0x0D66` | SC1 (`0x0D5A`), Ultimate (`0x0D5F`), Pro (`0x0D60`), Sport (`0x0D61`), Wireless Wheel (`0x0D63`), ActivePedal (`0x0D66`) | Confirmed from Granite Devices documentation |
| **Simagic / Simucube 1 (fallback)** | unspecified | Any PID not matched above | Legacy catch-all |

### Dispatch order

```text
0x16D0 + PID
  ├─ is_heusinkveld_product(pid)?  → HeusinkveldProtocolHandler
  ├─ is_simucube_product(pid)?     → SimucubeProtocolHandler
  └─ (default)                     → SimagicProtocol (legacy fallback)
```

Heusinkveld is checked first (pedal devices), then Simucube (wheelbases).
Any remaining PID falls through to the Simagic legacy handler.

### Conflict risk

The Heusinkveld PIDs (`0x115x`) and Simucube PIDs (`0x0D5x`–`0x0D6x`)
are well-separated. A future Simucube peripheral in the `0x0D6x`–`0x0D7x`
range is safe. However, a new MCS-based vendor whose PID falls outside
both known ranges would silently route to the Simagic legacy handler, which
may produce incorrect behaviour.

---

## Shared VID: `0x1209` — pid.codes (Open Hardware)

VID `0x1209` is the open-hardware shared VID managed by pid.codes.
Two device classes share it:

| Vendor | PID range | Known PIDs | Notes |
|--------|-----------|------------|-------|
| **OpenFFBoard** | `0xFFB0`–`0xFFB1` | Main (`0xFFB0`), Alt (`0xFFB1`) | `0xFFB0` confirmed; `0xFFB1` unverified |
| **Button Box** | `0x1BBD` | Generic button box (`0x1BBD`) | Input-only, no FFB |

### Dispatch order

```text
0x1209 + PID
  ├─ is_openffboard_product(pid)?  → OpenFFBoardHandler
  ├─ is_button_box_product(pid)?   → ButtonBoxProtocolHandler
  └─ (default)                     → None (no handler)
```

Unknown PIDs on this VID return `None`, unlike the STM and MCS VIDs which
have a legacy fallback.

### Conflict risk

The pid.codes registry allocates PIDs to individual projects, so collisions
are unlikely in practice. New open-hardware devices just need a new guard
function added to the dispatch.

---

## VID `0x1D50` — Granite Devices / OpenMoko (Hardware)

VID `0x1D50` is used exclusively by Granite Devices for SimpleMotion V2
controllers (IONI, IONI Premium, ARGON, OSW). This VID is **not currently
shared** with other vendors in the dispatch table.

| Device | PID | Notes |
|--------|-----|-------|
| IONI | `0x6050` | SimpleMotion V2 servo drive |
| IONI Premium | `0x6051` | SimpleMotion V2 premium variant |
| ARGON | `0x6052` | SimpleMotion V2 ARGON drive |

All PIDs route to `SimpleMotionProtocolHandler`. No disambiguation is
needed today, but the VID is technically an OpenMoko-era allocation and
could appear on unrelated hardware.

---

## Verifying No VID+PID Duplicates

The integration test `vid_pid_registry_has_no_duplicates` in
`crates/engine/tests/vid_pid_dispatch_verification.rs` programmatically
collects every known VID+PID pair from all protocol crates and asserts that
no two vendors claim the same combination.

Run it with:

```bash
cargo test --package racing-wheel-engine --test vid_pid_dispatch_verification
```

---

## Adding a New Device on a Shared VID

1. **Check this document** — verify the PID does not overlap an existing
   range.
2. **Add a guard function** (e.g., `is_newvendor_product(pid)`) in the
   vendor handler module.
3. **Insert the guard** in `get_vendor_protocol()` in
   `crates/engine/src/hid/vendor/mod.rs`, *above* the legacy fallback arm.
4. **Add PID constants** to the protocol crate's `ids.rs`.
5. **Update this document** with the new PID range.
6. **Run the duplicate check** to confirm no collision was introduced.
