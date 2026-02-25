Feature: Pipeline Compilation
  As an FFB system developer
  I want to compile filter configurations into executable pipelines
  So that I can process force feedback with RT-safe guarantees

  Background:
    Given the pipeline compiler is available

  Scenario: Compile default configuration
    Given a default filter configuration
    When I compile the configuration
    Then the compilation should succeed
    And the pipeline should be created

  Scenario: Compile configuration with all filters
    Given a filter configuration with reconstruction level 4
    And friction coefficient 0.1
    And damper coefficient 0.15
    And inertia coefficient 0.05
    And a 60Hz notch filter
    And slew rate 0.8
    And a nonlinear curve
    When I compile the configuration
    Then the compilation should succeed
    And the pipeline should have multiple nodes

  Scenario: Compile empty configuration
    Given an empty filter configuration
    When I compile the configuration
    Then the compilation should succeed
    And the pipeline should be empty

  Scenario: Configuration hash is deterministic
    Given a filter configuration
    When I compile the configuration twice
    Then the configuration hashes should be equal

  Scenario: Different configurations have different hashes
    Given two different filter configurations
    When I compile both configurations
    Then the configuration hashes should be different

  Scenario: Validate invalid reconstruction level
    Given a configuration with reconstruction level 10
    When I validate the configuration
    Then validation should fail with InvalidConfig error

  Scenario: Validate invalid gain value
    Given a configuration with friction value 1.5
    When I validate the configuration
    Then validation should fail with InvalidParameters error

  Scenario: Validate non-monotonic curve
    Given a configuration with non-monotonic curve points
    When I validate the configuration
    Then validation should fail with NonMonotonicCurve error

  Scenario: Compile with response curve
    Given a filter configuration
    And an exponential response curve with exponent 2.0
    When I compile with the response curve
    Then the compilation should succeed
    And the pipeline should have a response curve

  Scenario: Response curve affects hash
    Given a filter configuration
    When I compile with no response curve
    And I compile with a linear response curve
    Then the hashes should be different

  Scenario Outline: Compile with various reconstruction levels
    Given a configuration with reconstruction level <level>
    When I compile the configuration
    Then the compilation should <result>

    Examples:
      | level | result          |
      | 0     | succeed         |
      | 1     | succeed         |
      | 4     | succeed         |
      | 8     | succeed         |
      | 9     | fail validation |

  Scenario: Async compilation returns result
    Given a filter configuration
    When I compile the configuration asynchronously
    Then I should receive a result channel
    And the compilation result should be received

  Scenario: Pipeline swap is atomic
    Given a compiled pipeline
    And another compiled pipeline with different hash
    When I swap the pipelines at tick boundary
    Then the swap should be atomic
    And the new pipeline should be active
