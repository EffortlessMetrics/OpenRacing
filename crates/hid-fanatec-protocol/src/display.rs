//! Fanatec 7-segment display and RPM LED encoders.
//!
//! # Wire protocol (verified from gotzl/hid-fanatecff hid-ftecff.c)
//!
//! Fanatec wheelbases expose a 3-digit 7-segment display and 9 RPM LEDs
//! via 7-byte HID output reports.
//!
//! ## Display command
//!
//! ```text
//! [0xf8, 0x09, 0x01, 0x02, seg0, seg1, seg2]
//! ```
//!
//! Each segment byte is a 7-segment bitmask from the `SEGBITS` table.
//! Bit 7 (0x80) controls the decimal point. Digits are right-justified
//! (if fewer than 3 characters, left positions are blank).
//!
//! ## Wheelbase LED command (DD1, DD2, CSL DD)
//!
//! ```text
//! [0xf8, 0x13, leds_lo, 0x00, 0x00, 0x00, 0x00]
//! ```
//!
//! `leds_lo` is a 9-bit bitmask (bits 0–8), one bit per LED.
//!
//! ## Wheel LED command (steering wheel RPM LEDs)
//!
//! ```text
//! [0xf8, 0x09, 0x08, leds_hi, leds_lo, 0x00, 0x00]
//! ```
//!
//! LEDs are **bit-reversed**: the first LED (leftmost) is the highest bit.
//! For 9 LEDs, bit 8 is leftmost and bit 0 is rightmost.
//!
//! ## Range command sequence
//!
//! Three reports sent in order:
//! 1. `[0xf5, 0, 0, 0, 0, 0, 0]` — reset
//! 2. `[0xf8, 0x09, 0x01, 0x06, 0x01, 0, 0]` — prepare
//! 3. `[0xf8, 0x81, range_lo, range_hi, 0, 0, 0]` — set range (LE16)

#![allow(dead_code)]

/// Number of RPM LEDs on Fanatec wheels.
pub const LED_COUNT: usize = 9;

/// Size of a Fanatec HID output report.
pub const REPORT_SIZE: usize = 7;

/// Number of display digits.
pub const DISPLAY_DIGITS: usize = 3;

// ---------------------------------------------------------------------------
// 7-segment lookup table
// ---------------------------------------------------------------------------

/// 7-segment display bitmask table.
///
/// Index 0–9: digits '0'–'9'
/// Index 10: decimal point (0x80)
/// Index 11: blank (0x00)
/// Index 12: '\[' (0x39)
/// Index 13: '\]' (0x0F)
/// Index 14: '-' (0x40)
/// Index 15: '_' (0x08)
/// Index 16–41: letters 'a'–'z' (some blank for impossible glyphs)
///
/// Source: `segbits[]` in hid-ftecff.c.
pub const SEGBITS: [u8; 42] = [
    63,  // 0
    6,   // 1
    91,  // 2
    79,  // 3
    102, // 4
    109, // 5
    125, // 6
    7,   // 7
    127, // 8
    103, // 9
    128, // dot (index 10)
    0,   // blank (index 11)
    57,  // [ (index 12)
    15,  // ] (index 13)
    64,  // - (index 14)
    8,   // _ (index 15)
    119, // a (index 16)
    124, // b
    88,  // c
    94,  // d
    121, // e
    113, // f
    61,  // g
    118, // h
    48,  // i
    14,  // j
    0,   // k (no 7-seg representation)
    56,  // l
    0,   // m (no 7-seg representation)
    84,  // n
    92,  // o
    115, // p
    103, // q
    80,  // r
    109, // s
    120, // t
    62,  // u
    0,   // v (no 7-seg representation)
    0,   // w (no 7-seg representation)
    0,   // x (no 7-seg representation)
    110, // y
    91,  // z (index 41)
];

/// Convert an ASCII character to its 7-segment bitmask.
///
/// Uppercase letters are treated as lowercase. Characters without a
/// 7-segment representation return blank (0x00).
///
/// If `point` is true, the decimal-point bit (0x80) is OR'd in.
///
/// Source: `seg_bits()` in hid-ftecff.c.
pub fn seg_bits(ch: u8, point: bool) -> u8 {
    let idx = match ch {
        // '.' or ','
        b'.' | b',' => 10,
        // '['
        b'[' => 12,
        // ']'
        b']' => 13,
        // '-'
        b'-' => 14,
        // '_'
        b'_' => 15,
        // '0'–'9'
        b'0'..=b'9' => (ch - b'0') as usize,
        // 'A'–'Z' → lowercase
        b'A'..=b'Z' => (ch - b'A' + 16) as usize,
        // 'a'–'z'
        b'a'..=b'z' => (ch - b'a' + 16) as usize,
        // anything else → blank
        _ => 11,
    };
    if point {
        SEGBITS[idx] | SEGBITS[10]
    } else {
        SEGBITS[idx]
    }
}

