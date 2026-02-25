Feature: Signature Verification
  As a security-conscious user
  I want to verify signatures on native plugins
  So that I can trust the authenticity of plugins

  Background:
    Given a trust store for plugin verification
    And Ed25519 key pairs for signing

  Scenario: Verify valid signature from trusted signer
    Given a plugin file "plugin.so" with content "plugin data"
    And the file is signed with a trusted key
    When I verify the signature
    Then verification should succeed
    And the trust level should be "Trusted"

  Scenario: Reject tampered plugin
    Given a plugin file "plugin.so" signed with a trusted key
    When the content is modified after signing
    And I verify the signature
    Then verification should fail
    And the error should indicate signature mismatch

  Scenario: Reject plugin signed by distrusted key
    Given a plugin file "plugin.so" signed with a distrusted key
    When I verify the signature
    Then verification should fail
    And the error should mention "distrusted"

  Scenario: Warn about unknown signer
    Given a plugin file "plugin.so" signed with an unknown key
    And the trust store does not contain the key
    When I verify the signature
    Then verification should succeed with warning
    And the trust level should be "Unknown"

  Scenario: Reject unsigned plugin in strict mode
    Given a plugin file "plugin.so" without signature
    And the configuration requires signatures
    And unsigned plugins are not allowed
    When I verify the signature
    Then verification should fail
    And the error should mention "unsigned"

  Scenario: Allow unsigned plugin in permissive mode
    Given a plugin file "plugin.so" without signature
    And the configuration allows unsigned plugins
    When I verify the signature
    Then verification should succeed
    And a warning should be generated

  Scenario Outline: Configuration impact on verification
    Given configuration with require_signatures=<req> and allow_unsigned=<allow>
    And a plugin file "<plugin_type>"
    When I verify the signature
    Then the result should be <result>

    Examples:
      | req   | allow | plugin_type | result    |
      | true  | false | signed      | verified  |
      | true  | false | unsigned    | rejected  |
      | true  | true  | signed      | verified  |
      | true  | true  | unsigned    | accepted  |
      | false | true  | signed      | accepted  |
      | false | true  | unsigned    | accepted  |

  Scenario: Signature metadata extraction
    Given a plugin file "plugin.so" signed by "Test Signer"
    And the signature was created at timestamp T
    When I extract the signature metadata
    Then the signer name should be "Test Signer"
    And the timestamp should be T
    And the key fingerprint should be present

  Scenario: Multiple signatures on single plugin
    Given a plugin file "plugin.so"
    And the file has a detached signature file
    When I verify the signature
    Then the detached signature should be used

  Scenario: Signature verification logging
    Given a plugin file "signed.so" with valid signature
    When I verify the signature
    Then an info log should be generated
    And the log should contain the signer name

  Scenario: Verify plugin with corrupted signature file
    Given a plugin file "plugin.so"
    And a corrupted signature file
    When I verify the signature
    Then verification should fail
    And the error should indicate parsing failure

  Scenario: Empty trust store behavior
    Given an empty trust store
    And a plugin file "plugin.so" signed with key K
    When I verify the signature
    Then the key K should be marked as unknown
    And verification result depends on configuration