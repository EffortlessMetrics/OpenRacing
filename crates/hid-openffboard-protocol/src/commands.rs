//! OpenFFBoard vendor command protocol (HID Report ID 0xA1).
//!
//! The OpenFFBoard firmware exposes a vendor-specific HID command interface
//! alongside the standard PID FFB reports. This enables configuration,
//! firmware queries, and motor parameter tuning without OS-specific APIs.
//!
//! # Wire format
//!
//! Output (host→device) and input (device→host) share the same 25-byte
//! packed structure:
//!
//! ```text
//! Offset  Size  Field
//! ------  ----  -----
//!   0      1    Report ID (0xA1)
//!   1      1    Command type (HidCmdType)
//!   2      2    Class ID (u16 LE) — identifies the target class
//!   4      1    Instance index (u8) — 0 for most, 0xFF = broadcast
//!   5      4    Command ID (u32 LE) — operation/method within class
//!   9      8    Data / value (u64 LE) — primary payload
//!  17      8    Address (u64 LE) — secondary payload (CAN addr, etc.)
//! ```
//!
//! # Command types
//!
//! | Value | Name        | Direction    | Description                          |
//! |-------|-------------|-------------|--------------------------------------|
//! | 0     | Write       | Host→Device | Write `data` to the command          |
//! | 1     | Request     | Host→Device | Request the value of a command       |
//! | 2     | Info        | Host→Device | Request command metadata              |
//! | 3     | WriteAddr   | Host→Device | Write `data` at `addr`               |
//! | 4     | RequestAddr | Host→Device | Request value at `addr`              |
//! | 10    | ACK         | Device→Host | Successful response with data        |
//! | 13    | NotFound    | Device→Host | Command or class not found           |
//! | 14    | Notification| Device→Host | Unsolicited notification             |
//! | 15    | Error       | Device→Host | Error response                       |
//!
//! # Sources
//!
//! - `HidCommandInterface.h` in OpenFFBoard firmware (commit `cbd64db`)
//! - `usb_hid_gamepad.c` — HID report descriptor
//! - `ffb_defs.h` — `HID_ID_HIDCMD = 0xA1`

/// HID report ID for vendor commands.
pub const VENDOR_CMD_REPORT_ID: u8 = 0xA1;

/// Total size of the vendor command report in bytes (including report ID).
pub const VENDOR_CMD_REPORT_LEN: usize = 25;

/// Broadcast instance index — targets all instances of a class.
pub const INSTANCE_BROADCAST: u8 = 0xFF;

/// Command type sent from host to device or returned by device.
///
/// Source: `enum class HidCmdType` in `HidCommandInterface.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CmdType {
    /// Write a value to the specified command.
    Write = 0,
    /// Request the current value of a command.
    Request = 1,
    /// Request metadata / info about a command.
    Info = 2,
    /// Write a value at a specific address (for CAN, etc.).
    WriteAddr = 3,
    /// Request a value at a specific address.
    RequestAddr = 4,
    /// Acknowledgement — device returns requested data.
    Ack = 10,
    /// Command or class was not found.
    NotFound = 13,
    /// Unsolicited notification from device.
    Notification = 14,
    /// Error response.
    Error = 15,
}

impl CmdType {
    /// Parse from a raw byte. Returns `None` for unknown values.
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Write),
            1 => Some(Self::Request),
            2 => Some(Self::Info),
            3 => Some(Self::WriteAddr),
            4 => Some(Self::RequestAddr),
            10 => Some(Self::Ack),
            13 => Some(Self::NotFound),
            14 => Some(Self::Notification),
            15 => Some(Self::Error),
            _ => None,
        }
    }

    /// Whether this is a host→device command type.
    pub fn is_request(&self) -> bool {
        matches!(
            self,
            Self::Write | Self::Request | Self::Info | Self::WriteAddr | Self::RequestAddr
        )
    }

    /// Whether this is a device→host response type.
    pub fn is_response(&self) -> bool {
        matches!(
            self,
            Self::Ack | Self::NotFound | Self::Notification | Self::Error
        )
    }
}

