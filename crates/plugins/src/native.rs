//! Native plugin system with SPSC shared memory and RT watchdog
//!
//! This module provides native plugin loading with:
//! - Ed25519 signature verification against a trust store
//! - ABI version compatibility checking
//! - Support for both signed and unsigned plugins (configurable)
//! - SPSC shared memory for RT communication
//! - RT watchdog for budget enforcement

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam::channel::{Receiver, Sender, bounded};
use libloading::{Library, Symbol};
use shared_memory::{Shmem, ShmemConf};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

use crate::capability::CapabilityChecker;
use crate::manifest::PluginManifest;
use crate::{Plugin, PluginContext, PluginError, PluginOutput, PluginResult};
use racing_wheel_engine::NormalizedTelemetry;
use racing_wheel_engine::prelude::MutexExt;
use racing_wheel_service::crypto::ed25519::{Ed25519Verifier, Signature};
use racing_wheel_service::crypto::trust_store::TrustStore;
use racing_wheel_service::crypto::{SignatureMetadata, TrustLevel, utils as crypto_utils};

/// Current native plugin ABI version
///
/// This version must match the plugin's ABI version for successful loading.
/// Increment this when making breaking changes to the plugin ABI.
pub const CURRENT_ABI_VERSION: u32 = 1;

/// Native plugin ABI version (alias for backward compatibility)
const PLUGIN_ABI_VERSION: u32 = CURRENT_ABI_VERSION;

/// Maximum shared memory size (4MB)
const MAX_SHARED_MEMORY_SIZE: usize = 4 * 1024 * 1024;

/// Configuration for native plugin loading
///
/// This struct controls how the native plugin loader handles signature verification
/// and unsigned plugins. The behavior is determined by the combination of
/// `allow_unsigned` and `require_signatures` fields:
///
/// | `require_signatures` | `allow_unsigned` | Behavior |
/// |---------------------|------------------|----------|
/// | `true`              | `false`          | **Strict mode**: Only signed plugins with valid signatures are loaded. Unsigned plugins are rejected. |
/// | `true`              | `true`           | **Permissive mode**: Signed plugins are verified, unsigned plugins are allowed with a warning. |
/// | `false`             | `true`           | **Development mode**: No signature verification, all plugins are loaded. |
/// | `false`             | `false`          | Same as strict mode - unsigned plugins are rejected. |
///
/// # Security Considerations
///
/// - In production environments, use `require_signatures: true` and `allow_unsigned: false`
///   to ensure only trusted, signed plugins can be loaded.
/// - The `allow_unsigned: true` option should only be used in development or testing
///   environments where plugin signing infrastructure is not available.
/// - When an unsigned plugin is loaded (with `allow_unsigned: true`), a warning is logged
///   to alert operators of the potential security risk.
///
/// # Example
///
/// ```rust
/// use racing_wheel_plugins::native::NativePluginConfig;
///
/// // Production configuration (strict)
/// let production_config = NativePluginConfig {
///     allow_unsigned: false,
///     require_signatures: true,
/// };
///
/// // Development configuration (permissive)
/// let dev_config = NativePluginConfig {
///     allow_unsigned: true,
///     require_signatures: false,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct NativePluginConfig {
    /// Whether to allow loading unsigned plugins.
    ///
    /// When `true`, plugins without a valid signature file can be loaded,
    /// but a warning will be logged. When `false`, unsigned plugins are
    /// rejected with an error.
    ///
    /// **Default**: `false` (unsigned plugins are rejected)
    ///
    /// **Security Note**: Setting this to `true` bypasses signature verification
    /// for unsigned plugins, which may allow loading of untrusted code.
    /// Only enable this in development or testing environments.
    pub allow_unsigned: bool,

    /// Whether to require signature verification for signed plugins.
    ///
    /// When `true`, plugins with signature files must have valid signatures
    /// that can be verified against the trust store. When `false`, signature
    /// verification is skipped entirely.
    ///
    /// **Default**: `true` (signatures are verified)
    ///
    /// **Security Note**: Setting this to `false` disables all signature
    /// verification, which should only be done in development environments.
    pub require_signatures: bool,
}

impl Default for NativePluginConfig {
    fn default() -> Self {
        Self {
            allow_unsigned: false,
            require_signatures: true,
        }
    }
}

/// Error types specific to native plugin loading
#[derive(Debug, Clone)]
pub enum NativePluginLoadError {
    /// ABI version mismatch between plugin and loader
    AbiMismatch { expected: u32, actual: u32 },
    /// Plugin signature is invalid
    InvalidSignature { reason: String },
    /// Plugin is unsigned but unsigned plugins are not allowed
    UnsignedPlugin { path: String },
    /// Plugin signer is not trusted
    UntrustedSigner { fingerprint: String },
    /// Library loading failed
    LibraryLoadFailed { reason: String },
    /// Plugin initialization failed
    InitializationFailed { reason: String },
}

/// Frame data for RT communication
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PluginFrame {
    pub ffb_in: f32,
    pub torque_out: f32,
    pub wheel_speed: f32,
    pub timestamp_ns: u64,
    pub budget_us: u32,
    pub sequence: u32,
}

