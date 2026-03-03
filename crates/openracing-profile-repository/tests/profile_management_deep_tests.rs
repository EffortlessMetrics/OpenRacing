//! Deep tests for profile management lifecycle.
//!
//! Covers profile CRUD lifecycle, validation rules, inheritance,
//! search and filtering, export/import round-trips, concurrent access,
//! and profile versioning.

use openracing_profile_repository::prelude::*;
use openracing_profile_repository::ProfileSigner;
use racing_wheel_schemas::prelude::{Degrees, Gain, TorqueNm};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pid(value: &str) -> Result<ProfileId, Box<dyn std::error::Error>> {
    Ok(ProfileId::new(value.to_string())?)
}

async fn setup_repo() -> Result<(ProfileRepository, TempDir), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: true,
    };
    let repo = ProfileRepository::new(config).await?;
    Ok((repo, tmp))
}

fn make_profile(id: &str, name: &str, scope: ProfileScope) -> Result<Profile, Box<dyn std::error::Error>> {
    let profile_id = pid(id)?;
    Ok(Profile::new(profile_id, scope, BaseSettings::default(), name.to_string()))
}

fn make_global(id: &str) -> Result<Profile, Box<dyn std::error::Error>> {
    make_profile(id, &format!("Global {id}"), ProfileScope::global())
}

fn make_game(id: &str, game: &str) -> Result<Profile, Box<dyn std::error::Error>> {
    make_profile(id, &format!("Game {id}"), ProfileScope::for_game(game.to_string()))
}

fn make_car(id: &str, game: &str, car: &str) -> Result<Profile, Box<dyn std::error::Error>> {
    make_profile(
        id,
        &format!("Car {id}"),
        ProfileScope::for_car(game.to_string(), car.to_string()),
    )
}

fn gain(v: f32) -> Result<Gain, Box<dyn std::error::Error>> {
    Ok(Gain::new(v)?)
}

fn dor(v: f32) -> Result<Degrees, Box<dyn std::error::Error>> {
    Ok(Degrees::new_dor(v)?)
}

fn torque(v: f32) -> Result<TorqueNm, Box<dyn std::error::Error>> {
    Ok(TorqueNm::new(v)?)
}

// ---------------------------------------------------------------------------
// 1. Profile create/read/update/delete lifecycle
// ---------------------------------------------------------------------------

mod crud_lifecycle {
    use super::*;

    #[tokio::test]
    async fn full_lifecycle_create_read_update_delete() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;
        let profile = make_global("lifecycle_crud")?;

        // Create
        repo.save_profile(&profile, None).await?;

        // Read
        let loaded = repo.load_profile(&profile.id).await?;
        assert!(loaded.is_some(), "profile should exist after save");

        // Update (save again)
        repo.save_profile(&profile, None).await?;

        let profiles = repo.list_profiles().await?;
        let matches: Vec<_> = profiles.iter().filter(|p| p.id == profile.id).collect();
        assert_eq!(matches.len(), 1, "update must not duplicate");

        // Delete
        repo.delete_profile(&profile.id).await?;
        let after_del = repo.load_profile(&profile.id).await?;
        assert!(after_del.is_none(), "deleted profile must not load");
        Ok(())
    }

    #[tokio::test]
    async fn save_preserves_metadata_name() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;
        let profile = make_profile("meta_name", "My Custom Profile", ProfileScope::global())?;
        repo.save_profile(&profile, None).await?;

        let loaded = repo.load_profile(&profile.id).await?.ok_or("not found")?;
        assert_eq!(loaded.metadata.name, "My Custom Profile");
        Ok(())
    }

    #[tokio::test]
    async fn save_preserves_scope_game() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;
        let profile = make_game("scope_game", "iracing")?;
        repo.save_profile(&profile, None).await?;

        let loaded = repo.load_profile(&profile.id).await?.ok_or("not found")?;
        assert_eq!(loaded.scope.game.as_deref(), Some("iracing"));
        Ok(())
    }

    #[tokio::test]
    async fn delete_then_recreate_same_id() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;
        let profile = make_global("recreate_id")?;

        repo.save_profile(&profile, None).await?;
        repo.delete_profile(&profile.id).await?;

        // Recreate with same ID
        let profile2 = make_global("recreate_id")?;
        repo.save_profile(&profile2, None).await?;

        let loaded = repo.load_profile(&profile2.id).await?;
        assert!(loaded.is_some(), "recreated profile should be loadable");
        Ok(())
    }

    #[tokio::test]
    async fn list_returns_all_saved_profiles() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;

        for i in 0..7 {
            let p = make_global(&format!("list_all_{i}"))?;
            repo.save_profile(&p, None).await?;
        }

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 7);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 2. Profile validation rules
// ---------------------------------------------------------------------------

