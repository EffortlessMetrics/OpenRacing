//! Trust store management for OpenRacing
//!
//! Manages trusted public keys and their trust levels for signature verification.
//!
//! The trust store provides:
//! - File-backed persistence with automatic save on modification
//! - In-memory mode for testing
//! - Protection for system keys (cannot be removed by user)
//! - Import/export functionality for key sharing

#![deny(clippy::unwrap_used)]

use crate::TrustLevel;
use crate::ed25519::{Ed25519Verifier, PublicKey};
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

        if store_path.exists() {
            store
                .load_from_file(&store_path)
                .context("Failed to load existing trust store")?;
        } else {
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

        let _ = store.initialize_default_keys();

        store
    }

    /// Load trust store from file
    fn load_from_file(&mut self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path).context("Failed to read trust store file")?;

        let entries: HashMap<String, TrustEntry> =
            serde_json::from_str(&content).context("Failed to parse trust store JSON")?;

        self.entries = entries;
        self.dirty = false;

        Ok(())
    }

    /// Save trust store to file
    pub fn save_to_file(&mut self) -> Result<()> {
        if let Some(ref path) = self.store_path
            && self.dirty
        {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .context("Failed to create trust store directory")?;
            }

            let content = serde_json::to_string_pretty(&self.entries)
                .context("Failed to serialize trust store")?;

            std::fs::write(path, content).context("Failed to write trust store file")?;

            self.dirty = false;
        }

        Ok(())
    }

    /// Initialize with default trusted keys
    fn initialize_default_keys(&mut self) -> Result<()> {
        let official_key = PublicKey {
            key_bytes: [0u8; 32],
            identifier: "openracing-official".to_string(),
            comment: Some("Official OpenRacing signing key".to_string()),
        };

        let fingerprint = Ed25519Verifier::get_key_fingerprint(&official_key);

        self.entries.insert(
            fingerprint,
            TrustEntry {
                public_key: official_key,
                trust_level: TrustLevel::Trusted,
                added_at: chrono::Utc::now(),
                reason: Some("Official project signing key".to_string()),
                user_modifiable: false,
            },
        );

        self.dirty = true;
        Ok(())
    }

    /// Add a public key to the trust store
    pub fn add_key(
        &mut self,
        public_key: PublicKey,
        trust_level: TrustLevel,
        reason: Option<String>,
    ) -> Result<()> {
        let fingerprint = Ed25519Verifier::get_key_fingerprint(&public_key);

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
        if let Some(entry) = self.entries.get(key_fingerprint)
            && !entry.user_modifiable
        {
            return Err(anyhow::anyhow!("Cannot remove system key"));
        }

        let removed = self.entries.remove(key_fingerprint).is_some();
        if removed {
            self.dirty = true;
        }

        Ok(removed)
    }

    /// Update trust level for a key
    pub fn update_trust_level(
        &mut self,
        key_fingerprint: &str,
        trust_level: TrustLevel,
        reason: Option<String>,
    ) -> Result<()> {
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
        self.entries
            .get(key_fingerprint)
            .map(|entry| entry.public_key.clone())
    }

    /// Get trust level for a key
    pub fn get_trust_level(&self, key_fingerprint: &str) -> TrustLevel {
        self.entries
            .get(key_fingerprint)
            .map(|entry| entry.trust_level)
            .unwrap_or(TrustLevel::Unknown)
    }

    /// Get a trust entry by fingerprint
    pub fn get_entry(&self, key_fingerprint: &str) -> Option<&TrustEntry> {
        self.entries.get(key_fingerprint)
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
    pub fn import_keys(
        &mut self,
        import_path: &Path,
        overwrite_existing: bool,
    ) -> Result<ImportResult> {
        let content = std::fs::read_to_string(import_path).context("Failed to read import file")?;

        let import_entries: HashMap<String, TrustEntry> =
            serde_json::from_str(&content).context("Failed to parse import file")?;

        let mut result = ImportResult::default();

        for (fingerprint, mut entry) in import_entries {
            entry.user_modifiable = true;
            entry.added_at = chrono::Utc::now();

            match self.entries.entry(fingerprint) {
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    if overwrite_existing {
                        e.insert(entry);
                        result.updated += 1;
                    } else {
                        result.skipped += 1;
                    }
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(entry);
                    result.imported += 1;
                }
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

        std::fs::write(export_path, content).context("Failed to write export file")?;

        Ok(export_entries.len())
    }

    /// Check if a key exists in the trust store
    pub fn contains_key(&self, key_fingerprint: &str) -> bool {
        self.entries.contains_key(key_fingerprint)
    }

    /// Get the number of entries in the trust store
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the trust store is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Drop for TrustStore {
    fn drop(&mut self) {
        let _ = self.save_to_file();
    }
}

/// Statistics about the trust store
#[derive(Debug, Default)]
pub struct TrustStoreStats {
    /// Number of trusted keys
    pub trusted_keys: usize,
    /// Number of unknown keys
    pub unknown_keys: usize,
    /// Number of distrusted keys
    pub distrusted_keys: usize,
    /// Number of system (non-user-modifiable) keys
    pub system_keys: usize,
}

/// Result of importing keys
#[derive(Debug, Default)]
pub struct ImportResult {
    /// Number of keys imported
    pub imported: usize,
    /// Number of keys updated
    pub updated: usize,
    /// Number of keys skipped (already existed)
    pub skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_trust_store() -> Result<()> {
        let mut store = TrustStore::new_in_memory();

        let stats = store.get_stats();
        assert!(stats.trusted_keys > 0);

        let test_key = PublicKey {
            key_bytes: [1u8; 32],
            identifier: "test-key".to_string(),
            comment: None,
        };

        store.add_key(
            test_key.clone(),
            TrustLevel::Trusted,
            Some("Test key".to_string()),
        )?;

        let fingerprint = Ed25519Verifier::get_key_fingerprint(&test_key);
        let retrieved = store.get_public_key(&fingerprint);
        assert!(retrieved.is_some());

        let trust_level = store.get_trust_level(&fingerprint);
        assert_eq!(trust_level, TrustLevel::Trusted);

        Ok(())
    }

    #[test]
    fn test_trust_level_update() -> Result<()> {
        let mut store = TrustStore::new_in_memory();

        let test_key = PublicKey {
            key_bytes: [2u8; 32],
            identifier: "test-key-2".to_string(),
            comment: None,
        };

        let fingerprint = Ed25519Verifier::get_key_fingerprint(&test_key);

        store.add_key(test_key, TrustLevel::Trusted, None)?;
        assert_eq!(store.get_trust_level(&fingerprint), TrustLevel::Trusted);

        store.update_trust_level(
            &fingerprint,
            TrustLevel::Distrusted,
            Some("Compromised".to_string()),
        )?;
        assert_eq!(store.get_trust_level(&fingerprint), TrustLevel::Distrusted);

        Ok(())
    }

    #[test]
    fn test_remove_key() -> Result<()> {
        let mut store = TrustStore::new_in_memory();

        let test_key = PublicKey {
            key_bytes: [3u8; 32],
            identifier: "test-key-3".to_string(),
            comment: None,
        };

        let fingerprint = Ed25519Verifier::get_key_fingerprint(&test_key);

        store.add_key(test_key, TrustLevel::Trusted, None)?;
        assert!(store.get_public_key(&fingerprint).is_some());

        let removed = store.remove_key(&fingerprint)?;
        assert!(removed);
        assert!(store.get_public_key(&fingerprint).is_none());

        let removed_again = store.remove_key(&fingerprint)?;
        assert!(!removed_again);

        Ok(())
    }

    #[test]
    fn test_cannot_remove_system_key() -> Result<()> {
        let store = TrustStore::new_in_memory();

        let system_keys: Vec<_> = store
            .list_keys()
            .into_iter()
            .filter(|(_, entry)| !entry.user_modifiable)
            .collect();

        assert!(
            !system_keys.is_empty(),
            "Should have at least one system key"
        );

        let (system_fingerprint, _) = &system_keys[0];

        let mut store = TrustStore::new_in_memory();
        let result = store.remove_key(system_fingerprint);

        assert!(result.is_err(), "Should not be able to remove system key");

        Ok(())
    }

    #[test]
    fn test_cannot_modify_system_key() -> Result<()> {
        let store = TrustStore::new_in_memory();

        let system_keys: Vec<_> = store
            .list_keys()
            .into_iter()
            .filter(|(_, entry)| !entry.user_modifiable)
            .collect();

        assert!(
            !system_keys.is_empty(),
            "Should have at least one system key"
        );

        let (system_fingerprint, _) = &system_keys[0];

        let mut store = TrustStore::new_in_memory();
        let result = store.update_trust_level(system_fingerprint, TrustLevel::Distrusted, None);

        assert!(result.is_err(), "Should not be able to modify system key");

        Ok(())
    }

    #[test]
    fn test_list_keys() -> Result<()> {
        let mut store = TrustStore::new_in_memory();

        for i in 0..3 {
            let test_key = PublicKey {
                key_bytes: [i + 10; 32],
                identifier: format!("test-key-{}", i),
                comment: None,
            };
            store.add_key(test_key, TrustLevel::Trusted, None)?;
        }

        let keys = store.list_keys();
        assert!(keys.len() >= 4);

        Ok(())
    }

    #[test]
    fn test_get_stats() -> Result<()> {
        let mut store = TrustStore::new_in_memory();

        let trusted_key = PublicKey {
            key_bytes: [20u8; 32],
            identifier: "trusted".to_string(),
            comment: None,
        };
        let unknown_key = PublicKey {
            key_bytes: [21u8; 32],
            identifier: "unknown".to_string(),
            comment: None,
        };
        let distrusted_key = PublicKey {
            key_bytes: [22u8; 32],
            identifier: "distrusted".to_string(),
            comment: None,
        };

        store.add_key(trusted_key, TrustLevel::Trusted, None)?;
        store.add_key(unknown_key, TrustLevel::Unknown, None)?;
        store.add_key(distrusted_key, TrustLevel::Distrusted, None)?;

        let stats = store.get_stats();
        assert!(stats.trusted_keys >= 2);
        assert!(stats.unknown_keys >= 1);
        assert!(stats.distrusted_keys >= 1);
        assert!(stats.system_keys >= 1);

        Ok(())
    }

    #[test]
    fn test_unknown_key_returns_unknown_trust_level() -> Result<()> {
        let store = TrustStore::new_in_memory();

        let trust_level = store.get_trust_level("nonexistent-fingerprint");
        assert_eq!(trust_level, TrustLevel::Unknown);

        Ok(())
    }

    #[test]
    fn test_update_nonexistent_key_fails() -> Result<()> {
        let mut store = TrustStore::new_in_memory();

        let result = store.update_trust_level("nonexistent", TrustLevel::Trusted, None);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_file_backed_store() -> Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let store_path = temp_dir.path().join("trust_store.json");

        {
            let mut store = TrustStore::new(store_path.clone())?;

            let test_key = PublicKey {
                key_bytes: [30u8; 32],
                identifier: "persistent-key".to_string(),
                comment: None,
            };

            store.add_key(
                test_key,
                TrustLevel::Trusted,
                Some("Persistent".to_string()),
            )?;
            store.save_to_file()?;
        }

        {
            let store = TrustStore::new(store_path)?;

            let test_key = PublicKey {
                key_bytes: [30u8; 32],
                identifier: "persistent-key".to_string(),
                comment: None,
            };
            let fingerprint = Ed25519Verifier::get_key_fingerprint(&test_key);

            let retrieved = store.get_public_key(&fingerprint);
            assert!(retrieved.is_some(), "Key should persist after reload");

            let trust_level = store.get_trust_level(&fingerprint);
            assert_eq!(trust_level, TrustLevel::Trusted);
        }

        Ok(())
    }

    #[test]
    fn test_import_export_keys() -> Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let export_path = temp_dir.path().join("exported_keys.json");

        let mut source_store = TrustStore::new_in_memory();
        let test_key = PublicKey {
            key_bytes: [40u8; 32],
            identifier: "export-test-key".to_string(),
            comment: None,
        };
        let fingerprint = Ed25519Verifier::get_key_fingerprint(&test_key);
        source_store.add_key(
            test_key,
            TrustLevel::Trusted,
            Some("For export".to_string()),
        )?;

        let exported_count = source_store.export_keys(&export_path, false)?;
        assert!(exported_count >= 1);

        let mut dest_store = TrustStore::new_in_memory();
        let import_result = dest_store.import_keys(&export_path, false)?;
        assert!(import_result.imported >= 1);

        let retrieved = dest_store.get_public_key(&fingerprint);
        assert!(retrieved.is_some(), "Imported key should be present");

        Ok(())
    }

    #[test]
    fn test_import_with_overwrite() -> Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let export_path = temp_dir.path().join("keys_to_import.json");

        let mut source_store = TrustStore::new_in_memory();
        let test_key = PublicKey {
            key_bytes: [50u8; 32],
            identifier: "overwrite-test-key".to_string(),
            comment: None,
        };
        let fingerprint = Ed25519Verifier::get_key_fingerprint(&test_key);
        source_store.add_key(
            test_key.clone(),
            TrustLevel::Trusted,
            Some("Original".to_string()),
        )?;
        source_store.export_keys(&export_path, false)?;

        let mut dest_store = TrustStore::new_in_memory();
        dest_store.add_key(
            test_key,
            TrustLevel::Distrusted,
            Some("Existing".to_string()),
        )?;

        let result_no_overwrite = dest_store.import_keys(&export_path, false)?;
        assert_eq!(result_no_overwrite.skipped, 1);
        assert_eq!(
            dest_store.get_trust_level(&fingerprint),
            TrustLevel::Distrusted
        );

        let result_overwrite = dest_store.import_keys(&export_path, true)?;
        assert_eq!(result_overwrite.updated, 1);
        assert_eq!(
            dest_store.get_trust_level(&fingerprint),
            TrustLevel::Trusted
        );

        Ok(())
    }

    #[test]
    fn test_export_with_system_keys() -> Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let export_with_system = temp_dir.path().join("with_system.json");
        let export_without_system = temp_dir.path().join("without_system.json");

        let store = TrustStore::new_in_memory();

        let count_with = store.export_keys(&export_with_system, true)?;
        let count_without = store.export_keys(&export_without_system, false)?;

        assert!(count_with >= count_without);

        Ok(())
    }
}
