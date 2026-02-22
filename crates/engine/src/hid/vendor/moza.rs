//! Moza Racing protocol handler
//!
//! Thin re-export from `racing-wheel-hid-moza-protocol`.

// Types from types module
pub use racing_wheel_hid_moza_protocol::{
    ES_BUTTON_COUNT, ES_LED_COUNT, MozaDeviceCategory, MozaTopologyHint, MozaDeviceIdentity,
    MozaEsCompatibility, MozaEsJoystickMode, MozaHatDirection, MozaModel,
    MozaPedalAxesRaw, MozaPedalAxes, MozaInputState,
    identify_device, is_wheelbase_product, es_compatibility,
};
// Types from protocol module
pub use racing_wheel_hid_moza_protocol::{
    DEFAULT_MAX_RETRIES, FfbMode, MozaInitState, MozaProtocol, MozaRetryPolicy,
    default_ffb_mode, default_high_torque_enabled, effective_ffb_mode,
    effective_high_torque_opt_in, signature_is_trusted,
};
// Types from other modules
pub use racing_wheel_hid_moza_protocol::{
    MOZA_VENDOR_ID,
    DeviceSignature, SignatureVerdict, verify_signature,
    RawWheelbaseReport, parse_axis,
    StandaloneAxes, StandaloneParseResult, parse_hbp_report, parse_srp_report,
};
// Submodules (re-exported so callers can write `vendor::moza::product_ids::R5_V1` etc.)
pub use racing_wheel_hid_moza_protocol::product_ids;
pub use racing_wheel_hid_moza_protocol::rim_ids;
pub use racing_wheel_hid_moza_protocol::report_ids;
pub use racing_wheel_hid_moza_protocol::input_report;
pub use racing_wheel_hid_moza_protocol::hbp_report;
