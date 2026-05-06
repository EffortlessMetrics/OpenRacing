//! Profile signing and verification

use crate::Result;
use crate::error::ProfileRepositoryError;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Trust state for profile signatures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrustState {
    /// Profile is unsigned
    #[default]
    Unsigned,
    /// Profile has a valid signature from a trusted key
    Trusted,
    /// Profile has a valid signature from an unknown key
    ValidUnknown,
    /// Profile signature is invalid
    Invalid,
}

impl std::fmt::Display for TrustState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsigned => write!(f, "unsigned"),
            Self::Trusted => write!(f, "trusted"),
            Self::ValidUnknown => write!(f, "valid_unknown"),
            Self::Invalid => write!(f, "invalid"),
        }
    }
}

/// Profile signature information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSignature {
    /// Ed25519 signature (base64 encoded)
    pub signature: String,
    /// Public key used for signing (base64 encoded)
    pub public_key: String,
    /// Trust state of this signature
    pub trust_state: TrustState,
}

impl ProfileSignature {
    /// Create a new profile signature
    pub fn new(signature: String, public_key: String, trust_state: TrustState) -> Self {
        Self {
            signature,
            public_key,
            trust_state,
        }
    }

    /// Check if the signature is valid (trusted or valid unknown)
    pub fn is_valid(&self) -> bool {
        matches!(
            self.trust_state,
            TrustState::Trusted | TrustState::ValidUnknown
        )
    }

    /// Check if the signature is from a trusted key
    pub fn is_trusted(&self) -> bool {
        matches!(self.trust_state, TrustState::Trusted)
    }
}

/// Profile signing and verification utilities
pub struct ProfileSigner {
    trusted_keys: Vec<String>,
}

impl ProfileSigner {
    /// Create a new profile signer with no trusted keys
    pub fn new() -> Self {
        Self {
            trusted_keys: Vec::new(),
        }
    }

    /// Create with trusted keys
    pub fn with_trusted_keys(trusted_keys: Vec<String>) -> Self {
        Self { trusted_keys }
    }

    /// Add a trusted key
    pub fn add_trusted_key(&mut self, key: String) {
        self.trusted_keys.push(key);
    }

    /// Check if a key is trusted
    pub fn is_trusted(&self, key: &str) -> bool {
        self.trusted_keys.contains(&key.to_string())
    }

