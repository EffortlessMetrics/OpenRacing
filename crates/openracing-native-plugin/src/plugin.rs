//! Native plugin struct and lifecycle management.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::time::Instant;

use libloading::{Library, Symbol};
use openracing_crypto::SignatureMetadata;

use crate::abi_check::CURRENT_ABI_VERSION;
use crate::error::NativePluginError;

/// Frame data for RT communication.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PluginFrame {
    /// Input force feedback value.
    pub ffb_in: f32,
    /// Output torque value.
    pub torque_out: f32,
    /// Wheel speed in m/s.
    pub wheel_speed: f32,
    /// Timestamp in nanoseconds.
    pub timestamp_ns: u64,
    /// Execution budget in microseconds.
    pub budget_us: u32,
    /// Sequence number.
    pub sequence: u32,
}

impl Default for PluginFrame {
    fn default() -> Self {
        Self {
            ffb_in: 0.0,
            torque_out: 0.0,
            wheel_speed: 0.0,
            timestamp_ns: 0,
            budget_us: 1000,
            sequence: 0,
        }
    }
}

/// Shared memory header.
#[repr(C)]
#[derive(Debug)]
pub struct SharedMemoryHeader {
    /// Protocol version.
    pub version: u32,
    /// Producer sequence number.
    pub producer_seq: AtomicU32,
    /// Consumer sequence number.
    pub consumer_seq: AtomicU32,
    /// Size of each frame.
    pub frame_size: u32,
    /// Maximum number of frames.
    pub max_frames: u32,
    /// Shutdown flag.
    pub shutdown_flag: AtomicBool,
}

/// Native plugin function table (C ABI).
#[repr(C)]
pub struct PluginVTable {
    /// Create plugin instance.
    pub create: extern "C" fn(*const u8, usize) -> *mut c_void,
    /// Process a frame.
    pub process: extern "C" fn(*mut c_void, *mut PluginFrame) -> i32,
    /// Destroy plugin instance.
    pub destroy: extern "C" fn(*mut c_void),
    /// ABI version.
    pub abi_version: u32,
}

/// Native plugin instance.
pub struct NativePlugin {
    /// Plugin ID.
    pub id: uuid::Uuid,
    /// Plugin name.
    pub name: String,
    /// Loaded library.
    _library: Library,
    /// Function table.
    vtable: PluginVTable,
    /// Plugin state pointer.
    plugin_state: *mut c_void,
    /// ABI version.
    abi_version: u32,
    /// Signature metadata (if signed).
    signature: Option<SignatureMetadata>,
    /// Maximum execution time in microseconds.
    max_execution_time_us: u32,
}

unsafe impl Send for NativePlugin {}
unsafe impl Sync for NativePlugin {}

impl NativePlugin {
    /// Create a new native plugin from loaded components.
    ///
    /// # Safety
    ///
    /// The plugin_state pointer must be valid and owned by this plugin.
    pub(crate) unsafe fn new(
        id: uuid::Uuid,
        name: String,
        library: Library,
        vtable: PluginVTable,
        plugin_state: *mut c_void,
        signature: Option<SignatureMetadata>,
        max_execution_time_us: u32,
    ) -> Self {
        let abi_version = vtable.abi_version;
        Self {
            id,
            name,
            _library: library,
            vtable,
            plugin_state,
            abi_version,
            signature,
            max_execution_time_us,
        }
    }

    /// Load a native plugin from a shared library.
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin ID.
    /// * `name` - Plugin name.
    /// * `library_path` - Path to the shared library.
    /// * `max_execution_time_us` - Maximum execution time in microseconds.
    ///
    /// # Safety
    ///
    /// The library must export valid `get_plugin_vtable` function.
    pub unsafe fn load(
        id: uuid::Uuid,
        name: String,
        library_path: &std::path::Path,
        max_execution_time_us: u32,
    ) -> Result<Self, NativePluginError> {
        let library = unsafe { Library::new(library_path)? };

        let get_vtable: Symbol<'_, extern "C" fn() -> PluginVTable> =
            unsafe { library.get(b"get_plugin_vtable")? };

        let vtable = get_vtable();

        if vtable.abi_version != CURRENT_ABI_VERSION {
            return Err(NativePluginError::AbiMismatch {
                expected: CURRENT_ABI_VERSION,
                actual: vtable.abi_version,
            });
        }

        let config_json = serde_json::to_string(&serde_json::Value::Null)?;
        let plugin_state = (vtable.create)(config_json.as_ptr(), config_json.len());

        if plugin_state.is_null() {
            return Err(NativePluginError::InitializationFailed(
                "Plugin create returned null".to_string(),
            ));
        }

        tracing::info!(
            plugin_id = %id,
            name = %name,
            abi_version = vtable.abi_version,
            "Native plugin loaded"
        );

        Ok(unsafe {
            Self::new(
                id,
                name,
                library,
                vtable,
                plugin_state,
                None,
                max_execution_time_us,
            )
        })
    }

