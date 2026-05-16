//! Lane manifest and artifact path helpers for Moza support-bundle commands.
//!
//! This module keeps filesystem path normalization and lane manifest endpoint
//! selection separate from command orchestration.

use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::fs;
use std::path::{Component, Path, PathBuf};

use super::{
    MOZA_VENDOR_ID, hex_u16, json_string, json_u64, parse_hex_selector, product_ids,
    read_json_value,
};

pub(super) fn lane_manifest_r5_pid(lane: &Path) -> Option<u16> {
    let manifest = read_json_value(lane, "manifest.json").ok()?;
    let hardware = manifest.get("hardware")?;
    let pid = json_string(hardware, "wheelbase_pid").and_then(parse_hex_selector)?;
    matches!(pid, product_ids::R5_V1 | product_ids::R5_V2).then_some(pid)
}

pub(super) fn lane_manifest_r5_hid_observe_selector(lane: &Path) -> Option<String> {
    let manifest = read_json_value(lane, "manifest.json").ok()?;
    let expected_pid = lane_manifest_r5_pid(lane);
    let endpoints = manifest
        .get("topology")?
        .get("endpoints")
        .and_then(Value::as_array)?;

    endpoints.iter().find_map(|endpoint| {
        if json_string(endpoint, "kind") != Some("wheelbase_hub") {
            return None;
        }
        let vendor_id = json_string(endpoint, "vendor_id").and_then(parse_hex_selector)?;
        let product_id = json_string(endpoint, "product_id").and_then(parse_hex_selector)?;
        if vendor_id != MOZA_VENDOR_ID
            || !matches!(product_id, product_ids::R5_V1 | product_ids::R5_V2)
            || expected_pid.is_some_and(|expected| expected != product_id)
        {
            return None;
        }
        let interface_number = json_u64(endpoint, "interface_number")?;
        let usage_page = json_string(endpoint, "usage_page").and_then(parse_hex_selector)?;
        let usage = json_string(endpoint, "usage").and_then(parse_hex_selector)?;
        Some(format!(
            "hid-{}-{}-if{}-{}-{}",
            hex_u16(vendor_id),
            hex_u16(product_id),
            interface_number,
            hex_u16(usage_page),
            hex_u16(usage)
        ))
    })
}

pub(super) fn proof_output_path(
    lane: &Path,
    json_out: Option<&Path>,
    default_file: &str,
) -> PathBuf {
    json_out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| lane.join(default_file))
}

pub(super) fn ensure_receipt_writable(path: &Path, overwrite: bool) -> Result<()> {
    if path.exists() && !overwrite {
        return Err(anyhow!(
            "{} already exists; pass --overwrite to replace it",
            path.display()
        ));
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create '{}'", parent.display()))?;
    }
    Ok(())
}

pub(super) fn lane_relative_artifact_path(lane: &Path, artifact: &Path) -> Result<String> {
    let relative = if artifact.is_absolute() {
        let absolute_lane = std::path::absolute(lane)
            .with_context(|| format!("failed to absolutize lane '{}'", lane.display()))?;
        let absolute_artifact = std::path::absolute(artifact)
            .with_context(|| format!("failed to absolutize artifact '{}'", artifact.display()))?;
        absolute_artifact
            .strip_prefix(&absolute_lane)
            .with_context(|| {
                format!(
                    "artifact '{}' must be under lane '{}'",
                    artifact.display(),
                    lane.display()
                )
            })?
            .to_path_buf()
    } else if let Some(relative) = lane_prefixed_relative_path(lane, artifact) {
        relative
    } else if let Some(relative) = cwd_relative_lane_prefixed_path(lane, artifact)? {
        relative
    } else {
        artifact.to_path_buf()
    };
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(anyhow!(
            "artifact '{}' must be a simple lane-relative path",
            artifact.display()
        ));
    }
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

pub(super) fn simple_lane_relative_path_string(path: &Path, label: &str) -> Result<String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(anyhow!(
            "{label} '{}' must be a simple lane-relative path",
            path.display()
        ));
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
}

fn lane_prefixed_relative_path(lane: &Path, artifact: &Path) -> Option<PathBuf> {
    if lane.is_absolute() || artifact.is_absolute() {
        return None;
    }

    let relative = artifact.strip_prefix(lane).ok()?;
    (!relative.as_os_str().is_empty()).then(|| relative.to_path_buf())
}

fn cwd_relative_lane_prefixed_path(lane: &Path, artifact: &Path) -> Result<Option<PathBuf>> {
    if artifact.is_absolute() {
        return Ok(None);
    }

    let absolute_lane = std::path::absolute(lane)
        .with_context(|| format!("failed to absolutize lane '{}'", lane.display()))?;
    let absolute_artifact = std::path::absolute(artifact)
        .with_context(|| format!("failed to absolutize artifact '{}'", artifact.display()))?;
    let Some(relative) = absolute_artifact
        .strip_prefix(&absolute_lane)
        .ok()
        .filter(|path| !path.as_os_str().is_empty())
    else {
        return Ok(None);
    };
    Ok(Some(relative.to_path_buf()))
}

pub(super) fn lane_relative_output_path(lane: &Path, output: &Path) -> Result<(PathBuf, String)> {
    let artifact_path = lane_relative_artifact_path(lane, output)?;
    let write_path = if output.is_absolute() {
        output.to_path_buf()
    } else {
        lane.join(Path::new(&artifact_path))
    };
    Ok((write_path, artifact_path))
}
