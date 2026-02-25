Feature: Recovery Procedures
  As an FFB safety system developer
  I want to recover from faults gracefully
  So that the system can resume normal operation safely

  Background:
    Given an FMEA system with default configuration

  Scenario: Automatic recovery for USB stall
    Given a USB stall fault is active
    And the fault has completed soft-stop
    Then the fault should be recoverable
    And the recovery procedure should be automatic
    And the recovery should have a reconnect step

  Scenario: Manual recovery required for encoder fault
    Given an encoder NaN fault is active
    And the fault has completed soft-stop
    Then the fault should not be recoverable automatically
    And the recovery procedure should require manual intervention
    And the recovery should have a calibration step

  Scenario: Thermal recovery with cooldown
    Given a thermal limit fault is active
    Then the recovery procedure should be automatic
    And the recovery should have a cooldown step
    When the cooldown period completes
    Then the fault can be cleared

  Scenario: Soft-stop ramps torque smoothly
    Given an active fault with 10Nm current torque
    When soft-stop starts
    Then the torque should begin decreasing
    And the torque should reach 0Nm within 50ms
    And the ramp should be linear

  Scenario: Soft-stop can be forced immediately
    Given an active soft-stop in progress
    When force stop is called
    Then the torque should immediately be 0
    And soft-stop should no longer be active

  Scenario: Recovery procedure steps
    Given a recovery procedure for USB stall
    Then the procedure should have multiple steps
    And each step should have a timeout
    And the steps should be executed in order

  Scenario: Recovery retry logic
    Given a recovery context for USB stall
    When the first recovery attempt fails
    Then a retry should be available
    And up to 3 attempts should be allowed
    When all retries are exhausted
    Then no more retries should be available

  Scenario: Recovery timeout
    Given a recovery procedure with 5 second timeout
    When recovery starts
    And 5 seconds pass without completion
    Then the recovery should timeout
    And the status should be RecoveryStatus::Timeout

  Scenario: Recovery cancellation
    Given an ongoing recovery procedure
    When cancellation is requested
    Then the recovery should be cancelled
    And the status should be RecoveryStatus::Cancelled

  Scenario: Recovery for non-recoverable fault
    Given an encoder NaN fault is active
    Then can_recover should return false
    And the system should require manual intervention

  Scenario: Plugin quarantine recovery
    Given a plugin overrun fault is active
    Then the recovery procedure should have a quarantine step
    And the recovery procedure should have a release step

  Scenario: Safety interlock recovery
    Given a safety interlock violation fault is active
    Then the recovery should not be automatic
    And the recovery should require a new challenge
    And the recovery should verify physical presence

  Scenario: Recovery step optional flag
    Given a recovery procedure with optional steps
    When an optional step fails
    Then recovery can continue to the next step
    When a required step fails
    Then recovery should not continue

  Scenario: Multiple recovery attempts tracking
    Given a recovery context with 3 max attempts
    When 2 attempts have been made
    Then the attempt count should be 2
    And 1 more attempt should be available

  Scenario: Recovery result tracking
    When a recovery succeeds after 2 attempts
    Then the result status should be Completed
    And the attempt count should be 2
    And no error should be recorded

  Scenario: Recovery failure result
    When a recovery fails with error "Connection timeout"
    Then the result status should be Failed
    And the error message should be recorded

  Scenario: Soft-stop progress tracking
    Given an active soft-stop from 10Nm
    When 25ms has elapsed
    Then the progress should be approximately 50%
    And the current torque should be approximately 5Nm
    And the remaining time should be approximately 25ms

  Scenario Outline: All fault types have recovery procedures
    Given a <fault_type> fault
    Then a recovery procedure should be defined
    And the procedure should have appropriate steps

    Examples:
      | fault_type                   |
      | USB stall                    |
      | encoder NaN                  |
      | thermal limit                |
      | overcurrent                  |
      | plugin overrun               |
      | timing violation             |
      | safety interlock violation   |
      | hands-off timeout            |
      | pipeline fault               |