/// Shared memory header
#[repr(C)]
#[derive(Debug)]
pub struct SharedMemoryHeader {
    pub version: u32,
    pub producer_seq: AtomicU32,
    pub consumer_seq: AtomicU32,
    pub frame_size: u32,
    pub max_frames: u32,
    pub shutdown_flag: AtomicBool,
}

/// Native plugin function table (C ABI)
#[repr(C)]
pub struct PluginVTable {
    pub create: extern "C" fn(*const u8, usize) -> *mut std::ffi::c_void,
    pub process: extern "C" fn(*mut std::ffi::c_void, *mut PluginFrame) -> i32,
    pub destroy: extern "C" fn(*mut std::ffi::c_void),
    pub abi_version: u32,
}

/// Native plugin instance
pub struct NativePlugin {
    manifest: PluginManifest,
    _library: Library,
    vtable: PluginVTable,
    plugin_state: *mut std::ffi::c_void,
    capability_checker: CapabilityChecker,
    helper_process: Option<NativePluginHelper>,
    /// ABI version of the loaded plugin
    abi_version: u32,
    /// Signature metadata (if plugin was signed)
    signature: Option<SignatureMetadata>,
}

unsafe impl Send for NativePlugin {}
unsafe impl Sync for NativePlugin {}

impl NativePlugin {
    /// Load a native plugin from shared library with signature verification
    ///
    /// This method:
    /// 1. Verifies the plugin's Ed25519 signature against the trust store (if required)
    /// 2. Loads the shared library using libloading
    /// 3. Verifies ABI version compatibility
    /// 4. Initializes the plugin
    ///
    /// # Arguments
    /// * `manifest` - Plugin manifest with metadata
    /// * `library_path` - Path to the shared library (.dll/.so)
    /// * `trust_store` - Trust store for signature verification
    /// * `config` - Configuration for plugin loading
    ///
    /// # Errors
    /// Returns an error if:
    /// - Signature verification fails (when required)
    /// - Plugin is unsigned and unsigned plugins are not allowed
    /// - ABI version doesn't match CURRENT_ABI_VERSION
    /// - Library loading fails
    /// - Plugin initialization fails
    pub async fn load(
        manifest: PluginManifest,
        library_path: &Path,
        trust_store: &TrustStore,
        config: &NativePluginConfig,
    ) -> PluginResult<Self> {
        // Step 1: Verify signature if required
        let signature = Self::verify_signature(library_path, trust_store, config)?;

        // Step 2: Load the shared library
        let library = unsafe {
            Library::new(library_path)
                .map_err(|e| PluginError::LoadingFailed(format!("Library load failed: {}", e)))?
        };

        // Step 3: Get the plugin vtable
        let get_vtable: Symbol<extern "C" fn() -> PluginVTable> = unsafe {
            library.get(b"get_plugin_vtable").map_err(|e| {
                PluginError::LoadingFailed(format!("Missing vtable function: {}", e))
            })?
        };

        let vtable = get_vtable();
        let abi_version = vtable.abi_version;

        // Step 4: Check ABI version
        if vtable.abi_version != CURRENT_ABI_VERSION {
            return Err(PluginError::LoadingFailed(format!(
                "ABI version mismatch: expected {}, got {}",
                CURRENT_ABI_VERSION, vtable.abi_version
            )));
        }

        // Step 5: Create capability checker
        let capability_checker = CapabilityChecker::new(manifest.capabilities.clone());

        // Step 6: Initialize plugin state
        let config_json = serde_json::to_string(&serde_json::Value::Null)
            .map_err(|e| PluginError::LoadingFailed(format!("Config serialization: {}", e)))?;

        let plugin_state = (vtable.create)(config_json.as_ptr(), config_json.len());

        if plugin_state.is_null() {
            return Err(PluginError::LoadingFailed(
                "Plugin initialization failed".to_string(),
            ));
        }

        tracing::info!(
            plugin_id = %manifest.id,
            abi_version = abi_version,
            signed = signature.is_some(),
            "Native plugin loaded successfully"
        );

        Ok(Self {
            manifest,
            _library: library,
            vtable,
            plugin_state,
            capability_checker,
            helper_process: None,
            abi_version,
            signature,
        })
    }

