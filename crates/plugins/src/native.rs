//! Native plugin system with SPSC shared memory and RT watchdog

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam::channel::{bounded, Receiver, Sender};
use libloading::{Library, Symbol};
use shared_memory::{Shmem, ShmemConf};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

use crate::capability::CapabilityChecker;
use crate::manifest::{PluginManifest, PluginOperation};
use crate::{Plugin, PluginContext, PluginError, PluginOutput, PluginResult};
use racing_wheel_schemas::telemetry::NormalizedTelemetry;

/// Native plugin ABI version
const PLUGIN_ABI_VERSION: u32 = 1;

/// Maximum shared memory size (4MB)
const MAX_SHARED_MEMORY_SIZE: usize = 4 * 1024 * 1024;

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
    library: Library,
    vtable: PluginVTable,
    plugin_state: *mut std::ffi::c_void,
    capability_checker: CapabilityChecker,
    helper_process: Option<NativePluginHelper>,
}

unsafe impl Send for NativePlugin {}
unsafe impl Sync for NativePlugin {}

impl NativePlugin {
    /// Load a native plugin from shared library
    pub async fn load(manifest: PluginManifest, library_path: &Path) -> PluginResult<Self> {
        // Load the shared library
        let library = unsafe {
            Library::new(library_path)
                .map_err(|e| PluginError::LoadingFailed(format!("Library load failed: {}", e)))?
        };
        
        // Get the plugin vtable
        let get_vtable: Symbol<extern "C" fn() -> PluginVTable> = unsafe {
            library
                .get(b"get_plugin_vtable")
                .map_err(|e| PluginError::LoadingFailed(format!("Missing vtable function: {}", e)))?
        };
        
        let vtable = get_vtable();
        
        // Check ABI version
        if vtable.abi_version != PLUGIN_ABI_VERSION {
            return Err(PluginError::LoadingFailed(format!(
                "ABI version mismatch: expected {}, got {}",
                PLUGIN_ABI_VERSION, vtable.abi_version
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
        
        Ok(Self {
            manifest,
            library,
            vtable,
            plugin_state,
            capability_checker,
            helper_process: None,
        })
    }
    
    /// Start helper process for RT operations
    pub async fn start_helper_process(&mut self) -> PluginResult<()> {
        let helper = NativePluginHelper::new(
            self.manifest.id,
            self.manifest.constraints.max_execution_time_us,
        ).await?;
        
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
            return Err(PluginError::NativePlugin(libloading::Error::GetSymbol {
                symbol: "process".into(),
            }));
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
            timestamp_ns: chrono::Utc::now().timestamp_nanos() as u64,
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
        _input: &racing_wheel_engine::led_haptics::LedMappingInput,
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
                timestamp_ns: chrono::Utc::now().timestamp_nanos() as u64,
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
    plugin_id: uuid::Uuid,
    process: Child,
    shared_memory: Shmem,
    frame_sender: Sender<PluginFrame>,
    result_receiver: Receiver<PluginFrame>,
    budget_us: u32,
}

impl NativePluginHelper {
    /// Create a new helper process
    pub async fn new(plugin_id: uuid::Uuid, budget_us: u32) -> PluginResult<Self> {
        // Create shared memory
        let shmem_size = std::mem::size_of::<SharedMemoryHeader>() 
            + (std::mem::size_of::<PluginFrame>() * 1024); // Ring buffer for 1024 frames
        
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
        
        // Start helper process
        let process = Command::new("wheel-plugin-helper")
            .arg("--plugin-id")
            .arg(plugin_id.to_string())
            .arg("--shmem-id")
            .arg(shared_memory.get_os_id())
            .arg("--budget-us")
            .arg(budget_us.to_string())
            .spawn()
            .map_err(|e| PluginError::Ipc(format!("Helper process spawn failed: {}", e)))?;
        
        // Create communication channels
        let (frame_sender, _) = bounded(1024);
        let (_, result_receiver) = bounded(1024);
        
        Ok(Self {
            plugin_id,
            process,
            shared_memory,
            frame_sender,
            result_receiver,
            budget_us,
        })
    }
    
    /// Process a frame through the helper
    pub async fn process_frame(&mut self, frame: PluginFrame) -> PluginResult<PluginFrame> {
        // Send frame to helper process via shared memory
        self.write_frame_to_shared_memory(frame)?;
        
        // Wait for result with timeout
        let timeout = Duration::from_micros(self.budget_us as u64 * 2); // 2x budget for safety
        
        tokio::time::timeout(timeout, async {
            self.read_frame_from_shared_memory()
        })
        .await
        .map_err(|_| PluginError::ExecutionTimeout { duration: timeout })?
    }
    
    /// Shutdown the helper process
    pub async fn shutdown(&mut self) -> PluginResult<()> {
        // Set shutdown flag
        unsafe {
            let header = self.shared_memory.as_ptr() as *mut SharedMemoryHeader;
            (*header).shutdown_flag.store(true, Ordering::Relaxed);
        }
        
        // Wait for process to exit
        let _ = self.process.wait().await;
        
        Ok(())
    }
    
    fn write_frame_to_shared_memory(&self, frame: PluginFrame) -> PluginResult<()> {
        unsafe {
            let header = self.shared_memory.as_ptr() as *mut SharedMemoryHeader;
            let frames_ptr = (header as *mut u8).add(std::mem::size_of::<SharedMemoryHeader>()) as *mut PluginFrame;
            
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
            (*header).producer_seq.store(producer_seq.wrapping_add(1), Ordering::Release);
        }
        
        Ok(())
    }
    
    fn read_frame_from_shared_memory(&self) -> PluginResult<PluginFrame> {
        unsafe {
            let header = self.shared_memory.as_ptr() as *mut SharedMemoryHeader;
            let frames_ptr = (header as *mut u8).add(std::mem::size_of::<SharedMemoryHeader>()) as *mut PluginFrame;
            
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
            (*header).consumer_seq.store(consumer_seq.wrapping_add(1), Ordering::Release);
            
            Ok(frame)
        }
    }
}

/// Native plugin host manager
pub struct NativePluginHost {
    plugins: Arc<RwLock<HashMap<uuid::Uuid, NativePlugin>>>,
}

impl NativePluginHost {
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Load a native plugin
    pub async fn load_plugin(
        &self,
        manifest: PluginManifest,
        library_path: &Path,
    ) -> PluginResult<uuid::Uuid> {
        let plugin = NativePlugin::load(manifest.clone(), library_path).await?;
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
}