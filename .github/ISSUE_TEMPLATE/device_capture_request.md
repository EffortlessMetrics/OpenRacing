---
name: Device Capture Request
about: Request USB capture / HID descriptor data for a sim racing device
title: "[Capture Request] <Device Name>"
labels: device-capture, help wanted
assignees: ''
---

## Device Information

**Manufacturer:** (e.g. Cube Controls, Turtle Beach, PXN)

**Model:** (e.g. SimSport, VelocityOne Race, VD-series)

**Approximate purchase year / firmware version:** (if known)

**Platform(s) the device is used on:** (Windows / Linux / macOS)

---

## What we need

To add plug-and-play support for this device, we need at least one of the following:

- [ ] **USB descriptor dump** — `lsusb -v -d <VID>:<PID>` on Linux, or USBTreeView export on Windows
- [ ] **HID report descriptor** — obtained from `hidapitester` or `hidraw` on Linux
- [ ] **Live USB capture** — Wireshark + USBPcap session showing steering, force feedback, and button reports

See [`docs/CONTRIBUTING_CAPTURES.md`](../../docs/CONTRIBUTING_CAPTURES.md) for step-by-step instructions.

---

## Why this device

Describe why you'd like to see this device supported:

_e.g. "It's the only entry-level direct drive under $300 and many beginners use it."_

---

## Current status

Check [`docs/DEVICE_CAPABILITIES.md`](../../docs/DEVICE_CAPABILITIES.md) to see if this device is already listed under "Devices Under Investigation":

- [ ] Not listed yet — this is a new request
- [ ] Already listed — adding capture data to the existing entry

---

## Checklist

- [ ] I have read `docs/CONTRIBUTING_CAPTURES.md`
- [ ] I have searched existing issues to avoid duplicates
- [ ] I am able to provide at least a USB descriptor dump

---

**Captures submitted as file attachments or GitHub Gists are both welcome.**
