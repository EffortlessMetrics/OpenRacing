//! Snapshot tests for HidDeviceInfo — ensure display and serialized output is stable.

use openracing_hid_common::device_info::HidDeviceInfo;

#[test]
fn snapshot_hid_device_info_minimal() {
    let info = HidDeviceInfo::new(0x346E, 0x0005, "\\\\.\\HID#VID_346E".to_string());
    insta::assert_debug_snapshot!("hid_device_info_minimal", info);
}

#[test]
fn snapshot_hid_device_info_full() {
    let info = HidDeviceInfo::new(0x346E, 0x0005, "\\\\.\\HID#VID_346E".to_string())
        .with_manufacturer("MOZA Racing")
        .with_product_name("MOZA R9 V2")
        .with_serial("MZ-2024-001");
    insta::assert_debug_snapshot!("hid_device_info_full", info);
}

#[test]
fn snapshot_hid_device_info_display_name_with_product() {
    let info = HidDeviceInfo::new(0x346E, 0x0005, "/dev/hidraw0".to_string())
        .with_product_name("SimuCube 2 Pro");
    insta::assert_snapshot!("hid_device_info_display_name_product", info.display_name());
}

#[test]
fn snapshot_hid_device_info_display_name_manufacturer_only() {
    let info =
        HidDeviceInfo::new(0x0EB7, 0x183B, "/dev/hidraw1".to_string()).with_manufacturer("Fanatec");
    insta::assert_snapshot!(
        "hid_device_info_display_name_manufacturer",
        info.display_name()
    );
}

#[test]
fn snapshot_hid_device_info_display_name_fallback() {
    let info = HidDeviceInfo::new(0x1234, 0x5678, "/dev/hidraw2".to_string());
    insta::assert_snapshot!("hid_device_info_display_name_fallback", info.display_name());
}

#[test]
fn snapshot_hid_device_info_json() {
    let info = HidDeviceInfo::new(0x346E, 0x0005, "\\\\.\\HID#VID_346E".to_string())
        .with_manufacturer("MOZA Racing")
        .with_product_name("MOZA R9 V2")
        .with_serial("MZ-2024-001");
    insta::assert_json_snapshot!("hid_device_info_json", info);
}

#[test]
fn snapshot_hid_device_info_default() {
    insta::assert_debug_snapshot!("hid_device_info_default", HidDeviceInfo::default());
}
