Feature: Watchdog Monitoring
  As a system operator
  I want comprehensive monitoring of plugins and system components
  So that I can ensure reliable operation of the force feedback system

  Background:
    Given a watchdog system with default configuration
    And the watchdog system is initialized

  Scenario: Monitor plugin execution timing
    Given a plugin "test_plugin" is registered with the watchdog
    When the plugin executes with timing 50 microseconds
    Then the execution is recorded successfully
    And the plugin statistics show 1 total execution
    And the plugin is not marked as timed out

  Scenario: Detect plugin timeout
    Given a plugin "slow_plugin" is registered with the watchdog
    And the plugin timeout threshold is 100 microseconds
    When the plugin executes with timing 150 microseconds
    Then the execution is recorded as a timeout
    And the plugin statistics show 1 timeout count
    And the plugin consecutive timeout count is 1

  Scenario: Reset consecutive timeouts on success
    Given a plugin "recovering_plugin" is registered with the watchdog
    And the plugin has 3 consecutive timeouts
    When the plugin executes successfully within timeout
    Then the consecutive timeout count is reset to 0
    And the total timeout count remains 3

  Scenario: Track component heartbeats
    Given the RT thread component
    When a heartbeat is received from the component
    Then the component status is Healthy
    And the last heartbeat timestamp is updated

  Scenario: Detect component degradation
    Given the HID communication component
    And the component has a healthy status
    When 3 consecutive failures are reported
    Then the component status changes to Degraded
    And the component is not yet faulted

  Scenario: Detect component fault
    Given the telemetry adapter component
    And the component has a degraded status
    When 5 total consecutive failures are reported
    Then the component status changes to Faulted
    And the system indicates faulted components exist

  Scenario: Restore component health
    Given the plugin host component is faulted
    When a heartbeat is received from the component
    Then the component status changes to Healthy
    And the consecutive failure count is reset to 0

  Scenario: Get system health summary
    Given the RT thread component is healthy
    And the HID communication component is degraded
    And the telemetry adapter component is healthy
    When the health summary is requested
    Then the summary shows RT thread as Healthy
    And the summary shows HID communication as Degraded
    And the summary shows telemetry adapter as Healthy

  Scenario: Performance metrics collection
    Given plugin "plugin_a" has executed 100 times with average time 50 microseconds
    And plugin "plugin_a" has 5 timeouts
    When performance metrics are requested
    Then the metrics show total_executions as 100
    And the metrics show average_execution_time_us as 50
    And the metrics show timeout_rate_percent as 5.0

  Scenario: Register multiple plugins
    Given 10 plugins are registered with the watchdog
    When each plugin executes once
    Then the watchdog tracks 10 plugins
    And each plugin has independent statistics

  Scenario: Concurrent heartbeat processing
    Given the watchdog system is thread-safe
    When heartbeats are sent concurrently from multiple threads
    Then all heartbeats are processed correctly
    And the component status reflects the latest heartbeat

  Scenario: Configuration affects timeout detection
    Given a custom configuration with timeout threshold 200 microseconds
    When a plugin executes with timing 150 microseconds
    Then the execution is not marked as a timeout
    And the plugin statistics show 0 timeout count