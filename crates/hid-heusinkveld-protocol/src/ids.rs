//! Device IDs for Heusinkveld products

pub const HEUSINKVELD_VENDOR_ID: u16 = 0x16D0;

pub const HEUSINKVELD_SPRINT_PID: u16 = 0x1156;
pub const HEUSINKVELD_ULTIMATE_PID: u16 = 0x1157;
pub const HEUSINKVELD_PRO_PID: u16 = 0x1158;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeusinkveldModel {
    Sprint,
    Ultimate,
    Pro,
    Unknown,
}

impl HeusinkveldModel {
    pub fn from_product_id(product_id: u16) -> Self {
        match product_id {
            HEUSINKVELD_SPRINT_PID => Self::Sprint,
            HEUSINKVELD_ULTIMATE_PID => Self::Ultimate,
            HEUSINKVELD_PRO_PID => Self::Pro,
            _ => Self::Unknown,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Sprint => "Heusinkveld Sprint",
            Self::Ultimate => "Heusinkveld Ultimate+",
            Self::Pro => "Heusinkveld Pro",
            Self::Unknown => "Unknown Heusinkveld Device",
        }
    }

    pub fn max_load_kg(&self) -> f32 {
        match self {
            Self::Sprint => 55.0,
            Self::Ultimate => 140.0,
            Self::Pro => 200.0,
            Self::Unknown => 140.0,
        }
    }

    pub fn pedal_count(&self) -> usize {
        match self {
            Self::Sprint => 2,
            Self::Ultimate => 3,
            Self::Pro => 3,
            Self::Unknown => 3,
        }
    }
}

pub fn heusinkveld_model_from_info(vendor_id: u16, product_id: u16) -> HeusinkveldModel {
    if vendor_id != HEUSINKVELD_VENDOR_ID {
        return HeusinkveldModel::Unknown;
    }
    HeusinkveldModel::from_product_id(product_id)
}

pub fn is_heusinkveld_device(vendor_id: u16) -> bool {
    vendor_id == HEUSINKVELD_VENDOR_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_from_pid() {
        assert_eq!(
            HeusinkveldModel::from_product_id(HEUSINKVELD_SPRINT_PID),
            HeusinkveldModel::Sprint
        );
        assert_eq!(
            HeusinkveldModel::from_product_id(HEUSINKVELD_ULTIMATE_PID),
            HeusinkveldModel::Ultimate
        );
        assert_eq!(
            HeusinkveldModel::from_product_id(0xFFFF),
            HeusinkveldModel::Unknown
        );
    }

    #[test]
    fn test_max_load() {
        assert_eq!(HeusinkveldModel::Sprint.max_load_kg(), 55.0);
        assert_eq!(HeusinkveldModel::Ultimate.max_load_kg(), 140.0);
        assert_eq!(HeusinkveldModel::Pro.max_load_kg(), 200.0);
    }

    #[test]
    fn test_pedal_count() {
        assert_eq!(HeusinkveldModel::Sprint.pedal_count(), 2);
        assert_eq!(HeusinkveldModel::Ultimate.pedal_count(), 3);
        assert_eq!(HeusinkveldModel::Pro.pedal_count(), 3);
    }

    #[test]
    fn test_display_name() {
        assert_eq!(
            HeusinkveldModel::Sprint.display_name(),
            "Heusinkveld Sprint"
        );
        assert_eq!(
            HeusinkveldModel::Ultimate.display_name(),
            "Heusinkveld Ultimate+"
        );
    }
}
