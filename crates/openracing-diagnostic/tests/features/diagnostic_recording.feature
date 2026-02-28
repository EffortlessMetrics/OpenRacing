Feature: Diagnostic Recording
  As a system operator
  I want to record diagnostic data
  So that I can analyze and replay system behavior

  Background:
    Given a diagnostic recording system is configured
    And a temporary directory exists for recordings

  Scenario: Start and stop a basic recording
    Given a blackbox recorder is created with default configuration
    When I record 100 frames at 1kHz
    And I finalize the recording
    Then a .wbb file should be created
    And the file should contain valid WBB1 format

  Scenario: Record with all streams enabled
    Given a blackbox recorder with all streams enabled
    When I record 50 frames
    And I record 30 telemetry records
    And I record 10 health events
    And I finalize the recording
    Then the recording should contain frame data
    And the recording should contain telemetry data
    And the recording should contain health event data

  Scenario: Recording respects size limits
    Given a blackbox recorder with max file size of 1KB
    When I attempt to record 10000 frames with large node outputs
    Then the recording should fail with size limit error
    And the error should indicate file size exceeded

  Scenario: Recording respects duration limits
    Given a blackbox recorder with max duration of 1 second
    When I record frames for 2 seconds
    Then the recording should fail with duration limit error

  Scenario: Replay recorded data accurately
    Given a recording with 100 deterministic frames
    When I load the recording for replay
    And I execute replay with tolerance 1e-6
    Then all frames should match within tolerance
    And the replay should succeed

  Scenario: Deterministic replay produces identical results
    Given a recording with 50 frames
    When I replay the recording twice with seed 12345
    Then both replays should produce identical frame counts
    And both replays should produce identical deviation values

  Scenario: Generate support bundle with all components
    Given health events are recorded
    And log files exist in the log directory
    And profile files exist in the profile directory
    And blackbox recordings exist in the recording directory
    When I generate a support bundle
    Then the bundle should be a valid ZIP file
    And the bundle should contain a manifest
    And the bundle should contain system information
    And the bundle should contain health events
    And the bundle should contain log files
    And the bundle should contain profile files
    And the bundle should contain recordings

  Scenario: Support bundle respects size limit
    Given a support bundle with max size of 1MB
    When I add health events with large context data exceeding 2MB
    Then the bundle should reject the events with size limit error

  Scenario: Compressed recording is smaller than uncompressed
    Given two recorders with identical frame data
    And one recorder has compression level 9
    And one recorder has compression level 0
    When I finalize both recordings
    Then the compressed file should be smaller than uncompressed

  Scenario: File format validation
    Given a valid .wbb recording file
    When I load the file for replay
    Then the header should have valid magic number WBB1
    And the header should have version 1
    And the footer should have valid magic number 1BBW

  Scenario: Environment variable filtering in support bundle
    Given a support bundle is created
    When system information is collected
    Then CARGO_ prefixed variables should be included
    And RUST_ prefixed variables should be included
    And PASSWORD should be excluded
    And SECRET_KEY should be excluded
    And API_TOKEN should be excluded

  Scenario: Rate-limited telemetry recording
    Given a blackbox recorder with Stream B enabled
    When I record telemetry at 1kHz rate
    Then only approximately 60 records per second should be stored
    And the recording should not exceed the 60Hz limit

  Scenario: Index entries created at regular intervals
    Given a blackbox recorder with 1000 frames recorded over 1 second
    When the recording is finalized
    Then index entries should be created every 100ms
    And the index should contain approximately 10 entries

  Scenario: Empty recording creates valid file
    Given a blackbox recorder with no frames recorded
    When the recording is finalized
    Then a valid .wbb file should be created
    And replay should succeed with zero frames