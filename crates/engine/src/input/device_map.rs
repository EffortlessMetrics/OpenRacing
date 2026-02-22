//! Capture-driven input map schema.
//!
//! Thin re-export from `racing-wheel-input-maps`.

pub use racing_wheel_input_maps::{
    AxisDataType, AxisBinding, ButtonBinding, ClutchBinding, ClutchModeHint,
    DeviceInputMap, DeviceInputMapError, DeviceMapModeHints, DeviceTransportHint,
    InitFrameDirection, InitReportFrame, JsBinding, JsModeHint,
    ReportConstraint, RotaryBinding, RotaryModeHint,
    compile_ks_map,
};
