# Contributing USB Captures

This guide explains how to capture USB HID descriptors from a sim-racing device
so that OpenRacing can add verified support for it.

Community captures are the primary way we add new devices. If you own hardware
that isn't yet supported (see the "Devices Under Investigation" table in
`docs/protocols/SOURCES.md`), a USB capture takes about 5 minutes and is
extremely valuable.

---

## What We Need

For every new device we need:

1. **Vendor ID (VID) and Product ID (PID)** from the USB descriptor
2. **HID Report Descriptor** — tells us which reports the device accepts/sends
3. **Protocol type** — is it standard HID PIDFF, or proprietary vendor commands?

---

## How to Capture on Linux

```bash
# 1. Plug in your device. Find it:
lsusb

# 2. Note the Bus and Device numbers from the output. Example:
#    Bus 003 Device 007: ID 346E:0005 Moza Racing R3

# 3. Get the full descriptor (replace 003/007 with your values):
sudo lsusb -v -s 003:007 > my_device_descriptor.txt

# 4. Get the HID report descriptor (use hidtools if installed):
sudo usbhid-dump -d 346E:0005 > my_device_hid_report.txt

# Optional: capture live traffic (requires usbmon):
sudo modprobe usbmon
sudo wireshark
# Select the correct usbmonX interface and replicate an FFB effect
```

---

## How to Capture on Windows

1. Download **USBTreeView** (free) from [uwe-sieber.de](https://www.uwe-sieber.de/usbtreeview_e.html)
2. Plug in your device and find it in the tree
3. Right-click → "Copy Device Summary to Clipboard"
4. Also note the VID and PID from the "Device ID" field (e.g., `VID_346E&PID_0005`)

For HID report descriptor capture, use **HID Report Descriptor Tool** or
**USBlyzer** (trial available).

For live FFB traffic capture, use **Wireshark** with USBPcap:
1. Install [USBPcap](https://desowin.org/usbpcap/)
2. Open Wireshark → select the USBPcap interface connected to your device
3. Apply an FFB effect via a game or test tool
4. Save the `.pcapng` file

---

## How to Submit

Open a GitHub issue titled `[Device Capture] <Brand> <Model>` and attach:
- The descriptor text file from `lsusb -v` or USBTreeView
- The HID report descriptor file (if available)
- The Wireshark `.pcapng` capture (if available)
- Your OS and kernel/Windows version

We'll use the capture to verify VID/PID constants, add protocol support,
and move the device from "Under Investigation" to "Verified" in SOURCES.md.

---

## Privacy

USB captures may contain your device's serial number. If you want to redact it:
- In `lsusb -v` output: the serial number is on the `iSerial` line — feel free to replace it with `XXXXXXXX`
- In Wireshark captures: use the `Edit > Anonymize` feature or just note in the issue that the serial is present and we'll handle it
