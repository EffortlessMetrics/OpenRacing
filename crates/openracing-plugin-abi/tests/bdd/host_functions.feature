Feature: Host Functions
  As a WASM plugin developer
  I want to use host functions correctly
  So that my plugin can interact with the runtime

  Background:
    Given the plugin ABI crate is available
    And the host module name is "env"

  Scenario: Host function names are correct
    Then log_debug function name should be "log_debug"
    And log_info function name should be "log_info"
    And log_warn function name should be "log_warn"
    And log_error function name should be "log_error"
    And plugin_log function name should be "plugin_log"
    And check_capability function name should be "check_capability"
    And get_telemetry function name should be "get_telemetry"
    And get_timestamp_us function name should be "get_timestamp_us"

  Scenario: Log level constants are ordered
    Then ERROR level should be 0
    And WARN level should be 1
    And INFO level should be 2
    And DEBUG level should be 3
    And TRACE level should be 4
    And ERROR should be less than WARN
    And WARN should be less than INFO
    And INFO should be less than DEBUG
    And DEBUG should be less than TRACE

  Scenario: Return code constants are correct
    Then SUCCESS return code should be 0
    And ERROR return code should be -1
    And INVALID_ARG return code should be -2
    And PERMISSION_DENIED return code should be -3
    And BUFFER_TOO_SMALL return code should be -4
    And NOT_INITIALIZED return code should be -5
    And all error codes should be negative

  Scenario: Capability strings are correct
    Then READ_TELEMETRY should be "read_telemetry"
    And MODIFY_TELEMETRY should be "modify_telemetry"
    And CONTROL_LEDS should be "control_leds"
    And PROCESS_DSP should be "process_dsp"

  Scenario: Required WASM export names
    Then process export name should be "process"
    And memory export name should be "memory"

  Scenario: Optional WASM export names
    Then init export name should be "init"
    And shutdown export name should be "shutdown"
    And get_info export name should be "get_info"

  Scenario: String parameter validation
    Given host function parameter validation
    When I validate string params with ptr=-1 and len=10
    Then the result should be INVALID_ARG
    When I validate string params with ptr=0 and len=-1
    Then the result should be INVALID_ARG
    When I validate string params with ptr=0 and len=10000 and max_len=100
    Then the result should be BUFFER_TOO_SMALL
    When I validate string params with ptr=0 and len=10 and max_len=100
    Then the result should be SUCCESS

  Scenario: Output buffer validation
    Given host function parameter validation
    When I validate output buffer with ptr=-1 and len=32 and required=32
    Then the result should be INVALID_ARG
    When I validate output buffer with ptr=0 and len=-1 and required=32
    Then the result should be INVALID_ARG
    When I validate output buffer with ptr=0 and len=16 and required=32
    Then the result should be BUFFER_TOO_SMALL
    When I validate output buffer with ptr=0 and len=32 and required=32
    Then the result should be SUCCESS

  Scenario: Telemetry frame size requirement
    Given a telemetry frame
    Then the size should be 32 bytes
    And the alignment should be 8 bytes
    And get_telemetry requires at least 32 bytes

  Scenario: Plugin header size requirement
    Given a plugin header
    Then the size should be 16 bytes
    And the alignment should be 4 bytes

  Scenario: Log level severity ordering
    Given log levels ERROR, WARN, INFO, DEBUG, TRACE
    Then each level should be more verbose than the previous
    And ERROR should indicate critical issues
    And TRACE should indicate very detailed information

  Scenario: Return code interpretation
    Given return codes
    Then SUCCESS should indicate operation completed
    And ERROR should indicate generic failure
    And INVALID_ARG should indicate bad parameters
    And PERMISSION_DENIED should indicate capability not granted
    And BUFFER_TOO_SMALL should indicate output too small
    And NOT_INITIALIZED should indicate plugin not ready

  Scenario: Host function parameter constraints
    Given the log function signature
    Then msg_ptr must be a valid pointer
    And msg_len must be the exact string length
    And the string must be valid UTF-8

  Scenario: Plugin log function level constraint
    Given the plugin_log function signature
    Then level must be 0-4
    And level 0 should be ERROR
    And level 4 should be TRACE

  Scenario: Check capability return values
    Given the check_capability function
    Then it should return 1 if capability is granted
    And it should return 0 if capability is not granted
    And it should return negative on error

  Scenario: Get telemetry buffer requirements
    Given the get_telemetry function
    Then out_len must be at least 32 bytes
    And it should return SUCCESS on success
    And it should return BUFFER_TOO_SMALL if buffer too small

  Scenario: Get timestamp return value
    Given the get_timestamp_us function
    Then it should return microseconds since plugin start
    And the value should be monotonic increasing