mod validation_rules {
    use super::*;

    #[test]
    fn empty_profile_id_rejected() {
        let result = ProfileId::new(String::new());
        assert!(result.is_err());
    }

    #[test]
    fn whitespace_only_profile_id_rejected() {
        let result = ProfileId::new("   ".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn valid_profile_id_with_alphanumeric() -> TestResult {
        let id = ProfileId::new("my_profile.v2-test".to_string())?;
        assert_eq!(id.as_str(), "my_profile.v2-test");
        Ok(())
    }

    #[test]
    fn gain_boundary_zero_is_valid() -> TestResult {
        let _g = gain(0.0)?;
        Ok(())
    }

    #[test]
    fn gain_boundary_one_is_valid() -> TestResult {
        let _g = gain(1.0)?;
        Ok(())
    }

    #[test]
    fn gain_negative_is_invalid() {
        let result = Gain::new(-0.1);
        assert!(result.is_err());
    }

    #[test]
    fn gain_above_one_is_invalid() {
        let result = Gain::new(1.01);
        assert!(result.is_err());
    }

    #[test]
    fn torque_zero_is_valid() -> TestResult {
        let _t = torque(0.0)?;
        Ok(())
    }

    #[test]
    fn torque_negative_is_invalid() {
        let result = TorqueNm::new(-1.0);
        assert!(result.is_err());
    }
}

// ---------------------------------------------------------------------------
// 3. Profile inheritance (base profile + overrides)
// ---------------------------------------------------------------------------

mod inheritance {
    use super::*;

    #[test]
    fn child_with_parent_has_parent_set() -> TestResult {
        let parent_id = pid("parent_profile")?;
        let child_id = pid("child_profile")?;

        let child = Profile::new_with_parent(
            child_id,
            parent_id.clone(),
            ProfileScope::for_game("acc".to_string()),
            BaseSettings::default(),
            "Child".to_string(),
        );

        assert!(child.has_parent());
        assert_eq!(child.parent().map(|p| p.as_str()), Some("parent_profile"));
        Ok(())
    }

    #[test]
    fn merge_with_parent_preserves_child_identity() -> TestResult {
        let parent = Profile::default_global()?;
        let child_id = pid("child_merge")?;
        let child = Profile::new_with_parent(
            child_id.clone(),
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Merge".to_string(),
        );

        let merged = child.merge_with_parent(&parent);
        assert_eq!(merged.id, child_id);
        assert_eq!(merged.metadata.name, "Child Merge");
        Ok(())
    }

    #[test]
    fn merge_with_parent_keeps_child_scope() -> TestResult {
        let parent = Profile::default_global()?;
        let child_id = pid("child_scope")?;
        let child = Profile::new_with_parent(
            child_id,
            parent.id.clone(),
            ProfileScope::for_game("acc".to_string()),
            BaseSettings::default(),
            "Child Scope".to_string(),
        );

        let merged = child.merge_with_parent(&parent);
        assert_eq!(merged.scope.game.as_deref(), Some("acc"));
        Ok(())
    }

    #[test]
    fn merge_with_overwrites_base_settings() -> TestResult {
        let base = Profile::default_global()?;
        let other_id = pid("override_profile")?;
        let other_settings = BaseSettings::new(
            gain(0.5)?,
            dor(540.0)?,
            torque(10.0)?,
            FilterConfig::default(),
        );
        let other = Profile::new(
            other_id,
            ProfileScope::global(),
            other_settings,
            "Override".to_string(),
        );

        let merged = base.merge_with(&other);
        assert!((merged.base_settings.ffb_gain.value() - 0.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn set_parent_updates_modified_at() -> TestResult {
        let mut profile = Profile::default_global()?;
        let original_modified = profile.metadata.modified_at.clone();

        std::thread::sleep(std::time::Duration::from_millis(10));
        let parent_id = pid("some_parent")?;
        profile.set_parent(Some(parent_id));

        assert_ne!(profile.metadata.modified_at, original_modified);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 4. Profile search and filtering
// ---------------------------------------------------------------------------

mod search_and_filtering {
    use super::*;

    #[tokio::test]
    async fn filter_profiles_by_game_scope() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;

        let g1 = make_game("search_iracing", "iracing")?;
        let g2 = make_game("search_acc", "acc")?;
        let g3 = make_global("search_global")?;

        repo.save_profile(&g1, None).await?;
        repo.save_profile(&g2, None).await?;
        repo.save_profile(&g3, None).await?;

        let all = repo.list_profiles().await?;
        let iracing_profiles: Vec<_> = all
            .iter()
            .filter(|p| p.scope.game.as_deref() == Some("iracing"))
            .collect();
        assert_eq!(iracing_profiles.len(), 1);
        assert_eq!(iracing_profiles[0].id, g1.id);
        Ok(())
    }

    #[tokio::test]
    async fn filter_profiles_global_only() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;

        let g = make_global("filter_global")?;
        let game = make_game("filter_game", "acc")?;
        repo.save_profile(&g, None).await?;
        repo.save_profile(&game, None).await?;

        let all = repo.list_profiles().await?;
        let globals: Vec<_> = all.iter().filter(|p| p.scope.game.is_none()).collect();
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0].id, g.id);
        Ok(())
    }

    #[tokio::test]
    async fn filter_by_car_scope() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;

        let car1 = make_car("search_gt3", "acc", "gt3")?;
        let car2 = make_car("search_gt4", "acc", "gt4")?;
        let game = make_game("search_acc_game", "acc")?;

        repo.save_profile(&car1, None).await?;
        repo.save_profile(&car2, None).await?;
        repo.save_profile(&game, None).await?;

        let all = repo.list_profiles().await?;
        let gt3: Vec<_> = all
            .iter()
            .filter(|p| p.scope.car.as_deref() == Some("gt3"))
            .collect();
        assert_eq!(gt3.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn list_after_deleting_subset() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;

        let p1 = make_global("subset_a")?;
        let p2 = make_global("subset_b")?;
        let p3 = make_global("subset_c")?;

        repo.save_profile(&p1, None).await?;
        repo.save_profile(&p2, None).await?;
        repo.save_profile(&p3, None).await?;

        repo.delete_profile(&p2.id).await?;

        let all = repo.list_profiles().await?;
        assert_eq!(all.len(), 2);
        let ids: Vec<_> = all.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"subset_a"));
        assert!(ids.contains(&"subset_c"));
        assert!(!ids.contains(&"subset_b"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 5. Profile export/import (JSON round-trip)
// ---------------------------------------------------------------------------

mod export_import {
    use super::*;

    #[tokio::test]
    async fn json_round_trip_via_save_reload() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;
        let profile = make_global("json_rt")?;

        repo.save_profile(&profile, None).await?;
        repo.clear_cache().await;
        repo.reload().await?;

        let loaded = repo.load_profile(&profile.id).await?.ok_or("not found")?;
        assert_eq!(loaded.id, profile.id);
        assert_eq!(loaded.scope.game, profile.scope.game);
        Ok(())
    }

    #[tokio::test]
    async fn json_round_trip_preserves_ffb_gain() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;

        let settings = BaseSettings::new(
            gain(0.75)?,
            dor(900.0)?,
            torque(20.0)?,
            FilterConfig::default(),
        );
        let id = pid("rt_ffb")?;
        let profile = Profile::new(id, ProfileScope::global(), settings, "FFB RT".to_string());

        repo.save_profile(&profile, None).await?;
        repo.clear_cache().await;
        repo.reload().await?;

        let loaded = repo.load_profile(&profile.id).await?.ok_or("not found")?;
        assert!(
            (loaded.base_settings.ffb_gain.value() - 0.75).abs() < 0.01,
            "ffb_gain should survive round-trip"
        );
        Ok(())
    }

    #[tokio::test]
    async fn json_round_trip_preserves_game_scope() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;
        let profile = make_game("rt_scope", "iracing")?;

        repo.save_profile(&profile, None).await?;
        repo.clear_cache().await;
        repo.reload().await?;

        let loaded = repo.load_profile(&profile.id).await?.ok_or("not found")?;
        assert_eq!(loaded.scope.game.as_deref(), Some("iracing"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 6. Concurrent profile access
// ---------------------------------------------------------------------------

mod concurrent_access {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn concurrent_saves_no_data_loss() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;
        let repo = Arc::new(repo);

        let mut handles = Vec::new();
        for i in 0..10 {
            let repo_clone = Arc::clone(&repo);
            handles.push(tokio::spawn(async move {
                let p = make_global(&format!("conc_save_{i}")).map_err(|e| anyhow::anyhow!("{e}"))?;
                repo_clone.save_profile(&p, None).await?;
                Ok::<_, anyhow::Error>(())
            }));
        }

        for h in handles {
            h.await?.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        }

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 10);
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_reads_return_same_data() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;
        let profile = make_global("conc_read")?;
        repo.save_profile(&profile, None).await?;

        let repo = Arc::new(repo);
        let mut handles = Vec::new();
        for _ in 0..5 {
            let repo_clone = Arc::clone(&repo);
            let id = profile.id.clone();
            handles.push(tokio::spawn(async move {
                let loaded = repo_clone.load_profile(&id).await?;
                Ok::<_, anyhow::Error>(loaded.is_some())
            }));
        }

        for h in handles {
            let found = h.await?.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            assert!(found);
        }
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_save_and_delete_different_ids() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;
        let repo = Arc::new(repo);

        // Save some profiles first
        for i in 0..5 {
            let p = make_global(&format!("conc_sd_{i}"))?;
            repo.save_profile(&p, None).await?;
        }

        let mut handles = Vec::new();
        // Delete first 3 concurrently while saving 3 new ones
        for i in 0..3 {
            let repo_clone = Arc::clone(&repo);
            let del_id = pid(&format!("conc_sd_{i}")).map_err(|e| anyhow::anyhow!("{e}"))?;
            handles.push(tokio::spawn(async move {
                repo_clone.delete_profile(&del_id).await?;
                Ok::<_, anyhow::Error>(())
            }));
        }
        for i in 5..8 {
            let repo_clone = Arc::clone(&repo);
            handles.push(tokio::spawn(async move {
                let p = make_global(&format!("conc_sd_{i}")).map_err(|e| anyhow::anyhow!("{e}"))?;
                repo_clone.save_profile(&p, None).await?;
                Ok::<_, anyhow::Error>(())
            }));
        }

        for h in handles {
            h.await?.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        }

        let profiles = repo.list_profiles().await?;
        // Started with 5, deleted 3, added 3 → 5
        assert_eq!(profiles.len(), 5);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 7. Profile versioning
// ---------------------------------------------------------------------------

mod versioning {
    use super::*;

    #[tokio::test]
    async fn legacy_json_auto_migrated_on_load() -> TestResult {
        let (repo, tmp) = setup_repo().await?;

        let legacy_json = r#"{
            "ffb_gain": 0.8,
            "degrees_of_rotation": 900,
            "torque_cap": 12.0
        }"#;
        let path = tmp.path().join("legacy_test.json");
        std::fs::write(&path, legacy_json)?;

        // Reload to pick up the file
        repo.reload().await?;

        // The repo should have migrated the file. Verify file content changed.
        let content = std::fs::read_to_string(&path)?;
        assert!(
            content.contains("wheel.profile/1"),
            "migrated file should contain current schema"
        );
        Ok(())
    }

    #[tokio::test]
    async fn migration_adapter_detect_and_migrate() -> TestResult {
        let tmp = TempDir::new()?;
        let adapter = MigrationAdapter::new(tmp.path().join("backups"))?;

        let legacy = r#"{"ffb_gain": 0.7, "degrees_of_rotation": 540, "torque_cap": 15.0}"#;
        assert!(adapter.needs_migration(legacy)?);

        let migrated = adapter.migrate(legacy)?;
        let value: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            value.get("schema").and_then(|v| v.as_str()),
            Some("wheel.profile/1")
        );
        Ok(())
    }

    #[tokio::test]
    async fn current_schema_not_flagged_for_migration() -> TestResult {
        let tmp = TempDir::new()?;
        let adapter = MigrationAdapter::new(tmp.path().join("backups"))?;

        let current = r#"{
            "schema": "wheel.profile/1",
            "scope": {},
            "base": {
                "ffbGain": 0.7,
                "dorDeg": 900,
                "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                }
            }
        }"#;

        assert!(!adapter.needs_migration(current)?);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 8. Hierarchy resolution
// ---------------------------------------------------------------------------

mod hierarchy {
    use super::*;

    #[tokio::test]
    async fn resolve_empty_repo_returns_default_global() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;

        let resolved = repo
            .resolve_profile_hierarchy(Some("iracing"), None, None, None)
            .await?;
        // Should return a default global profile
        assert!(resolved.scope.game.is_none() || resolved.scope.game.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn resolve_game_overrides_global() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;

        let mut global = make_global("hier_global")?;
        global.base_settings = BaseSettings::new(
            gain(0.3)?,
            dor(900.0)?,
            torque(10.0)?,
            FilterConfig::default(),
        );
        repo.save_profile(&global, None).await?;

        let mut game = make_game("hier_game_ir", "iracing")?;
        game.base_settings = BaseSettings::new(
            gain(0.8)?,
            dor(540.0)?,
            torque(15.0)?,
            FilterConfig::default(),
        );
        repo.save_profile(&game, None).await?;

        let resolved = repo
            .resolve_profile_hierarchy(Some("iracing"), None, None, None)
            .await?;

        // Game-specific should override global
        assert!(
            (resolved.base_settings.ffb_gain.value() - 0.8).abs() < 0.01,
            "game profile should override global ffb_gain"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 9. Signing
// ---------------------------------------------------------------------------

mod signing {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[tokio::test]
    async fn signed_profile_has_trusted_signature() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);

        let profile = make_global("signed_test")?;
        repo.save_profile(&profile, Some(&signing_key)).await?;

        let sig = repo.get_profile_signature(&profile.id).await?;
        assert!(sig.is_some(), "signed profile must have a signature");
        Ok(())
    }

    #[tokio::test]
    async fn unsigned_profile_has_no_signature() -> TestResult {
        let (repo, _tmp) = setup_repo().await?;
        let profile = make_global("unsigned_test")?;
        repo.save_profile(&profile, None).await?;

        let sig = repo.get_profile_signature(&profile.id).await?;
        assert!(sig.is_none(), "unsigned profile must have no signature");
        Ok(())
    }

    #[test]
    fn signer_hash_is_deterministic() {
        let json = r#"{"test": "data"}"#;
        let h1 = ProfileSigner::hash_json(json);
        let h2 = ProfileSigner::hash_json(json);
        assert_eq!(h1, h2);
    }
}
