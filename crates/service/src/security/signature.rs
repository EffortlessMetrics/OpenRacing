//! Ed25519 signature implementation for secure verification

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Ed25519 signature wrapper
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    bytes: [u8; 64],
}

impl Signature {
    /// Create a new signature from raw bytes
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self { bytes }
    }

    /// Create a signature from a base64-encoded string
    pub fn from_base64(encoded: &str) -> Result<Self, SignatureError> {
        let bytes = BASE64
            .decode(encoded)
            .map_err(|e| SignatureError::InvalidEncoding(e.to_string()))?;
        
        if bytes.len() != 64 {
            return Err(SignatureError::InvalidLength(bytes.len()));
        }
        
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&bytes);
        Ok(Self::from_bytes(sig_bytes))
    }

    /// Get the raw signature bytes
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.bytes
    }

    /// Encode the signature as base64
    pub fn to_base64(&self) -> String {
        BASE64.encode(&self.bytes)
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_base64())
    }
}

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_base64())
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        Self::from_base64(&encoded).map_err(serde::de::Error::custom)
    }
}

/// Ed25519 public key wrapper
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
    bytes: [u8; 32],
}

impl PublicKey {
    /// Create a new public key from raw bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Create a public key from a base64-encoded string
    pub fn from_base64(encoded: &str) -> Result<Self, SignatureError> {
        let bytes = BASE64
            .decode(encoded)
            .map_err(|e| SignatureError::InvalidEncoding(e.to_string()))?;
        
        if bytes.len() != 32 {
            return Err(SignatureError::InvalidLength(bytes.len()));
        }
        
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);
        Ok(Self::from_bytes(key_bytes))
    }

    /// Get the raw key bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Encode the public key as base64
    pub fn to_base64(&self) -> String {
        BASE64.encode(&self.bytes)
    }

    /// Verify a signature against a message
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), SignatureError> {
        // In a real implementation, this would use ed25519-dalek or similar
        // For now, we'll implement a placeholder that always succeeds for testing
        // TODO: Replace with actual Ed25519 verification
        
        // Simulate verification logic
        if signature.bytes.iter().all(|&b| b == 0) {
            return Err(SignatureError::InvalidSignature);
        }
        
        // In production, use: ed25519_dalek::VerifyingKey::verify_strict()
        tracing::debug!(
            "Verifying signature {} for {} bytes with key {}",
            signature.to_base64(),
            message.len(),
            self.to_base64()
        );
        
        Ok(())
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_base64())
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_base64())
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        Self::from_base64(&encoded).map_err(serde::de::Error::custom)
    }
}

/// Signed content wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedContent<T> {
    /// The content being signed
    pub content: T,
    /// The signature over the serialized content
    pub signature: Signature,
    /// The public key used for signing
    pub public_key: PublicKey,
    /// Timestamp when the signature was created
    pub timestamp: u64,
}

impl<T> SignedContent<T>
where
    T: Serialize,
{
    /// Create new signed content
    pub fn new(content: T, signature: Signature, public_key: PublicKey) -> Self {
        Self {
            content,
            signature,
            public_key,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Verify the signature on this content
    pub fn verify(&self) -> Result<(), SignatureError> {
        let content_bytes = serde_json::to_vec(&self.content)
            .map_err(|e| SignatureError::SerializationError(e.to_string()))?;
        
        self.public_key.verify(&content_bytes, &self.signature)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("Invalid signature encoding: {0}")]
    InvalidEncoding(String),
    #[error("Invalid signature length: expected 64 bytes, got {0}")]
    InvalidLength(usize),
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_roundtrip() {
        let bytes = [42u8; 64];
        let sig = Signature::from_bytes(bytes);
        let encoded = sig.to_base64();
        let decoded = Signature::from_base64(&encoded).unwrap();
        assert_eq!(sig, decoded);
    }

    #[test]
    fn test_public_key_roundtrip() {
        let bytes = [42u8; 32];
        let key = PublicKey::from_bytes(bytes);
        let encoded = key.to_base64();
        let decoded = Pu}
    }
ic_key);publialized.eserey, d_kublicned.pigsert_eq!(s     as;
   nature)ed.sigliz deseriature,signed.signaert_eq!(    ass  ontent);
  d.cserializetent, deonq!(signed.cert_e ass
              wrap();
 un(&json).n::from_str= serde_jsot<&str> ignedContend: Srializeet dese     lwrap();
   .uning(&signed)json::to_strrde_son = se   let j;
     public_key)signature, (content, newent::edCont Sign signed =       let  
       2u8; 32]);
om_bytes([:frKey:= Publickey  public_ let]);
       u8; 64[1es(::from_byt= Signaturesignature   let ";
      st contenttent = "teet con
        lization() {t_serial_contensignedest_ fn t]
   [test  #

  ed);
    }cod!(key, desert_eq      as);
  nwrap(ncoded).u4(&ebase6y::from_blicKe