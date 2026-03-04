//! Snapshot tests for CryptoError — ensure error messages are stable.

use openracing_crypto::error::CryptoError;

#[test]
fn snapshot_crypto_error_invalid_signature() {
    insta::assert_snapshot!(
        "crypto_error_invalid_signature",
        format!("{}", CryptoError::InvalidSignature)
    );
}

#[test]
fn snapshot_crypto_error_untrusted_signer() {
    let err = CryptoError::UntrustedSigner("SHA256:deadbeef".to_string());
    insta::assert_snapshot!("crypto_error_untrusted_signer", format!("{}", err));
}

#[test]
fn snapshot_crypto_error_verification_failed() {
    let err = CryptoError::VerificationFailed("Ed25519 signature invalid".to_string());
    insta::assert_snapshot!("crypto_error_verification_failed", format!("{}", err));
}

#[test]
fn snapshot_crypto_error_key_format() {
    let err = CryptoError::KeyFormatError("invalid PEM encoding".to_string());
    insta::assert_snapshot!("crypto_error_key_format", format!("{}", err));
}

#[test]
fn snapshot_crypto_error_serialization() {
    let err = CryptoError::SerializationError("JSON parse error at line 5".to_string());
    insta::assert_snapshot!("crypto_error_serialization", format!("{}", err));
}

#[test]
fn snapshot_crypto_error_trust_store() {
    let err = CryptoError::TrustStoreError("trust store file not found".to_string());
    insta::assert_snapshot!("crypto_error_trust_store", format!("{}", err));
}

#[test]
fn snapshot_crypto_error_constant_time() {
    insta::assert_snapshot!(
        "crypto_error_constant_time",
        format!("{}", CryptoError::ConstantTimeError)
    );
}

#[test]
fn snapshot_crypto_error_invalid_key_length() {
    let err = CryptoError::InvalidKeyLength {
        expected: 32,
        actual: 16,
    };
    insta::assert_snapshot!("crypto_error_invalid_key_length", format!("{}", err));
}

#[test]
fn snapshot_crypto_error_invalid_signature_length() {
    let err = CryptoError::InvalidSignatureLength {
        expected: 64,
        actual: 48,
    };
    insta::assert_snapshot!("crypto_error_invalid_sig_length", format!("{}", err));
}

#[test]
fn snapshot_crypto_error_key_not_found() {
    let err = CryptoError::KeyNotFound("moza-signing-key-2024".to_string());
    insta::assert_snapshot!("crypto_error_key_not_found", format!("{}", err));
}

#[test]
fn snapshot_crypto_error_system_key_protected() {
    insta::assert_snapshot!(
        "crypto_error_system_key_protected",
        format!("{}", CryptoError::SystemKeyProtected)
    );
}
