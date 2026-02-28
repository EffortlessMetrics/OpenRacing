Feature: Curve Validation
  As an FFB system developer
  I want to validate curve parameters
  So that invalid configurations are rejected early

  Background:
    Given the curves crate is available

  Scenario: Valid Bezier curve creation
    Given I want to create a Bezier curve
    When I provide control points [(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)]
    Then the curve should be created successfully
    And the curve should pass validation

  Scenario: Reject Bezier curve with x coordinate out of range
    Given I want to create a Bezier curve
    When I provide control points [(0.0, 0.0), (1.5, 0.5), (0.75, 0.5), (1.0, 1.0)]
    Then creation should fail with ControlPointOutOfRange error
    And the error should reference point index 1
    And the error should reference coordinate "x"

  Scenario: Reject Bezier curve with y coordinate out of range
    Given I want to create a Bezier curve
    When I provide control points [(0.0, 0.0), (0.25, -0.1), (0.75, 0.5), (1.0, 1.0)]
    Then creation should fail with ControlPointOutOfRange error
    And the error should reference point index 1
    And the error should reference coordinate "y"

  Scenario: Reject Bezier curve with NaN control point
    Given I want to create a Bezier curve
    When I provide control points with NaN coordinates
    Then creation should fail with ControlPointOutOfRange error

  Scenario: Reject Bezier curve with Infinity control point
    Given I want to create a Bezier curve
    When I provide control points with Infinity coordinates
    Then creation should fail with ControlPointOutOfRange error

  Scenario: Valid exponential curve creation
    Given I want to create an exponential curve
    When I provide exponent 2.0
    Then the curve should be created successfully
    And the curve should pass validation

  Scenario: Reject exponential curve with zero exponent
    Given I want to create an exponential curve
    When I provide exponent 0.0
    Then creation should fail with InvalidConfiguration error
    And the error message should mention "must be > 0"

  Scenario: Reject exponential curve with negative exponent
    Given I want to create an exponential curve
    When I provide exponent -1.0
    Then creation should fail with InvalidConfiguration error

  Scenario: Reject exponential curve with NaN exponent
    Given I want to create an exponential curve
    When I provide exponent NaN
    Then creation should fail with InvalidConfiguration error
    And the error message should mention "finite"

  Scenario: Valid logarithmic curve creation
    Given I want to create a logarithmic curve
    When I provide base 10.0
    Then the curve should be created successfully
    And the curve should pass validation

  Scenario: Reject logarithmic curve with base 1
    Given I want to create a logarithmic curve
    When I provide base 1.0
    Then creation should fail with InvalidConfiguration error
    And the error message should mention "must be > 1"

  Scenario: Reject logarithmic curve with base less than 1
    Given I want to create a logarithmic curve
    When I provide base 0.5
    Then creation should fail with InvalidConfiguration error

  Scenario Outline: Valid exponential exponents
    Given I want to create an exponential curve
    When I provide exponent <exponent>
    Then the curve should be created successfully

    Examples:
      | exponent |
      | 0.001    |
      | 0.5      |
      | 1.0      |
      | 2.0      |
      | 10.0     |
      | 100.0    |

  Scenario Outline: Invalid exponential exponents
    Given I want to create an exponential curve
    When I provide exponent <exponent>
    Then creation should fail with InvalidConfiguration error

    Examples:
      | exponent       |
      | 0.0            |
      | -1.0           |
      | -0.5           |
      | NaN            |
      | Infinity       |
      | -Infinity      |

  Scenario Outline: Valid logarithmic bases
    Given I want to create a logarithmic curve
    When I provide base <base>
    Then the curve should be created successfully

    Examples:
      | base     |
      | 1.001    |
      | 2.0      |
      | 2.718    |
      | 10.0     |
      | 100.0    |

  Scenario Outline: Invalid logarithmic bases
    Given I want to create a logarithmic curve
    When I provide base <base>
    Then creation should fail with InvalidConfiguration error

    Examples:
      | base      |
      | 0.0       |
      | 0.5       |
      | 1.0       |
      | -5.0      |
      | NaN       |
      | Infinity  |
      | -Infinity |

  Scenario: Linear curve always valid
    Given a linear curve
    Then the curve should pass validation

  Scenario: Custom LUT is always valid
    Given a custom LUT
    When I wrap it in CurveType::Custom
    Then the curve should pass validation

  Scenario: Validation consistency between constructor and validate
    Given I want to create an exponential curve
    When I provide exponent -1.0
    Then creation should fail
    When I create an exponential curve directly with exponent -1.0
    And I call validate
    Then validation should fail

  Scenario: Error messages are descriptive
    Given I want to create an exponential curve
    When I provide exponent -1.0
    Then the error message should contain useful information
    And the error message should mention the invalid value -1.0