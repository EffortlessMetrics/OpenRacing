//! Integration tests for openracing-crypto
//!
//! These tests verify end-to-end cryptographic operations.

use openracing_crypto::prelude::*;
use openracing_crypto::trust_store::TrustStore;
use openracing_crypto::verification::verification_utils;
use tempfile::TempDir;

mod sign_verify_roundtrip {
    use super::*;

    #[test]
    fn test_full_sign_verify_cycle() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let content = b"Test content for signing";

        let metadata = Ed25519Signer::sign_with_metadata(
            content,
            &keypair,
            "Test Signer",
            ContentType::Plugin,
            Some("Test signature".to_string()),
        )?;

        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

        let verifier = Ed25519Verifier::new(trust_store);
        let result = verifier.verify_content(content, &metadata)?;

        assert!(result.signature_valid);
        assert_eq!(result.trust_level, TrustLevel::Trusted);
        assert_eq!(result.metadata.signer, "Test Signer");

        Ok(())
    }

    #[test]
    fn test_detached_signature_file() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.wasm");
        let content = b"(module (func))";
        std::fs::write(&file_path, content)?;

        let keypair = KeyPair::generate()?;
        let _metadata = Ed25519Signer::sign_file(
            &file_path,
            &keypair,
            "Plugin Author",
            ContentType::Plugin,
            None,
        )?;

        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

        let verifier = Ed25519Verifier::new(trust_store);
        let result = verifier.verify_file(&file_path)?;

        assert!(result.signature_valid);
        assert_eq!(result.metadata.signer, "Plugin Author");

        Ok(())
    }

    #[test]
    fn test_tampered_content_detection() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let original_content = b"Original content";
        let tampered_content = b"Tampered content";

        let metadata = Ed25519Signer::sign_with_metadata(
            original_content,
            &keypair,
            "Test Signer",
            ContentType::Binary,
            None,
        )?;

        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

        let verifier = Ed25519Verifier::new(trust_store);
        let result = verifier.verify_content(tampered_content, &metadata)?;

        assert!(!result.signature_valid);

        Ok(())
    }
}

mod trust_store_operations {
    use super::*;

    #[test]
    fn test_persist_and_reload() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let store_path = temp_dir.path().join("trust_store.json");

        let fingerprint = {
            let mut store = TrustStore::new(store_path.clone())?;

            let key = PublicKey {
                key_bytes: [42u8; 32],
                identifier: "test-key".to_string(),
                comment: Some("Test key".to_string()),
            };
            let fp = key.fingerprint();

            store.add_key(key, TrustLevel::Trusted, Some("Added in test".to_string()))?;
            store.save_to_file()?;

            fp
        };

        let reloaded_store = TrustStore::new(store_path)?;
        assert!(reloaded_store.contains_key(&fingerprint));
        assert_eq!(
            reloaded_store.get_trust_level(&fingerprint),
            TrustLevel::Trusted
        );

        Ok(())
    }

    #[test]
    fn test_import_export() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;

        let mut source_store = TrustStore::new_in_memory();
        let key = PublicKey {
            key_bytes: [1u8; 32],
            identifier: "export-key".to_string(),
            comment: None,
        };
        let fingerprint = key.fingerprint();
        source_store.add_key(key, TrustLevel::Trusted, None)?;

        let export_path = temp_dir.path().join("exported.json");
        let export_count = source_store.export_keys(&export_path, false)?;
        assert!(export_count >= 1);

        let mut dest_store = TrustStore::new_in_memory();
        let result = dest_store.import_keys(&export_path, false)?;
        assert!(result.imported >= 1);

        assert!(dest_store.contains_key(&fingerprint));

        Ok(())
    }

    #[test]
    fn test_system_key_protection() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = TrustStore::new_in_memory();

        let system_keys: Vec<_> = store
            .list_keys()
            .iter()
            .filter(|(_, e)| !e.user_modifiable)
            .map(|(fp, _)| fp.clone())
            .collect();

        assert!(
            !system_keys.is_empty(),
            "Should have at least one system key"
        );

        for fp in system_keys {
            assert!(store.remove_key(&fp).is_err());
        }

        Ok(())
    }
}

mod verification_service {
    use super::*;

    #[test]
    fn test_service_initialization() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let config = VerificationConfig {
            trust_store_path: temp_dir.path().join("trust_store.json"),
            require_binary_signatures: true,
            require_firmware_signatures: true,
            require_plugin_signatures: true,
            allow_unknown_signers: false,
            max_signature_age_seconds: Some(365 * 24 * 3600),
        };

        let service = VerificationService::new(config.clone())?;

        assert!(service.get_config().require_binary_signatures);
        assert!(!service.get_config().allow_unknown_signers);