    /// Load a native plugin without signature verification (legacy method)
    ///
    /// This method is provided for backward compatibility. New code should use
    /// `load()` with a trust store for proper signature verification.
    pub async fn load_without_verification(
        manifest: PluginManifest,
        library_path: &Path,
    ) -> PluginResult<Self> {
        // Load the shared library
        let library = unsafe {
            Library::new(library_path)
                .map_err(|e| PluginError::LoadingFailed(format!("Library load failed: {}", e)))?
        };

        // Get the plugin vtable
        let get_vtable: Symbol<extern "C" fn() -> PluginVTable> = unsafe {
            library.get(b"get_plugin_vtable").map_err(|e| {
                PluginError::LoadingFailed(format!("Missing vtable function: {}", e))
            })?
        };

        let vtable = get_vtable();
        let abi_version = vtable.abi_version;

        // Check ABI version
        if vtable.abi_version != CURRENT_ABI_VERSION {
            return Err(PluginError::LoadingFailed(format!(
                "ABI version mismatch: expected {}, got {}",
                CURRENT_ABI_VERSION, vtable.abi_version
            )));
        }

        // Create capability checker
        let capability_checker = CapabilityChecker::new(manifest.capabilities.clone());

        // Initialize plugin state
        let config_json = serde_json::to_string(&serde_json::Value::Null)
            .map_err(|e| PluginError::LoadingFailed(format!("Config serialization: {}", e)))?;

        let plugin_state = (vtable.create)(config_json.as_ptr(), config_json.len());

        if plugin_state.is_null() {
            return Err(PluginError::LoadingFailed(
                "Plugin initialization failed".to_string(),
            ));
        }

        tracing::warn!(
            plugin_id = %manifest.id,
            "Native plugin loaded without signature verification"
        );

        Ok(Self {
            manifest,
            _library: library,
            vtable,
            plugin_state,
            capability_checker,
            helper_process: None,
            abi_version,
            signature: None,
        })
    }

    /// Verify the plugin's signature against the trust store.
    ///
    /// This method implements the signature verification logic for native plugins,
    /// handling both signed and unsigned plugins according to the configuration.
    ///
    /// # Behavior
    ///
    /// 1. **Unsigned plugins** (no `.sig` file):
    ///    - If `allow_unsigned` is `true`: Returns `Ok(None)` with a warning log
    ///    - If `allow_unsigned` is `false`: Returns an error rejecting the plugin
    ///
    /// 2. **Signed plugins** (with `.sig` file):
    ///    - Verifies the signature against the trust store
    ///    - Rejects plugins signed by distrusted keys
    ///    - Accepts plugins signed by trusted keys after verification
    ///    - Handles unknown signers based on `allow_unsigned` setting
    ///
    /// # Arguments
    ///
    /// * `library_path` - Path to the plugin shared library
    /// * `trust_store` - Trust store containing trusted public keys
    /// * `config` - Plugin loading configuration
    ///
    /// # Returns
    ///
    /// * `Ok(Some(metadata))` - Plugin is signed and signature is valid
    /// * `Ok(None)` - Plugin is unsigned but `allow_unsigned` is true
    /// * `Err(...)` - Plugin rejected (unsigned when not allowed, invalid signature, etc.)
    ///
    /// # Logging
    ///
    /// This method logs at various levels:
    /// - `WARN`: When loading unsigned plugins or plugins with unknown signers
    /// - `ERROR`: When rejecting plugins signed by distrusted keys
    /// - `DEBUG`: When successfully verifying trusted signatures
    /// - `INFO`: When signature verification succeeds
    fn verify_signature(
        library_path: &Path,
        trust_store: &TrustStore,
        config: &NativePluginConfig,
    ) -> PluginResult<Option<SignatureMetadata>> {
        // Check if a signature file exists
        let has_signature = crypto_utils::signature_exists(library_path);

        if !has_signature {
            // Plugin is unsigned
            if config.require_signatures && !config.allow_unsigned {
                tracing::warn!(
                    path = %library_path.display(),
                    "Rejecting unsigned native plugin"
                );
                return Err(PluginError::LoadingFailed(format!(
                    "Plugin is unsigned and unsigned plugins are not allowed: {}",
                    library_path.display()
                )));
            }

            if config.allow_unsigned {
                tracing::warn!(
                    path = %library_path.display(),
                    "Loading unsigned native plugin (allow_unsigned=true)"
                );
                return Ok(None);
            }

            return Err(PluginError::LoadingFailed(format!(
                "Plugin is unsigned: {}",
                library_path.display()
            )));
        }

        // Read the signature metadata
        let metadata = crypto_utils::extract_signature_metadata(library_path)
            .map_err(|e| PluginError::LoadingFailed(format!("Failed to read signature: {}", e)))?
            .ok_or_else(|| {
                PluginError::LoadingFailed(
                    "Signature file exists but could not be parsed".to_string(),
                )
            })?;

        // Check if the signer is trusted
        let trust_level = trust_store.get_trust_level(&metadata.key_fingerprint);

        match trust_level {
            TrustLevel::Distrusted => {
                tracing::error!(
                    path = %library_path.display(),
                    fingerprint = %metadata.key_fingerprint,
                    "Rejecting plugin signed by distrusted key"
                );
                return Err(PluginError::LoadingFailed(format!(
                    "Plugin signed by distrusted key: {}",
                    metadata.key_fingerprint
                )));
            }
            TrustLevel::Unknown => {
                tracing::warn!(
                    path = %library_path.display(),
                    fingerprint = %metadata.key_fingerprint,
                    "Plugin signed by unknown key"
                );
                // Continue with verification - unknown keys can still have valid signatures
            }
            TrustLevel::Trusted => {
                tracing::debug!(
                    path = %library_path.display(),
                    fingerprint = %metadata.key_fingerprint,
                    "Plugin signed by trusted key"
                );
            }
        }

        // Get the public key from the trust store
        let public_key = match trust_store.get_public_key(&metadata.key_fingerprint) {
            Some(key) => key,
            None => {
                // Key not in trust store - if we require trusted signatures, reject
                if trust_level == TrustLevel::Unknown && !config.allow_unsigned {
                    tracing::warn!(
                        path = %library_path.display(),
                        fingerprint = %metadata.key_fingerprint,
                        "Plugin signed by key not in trust store"
                    );
                    // We can't verify without the public key
                    return Err(PluginError::LoadingFailed(format!(
                        "Signer key not in trust store: {}",
                        metadata.key_fingerprint
                    )));
                }
                // Allow loading with unknown signer if configured
                tracing::warn!(
                    path = %library_path.display(),
                    "Loading plugin with unverifiable signature (key not in trust store)"
                );
                return Ok(Some(metadata));
            }
        };

        // Read the plugin file content for verification
        let content = std::fs::read(library_path).map_err(|e| {
            PluginError::LoadingFailed(format!("Failed to read plugin file: {}", e))
        })?;

        // Parse and verify the signature
        let signature = Signature::from_base64(&metadata.signature)
            .map_err(|e| PluginError::LoadingFailed(format!("Invalid signature format: {}", e)))?;

        let is_valid = Ed25519Verifier::verify(&content, &signature, &public_key).map_err(|e| {
            PluginError::LoadingFailed(format!("Signature verification error: {}", e))
        })?;

        if !is_valid {
            tracing::error!(
                path = %library_path.display(),
                "Plugin signature verification failed"
            );
            return Err(PluginError::LoadingFailed(
                "Plugin signature verification failed".to_string(),
            ));
        }

        tracing::info!(
            path = %library_path.display(),
            signer = %metadata.signer,
            "Plugin signature verified successfully"
        );

        Ok(Some(metadata))
    }

