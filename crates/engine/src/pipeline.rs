//! Filter pipeline for real-time force feedback processing

use crate::{Frame, RTResult};

/// Function pointer type for filter nodes
pub type FilterNodeFn = fn(&mut Frame, *mut u8);

/// Compiled filter pipeline with zero-allocation execution
pub struct Pipeline {
    /// Function pointers for each filter node
    nodes: Vec<FilterNodeFn>,
    /// State storage for all nodes (Structure of Arrays)
    state: Vec<u8>,
    /// Offsets into state storage for each node
    state_offsets: Vec<usize>,
}

impl Pipeline {
    /// Create empty pipeline
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            state: Vec::new(),
            state_offsets: Vec::new(),
        }
    }

    /// Process frame through pipeline (RT-safe, no allocations)
    #[inline]
    pub fn process(&mut self, frame: &mut Frame) -> RTResult {
        for (i, &node_fn) in self.nodes.iter().enumerate() {
            let state_ptr = unsafe {
                self.state.as_mut_ptr().add(self.state_offsets[i])
            };
            
            // Call filter node function
            node_fn(frame, state_ptr);
            
            // Validate output is within bounds
            if !frame.torque_out.is_finite() || frame.torque_out.abs() > 1.0 {
                return Err(crate::RTError::PipelineFault);
            }
        }
        
        Ok(())
    }

    /// Swap pipeline at tick boundary (RT-safe)
    pub fn swap_at_tick_boundary(&mut self, new_pipeline: Pipeline) {
        *self = new_pipeline;
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

// Example filter node implementations

/// Reconstruction filter (anti-aliasing)
pub fn reconstruction_filter(frame: &mut Frame, state: *mut u8) {
    // Simple low-pass filter implementation
    // In real implementation, this would use proper DSP
    let alpha = 0.1f32;
    
    unsafe {
        let prev_output = *(state as *mut f32);
        let filtered = prev_output + alpha * (frame.ffb_in - prev_output);
        frame.torque_out = filtered;
        *(state as *mut f32) = filtered;
    }
}

/// Friction filter with speed adaptation
pub fn friction_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let friction_coeff = *(state as *mut f32);
        let speed_factor = 1.0 - (frame.wheel_speed.abs() * 0.1).min(0.8);
        let friction_torque = -frame.wheel_speed.signum() * friction_coeff * speed_factor;
        frame.torque_out += friction_torque;
    }
}

/// Damper filter
pub fn damper_filter(frame: &mut Frame, state: *mut f32) {
    unsafe {
        let damper_coeff = *state;
        let damper_torque = -frame.wheel_speed * damper_coeff;
        frame.torque_out += damper_torque;
    }
}

/// Torque limiting filter (safety)
pub fn torque_limit_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let max_torque = *(state as *mut f32);
        frame.torque_out = frame.torque_out.clamp(-max_torque, max_torque);
    }
}