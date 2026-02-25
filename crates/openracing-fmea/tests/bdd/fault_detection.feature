Feature: Fault Detection
  As an FFB safety system developer
  I want to detect various fault conditions reliably
  So that the system can respond appropriately to protect users

  Background:
    Given an FMEA system with default thresholds

  Scenario: USB stall detected after consecutive failures
    Given the USB communication is working
    When 2 consecutive USB failures occur
    Then no fault should be detected
    When 1 more USB failure occurs
    Then a USB stall fault should be detected
    And the fault should require immediate response

  Scenario: USB stall detected on timeout
    Given the last successful USB communication was 5ms ago
    When the USB timeout threshold is 10ms
    Then no fault should be detected
    When 10ms passes without communication
    Then a USB stall fault should be detected

  Scenario: Encoder NaN fault with window detection
    Given the encoder is producing valid values
    When the encoder produces 4 NaN values within the window
    Then no fault should be detected
    When 1 more NaN value occurs
    Then an encoder NaN fault should be detected
    And the fault should not be automatically recoverable

  Scenario: Thermal fault with hysteresis
    Given the temperature is at 70°C
    When the temperature rises to 85°C
    Then a thermal limit fault should be detected
    When the temperature drops to 78°C
    And the fault is still active
    Then the fault should not clear due to hysteresis
    When the temperature drops to 74°C
    Then the fault should be clearable

  Scenario: Plugin overrun quarantine
    Given a plugin with ID "test_plugin"
    When the plugin exceeds its time budget 9 times
    Then no fault should be detected
    When the plugin exceeds its time budget 1 more time
    Then a plugin overrun fault should be detected
    And the fault action should be quarantine

  Scenario: Timing violation accumulation
    Given the RT loop is running normally
    When 99 timing violations occur
    Then no fault should be detected
    When 1 more timing violation occurs
    Then a timing violation fault should be detected
    And the fault action should be log and continue

  Scenario: Overcurrent immediate detection
    Given the current is within limits
    When the current exceeds the safe threshold
    Then an overcurrent fault should be detected immediately
    And the fault should have severity 1
    And the fault should require immediate response

  Scenario: Fault severity ordering
    Given all fault types
    When comparing fault severities
    Then overcurrent should have the lowest severity number
    And thermal limit should have the lowest severity number
    And timing violation should have a higher severity number

  Scenario: Disabled fault type is ignored
    Given an FMEA system with timing violation disabled
    When a timing violation occurs
    And the fault is handled
    Then no active fault should be recorded

  Scenario Outline: Fault detection thresholds are configurable
    Given custom thresholds with <param> set to <value>
    When <condition>
    Then <expected_result>

    Examples:
      | param                        | value | condition                          | expected_result              |
      | usb_max_consecutive_failures | 1     | 1 USB failure occurs               | USB stall fault detected     |
      | thermal_limit_celsius        | 60.0  | temperature reaches 65°C           | thermal limit fault detected |
      | encoder_max_nan_count        | 2     | 2 NaN values occur                 | encoder NaN fault detected   |

  Scenario: Multiple faults prioritization
    Given an FMEA system with no active faults
    When a timing violation fault is handled
    And then an overcurrent fault is handled
    Then the active fault should be overcurrent
    And the higher severity fault should take priority

  Scenario: Fault statistics are tracked
    Given an FMEA system
    When 3 USB failures are detected
    And 5 timing violations are detected
    Then the USB failure count should be 3
    And the timing violation count should be 5

  Scenario: Detection state reset on fault clear
    Given an active USB stall fault
    When the fault is cleared
    Then the USB detection count should be 0
    And the last occurrence should be None
