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
    safety::watchdog::SystemComponent,
    safety::{FaultType, SafetyService, SafetyState},
    scheduler::{AbsoluteScheduler, AdaptiveSchedulingConfig, JitterMetrics, RTSetup},
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

#[derive(Debug, Clone, Copy)]
struct TorqueControlResult {
    torque_out_nm: f32,
    saturated: bool,
}

fn sanitize_unit_interval(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn sanitize_signed_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

fn monotonic_ns_since(epoch: Instant, now: Instant) -> u64 {
    now.checked_duration_since(epoch)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
        .min(u64::MAX as u128) as u64
}

fn apply_torque_safety_controls(
    pipeline_torque_normalized: f32,
    max_high_torque_nm: f32,
    fault_torque_multiplier: f32,
    safety: &SafetyService,
    force_zero_torque: bool,
) -> TorqueControlResult {
    let safe_pipeline_torque = sanitize_signed_unit(pipeline_torque_normalized);
    let safe_max_torque_nm = if max_high_torque_nm.is_finite() && max_high_torque_nm > 0.0 {
        max_high_torque_nm
    } else {
        0.0
    };
    let safe_multiplier = sanitize_unit_interval(fault_torque_multiplier);
    let requested_torque_nm = safe_pipeline_torque * safe_max_torque_nm * safe_multiplier;

    let torque_out_nm = if force_zero_torque {
        0.0
    } else {
        safety.clamp_torque_nm(requested_torque_nm)
    };

    if force_zero_torque {
        return TorqueControlResult {
            torque_out_nm,
            saturated: requested_torque_nm.abs() > f32::EPSILON,
        };
    }

    TorqueControlResult {
        torque_out_nm,
        saturated: (requested_torque_nm - torque_out_nm).abs() > 0.001,
    }
}

fn should_latch_safety_fault(fault: FaultType) -> bool {
    matches!(
        fault,
        FaultType::UsbStall
            | FaultType::EncoderNaN
            | FaultType::Overcurrent
            | FaultType::SafetyInterlockViolation
            | FaultType::HandsOffTimeout
            | FaultType::PipelineFault
    )
}

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
    /// Emergency stop - immediately zero torque
    EmergencyStop {
        response: oneshot::Sender<Result<(), String>>,
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

    /// Monotonic epoch for RT event/frame timestamps.
    rt_epoch: Instant,

    /// Current frame sequence number
    seq: u16,

    /// Performance metrics
    metrics: PerformanceMetrics,

    /// Atomic counters for RT-safe metrics collection
    atomic_counters: Arc<AtomicCounters>,

    /// Latest soft-stop/fault torque multiplier from fault manager.
    fault_torque_multiplier: f32,

    /// Latched emergency stop state.
    emergency_stop_active: bool,

    /// Tracing manager for observability
    tracing_manager: Option<TracingManager>,

    /// Reused heartbeat map to avoid per-tick hash-map initialization in the RT loop.
    component_heartbeats_scratch: Option<std::collections::HashMap<SystemComponent, bool>>,
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
        #[cfg(feature = "rt-hardening")]
        let tracing_manager = None;

        #[cfg(not(feature = "rt-hardening"))]
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

        let rt_epoch = Instant::now();

        // Create RT context
        let mut rt_context = RTContext {
            device,
            pipeline: Pipeline::new(),
            scheduler: {
                let mut scheduler = AbsoluteScheduler::new_1khz();
                scheduler.set_adaptive_scheduling(AdaptiveSchedulingConfig {
                    enabled: true,
                    ..Default::default()
                });
                scheduler
            },
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
            rt_epoch,
            seq: 0,
            metrics: PerformanceMetrics::default(),
            atomic_counters: Arc::clone(&self.atomic_counters),
            fault_torque_multiplier: 1.0,
            emergency_stop_active: false,
            tracing_manager,
            component_heartbeats_scratch: Some(std::collections::HashMap::with_capacity(8)),
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
        let mut stop_error: Option<String> = None;

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
                    Err(e) => {
                        warn!("RT thread stopped with error: {:?}", e);
                        stop_error = Some(format!("RT thread stopped with error: {:?}", e));
                    }
                },
                Err(_) => {
                    error!("RT thread panicked");
                    stop_error = Some("RT thread panicked".to_string());
                }
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
        match stop_error {
            Some(err) => Err(err),
            None => Ok(()),
        }
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

    /// Emergency stop - immediately zero torque on all devices
    ///
    /// This command triggers an immediate transition to safe mode,
    /// commanding zero torque. The response is sent once the command
    /// has been received by the RT thread.
    pub async fn emergency_stop(&self) -> Result<(), String> {
        if let Some(ref command_tx) = self.command_tx {
            let (response_tx, response_rx) = oneshot::channel();

            command_tx
                .try_send(EngineCommand::EmergencyStop {
                    response: response_tx,
                })
                .map_err(|_| "Failed to send emergency stop command")?;

            response_rx
                .await
                .map_err(|_| "Emergency stop command response lost")?
        } else {
            Err("Engine not started".to_string())
        }
    }

    /// Emergency stop (blocking version for non-async contexts)
    ///
    /// This is a synchronous version that can be called from non-async code.
    /// It sends the command but does not wait for confirmation.
    pub fn emergency_stop_sync(&self) -> Result<(), String> {
        if let Some(ref command_tx) = self.command_tx {
            let (response_tx, _response_rx) = oneshot::channel();

            command_tx
                .try_send(EngineCommand::EmergencyStop {
                    response: response_tx,
                })
                .map_err(|_| "Failed to send emergency stop command")?;

            Ok(())
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

        #[cfg(feature = "rt-hardening")]
        let mut rt_tick_alloc_guard = crate::allocation_tracker::track();

        // Main RT loop
        while ctx.running.load(Ordering::Acquire) {
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
                        let deadline_miss_ts_ns = monotonic_ns_since(ctx.rt_epoch, Instant::now());
                        tracer.emit_rt_event(RTTraceEvent::DeadlineMiss {
                            tick_count: ctx.frame_counter.load(Ordering::Relaxed),
                            timestamp_ns: deadline_miss_ts_ns,
                            jitter_ns: ctx.scheduler.metrics().last_jitter_ns,
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

            let tick_start = Instant::now();
            let tick_start_ns = monotonic_ns_since(ctx.rt_epoch, tick_start);

            // Emit RT trace event for tick start after scheduler gate.
            if let Some(ref tracer) = ctx.tracing_manager {
                tracer.emit_rt_event(RTTraceEvent::TickStart {
                    tick_count,
                    timestamp_ns: tick_start_ns,
                });
            }

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
                ts_mono_ns: tick_start_ns,
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

            // Apply torque safety chain:
            // pipeline torque -> fault multiplier -> safety clamp -> hard zero latch.
            let force_zero_output = ctx.emergency_stop_active
                || matches!(ctx.safety.state(), SafetyState::Faulted { .. });

            let torque_control = apply_torque_safety_controls(
                frame.torque_out,
                ctx.config.max_high_torque_nm,
                ctx.fault_torque_multiplier,
                &ctx.safety,
                force_zero_output,
            );
            ctx.atomic_counters
                .record_torque_saturation(torque_control.saturated);
            let final_torque_nm = torque_control.torque_out_nm;
            frame.torque_out = if ctx.config.max_high_torque_nm > f32::EPSILON {
                (final_torque_nm / ctx.config.max_high_torque_nm).clamp(-1.0, 1.0)
            } else {
                0.0
            };

            // Write to device
            let device_write_start = Instant::now();

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
                    timestamp_ns: monotonic_ns_since(ctx.rt_epoch, device_write_start),
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
            ctx.scheduler.record_processing_time_us(processing_time_us);

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
                    timestamp_ns: monotonic_ns_since(ctx.rt_epoch, Instant::now()),
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
            let component_heartbeats = match ctx.component_heartbeats_scratch.take() {
                Some(existing) => existing,
                None => std::collections::HashMap::with_capacity(8),
            };

            let fault_context = FaultManagerContext {
                current_torque: final_torque_nm,
                temperature: None,   // Would be read from device telemetry
                encoder_value: None, // Would be read from device telemetry
                timing_jitter_us: Some(ctx.scheduler.metrics().last_jitter_ns / 1_000),
                usb_info: None,
                plugin_execution: None,
                component_heartbeats,
                frame: Some(crate::rt::Frame {
                    ffb_in: frame.ffb_in,
                    torque_out: frame.torque_out,
                    wheel_speed: frame.wheel_speed,
                    hands_off: frame.hands_off,
                    ts_mono_ns: frame.ts_mono_ns,
                    seq: frame.seq,
                }),
            };
            let fault_result = ctx.fault_manager.update(&fault_context);
            ctx.component_heartbeats_scratch = Some(fault_context.component_heartbeats);
            ctx.fault_torque_multiplier =
                sanitize_unit_interval(fault_result.current_torque_multiplier);

            for fault in fault_result.new_faults {
                if should_latch_safety_fault(fault) {
                    ctx.safety.report_fault(fault);
                }
            }

            #[cfg(feature = "rt-hardening")]
            {
                const HARDENING_WARMUP_TICKS: u64 = 8;
                if tick_count > HARDENING_WARMUP_TICKS {
                    let allocs = rt_tick_alloc_guard.allocations_since_start();
                    if allocs > 0 {
                        let bytes = rt_tick_alloc_guard.bytes_allocated_since_start();
                        error!(
                            "RT hardening allocation violation: {} allocations ({} bytes) at tick {}",
                            allocs, bytes, tick_count
                        );
                        return Err(RTError::PipelineFault);
                    }
                }
                rt_tick_alloc_guard = crate::allocation_tracker::track();
            }
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

                EngineCommand::EmergencyStop { response } => {
                    info!("Emergency stop command received - zeroing torque immediately");
                    // Latch emergency stop and fault state.
                    ctx.emergency_stop_active = true;
                    ctx.safety.report_fault(FaultType::SafetyInterlockViolation);
                    // Zero torque is enforced in the same RT tick after command processing.
                    let _ = response.send(Ok(()));
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
    use crate::{DeviceHealthStatus, DeviceInfo, TelemetryData};
    use std::sync::{Arc, Mutex};
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

    struct CapturingDevice {
        info: DeviceInfo,
        capabilities: DeviceCapabilities,
        writes: Option<Arc<Mutex<Vec<f32>>>>,
        connected: bool,
    }

    impl CapturingDevice {
        fn new(device_id: DeviceId, writes: Option<Arc<Mutex<Vec<f32>>>>) -> Self {
            let capabilities = DeviceCapabilities::new(
                false,
                true,
                true,
                false,
                must(TorqueNm::new(25.0)),
                10000,
                1000,
            );

            let info = DeviceInfo {
                id: device_id,
                name: "Capturing Device".to_string(),
                vendor_id: 0xCAFE,
                product_id: 0xBEEF,
                serial_number: Some("CAPTURE001".to_string()),
                manufacturer: Some("OpenRacing".to_string()),
                path: "capture://test-device".to_string(),
                capabilities: capabilities.clone(),
                is_connected: true,
            };

            Self {
                info,
                capabilities,
                writes,
                connected: true,
            }
        }
    }

    impl HidDevice for CapturingDevice {
        fn write_ffb_report(&mut self, torque_nm: f32, _seq: u16) -> RTResult {
            if !self.connected {
                return Err(RTError::DeviceDisconnected);
            }

            let max_torque = self.capabilities.max_torque.value();
            if torque_nm.abs() > max_torque {
                return Err(RTError::TorqueLimit);
            }

            if let Some(writes) = &self.writes {
                let mut guard = match writes.lock() {
                    Ok(g) => g,
                    Err(e) => e.into_inner(),
                };
                if guard.len() < guard.capacity() {
                    guard.push(torque_nm);
                }
            }

            Ok(())
        }

        fn read_telemetry(&mut self) -> Option<TelemetryData> {
            None
        }

        fn capabilities(&self) -> &DeviceCapabilities {
            &self.capabilities
        }

        fn device_info(&self) -> &DeviceInfo {
            &self.info
        }

        fn is_connected(&self) -> bool {
            self.connected
        }

        fn health_status(&self) -> DeviceHealthStatus {
            DeviceHealthStatus {
                temperature_c: 30,
                fault_flags: 0,
                hands_on: true,
                last_communication: Instant::now(),
                communication_errors: 0,
            }
        }
    }

    fn create_capturing_device(writes: Option<Arc<Mutex<Vec<f32>>>>) -> Box<dyn HidDevice> {
        let device_id = must("test-device".parse::<DeviceId>());
        Box::new(CapturingDevice::new(device_id, writes))
    }

    #[cfg(not(feature = "rt-hardening"))]
    fn snapshot_torque_writes(writes: &Arc<Mutex<Vec<f32>>>) -> Vec<f32> {
        match writes.lock() {
            Ok(guard) => guard.clone(),
            Err(e) => e.into_inner().clone(),
        }
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

    #[test]
    fn test_torque_safety_controls_apply_multiplier_and_clamp() {
        let safety = SafetyService::new(5.0, 25.0);

        let result = apply_torque_safety_controls(0.2, 25.0, 1.0, &safety, false);
        assert!((result.torque_out_nm - 5.0).abs() < 0.0001);
        assert!(!result.saturated);

        let clamped = apply_torque_safety_controls(0.9, 25.0, 1.0, &safety, false);
        assert!((clamped.torque_out_nm - 5.0).abs() < 0.0001);
        assert!(clamped.saturated);
    }

    #[test]
    fn test_torque_safety_controls_emergency_stop_forces_zero() {
        let safety = SafetyService::new(5.0, 25.0);
        let result = apply_torque_safety_controls(0.75, 25.0, 1.0, &safety, true);
        assert_eq!(result.torque_out_nm, 0.0);
        assert!(result.saturated);
    }

    #[test]
    fn test_torque_safety_controls_fault_latch_forces_zero() {
        let mut safety = SafetyService::new(5.0, 25.0);
        safety.report_fault(FaultType::SafetyInterlockViolation);

        let result = apply_torque_safety_controls(0.25, 25.0, 0.6, &safety, false);
        assert_eq!(result.torque_out_nm, 0.0);
        assert!(result.saturated);
    }

    #[test]
    fn test_torque_safety_controls_sanitizes_non_finite_inputs() {
        let safety = SafetyService::new(5.0, 25.0);
        let result = apply_torque_safety_controls(f32::NAN, f32::INFINITY, -5.0, &safety, false);
        assert_eq!(result.torque_out_nm, 0.0);
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

    #[cfg(not(feature = "rt-hardening"))]
    #[tokio::test]
    async fn test_emergency_stop_collapses_torque_within_two_writes() {
        let captured_writes = Arc::new(Mutex::new(Vec::with_capacity(4096)));
        let bootstrap_device = create_test_device();
        let mut config = create_test_config();
        config.enable_blackbox = false;

        let mut engine = must(Engine::new(bootstrap_device, config));
        let runtime_device = create_capturing_device(Some(Arc::clone(&captured_writes)));
        must(engine.start(runtime_device).await);

        let mut filter_config = FilterConfig::default();
        filter_config.reconstruction = 1;
        let compiler = crate::pipeline::PipelineCompiler::new();
        let compiled = must(compiler.compile_pipeline(filter_config).await);
        must(engine.apply_pipeline(compiled).await);

        for _ in 0..16 {
            let _ = engine.send_game_input(GameInput {
                ffb_scalar: 1.0,
                telemetry: None,
                timestamp: Instant::now(),
            });
            sleep(TokioDuration::from_millis(2)).await;
        }

        let writes_before_stop = snapshot_torque_writes(&captured_writes);
        assert!(!writes_before_stop.is_empty());

        let write_count_before_stop = writes_before_stop.len();
        must(engine.emergency_stop().await);

        sleep(TokioDuration::from_millis(10)).await;
        must(engine.stop().await);

        let writes_after_stop = snapshot_torque_writes(&captured_writes);
        assert!(writes_after_stop.len() > write_count_before_stop);

        let post_stop_writes = &writes_after_stop[write_count_before_stop..];
        let first_zero_index = post_stop_writes.iter().position(|t| t.abs() <= 0.0001);
        assert!(
            matches!(first_zero_index, Some(idx) if idx <= 2),
            "expected zero torque within two writes after emergency stop, got {:?}",
            post_stop_writes
        );

        if let Some(zero_index) = first_zero_index {
            assert!(
                post_stop_writes[zero_index..]
                    .iter()
                    .all(|t| t.abs() <= 0.0001),
                "expected zero-torque latch after collapse, got {:?}",
                post_stop_writes
            );
        }
    }

    #[cfg(feature = "rt-hardening")]
    #[tokio::test]
    async fn test_rt_hardening_steady_state_has_no_rt_allocations() {
        let bootstrap_device = create_test_device();
        let mut config = create_test_config();
        config.enable_blackbox = false;
        config.rt_setup.high_priority = false;
        config.rt_setup.lock_memory = false;

        let mut engine = must(Engine::new(bootstrap_device, config));
        let runtime_device = create_capturing_device(None);
        must(engine.start(runtime_device).await);

        for _ in 0..20 {
            let _ = engine.send_game_input(GameInput {
                ffb_scalar: 0.5,
                telemetry: None,
                timestamp: Instant::now(),
            });
            sleep(TokioDuration::from_millis(1)).await;
        }

        sleep(TokioDuration::from_millis(30)).await;
        must(engine.stop().await);
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