    /// Initialize the plugin with configuration.
    ///
    /// # Safety
    ///
    /// The plugin must have been created with a valid vtable.
    pub unsafe fn initialize(
        &mut self,
        config: &serde_json::Value,
    ) -> Result<(), NativePluginError> {
        if !self.plugin_state.is_null() {
            (self.vtable.destroy)(self.plugin_state);
        }

        let config_json = serde_json::to_string(config)?;
        let plugin_state = (self.vtable.create)(config_json.as_ptr(), config_json.len());

        if plugin_state.is_null() {
            return Err(NativePluginError::InitializationFailed(
                "Plugin create returned null during initialization".to_string(),
            ));
        }

        self.plugin_state = plugin_state;
        Ok(())
    }

    /// Process a single frame.
    ///
    /// # Safety
    ///
    /// The plugin must be properly initialized.
    pub unsafe fn process_frame(
        &mut self,
        frame: &mut PluginFrame,
    ) -> Result<(), NativePluginError> {
        let start_time = Instant::now();

        let result = (self.vtable.process)(self.plugin_state, frame as *mut PluginFrame);

        let execution_time = start_time.elapsed();

        if execution_time.as_micros() > frame.budget_us as u128 {
            return Err(NativePluginError::BudgetViolation {
                used_us: execution_time.as_micros() as u32,
                budget_us: frame.budget_us,
            });
        }

        if result != 0 {
            return Err(NativePluginError::Crashed {
                reason: format!("Plugin process returned {}", result),
            });
        }

        Ok(())
    }

    /// Get the plugin's ABI version.
    pub fn abi_version(&self) -> u32 {
        self.abi_version
    }

    /// Get the plugin's signature metadata.
    pub fn signature(&self) -> Option<&SignatureMetadata> {
        self.signature.as_ref()
    }

    /// Check if the plugin is signed.
    pub fn is_signed(&self) -> bool {
        self.signature.is_some()
    }

    /// Get the maximum execution time.
    pub fn max_execution_time_us(&self) -> u32 {
        self.max_execution_time_us
    }

    /// Verify ABI compatibility.
    pub fn verify_abi(&self) -> Result<(), NativePluginError> {
        if self.abi_version != CURRENT_ABI_VERSION {
            return Err(NativePluginError::AbiMismatch {
                expected: CURRENT_ABI_VERSION,
                actual: self.abi_version,
            });
        }
        Ok(())
    }

    /// Shutdown the plugin.
    ///
    /// # Safety
    ///
    /// The plugin must have been properly initialized.
    pub unsafe fn shutdown(&mut self) -> Result<(), NativePluginError> {
        if !self.plugin_state.is_null() {
            (self.vtable.destroy)(self.plugin_state);
            self.plugin_state = std::ptr::null_mut();
        }
        Ok(())
    }
}

impl Drop for NativePlugin {
    fn drop(&mut self) {
        if !self.plugin_state.is_null() {
            tracing::debug!(
                plugin_id = %self.id,
                "Destroying native plugin"
            );
            (self.vtable.destroy)(self.plugin_state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_frame_default() {
        let frame = PluginFrame::default();
        assert_eq!(frame.ffb_in, 0.0);
        assert_eq!(frame.torque_out, 0.0);
        assert_eq!(frame.wheel_speed, 0.0);
        assert_eq!(frame.timestamp_ns, 0);
        assert_eq!(frame.budget_us, 1000);
        assert_eq!(frame.sequence, 0);
    }

    #[test]
    fn test_plugin_frame_copy() {
        let frame = PluginFrame {
            ffb_in: 1.0,
            torque_out: 0.5,
            wheel_speed: 10.0,
            timestamp_ns: 1234567890,
            budget_us: 500,
            sequence: 42,
        };
        let frame2 = frame;
        assert_eq!(frame2.ffb_in, 1.0);
        assert_eq!(frame2.sequence, 42);
    }
}
