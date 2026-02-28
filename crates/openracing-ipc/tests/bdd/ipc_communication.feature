Feature: IPC Communication
  As an OpenRacing client
  I want to communicate with the IPC server
  So that I can control wheel devices and receive updates

  Background:
    Given the IPC server is available

  Scenario: Server starts and stops successfully
    Given I create an IPC server with default configuration
    When I start the server
    Then the server should be in the Running state
    When I stop the server
    Then the server should be in the Stopped state

  Scenario: Feature negotiation with compatible client
    Given I create an IPC server with default configuration
    And I start the server
    When I negotiate features with client version "1.0.0"
    And I request features ["device_management", "profile_management"]
    Then negotiation should succeed
    And enabled features should include "device_management"
    And enabled features should include "profile_management"
    And the server version should be "1.0.0"
    When I stop the server

  Scenario: Feature negotiation with incompatible client
    Given I create an IPC server with default configuration
    And I start the server
    When I negotiate features with client version "0.1.0"
    Then negotiation should fail with incompatibility

  Scenario: Health event broadcasting
    Given I create an IPC server with default configuration
    And I subscribe to health events
    When I broadcast a health event for device "device-1"
    Then I should receive the health event
    And the event device ID should be "device-1"

  Scenario: Multiple health events in sequence
    Given I create an IPC server with default configuration
    And I subscribe to health events
    When I broadcast 10 health events
    Then I should receive 10 health events

  Scenario: Client registration and tracking
    Given I create an IPC server with default configuration
    When I register a client with ID "test-client"
    Then the client count should be 1
    When I unregister the client with ID "test-client"
    Then the client count should be 0

  Scenario: Version compatibility checking
    Given I have a minimum version requirement of "1.0.0"
    When I check compatibility with client version "1.0.0"
    Then the versions should be compatible
    When I check compatibility with client version "1.1.0"
    Then the versions should be compatible
    When I check compatibility with client version "1.0.5"
    Then the versions should be compatible
    When I check compatibility with client version "0.9.0"
    Then the versions should not be compatible
    When I check compatibility with client version "2.0.0"
    Then the versions should not be compatible

  Scenario: Message header encoding and decoding
    Given I create a message header with type DEVICE and payload length 1024
    When I encode the header
    And I decode the encoded header
    Then the decoded message type should be DEVICE
    And the decoded payload length should be 1024

  Scenario: Message size validation
    Given I create a codec with max size 1000
    When I validate a size of 500
    Then the size should be valid
    When I validate a size of 1000
    Then the size should be valid
    When I validate a size of 0
    Then the size should be invalid
    When I validate a size of 1001
    Then the size should be invalid

  Scenario: Transport type configuration
    Given I create a TCP transport
    Then the transport description should contain "TCP"
    Given I create a transport config with max connections 50
    Then the max connections should be 50

  Scenario Outline: Version compatibility matrix
    Given I have a minimum version requirement of "<min_version>"
    When I check compatibility with client version "<client_version>"
    Then the compatibility result should be <expected>

    Examples:
      | min_version | client_version | expected |
      | 1.0.0       | 1.0.0          | true     |
      | 1.0.0       | 1.1.0          | true     |
      | 1.0.0       | 1.0.1          | true     |
      | 1.0.0       | 0.9.0          | false    |
      | 1.0.0       | 2.0.0          | false    |
      | 1.1.0       | 1.0.0          | false    |
      | 1.1.0       | 1.1.0          | true     |
      | 1.1.0       | 1.2.0          | true     |

  Scenario: Concurrent client connections
    Given I create an IPC server with max connections 100
    And I start the server
    When 50 clients negotiate features successfully
    Then the client count should be 50
    When I stop the server

  Scenario: Server restart capability
    Given I create an IPC server with default configuration
    When I start the server
    And I stop the server
    And I start the server again
    Then the server should be in the Running state
    When I stop the server
