//! Type definitions for Heusinkveld protocol

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PedalModel {
    Sprint,
    Ultimate,
    Pro,
    Unknown,
}

impl Default for PedalModel {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PedalCapabilities {
    pub max_load_kg: f32,
    pub has_hydraulic_damping: bool,
    pub has_load_cell: bool,
    pub pedal_count: usize,
}

impl Default for PedalCapabilities {
    fn default() -> Self {
        Self {
            max_load_kg: 140.0,
            has_hydraulic_damping: true,
            has_load_cell: true,
            pedal_count: 3,
        }
    }
}

impl PedalCapabilities {
    pub fn for_model(model: PedalModel) -> Self {
        match model {
            PedalModel::Sprint => Self {
                max_load_kg: 55.0,
                has_hydraulic_damping: true,
                has_load_cell: true,
                pedal_count: 2,
            },
            PedalModel::Ultimate => Self {
                max_load_kg: 140.0,
                has_hydraulic_damping: true,
                has_load_cell: true,
                pedal_count: 3,
            },
            PedalModel::Pro => Self {
                max_load_kg: 200.0,
                has_hydraulic_damping: true,
                has_load_cell: true,
                pedal_count: 3,
            },
            PedalModel::Unknown => Self::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PedalStatus {
    Disconnected,
    Ready,
    Calibrating,
    Error,
}

impl Default for PedalStatus {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl PedalStatus {
    pub fn from_flags(flags: u8) -> Self {
        if flags & 0x01 == 0 {
            return Self::Disconnected;
        }
        if flags & 0x02 == 0 {
            return Self::Calibrating;
        }
        if flags & 0x04 != 0 {
            return Self::Error;
        }
        Self::Ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pedal_capabilities_sprint() {
        let caps = PedalCapabilities::for_model(PedalModel::Sprint);
        assert_eq!(caps.max_load_kg, 55.0);
        assert_eq!(caps.pedal_count, 2);
    }

    #[test]
    fn test_pedal_capabilities_ultimate() {
        let caps = PedalCapabilities::for_model(PedalModel::Ultimate);
        assert_eq!(caps.max_load_kg, 140.0);
        assert_eq!(caps.pedal_count, 3);
    }

    #[test]
    fn test_pedal_capabilities_pro() {
        let caps = PedalCapabilities::for_model(PedalModel::Pro);
        assert_eq!(caps.max_load_kg, 200.0);
    }

    #[test]
    fn test_pedal_status_from_flags() {
        assert_eq!(PedalStatus::from_flags(0x00), PedalStatus::Disconnected);
        assert_eq!(PedalStatus::from_flags(0x01), PedalStatus::Calibrating);
        assert_eq!(PedalStatus::from_flags(0x03), PedalStatus::Ready);
        assert_eq!(PedalStatus::from_flags(0x07), PedalStatus::Error);
    }
}
