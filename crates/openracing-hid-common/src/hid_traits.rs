//! HID device traits

use crate::{HidCommonError, HidCommonResult};
use async_trait::async_trait;

#[async_trait]
pub trait HidDevice: Send + Sync {
    async fn open(path: &str) -> HidCommonResult<Box<dyn HidDevice>>
    where
        Self: Sized;

    fn write_report(&mut self, data: &[u8]) -> HidCommonResult<usize>;

    fn read_report(&mut self, timeout_ms: u32) -> HidCommonResult<Vec<u8>>;

    fn get_device_info(&self) -> &crate::HidDeviceInfo;

    fn is_connected(&self) -> bool;

    fn close(&mut self) -> HidCommonResult<()>;
}

#[async_trait]
pub trait HidPort: Send + Sync {
    async fn list_devices(&self) -> HidCommonResult<Vec<crate::HidDeviceInfo>>;

    async fn open_device(&self, path: &str) -> HidCommonResult<Box<dyn HidDevice>>;

    async fn refresh(&self) -> HidCommonResult<()>;
}

pub mod mock {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    pub struct MockHidDevice {
        info: crate::HidDeviceInfo,
        read_queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
        write_history: Arc<Mutex<Vec<Vec<u8>>>>,
        connected: Arc<Mutex<bool>>,
    }

    impl MockHidDevice {
        pub fn new(vendor_id: u16, product_id: u16, path: impl Into<String>) -> Self {
            Self {
                info: crate::HidDeviceInfo::new(vendor_id, product_id, path.into()),
                read_queue: Arc::new(Mutex::new(VecDeque::new())),
                write_history: Arc::new(Mutex::new(Vec::new())),
                connected: Arc::new(Mutex::new(true)),
            }
        }

        pub fn queue_read(&self, data: Vec<u8>) {
            let mut queue = self.read_queue.lock().unwrap_or_else(|e| e.into_inner());
            queue.push_back(data);
        }

        pub fn get_write_history(&self) -> Vec<Vec<u8>> {
            let history = self.write_history.lock().unwrap_or_else(|e| e.into_inner());
            history.clone()
        }

        pub fn disconnect(&self) {
            let mut connected = self.connected.lock().unwrap_or_else(|e| e.into_inner());
            *connected = false;
        }

        pub fn reconnect(&self) {
            let mut connected = self.connected.lock().unwrap_or_else(|e| e.into_inner());
            *connected = true;
        }
    }

    #[async_trait]
    impl HidDevice for MockHidDevice {
        async fn open(_path: &str) -> HidCommonResult<Box<dyn HidDevice>>
        where
            Self: Sized,
        {
            unreachable!("Use MockHidDevice directly for testing")
        }

        fn write_report(&mut self, data: &[u8]) -> HidCommonResult<usize> {
            let connected = *self.connected.lock().unwrap_or_else(|e| e.into_inner());
            if !connected {
                return Err(HidCommonError::Disconnected);
            }

            let mut history = self.write_history.lock().unwrap_or_else(|e| e.into_inner());
            history.push(data.to_vec());
            Ok(data.len())
        }

        fn read_report(&mut self, _timeout_ms: u32) -> HidCommonResult<Vec<u8>> {
            let connected = *self.connected.lock().unwrap_or_else(|e| e.into_inner());
            if !connected {
                return Err(HidCommonError::Disconnected);
            }

            let mut queue = self.read_queue.lock().unwrap_or_else(|e| e.into_inner());
            queue
                .pop_front()
                .ok_or_else(|| HidCommonError::ReadError("No data available".to_string()))
        }

        fn get_device_info(&self) -> &crate::HidDeviceInfo {
            &self.info
        }

        fn is_connected(&self) -> bool {
            *self.connected.lock().unwrap_or_else(|e| e.into_inner())
        }

        fn close(&mut self) -> HidCommonResult<()> {
            self.disconnect();
            Ok(())
        }
    }

    pub struct MockHidPort {
        devices: Vec<MockHidDevice>,
    }

    impl MockHidPort {
        pub fn new() -> Self {
            Self {
                devices: Vec::new(),
            }
        }

        pub fn add_device(&mut self, device: MockHidDevice) {
            self.devices.push(device);
        }

        pub fn device_count(&self) -> usize {
            self.devices.len()
        }
    }

    #[async_trait]
    impl HidPort for MockHidPort {
        async fn list_devices(&self) -> HidCommonResult<Vec<crate::HidDeviceInfo>> {
            Ok(self
                .devices
                .iter()
                .map(|d| d.get_device_info().clone())
                .collect())
        }

        async fn open_device(&self, path: &str) -> HidCommonResult<Box<dyn HidDevice>> {
            for device in &self.devices {
                if device.info.path == path {
                    return Ok(Box::new(MockHidDevice {
                        info: device.info.clone(),
                        read_queue: Arc::clone(&device.read_queue),
                        write_history: Arc::clone(&device.write_history),
                        connected: Arc::clone(&device.connected),
                    }));
                }
            }
            Err(HidCommonError::DeviceNotFound(path.to_string()))
        }

        async fn refresh(&self) -> HidCommonResult<()> {
            Ok(())
        }
    }

    impl Default for MockHidPort {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_device_basic() {
        let device = mock::MockHidDevice::new(0x1234, 0x5678, "/dev/hidraw0");

        assert_eq!(device.get_device_info().vendor_id, 0x1234);
        assert_eq!(device.get_device_info().product_id, 0x5678);
        assert!(device.is_connected());
    }

    #[test]
    fn test_mock_device_write() {
        let mut device = mock::MockHidDevice::new(0x1234, 0x5678, "/dev/hidraw0");

        let result = device.write_report(&[0x01, 0x02, 0x03]);
        assert!(result.is_ok());
        assert_eq!(result.expect("write should succeed"), 3);

        let history = device.get_write_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0], vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_mock_device_read() {
        let device = mock::MockHidDevice::new(0x1234, 0x5678, "/dev/hidraw0");

        device.queue_read(vec![0xAA, 0xBB, 0xCC]);

        let mut device = device;
        let result = device.read_report(100);
        assert!(result.is_ok());
        assert_eq!(result.expect("read should succeed"), vec![0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn test_mock_device_disconnect() {
        let device = mock::MockHidDevice::new(0x1234, 0x5678, "/dev/hidraw0");

        device.disconnect();

        let mut device = device;
        assert!(!device.is_connected());

        let result = device.write_report(&[0x01]);
        assert!(matches!(result, Err(HidCommonError::Disconnected)));
    }

    #[test]
    fn test_mock_port() {
        let mut port = mock::MockHidPort::new();

        port.add_device(mock::MockHidDevice::new(0x1234, 0x5678, "/dev/hidraw0"));
        port.add_device(mock::MockHidDevice::new(0xABCD, 0xEF01, "/dev/hidraw1"));

        assert_eq!(port.device_count(), 2);
    }
}
