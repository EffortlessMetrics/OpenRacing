//! Real-time Force Feedback Engine with Integrated Safety
//!
//! This module implements the core real-time engine that processes force feedback
//! at 1kHz with integrated safety systems, SPSC communication rings, and fault handling.

use crate::{
    metrics::AtomicCounters,
    pipeline::{CompiledPipeline, Pipeline},
    ports::{HidDevice, NormalizedTelemetry},
    rt::FFBMode,
    rt::{Frame, PerformanceMetrics, RTError, RTResult},
    safety::integration::{FaultManagerContext, IntegratedFaultManager},
    safety::{FaultType, SafetyService, SafetyState},
    scheduler::{AbsoluteScheduler, JitterMetrics, RTSetup},
    tracing::{RTTraceEvent, TracingManager},
};
use crossbeam::channel::{Receiver, Sender, TrySendError};
use racing_wheel_schemas::prelude::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Instant,
};
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

/// SPSC ring buffer size (power of 2 for efficiency)
const RING_BUFFER_SIZE: usize = 4096;

/// Maximum processing time budget per tick (200µs)
const MAX_PROCESSING_TIME_US: u64 = 200;

/// Input data from game to engine
#[derive(Debug, Clone)]
pub struct GameInput {
    /// FFB scalar from game (-1.0 to 1.0)
    pub ffb_scalar: f32,
    /// Normalized telemetry data
    pub telemetry: Option<NormalizedTelemetry>,
    /// Timestamp when input was generated
    pub timestamp: Instant,
}

/// Blackbox recording data
#[derive(Debug, Clone)]
pub struct BlackboxFrame {
    /// RT frame data
    pub frame: Frame,
    /// Per-node filter outputs (for debugging)
    pub node_outputs: Vec<f32>,
    /// Safety state at time of frame
    pub safety_state: SafetyState,
    /// Processing time for this frame (µs)
    pub processing_time_us: u64,
}

/// Engine configuration
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Device ID this engine is managing
    pub device_id: DeviceId,
    /// FFB mode to operate in
    pub mode: FFBMode,
    /// Maximum safe torque (Nm)
    pub max_safe_torque_nm: f32,
    /// Maximum high torque (Nm)
    pub max_high_torque_nm: f32,
    /// Enable blackbox recording
    pub enable_blackbox: bool,
    /// RT setup configuration
    pub rt_setup: RTSetup,
}

/// Engine statistics and health information
#[derive(Debug, Clone)]
pub struct EngineStats {
    /// Total frames processed
    pub total_frames: u64,
    /// Frames dropped due to timing violations
    pub dropped_frames: u64,
    /// Current jitter metrics
    pub jitter_metrics: JitterMetrics,
    /// Safety fault count by type
    pub fault_counts: std::collections::HashMap<FaultType, u32>,
    /// Current safety state
    pub safety_state: SafetyState,
    /// Last update timestamp
    pub last_update: Instant,
}

/// Commands sent to the RT engine
#[derive(Debug)]
pub enum EngineCommand {
    /// Apply new filter pipeline
    ApplyPipeline {
        pipeline: CompiledPipeline,
        response: oneshot::Sender<Result<(), String>>,
    },
    /// Update safety state
    UpdateSafety { hands_on: bool, device_temp_c: u8 },
    /// Request engine shutdown
    Shutdown,
    /// Get current statistics
    GetStats {
        response: oneshot::Sender<EngineStats>,
    },
}

/// Diagnostic signals sent from RT thread to diagnostic thread
#[derive(Debug, Clone)]
pub enum DiagnosticSignal {
    /// HID write error occurred
    HidWriteError {
        timestamp: Instant,
        torque_nm: f32,
        seq: u16,
    },
}

/// Real-time Force Feedback Engine
pub struct Engine {
    /// Engine configuration
    config: EngineConfig,

    /// RT thread handle
    rt_thread: Option<JoinHandle<RTResult>>,

    /// Diagnostic thread handle
    diagnostic_thread: Option<JoinHandle<()>>,

    /// Command channel to RT thread
    command_tx: Option<Sender<EngineCommand>>,

    /// Game input channel
    game_input_tx: Option<Sender<GameInput>>,

    /// Blackbox output channel
    blackbox_rx: Option<Receiver<BlackboxFrame>>,

    /// Engine running flag
    running: Arc<AtomicBool>,

