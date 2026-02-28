Feature: Pipeline Execution
  As an FFB system developer
  I want to execute pipelines with RT-safe guarantees
  So that force feedback is processed correctly at 1kHz

  Background:
    Given the pipeline system is available

  Scenario: Process frame through empty pipeline
    Given an empty pipeline
    And a frame with torque 0.5
    When I process the frame
    Then the output should be approximately 0.5
    And processing should succeed

  Scenario: Process frame with response curve linear
    Given a pipeline with linear response curve
    And a frame with torque 0.5
    When I process the frame
    Then the output should be approximately 0.5
    And processing should succeed

  Scenario: Process frame with exponential response curve
    Given a pipeline with exponential response curve exponent 2.0
    And a frame with torque 0.5
    When I process the frame
    Then the output should be approximately 0.25

  Scenario: Response curve preserves sign
    Given a pipeline with exponential response curve exponent 2.0
    And a frame with positive torque 0.5
    When I process the frame
    Then the output should be positive
    When I process a frame with negative torque -0.5
    Then the output should be negative
    And the magnitudes should be equal

  Scenario: Pipeline validates output bounds
    Given an empty pipeline
    And a frame with torque 2.0
    When I process the frame
    Then processing should fail with PipelineFault

  Scenario: Pipeline validates NaN output
    Given an empty pipeline
    And a frame with NaN torque
    When I process the frame
    Then processing should fail with PipelineFault

  Scenario: Pipeline is RT-safe
    Given a pipeline
    When I process 10000 frames
    Then no allocations should occur in hot path
    And all outputs should be finite
    And all outputs should be bounded

  Scenario: Pipeline swap during execution
    Given an active pipeline
    When I swap to a new pipeline mid-execution
    Then the swap should be atomic
    And subsequent frames should use the new pipeline

  Scenario: Process with external response curve
    Given a pipeline without response curve
    And an external exponential response curve
    When I process with the external curve
    Then the output should be transformed by the curve

  Scenario: Multiple sequential process calls
    Given a pipeline with response curve
    When I process 100 sequential frames
    Then all process calls should succeed
    And outputs should be deterministic

  Scenario Outline: Process with various input ranges
    Given an empty pipeline
    And a frame with torque <torque>
    When I process the frame
    Then processing should <result>

    Examples:
      | torque | result    |
      | 0.0    | succeed   |
      | 0.5    | succeed   |
      | 1.0    | succeed   |
      | -0.5   | succeed   |
      | -1.0   | succeed   |
      | 1.5    | fail      |
      | -1.5   | fail      |
      | NaN    | fail      |
      | Inf    | fail      |

  Scenario: Pipeline state snapshot
    Given a compiled pipeline
    When I request a state snapshot
    Then I should receive node count
    And I should receive state size
    And I should receive configuration hash

  Scenario: Pipeline state alignment
    Given a compiled pipeline with filters
    When I check state alignment
    Then all state offsets should be aligned

  Scenario: Performance is bounded
    Given a pipeline with 10 filter nodes
    When I process a frame
    Then execution time should be bounded
    And no syscalls should occur