/// Well-known class IDs used by the OpenFFBoard firmware.
///
/// Class IDs identify the target subsystem on the device. The firmware
/// registers handlers dynamically, but these are the standard ones.
pub mod class_ids {
    /// System commands (firmware info, device reset, etc.).
    pub const SYSTEM: u16 = 0x0000;
    /// FFB axis class.
    pub const FFB_AXIS: u16 = 0x0001;
    /// Analog axis class.
    pub const ANALOG_AXIS: u16 = 0x0002;
    /// Button source class.
    pub const BUTTON_SOURCE: u16 = 0x0003;
}

/// Well-known system command IDs.
///
/// These are used with `class_ids::SYSTEM`.
pub mod system_cmds {
    /// Request firmware version string.
    pub const FW_VERSION: u32 = 0x0000;
    /// Request hardware type / board name.
    pub const HW_TYPE: u32 = 0x0001;
    /// Device reset / reboot.
    pub const RESET: u32 = 0x0002;
    /// Save configuration to flash.
    pub const SAVE: u32 = 0x0003;
    /// Request unique device ID.
    pub const DEVICE_ID: u32 = 0x0004;
}

/// Parsed vendor command report (25 bytes).
///
/// Can represent both host→device requests and device→host responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VendorCommand {
    /// Command type (write, request, ack, error, etc.).
    pub cmd_type: CmdType,
    /// Class ID — identifies the target subsystem.
    pub class_id: u16,
    /// Instance index (0 for most, 0xFF = broadcast).
    pub instance: u8,
    /// Command ID within the class.
    pub command: u32,
    /// Primary data / value payload.
    pub data: u64,
    /// Secondary address payload (for CAN, memory ops, etc.).
    pub addr: u64,
}

impl VendorCommand {
    /// Encode this command into a 25-byte HID output report.
    pub fn encode(&self) -> [u8; VENDOR_CMD_REPORT_LEN] {
        let mut buf = [0u8; VENDOR_CMD_REPORT_LEN];
        buf[0] = VENDOR_CMD_REPORT_ID;
        buf[1] = self.cmd_type as u8;
        buf[2..4].copy_from_slice(&self.class_id.to_le_bytes());
        buf[4] = self.instance;
        buf[5..9].copy_from_slice(&self.command.to_le_bytes());
        buf[9..17].copy_from_slice(&self.data.to_le_bytes());
        buf[17..25].copy_from_slice(&self.addr.to_le_bytes());
        buf
    }

    /// Parse a 25-byte HID report into a vendor command.
    ///
    /// Returns `None` if:
    /// - the buffer is too short
    /// - byte 0 is not `VENDOR_CMD_REPORT_ID`
    /// - the command type byte is unrecognized
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < VENDOR_CMD_REPORT_LEN {
            return None;
        }
        if buf[0] != VENDOR_CMD_REPORT_ID {
            return None;
        }
        let cmd_type = CmdType::from_byte(buf[1])?;
        let class_id = u16::from_le_bytes([buf[2], buf[3]]);
        let instance = buf[4];
        let command = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
        let data = u64::from_le_bytes([
            buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15], buf[16],
        ]);
        let addr = u64::from_le_bytes([
            buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23], buf[24],
        ]);
        Some(Self {
            cmd_type,
            class_id,
            instance,
            command,
            data,
            addr,
        })
    }
}

// ---------------------------------------------------------------------------
// Convenience builders for common operations
// ---------------------------------------------------------------------------

/// Build a "request firmware version" command.
pub fn build_request_fw_version() -> [u8; VENDOR_CMD_REPORT_LEN] {
    VendorCommand {
        cmd_type: CmdType::Request,
        class_id: class_ids::SYSTEM,
        instance: 0,
        command: system_cmds::FW_VERSION,
        data: 0,
        addr: 0,
    }
    .encode()
}

/// Build a "request hardware type" command.
pub fn build_request_hw_type() -> [u8; VENDOR_CMD_REPORT_LEN] {
    VendorCommand {
        cmd_type: CmdType::Request,
        class_id: class_ids::SYSTEM,
        instance: 0,
        command: system_cmds::HW_TYPE,
        data: 0,
        addr: 0,
    }
    .encode()
}

