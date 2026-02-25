//! Prelude for convenient imports.
//!
//! This module re-exports the most commonly used types and traits.
//!
//! # Example
//!
//! ```rust
//! use openracing_fmea::prelude::*;
//! ```

pub use crate::{
    AudioAlert, AudioAlertSystem, FaultAction, FaultDetectionState, FaultMarker, FaultThresholds,
    FaultType, FmeaEntry, FmeaError, FmeaMatrix, FmeaResult, FmeaSystem, PostMortemConfig,
    RecoveryContext, RecoveryProcedure, RecoveryResult, RecoveryStatus, SoftStopController,
};
