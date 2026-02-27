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
}
