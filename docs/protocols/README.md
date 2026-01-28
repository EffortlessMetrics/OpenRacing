# Device Protocol Knowledge Base

This document summarizes the known communication protocols for supported and planned racing wheels. It serves as the reference for implementing `crates/engine/src/hw/` drivers.

## 1. Moza Racing
**Status:** Well-Documented
**Type:** Serial over USB / Custom HID

*   **Protocol:**
    *   Uses a serial-like protocol encapsulated in USB HID Feature Reports or direct USB bulk transfers depending on the model.
    *   **Documentation:** The [Boxflat](https://github.com/Lawstorant/boxflat) project contains a detailed `moza-protocol.md`.
    *   **Linux Driver:** `hid-universal-pidff` supports Moza wheels via standard PIDFF after initialization.
*   **Initialization:** Requires specific "handshake" commands to unlock high-torque modes and FFB.
*   **Implementation Strategy:**
    *   Port the command structure from Boxflat.
    *   Implement as `MozaDevice` struct in `engine`.

## 2. Simagic
**Status:** Fragmented (Old vs. New)
**Type:** HID PIDFF (Old) / Proprietary (New)

*   **Legacy (Firmware <= v159):**
    *   Standard USB HID PIDFF.
    *   Works with generic drivers (mostly).
*   **Modern (Firmware >= v171):**
    *   **Protocol:** Proprietary. Shifts standard HID reports by one byte (byte 0 = 0x01).
    *   **Obfuscation:** Removes FFB descriptors from the USB descriptor, making OS drivers fail.
    *   **Research:** [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) is the active hub for reverse engineering.
*   **Implementation Strategy:**
    *   Support Legacy mode immediately (Standard HID).
    *   For Modern mode, we need to sniff the init sequence that `SimPro Manager` sends.

## 3. Fanatec
**Status:** Reverse-Engineered
**Type:** Custom HID

*   **Protocol:**
    *   Fanatec wheels start in "Xbox" or "Compatibility" mode.
    *   **Initialization:** Requires a "Magic Byte" sequence to switch to "PC Mode" (native).
    *   **FFB:** Standard-ish, but often requires specific report IDs.
*   **Resources:**
    *   [hid-fanatecff](https://github.com/gotzl/hid-fanatecff): Full Linux kernel driver source. Contains all device IDs, init sequences, and report structures.
*   **Implementation Strategy:**
    *   Transpile the C logic from `hid-fanatecff` to Rust.

## 4. Logitech (G29/G923)
**Status:** Standard + Proprietary Extensions
**Type:** HID PIDFF + TrueForce

*   **Basic:** Standard HID PIDFF. Documented by Linux `hid-logitech` driver.
*   **TrueForce (High Frequency):**
    *   Audio-based haptics sent over a separate endpoint or specific report.
    *   Proprietary and requires reverse engineering G-Hub if we want to support it (low priority).
*   **Implementation Strategy:**
    *   Use `generic_hid` implementation for basic FFB.

## 5. Thrustmaster (T300/T-GT)
**Status:** Niche
**Type:** USB HID

*   **Protocol:** Standard HID, but notoriously picky about USB initialization.
*   **Resources:** Arduino emulator projects (e.g., `rr-m.org/blog`) document the packet structures.

---

# Capture Strategy

To support these devices, we will build:

1.  **`openracing-mapper`**: A CLI tool for users to map buttons and axes.
2.  **`docs/new_device_guide.md`**: A guide for capturing USB traffic using Wireshark (for the "hard" parts like init sequences).

We will **not** build a custom USB sniffer, as Wireshark + USBPcap is the industry standard and safer for users.
