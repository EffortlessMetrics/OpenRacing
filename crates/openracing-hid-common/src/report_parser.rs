//! HID report parsing utilities

use crate::{HidCommonError, HidCommonResult};

pub struct ReportParser {
    buffer: Vec<u8>,
    position: usize,
}

impl ReportParser {
    pub fn new(data: impl Into<Vec<u8>>) -> Self {
        Self {
            buffer: data.into(),
            position: 0,
        }
    }

    pub fn from_slice(data: &[u8]) -> Self {
        Self {
            buffer: data.to_vec(),
            position: 0,
        }
    }

    pub fn remaining(&self) -> usize {
        self.buffer.len().saturating_sub(self.position)
    }

    pub fn read_u8(&mut self) -> HidCommonResult<u8> {
        if self.position >= self.buffer.len() {
            return Err(HidCommonError::InvalidReport(
                "Unexpected end of data".to_string(),
            ));
        }
        let value = self.buffer[self.position];
        self.position += 1;
        Ok(value)
    }

    pub fn read_i8(&mut self) -> HidCommonResult<i8> {
        Ok(self.read_u8()? as i8)
    }

    pub fn read_u16_le(&mut self) -> HidCommonResult<u16> {
        let lo = self.read_u8()? as u16;
        let hi = self.read_u8()? as u16;
        Ok(lo | (hi << 8))
    }

    pub fn read_i16_le(&mut self) -> HidCommonResult<i16> {
        Ok(self.read_u16_le()? as i16)
    }

    pub fn read_u16_be(&mut self) -> HidCommonResult<u16> {
        let hi = self.read_u8()? as u16;
        let lo = self.read_u8()? as u16;
        Ok(lo | (hi << 8))
    }

    pub fn read_u32_le(&mut self) -> HidCommonResult<u32> {
        let b0 = self.read_u8()? as u32;
        let b1 = self.read_u8()? as u32;
        let b2 = self.read_u8()? as u32;
        let b3 = self.read_u8()? as u32;
        Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
    }

    pub fn read_i32_le(&mut self) -> HidCommonResult<i32> {
        Ok(self.read_u32_le()? as i32)
    }

    pub fn read_bytes(&mut self, count: usize) -> HidCommonResult<Vec<u8>> {
        if self.position + count > self.buffer.len() {
            return Err(HidCommonError::InvalidReport(
                "Unexpected end of data".to_string(),
            ));
        }
        let result = self.buffer[self.position..self.position + count].to_vec();
        self.position += count;
        Ok(result)
    }

    pub fn read_f32_le(&mut self) -> HidCommonResult<f32> {
        let bits = self.read_u32_le()?;
        Ok(f32::from_le_bytes(bits.to_le_bytes()))
    }

    pub fn peek_u8(&mut self) -> HidCommonResult<u8> {
        if self.position >= self.buffer.len() {
            return Err(HidCommonError::InvalidReport(
                "Unexpected end of data".to_string(),
            ));
        }
        Ok(self.buffer[self.position])
    }

    pub fn skip(&mut self, count: usize) {
        self.position = (self.position + count).min(self.buffer.len());
    }

    pub fn reset(&mut self) {
        self.position = 0;
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.buffer
    }

    pub fn slice(&self) -> &[u8] {
        &self.buffer
    }
}

pub struct ReportBuilder {
    buffer: Vec<u8>,
}