// ---------------------------------------------------------------------------
// Display command encoder
// ---------------------------------------------------------------------------

/// Encode a display command for up to 3 characters.
///
/// The `text` slice may contain ASCII digits, letters, '.', ',', '-', '_',
/// '\[', '\]'. Decimal points/commas after a character are merged into that
/// character's segment byte (the point bit). At most 3 digit positions are
/// used; shorter strings are right-justified (matching the kernel behavior).
///
/// Returns a 7-byte report: `[0xf8, 0x09, 0x01, 0x02, seg0, seg1, seg2]`.
///
/// Source: `ftec_set_display()` in hid-ftecff.c.
pub fn encode_display(text: &[u8]) -> [u8; REPORT_SIZE] {
    let mut buf = [0xf8, 0x09, 0x01, 0x02, 0x00, 0x00, 0x00];
    let mut seg = [0u8; 3];
    let mut seg_count = 0usize;
    let mut i = 0usize;

    while i < text.len() && seg_count < 3 {
        let ch = text[i];
        let has_point = if i + 1 < text.len() {
            text[i + 1] == b'.' || text[i + 1] == b','
        } else {
            false
        };
        seg[seg_count] = seg_bits(ch, has_point);
        seg_count += 1;
        i += if has_point { 2 } else { 1 };
    }

    // Right-justify: shift segments to the right if fewer than 3
    match seg_count {
        1 => {
            buf[6] = seg[0];
        }
        2 => {
            buf[5] = seg[0];
            buf[6] = seg[1];
        }
        3 => {
            buf[4] = seg[0];
            buf[5] = seg[1];
            buf[6] = seg[2];
        }
        _ => {} // 0 digits: all blank
    }

    buf
}

// ---------------------------------------------------------------------------
// LED command encoders
// ---------------------------------------------------------------------------

/// Encode a wheelbase LED command (DD1, DD2, CSL DD).
///
/// `leds` is a 9-bit bitmask: bit 0 = LED 1 (rightmost),
/// bit 8 = LED 9 (leftmost). Only the lower 9 bits are used.
///
/// Returns a 7-byte report: `[0xf8, 0x13, leds_lo, 0, 0, 0, 0]`.
///
/// Source: `ftec_set_leds()` with `FTEC_WHEELBASE_LEDS` quirk.
pub fn encode_wheelbase_leds(leds: u16) -> [u8; REPORT_SIZE] {
    [0xf8, 0x13, (leds & 0xff) as u8, 0x00, 0x00, 0x00, 0x00]
}

/// Encode a steering-wheel LED command.
///
/// `leds` is a 9-bit bitmask: bit 0 = LED 1, bit 8 = LED 9.
/// The encoding **reverses** the bit order: the first LED (leftmost
/// on the wheel) corresponds to the highest bit in the output.
///
/// Returns a 7-byte report: `[0xf8, 0x09, 0x08, hi, lo, 0, 0]`.
///
/// Source: `ftec_set_leds()` — wheel LED path (bit reversal loop).
pub fn encode_wheel_leds(leds: u16) -> [u8; REPORT_SIZE] {
    let mut reversed = 0u16;
    for i in 0..LED_COUNT {
        if (leds >> i) & 1 != 0 {
            reversed |= 1 << (LED_COUNT - 1 - i);
        }
    }
    [
        0xf8,
        0x09,
        0x08,
        ((reversed >> 8) & 0xff) as u8,
        (reversed & 0xff) as u8,
        0x00,
        0x00,
    ]
}

// ---------------------------------------------------------------------------
// Range command encoder
// ---------------------------------------------------------------------------

