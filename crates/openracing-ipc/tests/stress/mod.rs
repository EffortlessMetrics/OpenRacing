//! Stress tests for concurrent connections

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinSet;

use openracing_ipc::prelude::*;

#[tokio::test]
async fn concurrent_client_negotiations() -> IpcResult<()> {
    let config = IpcConfig::default().max_connections(1000);
    let server = Arc::new(IpcServer::new(config));
    server.start().await?;

    let num_clients = 100;
    let mut tasks = JoinSet::new();

    for i in 0..num_clients {
        let server = server.clone();
        tasks.spawn(async move {
            let features = vec!["device_management".to_string()];
            server.negotiate_features("1.0.0", &features).await
        });
    }

    let mut successes = 0;
    while let Some(result) = tasks.join_next().await {
        if result.expect("task should complete").is_ok() {
            successes += 1;
        }
    }

    assert_eq!(successes, num_clients);
    assert_eq!(server.client_count().await, num_clients);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_health_broadcasts() {
    let config = IpcConfig::default().health_buffer_size(10000);
    let server = Arc::new(IpcServer::new(config));

    let num_events = 1000;
    let mut tasks = JoinSet::new();

    for i in 0..num_events {
        let server = server.clone();
        tasks.spawn(async move {
            let event = HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: format!("device-{}", i),
                event_type: HealthEventType::Connected,
                message: format!("Event {}", i),
                metadata: std::collections::HashMap::new(),
            };
            server.broadcast_health_event(event);
        });
    }

    while let Some(_) = tasks.join_next().await {}
}

#[tokio::test]
async fn rapid_start_stop_cycles() -> IpcResult<()> {
    let config = IpcConfig::default();
    let server = IpcServer::new(config);

    for _ in 0..10 {
        server.start().await?;
        server.stop().await?;
    }

    Ok(())
}

#[tokio::test]
async fn client_registration_stress() {
    let config = IpcConfig::default();
    let server = Arc::new(IpcServer::new(config));

    let num_clients = 500;
    let mut tasks = JoinSet::new();

    for i in 0..num_clients {
        let server = server.clone();
        tasks.spawn(async move {
            let client = ClientInfo {
                id: format!("client-{}", i),
                connected_at: std::time::Instant::now(),
                version: "1.0.0".to_string(),
                features: vec![],
                peer_info: PeerInfo::default(),
            };
            server.register_client(client).await;
        });
    }

    while let Some(_) = tasks.join_next().await {}

    assert_eq!(server.client_count().await, num_clients);
}

#[tokio::test]
async fn concurrent_register_unregister() {
    let config = IpcConfig::default();
    let server = Arc::new(IpcServer::new(config));

    let num_operations = 200;
    let mut tasks = JoinSet::new();

    for i in 0..num_operations {
        let server = server.clone();
        let operation = i % 2;
        let client_id = format!("client-{}", i / 2);

        tasks.spawn(async move {
            if operation == 0 {
                let client = ClientInfo {
                    id: client_id,
                    connected_at: std::time::Instant::now(),
                    version: "1.0.0".to_string(),
                    features: vec![],
                    peer_info: PeerInfo::default(),
                };
                server.register_client(client).await;
            } else {
                server.unregister_client(&client_id).await;
            }
        });
    }

    while let Some(_) = tasks.join_next().await {}
}

#[tokio::test]
async fn health_event_throughput() {
    let config = IpcConfig::default().health_buffer_size(50000);
    let server = Arc::new(IpcServer::new(config));

    let num_events = 10000;
    let start = std::time::Instant::now();

    for i in 0..num_events {
        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: format!("device-{}", i),
            event_type: HealthEventType::Connected,
            message: format!("Event {}", i),
            metadata: std::collections::HashMap::new(),
        };
        server.broadcast_health_event(event);
    }

    let elapsed = start.elapsed();
    let events_per_sec = num_events as f64 / elapsed.as_secs_f64();

    println!(
        "Broadcast {} events in {:?} ({:.0} events/sec)",
        num_events, elapsed, events_per_sec
    );

    assert!(events_per_sec > 10000.0, "Should handle > 10k events/sec");
}

#[tokio::test]
async fn message_header_throughput() {
    use openracing_ipc::codec::MessageHeader;

    let num_headers = 100000;
    let start = std::time::Instant::now();

    for i in 0..num_headers {
        let header = MessageHeader::new(message_types::DEVICE, 100, i as u32);
        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded).expect("decode");
        std::hint::black_box(decoded);
    }

    let elapsed = start.elapsed();
    let headers_per_sec = num_headers as f64 / elapsed.as_secs_f64();

    println!(
        "Processed {} headers in {:?} ({:.0} headers/sec)",
        num_headers, elapsed, headers_per_sec
    );

    assert!(
        headers_per_sec > 100000.0,
        "Should handle > 100k headers/sec"
    );
}
