//! Stress tests for native plugin loading.

use std::sync::Arc;

use openracing_crypto::trust_store::TrustStore;
use openracing_native_plugin::{NativePluginConfig, NativePluginHost, SpscChannel};
use tokio::task::JoinSet;

type TestResult = Result<(), Box<dyn std::error::Error>>;

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
        assert!(result.is_ok(), "Task panicked");
        if let Ok(count) = result {
            results.push(count);
        }
    }

    assert_eq!(results.len(), 100);
    assert!(results.iter().all(|&c| c == 0));
}

#[tokio::test]
async fn test_spsc_high_throughput() -> TestResult {
    let frame_size = 64;
    let channel = SpscChannel::with_capacity(frame_size, 1024)?;
    let channel = Arc::new(channel);

    let writer_channel = Arc::clone(&channel);
    let write_handle = tokio::spawn(async move {
        let writer = writer_channel.writer();
        let frame = vec![0x42u8; frame_size];
        let mut written = 0u64;

        for _ in 0..10000 {
            if let Ok(true) = writer.try_write(&frame) {
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
            if let Ok(true) = reader.try_read(&mut buffer) {
                read += 1;
            }
            tokio::task::yield_now().await;
        }
        read
    });

    let (written, read) = tokio::join!(write_handle, read_handle);

    assert!(written.is_ok(), "Write task failed");
    assert!(read.is_ok(), "Read task failed");
    assert!(written? > 0);
    assert!(read? > 0);
    Ok(())
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
fn test_spsc_ring_buffer_overflow() -> TestResult {
    let frame_size = 16;
    let capacity = 4u32;
    let channel = SpscChannel::with_capacity(frame_size, capacity)?;

    let writer = channel.writer();
    let frame = vec![0xFFu8; frame_size];

    for _ in 0..capacity {
        assert!(writer.write(&frame).is_ok());
    }

    assert!(writer.write(&frame).is_err());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_spsc_operations() -> TestResult {
    let frame_size = 32;
    let channel = Arc::new(SpscChannel::new(frame_size)?);

    let mut handles = vec![];

    for _ in 0..4 {
        let ch = Arc::clone(&channel);
        handles.push(tokio::spawn(async move {
            let writer = ch.writer();
            let frame = vec![0xABu8; frame_size];
            let mut success = 0u64;

            for _ in 0..1000 {
                if let Ok(true) = writer.try_write(&frame) {
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
                if let Ok(true) = reader.try_read(&mut buffer) {
                    success += 1;
                }
            }
            success
        }));
    }

    for handle in handles {
        assert!(handle.await.is_ok(), "Task failed");
    }

    Ok(())
}