/// Encode the 3-report range command sequence.
///
/// `range` is the desired rotation range in degrees (e.g. 900).
/// The caller is responsible for clamping to the wheelbase's supported range.
///
/// Returns 3 reports that must be sent in order:
/// 1. `[0xf5, 0, 0, 0, 0, 0, 0]` — reset
/// 2. `[0xf8, 0x09, 0x01, 0x06, 0x01, 0, 0]` — prepare
/// 3. `[0xf8, 0x81, lo, hi, 0, 0, 0]` — set range (LE16)
///
/// Source: `ftec_set_range()` in hid-ftecff.c.
pub fn encode_range(range: u16) -> [[u8; REPORT_SIZE]; 3] {
    let lo = (range & 0xff) as u8;
    let hi = ((range >> 8) & 0xff) as u8;
    [
        [0xf5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        [0xf8, 0x09, 0x01, 0x06, 0x01, 0x00, 0x00],
        [0xf8, 0x81, lo, hi, 0x00, 0x00, 0x00],
    ]
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // seg_bits
    // -----------------------------------------------------------------------

    #[test]
    fn seg_bits_digit_0() {
        assert_eq!(seg_bits(b'0', false), 63);
    }

    #[test]
    fn seg_bits_digit_9() {
        assert_eq!(seg_bits(b'9', false), 103);
    }

    #[test]
    fn seg_bits_letter_a_lower() {
        assert_eq!(seg_bits(b'a', false), 119);
    }

    #[test]
    fn seg_bits_letter_a_upper() {
        assert_eq!(seg_bits(b'A', false), 119);
    }

    #[test]
    fn seg_bits_with_point() {
        let without = seg_bits(b'1', false);
        let with = seg_bits(b'1', true);
        assert_eq!(with, without | 128);
    }

    #[test]
    fn seg_bits_blank_for_unknown() {
        assert_eq!(seg_bits(b'@', false), 0);
        assert_eq!(seg_bits(b' ', false), 0);
    }

    #[test]
    fn seg_bits_hyphen() {
        assert_eq!(seg_bits(b'-', false), 64);
    }

    #[test]
    fn seg_bits_underscore() {
        assert_eq!(seg_bits(b'_', false), 8);
    }

    #[test]
    fn seg_bits_brackets() {
        assert_eq!(seg_bits(b'[', false), 57);
        assert_eq!(seg_bits(b']', false), 15);
    }

    // -----------------------------------------------------------------------
    // Display encoding
    // -----------------------------------------------------------------------

    #[test]
    fn display_three_digits() {
        let buf = encode_display(b"123");
        assert_eq!(buf[0], 0xf8);
        assert_eq!(buf[1], 0x09);
        assert_eq!(buf[2], 0x01);
        assert_eq!(buf[3], 0x02);
        assert_eq!(buf[4], seg_bits(b'1', false));
        assert_eq!(buf[5], seg_bits(b'2', false));
        assert_eq!(buf[6], seg_bits(b'3', false));
    }

    #[test]
    fn display_two_digits_right_justified() {
        let buf = encode_display(b"42");
        assert_eq!(buf[4], 0x00); // blank
        assert_eq!(buf[5], seg_bits(b'4', false));
        assert_eq!(buf[6], seg_bits(b'2', false));
    }

    #[test]
    fn display_one_digit_right_justified() {
        let buf = encode_display(b"7");
        assert_eq!(buf[4], 0x00);
        assert_eq!(buf[5], 0x00);
        assert_eq!(buf[6], seg_bits(b'7', false));
    }

    #[test]
    fn display_empty_all_blank() {
        let buf = encode_display(b"");
        assert_eq!(buf[4], 0x00);
        assert_eq!(buf[5], 0x00);
        assert_eq!(buf[6], 0x00);
    }

    #[test]
    fn display_with_decimal_point() {
        let buf = encode_display(b"1.2");
        // '1' with point, then '2'
        assert_eq!(buf[4], 0x00); // blank (right-justified: 2 positions used)
        assert_eq!(buf[5], seg_bits(b'1', true));
        assert_eq!(buf[6], seg_bits(b'2', false));
    }

    #[test]
    fn display_three_with_point() {
        let buf = encode_display(b"1.23");
        // '1' with point, '2', '3'
        assert_eq!(buf[4], seg_bits(b'1', true));
        assert_eq!(buf[5], seg_bits(b'2', false));
        assert_eq!(buf[6], seg_bits(b'3', false));
    }

    #[test]
    fn display_header_bytes() {
        let buf = encode_display(b"abc");
        assert_eq!(&buf[..4], &[0xf8, 0x09, 0x01, 0x02]);
    }

    // -----------------------------------------------------------------------
    // LED encoding — wheelbase
    // -----------------------------------------------------------------------

    #[test]
    fn wheelbase_leds_all_off() {
        let buf = encode_wheelbase_leds(0);
        assert_eq!(buf, [0xf8, 0x13, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn wheelbase_leds_all_on() {
        let buf = encode_wheelbase_leds(0x1FF); // 9 bits
        assert_eq!(buf[2], 0xFF); // lower 8 bits
        // bit 8 is truncated to leds_lo byte (only byte 2)
    }

    #[test]
    fn wheelbase_leds_single() {
        let buf = encode_wheelbase_leds(1);
        assert_eq!(buf[2], 0x01);
    }

    // -----------------------------------------------------------------------
    // LED encoding — wheel (bit-reversed)
    // -----------------------------------------------------------------------

    #[test]
    fn wheel_leds_all_off() {
        let buf = encode_wheel_leds(0);
        assert_eq!(buf, [0xf8, 0x09, 0x08, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn wheel_leds_bit0_becomes_bit8() {
        // Input: bit 0 set → output: bit 8 (LED_COUNT-1) set
        let buf = encode_wheel_leds(1);
        let reversed = ((buf[3] as u16) << 8) | buf[4] as u16;
        assert_eq!(reversed, 1 << 8); // bit 8
    }

    #[test]
    fn wheel_leds_bit8_becomes_bit0() {
        // Input: bit 8 set → output: bit 0 set
        let buf = encode_wheel_leds(1 << 8);
        let reversed = ((buf[3] as u16) << 8) | buf[4] as u16;
        assert_eq!(reversed, 1); // bit 0
    }

    #[test]
    fn wheel_leds_all_on_stays_all_on() {
        let buf = encode_wheel_leds(0x1FF);
        let reversed = ((buf[3] as u16) << 8) | buf[4] as u16;
        assert_eq!(reversed, 0x1FF);
    }

    #[test]
    fn wheel_leds_header() {
        let buf = encode_wheel_leds(0);
        assert_eq!(&buf[..3], &[0xf8, 0x09, 0x08]);
    }

    // -----------------------------------------------------------------------
    // Range encoding
    // -----------------------------------------------------------------------

    #[test]
    fn range_900_degrees() {
        let cmds = encode_range(900);
        assert_eq!(cmds[0], [0xf5, 0, 0, 0, 0, 0, 0]);
        assert_eq!(cmds[1], [0xf8, 0x09, 0x01, 0x06, 0x01, 0, 0]);
        assert_eq!(cmds[2][0], 0xf8);
        assert_eq!(cmds[2][1], 0x81);
        let range = u16::from_le_bytes([cmds[2][2], cmds[2][3]]);
        assert_eq!(range, 900);
    }

    #[test]
    fn range_1080_degrees() {
        let cmds = encode_range(1080);
        let range = u16::from_le_bytes([cmds[2][2], cmds[2][3]]);
        assert_eq!(range, 1080);
    }

    #[test]
    fn range_roundtrip() {
        for deg in [90, 180, 360, 540, 900, 1080] {
            let cmds = encode_range(deg);
            let got = u16::from_le_bytes([cmds[2][2], cmds[2][3]]);
            assert_eq!(got, deg, "range roundtrip failed for {deg}");
        }
    }

    // -----------------------------------------------------------------------
    // Property: seg_bits is consistent with SEGBITS table
    // -----------------------------------------------------------------------

    #[test]
    fn all_digits_match_segbits_table() {
        for d in b'0'..=b'9' {
            assert_eq!(
                seg_bits(d, false),
                SEGBITS[(d - b'0') as usize],
                "digit {} mismatch",
                d as char
            );
        }
    }

    #[test]
    fn all_lowercase_letters_match_segbits_table() {
        for c in b'a'..=b'z' {
            assert_eq!(
                seg_bits(c, false),
                SEGBITS[(c - b'a' + 16) as usize],
                "letter {} mismatch",
                c as char
            );
        }
    }

    #[test]
    fn uppercase_equals_lowercase() {
        for c in b'A'..=b'Z' {
            let lower = c + 32; // ASCII lowercase
            assert_eq!(
                seg_bits(c, false),
                seg_bits(lower, false),
                "case mismatch for {}",
                c as char
            );
        }
    }

    #[test]
    fn point_bit_is_0x80() {
        for d in b'0'..=b'9' {
            let with = seg_bits(d, true);
            let without = seg_bits(d, false);
            assert_eq!(with & 0x80, 0x80, "point bit not set for {}", d as char);
            assert_eq!(
                with & 0x7f,
                without & 0x7f,
                "lower bits differ for {}",
                d as char
            );
        }
    }

    // -----------------------------------------------------------------------
    // LED bit reversal property
    // -----------------------------------------------------------------------

    #[test]
    fn wheel_led_reversal_is_self_inverse() {
        // Reversing twice should yield the original
        for input in 0..=0x1FFu16 {
            let buf1 = encode_wheel_leds(input);
            let reversed = ((buf1[3] as u16) << 8) | buf1[4] as u16;
            let buf2 = encode_wheel_leds(reversed);
            let double_reversed = ((buf2[3] as u16) << 8) | buf2[4] as u16;
            assert_eq!(
                double_reversed, input,
                "double reversal failed for 0x{input:03x}"
            );
        }
    }
}
