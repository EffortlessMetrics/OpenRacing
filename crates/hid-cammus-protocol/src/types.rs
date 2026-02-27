//! Cammus device model classification and capabilities.

/// Cammus wheel model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CammusModel {
    /// Cammus C5 – 5 Nm desktop direct drive.
    C5,
    /// Cammus C12 – 12 Nm desktop direct drive.
    C12,
}

impl CammusModel {
    /// Construct a model from a USB product ID, returning `None` for unknown PIDs.
    pub fn from_pid(pid: u16) -> Option<Self> {
        match pid {
            crate::ids::PRODUCT_C5 => Some(CammusModel::C5),
            crate::ids::PRODUCT_C12 => Some(CammusModel::C12),
            _ => None,
        }
    }

    /// Maximum continuous torque in Newton-metres.
    pub fn max_torque_nm(self) -> f32 {
        match self {
            CammusModel::C5 => 5.0,
            CammusModel::C12 => 12.0,
        }
    }

    /// Human-readable product name.
    pub fn name(self) -> &'static str {
        match self {
            CammusModel::C5 => "Cammus C5",
            CammusModel::C12 => "Cammus C12",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{PRODUCT_C12, PRODUCT_C5};

    #[test]
    fn from_pid_known() {
        assert_eq!(CammusModel::from_pid(PRODUCT_C5), Some(CammusModel::C5));
        assert_eq!(CammusModel::from_pid(PRODUCT_C12), Some(CammusModel::C12));
    }

    #[test]
    fn from_pid_unknown() {
        assert_eq!(CammusModel::from_pid(0xFFFF), None);
    }

    #[test]
    fn torque_values() {
        assert!((CammusModel::C5.max_torque_nm() - 5.0).abs() < 0.001);
        assert!((CammusModel::C12.max_torque_nm() - 12.0).abs() < 0.001);
    }

    #[test]
    fn names() {
        assert_eq!(CammusModel::C5.name(), "Cammus C5");
        assert_eq!(CammusModel::C12.name(), "Cammus C12");
    }
}