    /// Verify ABI compatibility
    ///
    /// Returns Ok(()) if the plugin's ABI version matches CURRENT_ABI_VERSION,
    /// or an error describing the mismatch.
    pub fn verify_abi(&self) -> PluginResult<()> {
        if self.abi_version != CURRENT_ABI_VERSION {
            return Err(PluginError::LoadingFailed(format!(
                "ABI version mismatch: expected {}, got {}",
                CURRENT_ABI_VERSION, self.abi_version
            )));
        }
        Ok(())
    }

    /// Get the plugin's ABI version
    pub fn abi_version(&self) -> u32 {
        self.abi_version
    }

    /// Get the plugin's signature metadata (if signed)
    pub fn signature(&self) -> Option<&SignatureMetadata> {
        self.signature.as_ref()
    }

    /// Check if the plugin is signed
    pub fn is_signed(&self) -> bool {
        self.signature.is_some()
    }

    /// Start helper process for RT operations
    pub async fn start_helper_process(&mut self) -> PluginResult<()> {
        let helper = NativePluginHelper::new(
            self.manifest.id,
            self.manifest.constraints.max_execution_time_us,
        )
        .await?;

        self.helper_process = Some(helper);
        Ok(())
    }

    /// Process frame in RT context (called from helper process)
    pub fn process_frame_rt(&mut self, frame: &mut PluginFrame) -> PluginResult<()> {
        let start_time = Instant::now();

        // Call native plugin function
        let result = (self.vtable.process)(self.plugin_state, frame as *mut PluginFrame);

        let execution_time = start_time.elapsed();

        // Check budget violation
        if execution_time.as_micros() > frame.budget_us as u128 {
            return Err(PluginError::BudgetViolation {
                used_us: execution_time.as_micros() as u32,
                budget_us: frame.budget_us,
            });
        }

        if result != 0 {
            return Err(PluginError::LoadingFailed(format!(
                "Native plugin process function failed with code: {}",
                result
            )));
        }

        Ok(())
    }
}

impl Drop for NativePlugin {
    fn drop(&mut self) {
        if !self.plugin_state.is_null() {
            (self.vtable.destroy)(self.plugin_state);
        }
    }
}

#[async_trait::async_trait]
impl Plugin for NativePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn initialize(&mut self, config: serde_json::Value) -> PluginResult<()> {
        // Reinitialize with actual config
        if !self.plugin_state.is_null() {
            (self.vtable.destroy)(self.plugin_state);
        }

        let config_json = serde_json::to_string(&config)
            .map_err(|e| PluginError::LoadingFailed(format!("Config serialization: {}", e)))?;

        self.plugin_state = (self.vtable.create)(config_json.as_ptr(), config_json.len());

        if self.plugin_state.is_null() {
            return Err(PluginError::LoadingFailed(
                "Plugin initialization failed".to_string(),
            ));
        }

        // Start helper process for RT operations
        self.start_helper_process().await?;

