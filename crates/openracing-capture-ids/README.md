# openracing-capture-ids

HID device enumeration, capture, replay, and inspection tool for OpenRacing.

## Usage

### Enumerate devices

List all HID devices matching a vendor ID (defaults to MOZA `0x346E`):

```sh
openracing-capture-ids
openracing-capture-ids --vid 0x046D          # Logitech
openracing-capture-ids --vid 0x346E --descriptor-hex   # include descriptor bytes
```

Output is a single JSON object printed to stdout.

### Record HID traffic

Capture all incoming HID input reports from a device to a **JSON Lines** file:

```sh
openracing-capture-ids --record capture.jsonl --vid 0x046D --pid 0xC262
openracing-capture-ids --record capture.jsonl --vid 0x346E --pid 0x0010 --duration-secs 60
```

Each line of the output file has the format:

```json
{"ts_ns":1720000000000000000,"vid":"0x046D","pid":"0xC262","report":"0180003f..."}
```

| Field | Description |
|-------|-------------|
| `ts_ns` | Unix timestamp in nanoseconds |
| `vid` | Vendor ID (hex, 4 digits) |
| `pid` | Product ID (hex, 4 digits) |
| `report` | Report bytes as lowercase hex |

Recording stops automatically after `--duration-secs` seconds (default: 30) or on **Ctrl-C**.

### Replay a capture

Play back a recorded file at the original timing (or at a custom speed):

```sh
openracing-capture-ids --replay capture.jsonl
openracing-capture-ids --replay capture.jsonl --speed 2.0   # double speed
openracing-capture-ids --replay capture.jsonl --speed 0.0   # no delay, dump all
```

Each report is printed with its timestamp offset, report ID, hex dump, ASCII
representation, and decoded fields when the vendor is known.

### Inspect live reports

Continuously read and print input reports from a connected device:

```sh
openracing-capture-ids --inspect --vid 0x046D --pid 0xC262
openracing-capture-ids --inspect --vid 0x346E --pid 0x0010 --duration-secs 120
```

Output shows the inter-report delta in microseconds, the report ID, hex dump,
printable ASCII, and decoded field values for known vendors.

## Vendor decoding

When the VID matches a known vendor, decoded field values are printed alongside
the raw hex:

| VID | Vendor | Decoded fields |
|-----|--------|----------------|
| `0x346E` | MOZA Racing | steering, throttle, brake (normalised 0–1) |
| `0x046D` | Logitech | steering (±1), throttle, brake, clutch, buttons |
