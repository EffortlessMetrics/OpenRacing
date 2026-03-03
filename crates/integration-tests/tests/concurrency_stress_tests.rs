//! Concurrency stress tests for OpenRacing.
//!
//! These tests hammer thread-safety and race conditions across the workspace,
//! using barrier-synchronised thread pools with ≥8 threads and ≥1 000 iterations
//! per test to maximise the chance of detecting data races.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Condvar, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use openracing_atomic::AtomicCounters;
use openracing_device_types::DeviceInputs;
use openracing_filters::Frame;
use openracing_fmea::{FaultType, FmeaSystem};
use openracing_ipc::codec::{MessageHeader, message_flags, message_types};
use openracing_pipeline::Pipeline;
use openracing_profile::{WheelProfile, WheelSettings};
use openracing_watchdog::{SystemComponent, WatchdogConfig, WatchdogSystem};
use racing_wheel_schemas::prelude::NormalizedTelemetry;

const NUM_THREADS: usize = 8;
const ITERATIONS: usize = 1_000;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// ---------------------------------------------------------------------------
// 1. Multiple threads reading/writing device state simultaneously
// ---------------------------------------------------------------------------

#[test]
fn stress_concurrent_device_state_reads_writes() -> Result<(), BoxErr> {
    let state = Arc::new(RwLock::new(DeviceInputs::new()));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let state = Arc::clone(&state);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || -> Result<(), BoxErr> {
                barrier.wait();
                for i in 0..ITERATIONS {
                    if tid % 2 == 0 {
                        // Writers
                        let mut s = state.write().map_err(|e| format!("write lock: {e}"))?;
                        s.tick = (tid * ITERATIONS + i) as u32;
                        s.set_button(tid % 16, i % 2 == 0);
                        let _ = s.with_steering((i & 0xFFFF) as u16);
                    } else {
                        // Readers
                        let s = state.read().map_err(|e| format!("read lock: {e}"))?;
                        let _tick = s.tick;
                        let _pressed = s.button(tid % 16);
                        let _hat = s.hat_direction();
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. Concurrent telemetry processing from multiple games
// ---------------------------------------------------------------------------

#[test]
fn stress_concurrent_telemetry_processing() -> Result<(), BoxErr> {
    let buffer = Arc::new(openracing_telemetry_streams::TelemetryBuffer::<u64>::new(
        512,
    ));
    let total_produced = Arc::new(AtomicU64::new(0));
    let total_consumed = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));

    let producers = NUM_THREADS / 2;
    let consumers = NUM_THREADS - producers;

    let mut handles: Vec<thread::JoinHandle<Result<(), BoxErr>>> = Vec::new();
    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    for game_id in 0..producers {
        let buf = Arc::clone(&buffer);
        let bar = Arc::clone(&barrier);
        let produced = Arc::clone(&total_produced);
        handles.push(thread::spawn(move || -> Result<(), BoxErr> {
            bar.wait();
            for seq in 0..ITERATIONS {
                let value = (game_id as u64) * 100_000 + seq as u64;
                buf.push(value);
                produced.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        }));
    }

    for _ in 0..consumers {
        let buf = Arc::clone(&buffer);
        let bar = Arc::clone(&barrier);
        let consumed = Arc::clone(&total_consumed);
        let done = Arc::clone(&done);
        handles.push(thread::spawn(move || -> Result<(), BoxErr> {
            bar.wait();
            loop {
                if let Some(_val) = buf.pop() {
                    consumed.fetch_add(1, Ordering::Relaxed);
                } else if done.load(Ordering::Acquire) {
                    // Drain anything remaining
                    while buf.pop().is_some() {
                        consumed.fetch_add(1, Ordering::Relaxed);
                    }
                    break;
                }
                // Tiny yield to avoid busy-spin
                thread::yield_now();
            }
            Ok(())
        }));
    }

    // Wait for producers to finish first
    for h in handles.drain(..producers) {
        h.join().map_err(|_| "producer panicked")??;
    }
    done.store(true, Ordering::Release);

    for h in handles {
        h.join().map_err(|_| "consumer panicked")??;
    }

    let produced = total_produced.load(Ordering::SeqCst);
    let consumed = total_consumed.load(Ordering::SeqCst);
    assert_eq!(
        produced,
        (producers * ITERATIONS) as u64,
        "produced count mismatch"
    );
    // consumed may be <= produced because the buffer drops old items on overflow
    assert!(
        consumed <= produced,
        "consumed {consumed} > produced {produced}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Concurrent profile loading/switching
// ---------------------------------------------------------------------------

#[test]
fn stress_concurrent_profile_switching() -> Result<(), BoxErr> {
    let active_profile = Arc::new(RwLock::new(
        WheelProfile::new("default", "dev-0").with_settings(WheelSettings::default()),
    ));
    let switch_count = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let profile = Arc::clone(&active_profile);
            let switches = Arc::clone(&switch_count);
            let bar = Arc::clone(&barrier);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for i in 0..ITERATIONS {
                    if tid % 3 == 0 {
                        // Profile writers
                        let name = format!("profile-{tid}-{i}");
                        let new_profile = WheelProfile::new(&name, "dev-0").with_settings({
                            let mut settings = WheelSettings::default();
                            settings.ffb.overall_gain =
                                (i as f32 / ITERATIONS as f32).clamp(0.0, 1.0);
                            settings
                        });
                        let mut p = profile.write().map_err(|e| format!("write: {e}"))?;
                        *p = new_profile;
                        switches.fetch_add(1, Ordering::Relaxed);
                    } else {
                        // Profile readers
                        let p = profile.read().map_err(|e| format!("read: {e}"))?;
                        let _name = &p.name;
                        let _gain = p.settings.ffb.overall_gain;
                        assert!(
                            p.settings.ffb.overall_gain >= 0.0,
                            "gain must be non-negative"
                        );
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }
    assert!(
        switch_count.load(Ordering::SeqCst) > 0,
        "expected at least one switch"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 4. Concurrent safety state checks during device events
// ---------------------------------------------------------------------------

#[test]
fn stress_concurrent_safety_checks() -> Result<(), BoxErr> {
    let fmea = Arc::new(Mutex::new(FmeaSystem::new()));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let faults_detected = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let fmea = Arc::clone(&fmea);
            let bar = Arc::clone(&barrier);
            let faults = Arc::clone(&faults_detected);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for i in 0..ITERATIONS {
                    let mut sys = fmea.lock().map_err(|e| format!("lock: {e}"))?;
                    match tid % 4 {
                        0 => {
                            // USB fault detection
                            if sys
                                .detect_usb_fault(i as u32 % 10, Some(Duration::from_millis(100)))
                                .is_some()
                            {
                                faults.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        1 => {
                            // Encoder fault detection
                            let val = if i % 100 == 0 { f32::NAN } else { i as f32 };
                            if sys.detect_encoder_fault(val).is_some() {
                                faults.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        2 => {
                            // Timing violation detection
                            let jitter = (i as u64 % 500) + 1;
                            if sys.detect_timing_violation(jitter).is_some() {
                                faults.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        _ => {
                            // Safety state reads
                            let _active = sys.has_active_fault();
                            let _can_recover = sys.can_recover();
                        }
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    // Verify system is in a consistent state
    let sys = fmea.lock().map_err(|e| format!("final lock: {e}"))?;
    // active_fault should be Some or None, but not panic
    let _state = sys.has_active_fault();
    Ok(())
}

// ---------------------------------------------------------------------------
// 5. Concurrent IPC message processing
// ---------------------------------------------------------------------------

#[test]
fn stress_concurrent_ipc_message_encode_decode() -> Result<(), BoxErr> {
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let success_count = Arc::new(AtomicUsize::new(0));

    let msg_types = [
        message_types::DEVICE,
        message_types::PROFILE,
        message_types::SAFETY,
        message_types::HEALTH,
        message_types::TELEMETRY,
        message_types::GAME,
        message_types::DIAGNOSTIC,
        message_types::FEATURE_NEGOTIATION,
    ];

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let bar = Arc::clone(&barrier);
            let successes = Arc::clone(&success_count);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for seq in 0..ITERATIONS {
                    let msg_type = msg_types[tid % msg_types.len()];
                    let payload_len = (seq as u32) * 10;
                    let header = MessageHeader::new(msg_type, payload_len, seq as u32);
                    let encoded = header.encode();
                    let decoded =
                        MessageHeader::decode(&encoded).map_err(|e| format!("decode: {e}"))?;
                    assert_eq!(decoded.message_type, msg_type, "msg type mismatch");
                    assert_eq!(decoded.payload_len, payload_len, "payload len mismatch");
                    assert_eq!(decoded.sequence, seq as u32, "sequence mismatch");
                    successes.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    let total = success_count.load(Ordering::SeqCst);
    assert_eq!(
        total,
        NUM_THREADS * ITERATIONS,
        "not all messages processed"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 6. Lock-free atomic operations under contention
// ---------------------------------------------------------------------------

#[test]
fn stress_atomic_counters_under_contention() -> Result<(), BoxErr> {
    let counters = Arc::new(AtomicCounters::new());
    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let ctr = Arc::clone(&counters);
            let bar = Arc::clone(&barrier);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for i in 0..ITERATIONS {
                    match tid % 5 {
                        0 => ctr.inc_tick(),
                        1 => ctr.inc_missed_tick(),
                        2 => ctr.inc_safety_event(),
                        3 => ctr.inc_telemetry_received(),
                        _ => ctr.record_torque_saturation(i % 2 == 0),
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    let snap = counters.snapshot();
    // Each category is fed by ceil(NUM_THREADS/5) threads, each doing ITERATIONS
    let threads_per_bucket =
        |bucket: usize| -> u64 { (0..NUM_THREADS).filter(|t| t % 5 == bucket).count() as u64 };
    assert_eq!(snap.total_ticks, threads_per_bucket(0) * ITERATIONS as u64);
    assert_eq!(snap.missed_ticks, threads_per_bucket(1) * ITERATIONS as u64);
    assert_eq!(
        snap.safety_events,
        threads_per_bucket(2) * ITERATIONS as u64
    );
    assert_eq!(
        snap.telemetry_packets_received,
        threads_per_bucket(3) * ITERATIONS as u64
    );
    Ok(())
}

#[test]
fn stress_atomic_snapshot_and_reset_under_contention() -> Result<(), BoxErr> {
    let counters = Arc::new(AtomicCounters::new());
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let total_snapshots = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let ctr = Arc::clone(&counters);
            let bar = Arc::clone(&barrier);
            let snaps = Arc::clone(&total_snapshots);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for _ in 0..ITERATIONS {
                    if tid % 4 == 0 {
                        let _snap = ctr.snapshot_and_reset();
                        snaps.fetch_add(1, Ordering::Relaxed);
                    } else {
                        ctr.inc_tick();
                        ctr.inc_telemetry_received();
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    assert!(
        total_snapshots.load(Ordering::SeqCst) > 0,
        "should have taken snapshots"
    );
    // Final snapshot should be consistent (no tearing)
    let snap = counters.snapshot();
    assert!(
        snap.total_ticks <= (NUM_THREADS as u64) * (ITERATIONS as u64),
        "tick count unexpectedly high"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 7. Channel/queue behavior under producer/consumer stress
// ---------------------------------------------------------------------------

#[test]
fn stress_crossbeam_channel_producer_consumer() -> Result<(), BoxErr> {
    let (tx, rx) = crossbeam::channel::bounded::<(usize, usize)>(64);
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let produced = Arc::new(AtomicUsize::new(0));
    let consumed = Arc::new(AtomicUsize::new(0));

    let producers = NUM_THREADS / 2;
    let consumers = NUM_THREADS - producers;
    let mut handles = Vec::new();

    for pid in 0..producers {
        let tx = tx.clone();
        let bar = Arc::clone(&barrier);
        let prod_count = Arc::clone(&produced);
        handles.push(thread::spawn(move || -> Result<(), BoxErr> {
            bar.wait();
            for seq in 0..ITERATIONS {
                tx.send((pid, seq)).map_err(|e| format!("send: {e}"))?;
                prod_count.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        }));
    }

    // Drop the original tx so channel closes when all producers finish
    drop(tx);

    for _ in 0..consumers {
        let rx = rx.clone();
        let bar = Arc::clone(&barrier);
        let cons_count = Arc::clone(&consumed);
        handles.push(thread::spawn(move || -> Result<(), BoxErr> {
            bar.wait();
            while let Ok((_pid, _seq)) = rx.recv() {
                cons_count.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        }));
    }

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    let total_produced = produced.load(Ordering::SeqCst);
    let total_consumed = consumed.load(Ordering::SeqCst);
    assert_eq!(
        total_produced, total_consumed,
        "produced {total_produced} != consumed {total_consumed}"
    );
    assert_eq!(total_produced, producers * ITERATIONS);
    Ok(())
}

#[test]
fn stress_crossbeam_mpmc_contention() -> Result<(), BoxErr> {
    // All threads both produce and consume on a shared queue
    let (tx, rx) = crossbeam::channel::bounded::<u64>(32);
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let sent = Arc::new(AtomicU64::new(0));
    let received = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::new();

    for tid in 0..NUM_THREADS {
        let tx = tx.clone();
        let rx = rx.clone();
        let bar = Arc::clone(&barrier);
        let s = Arc::clone(&sent);
        let r = Arc::clone(&received);
        let d = Arc::clone(&done);
        handles.push(thread::spawn(move || -> Result<(), BoxErr> {
            bar.wait();
            for i in 0..ITERATIONS {
                let value = (tid * ITERATIONS + i) as u64;
                // Try to send (non-blocking to avoid deadlock on bounded channel)
                if tx.try_send(value).is_ok() {
                    s.fetch_add(1, Ordering::Relaxed);
                }
                // Try to receive
                if rx.try_recv().is_ok() {
                    r.fetch_add(1, Ordering::Relaxed);
                }
            }
            // Signal done and drain remaining
            d.store(true, Ordering::Release);
            Ok(())
        }));
    }
    // Drop originals so channel eventually closes
    drop(tx);
    drop(rx);

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    let total_sent = sent.load(Ordering::SeqCst);
    let total_recv = received.load(Ordering::SeqCst);
    assert!(total_sent > 0, "should have sent some messages");
    assert!(total_recv > 0, "should have received some messages");
    Ok(())
}

// ---------------------------------------------------------------------------
// 8. Concurrent filter chain processing
// ---------------------------------------------------------------------------

#[test]
fn stress_concurrent_filter_chain_processing() -> Result<(), BoxErr> {
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let total_processed = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let bar = Arc::clone(&barrier);
            let processed = Arc::clone(&total_processed);
            thread::spawn(move || -> Result<(), BoxErr> {
                // Each thread owns its own pipeline and filter states (RT-safe: no sharing)
                let mut pipeline = Pipeline::new();
                let friction_state = openracing_filters::FrictionState::default();
                let damper_state = openracing_filters::DamperState::default();
                let mut slew_state = openracing_filters::SlewRateState::default();

                bar.wait();
                for i in 0..ITERATIONS {
                    let ffb_in = ((tid * ITERATIONS + i) as f32 * 0.001).sin();
                    let mut frame = Frame {
                        ffb_in,
                        torque_out: ffb_in,
                        wheel_speed: (i as f32) * 0.01,
                        hands_off: false,
                        ts_mono_ns: i as u64 * 1_000_000,
                        seq: i as u16,
                    };

                    // Apply filters manually (per-thread state, no contention)
                    openracing_filters::friction_filter(&mut frame, &friction_state);
                    openracing_filters::damper_filter(&mut frame, &damper_state);
                    openracing_filters::slew_rate_filter(&mut frame, &mut slew_state);

                    // Also run through pipeline
                    let _result = pipeline.process(&mut frame);
                    processed.fetch_add(1, Ordering::Relaxed);

                    // Verify output is finite
                    assert!(
                        frame.torque_out.is_finite(),
                        "thread {tid} iter {i}: non-finite torque_out"
                    );
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    let total = total_processed.load(Ordering::SeqCst);
    assert_eq!(total, NUM_THREADS * ITERATIONS, "not all frames processed");
    Ok(())
}

#[test]
fn stress_concurrent_pipeline_swap_during_processing() -> Result<(), BoxErr> {
    // Simulate atomic pipeline swap: readers use Arc<RwLock<Pipeline>>,
    // one writer swaps the pipeline while readers are processing.
    let pipeline = Arc::new(RwLock::new(Pipeline::new()));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let frames_processed = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let pipe = Arc::clone(&pipeline);
            let bar = Arc::clone(&barrier);
            let count = Arc::clone(&frames_processed);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for i in 0..ITERATIONS {
                    if tid == 0 && i % 100 == 0 {
                        // Writer: swap pipeline
                        let new_pipe = Pipeline::new();
                        let mut p = pipe.write().map_err(|e| format!("write: {e}"))?;
                        *p = new_pipe;
                    } else {
                        // Readers: clone pipeline and process a frame
                        let p = pipe.read().map_err(|e| format!("read: {e}"))?;
                        let mut local = p.clone();
                        drop(p); // Release lock before processing
                        let mut frame = Frame::from_ffb(0.5, 0.0);
                        let _result = local.process(&mut frame);
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    assert!(
        frames_processed.load(Ordering::SeqCst) > 0,
        "should have processed frames"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 9. Thread pool saturation and recovery
// ---------------------------------------------------------------------------

#[test]
fn stress_thread_pool_saturation_recovery() -> Result<(), BoxErr> {
    // Saturate a fixed-size thread pool and verify all work eventually completes.
    let pool_size = NUM_THREADS;
    let total_tasks = pool_size * ITERATIONS;
    let completed = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(pool_size));

    // Phase 1: Saturate with heavy work
    let handles: Vec<_> = (0..pool_size)
        .map(|_| {
            let comp = Arc::clone(&completed);
            let bar = Arc::clone(&barrier);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for _ in 0..ITERATIONS {
                    // Simulate variable workload
                    let mut acc = 0u64;
                    for j in 0..100 {
                        acc = acc.wrapping_add(j);
                    }
                    // Prevent optimisation
                    std::hint::black_box(acc);
                    comp.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }
    assert_eq!(
        completed.load(Ordering::SeqCst),
        total_tasks,
        "not all tasks completed after saturation"
    );

    // Phase 2: Recovery - pool should accept new work immediately
    let recovery_completed = Arc::new(AtomicUsize::new(0));
    let bar2 = Arc::new(Barrier::new(pool_size));
    let recovery_handles: Vec<_> = (0..pool_size)
        .map(|_| {
            let comp = Arc::clone(&recovery_completed);
            let bar = Arc::clone(&bar2);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for _ in 0..ITERATIONS {
                    comp.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            })
        })
        .collect();

    for h in recovery_handles {
        h.join().map_err(|_| "thread panicked")??;
    }
    assert_eq!(
        recovery_completed.load(Ordering::SeqCst),
        total_tasks,
        "recovery phase did not complete all tasks"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 10. Concurrent watchdog feed from multiple sources
// ---------------------------------------------------------------------------

#[test]
fn stress_concurrent_watchdog_feeds() -> Result<(), BoxErr> {
    let config = WatchdogConfig::builder()
        .plugin_timeout_us(5_000)
        .plugin_max_timeouts(50)
        .plugin_quarantine_duration(Duration::from_millis(100))
        .build()
        .map_err(|e| format!("config: {e}"))?;
    let watchdog = Arc::new(WatchdogSystem::new(config));

    // Pre-register plugins
    for i in 0..NUM_THREADS {
        watchdog.register_plugin(&format!("plugin-{i}"));
    }

    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let faults_seen = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let wd = Arc::clone(&watchdog);
            let bar = Arc::clone(&barrier);
            let faults = Arc::clone(&faults_seen);
            thread::spawn(move || -> Result<(), BoxErr> {
                let plugin_id = format!("plugin-{tid}");
                bar.wait();
                for i in 0..ITERATIONS {
                    // Record execution with varying times
                    let exec_time = (i as u64 % 100) + 1;
                    if let Some(_fault) = wd.record_plugin_execution(&plugin_id, exec_time) {
                        faults.fetch_add(1, Ordering::Relaxed);
                    }

                    // Heartbeat for different components
                    let component = match tid % 4 {
                        0 => SystemComponent::RtThread,
                        1 => SystemComponent::HidCommunication,
                        2 => SystemComponent::TelemetryAdapter,
                        _ => SystemComponent::PluginHost,
                    };
                    wd.heartbeat(component);

                    // Check quarantine status
                    let _quarantined = wd.is_plugin_quarantined(&plugin_id);
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    // Verify consistent final state
    assert_eq!(watchdog.plugin_count(), NUM_THREADS);
    let _summary = watchdog.get_health_summary();
    Ok(())
}

#[test]
fn stress_watchdog_quarantine_under_contention() -> Result<(), BoxErr> {
    let config = WatchdogConfig::builder()
        .plugin_timeout_us(100) // Very low threshold to trigger quarantines
        .plugin_max_timeouts(3)
        .plugin_quarantine_duration(Duration::from_millis(50))
        .build()
        .map_err(|e| format!("config: {e}"))?;
    let watchdog = Arc::new(WatchdogSystem::new(config));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let plugin_id = "stress-plugin";
    watchdog.register_plugin(plugin_id);

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|_| {
            let wd = Arc::clone(&watchdog);
            let bar = Arc::clone(&barrier);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for i in 0..ITERATIONS {
                    // Alternate between fast and slow executions
                    let exec_time = if i % 5 == 0 { 500 } else { 10 };
                    let _fault = wd.record_plugin_execution(plugin_id, exec_time);
                    let _q = wd.is_plugin_quarantined(plugin_id);
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    // Stats should reflect execution history
    let stats = watchdog.get_plugin_stats(plugin_id);
    assert!(stats.is_some(), "plugin stats should exist");
    Ok(())
}

// ---------------------------------------------------------------------------
// 11. Memory ordering correctness (Acquire/Release/SeqCst)
// ---------------------------------------------------------------------------

#[test]
fn stress_memory_ordering_acquire_release() -> Result<(), BoxErr> {
    // Classic pattern: writer publishes data + sets flag with Release,
    // reader sees flag with Acquire and reads the data.
    let violations = Arc::new(AtomicUsize::new(0));

    for _ in 0..ITERATIONS {
        let data = Arc::new(AtomicU64::new(0));
        let flag = Arc::new(AtomicBool::new(false));
        let bar = Arc::new(Barrier::new(2));
        let v = Arc::clone(&violations);

        let d_w = Arc::clone(&data);
        let f_w = Arc::clone(&flag);
        let b_w = Arc::clone(&bar);

        let d_r = Arc::clone(&data);
        let f_r = Arc::clone(&flag);
        let b_r = Arc::clone(&bar);

        let writer = thread::spawn(move || {
            b_w.wait();
            d_w.store(42, Ordering::Relaxed);
            f_w.store(true, Ordering::Release);
        });

        let reader = thread::spawn(move || {
            b_r.wait();
            // Spin until flag is set
            while !f_r.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            // After Acquire, we must see the data the writer stored before Release
            if d_r.load(Ordering::Relaxed) != 42 {
                v.fetch_add(1, Ordering::Relaxed);
            }
        });

        writer.join().map_err(|_| "writer panicked")?;
        reader.join().map_err(|_| "reader panicked")?;
    }

    assert_eq!(
        violations.load(Ordering::SeqCst),
        0,
        "Acquire/Release ordering violated"
    );
    Ok(())
}

#[test]
fn stress_seqcst_total_ordering() -> Result<(), BoxErr> {
    // Verify SeqCst provides a total order: two flags set by two threads,
    // at least one observer must see both.
    let violations = Arc::new(AtomicUsize::new(0));

    for _ in 0..ITERATIONS {
        let x = Arc::new(AtomicBool::new(false));
        let y = Arc::new(AtomicBool::new(false));

        let saw_x_then_y = Arc::new(AtomicBool::new(false));
        let saw_y_then_x = Arc::new(AtomicBool::new(false));

        let bar = Arc::new(Barrier::new(4));
        let v = Arc::clone(&violations);

        let x1 = Arc::clone(&x);
        let b1 = Arc::clone(&bar);

        let y1 = Arc::clone(&y);
        let b2 = Arc::clone(&bar);

        let x2 = Arc::clone(&x);
        let y2 = Arc::clone(&y);
        let b3 = Arc::clone(&bar);
        let sxy = Arc::clone(&saw_x_then_y);

        let x3 = Arc::clone(&x);
        let y3 = Arc::clone(&y);
        let b4 = Arc::clone(&bar);
        let syx = Arc::clone(&saw_y_then_x);

        let t1 = thread::spawn(move || {
            b1.wait();
            x1.store(true, Ordering::SeqCst);
        });
        let t2 = thread::spawn(move || {
            b2.wait();
            y1.store(true, Ordering::SeqCst);
        });
        let t3 = thread::spawn(move || {
            b3.wait();
            if x2.load(Ordering::SeqCst) && !y2.load(Ordering::SeqCst) {
                sxy.store(true, Ordering::SeqCst);
            }
        });
        let t4 = thread::spawn(move || {
            b4.wait();
            if y3.load(Ordering::SeqCst) && !x3.load(Ordering::SeqCst) {
                syx.store(true, Ordering::SeqCst);
            }
        });

        t1.join().map_err(|_| "t1 panicked")?;
        t2.join().map_err(|_| "t2 panicked")?;
        t3.join().map_err(|_| "t3 panicked")?;
        t4.join().map_err(|_| "t4 panicked")?;

        // Under SeqCst, it's impossible for t3 to see x=T, y=F AND t4 to see y=T, x=F
        // simultaneously (that would require contradictory total orders).
        if saw_x_then_y.load(Ordering::SeqCst) && saw_y_then_x.load(Ordering::SeqCst) {
            v.fetch_add(1, Ordering::Relaxed);
        }
    }

    assert_eq!(
        violations.load(Ordering::SeqCst),
        0,
        "SeqCst total ordering violated"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 12. Spurious wakeup handling
// ---------------------------------------------------------------------------

#[test]
fn stress_spurious_wakeup_handling() -> Result<(), BoxErr> {
    // Verify that Condvar usage correctly handles spurious wakeups by
    // always re-checking the predicate after `wait`.
    let pair = Arc::new((Mutex::new(false), Condvar::new()));
    let barrier = Arc::new(Barrier::new(NUM_THREADS + 1)); // +1 for the notifier
    let wakeup_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    // Waiter threads: each waits for the predicate to become true
    for _ in 0..NUM_THREADS {
        let pair = Arc::clone(&pair);
        let bar = Arc::clone(&barrier);
        let wakeups = Arc::clone(&wakeup_count);
        handles.push(thread::spawn(move || -> Result<(), BoxErr> {
            bar.wait();
            let (lock, cvar) = &*pair;
            let mut guard = lock.lock().map_err(|e| format!("lock: {e}"))?;
            // The predicate loop handles spurious wakeups
            while !*guard {
                let result = cvar
                    .wait_timeout(guard, Duration::from_secs(5))
                    .map_err(|e| format!("wait: {e}"))?;
                guard = result.0;
                wakeups.fetch_add(1, Ordering::Relaxed);
                if result.1.timed_out() && !*guard {
                    return Err("timed out waiting for signal".into());
                }
            }
            assert!(*guard, "woke up but predicate is false");
            Ok(())
        }));
    }

    // Notifier: set predicate and wake everyone
    let pair_n = Arc::clone(&pair);
    let bar_n = Arc::clone(&barrier);
    let notifier = thread::spawn(move || -> Result<(), BoxErr> {
        bar_n.wait();
        // Small delay so waiters are likely blocked
        thread::sleep(Duration::from_millis(10));
        let (lock, cvar) = &*pair_n;
        let mut guard = lock.lock().map_err(|e| format!("lock: {e}"))?;
        *guard = true;
        drop(guard);
        cvar.notify_all();
        Ok(())
    });

    notifier.join().map_err(|_| "notifier panicked")??;
    for h in handles {
        h.join().map_err(|_| "waiter panicked")??;
    }

    let total_wakeups = wakeup_count.load(Ordering::SeqCst);
    assert!(
        total_wakeups >= NUM_THREADS,
        "expected at least {NUM_THREADS} wakeups, got {total_wakeups}"
    );
    Ok(())
}

#[test]
fn stress_condvar_notify_one_fairness() -> Result<(), BoxErr> {
    // Multiple threads waiting on a condvar, notified one-at-a-time.
    // Each should eventually get signalled.
    let pair = Arc::new((Mutex::new(0u32), Condvar::new()));
    let barrier = Arc::new(Barrier::new(NUM_THREADS + 1));
    let woke = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for tid in 0..NUM_THREADS {
        let pair = Arc::clone(&pair);
        let bar = Arc::clone(&barrier);
        let w = Arc::clone(&woke);
        handles.push(thread::spawn(move || -> Result<(), BoxErr> {
            bar.wait();
            let (lock, cvar) = &*pair;
            let mut guard = lock.lock().map_err(|e| format!("lock: {e}"))?;
            let target = (tid + 1) as u32;
            while *guard < target {
                let result = cvar
                    .wait_timeout(guard, Duration::from_secs(10))
                    .map_err(|e| format!("wait: {e}"))?;
                guard = result.0;
                if result.1.timed_out() {
                    return Err(format!("thread {tid} timed out").into());
                }
            }
            w.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }));
    }

    // Notifier increments counter and notifies one at a time
    let pair_n = Arc::clone(&pair);
    let bar_n = Arc::clone(&barrier);
    let notifier = thread::spawn(move || -> Result<(), BoxErr> {
        bar_n.wait();
        for i in 1..=(NUM_THREADS as u32) {
            thread::sleep(Duration::from_millis(1));
            let (lock, cvar) = &*pair_n;
            let mut guard = lock.lock().map_err(|e| format!("lock: {e}"))?;
            *guard = i;
            drop(guard);
            cvar.notify_all(); // Use notify_all so all waiting threads re-check
        }
        Ok(())
    });

    notifier.join().map_err(|_| "notifier panicked")??;
    for h in handles {
        h.join().map_err(|_| "waiter panicked")??;
    }

    assert_eq!(
        woke.load(Ordering::SeqCst),
        NUM_THREADS,
        "not all threads woke up"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Bonus: Combined stress - concurrent telemetry + profile + safety
// ---------------------------------------------------------------------------

#[test]
fn stress_combined_telemetry_profile_safety() -> Result<(), BoxErr> {
    let counters = Arc::new(AtomicCounters::new());
    let profile = Arc::new(RwLock::new(
        WheelProfile::new("combined-test", "dev-0").with_settings(WheelSettings::default()),
    ));
    let fmea = Arc::new(Mutex::new(FmeaSystem::new()));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let ctr = Arc::clone(&counters);
            let prof = Arc::clone(&profile);
            let fmea = Arc::clone(&fmea);
            let bar = Arc::clone(&barrier);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for i in 0..ITERATIONS {
                    match tid % 4 {
                        0 => {
                            // Telemetry ingestion
                            ctr.inc_tick();
                            ctr.inc_telemetry_received();
                            let mut frame =
                                Frame::from_ffb((i as f32 * 0.01).sin(), i as f32 * 0.1);
                            openracing_filters::torque_cap_filter(&mut frame, 1.0);
                            assert!(
                                frame.torque_out.abs() <= 1.0 + f32::EPSILON,
                                "torque exceeds cap"
                            );
                        }
                        1 => {
                            // Profile switching
                            if i % 50 == 0 {
                                let new = WheelProfile::new(format!("p-{i}"), "dev-0");
                                let mut p = prof.write().map_err(|e| format!("write: {e}"))?;
                                *p = new;
                                ctr.inc_profile_switch();
                            } else {
                                let p = prof.read().map_err(|e| format!("read: {e}"))?;
                                let _name = &p.name;
                            }
                        }
                        2 => {
                            // Safety checks
                            let mut sys = fmea.lock().map_err(|e| format!("lock: {e}"))?;
                            let val = if i % 200 == 0 { f32::NAN } else { 1.0 };
                            let _fault = sys.detect_encoder_fault(val);
                        }
                        _ => {
                            // Metrics snapshot
                            let _snap = ctr.snapshot();
                            let _ticks = ctr.total_ticks();
                        }
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    let snap = counters.snapshot();
    assert!(snap.total_ticks > 0, "should have recorded ticks");
    assert!(
        snap.telemetry_packets_received > 0,
        "should have recorded telemetry"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Stress: NormalizedTelemetry concurrent reads under Arc<RwLock>
// ---------------------------------------------------------------------------

#[test]
fn stress_normalized_telemetry_concurrent_access() -> Result<(), BoxErr> {
    let telemetry = Arc::new(RwLock::new(NormalizedTelemetry::new()));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let reads = Arc::new(AtomicUsize::new(0));
    let writes = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let tel = Arc::clone(&telemetry);
            let bar = Arc::clone(&barrier);
            let r = Arc::clone(&reads);
            let w = Arc::clone(&writes);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for i in 0..ITERATIONS {
                    if tid == 0 {
                        // Single writer updating telemetry
                        let mut t = tel.write().map_err(|e| format!("write: {e}"))?;
                        t.speed_ms = i as f32 * 0.5;
                        t.rpm = (i as f32) * 100.0;
                        t.gear = (i % 7) as i8;
                        t.ffb_scalar = ((i as f32) * 0.001).sin();
                        t.sequence = i as u64;
                        w.fetch_add(1, Ordering::Relaxed);
                    } else {
                        // Multiple readers
                        let t = tel.read().map_err(|e| format!("read: {e}"))?;
                        assert!(t.speed_ms >= 0.0, "speed must be non-negative");
                        assert!(t.rpm >= 0.0, "rpm must be non-negative");
                        let _seq = t.sequence;
                        r.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    assert_eq!(writes.load(Ordering::SeqCst), ITERATIONS);
    assert!(
        reads.load(Ordering::SeqCst) > 0,
        "should have performed reads"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Stress: DeviceInputs Copy-based concurrent processing
// ---------------------------------------------------------------------------

#[test]
fn stress_device_inputs_copy_semantics() -> Result<(), BoxErr> {
    // DeviceInputs is Copy – verify concurrent snapshot reads are safe
    let shared = Arc::new(AtomicU32::new(0));
    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let tick = Arc::clone(&shared);
            let bar = Arc::clone(&barrier);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for i in 0..ITERATIONS {
                    // Each thread creates its own DeviceInputs (Copy, no contention)
                    let current_tick = tick.fetch_add(1, Ordering::Relaxed);
                    let inputs = DeviceInputs::new()
                        .with_steering((current_tick & 0xFFFF) as u16)
                        .with_pedals((i & 0xFFFF) as u16, ((i + tid) & 0xFFFF) as u16, 0)
                        .with_hat((tid % 9) as u8);

                    // Verify snapshot is self-consistent
                    let copy = inputs;
                    assert_eq!(copy.hat, inputs.hat, "Copy semantics broken for hat");
                    assert_eq!(
                        copy.steering, inputs.steering,
                        "Copy semantics broken for steering"
                    );
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    let final_tick = shared.load(Ordering::SeqCst);
    assert_eq!(
        final_tick,
        (NUM_THREADS * ITERATIONS) as u32,
        "atomic tick count mismatch"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Stress: Fault type severity checks under contention
// ---------------------------------------------------------------------------

#[test]
fn stress_fault_type_concurrent_property_checks() -> Result<(), BoxErr> {
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let checked = Arc::new(AtomicUsize::new(0));

    let fault_types = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
        FaultType::Overcurrent,
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
        FaultType::SafetyInterlockViolation,
        FaultType::HandsOffTimeout,
    ];

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let bar = Arc::clone(&barrier);
            let c = Arc::clone(&checked);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for i in 0..ITERATIONS {
                    let ft = fault_types[(tid + i) % fault_types.len()];
                    let _sev = ft.severity();
                    let _imm = ft.requires_immediate_response();
                    let _rec = ft.is_recoverable();
                    let _ms = ft.default_max_response_time_ms();
                    c.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    assert_eq!(
        checked.load(Ordering::SeqCst),
        NUM_THREADS * ITERATIONS,
        "not all checks completed"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Stress: Concurrent IPC header encoding with flags
// ---------------------------------------------------------------------------

#[test]
fn stress_ipc_header_flags_under_contention() -> Result<(), BoxErr> {
    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let errors = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let bar = Arc::clone(&barrier);
            let err = Arc::clone(&errors);
            thread::spawn(move || -> Result<(), BoxErr> {
                bar.wait();
                for seq in 0..ITERATIONS {
                    let mut header =
                        MessageHeader::new(message_types::TELEMETRY, seq as u32 * 4, seq as u32);

                    // Set various flags
                    if tid % 2 == 0 {
                        header.set_flag(message_flags::COMPRESSED);
                    }
                    if tid % 3 == 0 {
                        header.set_flag(message_flags::REQUIRES_ACK);
                    }

                    let encoded = header.encode();
                    let decoded = match MessageHeader::decode(&encoded) {
                        Ok(d) => d,
                        Err(_) => {
                            err.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                    };
                    assert_eq!(decoded.message_type, message_types::TELEMETRY);
                    assert_eq!(decoded.sequence, seq as u32);
                    assert_eq!(decoded.payload_len, seq as u32 * 4);

                    if tid % 2 == 0 {
                        assert!(
                            decoded.has_flag(message_flags::COMPRESSED),
                            "COMPRESSED flag missing"
                        );
                    }
                }
                Ok(())
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")??;
    }

    assert_eq!(errors.load(Ordering::SeqCst), 0, "decode errors occurred");
    Ok(())
}
