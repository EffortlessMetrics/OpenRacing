Feature: Trust Store Management
  As a system administrator
  I want to manage trusted public keys
  So that I can control which signers are trusted

  Background:
    Given an empty trust store

  Scenario: Add a new trusted key
    Given a public key with identifier "my-key"
    When I add the key to the trust store with trust level "Trusted"
    Then the key should be present in the trust store
    And the trust level should be "Trusted"

  Scenario: Remove a user key
    Given a public key with identifier "user-key"
    And the key is added to the trust store with trust level "Trusted"
    When I remove the key from the trust store
    Then the key should not be present in the trust store

  Scenario: System key cannot be removed
    Given a trust store with default system keys
    When I attempt to remove a system key
    Then the operation should fail with "Cannot remove system key"
    And the system key should still be present

  Scenario: System key cannot be modified
    Given a trust store with default system keys
    When I attempt to change a system key trust level to "Distrusted"
    Then the operation should fail with "Cannot modify system key"
    And the system key trust level should remain "Trusted"

  Scenario: Update trust level
    Given a public key with identifier "test-key"
    And the key is added to the trust store with trust level "Trusted"
    When I update the trust level to "Distrusted" with reason "Key compromised"
    Then the trust level should be "Distrusted"
    And the reason should be "Key compromised"

  Scenario: Query unknown key
    Given a trust store with no keys matching fingerprint "unknown-fingerprint"
    When I query the trust level for "unknown-fingerprint"
    Then the trust level should be "Unknown"

  Scenario: List all keys
    Given 5 public keys are added to the trust store
    When I list all keys
    Then the result should contain 5 keys plus system keys

  Scenario: Export trust store
    Given 3 public keys are added to the trust store
    When I export the trust store to a file
    Then the export file should be created
    And the export file should contain the 3 keys

  Scenario: Export excludes system keys by default
    Given a trust store with system keys and 2 user keys
    When I export the trust store without system keys
    Then the export should contain only user keys

  Scenario: Import keys to trust store
    Given a trust store export file with 3 keys
    When I import the keys into an empty trust store
    Then 3 keys should be imported
    And the keys should be present in the trust store

  Scenario: Import with overwrite disabled
    Given a trust store with key "key-1" at trust level "Trusted"
    And an import file with key "key-1" at trust level "Distrusted"
    When I import the keys without overwrite
    Then the key should be skipped
    And the trust level should remain "Trusted"

  Scenario: Import with overwrite enabled
    Given a trust store with key "key-1" at trust level "Trusted"
    And an import file with key "key-1" at trust level "Distrusted"
    When I import the keys with overwrite
    Then the key should be updated
    And the trust level should be "Distrusted"

  Scenario: Persist trust store to file
    Given a trust store with 3 user keys
    And a file path for storage
    When I save the trust store
    Then the file should be created
    And the file should contain valid JSON

  Scenario: Load trust store from file
    Given a trust store file with 3 keys
    When I load the trust store
    Then the trust store should contain 3 keys plus system keys

  Scenario: Trust store statistics
    Given 3 trusted keys, 2 unknown keys, and 1 distrusted key
    When I get trust store statistics
    Then trusted_keys should be 3 plus system keys
    And unknown_keys should be 2
    And distrusted_keys should be 1
    And system_keys should be at least 1

  Scenario: Add same key twice updates entry
    Given a public key with identifier "duplicate-key"
    And the key is added with trust level "Trusted"
    When I add the same key with trust level "Distrusted"
    Then the trust level should be "Distrusted"
    And there should be only one entry for the key

  Scenario Outline: Trust level transitions
    Given a public key with identifier "transition-key"
    And the key is added with trust level "<initial>"
    When I update the trust level to "<final>"
    Then the trust level should be "<final>"

    Examples:
      | initial     | final       |
      | Trusted     | Unknown     |
      | Trusted     | Distrusted  |
      | Unknown     | Trusted     |
      | Unknown     | Distrusted  |
      | Distrusted  | Trusted     |
      | Distrusted  | Unknown     |

  Scenario: Key fingerprint uniqueness
    Given two different public keys
    When I compute their fingerprints
    Then the fingerprints should be different

  Scenario: Same key same fingerprint
    Given a public key
    When I compute the fingerprint twice
    Then the fingerprints should be identical