    /// Frame counter (atomic for thread-safe access)
    frame_counter: Arc<AtomicU64>,

    /// Atomic counters for metrics collection
    atomic_counters: Arc<AtomicCounters>,
}

/// RT thread context (all data needed by RT thread)
struct RTContext {
    /// Device for HID output
    device: Box<dyn HidDevice>,

    /// Current filter pipeline
    pipeline: Pipeline,

    /// Absolute scheduler for 1kHz timing
    scheduler: AbsoluteScheduler,

    /// Safety service
    safety: SafetyService,

    /// Integrated fault manager
    fault_manager: IntegratedFaultManager,

    /// Game input receiver (SPSC)
    game_input_rx: Receiver<GameInput>,

    /// Command receiver
    command_rx: Receiver<EngineCommand>,

    /// Blackbox output sender (SPSC)
    blackbox_tx: Option<Sender<BlackboxFrame>>,

    /// Diagnostic signal sender (non-blocking)
    diagnostic_tx: Option<Sender<DiagnosticSignal>>,

    /// Engine configuration
    config: EngineConfig,

    /// Running flag
    running: Arc<AtomicBool>,

    /// Frame counter
    frame_counter: Arc<AtomicU64>,

    /// Current frame sequence number
    seq: u16,

    /// Performance metrics
    metrics: PerformanceMetrics,

    /// Atomic counters for RT-safe metrics collection
    atomic_counters: Arc<AtomicCounters>,

    /// Tracing manager for observability
    tracing_manager: Option<TracingManager>,
}

impl Engine {
    /// Create new engine with device and configuration
    pub fn new(_device: Box<dyn HidDevice>, config: EngineConfig) -> Result<Self, String> {
        info!("Creating new RT engine for device {:?}", config.device_id);

        Ok(Self {
            config,
            rt_thread: None,
            diagnostic_thread: None,
            command_tx: None,
            game_input_tx: None,
            blackbox_rx: None,
            running: Arc::new(AtomicBool::new(false)),
            frame_counter: Arc::new(AtomicU64::new(0)),
            atomic_counters: Arc::new(AtomicCounters::new()),
        })
    }

