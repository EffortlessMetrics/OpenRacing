Feature: Multi-Vendor HID Device Support
  As a sim racer
  I want my force feedback wheel to be automatically detected and configured
  So that I can plug it in and have it work without manual setup

  Scenario Outline: Device is identified by VID and PID
    Given a HID device with VID <vid> and PID <pid>
    When the device manager scans for devices
    Then the device is identified as vendor "<vendor>"
    And the appropriate protocol handler is loaded

    Examples:
      | vid    | pid    | vendor       |
      | 0x346E | 0x0005 | MOZA         |
      | 0x046D | 0x0002 | Logitech     |
      | 0x0EB7 | 0x0005 | Fanatec      |
      | 0x3416 | 0x0301 | Cammus       |
      | 0x0483 | 0x0001 | Simagic      |
      | 0x1209 | 0xFFB0 | OpenFFBoard  |

  Scenario: Torque command stays within safety limits
    Given a connected force feedback device
    When a torque command of 1.5 (out of range) is sent
    Then the actual torque output is clamped to 1.0
    And no safety fault is triggered

  Scenario: Motor enable follows expected state transitions
    Given a connected force feedback device in disabled state
    When the device is enabled
    Then the motor enable bit is set in the output report
    And the device responds to force feedback commands

  Scenario: Unknown device does not crash the service
    Given a HID device with VID 0xDEAD and PID 0xBEEF
    When the device manager scans for devices
    Then the device is ignored gracefully
    And no error is logged as critical