        Ok(())
    }

    async fn process_telemetry(
        &mut self,
        input: &NormalizedTelemetry,
        context: &PluginContext,
    ) -> PluginResult<PluginOutput> {
        // Check capability
        self.capability_checker.check_telemetry_read()?;

        // For non-RT telemetry processing, we can call directly
        let mut frame = PluginFrame {
            ffb_in: input.ffb_scalar,
            torque_out: input.ffb_scalar,
            wheel_speed: input.speed_ms,
            timestamp_ns: chrono::Utc::now()
                .timestamp_nanos_opt()
                .map(|ts| ts as u64)
                .unwrap_or(0),
            budget_us: context.budget_us,
            sequence: 0,
        };

        self.process_frame_rt(&mut frame)?;

        // Create modified telemetry
        let mut modified_telemetry = input.clone();
        modified_telemetry.ffb_scalar = frame.torque_out;

        Ok(PluginOutput::Telemetry(crate::PluginTelemetryOutput {
            modified_telemetry: Some(modified_telemetry),
            custom_data: serde_json::Value::Null,
        }))
    }

    async fn process_led_mapping(
        &mut self,
        _input: &NormalizedTelemetry,
        _context: &PluginContext,
    ) -> PluginResult<PluginOutput> {
        // Check capability
        self.capability_checker.check_led_control()?;

        // Return default LED output (simplified)
        Ok(PluginOutput::Led(crate::PluginLedOutput {
            led_pattern: vec![0, 255, 0], // Green
            brightness: 1.0,
            duration_ms: 100,
        }))
    }

    async fn process_dsp(
        &mut self,
        ffb_input: f32,
        wheel_speed: f32,
        context: &PluginContext,
    ) -> PluginResult<PluginOutput> {
        // Check capability
        self.capability_checker.check_dsp_processing()?;

        // Use helper process for RT DSP processing
        if let Some(helper) = &mut self.helper_process {
            let frame = PluginFrame {
                ffb_in: ffb_input,
                torque_out: ffb_input,
                wheel_speed,
                timestamp_ns: chrono::Utc::now()
                    .timestamp_nanos_opt()
                    .map(|ts| ts as u64)
                    .unwrap_or(0),
                budget_us: context.budget_us,
                sequence: 0,
            };

            let result_frame = helper.process_frame(frame).await?;

            Ok(PluginOutput::Dsp(crate::PluginDspOutput {
                modified_ffb: result_frame.torque_out,
                filter_state: serde_json::Value::Null,
            }))
        } else {
            Err(PluginError::LoadingFailed(
                "Helper process not started".to_string(),
            ))
        }
    }

    async fn shutdown(&mut self) -> PluginResult<()> {
        // Shutdown helper process
        if let Some(helper) = &mut self.helper_process {
            helper.shutdown().await?;
        }

        Ok(())
    }
}

/// Native plugin helper process for RT operations
pub struct NativePluginHelper {
    _plugin_id: uuid::Uuid,
    process: Child,
    shared_memory: Arc<Mutex<Shmem>>, // Wrap in Arc<Mutex<>> for Send/Sync
    _frame_sender: Sender<PluginFrame>,
    _result_receiver: Receiver<PluginFrame>,
    budget_us: u32,
}

// Manually implement Send and Sync since we're using Arc<Mutex<>>
unsafe impl Send for NativePluginHelper {}
unsafe impl Sync for NativePluginHelper {}

impl NativePluginHelper {
    /// Create a new helper process
    pub async fn new(plugin_id: uuid::Uuid, budget_us: u32) -> PluginResult<Self> {
        // Create shared memory
        let shmem_size =
            std::mem::size_of::<SharedMemoryHeader>() + (std::mem::size_of::<PluginFrame>() * 1024); // Ring buffer for 1024 frames

        if shmem_size > MAX_SHARED_MEMORY_SIZE {
            return Err(PluginError::Ipc(format!(
                "Shared memory size {} exceeds maximum {}",
                shmem_size, MAX_SHARED_MEMORY_SIZE
            )));
        }

        let shared_memory = ShmemConf::new()
            .size(shmem_size)
            .create()
            .map_err(|e| PluginError::Ipc(format!("Shared memory creation failed: {}", e)))?;

        // Initialize shared memory header
        unsafe {
            let header = shared_memory.as_ptr() as *mut SharedMemoryHeader;
            (*header).version = PLUGIN_ABI_VERSION;
            (*header).producer_seq.store(0, Ordering::Relaxed);
            (*header).consumer_seq.store(0, Ordering::Relaxed);
            (*header).frame_size = std::mem::size_of::<PluginFrame>() as u32;
            (*header).max_frames = 1024;
            (*header).shutdown_flag.store(false, Ordering::Relaxed);
        }

        let shmem_os_id = shared_memory.get_os_id().to_string();
        #[allow(clippy::arc_with_non_send_sync)]
        let shared_memory = Arc::new(Mutex::new(shared_memory));

        // Start helper process
        let process = Command::new("wheel-plugin-helper")
            .arg("--plugin-id")
            .arg(plugin_id.to_string())
            .arg("--shmem-id")
            .arg(shmem_os_id)
            .arg("--budget-us")
            .arg(budget_us.to_string())
            .spawn()
            .map_err(|e| PluginError::Ipc(format!("Helper process spawn failed: {}", e)))?;

        // Create communication channels
        let (frame_sender, _) = bounded(1024);
        let (_, result_receiver) = bounded(1024);

        Ok(Self {
            _plugin_id: plugin_id,
            process,
            shared_memory,
            _frame_sender: frame_sender,
            _result_receiver: result_receiver,
            budget_us,
        })
    }

