Feature: ABI Compatibility
  As a plugin developer
  I want to verify ABI version compatibility
  So that plugins work reliably across versions

  Background:
    Given the plugin ABI crate is available
    And the ABI version is 1.0

  Scenario: Valid plugin header with matching version
    Given a plugin header with default values
    When I validate the header
    Then the header should be valid
    And the magic number should match PLUG_ABI_MAGIC
    And the version should match PLUG_ABI_VERSION

  Scenario: Invalid plugin header with wrong magic
    Given a plugin header with magic 0xDEADBEEF
    When I validate the header
    Then the header should be invalid
    And the validation should fail due to magic mismatch

  Scenario: Invalid plugin header with wrong version
    Given a plugin header with version 0x0002_0000
    When I validate the header
    Then the header should be invalid
    And the validation should fail due to version mismatch

  Scenario: Plugin header byte serialization roundtrip
    Given a plugin header with TELEMETRY and LEDS capabilities
    When I serialize the header to bytes
    And I deserialize the bytes back to a header
    Then the deserialized header should equal the original

  Scenario: Telemetry frame byte serialization roundtrip
    Given a telemetry frame with timestamp 1234567890
    And wheel angle 90.0 degrees
    And wheel speed 1.57 rad/s
    And temperature 45.0 celsius
    And fault flags 0xFF
    When I serialize the frame to bytes
    And I deserialize the bytes back to a frame
    Then the deserialized frame should equal the original

  Scenario: Capability flags are correctly truncated
    Given capability bits 0xFFFFFFFF
    When I create PluginCapabilities from bits
    Then only valid capability bits should be set
    And reserved bits should be stripped

  Scenario Outline: Valid capability combinations
    Given I combine capabilities <caps>
    When I create a plugin header with those capabilities
    Then the header should be valid
    And the header should have exactly those capabilities

    Examples:
      | caps                    |
      | TELEMETRY               |
      | LEDS                    |
      | HAPTICS                 |
      | TELEMETRY | LEDS        |
      | TELEMETRY | HAPTICS     |
      | LEDS | HAPTICS          |
      | TELEMETRY | LEDS | HAPTICS |

  Scenario: Telemetry frame default values
    Given a default telemetry frame
    Then the timestamp should be 0
    And the wheel angle should be 0.0
    And the wheel speed should be 0.0
    And the temperature should be 20.0
    And the fault flags should be 0

  Scenario: Temperature validation
    Given a telemetry frame with temperature <temp>
    Then is_temperature_normal should return <is_normal>

    Examples:
      | temp  | is_normal |
      | 19.9  | false     |
      | 20.0  | true      |
      | 45.0  | true      |
      | 80.0  | true      |
      | 80.1  | false     |

  Scenario: Wheel angle validation
    Given a telemetry frame with wheel angle <angle>
    Then is_angle_valid should return <is_valid>

    Examples:
      | angle    | is_valid |
      | -1801.0  | false    |
      | -1800.0  | true     |
      | 0.0      | true     |
      | 900.0    | true     |
      | 1800.0   | true     |
      | 1801.0   | false    |

  Scenario: Fault detection
    Given a telemetry frame with fault flags <flags>
    Then has_faults should return <has_faults>

    Examples:
      | flags | has_faults |
      | 0     | false      |
      | 1     | true       |
      | 255   | true       |
      | 256   | true       |

  Scenario: ABI constants have correct values
    Then PLUG_ABI_VERSION should be 0x0001_0000
    And PLUG_ABI_MAGIC should be 0x57574C31
    And WASM_ABI_VERSION should be 1
    And HOST_MODULE should be "env"

  Scenario: WASM export validation passes with required exports
    Given a WASM export validation with process=true and memory=true
    When I check if the exports are valid
    Then the validation should pass
    And missing_required should be empty

  Scenario: WASM export validation fails without process
    Given a WASM export validation with process=false and memory=true
    When I check if the exports are valid
    Then the validation should fail
    And missing_required should contain "process"

  Scenario: WASM export validation fails without memory
    Given a WASM export validation with process=true and memory=false
    When I check if the exports are valid
    Then the validation should fail
    And missing_required should contain "memory"

  Scenario: WASM export validation fails without both
    Given a WASM export validation with process=false and memory=false
    When I check if the exports are valid
    Then the validation should fail
    And missing_required should contain "process"
    And missing_required should contain "memory"

  Scenario: Plugin init status defaults to Uninitialized
    Given a default PluginInitStatus
    Then the status should be Uninitialized

  Scenario: Plugin info defaults
    Given a default WasmPluginInfo
    Then the name should be empty
    And the version should be empty
    And the abi_version should be WASM_ABI_VERSION