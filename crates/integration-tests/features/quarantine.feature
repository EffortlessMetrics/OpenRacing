Feature: Plugin Quarantine Management
  As a system operator
  I want misbehaving plugins to be automatically quarantined
  So that system stability is maintained

  Background:
    Given a watchdog system with default configuration
    And the quarantine policy is enabled
    And the quarantine duration is 300 seconds

  Scenario: Quarantine plugin after consecutive timeouts
    Given plugin "problematic_plugin" is registered
    And the maximum consecutive timeouts is 5
    When the plugin times out 5 consecutive times
    Then the plugin is quarantined
    And a PluginOverrun fault is returned on the 5th timeout
    And the quarantine count is incremented to 1

  Scenario: Quarantine reason is tracked
    Given plugin "crashing_plugin" is registered
    When the plugin is quarantined for consecutive timeouts
    Then the quarantine reason is recorded as "ConsecutiveTimeouts"
    And the quarantine timestamp is recorded

  Scenario: Quarantined plugin is excluded from execution
    Given plugin "quarantined_plugin" is quarantined
    When the execution status is checked
    Then the plugin is marked as quarantined
    And the quarantine remaining time is available

  Scenario: Quarantine expires automatically
    Given plugin "timed_quarantine_plugin" is quarantined for 10 milliseconds
    When 15 milliseconds have passed
    And health checks are performed
    Then the plugin is no longer quarantined
    And the plugin can execute again

  Scenario: Manual quarantine release
    Given plugin "locked_plugin" is quarantined
    When the quarantine is manually released
    Then the plugin is no longer quarantined
    And the plugin statistics are reset for consecutive timeouts

  Scenario: Quarantine policy disabled
    Given the quarantine policy is disabled
    When plugin "bad_plugin" times out 10 consecutive times
    Then the plugin is not quarantined
    And the timeout statistics are still recorded

  Scenario: Quarantine policy re-enabled
    Given the quarantine policy was disabled
    And plugin "test_plugin" has accumulated timeouts
    When the quarantine policy is re-enabled
    Then future timeouts can trigger quarantine
    And existing statistics are preserved

  Scenario: Multiple plugins quarantined independently
    Given plugins "plugin_a" and "plugin_b" are registered
    When "plugin_a" times out 5 times
    And "plugin_b" executes successfully
    Then only "plugin_a" is quarantined
    And "plugin_b" remains active

  Scenario: Get all quarantined plugins
    Given plugins "slow_1", "slow_2", "slow_3" are quarantined
    When the list of quarantined plugins is requested
    Then 3 plugins are returned
    And each plugin has its remaining quarantine duration

  Scenario: Quarantine with custom duration
    Given the quarantine duration is set to 600 seconds
    When plugin "custom_duration_plugin" is quarantined
    Then the quarantine expires after 600 seconds
    And the quarantine entry reflects the custom duration

  Scenario: Quarantine count increments on repeated offenses
    Given plugin "repeat_offender" is quarantined and released
    When the plugin is quarantined again
    Then the quarantine count is 2
    And the history shows repeated quarantines

  Scenario: Fault callback invoked on quarantine
    Given a fault callback is registered
    When plugin "callback_test_plugin" is quarantined
    Then the fault callback is invoked with PluginOverrun fault
    And the callback receives the plugin identifier

  Scenario: Clear all quarantines
    Given 5 plugins are quarantined
    When all quarantines are cleared
    Then no plugins are quarantined
    And the quarantine manager is empty

  Scenario: Quarantine does not affect other plugins
    Given 10 plugins are registered and executing normally
    When plugin "isolation_test" is quarantined
    Then the other 9 plugins remain active
    And their statistics are unaffected

  Scenario: Quarantine after reset statistics
    Given plugin "reset_test_plugin" has been quarantined
    And the plugin statistics are reset
    When the plugin times out 5 consecutive times again
    Then the plugin is quarantined again
    And the quarantine count shows 2 total quarantines

  Scenario Outline: Quarantine threshold configuration
    Given the maximum consecutive timeouts is <max_timeouts>
    When plugin "threshold_test" times out <timeouts> consecutive times
    Then the quarantine status is <quarantined>

    Examples:
      | max_timeouts | timeouts | quarantined |
      | 3            | 2        | false       |
      | 3            | 3        | true        |
      | 5            | 4        | false       |
      | 5            | 5        | true        |
      | 10           | 9        | false       |
      | 10           | 10       | true        |

  Scenario: Component failure triggers fault callback
    Given a fault callback is registered
    When the RT thread component fails 5 consecutive times
    Then the fault callback is invoked with TimingViolation fault
    And the component status is Faulted

  Scenario: Component failure maps to correct fault type
    Given a fault callback is registered
    When component <component> fails 5 consecutive times
    Then the fault callback is invoked with <fault_type> fault

    Examples:
      | component           | fault_type                |
      | RT Thread           | TimingViolation           |
      | HID Communication   | UsbStall                  |
      | Telemetry Adapter   | TimingViolation           |
      | Plugin Host         | PluginOverrun             |
      | Safety System       | SafetyInterlockViolation  |
      | Device Manager      | UsbStall                  |