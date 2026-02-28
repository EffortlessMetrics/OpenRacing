//! SimpleMotion V2 device types: models, categories, and device identity.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmDeviceCategory {
    Wheelbase,
    ServoDrive,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmDeviceIdentity {
    pub product_id: u16,
    pub name: &'static str,
    pub category: SmDeviceCategory,
    pub supports_ffb: bool,
    pub max_torque_nm: Option<f32>,
    pub max_rpm: Option<u32>,
}

pub fn identify_device(product_id: u16) -> SmDeviceIdentity {
    match product_id {
        0x6050 => SmDeviceIdentity {
            product_id,
            name: "Simucube 1 / IONI Servo Drive",
            category: SmDeviceCategory::Wheelbase,
            supports_ffb: true,
            max_torque_nm: Some(15.0),
            max_rpm: Some(10000),
        },
        0x6051 => SmDeviceIdentity {
            product_id,
            name: "Simucube 2 / IONI Premium Servo Drive",
            category: SmDeviceCategory::Wheelbase,
            supports_ffb: true,
            max_torque_nm: Some(35.0),
            max_rpm: Some(15000),
        },
        0x6052 => SmDeviceIdentity {
            product_id,
            name: "Simucube Sport / ARGON Servo Drive",
            category: SmDeviceCategory::Wheelbase,
            supports_ffb: true,
            max_torque_nm: Some(10.0),
            max_rpm: Some(8000),
        },
        _ => SmDeviceIdentity {
            product_id,
            name: "Unknown SimpleMotion Device",
            category: SmDeviceCategory::Unknown,
            supports_ffb: false,
            max_torque_nm: None,
            max_rpm: None,
        },
    }
}

pub fn is_wheelbase_product(product_id: u16) -> bool {
    matches!(
        identify_device(product_id).category,
        SmDeviceCategory::Wheelbase
    )
}

pub fn sm_device_identity(product_id: u16) -> SmDeviceIdentity {
    identify_device(product_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_ioni() {
        let identity = identify_device(0x6050);
        assert_eq!(identity.name, "Simucube 1 / IONI Servo Drive");
        assert!(identity.supports_ffb);
        assert!(identity.max_torque_nm.is_some());
    }

    #[test]
    fn test_identify_ioni_premium() {
        let identity = identify_device(0x6051);
        assert_eq!(identity.name, "Simucube 2 / IONI Premium Servo Drive");
        assert!(identity.supports_ffb);
    }

    #[test]
    fn test_identify_argon() {
        let identity = identify_device(0x6052);
        assert_eq!(identity.name, "Simucube Sport / ARGON Servo Drive");
        assert!(identity.supports_ffb);
    }

    #[test]
    fn test_identify_simucube_1() {
        let identity = identify_device(0x6050);
        assert_eq!(identity.name, "Simucube 1 / IONI Servo Drive");
        assert!(identity.supports_ffb);
    }

    #[test]
    fn test_identify_simucube_2() {
        let identity = identify_device(0x6051);
        assert_eq!(identity.name, "Simucube 2 / IONI Premium Servo Drive");
        assert!(identity.supports_ffb);
    }

    #[test]
    fn test_identify_unknown() {
        let identity = identify_device(0xFFFF);
        assert_eq!(identity.name, "Unknown SimpleMotion Device");
        assert!(!identity.supports_ffb);
    }

    #[test]
    fn test_is_wheelbase_product_simucube() {
        assert!(is_wheelbase_product(0x6050));
        assert!(is_wheelbase_product(0x6051));
        assert!(is_wheelbase_product(0x6052));
    }

    #[test]
    fn test_is_wheelbase_product_unknown() {
        assert!(!is_wheelbase_product(0xFFFF));
    }
}