    /// Process a frame through the helper
    pub async fn process_frame(&mut self, frame: PluginFrame) -> PluginResult<PluginFrame> {
        // Send frame to helper process via shared memory
        self.write_frame_to_shared_memory(frame)?;

        // Wait for result with timeout
        let timeout = Duration::from_micros(self.budget_us as u64 * 2); // 2x budget for safety

        tokio::time::timeout(timeout, async { self.read_frame_from_shared_memory() })
            .await
            .map_err(|_| PluginError::ExecutionTimeout { duration: timeout })?
    }

    /// Shutdown the helper process
    pub async fn shutdown(&mut self) -> PluginResult<()> {
        // Set shutdown flag
        unsafe {
            let shared_memory = self.shared_memory.lock_or_panic();
            let header = shared_memory.as_ptr() as *mut SharedMemoryHeader;
            (*header).shutdown_flag.store(true, Ordering::Relaxed);
        }

        // Wait for process to exit
        let _ = self.process.wait().await;

        Ok(())
    }

    fn write_frame_to_shared_memory(&self, frame: PluginFrame) -> PluginResult<()> {
        unsafe {
            let shared_memory = self.shared_memory.lock_or_panic();
            let header = shared_memory.as_ptr() as *mut SharedMemoryHeader;
            let frames_ptr = (header as *mut u8).add(std::mem::size_of::<SharedMemoryHeader>())
                as *mut PluginFrame;

            let producer_seq = (*header).producer_seq.load(Ordering::Acquire);
            let consumer_seq = (*header).consumer_seq.load(Ordering::Acquire);

            // Check if ring buffer is full
            if producer_seq.wrapping_sub(consumer_seq) >= (*header).max_frames {
                return Err(PluginError::Ipc("Ring buffer full".to_string()));
            }

            // Write frame
            let index = producer_seq % (*header).max_frames;
            *frames_ptr.add(index as usize) = frame;

            // Update producer sequence
            (*header)
                .producer_seq
                .store(producer_seq.wrapping_add(1), Ordering::Release);
        }

        Ok(())
    }

    fn read_frame_from_shared_memory(&self) -> PluginResult<PluginFrame> {
        unsafe {
            let shared_memory = self.shared_memory.lock_or_panic();
            let header = shared_memory.as_ptr() as *mut SharedMemoryHeader;
            let frames_ptr = (header as *mut u8).add(std::mem::size_of::<SharedMemoryHeader>())
                as *mut PluginFrame;

            let producer_seq = (*header).producer_seq.load(Ordering::Acquire);
            let consumer_seq = (*header).consumer_seq.load(Ordering::Acquire);

            // Check if data is available
            if consumer_seq >= producer_seq {
                return Err(PluginError::Ipc("No data available".to_string()));
            }

            // Read frame
            let index = consumer_seq % (*header).max_frames;
            let frame = *frames_ptr.add(index as usize);

            // Update consumer sequence
            (*header)
                .consumer_seq
                .store(consumer_seq.wrapping_add(1), Ordering::Release);

            Ok(frame)
        }
    }
}

/// Native plugin host manager
pub struct NativePluginHost {
    plugins: Arc<RwLock<HashMap<uuid::Uuid, NativePlugin>>>,
    trust_store: Arc<TrustStore>,
    config: NativePluginConfig,
}

