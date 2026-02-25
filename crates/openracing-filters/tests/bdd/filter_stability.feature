Feature: Filter Stability
  As an FFB system developer
  I want all filters to be stable and well-behaved
  So that the system operates safely under all conditions

  Background:
    Given the filters crate is available

  Scenario: Reconstruction filter converges to input
    Given a reconstruction filter with level 4
    When I apply a constant input of 1.0 for 200 iterations
    Then the output should converge to within 0.01 of the input

  Scenario: Reconstruction filter preserves sign
    Given a reconstruction filter with level 4
    When I apply input 0.5
    Then the output should be positive
    When I apply input -0.5
    Then the output should be negative

  Scenario: Notch filter remains stable with DC input
    Given a notch filter at 50Hz with Q 2.0
    When I apply DC input 1.0 for 100 iterations
    Then the output should remain finite
    And the output should not exceed magnitude 10.0

  Scenario: Notch filter handles extreme input
    Given a notch filter at 50Hz with Q 2.0
    When I apply input 100.0
    Then the output should be finite

  Scenario: Slew rate limiter approaches target
    Given a slew rate limiter with rate 0.5
    When I apply constant input 1.0 for 1000 iterations
    Then the output should be at least 0.4

  Scenario: Slew rate limiter never exceeds rate limit
    Given a slew rate limiter with rate 0.5
    When I apply step input from 0.0 to 1.0
    Then the output change per tick should not exceed 0.0005

  Scenario: Curve filter preserves endpoints
    Given a linear curve
    When I apply input 0.0
    Then the output should be approximately 0.0
    When I apply input 1.0
    Then the output should be approximately 1.0

  Scenario: Curve filter handles extreme inputs
    Given a quadratic curve
    When I apply input -100.0
    Then the output should be approximately 0.0
    When I apply input 100.0
    Then the output should be approximately 1.0

  Scenario: All filters produce finite output for finite input
    Given all filter types
    When I apply input 0.5 with wheel speed 1.0
    Then all outputs should be finite

  Scenario: Filters are deterministic
    Given two identical filter instances
    When I apply the same input sequence
    Then the outputs should be identical

  Scenario Outline: Filter stability with sinusoidal input
    Given a <filter_type> filter
    When I apply sinusoidal input at <frequency>Hz for 1000 iterations
    Then the output should remain bounded within [-10.0, 10.0]
    And the output should remain finite

    Examples:
      | filter_type    | frequency |
      | reconstruction | 10        |
      | reconstruction | 50        |
      | reconstruction | 100       |
      | notch          | 10        |
      | notch          | 50        |
      | notch          | 100       |
      | slew_rate      | 10        |
      | slew_rate      | 50        |

  Scenario: Bumpstop only activates beyond start angle
    Given a bumpstop with start angle 450 degrees
    When the current angle is 400 degrees
    Then the bumpstop should produce no torque
    When the current angle is 500 degrees
    Then the bumpstop should produce opposing torque

  Scenario: Hands-off detector respects timeout
    Given a hands-off detector with 0.5 second timeout
    When I apply low torque below threshold for 400 ticks
    Then hands-off should not be detected
    When I apply low torque below threshold for 600 ticks
    Then hands-off should be detected

  Scenario: Hands-off detector resets on resistance
    Given a hands-off detector with 0.5 second timeout
    When I apply low torque for 400 ticks
    And then apply high torque above threshold
    Then the counter should reset to zero
    And hands-off should not be detected