    /// Start the real-time engine
    pub async fn start(&mut self, device: Box<dyn HidDevice>) -> Result<(), String> {
        if self.running.load(Ordering::Acquire) {
            return Err("Engine already running".to_string());
        }

        info!("Starting RT engine for device {:?}", self.config.device_id);

        // Create communication channels
        let (command_tx, command_rx) = crossbeam::channel::bounded(64);
        let (game_input_tx, game_input_rx) = crossbeam::channel::bounded(RING_BUFFER_SIZE);

        let (blackbox_tx, blackbox_rx) = if self.config.enable_blackbox {
            let (tx, rx) = crossbeam::channel::bounded(RING_BUFFER_SIZE);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        // Create diagnostic channel for RT-safe error reporting
        let (diagnostic_tx, diagnostic_rx) = crossbeam::channel::bounded(1024);

        // Initialize tracing manager
        let tracing_manager = match TracingManager::new() {
            Ok(mut manager) => {
                if let Err(e) = manager.initialize() {
                    warn!("Failed to initialize tracing: {}", e);
                    None
                } else {
                    Some(manager)
                }
            }
            Err(e) => {
                warn!("Failed to create tracing manager: {}", e);
                None
            }
        };

        // Create RT context
        let mut rt_context = RTContext {
            device,
            pipeline: Pipeline::new(),
            scheduler: AbsoluteScheduler::new_1khz(),
            safety: SafetyService::new(
                self.config.max_safe_torque_nm,
                self.config.max_high_torque_nm,
            ),
            fault_manager: IntegratedFaultManager::new(
                self.config.max_safe_torque_nm,
                self.config.max_high_torque_nm,
                crate::safety::watchdog::WatchdogConfig::default(),
            ),
            game_input_rx,
            command_rx,
            blackbox_tx,
            diagnostic_tx: Some(diagnostic_tx),
            config: self.config.clone(),
            running: Arc::clone(&self.running),
            frame_counter: Arc::clone(&self.frame_counter),
            seq: 0,
            metrics: PerformanceMetrics::default(),
            atomic_counters: Arc::clone(&self.atomic_counters),
            tracing_manager,
        };

        // Apply RT setup
        rt_context
            .scheduler
            .apply_rt_setup(&self.config.rt_setup)
            .map_err(|e| format!("Failed to apply RT setup: {:?}", e))?;

        // Mark as running
        self.running.store(true, Ordering::Release);

        // Start diagnostic thread for off-thread error reporting
        let diagnostic_running = Arc::clone(&self.running);
        let diagnostic_counters = Arc::clone(&self.atomic_counters);
        let diagnostic_thread = thread::Builder::new()
            .name(format!("diagnostic-{:?}", self.config.device_id))
            .spawn(move || {
                Self::diagnostic_thread_main(diagnostic_rx, diagnostic_running, diagnostic_counters)
            })
            .map_err(|e| format!("Failed to spawn diagnostic thread: {}", e))?;

        // Start RT thread
        let rt_thread = thread::Builder::new()
            .name(format!("rt-engine-{:?}", self.config.device_id))
            .spawn(move || Self::rt_thread_main(rt_context))
            .map_err(|e| format!("Failed to spawn RT thread: {}", e))?;

        // Store handles
        self.rt_thread = Some(rt_thread);
        self.diagnostic_thread = Some(diagnostic_thread);
        self.command_tx = Some(command_tx);
        self.game_input_tx = Some(game_input_tx);
        self.blackbox_rx = blackbox_rx;

        info!("RT engine started successfully");
        Ok(())
    }

    /// Stop the real-time engine
    pub async fn stop(&mut self) -> Result<(), String> {
        self.stop_blocking()
    }

    fn stop_blocking(&mut self) -> Result<(), String> {
        if !self.running.load(Ordering::Acquire) {
            return Ok(()); // Already stopped
        }

        info!("Stopping RT engine");

        // Signal shutdown
        self.running.store(false, Ordering::Release);

        // Send shutdown command
        if let Some(ref command_tx) = self.command_tx {
            let _ = command_tx.try_send(EngineCommand::Shutdown);
        }

        // Wait for RT thread to finish
        if let Some(rt_thread) = self.rt_thread.take() {
            match rt_thread.join() {
                Ok(result) => match result {
                    Ok(()) => info!("RT thread stopped cleanly"),
                    Err(e) => warn!("RT thread stopped with error: {:?}", e),
                },
                Err(_) => error!("RT thread panicked"),
            }
        }

        // Wait for diagnostic thread to finish
        if let Some(diagnostic_thread) = self.diagnostic_thread.take() {
            match diagnostic_thread.join() {
                Ok(()) => info!("Diagnostic thread stopped cleanly"),
                Err(_) => error!("Diagnostic thread panicked"),
            }
        }

        // Clear handles
        self.command_tx = None;
        self.game_input_tx = None;
        self.blackbox_rx = None;

        info!("RT engine stopped");
        Ok(())
    }

    /// Send game input to the engine
    pub fn send_game_input(&self, input: GameInput) -> Result<(), String> {
        if let Some(ref tx) = self.game_input_tx {
            match tx.try_send(input) {
                Ok(()) => Ok(()),
                Err(TrySendError::Full(_)) => {
                    warn!("Game input ring buffer full, dropping frame");
                    Err("Input buffer full".to_string())
                }
                Err(TrySendError::Disconnected(_)) => Err("Engine not running".to_string()),
            }
        } else {
            Err("Engine not started".to_string())
        }
    }

    /// Apply new filter pipeline
    pub async fn apply_pipeline(&self, pipeline: CompiledPipeline) -> Result<(), String> {
        if let Some(ref command_tx) = self.command_tx {
            let (response_tx, response_rx) = oneshot::channel();

            command_tx
                .try_send(EngineCommand::ApplyPipeline {
                    pipeline,
                    response: response_tx,
                })
                .map_err(|_| "Failed to send pipeline command")?;

            response_rx
                .await
                .map_err(|_| "Pipeline command response lost")?
        } else {
            Err("Engine not started".to_string())
        }
    }

    /// Update safety parameters
    pub fn update_safety(&self, hands_on: bool, device_temp_c: u8) -> Result<(), String> {
        if let Some(ref command_tx) = self.command_tx {
            command_tx
                .try_send(EngineCommand::UpdateSafety {
                    hands_on,
                    device_temp_c,
                })
                .map_err(|_| "Failed to send safety update")?;
            Ok(())
        } else {
            Err("Engine not started".to_string())
        }
    }

    /// Get current engine statistics
    pub async fn get_stats(&self) -> Result<EngineStats, String> {
        if let Some(ref command_tx) = self.command_tx {
            let (response_tx, response_rx) = oneshot::channel();

            command_tx
                .try_send(EngineCommand::GetStats {
                    response: response_tx,
                })
                .map_err(|_| "Failed to send stats command")?;

            response_rx
                .await
                .map_err(|_| "Stats command response lost".to_string())
        } else {
            Err("Engine not started".to_string())
        }
    }

    /// Get blackbox frames (non-blocking)
    pub fn get_blackbox_frames(&self) -> Vec<BlackboxFrame> {
        if let Some(ref rx) = self.blackbox_rx {
            let mut frames = Vec::new();
            while let Ok(frame) = rx.try_recv() {
                frames.push(frame);
            }
            frames
        } else {
            Vec::new()
        }
    }

    /// Check if engine is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Get current frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_counter.load(Ordering::Acquire)
    }

