# ADR-0003: OWP-1 (Open Wheel Protocol) Specification

**Status:** Accepted  
**Date:** 2024-01-15  
**Authors:** Hardware Team, Architecture Team  
**Reviewers:** Engineering Team, Safety Team  
**Related ADRs:** ADR-0001 (FFB Modes), ADR-0005 (Safety Interlocks)

## Context

Modern racing wheel bases need a standardized protocol for high-frequency force feedback communication that goes beyond legacy HID PID limitations. The protocol must support:

1. 1kHz torque commands with precise timing
2. Real-time telemetry and health monitoring
3. Capability negotiation and configuration
4. Safety interlocks and fault reporting
5. Cross-platform compatibility via standard HID

## Decision

Define OWP-1 (Open Wheel Protocol v1) as a HID-based protocol with the following reports:

**HID OUT Report 0x20 - Torque Command (1kHz)**:
```c
struct torque_command {
    uint8_t  report_id;     // 0x20
    int16_t  torque_mN_m;   // Q8.8 fixed-point, full-scale from caps
    uint8_t  flags;         // bit0: hands_on_hint, bit1: sat_warn
    uint16_t seq;           // sequence number, wraps
    uint8_t  crc8;          // CRC-8 of payload
};
```

**HID IN Report 0x21 - Telemetry (60-200Hz)**:
```c
struct device_telemetry {
    uint8_t  report_id;     // 0x21
    int32_t  wheel_angle_mdeg;  // millidegrees
    int16_t  wheel_speed_mrad_s; // milliradians/second
    uint8_t  temp_c;        // temperature in Celsius
    uint8_t  faults;        // fault bitfield
    uint8_t  hands_on;      // 0/1 detection if supported
    uint16_t seq;           // sequence number
    uint8_t  crc8;          // CRC-8 of payload
};
```

**Feature Report 0x01 - Device Capabilities**:
```c
struct device_caps {
    uint8_t  report_id;     // 0x01
    uint8_t  supports_pid : 1;
    uint8_t  supports_raw_torque_1khz : 1;
    uint8_t  supports_health_stream : 1;
    uint8_t  supports_led_bus : 1;
    uint8_t  reserved : 4;
    uint16_t max_torque_cNcm;   // centinewton-meters
    uint16_t encoder_cpr;       // counts per revolution
    uint8_t  min_report_period_us; // minimum report period
    uint8_t  protocol_version;  // OWP version (0x01)
};
```

**Feature Report 0x02 - Configuration**:
```c
struct device_config {
    uint8_t  report_id;     // 0x02
    uint16_t dor_degrees;   // degrees of rotation
    uint8_t  torque_cap_percent; // percentage of max torque
    uint8_t  bumpstop_model;     // 0=off, 1=linear, 2=progressive
    // Additional configuration fields...
};
```

## Rationale

- **HID Compliance**: Works with standard OS HID drivers, no kernel modules required
- **Real-time**: 1kHz torque updates with sequence numbers for drop detection
- **Safety**: CRC validation and fault reporting built into protocol
- **Extensible**: Feature reports allow capability discovery and configuration
- **Efficient**: Fixed-size reports with packed binary data for minimal overhead

## Consequences

### Positive
- Standard HID transport works across all platforms
- High-frequency updates with built-in integrity checking
- Clear capability negotiation prevents incompatible operations
- Fault reporting enables proactive safety measures
- Sequence numbers allow detection of dropped commands

### Negative
- Fixed report sizes limit future extensibility
- CRC overhead adds computational cost
- HID report size limitations constrain data payload
- Requires device firmware implementing full protocol

### Neutral
- Protocol versioning requires careful backward compatibility management
- Endianness must be consistent (little-endian chosen)

## Alternatives Considered

1. **USB Bulk Transfer**: Rejected due to complexity and driver requirements
2. **Serial over USB**: Rejected due to lack of standardization and OS integration
3. **Existing HID PID**: Rejected due to frequency limitations and complexity
4. **Custom USB Class**: Rejected due to driver installation requirements

## Implementation Notes

- All multi-byte fields use little-endian byte order
- CRC-8 uses polynomial 0x07 (x^8 + x^2 + x + 1)
- Sequence numbers wrap at 65535, gaps indicate dropped reports
- Fault bits defined: USB=0x01, Encoder=0x02, Thermal=0x04, Overcurrent=0x08
- Configuration changes acknowledged via Report 0x22 (ConfigAck)

## Compliance & Verification

- Protocol compliance tests with reference implementation
- Interoperability tests between different device manufacturers
- Timing validation ensures 1kHz capability
- Fault injection tests verify error handling
- Cross-platform HID compatibility validation

## References

- Requirements: DM-01, DM-02, FFB-01, SAFE-03, XPLAT-01
- Design Document: Device Management System
- HID Specification: https://www.usb.org/hid
- Safety Requirements: ADR-0005