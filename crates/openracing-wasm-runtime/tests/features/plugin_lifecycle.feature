Feature: Plugin Lifecycle Management

  As a plugin developer
  I want to manage the lifecycle of WASM plugins
  So that I can safely load, execute, and unload plugins at runtime

  Background:
    Given a WASM runtime with default resource limits
    And a valid WASM plugin module with process function

  Scenario: Load a plugin successfully
    When I load the plugin with a unique ID
    Then the plugin should be available in the runtime
    And the plugin should be initialized

  Scenario: Process data through a loaded plugin
    Given a loaded plugin with ID "test-plugin"
    When I process input value 0.5 with delta time 0.001
    Then the result should be computed correctly
    And the plugin statistics should be updated

  Scenario: Unload a plugin
    Given a loaded plugin with ID "test-plugin"
    When I unload the plugin
    Then the plugin should be removed from the runtime
    And subsequent processing attempts should fail

  Scenario: Plugin trap disables plugin
    Given a plugin with a trap instruction
    When I attempt to process data
    Then the plugin should be disabled
    And subsequent processing attempts should indicate the plugin is disabled

  Scenario: Re-enable a disabled plugin
    Given a disabled plugin
    When I re-enable the plugin
    Then the plugin should be available for processing again

  Scenario: Maximum instances limit
    Given a runtime with max instances set to 2
    When I load 2 plugins successfully
    Then loading a third plugin should fail with max instances error

  Scenario: Plugin with init function
    Given a plugin with an init function
    When I load the plugin
    Then the init function should be called
    And the plugin should be initialized after successful init

  Scenario: Plugin with shutdown function
    Given a loaded plugin with a shutdown function
    When I unload the plugin
    Then the shutdown function should be called
