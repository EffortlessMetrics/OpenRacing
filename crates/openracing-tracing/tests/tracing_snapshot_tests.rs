//! Snapshot tests for tracing output format stability.
//!
//! These tests capture the exact formatted output of tracing events and spans
//! to detect unintentional format regressions.

use std::io;
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::MakeWriter;

/// Writer that captures tracing output to a shared buffer.
#[derive(Clone)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl CaptureWriter {
    fn new() -> (Self, Arc<Mutex<Vec<u8>>>) {
        let buf = Arc::new(Mutex::new(Vec::new()));
        (Self(buf.clone()), buf)
    }
}

impl io::Write for CaptureWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if let Ok(mut inner) = self.0.lock() {
            inner.extend_from_slice(data);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CaptureWriter {
    type Writer = CaptureWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn read_captured(buf: &Arc<Mutex<Vec<u8>>>) -> String {
    buf.lock()
        .map(|g| String::from_utf8_lossy(&g).into_owned())
        .unwrap_or_default()
}

#[test]
fn snapshot_default_span_format() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .without_time()
        .with_ansi(false)
        .with_target(false)
        .with_level(true)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("rt_pipeline", tick = 42u64, stage = "filter");
        let _guard = span.enter();
        tracing::info!("processing tick");
    });

    let out = read_captured(&buf);
    insta::assert_snapshot!("default_span_format", out.trim());
}

#[test]
fn snapshot_error_event_format() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .without_time()
        .with_ansi(false)
        .with_target(false)
        .with_level(true)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::error!(
            error_code = 5u8,
            tick_count = 42u64,
            "pipeline fault detected"
        );
    });

    let out = read_captured(&buf);
    insta::assert_snapshot!("error_event_format", out.trim());
}

#[test]
fn snapshot_warning_event_format() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .without_time()
        .with_ansi(false)
        .with_target(false)
        .with_level(true)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::warn!(
            tick_count = 42u64,
            jitter_ns = 250_000u64,
            "deadline miss detected"
        );
    });

    let out = read_captured(&buf);
    insta::assert_snapshot!("warning_event_format", out.trim());
}

#[test]
fn snapshot_structured_event_all_field_types() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .without_time()
        .with_ansi(false)
        .with_target(false)
        .with_level(true)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(
            u64_field = 42u64,
            f64_field = 2.72f64,
            bool_field = true,
            str_field = "hello",
            i32_field = -10i32,
            "all field types"
        );
    });

    let out = read_captured(&buf);
    insta::assert_snapshot!("structured_event_all_fields", out.trim());
}
