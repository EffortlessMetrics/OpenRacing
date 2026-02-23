//! Minimal RT-safe torque encoding types.
//!
//! These types are defined here (rather than in the engine) so that
//! `MozaDirectTorqueEncoder` can implement `TorqueEncoder` without
//! creating a dependency on the full engine crate.

#![deny(static_mut_refs)]

/// Torque in Q8.8 fixed-point Newton-meters (Nm).
///
/// `1.0 Nm == 256`.
pub type TorqueQ8_8 = i16;

/// Device-specific torque encoder.
///
/// Implementations must be allocation-free and deterministic in execution time.
pub trait TorqueEncoder<const N: usize> {
    /// Encode torque command into `out`, returning payload length.
    fn encode(&self, torque: TorqueQ8_8, seq: u16, flags: u8, out: &mut [u8; N]) -> usize;
    /// Encode an explicit zero torque command into `out`, returning payload length.
    fn encode_zero(&self, out: &mut [u8; N]) -> usize;
    fn clamp_min(&self) -> TorqueQ8_8;
    fn clamp_max(&self) -> TorqueQ8_8;
    fn positive_is_clockwise(&self) -> bool;
}