/// Build a "request device ID" command.
pub fn build_request_device_id() -> [u8; VENDOR_CMD_REPORT_LEN] {
    VendorCommand {
        cmd_type: CmdType::Request,
        class_id: class_ids::SYSTEM,
        instance: 0,
        command: system_cmds::DEVICE_ID,
        data: 0,
        addr: 0,
    }
    .encode()
}

/// Build a "save configuration" command.
pub fn build_save_config() -> [u8; VENDOR_CMD_REPORT_LEN] {
    VendorCommand {
        cmd_type: CmdType::Write,
        class_id: class_ids::SYSTEM,
        instance: 0,
        command: system_cmds::SAVE,
        data: 0,
        addr: 0,
    }
    .encode()
}

/// Build a "reset device" command.
pub fn build_reset_device() -> [u8; VENDOR_CMD_REPORT_LEN] {
    VendorCommand {
        cmd_type: CmdType::Write,
        class_id: class_ids::SYSTEM,
        instance: 0,
        command: system_cmds::RESET,
        data: 0,
        addr: 0,
    }
    .encode()
}

/// Build a generic write command for any class/instance/command.
pub fn build_write(
    class_id: u16,
    instance: u8,
    command: u32,
    data: u64,
) -> [u8; VENDOR_CMD_REPORT_LEN] {
    VendorCommand {
        cmd_type: CmdType::Write,
        class_id,
        instance,
        command,
        data,
        addr: 0,
    }
    .encode()
}

