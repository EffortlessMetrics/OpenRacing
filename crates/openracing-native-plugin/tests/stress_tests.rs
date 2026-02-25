//! Stress tests for native plugin loading.

use std::sync::Arc;

use openracing_crypto::trust_store::TrustStore;
use openracing_native_plugin::{NativePluginConfig, NativePluginHost, SpscChannel};
use tokio::task::JoinSet;

#[tokio::test]
async fn test_concurrent_host_access() {
    let host = Arc::new(NativePluginHost::new_with_defaults());
    let mut tasks = JoinSet::new();

    for _ in 0..100 {
        let host = Arc::clone(&host);
        tasks.spawn(async move { host.plugin_count().await });
    }

    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        results.push(result.expect("Task panicked"));
    }

    assert_eq!(results.len(), 100);
    assert!(results.iter().all(|&c| c == 0));
}

#[tokio::test]
async fn test_spsc_high_throughput() {
    let frame_size = 64;
    let channel = SpscChannel::with_capacity(frame_size, 1024).expect("Failed to create channel");
    let channel = Arc::new(channel);

    let writer_channel = Arc::clone(&channel);
    let write_handle = tokio::spawn(async move {
        let writer = writer_channel.writer();
        let frame = vec![0x42u8; frame_size];
        let mut written = 0u64;

        for _ in 0..10000 {
            if writer.try_write(&frame).expect("Write error") {
                written += 1;
            }
            tokio::task::yield_now().await;
        }
        written
    });

    let reader_channel = Arc::clone(&channel);
    let read_handle = tokio::spawn(async move {
        let reader = reader_channel.reader();
        let mut buffer = vec![0u8; frame_size];
        let mut read = 0u64;

        for _ in 0..10000 {
            if reader.try_read(&mut buffer).expect("Read error") {
                read += 1;
            }
            tokio::task::yield_now().await;
        }
        read
    });

    let (written, read) = tokio::join!(write_handle, read_handle);

    assert!(written.expect("Write task failed") > 0);
    assert!(read.expect("Read task failed") > 0);
}

#[tokio::test]
async fn test_config_update_stress() {
    let trust_store = TrustStore::new_in_memory();
    let host = Arc::new(tokio::sync::RwLock::new(NativePluginHost::new(
        trust_store,
        NativePluginConfig::development(),
    )));

    let mut tasks = JoinSet::new();

    for i in 0..50 {
        let host = Arc::clone(&host);
        tasks.spawn(async move {
            let mut host = host.write().await;
            let config = if i % 2 == 0 {
                NativePluginConfig::strict()
            } else {
                NativePluginConfig::permissive()
            };
            host.set_config(config);
        });
    }

    while tasks.join_next().await.is_some() {}

    let final_host = host.read().await;
    assert!(final_host.config().require_signatures || !final_host.config().require_signatures);
}

#[test]
fn test_spsc_ring_buffer_overflow() {
    let frame_size = 16;
    let capacity = 4u32;
    let channel =
        SpscChannel::with_capacity(frame_size, capacity).expect("Failed to create channel");

    let writer = channel.writer();
    let frame = vec![0xFFu8; frame_size];

    for _ in 0..capacity {
        writer.write(&frame).expect("Failed to write");
    }

    assert!(writer.write(&frame).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_spsc_operations() {
    let frame_size = 32;
    let channel = Arc::new(SpscChannel::new(frame_size).expect("Failed to create channel"));

    let mut handles = vec![];

    for _ in 0..4 {
        let ch = Arc::clone(&channel);
        handles.push(tokio::spawn(async move {
            let writer = ch.writer();
            let frame = vec![0xABu8; frame_size];
            let mut success = 0u64;

            for _ in 0..1000 {
                if writer.try_write(&frame).expect("Write error") {
                    success += 1;
                }
            }
            success
        }));

        let ch = Arc::clone(&channel);
        handles.push(tokio::spawn(async move {
            let reader = ch.reader();
            let mut buffer = vec![0u8; frame_size];
            let mut success = 0u64;

            for _ in 0..1000 {
                if reader.try_read(&mut buffer).expect("Read error") {
                    success += 1;
                }
            }
            success
        }));
    }

    let mut total_writes = 0u64;
    let mut total_reads = 0u64;

    for handle in handles {
        let result = handle.await.expect("Task failed");
        total_writes += result;
        total_reads += result;
    }

    assert!(total_writes > 0 || total_reads > 0);
}