    /// Get atomic counters for metrics collection
    pub fn atomic_counters(&self) -> Arc<AtomicCounters> {
        Arc::clone(&self.atomic_counters)
    }

    /// Real-time thread main loop
    fn rt_thread_main(mut ctx: RTContext) -> RTResult {
        info!("RT thread started for device {:?}", ctx.config.device_id);

        let mut last_game_input = GameInput {
            ffb_scalar: 0.0,
            telemetry: None,
            timestamp: Instant::now(),
        };

        // Main RT loop
        while ctx.running.load(Ordering::Acquire) {
            let tick_start = Instant::now();
            let tick_start_ns = tick_start.elapsed().as_nanos() as u64;

            // Emit RT trace event for tick start
            if let Some(ref tracer) = ctx.tracing_manager {
                tracer.emit_rt_event(RTTraceEvent::TickStart {
                    tick_count: ctx.frame_counter.load(Ordering::Relaxed),
                    timestamp_ns: tick_start_ns,
                });
            }

            // Wait for next 1kHz tick
            let tick_count = match ctx.scheduler.wait_for_tick() {
                Ok(count) => {
                    // Increment tick counter (RT-safe)
                    ctx.atomic_counters.inc_tick();
                    count
                }
                Err(RTError::TimingViolation) => {
                    ctx.metrics.missed_ticks += 1;
                    // Increment missed tick counter (RT-safe)
                    ctx.atomic_counters.inc_missed_tick();

                    // Emit RT trace event for deadline miss
                    if let Some(ref tracer) = ctx.tracing_manager {
                        tracer.emit_rt_event(RTTraceEvent::DeadlineMiss {
                            tick_count: ctx.frame_counter.load(Ordering::Relaxed),
                            timestamp_ns: tick_start_ns,
                            jitter_ns: tick_start.elapsed().as_nanos() as u64,
                        });
                    }

                    ctx.safety.report_fault(FaultType::TimingViolation);
                    continue;
                }
                Err(e) => {
                    error!("Scheduler error: {:?}", e);
                    return Err(e);
                }
            };

            // Process commands (non-blocking)
            Self::process_commands(&mut ctx)?;

            // Get latest game input (non-blocking)
            if let Ok(input) = ctx.game_input_rx.try_recv() {
                last_game_input = input;
            }

            // Create frame for processing
            let mut frame = Frame {
                ffb_in: last_game_input.ffb_scalar,
                torque_out: 0.0,
                wheel_speed: last_game_input
                    .telemetry
                    .as_ref()
                    .map(|t| t.speed_ms * 3.6) // Convert m/s to km/h for display
                    .unwrap_or(0.0),
                hands_off: false, // Will be updated by hands-off detector
                ts_mono_ns: tick_start.elapsed().as_nanos() as u64,
                seq: ctx.seq,
            };

            // Process through filter pipeline
            let pipeline_start = Instant::now();

            // Convert rt::Frame to ffb::Frame for pipeline processing
            let mut ffb_frame = crate::rt::Frame {
                ffb_in: frame.ffb_in,
                torque_out: frame.torque_out,
                wheel_speed: frame.wheel_speed,
                hands_off: frame.hands_off,
                ts_mono_ns: frame.ts_mono_ns,
                seq: frame.seq,
            };

            if let Err(e) = ctx.pipeline.process(&mut ffb_frame) {
                error!("Pipeline processing failed: {:?}", e);
                ctx.safety.report_fault(FaultType::TimingViolation); // Use existing fault type
                continue;
            }

            // Copy results back to rt::Frame
            frame.torque_out = ffb_frame.torque_out;
            frame.hands_off = ffb_frame.hands_off;

            let _pipeline_time = pipeline_start.elapsed();

            // Apply safety limits
            let max_torque =
                ctx.safety.get_max_torque(true).value() / ctx.config.max_high_torque_nm;
            let clamped_torque = frame.torque_out.clamp(-max_torque, max_torque);

            // Record torque saturation (RT-safe)
            let is_saturated = (frame.torque_out.abs() - clamped_torque.abs()).abs() > 0.001;
            ctx.atomic_counters.record_torque_saturation(is_saturated);

            frame.torque_out = clamped_torque;

            // Check for safety violations
            if frame.torque_out.abs() > max_torque {
                ctx.safety.report_fault(FaultType::SafetyInterlockViolation);
            }

            // Write to device
            let device_write_start = Instant::now();
            let final_torque_nm = frame.torque_out * ctx.config.max_high_torque_nm;

            // Handle write_ffb_report Result with RT-safe error counting
            if ctx
                .device
                .write_ffb_report(final_torque_nm, ctx.seq)
                .is_err()
            {
                // Non-allocating error accounting - just increment atomic counter
                ctx.atomic_counters.inc_hid_write_error();

                // Send lossy diagnostic signal to side thread (non-blocking)
                if let Some(ref diagnostic_tx) = ctx.diagnostic_tx {
                    let signal = DiagnosticSignal::HidWriteError {
                        timestamp: device_write_start,
                        torque_nm: final_torque_nm,
                        seq: ctx.seq,
                    };
                    // Use try_send to avoid blocking RT thread
                    let _ = diagnostic_tx.try_send(signal);
                }
            }

            let _device_write_time = device_write_start.elapsed();

            // Emit RT trace event for HID write
            if let Some(ref tracer) = ctx.tracing_manager {
                tracer.emit_rt_event(RTTraceEvent::HidWrite {
                    tick_count,
                    timestamp_ns: device_write_start.elapsed().as_nanos() as u64,
                    torque_nm: final_torque_nm,
                    seq: ctx.seq,
                });
            }

            // Update metrics
            ctx.frame_counter.store(tick_count, Ordering::Release);
            ctx.metrics.total_ticks = tick_count;
            ctx.seq = ctx.seq.wrapping_add(1);

            // Check timing budget
            let total_processing_time = tick_start.elapsed();
            let processing_time_us = total_processing_time.as_micros() as u64;

            if processing_time_us > MAX_PROCESSING_TIME_US {
                warn!(
                    "Processing time exceeded budget: {}µs > {}µs",
                    processing_time_us, MAX_PROCESSING_TIME_US
                );
                ctx.safety.report_fault(FaultType::TimingViolation);
            }

            // Emit RT trace event for tick end
            if let Some(ref tracer) = ctx.tracing_manager {
                tracer.emit_rt_event(RTTraceEvent::TickEnd {
                    tick_count,
                    timestamp_ns: tick_start.elapsed().as_nanos() as u64,
                    processing_time_ns: total_processing_time.as_nanos() as u64,
                });
            }

            // Record blackbox data if enabled
            if let Some(ref blackbox_tx) = ctx.blackbox_tx {
                let blackbox_frame = BlackboxFrame {
                    frame,
                    node_outputs: Vec::new(), // TODO: Collect from pipeline
                    safety_state: ctx.safety.state().clone(),
                    processing_time_us: total_processing_time.as_micros() as u64,
                };

                // Non-blocking send to avoid affecting RT timing
                let _ = blackbox_tx.try_send(blackbox_frame);
            }

            // Update fault manager with context
            let fault_context = FaultManagerContext {
                current_torque: frame.torque_out * ctx.config.max_high_torque_nm,
                temperature: None,   // Would be read from device telemetry
                encoder_value: None, // Would be read from device telemetry
                timing_jitter_us: Some(total_processing_time.as_micros() as u64),
                usb_info: None,
                plugin_execution: None,
                component_heartbeats: std::collections::HashMap::new(),
                frame: Some(crate::rt::Frame {
                    ffb_in: frame.ffb_in,
                    torque_out: frame.torque_out,
                    wheel_speed: frame.wheel_speed,
                    hands_off: frame.hands_off,
                    ts_mono_ns: frame.ts_mono_ns,
                    seq: frame.seq,
                }),
            };
            let _fault_result = ctx.fault_manager.update(&fault_context);
        }

        info!("RT thread stopping");
        Ok(())
    }

