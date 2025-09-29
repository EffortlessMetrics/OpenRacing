//! Trust store for managing trusted public keys

use crate::security::signature::PublicKey;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

/// Trust level for public keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrustLevel {
    /// Explicitly untrusted - will be rejected
    Untrusted = 0,
    /// Unknown key - requires user approval
    Unknown = 1,
    /// Trusted for specific operations
    Trusted = 2,
    /// Highly trusted - can perform all operations
    HighlyTrusted = 3,
}

impl Default for TrustLevel {
    fn default() -> Self {
        TrustLevel::Unknown
    }
}

/// Trust entry for a public key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEntry {
    /// The public key
    pub public_key: PublicKey,
    /// Trust level for this key
    pub trust_level: TrustLevel,
    /// Human-readable name/description
    pub name: String,
    /// When this entry was added
    pub added_at: chrono::DateTime<chrono::Utc>,
    /// When this entry was last used
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
    /// Allowed operations for this key
    pub allowed_operations: Vec<Operation>,
    /// Optional expiration date
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Operations that can be performed with a trusted key
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    /// Sign application updates
    SignUpdates,
    /// Sign firmware images
    SignFirmware,
    /// Sign plugins
    SignPlugins,
    /// Sign profiles
    SignProfiles,
    /// All operations
    All,
}

/// Trust store for managing public keys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustStore {
    /// Version of the trust store format
    pub version: String,
    /// Map of key fingerprint to trust entry
    pub entries: HashMap<String, TrustEntry>,
    /// Default trust level for unknown keys
    pub default_trust_level: TrustLevel,
    /// When the trust store was last modified
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

impl Default for TrustStore {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            entries: HashMap::new(),
            default_trust_level: TrustLevel::Unknown,
            last_modified: chrono::Utc::now(),
        }
    }
}

impl TrustStore {
    /// Create a new empty trust store
    pub fn new() -> Self {
        Self::default()
    }

    /// Load trust store from file, or create new if it doesn't exist
    pub async fn load_or_create(path: &str) -> Result<Self, TrustStoreError> {
        let path = Path::new(path);
        
        if path.exists() {
            Self::load(path).await
        } else {
            let store = Self::new();
            store.save(path).await?;
            Ok(store)
        }
    }

