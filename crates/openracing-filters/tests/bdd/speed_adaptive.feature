Feature: Speed-Adaptive Filters
  As an FFB system developer
  I want speed-adaptive filters that adjust their behavior based on wheel speed
  So that the force feedback feels natural across different driving conditions

  Background:
    Given the filters crate is available

  Scenario: Friction filter reduces effect at high speed when adaptive
    Given a friction filter with coefficient 0.1 and speed adaptation enabled
    When I apply wheel speed 1.0 rad/s
    Then the friction torque should be approximately 0.1
    When I apply wheel speed 10.0 rad/s
    Then the friction torque should be less than at 1.0 rad/s

  Scenario: Friction filter has constant effect when non-adaptive
    Given a friction filter with coefficient 0.1 and speed adaptation disabled
    When I apply wheel speed 1.0 rad/s
    Then the friction torque should be approximately 0.1
    When I apply wheel speed 10.0 rad/s
    Then the friction torque should be approximately the same as at 1.0 rad/s

  Scenario: Damper filter increases effect at high speed when adaptive
    Given a damper filter with coefficient 0.1 and speed adaptation enabled
    When I apply wheel speed 1.0 rad/s
    Then the damping torque should be 0.1
    When I apply wheel speed 10.0 rad/s
    Then the damping torque should be greater than at 1.0 rad/s

  Scenario: Damper filter has proportional effect when non-adaptive
    Given a damper filter with coefficient 0.1 and speed adaptation disabled
    When I apply wheel speed 1.0 rad/s
    Then the damping torque should be approximately 0.1
    When I apply wheel speed 10.0 rad/s
    Then the damping torque should be approximately 1.0

  Scenario Outline: Friction speed adaptation factor
    Given a friction filter with coefficient 0.1 and speed adaptation enabled
    When I apply wheel speed <speed> rad/s
    Then the friction coefficient should be reduced by factor <factor> or more

    Examples:
      | speed | factor |
      | 1.0   | 0.0    |
      | 5.0   | 0.4    |
      | 8.0   | 0.64   |
      | 10.0  | 0.8    |

  Scenario Outline: Damper speed adaptation factor
    Given a damper filter with coefficient 0.1 and speed adaptation enabled
    When I apply wheel speed <speed> rad/s
    Then the damping coefficient should be increased by factor <factor> or more

    Examples:
      | speed | factor |
      | 1.0   | 0.0    |
      | 5.0   | 0.5    |
      | 10.0  | 1.0    |

  Scenario: Friction filter produces no effect at zero speed
    Given a friction filter with coefficient 0.1
    When I apply wheel speed 0.0 rad/s
    Then the torque output should not change

  Scenario: Damper filter produces no effect at zero speed
    Given a damper filter with coefficient 0.1
    When I apply wheel speed 0.0 rad/s
    Then the torque output should not change

  Scenario: Friction filter opposes motion direction
    Given a friction filter with coefficient 0.1
    When I apply positive wheel speed
    Then the friction torque should be negative
    When I apply negative wheel speed
    Then the friction torque should be positive

  Scenario: Damper filter opposes motion direction
    Given a damper filter with coefficient 0.1
    When I apply positive wheel speed
    Then the damping torque should be negative
    When I apply negative wheel speed
    Then the damping torque should be positive

  Scenario: Speed-adaptive filters are bounded
    Given a friction filter with speed adaptation enabled
    When I apply very high wheel speed 100.0 rad/s
    Then the friction coefficient should not be negative
    And the friction torque should be finite

  Scenario: Inertia filter responds to acceleration not speed
    Given an inertia filter with coefficient 0.1
    When wheel speed is constant at 5.0 rad/s
    Then the inertia torque should be zero
    When wheel speed changes from 0.0 to 5.0 rad/s
    Then the inertia torque should oppose the acceleration

  Scenario: Inertia filter scales with acceleration magnitude
    Given an inertia filter with coefficient 0.1
    When wheel speed changes by 1.0 rad/s
    Then the inertia torque should be 0.1 Nm
    When wheel speed changes by 10.0 rad/s
    Then the inertia torque should be 1.0 Nm

  Scenario: Combined speed-adaptive filters work together
    Given a friction filter with coefficient 0.1 and speed adaptation enabled
    And a damper filter with coefficient 0.1 and speed adaptation enabled
    When I apply wheel speed 5.0 rad/s with initial torque 0.0
    Then the total torque should be negative
    And the total torque should be the sum of friction and damping