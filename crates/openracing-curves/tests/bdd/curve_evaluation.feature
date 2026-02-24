Feature: Curve Evaluation
  As an FFB system developer
  I want to evaluate various curve types
  So that I can create custom force feedback response mappings

  Background:
    Given the curves crate is available

  Scenario: Evaluate linear curve
    Given a linear curve
    When I evaluate at input 0.0
    Then the output should be approximately 0.0
    When I evaluate at input 0.5
    Then the output should be approximately 0.5
    When I evaluate at input 1.0
    Then the output should be approximately 1.0

  Scenario: Evaluate exponential curve with exponent 2
    Given an exponential curve with exponent 2.0
    When I evaluate at input 0.0
    Then the output should be approximately 0.0
    When I evaluate at input 0.5
    Then the output should be approximately 0.25
    When I evaluate at input 1.0
    Then the output should be approximately 1.0

  Scenario: Evaluate logarithmic curve with base 10
    Given a logarithmic curve with base 10.0
    When I evaluate at input 0.0
    Then the output should be approximately 0.0
    When I evaluate at input 1.0
    Then the output should be approximately 1.0
    When I evaluate at input 0.5
    Then the output should be between 0.0 and 1.0

  Scenario: Evaluate Bezier curve with linear control points
    Given a Bezier curve with control points [(0.0, 0.0), (0.33, 0.33), (0.67, 0.67), (1.0, 1.0)]
    When I evaluate at input 0.5
    Then the output should be approximately 0.5

  Scenario: Evaluate Bezier S-curve
    Given a Bezier curve with control points [(0.0, 0.0), (0.0, 0.5), (1.0, 0.5), (1.0, 1.0)]
    When I evaluate at input 0.0
    Then the output should be approximately 0.0
    When I evaluate at input 0.5
    Then the output should be approximately 0.5
    When I evaluate at input 1.0
    Then the output should be approximately 1.0

  Scenario: LUT lookup matches direct evaluation
    Given an exponential curve with exponent 2.0
    When I convert it to a LUT
    And I lookup input 0.5
    Then the result should match direct evaluation within tolerance 0.02

  Scenario Outline: All curves map endpoints correctly
    Given a <curve_type> curve
    When I evaluate at input 0.0
    Then the output should be approximately 0.0
    When I evaluate at input 1.0
    Then the output should be approximately 1.0

    Examples:
      | curve_type      |
      | linear          |
      | exponential_2   |
      | logarithmic_10  |
      | bezier_linear   |

  Scenario: Curve output is always in valid range
    Given an exponential curve with exponent 2.0
    When I evaluate at input -0.5
    Then the output should be in range [0.0, 1.0]
    When I evaluate at input 1.5
    Then the output should be in range [0.0, 1.0]

  Scenario: LUT lookup clamps out-of-range inputs
    Given a linear LUT
    When I lookup input -10.0
    Then the output should be approximately 0.0
    When I lookup input 10.0
    Then the output should be approximately 1.0