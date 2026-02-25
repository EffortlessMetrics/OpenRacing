//! Property tests for IPC message serialization

use proptest::prelude::*;

use openracing_ipc::codec::{MessageCodec, MessageHeader};

proptest! {
    #[test]
    fn prop_message_header_roundtrip(
        message_type in 0u16..=u16::MAX,
        payload_len in 0u32..=1_000_000u32,
        sequence in 0u32..=u32::MAX,
        flags in 0u16..=u16::MAX
    ) {
        let mut header = MessageHeader::new(message_type, payload_len, sequence);
        header.flags = flags;

        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded).expect("decode should succeed");

        prop_assert_eq!(decoded.message_type, message_type);
        prop_assert_eq!(decoded.payload_len, payload_len);
        prop_assert_eq!(decoded.sequence, sequence);
        prop_assert_eq!(decoded.flags, flags);
    }

    #[test]
    fn prop_message_size_validation(
        max_size in 100usize..=10_000usize,
        test_size in 0usize..=20_000usize
    ) {
        let codec = MessageCodec::with_max_size(max_size);

        let expected_valid = test_size > 0 && test_size <= max_size;
        let actual_valid = codec.is_valid_size(test_size);

        prop_assert_eq!(actual_valid, expected_valid);
    }

    #[test]
    fn prop_version_compatibility_major_match(
        minor_a in 0u32..=100u32,
        patch_a in 0u32..=100u32,
        minor_b in 0u32..=100u32,
        patch_b in 0u32..=100u32
    ) {
        use openracing_ipc::server::is_version_compatible;

        let client = format!("1.{}.{}", minor_a, patch_a);
        let min = format!("1.{}.{}", minor_b, patch_b);

        let result = is_version_compatible(&client, &min);

        let expected = if minor_a > minor_b {
            true
        } else if minor_a == minor_b {
            patch_a >= patch_b
        } else {
            false
        };

        prop_assert_eq!(result, expected);
    }

    #[test]
    fn prop_message_type_flags_combination(
        msg_type in any::<u16>(),
        flag_bits in any::<u16>()
    ) {
        let mut header = MessageHeader::new(msg_type, 100, 0);

        header.flags = 0;
        for bit in 0..16 {
            if (flag_bits & (1 << bit)) != 0 {
                header.set_flag(1 << bit);
            }
        }

        prop_assert_eq!(header.flags, flag_bits);

        for bit in 0..16 {
            let has_flag = header.has_flag(1 << bit);
            let expected = (flag_bits & (1 << bit)) != 0;
            prop_assert_eq!(has_flag, expected);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proptest_framework_works() {
        proptest!(|(a in 0..100)| {
            prop_assert!(a < 100);
        });
    }
}
