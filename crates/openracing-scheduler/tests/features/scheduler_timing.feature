Feature: Scheduler Timing Accuracy
  As a real-time system developer
  I want precise 1kHz timing with drift correction
  So that force feedback remains stable and responsive

  Background:
    Given a scheduler with 1kHz period
    And RT setup is applied

  Scenario: Scheduler maintains accurate timing over multiple ticks
    Given the scheduler is initialized
    When 100 ticks are processed
    Then the total elapsed time should be approximately 100ms
    And the p99 jitter should be less than 0.25ms
    And no deadlines should be missed

  Scenario: PLL corrects for systematic drift
    Given a PLL with target period 1ms
    When the actual interval consistently drifts by +5%
    Then the PLL estimated period should compensate
    And the phase error should stabilize

  Scenario: Jitter metrics accurately track timing variance
    Given jitter metrics with 1000 samples
    When samples have jitter values from 0 to 100us
    Then the p99 jitter should be approximately 99us
    And the max jitter should be 100us
    And the missed tick rate should be 0

  Scenario: Timing violation is detected and reported
    Given a scheduler with strict timing requirements
    When jitter exceeds 5ms in test mode
    Then a TimingViolation error should be returned
    And the tick count should still be incremented

  Scenario Outline: Adaptive scheduling responds to load
    Given adaptive scheduling is enabled
    When processing time is <processing_time>us
    And jitter is <jitter>ns
    Then the target period should <action>

    Examples:
      | processing_time | jitter    | action           |
      | 200             | 250000    | increase         |
      | 50              | 30000     | decrease         |
      | 100             | 100000    | stay same        |

  Scenario: Scheduler reset clears all state
    Given a scheduler with 1000 ticks processed
    And some timing violations recorded
    When the scheduler is reset
    Then tick count should be 0
    And jitter metrics should be cleared
    And PLL should be reset to initial state