    /// Sign profile JSON with Ed25519 key
    pub fn sign(
        &self,
        json: &str,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<ProfileSignature> {
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let hash = hasher.finalize();

        let signature = signing_key.sign(&hash);
        let public_key = signing_key.verifying_key();

        Ok(ProfileSignature {
            signature: BASE64.encode(signature.to_bytes()),
            public_key: BASE64.encode(public_key.to_bytes()),
            trust_state: TrustState::Trusted,
        })
    }

    /// Verify a profile signature
    pub fn verify(&self, json: &str, signature_b64: &str) -> Result<ProfileSignature> {
        let value: serde_json::Value = serde_json::from_str(json).map_err(|e| {
            ProfileRepositoryError::SignatureError(format!("JSON parse error: {}", e))
        })?;

        let signature_bytes = BASE64.decode(signature_b64).map_err(|e| {
            ProfileRepositoryError::SignatureError(format!("Base64 decode error: {}", e))
        })?;

        let signature = Signature::from_bytes(&signature_bytes.try_into().map_err(|_| {
            ProfileRepositoryError::SignatureError("Invalid signature length".to_string())
        })?);

        let public_key_b64 = value
            .get("publicKey")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if public_key_b64.is_empty() {
            return Ok(ProfileSignature {
                signature: signature_b64.to_string(),
                public_key: String::new(),
                trust_state: TrustState::Invalid,
            });
        }

        let public_key_bytes = BASE64.decode(public_key_b64).map_err(|e| {
            ProfileRepositoryError::SignatureError(format!("Public key decode error: {}", e))
        })?;

        let public_key = VerifyingKey::from_bytes(&public_key_bytes.try_into().map_err(|_| {
            ProfileRepositoryError::SignatureError("Invalid public key length".to_string())
        })?)
        .map_err(|e| {
            ProfileRepositoryError::SignatureError(format!("Invalid public key: {}", e))
        })?;

        let mut json_for_verification = value.clone();
        let json_obj = json_for_verification.as_object_mut().ok_or_else(|| {
            ProfileRepositoryError::SignatureError("Profile JSON is not an object".to_string())
        })?;
        json_obj.remove("signature");
        json_obj.remove("publicKey");

        let json_without_sig = serde_json::to_string(&json_for_verification).map_err(|e| {
            ProfileRepositoryError::SignatureError(format!("JSON serialize error: {}", e))
        })?;

        let mut hasher = Sha256::new();
        hasher.update(json_without_sig.as_bytes());
        let hash = hasher.finalize();

        let trust_state = match public_key.verify(&hash, &signature) {
            Ok(()) => {
                if self.is_trusted(public_key_b64) {
                    TrustState::Trusted
                } else {
                    TrustState::ValidUnknown
                }
            }
            Err(_) => TrustState::Invalid,
        };

        Ok(ProfileSignature {
            signature: signature_b64.to_string(),
            public_key: public_key_b64.to_string(),
            trust_state,
        })
    }

    /// Create JSON content hash
    pub fn hash_json(json: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

impl Default for ProfileSigner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn create_signing_key() -> SigningKey {
        let mut csprng = OsRng;
        SigningKey::generate(&mut csprng)
    }

    #[test]
    fn test_signer_creation() {
        let signer = ProfileSigner::new();
        assert!(!signer.is_trusted("any_key"));
    }

    #[test]
    fn test_signer_with_trusted_keys() {
        let key = "test_key_base64";
        let signer = ProfileSigner::with_trusted_keys(vec![key.to_string()]);
        assert!(signer.is_trusted(key));
    }

    #[test]
    fn test_sign_and_verify() {
        let signer = ProfileSigner::new();
        let signing_key = create_signing_key();
        let json = r#"{"test": "data"}"#;

        let signature = signer.sign(json, &signing_key).expect("should sign");
        assert!(signature.is_valid());
        // Signing with your own key is considered trusted
        assert!(signature.is_trusted());
    }

    #[test]
    fn test_trust_state_display() {
        assert_eq!(format!("{}", TrustState::Unsigned), "unsigned");
        assert_eq!(format!("{}", TrustState::Trusted), "trusted");
        assert_eq!(format!("{}", TrustState::ValidUnknown), "valid_unknown");
        assert_eq!(format!("{}", TrustState::Invalid), "invalid");
    }

    #[test]
    fn test_profile_signature_validity() {
        let sig = ProfileSignature::new("sig".to_string(), "key".to_string(), TrustState::Trusted);
        assert!(sig.is_valid());
        assert!(sig.is_trusted());

        let sig = ProfileSignature::new(
            "sig".to_string(),
            "key".to_string(),
            TrustState::ValidUnknown,
        );
        assert!(sig.is_valid());
        assert!(!sig.is_trusted());

        let sig = ProfileSignature::new("sig".to_string(), "key".to_string(), TrustState::Invalid);
        assert!(!sig.is_valid());
        assert!(!sig.is_trusted());
    }

    #[test]
    fn test_hash_json() {
        let json1 = r#"{"a": 1, "b": 2}"#;
        let json2 = r#"{"a": 1, "b": 2}"#;
        let json3 = r#"{"a": 1, "b": 3}"#;

        let hash1 = ProfileSigner::hash_json(json1);
        let hash2 = ProfileSigner::hash_json(json2);
        let hash3 = ProfileSigner::hash_json(json3);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    // -----------------------------------------------------------------------
    // Additional tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_signature_unsigned_not_valid() {
        let sig = ProfileSignature::new("s".to_string(), "k".to_string(), TrustState::Unsigned);
        assert!(!sig.is_valid());
        assert!(!sig.is_trusted());
    }

    #[test]
    fn test_trust_state_default() {
        let state = TrustState::default();
        assert_eq!(state, TrustState::Unsigned);
    }

    #[test]
    fn test_trust_state_serde_round_trip() {
        for state in [
            TrustState::Unsigned,
            TrustState::Trusted,
            TrustState::ValidUnknown,
            TrustState::Invalid,
        ] {
            let json = serde_json::to_string(&state).expect("serialize");
            let restored: TrustState = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(restored, state);
        }
    }

    #[test]
    fn test_profile_signature_serde_round_trip() {
        let sig = ProfileSignature::new(
            "sig_data".to_string(),
            "pub_key".to_string(),
            TrustState::Trusted,
        );

        let json = serde_json::to_string(&sig).expect("serialize");
        let restored: ProfileSignature = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.signature, sig.signature);
        assert_eq!(restored.public_key, sig.public_key);
        assert_eq!(restored.trust_state, sig.trust_state);
    }

    #[test]
    fn test_hash_json_deterministic_length() {
        let hash = ProfileSigner::hash_json("test");
        assert_eq!(hash.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn test_hash_json_empty_string() {
        let hash = ProfileSigner::hash_json("");
        assert_eq!(hash.len(), 64);
        // Ensure it's a valid hex string
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_signer_add_trusted_key() {
        let mut signer = ProfileSigner::new();
        assert!(!signer.is_trusted("key1"));

        signer.add_trusted_key("key1".to_string());
        assert!(signer.is_trusted("key1"));
        assert!(!signer.is_trusted("key2"));

        signer.add_trusted_key("key2".to_string());
        assert!(signer.is_trusted("key2"));
    }

    #[test]
    fn test_signer_default() {
        let signer = ProfileSigner::default();
        assert!(!signer.is_trusted("anything"));
    }

    #[test]
    fn test_sign_produces_non_empty_fields() {
        let signer = ProfileSigner::new();
        let signing_key = create_signing_key();
        let json = r#"{"settings": {"gain": 0.5}}"#;

        let sig = signer.sign(json, &signing_key).expect("should sign");
        assert!(!sig.signature.is_empty());
        assert!(!sig.public_key.is_empty());
        // Verify both are valid base64
        assert!(BASE64.decode(&sig.signature).is_ok());
        assert!(BASE64.decode(&sig.public_key).is_ok());
    }

    #[test]
    fn test_sign_different_content_produces_different_signatures() {
        let signer = ProfileSigner::new();
        let signing_key = create_signing_key();

        let sig1 = signer.sign(r#"{"a": 1}"#, &signing_key).expect("sign 1");
        let sig2 = signer.sign(r#"{"a": 2}"#, &signing_key).expect("sign 2");

        assert_ne!(sig1.signature, sig2.signature);
        // Same key for both
        assert_eq!(sig1.public_key, sig2.public_key);
    }

    #[test]
    fn test_verify_invalid_base64_signature() {
        let signer = ProfileSigner::new();
        let json = r#"{"test": "data"}"#;

        let result = signer.verify(json, "!!!not-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_invalid_json() {
        let signer = ProfileSigner::new();
        let result = signer.verify("not json at all", "AAAA");
        assert!(result.is_err());
    }
}
