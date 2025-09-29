//! Trust store management for Racing Wheel Suite
//! 
//! Manages trusted public keys and their trust levels

use super::{TrustLevel, ed25519::PublicKey};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Trust store entry for a public key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEntry {
    /// The public key
    pub public_key: PublicKey,
    
    /// Trust level for this key
    pub trust_level: TrustLevel,
    
    /// When this entry was added
    pub added_at: chrono::DateTime<chrono::Utc>,
    
    /// Optional reason for trust/distrust
    pub reason: Option<String>,
    
    /// Whether this entry can be modified by the user
    pub user_modifiable: bool,
}

/// Trust store for managing public keys and their trust levels
pub struct TrustStore {
    /// Map from key fingerprint to trust entry
    entries: HashMap<String, TrustEntry>,
    
    /// Path to the trust store file (None for in-memory)
    store_path: Option<PathBuf>,
    
    /// Whether the store has been modified since last save
    dirty: bool,
}

impl TrustStore {
    /// Create a new trust store backed by a file
    pub fn new(store_path: PathBuf) -> Result<Self> {
        let mut store = Self {
            entries: HashMap::new(),
            store_path: Some(store_path.clone()),
            dirty: false,
        };
        
        // Load existing trust store if it exists
        if store_path.exists() {
            store.load_from_file(&store_path)
                .context("Failed to load existing trust store")?;
        } else {
            // Initialize with default trusted keys
            store.initialize_default_keys()?;
            store.save_to_file()?;
        }
        
        Ok(store)
    }
    
    /// Create a new in-memory trust store (for testing)
    pub fn new_in_memory() -> Self {
        let mut store = Self {
            entries: HashMap::new(),
            store_path: None,
            dirty: false,
        };
        
        // Initialize with default keys
        let _ = store.initialize_default_keys();
        
        store
    }
    
    /// Load trust store from file
    fn load_from_file(&mut self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path)
            .context("Failed to read trust store file")?;
        
        let entries: HashMap<String, TrustEntry> = serde_json::from_str(&content)
            .context("Failed to parse trust store JSON")?;
        
        self.entries = entries;
        self.dirty = false;
        
        Ok(())
    }
    
    /// Save trust store to file
    pub fn save_to_file(&mut self) -> Result<()> {
        if let Some(ref path) = self.store_path {
            if self.dirty {
                // Create parent directory if it doesn't exist
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .context("Failed to create trust store directory")?;
                }
                
                let content = serde_json::to_string_pretty(&self.entries)
                    .context("Failed to serialize trust store")?;
                
                std::fs::write(path, content)
                    .context("Failed to write trust store file")?;
                
                self.dirty = false;
            }
        }
        
        Ok(())
    }
    
    /// Initialize with default trusted keys
    fn initialize_default_keys(&mut self) -> Result<()> {
        // Add Racing Wheel Suite official signing key
        // In a real implementation, this would be the actual public key
        let official_key = PublicKey {
            key_bytes: [0u8; 32], // Placeholder - would be real key
            identifier: "racing-wheel-suite-official".to_string(),
            comment: Some("Official Racing Wheel Suite signing key".to_string()),
        };
        
        let fingerprint = super::ed25519::Ed25519Verifier::get_key_fingerprint(&official_key);
        
        self.entries.insert(fingerprint, TrustEntry {
            public_key: official_key,
            trust_level: TrustLevel::Trusted,
            added_at: chrono::Utc::now(),
            reason: Some("Official project signing key".to_string()),
            user_modifiable: false, // System key, cannot be removed by user
        });
        
        self.dirty = true;
        Ok(())
    }
    
    /// Add a public key to the trust store
    pub fn add_key(&mut self, public_key: PublicKey, trust_level: TrustLevel, reason: Option<String>) -> Result<()> {
        let fingerprint = super::ed25519::Ed25519Verifier::get_key_fingerprint(&public_key);
        
        let entry = TrustEntry {
            public_key,
            trust_level,
            added_at: chrono::Utc::now(),
            reason,
            user_modifiable: true,
        };
        
        self.entries.insert(fingerprint, entry);
        self.dirty = true;
        
        Ok(())
    }
    
    /// Remove a public key from the trust store
    pub fn remove_key(&mut self, key_fingerprint: &str) -> Result<bool> {
        if let Some(entry) = self.entries.get(key_fingerprint) {
            if !entry.user_modifiable {
                return Err(anyhow::anyhow!("Cannot remove system key"));
            }
        }
        
        let removed = self.entries.remove(key_fingerprint).is_some();
        if removed {
            self.dirty = true;
        }
        
        Ok(removed)
    }
    
    /// Update trust level for a key
    pub fn update_trust_level(&mut self, key_fingerprint: &str, trust_level: TrustLevel, reason: Option<String>) -> Result<()> {
        if let Some(entry) = self.entries.get_mut(key_fingerprint) {
            if !entry.user_modifiable {
                return Err(anyhow::anyhow!("Cannot modify system key"));
            }
            
            entry.trust_level = trust_level;
            entry.reason = reason;
            self.dirty = true;
            
            Ok(())
        } else {
            Err(anyhow::anyhow!("Key not found in trust store"))
        }
    }
    
    /// Get a public key by fingerprint
    pub fn get_public_key(&self, key_fingerprint: &str) -> Option<PublicKey> {
        self.entries.get(key_fingerprint).map(|entry| entry.public_key.clone())
    }
    
    /// Get trust level for a key
    pub fn get_trust_level(&self, key_fingerprint: &str) -> TrustLevel {
        self.entries
            .get(key_fingerprint)
            .map(|entry| entry.trust_level)
            .unwrap_or(TrustLevel::Unknown)
    }
    
    /// List all keys in the trust store
    pub fn list_keys(&self) -> Vec<(String, &TrustEntry)> {
        self.entries
            .iter()
            .map(|(fingerprint, entry)| (fingerprint.clone(), entry))
            .collect()
    }
    
    /// Get trust store statistics
    pub fn get_stats(&self) -> TrustStoreStats {
        let mut stats = TrustStoreStats::default();
        
        for entry in self.entries.values() {
            match entry.trust_level {
                TrustLevel::Trusted => stats.trusted_keys += 1,
                TrustLevel::Unknown => stats.unknown_keys += 1,
                TrustLevel::Distrusted => stats.distrusted_keys += 1,
            }
            
            if !entry.user_modifiable {
                stats.system_keys += 1;
            }
        }
        
        stats
    }
    
    /// Import keys from another trust store file
    pub fn import_keys(&mut self, import_path: &Path, overwrite_existing: bool) -> Result<ImportResult> {
        let content = std::fs::read_to_string(import_path)
            .context("Failed to read import file")?;
        
        let import_entries: HashMap<String, TrustEntry> = serde_json::from_str(&content)
            .context("Failed to parse import file")?;
        
        let mut result = ImportResult::default();
        
        for (fingerprint, mut entry) in import_entries {
            // Mark imported keys as user-modifiable
            entry.user_modifiable = true;
            entry.added_at = chrono::Utc::now();
            
            if self.entries.contains_key(&fingerprint) {
                if overwrite_existing {
                    self.entries.insert(fingerprint, entry);
                    result.updated += 1;
                } else {
                    result.skipped += 1;
                }
            } else {
                self.entries.insert(fingerprint, entry);
                result.imported += 1;
            }
        }
        
        if result.imported > 0 || result.updated > 0 {
            self.dirty = true;
        }
        
        Ok(result)
    }
    
    /// Export keys to a file
    pub fn export_keys(&self, export_path: &Path, include_system_keys: bool) -> Result<usize> {
        let mut export_entries = HashMap::new();
        
        for (fingerprint, entry) in &self.entries {
            if include_system_keys || entry.user_modifiable {
                export_entries.insert(fingerprint.clone(), entry.clone());
            }
        }
        
        let content = serde_json::to_string_pretty(&export_entries)
            .context("Failed to serialize export data")?;
        
        std::fs::write(export_path, content)
            .context("Failed to write export file")?;
        
        Ok(export_entries.len())
    }
}

