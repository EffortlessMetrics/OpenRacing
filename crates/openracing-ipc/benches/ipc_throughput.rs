//! IPC Throughput Benchmarks

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use openracing_ipc::codec::message_types;
use openracing_ipc::codec::{MessageCodec, MessageHeader};
use openracing_ipc::server::{IpcConfig, IpcServer, is_version_compatible};

fn bench_message_header_encode(c: &mut Criterion) {
    let header = MessageHeader::new(message_types::DEVICE, 1024, 42);

    c.bench_function("message_header_encode", |b| {
        b.iter(|| {
            black_box(header.encode());
        })
    });
}

fn bench_message_header_decode(c: &mut Criterion) {
    let header = MessageHeader::new(message_types::DEVICE, 1024, 42);
    let encoded = header.encode();

    c.bench_function("message_header_decode", |b| {
        b.iter(|| {
            black_box(MessageHeader::decode(&encoded).expect("decode"));
        })
    });
}

fn bench_message_header_roundtrip(c: &mut Criterion) {
    c.bench_function("message_header_roundtrip", |b| {
        b.iter(|| {
            let header = MessageHeader::new(message_types::DEVICE, 1024, 42);
            let encoded = header.encode();
            let decoded = MessageHeader::decode(&encoded).expect("decode");
            black_box(decoded);
        })
    });
}

fn bench_version_compatibility(c: &mut Criterion) {
    c.bench_function("version_compatibility", |b| {
        b.iter(|| {
            black_box(is_version_compatible("1.0.0", "1.0.0"));
        })
    });
}

fn bench_codec_validation(c: &mut Criterion) {
    let codec = MessageCodec::new();

    c.bench_function("codec_size_validation", |b| {
        b.iter(|| {
            black_box(codec.is_valid_size(1024));
        })
    });
}

fn bench_health_event_creation(c: &mut Criterion) {
    use openracing_ipc::server::{HealthEvent, HealthEventType};
    use std::collections::HashMap;

    c.bench_function("health_event_creation", |b| {
        b.iter(|| {
            let event = HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: "test-device".to_string(),
                event_type: HealthEventType::Connected,
                message: "Device connected".to_string(),
                metadata: HashMap::new(),
            };
            black_box(event);
        })
    });
}

fn bench_server_creation(c: &mut Criterion) {
    c.bench_function("server_creation", |b| {
        b.iter(|| {
            let config = IpcConfig::default();
            black_box(IpcServer::new(config));
        })
    });
}

fn bench_multiple_header_encodes(c: &mut Criterion) {
    c.bench_function("100_header_encodes", |b| {
        b.iter(|| {
            for i in 0..100u32 {
                let header = MessageHeader::new(message_types::DEVICE, 100, i);
                black_box(header.encode());
            }
        })
    });
}

fn bench_multiple_header_decodes(c: &mut Criterion) {
    let headers: Vec<[u8; 12]> = (0..100)
        .map(|i| {
            let header = MessageHeader::new(message_types::DEVICE, 100, i as u32);
            header.encode()
        })
        .collect();

    c.bench_function("100_header_decodes", |b| {
        b.iter(|| {
            for encoded in &headers {
                black_box(MessageHeader::decode(encoded).expect("decode"));
            }
        })
    });
}

fn bench_flag_operations(c: &mut Criterion) {
    use openracing_ipc::codec::message_flags;

    c.bench_function("flag_set_and_check", |b| {
        b.iter(|| {
            let mut header = MessageHeader::new(message_types::DEVICE, 100, 0);
            header.set_flag(message_flags::COMPRESSED);
            header.set_flag(message_flags::REQUIRES_ACK);
            black_box(header.has_flag(message_flags::COMPRESSED));
            black_box(header.has_flag(message_flags::REQUIRES_ACK));
        })
    });
}

criterion_group!(
    benches,
    bench_message_header_encode,
    bench_message_header_decode,
    bench_message_header_roundtrip,
    bench_version_compatibility,
    bench_codec_validation,
    bench_health_event_creation,
    bench_server_creation,
    bench_multiple_header_encodes,
    bench_multiple_header_decodes,
    bench_flag_operations,
);

criterion_main!(benches);
