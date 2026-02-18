//! Control-surface parsing and lock-free publication primitives.

pub mod ks;
pub mod mailbox;
pub mod device_map;

pub use ks::{
    KsAxisSource,
    KsBitSource,
    KsByteSource,
    KsClutchMode,
    KsJoystickMode,
    KsReportMap,
    KsReportSnapshot,
    KsRotaryMode,
    KS_BUTTON_BYTES,
    KS_ENCODER_COUNT,
};
pub use device_map::{
    AxisDataType,
    ButtonBinding,
    ClutchBinding,
    ClutchModeHint,
    DeviceInputMap,
    DeviceInputMapError,
    DeviceMapModeHints,
    DeviceTransportHint,
    InitReportFrame,
    JsBinding,
    ReportConstraint,
    RotaryBinding,
};
pub use mailbox::SnapshotMailbox;
