//! Thrustmaster protocol handler: initialization and device management.

#![deny(static_mut_refs)]

use crate::ids::{Model, product_ids};
use crate::input::{ThrustmasterInputState, parse_input_report};
use crate::output::{
    ThrustmasterConstantForceEncoder, build_actuator_enable, build_device_gain,
    build_set_range_report,
};
use crate::types::{is_pedal_product, is_wheel_product};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrustmasterInitState {
    Uninitialized,
    Initializing,
    Ready,
    Failed,
}

pub struct ThrustmasterProtocol {
    product_id: u16,
    model: Model,
    init_state: ThrustmasterInitState,
    gain: u8,
    rotation_range: u16,
    max_torque_nm: f32,
}

impl ThrustmasterProtocol {
    pub fn new(product_id: u16) -> Self {
        let model = Model::from_product_id(product_id);
        let max_torque = model.max_torque_nm();
        let rotation_range = model.max_rotation_deg();

        Self {
            product_id,
            model,
            init_state: ThrustmasterInitState::Uninitialized,
            gain: 0xFF,
            rotation_range,
            max_torque_nm: max_torque,
        }
    }

    pub fn new_with_config(product_id: u16, max_torque_nm: f32, rotation_range: u16) -> Self {
        Self {
            product_id,
            model: Model::Unknown,
            init_state: ThrustmasterInitState::Uninitialized,
            gain: 0xFF,
            rotation_range,
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    pub fn model(&self) -> Model {
        self.model
    }

    pub fn init_state(&self) -> ThrustmasterInitState {
        self.init_state
    }

    pub fn max_torque_nm(&self) -> f32 {
        self.max_torque_nm
    }

    pub fn rotation_range(&self) -> u16 {
        self.rotation_range
    }

    pub fn gain(&self) -> u8 {
        self.gain
    }

    pub fn set_gain(&mut self, gain: u8) {
        self.gain = gain;
    }

    pub fn set_rotation_range(&mut self, degrees: u16) {
        self.rotation_range = degrees;
    }

    pub fn supports_ffb(&self) -> bool {
        self.model.supports_ffb()
    }

    pub fn is_wheelbase(&self) -> bool {
        is_wheel_product(self.product_id)
    }

    pub fn is_pedals(&self) -> bool {
        is_pedal_product(self.product_id)
    }

    pub fn init(&mut self) {
        self.init_state = ThrustmasterInitState::Ready;
    }

    pub fn reset(&mut self) {
        self.init_state = ThrustmasterInitState::Uninitialized;
    }

    pub fn parse_input(&self, report: &[u8]) -> Option<ThrustmasterInputState> {
        if self.is_pedals() {
            return None;
        }
        parse_input_report(report)
    }

    pub fn create_encoder(&self) -> ThrustmasterConstantForceEncoder {
        ThrustmasterConstantForceEncoder::new(self.max_torque_nm)
    }

    pub fn build_init_sequence(&self) -> Vec<Vec<u8>> {
        vec![
            build_device_gain(0).to_vec(),
            build_device_gain(self.gain).to_vec(),
            build_actuator_enable(true).to_vec(),
            build_set_range_report(self.rotation_range).to_vec(),
        ]
    }
}

impl Default for ThrustmasterProtocol {
    fn default() -> Self {
        Self::new(product_ids::T300_RS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::EFFECT_REPORT_LEN;

    #[test]
    fn test_new_tgt() {
        let proto = ThrustmasterProtocol::new(product_ids::TS_XW);
        assert_eq!(proto.model(), Model::TSXW);
        assert!((proto.max_torque_nm() - 6.0).abs() < 0.01);
        assert_eq!(proto.rotation_range(), 1070);
        assert!(proto.supports_ffb());
    }

    #[test]
    fn test_new_t818() {
        let proto = ThrustmasterProtocol::new(product_ids::T818);
        assert_eq!(proto.model(), Model::T818);
        assert!((proto.max_torque_nm() - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_new_pedals() {
        let proto = ThrustmasterProtocol::new(product_ids::T_LCM);
        assert!(proto.is_pedals());
        assert!(!proto.is_wheelbase());
    }

    #[test]
    fn test_set_gain() {
        let mut proto = ThrustmasterProtocol::new(product_ids::T300_RS);
        proto.set_gain(128);
        assert_eq!(proto.gain(), 128);
    }

    #[test]
    fn test_set_rotation_range() {
        let mut proto = ThrustmasterProtocol::new(product_ids::T300_RS);
        proto.set_rotation_range(900);
        assert_eq!(proto.rotation_range(), 900);
    }

    #[test]
    fn test_init_sequence() {
        let proto = ThrustmasterProtocol::new(product_ids::T300_RS);
        let seq = proto.build_init_sequence();
        assert_eq!(seq.len(), 4);
    }

    #[test]
    fn test_encoder() {
        let proto = ThrustmasterProtocol::new(product_ids::T300_RS);
        let enc = proto.create_encoder();
        let mut out = [0u8; EFFECT_REPORT_LEN];
        enc.encode(3.0, &mut out);
        assert_eq!(out[0], 0x23);
    }

    #[test]
    fn test_default() {
        let proto = ThrustmasterProtocol::default();
        assert!(proto.supports_ffb());
    }

    #[test]
    fn test_unknown_model() {
        let proto = ThrustmasterProtocol::new(0xFFFF);
        assert_eq!(proto.model(), Model::Unknown);
    }
}
