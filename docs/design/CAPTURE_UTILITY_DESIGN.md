# OpenRacing Capture Utility Design

## Overview

The `openracing-capture` utility is a standalone tool designed to democratize the reverse engineering of racing wheels. Instead of requiring users to be experts in Wireshark or USB protocols, this tool gamifies the process of mapping device inputs and capturing initialization sequences.

## Architecture

The tool is split into two distinct modes of operation: **Mapping** (Safe, Easy) and **Sniffing** (Advanced, Admin-only).

### 1. Mapper Mode (Input Mapping)
* **Goal**: Identify which bit in the HID report corresponds to which physical button/axis.
* **Tech Stack**: `hidapi` (Cross-platform HID access).
* **Workflow**:
    1.  **Detection**: List all connected HID devices and let user select the wheel.
    2.  **Baseline**: Read 50 frames of "hands off" data to establish a baseline.
    3.  **Prompt & Detect**:
        *   "Please turn the wheel 90 degrees right." -> Detect largest changing `u16` or `i16`.
        *   "Please press the Throttle." -> Detect axis change.
        *   "Please press Button A." -> Detect bit toggle.
    4.  **Verification**: Ask user to press the detected input again to confirm.
    5.  **Output**: Generate `device_map.json`.

### 2. Sniffer Mode (Protocol Capture)
* **Goal**: Capture the "Magic Bytes" sent by the OEM driver to initialize the wheel (enable FFB).
* **Tech Stack**:
    *   **Windows**: Wrapper around `USBPcap` (requires installation).
    *   **Linux**: `usbmon` (kernel module).
* **Workflow**:
    1.  **Setup**: Instruct user to close all wheel software.
    2.  **Start Capture**: Tool starts listening to the specific USB Bus/Device.
    3.  **Trigger**: Instruct user to "Open the OEM Driver Software (e.g., Pit House)".
    4.  **Capture**: Record the first 5 seconds of `OUT` packets (Host -> Device).
    5.  **Filter**: automatically strip standard Windows descriptors requests, isolating the vendor-specific "Magic Bytes".
    6.  **Output**: Append initialization sequence to `device_map.json`.

## Device Definition Schema (`device.json`)

```json
{
  "info": {
    "vendor_id": "0x1234",
    "product_id": "0x5678",
    "name": "Moza R9",
    "manufacturer": "Moza Racing"
  },
  "protocol": {
    "init_sequence": [
      { "report_id": 0x01, "payload": "AA55..." }
    ],
    "ffb": {
      "type": "pidff",
      "quirks": ["shift_byte", "reverse_force"]
    }
  },
  "inputs": {
    "steering": {
      "type": "axis",
      "byte_offset": 0,
      "data_type": "u16_le",
      "min": 0,
      "max": 65535
    },
    "throttle": { "byte_offset": 4, "data_type": "u8" },
    "buttons": [
      { "name": "A", "byte_offset": 8, "bit_mask": 0x01 },
      { "name": "B", "byte_offset": 8, "bit_mask": 0x02 }
    ]
  }
}
```

## Implementation Plan

### Phase 1: Mapper CLI (MVP)
- Implement `openracing-mapper` binary.
- Dependencies: `hidapi`, `crossterm` (for UI), `serde_json`.
- Support: Windows & Linux.
- Deliverable: Tool that produces valid JSON for Button/Axis mapping.

### Phase 2: Sniffer Integration
- Integrate `pcap` crate.
- Add admin-check logic.
- Deliverable: Tool can capture Init packets.

### Phase 3: Community Platform
- GitHub Actions workflow to validate submitted JSONs.
- "Device Library" registry in the main `OpenRacing` repo.
