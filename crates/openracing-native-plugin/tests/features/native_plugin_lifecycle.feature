Feature: Native Plugin Lifecycle
  As a plugin system developer
  I want to manage native plugin lifecycle
  So that plugins can be loaded and unloaded safely

  Background:
    Given a native plugin host with secure defaults
    And a trust store with trusted keys

  Scenario: Create host with secure defaults
    Given I create a native plugin host
    Then the host should have strict configuration
    And unsigned plugins should be rejected

  Scenario: Create host for development
    Given I create a native plugin host for development
    Then the host should have permissive configuration
    And unsigned plugins should be allowed

  Scenario: Load signed plugin with valid signature
    Given a plugin file "test.so" signed by a trusted key
    When I load the plugin
    Then the plugin should be loaded successfully
    And the plugin ID should be returned

  Scenario: Reject unsigned plugin in strict mode
    Given a plugin file "unsigned.so" without signature
    And the host is in strict mode
    When I attempt to load the plugin
    Then the load should fail
    And the error should mention "unsigned"

  Scenario: Load plugin with ABI version mismatch
    Given a plugin file "old_plugin.so" with ABI version 0
    When I attempt to load the plugin
    Then the load should fail
    And the error should mention "ABI version mismatch"

  Scenario: Unload loaded plugin
    Given a loaded plugin with ID "plugin-123"
    When I unload the plugin
    Then the plugin should be removed from the host
    And plugin resources should be freed

  Scenario: Initialize plugin with configuration
    Given a loaded native plugin
    When I initialize the plugin with configuration
    Then the plugin should accept the configuration
    And the plugin state should be updated

  Scenario: Process frame through plugin
    Given an initialized native plugin
    And a plugin frame with FFB input 0.5
    When I process the frame
    Then the output frame should be modified
    And the execution time should be within budget

  Scenario: Detect budget violation
    Given an initialized native plugin
    And a frame with budget 100 microseconds
    When the plugin exceeds the budget
    Then a BudgetViolation error should be returned

  Scenario: Shutdown plugin gracefully
    Given a loaded native plugin
    When I shutdown the plugin
    Then the plugin state should be destroyed
    And subsequent operations should fail

  Scenario Outline: Configuration modes
    Given I create a host with <mode> configuration
    Then allow_unsigned should be <allow_unsigned>
    And require_signatures should be <require_signatures>

    Examples:
      | mode        | allow_unsigned | require_signatures |
      | strict      | false          | true               |
      | permissive  | true           | true               |
      | development | true           | false              |

  Scenario: Update host configuration
    Given a host with development configuration
    When I update the configuration to strict
    Then the host should have strict configuration
    And new loads should use strict rules