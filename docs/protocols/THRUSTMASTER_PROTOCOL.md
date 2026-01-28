# Thrustmaster Protocol Documentation

**Status**: Proprietary Initialization / Standard FFB

## Overview

Thrustmaster wheels (T300RS, T-GT, TS-PC) require a specific initialization sequence to enable the Force Feedback motor. Without this, the wheel is detected as a generic input device with no FFB.

## Initialization

1.  **Mode Switch**: Hardware switch must be in "PS3" position (for PC/Linux compatibility) or "PC" mode depending on firmware.
2.  **Magic Packet**:
    - Host sends a specific USB Control Transfer or Output Report.
    - Often related to setting the "Gain" to non-zero.
    - Example: `0x40` (Set Gain) command.

## Force Feedback

- Uses standard USB HID PID (Usage Page `0x0F`).
- **Effect Types**: Constant, Ramp, Square, Sine, Triangle, Sawtooth.
- **Spring/Damper**: Supported via standard PID blocks.

## Resources

- **hid-tmff2**: [https://github.com/Kimplul/hid-tmff2](https://github.com/Kimplul/hid-tmff2)
- **Note**: This driver is the gold standard for non-Windows Thrustmaster support.
