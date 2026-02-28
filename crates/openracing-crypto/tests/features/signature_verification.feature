Feature: Signature Verification
  As a security-conscious user
  I want to verify signatures on plugins, firmware, and binaries
  So that I can trust the authenticity of software components

  Background:
    Given a trust store with trusted keys
    And a keypair for signing

  Scenario: Verify a valid plugin signature
    Given a plugin file "test.wasm" with content "(module)"
    And the file is signed with a trusted key
    When I verify the plugin signature
    Then the signature should be valid
    And the trust level should be "Trusted"

  Scenario: Detect tampered content
    Given a plugin file "test.wasm" with content "(module)"
    And the file is signed with a trusted key
    When the content is modified to "(module (func))"
    And I verify the plugin signature
    Then the signature should be invalid

  Scenario: Reject unknown signer
    Given a plugin file "test.wasm" with content "(module)"
    And the file is signed with an unknown key
    When I verify the plugin signature
    Then verification should fail with "UntrustedSigner" error

  Scenario: Flag distrusted signer
    Given a plugin file "test.wasm" with content "(module)"
    And the file is signed with a distrusted key
    When I verify the plugin signature
    Then the signature should be valid
    And the trust level should be "Distrusted"
    And a warning should be generated

  Scenario: Verify firmware with strict policy
    Given a firmware file "firmware.fw"
    And the file is signed with a trusted key
    And firmware signature requirement is enabled
    When I verify the firmware signature
    Then the signature should be valid
    And the trust level should be "Trusted"

  Scenario: Reject unsigned firmware
    Given a firmware file "firmware.fw" without signature
    And firmware signature requirement is enabled
    When I verify the firmware signature
    Then verification should fail

  Scenario: Verify binary executable
    Given a binary file "wheeld.exe"
    And the file is signed with a trusted key
    When I verify the binary signature
    Then the signature should be valid

  Scenario: Allow unsigned profile
    Given a profile file "car.profile.json" without signature
    When I verify the profile signature
    Then verification should succeed with no signature

  Scenario: Verify update package
    Given an update package "v1.0.0.wup"
    And the package is signed with a trusted key
    When I verify the update signature
    Then the signature should be valid
    And the trust level should be "Trusted"

  Scenario Outline: Verify signatures for different content types
    Given a <type> file "<filename>"
    And the file is signed with a trusted key
    When I verify the <type> signature
    Then the signature should be valid

    Examples:
      | type      | filename       |
      | plugin    | test.wasm      |
      | firmware  | firmware.fw    |
      | binary    | wheeld.exe     |
      | update    | update.wup     |

  Scenario: Verify with corrupted signature
    Given a plugin file "test.wasm" with content "(module)"
    And the file is signed with a trusted key
    And the signature is corrupted
    When I verify the plugin signature
    Then the signature should be invalid

  Scenario: Verify with wrong public key
    Given a plugin file "test.wasm" with content "(module)"
    And the file is signed with key A
    And the trust store contains only key B
    When I verify the plugin signature
    Then verification should fail with "UntrustedSigner" error