/// Build a generic request command for any class/instance/command.
pub fn build_request(class_id: u16, instance: u8, command: u32) -> [u8; VENDOR_CMD_REPORT_LEN] {
    VendorCommand {
        cmd_type: CmdType::Request,
        class_id,
        instance,
        command,
        data: 0,
        addr: 0,
    }
    .encode()
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // CmdType parsing
    // -----------------------------------------------------------------------

    #[test]
    fn cmd_type_round_trip_all_known() {
        let known = [
            (0u8, CmdType::Write),
            (1, CmdType::Request),
            (2, CmdType::Info),
            (3, CmdType::WriteAddr),
            (4, CmdType::RequestAddr),
            (10, CmdType::Ack),
            (13, CmdType::NotFound),
            (14, CmdType::Notification),
            (15, CmdType::Error),
        ];
        for (byte, expected) in &known {
            let parsed = CmdType::from_byte(*byte);
            assert_eq!(parsed, Some(*expected));
            assert_eq!(*expected as u8, *byte);
        }
    }

    #[test]
    fn cmd_type_unknown_returns_none() {
        for b in [5, 6, 7, 8, 9, 11, 12, 16, 255] {
            assert_eq!(CmdType::from_byte(b), None);
        }
    }

    #[test]
    fn cmd_type_request_classification() {
        assert!(CmdType::Write.is_request());
        assert!(CmdType::Request.is_request());
        assert!(CmdType::Info.is_request());
        assert!(CmdType::WriteAddr.is_request());
        assert!(CmdType::RequestAddr.is_request());
        assert!(!CmdType::Ack.is_request());
        assert!(!CmdType::Error.is_request());
    }

    #[test]
    fn cmd_type_response_classification() {
        assert!(CmdType::Ack.is_response());
        assert!(CmdType::NotFound.is_response());
        assert!(CmdType::Notification.is_response());
        assert!(CmdType::Error.is_response());
        assert!(!CmdType::Write.is_response());
        assert!(!CmdType::Request.is_response());
    }

    // -----------------------------------------------------------------------
    // VendorCommand encode / parse round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn encode_report_id() {
        let cmd = VendorCommand {
            cmd_type: CmdType::Write,
            class_id: 0,
            instance: 0,
            command: 0,
            data: 0,
            addr: 0,
        };
        let buf = cmd.encode();
        assert_eq!(buf[0], VENDOR_CMD_REPORT_ID);
    }

    #[test]
    fn encode_report_length() {
        let cmd = VendorCommand {
            cmd_type: CmdType::Write,
            class_id: 0,
            instance: 0,
            command: 0,
            data: 0,
            addr: 0,
        };
        assert_eq!(cmd.encode().len(), VENDOR_CMD_REPORT_LEN);
    }

    #[test]
    fn round_trip_write() {
        let original = VendorCommand {
            cmd_type: CmdType::Write,
            class_id: 0x1234,
            instance: 7,
            command: 0xDEADBEEF,
            data: 0x0102030405060708,
            addr: 0xAABBCCDDEEFF0011,
        };
        let buf = original.encode();
        let parsed = VendorCommand::parse(&buf);
        assert_eq!(parsed, Some(original));
    }

    #[test]
    fn round_trip_ack_response() {
        let original = VendorCommand {
            cmd_type: CmdType::Ack,
            class_id: class_ids::SYSTEM,
            instance: 0,
            command: system_cmds::FW_VERSION,
            data: 0x00010203,
            addr: 0,
        };
        let buf = original.encode();
        let parsed = VendorCommand::parse(&buf);
        assert_eq!(parsed, Some(original));
    }

    #[test]
    fn round_trip_broadcast_instance() {
        let original = VendorCommand {
            cmd_type: CmdType::Request,
            class_id: class_ids::FFB_AXIS,
            instance: INSTANCE_BROADCAST,
            command: 42,
            data: 0,
            addr: 0,
        };
        let buf = original.encode();
        let parsed = VendorCommand::parse(&buf);
        assert_eq!(parsed, Some(original));
    }

    #[test]
    fn parse_too_short_returns_none() {
        let buf = [VENDOR_CMD_REPORT_ID; 10];
        assert_eq!(VendorCommand::parse(&buf), None);
    }

    #[test]
    fn parse_wrong_report_id_returns_none() {
        let mut buf = [0u8; VENDOR_CMD_REPORT_LEN];
        buf[0] = 0x42; // wrong report ID
        buf[1] = CmdType::Write as u8;
        assert_eq!(VendorCommand::parse(&buf), None);
    }

    #[test]
    fn parse_unknown_cmd_type_returns_none() {
        let mut buf = [0u8; VENDOR_CMD_REPORT_LEN];
        buf[0] = VENDOR_CMD_REPORT_ID;
        buf[1] = 0x09; // not a valid CmdType
        assert_eq!(VendorCommand::parse(&buf), None);
    }

    // -----------------------------------------------------------------------
    // Wire layout verification
    // -----------------------------------------------------------------------

    #[test]
    fn wire_layout_cmd_type_at_byte_1() {
        let cmd = VendorCommand {
            cmd_type: CmdType::Request,
            class_id: 0,
            instance: 0,
            command: 0,
            data: 0,
            addr: 0,
        };
        let buf = cmd.encode();
        assert_eq!(buf[1], CmdType::Request as u8);
    }

    #[test]
    fn wire_layout_class_id_le16_at_bytes_2_3() {
        let cmd = VendorCommand {
            cmd_type: CmdType::Write,
            class_id: 0xABCD,
            instance: 0,
            command: 0,
            data: 0,
            addr: 0,
        };
        let buf = cmd.encode();
        assert_eq!(buf[2], 0xCD); // LE low byte
        assert_eq!(buf[3], 0xAB); // LE high byte
    }

    #[test]
    fn wire_layout_instance_at_byte_4() {
        let cmd = VendorCommand {
            cmd_type: CmdType::Write,
            class_id: 0,
            instance: 0xFF,
            command: 0,
            data: 0,
            addr: 0,
        };
        let buf = cmd.encode();
        assert_eq!(buf[4], 0xFF);
    }

    #[test]
    fn wire_layout_command_le32_at_bytes_5_8() {
        let cmd = VendorCommand {
            cmd_type: CmdType::Write,
            class_id: 0,
            instance: 0,
            command: 0x12345678,
            data: 0,
            addr: 0,
        };
        let buf = cmd.encode();
        assert_eq!(buf[5..9], [0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn wire_layout_data_le64_at_bytes_9_16() {
        let cmd = VendorCommand {
            cmd_type: CmdType::Write,
            class_id: 0,
            instance: 0,
            command: 0,
            data: 0x0102030405060708,
            addr: 0,
        };
        let buf = cmd.encode();
        assert_eq!(buf[9..17], [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]);
    }

    #[test]
    fn wire_layout_addr_le64_at_bytes_17_24() {
        let cmd = VendorCommand {
            cmd_type: CmdType::Write,
            class_id: 0,
            instance: 0,
            command: 0,
            data: 0,
            addr: 0xAABBCCDDEEFF0011,
        };
        let buf = cmd.encode();
        assert_eq!(
            buf[17..25],
            [0x11, 0x00, 0xFF, 0xEE, 0xDD, 0xCC, 0xBB, 0xAA]
        );
    }

    // -----------------------------------------------------------------------
    // Convenience builders
    // -----------------------------------------------------------------------

    #[test]
    fn fw_version_request_structure() {
        let buf = build_request_fw_version();
        assert_eq!(buf[0], VENDOR_CMD_REPORT_ID);
        assert_eq!(buf[1], CmdType::Request as u8);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), class_ids::SYSTEM);
        assert_eq!(buf[4], 0); // instance 0
        assert_eq!(
            u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]),
            system_cmds::FW_VERSION
        );
    }

    #[test]
    fn hw_type_request_structure() {
        let buf = build_request_hw_type();
        let cmd = VendorCommand::parse(&buf);
        assert!(cmd.is_some());
        let cmd = cmd.as_ref().map(|c| c.command);
        assert_eq!(cmd, Some(system_cmds::HW_TYPE));
    }

    #[test]
    fn device_id_request_structure() {
        let buf = build_request_device_id();
        let parsed = VendorCommand::parse(&buf);
        assert!(parsed.is_some());
        let p = parsed.as_ref();
        assert_eq!(p.map(|c| c.command), Some(system_cmds::DEVICE_ID));
    }

    #[test]
    fn save_config_is_write_type() {
        let buf = build_save_config();
        let parsed = VendorCommand::parse(&buf);
        assert!(parsed.is_some());
        assert_eq!(parsed.as_ref().map(|c| c.cmd_type), Some(CmdType::Write));
    }

    #[test]
    fn reset_device_is_write_type() {
        let buf = build_reset_device();
        let parsed = VendorCommand::parse(&buf);
        assert!(parsed.is_some());
        assert_eq!(parsed.as_ref().map(|c| c.cmd_type), Some(CmdType::Write));
        assert_eq!(parsed.as_ref().map(|c| c.command), Some(system_cmds::RESET));
    }

    #[test]
    fn generic_write_carries_data() {
        let buf = build_write(0x0001, 0, 0x42, 12345);
        let parsed = VendorCommand::parse(&buf);
        assert!(parsed.is_some());
        let p = parsed.as_ref();
        assert_eq!(p.map(|c| c.cmd_type), Some(CmdType::Write));
        assert_eq!(p.map(|c| c.class_id), Some(0x0001));
        assert_eq!(p.map(|c| c.command), Some(0x42));
        assert_eq!(p.map(|c| c.data), Some(12345));
    }

    #[test]
    fn generic_request_has_zero_data() {
        let buf = build_request(class_ids::FFB_AXIS, 1, 0x10);
        let parsed = VendorCommand::parse(&buf);
        assert!(parsed.is_some());
        let p = parsed.as_ref();
        assert_eq!(p.map(|c| c.cmd_type), Some(CmdType::Request));
        assert_eq!(p.map(|c| c.class_id), Some(class_ids::FFB_AXIS));
        assert_eq!(p.map(|c| c.instance), Some(1));
        assert_eq!(p.map(|c| c.command), Some(0x10));
        assert_eq!(p.map(|c| c.data), Some(0));
    }

    // -----------------------------------------------------------------------
    // Constants
    // -----------------------------------------------------------------------

    #[test]
    fn vendor_cmd_report_id_matches_firmware() {
        // HID_ID_HIDCMD = 0xA1 in ffb_defs.h
        assert_eq!(VENDOR_CMD_REPORT_ID, 0xA1);
    }

    #[test]
    fn vendor_cmd_report_len_matches_firmware() {
        // Firmware struct HID_CMD_Data_t is exactly 25 bytes packed
        // 1 + 1 + 2 + 1 + 4 + 8 + 8 = 25
        assert_eq!(VENDOR_CMD_REPORT_LEN, 25);
    }

    #[test]
    fn broadcast_instance_is_0xff() {
        // 0xFF = broadcast to all instances per firmware docs
        assert_eq!(INSTANCE_BROADCAST, 0xFF);
    }
}