    /// Process commands from main thread (RT-safe, non-blocking)
    fn process_commands(ctx: &mut RTContext) -> RTResult {
        // Process all available commands without blocking
        while let Ok(command) = ctx.command_rx.try_recv() {
            match command {
                EngineCommand::ApplyPipeline { pipeline, response } => {
                    debug!("Applying new pipeline with hash {:x}", pipeline.config_hash);
                    ctx.pipeline.swap_at_tick_boundary(pipeline.pipeline);
                    let _ = response.send(Ok(()));
                }

                EngineCommand::UpdateSafety {
                    hands_on,
                    device_temp_c,
                } => {
                    let _ = ctx.safety.update_hands_on_status(hands_on);

                    // Check thermal limits
                    if device_temp_c > 80 {
                        ctx.safety.report_fault(FaultType::ThermalLimit);
                    }
                }

                EngineCommand::Shutdown => {
                    info!("Received shutdown command");
                    ctx.running.store(false, Ordering::Release);
                    return Ok(());
                }

                EngineCommand::GetStats { response } => {
                    let stats = EngineStats {
                        total_frames: ctx.frame_counter.load(Ordering::Acquire),
                        dropped_frames: ctx.metrics.missed_ticks,
                        jitter_metrics: ctx.scheduler.metrics().clone(),
                        fault_counts: std::collections::HashMap::new(), // TODO: Get from fault manager
                        safety_state: ctx.safety.state().clone(),
                        last_update: Instant::now(),
                    };
                    let _ = response.send(stats);
                }
            }
        }

        Ok(())
    }

