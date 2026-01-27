# Simagic Protocol Documentation

**Status**: Proprietary / Version Dependent

## Device Versions

Simagic protocols changed significantly between firmware versions.

### Legacy (Firmware < 159)
- **Protocol**: Standard USB HID PID.
- **Support**: Works with generic HID drivers (mostly).
- **Quirks**: Missing some effect descriptors, requires patching descriptor to be valid standard HID.

### Modern (Firmware > 171)
- **Protocol**: Proprietary / Obfuscated HID.
- **Behavior**:
    - Report IDs and Effect Types are not advertised in the Descriptor.
    - They are hardcoded in the official driver (`simpro_1.dll`).
    - Data format resembles standard PID but is "shifted" (e.g., Effect ID is at offset+1).
- **Status**: Difficult to support without specific reverse-engineered mapping of the "shifted" protocol.

## Device IDs

| Model | Vendor ID | Product ID |
|-------|-----------|------------|
| Alpha Mini | `0x0483` | `0x0522` |
| Alpha      | `0x0483` | `0x0522` |

*Note: `0x0483` is STMicroelectronics generic VID.*

## Resources

- **Linux Driver (Old FW)**: [https://github.com/JacKeTUs/simagic-ff](https://github.com/JacKeTUs/simagic-ff)
- **Discussion**: [https://github.com/JacKeTUs/linux-steering-wheels/issues/3](https://github.com/JacKeTUs/linux-steering-wheels/issues/3)
