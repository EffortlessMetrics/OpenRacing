Feature: Resource Limits Enforcement

  As a system administrator
  I want to enforce resource limits on WASM plugins
  So that plugins cannot consume excessive system resources

  Background:
    Given a WASM runtime with configured resource limits

  Scenario: Default resource limits are applied
    Given a runtime with default resource limits
    Then the memory limit should be 16MB
    And the fuel limit should be 10,000,000 instructions
    And the max instances should be 32

  Scenario: Custom resource limits
    Given resource limits with:
      | memory_bytes    | 8388608   |
      | fuel            | 5000000   |
      | max_instances   | 16        |
    When I create a runtime with these limits
    Then the runtime should enforce these limits

  Scenario: Conservative limits for untrusted plugins
    Given conservative resource limits
    Then the memory limit should be 4MB
    And the fuel limit should be 1,000,000 instructions
    And the max instances should be 8
    And execution timeout should be set

  Scenario: Generous limits for trusted plugins
    Given generous resource limits
    Then the memory limit should be 64MB
    And the fuel limit should be 50,000,000 instructions
    And the max instances should be 128

  Scenario: Memory limit validation
    Given resource limits with memory limit 1024 bytes
    Then validation should fail with error about minimum memory

  Scenario: Fuel limit validation
    Given resource limits with fuel limit 100
    Then validation should fail with error about minimum fuel

  Scenario: Max instances validation
    Given resource limits with max instances 0
    Then validation should fail with error about minimum instances

  Scenario: Fuel exhaustion during execution
    Given a plugin that consumes excessive fuel
    When I attempt to process data
    Then a budget violation error should be returned
    And the plugin should be disabled

  Scenario: Multiple plugins share the instance limit
    Given a runtime with max instances 3
    When I load 3 plugins
    Then loading a 4th plugin should fail
    And the error should indicate max instances reached

  Scenario: Unloading a plugin frees an instance slot
    Given a runtime with max instances 2
    And 2 loaded plugins
    When I unload one plugin
    Then I should be able to load another plugin