    /// Diagnostic thread main loop - publishes aggregated error counts at 1-2 Hz
    fn diagnostic_thread_main(
        diagnostic_rx: Receiver<DiagnosticSignal>,
        running: Arc<AtomicBool>,
        _counters: Arc<AtomicCounters>,
    ) {
        info!("Diagnostic thread started");

        let mut last_publish = Instant::now();
        const PUBLISH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500); // 2 Hz

        while running.load(Ordering::Acquire) {
            // Process diagnostic signals with timeout
            match diagnostic_rx.recv_timeout(PUBLISH_INTERVAL) {
                Ok(signal) => {
                    match signal {
                        DiagnosticSignal::HidWriteError {
                            timestamp,
                            torque_nm,
                            seq,
                        } => {
                            // Log HID write error (safe to do off RT thread)
                            debug!(
                                "HID write error at seq={}, torque={:.2}Nm, timestamp={:?}",
                                seq, torque_nm, timestamp
                            );
                        }
                    }
                }
                Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
                    // Timeout is expected - continue to publish interval check
                }
                Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
                    // RT thread disconnected, exit
                    break;
                }
            }

            // Publish aggregated diagnostics at regular intervals
            let now = Instant::now();
            if now.duration_since(last_publish) >= PUBLISH_INTERVAL {
                // Here we could publish aggregated error counts to external systems
                // For now, we just update the timestamp
                last_publish = now;
            }
        }

        info!("Diagnostic thread stopping");
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if self.is_running() {
            warn!("Engine dropped while still running - forcing stop");
            let _ = self.stop_blocking();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::device::VirtualDevice;
    use tokio::time::{Duration as TokioDuration, sleep};

    #[track_caller]
    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("unexpected Err: {e:?}"),
        }
    }

    fn create_test_device() -> Box<dyn HidDevice> {
        let device_id = must("test-device".parse::<DeviceId>());
        let virtual_device = VirtualDevice::new(device_id, "Test Device".to_string());
        Box::new(virtual_device)
    }

    fn create_test_config() -> EngineConfig {
        EngineConfig {
            device_id: must("test-device".parse::<DeviceId>()),
            mode: FFBMode::RawTorque,
            max_safe_torque_nm: 5.0,
            max_high_torque_nm: 25.0,
            enable_blackbox: true,
            rt_setup: RTSetup::default(),
        }
    }

    #[tokio::test]
    async fn test_engine_creation() {
        let device = create_test_device();
        let config = create_test_config();

        let engine = must(Engine::new(device, config));
        assert!(!engine.is_running());
        assert_eq!(engine.frame_count(), 0);
    }

    #[tokio::test]
    async fn test_engine_start_stop() {
        let device = create_test_device();
        let config = create_test_config();

        let mut engine = must(Engine::new(device, config));

        // Start engine
        let device = create_test_device();
        let result = engine.start(device).await;
        assert!(result.is_ok());
        assert!(engine.is_running());

        // Give it a moment to initialize
        sleep(TokioDuration::from_millis(10)).await;

        // Stop engine
        let result = engine.stop().await;
        assert!(result.is_ok());
        assert!(!engine.is_running());
    }

    #[tokio::test]
    async fn test_game_input_processing() {
        let device = create_test_device();
        let config = create_test_config();

        let mut engine = must(Engine::new(device, config));
        let device = create_test_device();
        engine.start(device).await.unwrap();

        // Send some game input
        let input = GameInput {
            ffb_scalar: 0.5,
            telemetry: None,
            timestamp: Instant::now(),
        };

        let result = engine.send_game_input(input);
        assert!(result.is_ok());

        // Give engine time to process at least one frame.
        let result = tokio::time::timeout(TokioDuration::from_millis(100), async {
            while engine.frame_count() == 0 {
                sleep(TokioDuration::from_millis(5)).await;
            }
        })
        .await;
        assert!(result.is_ok(), "engine did not process frames within 100ms");

        engine.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_pipeline_application() {
        let device = create_test_device();
        let config = create_test_config();

        let mut engine = must(Engine::new(device, config));
        let device = create_test_device();
        engine.start(device).await.unwrap();

        // Create a test pipeline (simplified without compiler dependency)
        let compiled = CompiledPipeline {
            pipeline: Pipeline::new(),
            config_hash: 0x12345678,
        };

        // Apply pipeline
        let result = engine.apply_pipeline(compiled).await;
        assert!(result.is_ok());

        engine.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_safety_updates() {
        let device = create_test_device();
        let config = create_test_config();

        let mut engine = must(Engine::new(device, config));
        let device = create_test_device();
        engine.start(device).await.unwrap();

        // Update safety parameters
        let result = engine.update_safety(true, 60);
        assert!(result.is_ok());

        // Update with high temperature (should trigger thermal fault)
        let result = engine.update_safety(false, 85);
        assert!(result.is_ok());

        engine.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_engine_stats() {
        let device = create_test_device();
        let config = create_test_config();

        let mut engine = must(Engine::new(device, config));
        let device = create_test_device();
        engine.start(device).await.unwrap();

        // Give engine time to process some frames
        sleep(TokioDuration::from_millis(50)).await;

        // Get stats
        let stats = engine.get_stats().await;
        assert!(stats.is_ok());

        let stats = stats.unwrap();
        assert!(stats.total_frames > 0);

        engine.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_blackbox_recording() {
        let device = create_test_device();
        let config = create_test_config();

        let mut engine = must(Engine::new(device, config));
        let device = create_test_device();
        engine.start(device).await.unwrap();

        // Send some input to generate frames
        for i in 0..10 {
            let input = GameInput {
                ffb_scalar: (i as f32) * 0.1,
                telemetry: None,
                timestamp: Instant::now(),
            };
            let _ = engine.send_game_input(input);
        }

        // Give engine time to process
        sleep(TokioDuration::from_millis(50)).await;

        // Check blackbox frames
        let frames = engine.get_blackbox_frames();
        assert!(!frames.is_empty());

        engine.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_timing_violation_handling() {
        let device = create_test_device();
        let mut config = create_test_config();

        // Disable RT setup to avoid permission issues in tests
        config.rt_setup.high_priority = false;
        config.rt_setup.lock_memory = false;

        let mut engine = must(Engine::new(device, config));
        let device = create_test_device();
        engine.start(device).await.unwrap();

        // Run for a bit to potentially trigger timing violations in CI
        sleep(TokioDuration::from_millis(100)).await;

        let _stats = engine.get_stats().await.unwrap();

        // In CI environments, we might have timing violations
        // Just verify the engine continues running
        assert!(engine.is_running());

        engine.stop().await.unwrap();
    }
}
