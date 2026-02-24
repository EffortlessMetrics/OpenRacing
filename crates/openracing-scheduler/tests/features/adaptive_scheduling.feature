Feature: Adaptive Scheduling Behavior
  As a real-time system operator
  I want the scheduler to adapt to system load
  So that timing violations are minimized under varying conditions

  Background:
    Given a scheduler with adaptive scheduling enabled
    And period bounds of 0.9ms to 1.1ms
    And increase step of 5us
    And decrease step of 2us

  Scenario: Period increases under high jitter
    Given adaptive scheduling is enabled
    And current target period is 1.0ms
    When jitter exceeds relax threshold of 200us
    Then target period should increase by 5us
    And target period should not exceed 1.1ms

  Scenario: Period increases under high processing time
    Given adaptive scheduling is enabled
    And processing time EMA exceeds relax threshold of 180us
    When a tick is processed
    Then target period should increase
    And PLL target should be updated

  Scenario: Period decreases when system is healthy
    Given adaptive scheduling is enabled
    And current target period is 1.05ms
    And jitter is below tighten threshold of 50us
    And processing time EMA is below tighten threshold of 80us
    When a tick is processed
    Then target period should decrease by 2us
    And target period should not go below 0.9ms

  Scenario: Period stays same under moderate load
    Given adaptive scheduling is enabled
    And current target period is 1.0ms
    When jitter is between tighten and relax thresholds
    And processing time is moderate
    Then target period should remain unchanged

  Scenario: Adaptive scheduling respects bounds
    Given adaptive scheduling is enabled
    And target period is at maximum 1.1ms
    When jitter continues to exceed threshold
    Then target period should stay at 1.1ms
    And should not exceed maximum

  Scenario: Disabled adaptive scheduling uses fixed period
    Given adaptive scheduling is disabled
    When ticks are processed under any load
    Then target period should always be 1.0ms
    And PLL should track the fixed period

  Scenario: Processing time EMA updates correctly
    Given adaptive scheduling with EMA alpha of 0.2
    When processing times are [100, 200, 300]us
    Then EMA should converge toward recent values
    And last processing time should be 300us

  Scenario: Adaptive state snapshot is accurate
    Given adaptive scheduling is running
    When state is queried
    Then enabled flag should reflect configuration
    And target period should be within bounds
    And processing time EMA should be current

  Scenario Outline: Configuration normalization
    Given an adaptive config with min <min> and max <max>
    When the config is normalized
    Then min should be <expected_min>
    And max should be <expected_max>

    Examples:
      | min       | max       | expected_min | expected_max |
      | 1100000   | 900000    | 900000       | 1100000      |
      | 0         | 1000000   | 1            | 1000000      |
      | 500000    | 500000    | 500000       | 500000       |

  Scenario: Missed deadline always triggers period increase
    Given adaptive scheduling is enabled
    And jitter is very low
    When a deadline is missed
    Then target period should still increase
    And system should attempt recovery