    /// Load trust store from file
    pub async fn load(path: &Path) -> Result<Self, TrustStoreError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| TrustStoreError::IoError(e.to_string()))?;
        
        serde_json::from_str(&content)
            .map_err(|e| TrustStoreError::ParseError(e.to_string()))
    }

    /// Save trust store to file
    pub async fn save(&self, path: &Path) -> Result<(), TrustStoreError> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| TrustStoreError::IoError(e.to_string()))?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| TrustStoreError::SerializationError(e.to_string()))?;
        
        fs::write(path, content).await
            .map_err(|e| TrustStoreError::IoError(e.to_string()))?;
        
        Ok(())
    }

    /// Add a trusted public key
    pub fn add_key(
        &mut self,
        public_key: PublicKey,
        trust_level: TrustLevel,
        name: String,
        allowed_operations: Vec<Operation>,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> String {
        let fingerprint = self.compute_fingerprint(&public_key);
        
        let entry = TrustEntry {
            public_key,
            trust_level,
            name,
            added_at: chrono::Utc::now(),
            last_used: None,
            allowed_operations,
            expires_at,
        };
        
        self.entries.insert(fingerprint.clone(), entry);
        self.last_modified = chrono::Utc::now();
        
        fingerprint
    }

    /// Get trust level for a public key
    pub fn get_trust_level(&self, public_key: &PublicKey) -> TrustLevel {
        let fingerprint = self.compute_fingerprint(public_key);
        
        match self.entries.get(&fingerprint) {
            Some(entry) => {
                // Check if key has expired
                if let Some(expires_at) = entry.expires_at {
                    if chrono::Utc::now() > expires_at {
                        return TrustLevel::Untrusted;
                    }
                }
                entry.trust_level
            }
            None => self.default_trust_level,
        }
    }

    /// Check if a key is allowed to perform an operation
    pub fn is_operation_allowed(&self, public_key: &PublicKey, operation: &Operation) -> bool {
        let fingerprint = self.compute_fingerprint(public_key);
        
        match self.entries.get(&fingerprint) {
            Some(entry) => {
                // Check trust level first
                if entry.trust_level == TrustLevel::Untrusted {
                    return false;
                }
                
                // Check if key has expired
                if let Some(expires_at) = entry.expires_at {
                    if chrono::Utc::now() > expires_at {
                        return false;
                    }
                }
                
                // Check allowed operations
                entry.allowed_operations.contains(operation) 
                    || entry.allowed_operations.contains(&Operation::All)
            }
            None => false, // Unknown keys can't perform operations by default
        }
    }

    /// Update last used timestamp for a key
    pub fn mark_key_used(&mut self, public_key: &PublicKey) {
        let fingerprint = self.compute_fingerprint(public_key);
        
        if let Some(entry) = self.entries.get_mut(&fingerprint) {
            entry.last_used = Some(chrono::Utc::now());
            self.last_modified = chrono::Utc::now();
        }
    }

    /// Remove a key from the trust store
    pub fn remove_key(&mut self, public_key: &PublicKey) -> bool {
        let fingerprint = self.compute_fingerprint(public_key);
        let removed = self.entries.remove(&fingerprint).is_some();
        
        if removed {
            self.last_modified = chrono::Utc::now();
        }
        
        removed
    }

    /// List all trusted keys
    pub fn list_keys(&self) -> Vec<&TrustEntry> {
        self.entries.values().collect()
    }

    /// Get a specific trust entry
    pub fn get_entry(&self, public_key: &PublicKey) -> Option<&TrustEntry> {
        let fingerprint = self.compute_fingerprint(public_key);
        self.entries.get(&fingerprint)
    }

    /// Update trust level for an existing key
    pub fn update_trust_level(&mut self, public_key: &PublicKey, trust_level: TrustLevel) -> bool {
        let fingerprint = self.compute_fingerprint(public_key);
        
        if let Some(entry) = self.entries.get_mut(&fingerprint) {
            entry.trust_level = trust_level;
            self.last_modified = chrono::Utc::now();
            true
        } else {
            false
        }
    }

    /// Clean up expired keys
    pub fn cleanup_expired_keys(&mut self) -> usize {
        let now = chrono::Utc::now();
        let initial_count = self.entries.len();
        
        self.entries.retain(|_, entry| {
            match entry.expires_at {
                Some(expires_at) => now <= expires_at,
                None => true, // Keep keys without expiration
            }
        });
        
        let removed_count = initial_count - self.entries.len();
        
        if removed_count > 0 {
            self.last_modified = now;
        }
        
        removed_count
    }

    /// Compute fingerprint for a public key
    fn compute_fingerprint(&self, public_key: &PublicKey) -> String {
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        hasher.update(public_key.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Add built-in trusted keys (for initial setup)
    pub fn add_builtin_keys(&mut self) {
        // Add Racing Wheel Suite official signing key
        // In production, this would be the actual public key
        let official_key = PublicKey::from_bytes([1u8; 32]); // Placeholder
        
        self.add_key(
            official_key,
            TrustLevel::HighlyTrusted,
            "Racing Wheel Suite Official".to_string(),
            vec![Operation::All],
            None, // No expiration
        );
        
        tracing::info!("Added built-in trusted keys");
    }

    /// Import keys from another trust store
    pub fn import_keys(&mut self, other: &TrustStore, overwrite: bool) -> usize {
        let mut imported_count = 0;
        
        for (fingerprint, entry) in &other.entries {
            if overwrite || !self.entries.contains_key(fingerprint) {
                self.entries.insert(fingerprint.clone(), entry.clone());
                imported_count += 1;
            }
        }
        
        if imported_count > 0 {
            self.last_modified = chrono::Utc::now();
        }
        
        imported_count
    }

    /// Export specific keys to a new trust store
    pub fn export_keys(&self, fingerprints: &[String]) -> TrustStore {
        let mut exported = TrustStore::new();
        
        for fingerprint in fingerprints {
            if let Some(entry) = self.entries.get(fingerprint) {
                exported.entries.insert(fingerprint.clone(), entry.clone());
            }
        }
        
        exported
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TrustStoreError {
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("Operation not allowed: {0}")]
    OperationNotAllowed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_trust_store_operations() {
        let mut store = TrustStore::new();
        let key = PublicKey::from_bytes([42u8; 32]);
        
        // Add key
        let fingerprint = store.add_key(
            key.clone(),
            TrustLevel::Trusted,
            "Test Key".to_string(),
            vec![Operation::SignUpdates],
            None,
        );
        
        // Check trust level
        assert_eq!(store.get_trust_level(&key), TrustLevel::Trusted);
        
        // Check operation permission
        assert!(store.is_operation_allowed(&key, &Operation::SignUpdates));
        assert!(!store.is_operation_allowed(&key, &Operation::SignFirmware));
        
        // Update trust level
        assert!(store.update_trust_level(&key, TrustLevel::HighlyTrusted));
        assert_eq!(store.get_trust_level(&key), TrustLevel::HighlyTrusted);
        
        // Remove key
        assert!(store.remove_key(&key));
        assert_eq!(store.get_trust_level(&key), TrustLevel::Unknown);
    }

    #[tokio::test]
    async fn test_trust_store_persistence() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        
        // Create and save trust store
        let mut store = TrustStore::new();
        let key = PublicKey::from_bytes([42u8; 32]);
        
        store.add_key(
            key.clone(),
            TrustLevel::Trusted,
            "Test Key".to_string(),
            vec![Operation::SignUpdates],
            None,
        );
        
        store.save(path).await.unwrap();
        
        // Load trust store
        let loaded_store = TrustStore::load(path).await.unwrap();
        
        assert_eq!(loaded_store.get_trust_level(&key), TrustLevel::Trusted);
        assert!(loaded_store.is_operation_allowed(&key, &Operation::SignUpdates));
    }

    #[test]
    fn test_key_expiration() {
        let mut store = TrustStore::new();
        let key = PublicKey::from_bytes([42u8; 32]);
        
        // Add expired key
        let past_time = chrono::Utc::now() - chrono::Duration::hours(1);
        store.add_key(
            key.clone(),
            TrustLevel::Trusted,
            "Expired Key".to_string(),
            vec![Operation::SignUpdates],
            Some(past_time),
        );
        
        // Should be untrusted due to expiration
        assert_eq!(store.get_trust_level(&key), TrustLevel::Untrusted);
        assert!(!store.is_operation_allowed(&key, &Operation::SignUpdates));
        
        // Clean up expired keys
        let removed = store.cleanup_expired_keys();
        assert_eq!(removed, 1);
    }
}