impl NativePluginHost {
    /// Create a new native plugin host with a trust store
    pub fn new(trust_store: TrustStore, config: NativePluginConfig) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            trust_store: Arc::new(trust_store),
            config,
        }
    }

    /// Create a new native plugin host with secure default configuration
    ///
    /// Default behavior is secure-by-default:
    /// - `require_signatures = true`
    /// - `allow_unsigned = false`
    ///
    /// Use [`NativePluginHost::new_permissive_for_development`] for explicit
    /// development opt-out.
    pub fn new_with_defaults() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            trust_store: Arc::new(TrustStore::new_in_memory()),
            config: NativePluginConfig::default(),
        }
    }

    /// Create a native plugin host with permissive development defaults
    ///
    /// This is an explicit opt-out from secure defaults and should only be used
    /// in development/testing environments.
    pub fn new_permissive_for_development() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            trust_store: Arc::new(TrustStore::new_in_memory()),
            config: NativePluginConfig {
                allow_unsigned: true,
                require_signatures: false,
            },
        }
    }

    /// Load a native plugin with signature verification
    pub async fn load_plugin(
        &self,
        manifest: PluginManifest,
        library_path: &Path,
    ) -> PluginResult<uuid::Uuid> {
        let plugin = NativePlugin::load(
            manifest.clone(),
            library_path,
            &self.trust_store,
            &self.config,
        )
        .await?;
        let plugin_id = manifest.id;

        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin_id, plugin);

        Ok(plugin_id)
    }

    /// Load a native plugin without signature verification (legacy)
    pub async fn load_plugin_without_verification(
        &self,
        manifest: PluginManifest,
        library_path: &Path,
    ) -> PluginResult<uuid::Uuid> {
        let plugin =
            NativePlugin::load_without_verification(manifest.clone(), library_path).await?;
        let plugin_id = manifest.id;

        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin_id, plugin);

        Ok(plugin_id)
    }

    /// Unload a plugin
    pub async fn unload_plugin(&self, plugin_id: uuid::Uuid) -> PluginResult<()> {
        let mut plugins = self.plugins.write().await;
        if let Some(mut plugin) = plugins.remove(&plugin_id) {
            plugin.shutdown().await?;
        }
        Ok(())
    }

    /// Get the trust store
    pub fn trust_store(&self) -> &TrustStore {
        &self.trust_store
    }

    /// Get the plugin configuration
    pub fn config(&self) -> &NativePluginConfig {
        &self.config
    }

    /// Update the plugin configuration
    pub fn set_config(&mut self, config: NativePluginConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_service::crypto::ContentType;
    use racing_wheel_service::crypto::ed25519::{Ed25519Signer, KeyPair};
    use tempfile::TempDir;

    /// Helper to create a test manifest
    #[allow(dead_code)]
    fn create_test_manifest() -> PluginManifest {
        PluginManifest {
            id: uuid::Uuid::new_v4(),
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            homepage: None,
            class: crate::PluginClass::Fast,
            capabilities: vec![],
            operations: vec![],
            constraints: crate::manifest::PluginConstraints {
                max_execution_time_us: 1000,
                max_memory_bytes: 1024 * 1024,
                update_rate_hz: 1000,
                cpu_affinity: None,
            },
            entry_points: crate::manifest::EntryPoints {
                wasm_module: None,
                native_library: Some("test_plugin.so".to_string()),
                main_function: "process".to_string(),
                init_function: None,
                cleanup_function: None,
            },
            config_schema: None,
            signature: None,
        }
    }

    #[test]
    fn test_current_abi_version_constant() -> Result<(), Box<dyn std::error::Error>> {
        // Verify the ABI version constant is set correctly
        assert_eq!(CURRENT_ABI_VERSION, 1);
        assert_eq!(PLUGIN_ABI_VERSION, CURRENT_ABI_VERSION);
        Ok(())
    }

    #[test]
    fn test_native_plugin_config_default() -> Result<(), Box<dyn std::error::Error>> {
        let config = NativePluginConfig::default();

        // Default should require signatures and not allow unsigned
        assert!(!config.allow_unsigned);
        assert!(config.require_signatures);

        Ok(())
    }

    #[test]
    fn test_native_plugin_host_new_with_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let host = NativePluginHost::new_with_defaults();

        // Default host should be secure-by-default
        assert!(!host.config().allow_unsigned);
        assert!(host.config().require_signatures);

        Ok(())
    }

    #[test]
    fn test_native_plugin_host_new_permissive_for_development()
    -> Result<(), Box<dyn std::error::Error>> {
        let host = NativePluginHost::new_permissive_for_development();

        // Development host explicitly opts out of strict verification
        assert!(host.config().allow_unsigned);
        assert!(!host.config().require_signatures);

        Ok(())
    }

    #[test]
    fn test_verify_signature_unsigned_plugin_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let plugin_path = temp_dir.path().join("test_plugin.so");

        // Create a fake plugin file (no signature)
        std::fs::write(&plugin_path, b"fake plugin content")?;

        let trust_store = TrustStore::new_in_memory();
        let config = NativePluginConfig {
            allow_unsigned: false,
            require_signatures: true,
        };

        // Should reject unsigned plugin
        let result = NativePlugin::verify_signature(&plugin_path, &trust_store, &config);
        assert!(result.is_err());

        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.contains("unsigned"),
            "Error should mention unsigned: {}",
            err_msg
        );

        Ok(())
    }

    #[test]
    fn test_verify_signature_unsigned_plugin_allowed() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let plugin_path = temp_dir.path().join("test_plugin.so");

        // Create a fake plugin file (no signature)
        std::fs::write(&plugin_path, b"fake plugin content")?;

        let trust_store = TrustStore::new_in_memory();
        let config = NativePluginConfig {
            allow_unsigned: true,
            require_signatures: true,
        };

        // Should allow unsigned plugin when configured
        let result = NativePlugin::verify_signature(&plugin_path, &trust_store, &config);
        assert!(result.is_ok());

        // Should return None (no signature metadata)
        let signature = result?;
        assert!(signature.is_none());

        Ok(())
    }

    #[test]
    fn test_verify_signature_valid_signature() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let plugin_path = temp_dir.path().join("test_plugin.so");
        let plugin_content = b"fake plugin content for signing";

        // Create a fake plugin file
        std::fs::write(&plugin_path, plugin_content)?;

        // Generate a keypair and sign the plugin
        let keypair = KeyPair::generate()?;
        let metadata = Ed25519Signer::sign_with_metadata(
            plugin_content,
            &keypair,
            "Test Signer",
            ContentType::Plugin,
            Some("Test signature".to_string()),
        )?;

        // Write the signature file
        racing_wheel_service::crypto::utils::create_detached_signature(&plugin_path, &metadata)?;

        // Add the public key to the trust store
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Trusted,
            Some("Test key".to_string()),
        )?;

        let config = NativePluginConfig {
            allow_unsigned: false,
            require_signatures: true,
        };

        // Should verify successfully
        let result = NativePlugin::verify_signature(&plugin_path, &trust_store, &config);
        assert!(
            result.is_ok(),
            "Signature verification failed: {:?}",
            result.err()
        );

        let signature = result?;
        assert!(signature.is_some());
        assert_eq!(
            signature.as_ref().map(|s| s.signer.as_str()),
            Some("Test Signer")
        );

        Ok(())
    }

    #[test]
    fn test_verify_signature_invalid_signature() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let plugin_path = temp_dir.path().join("test_plugin.so");
        let plugin_content = b"fake plugin content for signing";

        // Create a fake plugin file
        std::fs::write(&plugin_path, plugin_content)?;

        // Generate a keypair and sign the plugin
        let keypair = KeyPair::generate()?;
        let metadata = Ed25519Signer::sign_with_metadata(
            plugin_content,
            &keypair,
            "Test Signer",
            ContentType::Plugin,
            None,
        )?;

        // Write the signature file
        racing_wheel_service::crypto::utils::create_detached_signature(&plugin_path, &metadata)?;

        // Modify the plugin content after signing (invalidates signature)
        std::fs::write(&plugin_path, b"modified plugin content")?;

        // Add the public key to the trust store
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Trusted,
            Some("Test key".to_string()),
        )?;

        let config = NativePluginConfig {
            allow_unsigned: false,
            require_signatures: true,
        };

        // Should fail verification
        let result = NativePlugin::verify_signature(&plugin_path, &trust_store, &config);
        assert!(result.is_err());

        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.contains("verification failed"),
            "Error should mention verification failed: {}",
            err_msg
        );

        Ok(())
    }

    #[test]
    fn test_verify_signature_distrusted_signer() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let plugin_path = temp_dir.path().join("test_plugin.so");
        let plugin_content = b"fake plugin content";

        // Create a fake plugin file
        std::fs::write(&plugin_path, plugin_content)?;

        // Generate a keypair and sign the plugin
        let keypair = KeyPair::generate()?;
        let metadata = Ed25519Signer::sign_with_metadata(
            plugin_content,
            &keypair,
            "Distrusted Signer",
            ContentType::Plugin,
            None,
        )?;

        // Write the signature file
        racing_wheel_service::crypto::utils::create_detached_signature(&plugin_path, &metadata)?;

        // Add the public key to the trust store as DISTRUSTED
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Distrusted,
            Some("Compromised key".to_string()),
        )?;

        let config = NativePluginConfig {
            allow_unsigned: false,
            require_signatures: true,
        };

        // Should reject plugin signed by distrusted key
        let result = NativePlugin::verify_signature(&plugin_path, &trust_store, &config);
        assert!(result.is_err());

        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.contains("distrusted"),
            "Error should mention distrusted: {}",
            err_msg
        );

        Ok(())
    }

    #[test]
    fn test_native_plugin_load_error_display() -> Result<(), Box<dyn std::error::Error>> {
        // Test that error types display correctly
        let abi_error = NativePluginLoadError::AbiMismatch {
            expected: 1,
            actual: 2,
        };
        assert!(format!("{:?}", abi_error).contains("AbiMismatch"));

        let unsigned_error = NativePluginLoadError::UnsignedPlugin {
            path: "/path/to/plugin.so".to_string(),
        };
        assert!(format!("{:?}", unsigned_error).contains("UnsignedPlugin"));

        let untrusted_error = NativePluginLoadError::UntrustedSigner {
            fingerprint: "abc123".to_string(),
        };
        assert!(format!("{:?}", untrusted_error).contains("UntrustedSigner"));

        Ok(())
    }

    #[test]
    fn test_native_plugin_host_with_custom_config() -> Result<(), Box<dyn std::error::Error>> {
        let trust_store = TrustStore::new_in_memory();
        let config = NativePluginConfig {
            allow_unsigned: true,
            require_signatures: false,
        };

        let host = NativePluginHost::new(trust_store, config);

        assert!(host.config().allow_unsigned);
        assert!(!host.config().require_signatures);

        Ok(())
    }

    #[test]
    fn test_native_plugin_host_set_config() -> Result<(), Box<dyn std::error::Error>> {
        let trust_store = TrustStore::new_in_memory();
        let initial_config = NativePluginConfig {
            allow_unsigned: true,
            require_signatures: false,
        };

        let mut host = NativePluginHost::new(trust_store, initial_config);

        // Update config
        let new_config = NativePluginConfig {
            allow_unsigned: false,
            require_signatures: true,
        };
        host.set_config(new_config);

        assert!(!host.config().allow_unsigned);
        assert!(host.config().require_signatures);

        Ok(())
    }
}