/// Statistics about the trust store
#[derive(Debug, Default)]
pub struct TrustStoreStats {
    pub trusted_keys: usize,
    pub unknown_keys: usize,
    pub distrusted_keys: usize,
    pub system_keys: usize,
}

/// Result of importing keys
#[derive(Debug, Default)]
pub struct ImportResult {
    pub imported: usize,
    pub updated: usize,
    pub skipped: usize,
}

impl Drop for TrustStore {
    fn drop(&mut self) {
        // Auto-save on drop if dirty
        let _ = self.save_to_file();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_in_memory_trust_store() {
        let mut store = TrustStore::new_in_memory();
        
        // Should have default keys
        let stats = store.get_stats();
        assert!(stats.trusted_keys > 0);
        
        // Test adding a key
        let test_key = PublicKey {
            key_bytes: [1u8; 32],
            identifier: "test-key".to_string(),
            comment: None,
        };
        
        let result = store.add_key(test_key.clone(), TrustLevel::Trusted, Some("Test key".to_string()));
        assert!(result.is_ok());
        
        // Test retrieving the key
        let fingerprint = super::super::ed25519::Ed25519Verifier::get_key_fingerprint(&test_key);
        let retrieved = store.get_public_key(&fingerprint);
        assert!(retrieved.is_some());
        
        let trust_level = store.get_trust_level(&fingerprint);
        assert_eq!(trust_level, TrustLevel::Trusted);
    }
    
    #[test]
    fn test_trust_level_update() {
        let mut store = TrustStore::new_in_memory();
        
        let test_key = PublicKey {
            key_bytes: [2u8; 32],
            identifier: "test-key-2".to_string(),
            comment: None,
        };
        
        let fingerprint = super::super::ed25519::Ed25519Verifier::get_key_fingerprint(&test_key);
        
        // Add key as trusted
        store.add_key(test_key, TrustLevel::Trusted, None).unwrap();
        assert_eq!(store.get_trust_level(&fingerprint), TrustLevel::Trusted);
        
        // Update to distrusted
        store.update_trust_level(&fingerprint, TrustLevel::Distrusted, Some("Compromised".to_string())).unwrap();
        assert_eq!(store.get_trust_level(&fingerprint), TrustLevel::Distrusted);
    }
}