impl ReportBuilder {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0u8; capacity],
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
        }
    }

    pub fn write_u8(&mut self, value: u8) -> &mut Self {
        self.buffer.push(value);
        self
    }

    pub fn write_i8(&mut self, value: i8) -> &mut Self {
        self.buffer.push(value as u8);
        self
    }

    pub fn write_u16_le(&mut self, value: u16) -> &mut Self {
        self.buffer.push((value & 0xFF) as u8);
        self.buffer.push((value >> 8) as u8);
        self
    }

    pub fn write_i16_le(&mut self, value: i16) -> &mut Self {
        self.write_u16_le(value as u16)
    }

    pub fn write_u32_le(&mut self, value: u32) -> &mut Self {
        self.buffer.push((value & 0xFF) as u8);
        self.buffer.push(((value >> 8) & 0xFF) as u8);
        self.buffer.push(((value >> 16) & 0xFF) as u8);
        self.buffer.push(((value >> 24) & 0xFF) as u8);
        self
    }

    pub fn write_bytes(&mut self, data: &[u8]) -> &mut Self {
        self.buffer.extend_from_slice(data);
        self
    }

    pub fn write_f32_le(&mut self, value: f32) -> &mut Self {
        let bytes = value.to_le_bytes();
        self.write_u32_le(u32::from_le_bytes(bytes))
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.buffer
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buffer
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

impl Default for ReportBuilder {
    fn default() -> Self {
        Self::new(64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // ReportParser basic reads
    // -----------------------------------------------------------------------

    #[test]
    fn test_report_parser_u8() {
        let data = vec![0x01, 0x02, 0x03];
        let mut parser = ReportParser::new(data);

        assert_eq!(parser.read_u8().expect("read byte"), 0x01);
        assert_eq!(parser.read_u8().expect("read byte"), 0x02);
        assert_eq!(parser.read_u8().expect("read byte"), 0x03);
        assert!(parser.read_u8().is_err());
    }

    #[test]
    fn test_report_parser_u16_le() {
        let data = vec![0x34, 0x12];
        let mut parser = ReportParser::new(data);

        assert_eq!(parser.read_u16_le().expect("read u16"), 0x1234);
    }

    #[test]
    fn test_report_parser_u32_le() {
        let data = vec![0x78, 0x56, 0x34, 0x12];
        let mut parser = ReportParser::new(data);

        assert_eq!(parser.read_u32_le().expect("read u32"), 0x12345678);
    }

    #[test]
    fn test_report_parser_bytes() {
        let data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let mut parser = ReportParser::new(data);

        let bytes = parser.read_bytes(3).expect("read bytes");
        assert_eq!(bytes, vec![0x01, 0x02, 0x03]);

        let bytes = parser.read_bytes(2).expect("read bytes");
        assert_eq!(bytes, vec![0x04, 0x05]);
    }

    #[test]
    fn test_report_builder() {
        let mut builder = ReportBuilder::new(0);

        builder
            .write_u8(0x01)
            .write_u16_le(0x1234)
            .write_u32_le(0x12345678)
            .write_bytes(&[0xAA, 0xBB]);

        let data = builder.into_inner();
        assert_eq!(
            data,
            vec![0x01, 0x34, 0x12, 0x78, 0x56, 0x34, 0x12, 0xAA, 0xBB]
        );
    }

    #[test]
    fn test_report_parser_f32() {
        let value: f32 = std::f32::consts::PI;
        let bytes = value.to_le_bytes();

        let mut parser = ReportParser::new(bytes.to_vec());
        let parsed = parser.read_f32_le().expect("read f32");

        assert!((parsed - value).abs() < 0.0001);
    }

    // -----------------------------------------------------------------------
    // Signed reads
    // -----------------------------------------------------------------------

    #[test]
    fn test_report_parser_i8() {
        let data = vec![0xFF]; // -1 as i8
        let mut parser = ReportParser::new(data);
        assert_eq!(parser.read_i8().expect("read i8"), -1);
    }

    #[test]
    fn test_report_parser_i8_positive() {
        let data = vec![0x7F]; // 127 as i8
        let mut parser = ReportParser::new(data);
        assert_eq!(parser.read_i8().expect("read i8"), 127);
    }

    #[test]
    fn test_report_parser_i16_le() {
        // -256 in LE: 0x00, 0xFF => i16 = 0xFF00 = -256
        let data = vec![0x00, 0xFF];
        let mut parser = ReportParser::new(data);
        assert_eq!(parser.read_i16_le().expect("read i16"), -256);
    }

    #[test]
    fn test_report_parser_i32_le() {
        // -1 in i32 LE: 0xFF 0xFF 0xFF 0xFF
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let mut parser = ReportParser::new(data);
        assert_eq!(parser.read_i32_le().expect("read i32"), -1);
    }

    // -----------------------------------------------------------------------
    // Big-endian
    // -----------------------------------------------------------------------

    #[test]
    fn test_report_parser_u16_be() {
        let data = vec![0x12, 0x34];
        let mut parser = ReportParser::new(data);
        assert_eq!(parser.read_u16_be().expect("read u16_be"), 0x1234);
    }

    // -----------------------------------------------------------------------
    // Navigation: remaining, skip, reset, peek
    // -----------------------------------------------------------------------

    #[test]
    fn test_report_parser_remaining() {
        let data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let mut parser = ReportParser::new(data);

        assert_eq!(parser.remaining(), 5);
        parser.read_u8().expect("read byte");
        assert_eq!(parser.remaining(), 4);
        parser.read_u16_le().expect("read u16");
        assert_eq!(parser.remaining(), 2);
    }

    #[test]
    fn test_report_parser_skip() {
        let data = vec![0x01, 0x02, 0x03, 0x04];
        let mut parser = ReportParser::new(data);

        parser.skip(2);
        assert_eq!(parser.read_u8().expect("read byte after skip"), 0x03);
    }

    #[test]
    fn test_report_parser_skip_past_end() {
        let data = vec![0x01, 0x02];
        let mut parser = ReportParser::new(data);

        parser.skip(100); // should clamp, not panic
        assert_eq!(parser.remaining(), 0);
        assert!(parser.read_u8().is_err());
    }

    #[test]
    fn test_report_parser_reset() {
        let data = vec![0xAA, 0xBB, 0xCC];
        let mut parser = ReportParser::new(data);

        parser.read_u8().expect("first read");
        parser.read_u8().expect("second read");
        assert_eq!(parser.remaining(), 1);

        parser.reset();
        assert_eq!(parser.remaining(), 3);
        assert_eq!(parser.read_u8().expect("read after reset"), 0xAA);
    }

    #[test]
    fn test_report_parser_peek() {
        let data = vec![0x42, 0x43];
        let mut parser = ReportParser::new(data);

        assert_eq!(parser.peek_u8().expect("peek byte"), 0x42);
        // Peek should not advance
        assert_eq!(parser.peek_u8().expect("peek again"), 0x42);
        assert_eq!(parser.remaining(), 2);

        parser.read_u8().expect("consume byte");
        assert_eq!(parser.peek_u8().expect("peek next"), 0x43);
    }

    #[test]
    fn test_report_parser_peek_at_end() {
        let data = vec![0x01];
        let mut parser = ReportParser::new(data);
        parser.read_u8().expect("consume");
        assert!(parser.peek_u8().is_err());
    }

    // -----------------------------------------------------------------------
    // Over-read errors
    // -----------------------------------------------------------------------

    #[test]
    fn test_report_parser_u16_over_read() {
        let data = vec![0x01]; // only 1 byte, need 2
        let mut parser = ReportParser::new(data);
        assert!(parser.read_u16_le().is_err());
    }

    #[test]
    fn test_report_parser_u32_over_read() {
        let data = vec![0x01, 0x02]; // only 2 bytes, need 4
        let mut parser = ReportParser::new(data);
        assert!(parser.read_u32_le().is_err());
    }

    #[test]
    fn test_report_parser_bytes_over_read() {
        let data = vec![0x01, 0x02];
        let mut parser = ReportParser::new(data);
        assert!(parser.read_bytes(5).is_err());
    }

    #[test]
    fn test_report_parser_f32_over_read() {
        let data = vec![0x01, 0x02, 0x03]; // only 3 bytes, need 4
        let mut parser = ReportParser::new(data);
        assert!(parser.read_f32_le().is_err());
    }

    // -----------------------------------------------------------------------
    // Constructors and accessors
    // -----------------------------------------------------------------------

    #[test]
    fn test_report_parser_from_slice() {
        let data = [0x01, 0x02, 0x03];
        let mut parser = ReportParser::from_slice(&data);
        assert_eq!(parser.read_u8().expect("read"), 0x01);
        assert_eq!(parser.remaining(), 2);
    }

    #[test]
    fn test_report_parser_into_inner() {
        let data = vec![0x0A, 0x0B, 0x0C];
        let parser = ReportParser::new(data.clone());
        assert_eq!(parser.into_inner(), data);
    }

    #[test]
    fn test_report_parser_slice() {
        let data = vec![0x01, 0x02, 0x03];
        let parser = ReportParser::new(data.clone());
        assert_eq!(parser.slice(), data.as_slice());
    }

    // -----------------------------------------------------------------------
    // ReportBuilder additional coverage
    // -----------------------------------------------------------------------

    #[test]
    fn test_report_builder_f32() {
        let mut builder = ReportBuilder::with_capacity(4);
        builder.write_f32_le(std::f32::consts::E);
        let data = builder.into_inner();

        let mut parser = ReportParser::new(data);
        let value = parser.read_f32_le().expect("read f32");
        assert!((value - std::f32::consts::E).abs() < 0.0001);
    }

    #[test]
    fn test_report_builder_signed_writes() {
        let mut builder = ReportBuilder::with_capacity(8);
        builder.write_i8(-1).write_i16_le(-1000);
        let data = builder.into_inner();

        let mut parser = ReportParser::new(data);
        assert_eq!(parser.read_i8().expect("read i8"), -1);
        assert_eq!(parser.read_i16_le().expect("read i16"), -1000);
    }

    #[test]
    fn test_report_builder_default() {
        let builder = ReportBuilder::default();
        assert_eq!(builder.len(), 64); // default capacity pre-fills with zeros
        assert!(!builder.is_empty());
    }

    #[test]
    fn test_report_builder_with_capacity_empty() {
        let builder = ReportBuilder::with_capacity(8);
        assert_eq!(builder.len(), 0);
        assert!(builder.is_empty());
    }

    #[test]
    fn test_report_builder_as_slice() {
        let mut builder = ReportBuilder::with_capacity(4);
        builder.write_u8(0xDE).write_u8(0xAD);
        assert_eq!(builder.as_slice(), &[0xDE, 0xAD]);
    }

    // -----------------------------------------------------------------------
    // Builder → Parser round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_builder_parser_round_trip() {
        let mut builder = ReportBuilder::with_capacity(32);
        builder
            .write_u8(0x42)
            .write_u16_le(0xCAFE)
            .write_u32_le(0xDEADBEEF)
            .write_f32_le(1.234)
            .write_i8(-5)
            .write_i16_le(-12345);

        let mut parser = ReportParser::new(builder.into_inner());

        assert_eq!(parser.read_u8().expect("u8"), 0x42);
        assert_eq!(parser.read_u16_le().expect("u16"), 0xCAFE);
        assert_eq!(parser.read_u32_le().expect("u32"), 0xDEADBEEF);
        assert!((parser.read_f32_le().expect("f32") - 1.234).abs() < 0.0001);
        assert_eq!(parser.read_i8().expect("i8"), -5);
        assert_eq!(parser.read_i16_le().expect("i16"), -12345);
        assert_eq!(parser.remaining(), 0);
    }
}
