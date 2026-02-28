//! FMEA compatibility module - re-exports from openracing-fmea crate.
//!
//! This module provides backward compatibility for code that imports FMEA types
//! from the engine crate. New code should import directly from `openracing_fmea`.

// Re-export all FMEA types from the openracing-fmea crate
pub use openracing_fmea::{
    AudioAlert, AudioAlertSystem, FaultAction, FaultDetectionState, FaultMarker, FaultThresholds,
    FaultType, FmeaEntry, FmeaError, FmeaMatrix, FmeaResult, FmeaSystem, PostMortemConfig,
    RecoveryContext, RecoveryProcedure, RecoveryResult, RecoveryStatus, SoftStopController,
};