        Ok(())
    }

    #[test]
    fn test_verify_signed_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let plugin_path = temp_dir.path().join("plugin.wasm");
        let content = b"(module)";
        std::fs::write(&plugin_path, content)?;

        let keypair = KeyPair::generate()?;
        let metadata = Ed25519Signer::sign_with_metadata(
            content,
            &keypair,
            "Plugin Dev",
            ContentType::Plugin,
            None,
        )?;
        openracing_crypto::utils::create_detached_signature(&plugin_path, &metadata)?;

        let trust_store_path = temp_dir.path().join("trust_store.json");
        let mut trust_store = TrustStore::new(trust_store_path.clone())?;
        trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;
        trust_store.save_to_file()?;

        let service = VerificationService::new(VerificationConfig {
            trust_store_path,
            ..Default::default()
        })?;

        let result = service.verify_plugin(&plugin_path)?;
        assert!(result.signature_valid);

        Ok(())
    }

    #[test]
    fn test_content_type_detection() {
        assert!(matches!(
            verification_utils::detect_content_type(std::path::Path::new("wheeld.exe")),
            Some(ContentType::Binary)
        ));

        assert!(matches!(
            verification_utils::detect_content_type(std::path::Path::new("firmware.fw")),
            Some(ContentType::Firmware)
        ));

        assert!(matches!(
            verification_utils::detect_content_type(std::path::Path::new("plugin.wasm")),
            Some(ContentType::Plugin)
        ));

        assert!(
            verification_utils::detect_content_type(std::path::Path::new("random.json")).is_none()
        );

        assert!(matches!(
            verification_utils::detect_content_type(std::path::Path::new("profile.json")),
            Some(ContentType::Profile)
        ));

        assert!(matches!(
            verification_utils::detect_content_type(std::path::Path::new("car.profile.json")),
            Some(ContentType::Profile)
        ));

        assert!(matches!(
            verification_utils::detect_content_type(std::path::Path::new("update.wup")),
            Some(ContentType::Update)
        ));
    }
}

mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_content() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let empty_content: &[u8] = &[];

        let metadata = Ed25519Signer::sign_with_metadata(
            empty_content,
            &keypair,
            "Test",
            ContentType::Binary,
            None,
        )?;

        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

        let verifier = Ed25519Verifier::new(trust_store);
        let result = verifier.verify_content(empty_content, &metadata)?;

        assert!(result.signature_valid);

        Ok(())
    }

    #[test]
    fn test_large_content() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let large_content = vec![0xABu8; 1024 * 1024]; // 1MB

        let metadata = Ed25519Signer::sign_with_metadata(
            &large_content,
            &keypair,
            "Test",
            ContentType::Firmware,
            None,
        )?;

        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

        let verifier = Ed25519Verifier::new(trust_store);
        let result = verifier.verify_content(&large_content, &metadata)?;

        assert!(result.signature_valid);

        Ok(())
    }

    #[test]
    fn test_multiple_signatures_same_content() -> Result<(), Box<dyn std::error::Error>> {
        let keypair1 = KeyPair::generate()?;
        let keypair2 = KeyPair::generate()?;
        let content = b"Content signed by multiple keys";

        let metadata1 = Ed25519Signer::sign_with_metadata(
            content,
            &keypair1,
            "Signer 1",
            ContentType::Binary,
            None,
        )?;

        let metadata2 = Ed25519Signer::sign_with_metadata(
            content,
            &keypair2,
            "Signer 2",
            ContentType::Binary,
            None,
        )?;

        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(keypair1.public_key.clone(), TrustLevel::Trusted, None)?;
        trust_store.add_key(keypair2.public_key.clone(), TrustLevel::Trusted, None)?;

        let verifier = Ed25519Verifier::new(trust_store);

        let result1 = verifier.verify_content(content, &metadata1)?;
        let result2 = verifier.verify_content(content, &metadata2)?;

        assert!(result1.signature_valid);
        assert!(result2.signature_valid);
        assert_ne!(
            result1.metadata.key_fingerprint,
            result2.metadata.key_fingerprint
        );

        Ok(())
    }

    #[test]
    fn test_distrusted_key_behavior() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let content = b"Content from distrusted key";

        let metadata = Ed25519Signer::sign_with_metadata(
            content,
            &keypair,
            "Distrusted Signer",
            ContentType::Plugin,
            None,
        )?;

        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Distrusted,
            Some("Key was compromised".to_string()),
        )?;

        let verifier = Ed25519Verifier::new(trust_store);
        let result = verifier.verify_content(content, &metadata)?;

        assert!(
            result.signature_valid,
            "Signature should be cryptographically valid"
        );
        assert_eq!(result.trust_level, TrustLevel::Distrusted);
        assert!(
            result.warnings.iter().any(|w| w.contains("distrusted")),
            "Should have warning about distrusted key"
        );

        Ok(())
    }
}
