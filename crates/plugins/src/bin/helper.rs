//! Native plugin helper process for RT operations

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use clap::Parser;
use shared_memory::{Shmem, ShmemConf};
use uuid::Uuid;

use racing_wheel_plugins::native::{PluginFrame, SharedMemoryHeader};

#[derive(Parser)]
#[command(name = "wheel-plugin-helper")]
#[command(about = "Helper process for native plugin RT operations")]
struct Args {
    /// Plugin ID
    #[arg(long)]
    plugin_id: Uuid,
    
    /// Shared memory ID
    #[arg(long)]
    shmem_id: String,
    
    /// Budget in microseconds
    #[arg(long)]
    budget_us: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    tracing::info!(
        plugin_id = %args.plugin_id,
        shmem_id = %args.shmem_id,
        budget_us = args.budget_us,
        "Starting plugin helper process"
    );
    
    // Open shared memory
    let shared_memory = ShmemConf::new().os_id(&args.shmem_id).open()?;
    
    // Main processing loop
    let mut frame_count = 0u64;
    let mut total_processing_time = Duration::ZERO;
    
    loop {
        // Check shutdown flag
        let shutdown = unsafe {
            let header = shared_memory.as_ptr() as *const SharedMemoryHeader;
            (*header).shutdown_flag.load(Ordering::Relaxed)
        };
        
        if shutdown {
            tracing::info!("Shutdown requested, exiting");
            break;
        }
        
        // Try to read a frame
        if let Some(mut frame) = read_frame_from_shared_memory(&shared_memory)? {
            let start_time = Instant::now();
            
            // Process the frame (simplified - real implementation would load and call plugin)
            process_frame(&mut frame, args.budget_us)?;
            
            let processing_time = start_time.elapsed();
            total_processing_time += processing_time;
            frame_count += 1;
            
            // Write result back
            write_frame_to_shared_memory(&shared_memory, frame)?;
            
            // Check budget violation
            if processing_time.as_micros() > args.budget_us as u128 {
                tracing::warn!(
                    processing_time_us = processing_time.as_micros(),
                    budget_us = args.budget_us,
                    "Budget violation detected"
                );
            }
            
            // Log statistics periodically
            if frame_count % 1000 == 0 {
                let avg_time = total_processing_time / frame_count as u32;
                tracing::info!(
                    frames_processed = frame_count,
                    avg_processing_time_us = avg_time.as_micros(),
                    "Processing statistics"
                );
            }
        } else {
            // No frame available, sleep briefly
            std::thread::sleep(Duration::from_micros(100));
        }
    }
    
    tracing::info!(
        frames_processed = frame_count,
        total_time_ms = total_processing_time.as_millis(),
        "Helper process shutting down"
    );
    
    Ok(())
}

fn read_frame_from_shared_memory(shared_memory: &Shmem) -> Result<Option<PluginFrame>, Box<dyn std::error::Error>> {
    unsafe {
        let header = shared_memory.as_ptr() as *const SharedMemoryHeader;
        let frames_ptr = (header as *const u8).add(std::mem::size_of::<SharedMemoryHeader>()) as *const PluginFrame;
        
        let producer_seq = (*header).producer_seq.load(Ordering::Acquire);
        let consumer_seq = (*header).consumer_seq.load(Ordering::Acquire);
        
        // Check if data is available
        if consumer_seq >= producer_seq {
            return Ok(None);
        }
        
        // Read frame
        let index = consumer_seq % (*header).max_frames;
        let frame = *frames_ptr.add(index as usize);
        
        // Update consumer sequence
        let header_mut = shared_memory.as_ptr() as *mut SharedMemoryHeader;
        (*header_mut).consumer_seq.store(consumer_seq.wrapping_add(1), Ordering::Release);
        
        Ok(Some(frame))
    }
}

fn write_frame_to_shared_memory(shared_memory: &Shmem, frame: PluginFrame) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let header = shared_memory.as_ptr() as *mut SharedMemoryHeader;
        let frames_ptr = (header as *mut u8).add(std::mem::size_of::<SharedMemoryHeader>()) as *mut PluginFrame;
        
        let producer_seq = (*header).producer_seq.load(Ordering::Acquire);
        let consumer_seq = (*header).consumer_seq.load(Ordering::Acquire);
        
        // Check if ring buffer is full
        if producer_seq.wrapping_sub(consumer_seq) >= (*header).max_frames {
            return Err("Ring buffer full".into());
        }
        
        // Write frame
        let index = producer_seq % (*header).max_frames;
        *frames_ptr.add(index as usize) = frame;
        
        // Update producer sequence
        (*header).producer_seq.store(producer_seq.wrapping_add(1), Ordering::Release);
    }
    
    Ok(())
}

fn process_frame(frame: &mut PluginFrame, budget_us: u32) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    
    // Simplified DSP processing - in real implementation, this would call the loaded plugin
    // For now, just apply a simple gain and add some processing delay
    frame.torque_out = frame.ffb_in * 0.95; // Slight attenuation
    
    // Simulate some processing time
    let target_time = Duration::from_micros((budget_us / 4) as u64); // Use 1/4 of budget
    while start_time.elapsed() < target_time {
        // Busy wait to simulate processing
        std::hint::spin_loop();
    }
    
    Ok(())
}