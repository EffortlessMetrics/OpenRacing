//! iRacing telemetry adapter with Windows shared-memory access.
//!
//! This adapter opens `Local\\IRSDKMemMapFileName` with `FILE_MAP_READ`,
//! reads the IRSDK header, and selects the newest rotating telemetry buffer.
#![cfg_attr(not(windows), allow(unused, dead_code))]

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    TelemetryValue, telemetry_now_ns,
};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use std::mem;
use std::ptr;
use std::time::Duration;
use tokio::sync::mpsc;
#[cfg(windows)]
use tokio::task;
use tracing::{debug, info, warn};

#[cfg(windows)]
use winapi::um::{
    handleapi::CloseHandle,
    memoryapi::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile},
    synchapi::{OpenEventW, WaitForSingleObject},
    winnt::{HANDLE, SYNCHRONIZE},
};

#[cfg(windows)]
const IRACING_MAP_NAME: &str = "Local\\IRSDKMemMapFileName";
#[cfg(windows)]
const IRACING_DATA_VALID_EVENT_NAME: &str = "Local\\IRSDKDataValidEvent";
const IRSDK_MAX_BUFS: usize = 4;
const IRSDK_DEFAULT_TICK_RATE: Duration = Duration::from_millis(16);
const IRSDK_SESSION_FLAG_CHECKERED: u32 = 0x0000_0001;
const IRSDK_SESSION_FLAG_GREEN: u32 = 0x0000_0004;
const IRSDK_SESSION_FLAG_YELLOW: u32 = 0x0000_0008;
const IRSDK_SESSION_FLAG_RED: u32 = 0x0000_0010;
const IRSDK_SESSION_FLAG_BLUE: u32 = 0x0000_0020;
const IRSDK_DEFAULT_TIRE_RADIUS_M: f32 = 0.33;
const IRSDK_MIN_TIRE_SURFACE_SPEED_MPS: f32 = 0.05;
const IRSDK_STABLE_READ_ATTEMPTS: usize = 3;
const IRSDK_MAX_VARS: i32 = 4096;
const IRSDK_VAR_NAME_LEN: usize = 32;
#[cfg(windows)]
const WAIT_OBJECT_0: u32 = 0;
#[cfg(windows)]
const WAIT_TIMEOUT: u32 = 0x00000102;
#[cfg(windows)]
const WAIT_FAILED: u32 = u32::MAX;

const IRSDK_VAR_TYPE_CHAR: i32 = 0;
const IRSDK_VAR_TYPE_BOOL: i32 = 1;
const IRSDK_VAR_TYPE_INT: i32 = 2;
const IRSDK_VAR_TYPE_BITFIELD: i32 = 3;
const IRSDK_VAR_TYPE_FLOAT: i32 = 4;
const IRSDK_VAR_TYPE_DOUBLE: i32 = 5;

/// iRacing telemetry adapter using shared memory.
pub struct IRacingAdapter {
    update_rate: Duration,
    #[cfg(windows)]
    shared_memory: Option<SharedMemoryHandle>,
}

#[cfg(windows)]
struct SharedMemoryHandle {
    handle: HANDLE,
    base_ptr: *const u8,
    layout: IRacingLayout,
    tick_interval: Duration,
    data_valid_event: Option<HANDLE>,
}

#[cfg(windows)]
unsafe impl Send for SharedMemoryHandle {}
#[cfg(windows)]
unsafe impl Sync for SharedMemoryHandle {}

/// IRSDK rotating buffer descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct IRSDKVarBuf {
    tick_count: i32,
    buf_offset: i32,
    pad: [i32; 2],
}

/// IRSDK memory-map header.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct IRSDKHeader {
    ver: i32,
    status: i32,
    tick_rate: i32,
    session_info_update: i32,
    session_info_len: i32,
    session_info_offset: i32,
    num_vars: i32,
    var_header_offset: i32,
    num_buf: i32,
    buf_len: i32,
    pad: [i32; 2],
    var_buf: [IRSDKVarBuf; IRSDK_MAX_BUFS],
}

/// IRSDK variable header (name/type/offset metadata).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct IRSDKVarHeader {
    var_type: i32,
    offset: i32,
    count: i32,
    count_as_time: u8,
    pad: [u8; 3],
    name: [u8; IRSDK_VAR_NAME_LEN],
    desc: [u8; 64],
    unit: [u8; 32],
}

impl Default for IRSDKVarHeader {
    fn default() -> Self {
        Self {
            var_type: 0,
            offset: 0,
            count: 0,
            count_as_time: 0,
            pad: [0; 3],
            name: [0; IRSDK_VAR_NAME_LEN],
            desc: [0; 64],
            unit: [0; 32],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct VarBinding {
    var_type: i32,
    offset: usize,
    count: usize,
    _unit: [u8; 32],
}

#[derive(Debug, Clone, Copy, Default)]
struct IRacingLayout {
    session_time: Option<VarBinding>,
    session_flags: Option<VarBinding>,
    speed: Option<VarBinding>,
    rpm: Option<VarBinding>,
    gear: Option<VarBinding>,
    throttle: Option<VarBinding>,
    brake: Option<VarBinding>,
    steering_wheel_angle: Option<VarBinding>,
    steering_wheel_torque: Option<VarBinding>,
    steering_wheel_pct_torque_sign: Option<VarBinding>,
    steering_wheel_max_force_nm: Option<VarBinding>,
    steering_wheel_limiter: Option<VarBinding>,
    lf_tire_speed: Option<VarBinding>,
    rf_tire_speed: Option<VarBinding>,
    lr_tire_speed: Option<VarBinding>,
    rr_tire_speed: Option<VarBinding>,
    lf_tire_slip_ratio: Option<VarBinding>,
    rf_tire_slip_ratio: Option<VarBinding>,
    lr_tire_slip_ratio: Option<VarBinding>,
    rr_tire_slip_ratio: Option<VarBinding>,
    lap_current: Option<VarBinding>,
    lap_best_time: Option<VarBinding>,
    fuel_level: Option<VarBinding>,
    on_pit_road: Option<VarBinding>,
    car_path: Option<VarBinding>,
    track_name: Option<VarBinding>,
}

#[derive(Debug, Clone, Copy)]
struct IRacingSample {
    data: IRacingData,
    header: IRSDKHeader,
    tick_count: i32,
    session_info_update: i32,
    session_info_offset: i32,
    session_info_len: i32,
    tick_interval: Duration,
}

impl Default for IRacingAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl IRacingAdapter {
    /// Create a new iRacing adapter.
    pub fn new() -> Self {
        Self {
            update_rate: Duration::from_millis(16),
            #[cfg(windows)]
            shared_memory: None,
        }
    }

    /// Initialize shared memory connection to iRacing.
    #[cfg(windows)]
    fn initialize_shared_memory(&mut self) -> Result<()> {
        let wide_name = to_wide_null_terminated(IRACING_MAP_NAME);

        // SAFETY: Win32 calls with a valid, null-terminated UTF-16 name.
        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide_name.as_ptr());
            if handle.is_null() {
                return Err(anyhow!("Failed to open iRacing shared memory mapping"));
            }

            let base_ptr = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0) as *const u8;
            if base_ptr.is_null() {
                CloseHandle(handle);
                return Err(anyhow!("Failed to map iRacing shared memory view"));
            }

            // Validate header shape once at startup.
            let header = read_irsdk_header_from_ptr(base_ptr);
            validate_irsdk_header(&header)?;
            let layout = build_iracing_layout(base_ptr, &header)?;
            let data_valid_event = open_irsdk_data_valid_event();
            if data_valid_event.is_none() {
                debug!("IRSDK data-valid event unavailable; using tick pacing fallback");
            }

            self.shared_memory = Some(SharedMemoryHandle {
                handle,
                base_ptr,
                layout,
                tick_interval: calculate_tick_interval(header.tick_rate),
                data_valid_event,
            });
            info!("Connected to iRacing shared memory");
            Ok(())
        }
    }

    #[cfg(not(windows))]
    fn initialize_shared_memory(&mut self) -> Result<()> {
        Err(anyhow!(
            "iRacing shared memory is only available on Windows"
        ))
    }

    /// Read telemetry data from the newest stable IRSDK rotating buffer.
    #[cfg(windows)]
    fn read_telemetry_data(&self) -> Result<IRacingSample> {
        let shared_memory = self
            .shared_memory
            .as_ref()
            .ok_or_else(|| anyhow!("Shared memory not initialized"))?;

        for _ in 0..IRSDK_STABLE_READ_ATTEMPTS {
            let header_before = read_irsdk_header_from_ptr(shared_memory.base_ptr);
            validate_irsdk_header(&header_before)?;

            let (_, latest_buf_before) = select_latest_var_buffer(&header_before)
                .ok_or_else(|| anyhow!("IRSDK header does not expose any telemetry buffers"))?;

            let data = read_iracing_data_from_ptr(
                shared_memory.base_ptr,
                latest_buf_before,
                &shared_memory.layout,
            )?;
            let header_after = read_irsdk_header_from_ptr(shared_memory.base_ptr);
            validate_irsdk_header(&header_after)?;

            if let Some((_, latest_buf_after)) = select_latest_var_buffer(&header_after)
                && latest_buf_after.tick_count == latest_buf_before.tick_count
                && latest_buf_after.buf_offset == latest_buf_before.buf_offset
            {
                return Ok(IRacingSample {
                    data,
                    header: header_after,
                    tick_count: latest_buf_after.tick_count,
                    session_info_update: header_after.session_info_update,
                    session_info_offset: header_after.session_info_offset,
                    session_info_len: header_after.session_info_len,
                    tick_interval: calculate_tick_interval(header_after.tick_rate),
                });
            }
        }

        Err(anyhow!(
            "Failed to obtain stable iRacing telemetry snapshot after {} attempts",
            IRSDK_STABLE_READ_ATTEMPTS
        ))
    }

    #[cfg(not(windows))]
    fn read_telemetry_data(&self) -> Result<IRacingSample> {
        Err(anyhow!(
            "iRacing shared memory is only available on Windows"
        ))
    }

    /// Check if iRacing is running by attempting to open shared memory.
    #[cfg(windows)]
    async fn check_iracing_running(&self) -> bool {
        let wide_name = to_wide_null_terminated(IRACING_MAP_NAME);

        // SAFETY: Win32 call with a valid mapping name.
        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide_name.as_ptr());
            if handle.is_null() {
                false
            } else {
                CloseHandle(handle);
                true
            }
        }
    }

    #[cfg(not(windows))]
    async fn check_iracing_running(&self) -> bool {
        false
    }
}

#[async_trait]
impl TelemetryAdapter for IRacingAdapter {
    fn game_id(&self) -> &str {
        "iracing"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            let mut adapter = IRacingAdapter::new();
            let mut sequence = 0u64;
            let mut last_tick_count: Option<i32> = None;
            let mut last_session_info_update: Option<i32> = None;
            let mut last_layout_signature: Option<(i32, i32, i32, i32)> = None;
            let mut warned_unscaled_ffb = false;
            let mut tick_interval = update_rate;

            #[cfg(windows)]
            loop {
                if adapter.shared_memory.is_none() {
                    if let Err(err) = adapter.initialize_shared_memory() {
                        warn!("Waiting for iRacing shared memory: {}", err);
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        continue;
                    }
                    if let Some(handle) = adapter.shared_memory.as_ref() {
                        tick_interval = handle.tick_interval;
                        adapter.update_rate = handle.tick_interval;
                    }
                    warned_unscaled_ffb = false;
                    last_tick_count = None;
                    last_layout_signature = None;
                    last_session_info_update = None;
                    info!("Connected to iRacing shared memory");
                }

                let use_data_valid_event = adapter
                    .shared_memory
                    .as_ref()
                    .and_then(|handle| handle.data_valid_event)
                    .is_some();
                if let Some(handle) = adapter.shared_memory.as_ref() {
                    if let Some(event_handle) = handle.data_valid_event {
                        match wait_for_data_valid_event(event_handle as usize, tick_interval).await
                        {
                            Ok(_) => {}
                            Err(err) => {
                                warn!("Failed waiting on iRacing data-valid event: {}", err);
                                adapter.shared_memory = None;
                                continue;
                            }
                        }
                    } else {
                        tokio::time::sleep(tick_interval).await;
                    }
                }

                match adapter.read_telemetry_data() {
                    Ok(sample) => {
                        if last_tick_count == Some(sample.tick_count) {
                            continue;
                        }
                        last_tick_count = Some(sample.tick_count);
                        tick_interval = sample.tick_interval;
                        let layout_signature = irsdk_layout_signature(&sample.header);
                        let layout_changed = last_layout_signature != Some(layout_signature);
                        let session_info_changed =
                            last_session_info_update != Some(sample.session_info_update);

                        if layout_changed && let Some(shared) = adapter.shared_memory.as_mut() {
                            match build_iracing_layout(shared.base_ptr, &sample.header) {
                                Ok(updated_layout) => {
                                    shared.layout = updated_layout;
                                    last_layout_signature = Some(layout_signature);
                                    debug!("Refreshed iRacing layout from header change");
                                }
                                Err(err) => {
                                    warn!(
                                        "Failed to refresh iRacing variable layout after header change: {}",
                                        err
                                    );
                                }
                            }
                        }

                        if session_info_changed {
                            last_session_info_update = Some(sample.session_info_update);
                            warned_unscaled_ffb = false;
                            if let Some(session_info) = read_session_info_yaml(
                                adapter.shared_memory.as_ref(),
                                sample.session_info_offset,
                                sample.session_info_len,
                            ) {
                                debug!(
                                    "Updated iRacing session info ({} bytes)",
                                    session_info.len()
                                );
                                if let Err(err) =
                                    serde_yaml::from_str::<serde_yaml::Value>(&session_info)
                                {
                                    debug!("Failed to parse iRacing session info YAML: {}", err);
                                }
                            }

                            if !layout_changed && let Some(shared) = adapter.shared_memory.as_mut()
                            {
                                match build_iracing_layout(shared.base_ptr, &sample.header) {
                                    Ok(updated_layout) => {
                                        shared.layout = updated_layout;
                                    }
                                    Err(err) => {
                                        warn!("Failed to refresh iRacing variable layout: {}", err);
                                    }
                                }
                            }
                        }

                        let layout = match adapter.shared_memory.as_ref() {
                            Some(shared) => shared.layout,
                            None => IRacingLayout::default(),
                        };

                        let frame = TelemetryFrame::new(
                            adapter.normalize_iracing_data(
                                &sample.data,
                                &layout,
                                &mut warned_unscaled_ffb,
                            ),
                            telemetry_now_ns(),
                            sequence,
                            mem::size_of::<IRacingData>(),
                        );

                        if tx.send(frame).await.is_err() {
                            debug!("Telemetry receiver dropped, stopping monitoring");
                            break;
                        }

                        sequence = sequence.saturating_add(1);
                    }
                    Err(e) => {
                        warn!("Failed to read iRacing telemetry: {}", e);
                        adapter.shared_memory = None;
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                }

                if use_data_valid_event {
                    // Event mode already blocks until new data is available.
                    continue;
                }
            }

            #[cfg(not(windows))]
            warn!("iRacing shared memory is only supported on Windows");

            info!("Stopped iRacing telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        let min_raw_size = mem::size_of::<IRacingLegacyData>();
        let max_raw_size = mem::size_of::<IRacingData>();

        if raw.len() < min_raw_size {
            return Err(anyhow!(
                "Invalid iRacing raw size: expected at least {min_raw_size}, got {}",
                raw.len()
            ));
        }

        let data = if raw.len() < max_raw_size {
            let mut legacy = IRacingLegacyData::default();
            // SAFETY: destination is a plain-old-data struct and `raw` length is validated.
            unsafe {
                ptr::copy_nonoverlapping(
                    raw.as_ptr(),
                    &mut legacy as *mut IRacingLegacyData as *mut u8,
                    min_raw_size,
                );
            }
            convert_legacy_to_current(&legacy)
        } else {
            let mut data = IRacingData::default();
            // SAFETY: destination is a plain-old-data struct and `raw` length is validated.
            unsafe {
                ptr::copy_nonoverlapping(
                    raw.as_ptr(),
                    &mut data as *mut IRacingData as *mut u8,
                    max_raw_size,
                );
            }
            data
        };
        let mut warned_unscaled_ffb = false;
        Ok(self.normalize_iracing_data(&data, &IRacingLayout::default(), &mut warned_unscaled_ffb))
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.check_iracing_running().await)
    }
}

impl IRacingAdapter {
    fn normalize_iracing_data(
        &self,
        data: &IRacingData,
        layout: &IRacingLayout,
        warned_unscaled_ffb: &mut bool,
    ) -> NormalizedTelemetry {
        let (ffb_scalar_source, ffb_scalar) = resolve_ffb_scalar_with_source(data, layout);

        if ffb_scalar.is_none() && !*warned_unscaled_ffb {
            warn!(
                "iRacing FFB scalar missing. Data source may not expose steering torque metadata yet."
            );
            *warned_unscaled_ffb = true;
        }

        let flags = TelemetryFlags {
            yellow_flag: (data.session_flags & IRSDK_SESSION_FLAG_YELLOW) != 0,
            red_flag: (data.session_flags & IRSDK_SESSION_FLAG_RED) != 0,
            blue_flag: (data.session_flags & IRSDK_SESSION_FLAG_BLUE) != 0,
            checkered_flag: (data.session_flags & IRSDK_SESSION_FLAG_CHECKERED) != 0,
            green_flag: (data.session_flags & IRSDK_SESSION_FLAG_GREEN) != 0,
            in_pits: data.on_pit_road != 0,
            ..Default::default()
        };

        let car_id = extract_string(&data.car_path);
        let track_id = extract_string(&data.track_name);

        let mut builder = NormalizedTelemetry::builder()
            .rpm(data.rpm)
            .speed_ms(data.speed)
            .gear(data.gear)
            .car_id(car_id)
            .track_id(track_id)
            .flags(flags)
            .extended(
                "ffb_scalar_source".to_string(),
                TelemetryValue::String(ffb_scalar_source.as_str().to_string()),
            )
            .extended(
                "session_flags_raw".to_string(),
                TelemetryValue::Integer(data.session_flags as i32),
            );

        if let Some(ffb) = ffb_scalar {
            builder = builder.ffb_scalar(ffb);
        }

        if let Some((slip_ratio, slip_ratio_source)) = resolve_slip_ratio(data, layout) {
            builder = builder.slip_ratio(slip_ratio).extended(
                "slip_ratio_source".to_string(),
                TelemetryValue::String(match slip_ratio_source {
                    SlipRatioSource::Explicit => "explicit".to_string(),
                    SlipRatioSource::DerivedFromWheelSpeeds => "derived_wheel_rps".to_string(),
                }),
            );
        }

        if layout.steering_wheel_limiter.is_some() && data.steering_wheel_limiter.is_finite() {
            builder = builder.extended(
                "ffb_limiter_pct".to_string(),
                TelemetryValue::Float(data.steering_wheel_limiter),
            );
        }

        builder
            .extended(
                "fuel_level".to_string(),
                TelemetryValue::Float(data.fuel_level),
            )
            .extended(
                "lap_current".to_string(),
                TelemetryValue::Integer(data.lap_current),
            )
            .extended(
                "lap_best_time".to_string(),
                TelemetryValue::Float(data.lap_best_time),
            )
            .extended(
                "session_time".to_string(),
                TelemetryValue::Float(data.session_time),
            )
            .extended("throttle".to_string(), TelemetryValue::Float(data.throttle))
            .extended("brake".to_string(), TelemetryValue::Float(data.brake))
            .extended(
                "steering_wheel_angle".to_string(),
                TelemetryValue::Float(data.steering_wheel_angle),
            )
            .build()
    }
}

fn convert_legacy_to_current(legacy: &IRacingLegacyData) -> IRacingData {
    IRacingData {
        session_time: legacy.session_time,
        session_flags: legacy.session_flags,
        speed: legacy.speed,
        rpm: legacy.rpm,
        gear: legacy.gear,
        throttle: legacy.throttle,
        brake: legacy.brake,
        steering_wheel_angle: legacy.steering_wheel_angle,
        steering_wheel_torque: legacy.steering_wheel_torque,
        lf_tire_rps: legacy.lf_tire_rps,
        rf_tire_rps: legacy.rf_tire_rps,
        lr_tire_rps: legacy.lr_tire_rps,
        rr_tire_rps: legacy.rr_tire_rps,
        lap_current: legacy.lap_current,
        lap_best_time: legacy.lap_best_time,
        fuel_level: legacy.fuel_level,
        on_pit_road: legacy.on_pit_road,
        car_path: legacy.car_path,
        track_name: legacy.track_name,
        ..IRacingData::default()
    }
}

/// Extract null-terminated string from byte array.
fn extract_string(bytes: &[u8]) -> String {
    match bytes.iter().position(|&b| b == 0) {
        Some(pos) => decode_iso_8859_1_string(&bytes[..pos]),
        None => decode_iso_8859_1_string(bytes),
    }
}

fn validate_irsdk_header(header: &IRSDKHeader) -> Result<()> {
    if header.num_buf <= 0 {
        return Err(anyhow!(
            "IRSDK header has no buffers (num_buf={})",
            header.num_buf
        ));
    }
    if header.num_buf as usize > IRSDK_MAX_BUFS {
        return Err(anyhow!(
            "IRSDK header reports too many buffers: {}",
            header.num_buf
        ));
    }
    if header.buf_len <= 0 {
        return Err(anyhow!(
            "IRSDK header reports invalid buffer length: {}",
            header.buf_len
        ));
    }
    if header.num_vars <= 0 || header.num_vars > IRSDK_MAX_VARS {
        return Err(anyhow!(
            "IRSDK header reports invalid variable count: {}",
            header.num_vars
        ));
    }
    if header.var_header_offset < 0 {
        return Err(anyhow!(
            "IRSDK header reports negative variable header offset: {}",
            header.var_header_offset
        ));
    }
    Ok(())
}

fn select_latest_var_buffer(header: &IRSDKHeader) -> Option<(usize, IRSDKVarBuf)> {
    let num_buf = usize::try_from(header.num_buf).ok()?;
    if num_buf == 0 || num_buf > IRSDK_MAX_BUFS {
        return None;
    }

    let mut best_index = 0usize;
    let mut best = header.var_buf[0];
    for index in 1..num_buf {
        let candidate = header.var_buf[index];
        if candidate.tick_count > best.tick_count {
            best = candidate;
            best_index = index;
        }
    }

    Some((best_index, best))
}

fn irsdk_layout_signature(header: &IRSDKHeader) -> (i32, i32, i32, i32) {
    (
        header.num_vars,
        header.var_header_offset,
        header.num_buf,
        header.buf_len,
    )
}

#[cfg(windows)]
fn read_irsdk_header_from_ptr(base_ptr: *const u8) -> IRSDKHeader {
    // SAFETY: caller provides a valid mapped view beginning at IRSDK header.
    unsafe { ptr::read_volatile(base_ptr as *const IRSDKHeader) }
}

#[cfg(windows)]
fn read_iracing_data_from_ptr(
    base_ptr: *const u8,
    buffer: IRSDKVarBuf,
    layout: &IRacingLayout,
) -> Result<IRacingData> {
    let offset = usize::try_from(buffer.buf_offset).with_context(|| {
        format!(
            "Invalid negative iRacing buffer offset: {}",
            buffer.buf_offset
        )
    })?;
    let mut data = IRacingData {
        session_time: read_f32_var(base_ptr, offset, layout.session_time).unwrap_or(0.0),
        session_flags: read_i32_var(base_ptr, offset, layout.session_flags).unwrap_or(0) as u32,
        speed: read_f32_var(base_ptr, offset, layout.speed).unwrap_or(0.0),
        rpm: read_f32_var(base_ptr, offset, layout.rpm).unwrap_or(0.0),
        gear: read_i32_var(base_ptr, offset, layout.gear)
            .unwrap_or(0)
            .clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8,
        throttle: read_f32_var(base_ptr, offset, layout.throttle).unwrap_or(0.0),
        brake: read_f32_var(base_ptr, offset, layout.brake).unwrap_or(0.0),
        steering_wheel_angle: read_f32_var(base_ptr, offset, layout.steering_wheel_angle)
            .unwrap_or(0.0),
        steering_wheel_torque: read_f32_var(base_ptr, offset, layout.steering_wheel_torque)
            .unwrap_or(0.0),
        steering_wheel_pct_torque_sign: read_f32_var(
            base_ptr,
            offset,
            layout.steering_wheel_pct_torque_sign,
        )
        .unwrap_or(0.0),
        steering_wheel_limiter: read_f32_var(base_ptr, offset, layout.steering_wheel_limiter)
            .unwrap_or(0.0),
        steering_wheel_max_force_nm: read_f32_var(
            base_ptr,
            offset,
            layout.steering_wheel_max_force_nm,
        )
        .unwrap_or(0.0),
        lf_tire_slip_ratio: read_f32_var(base_ptr, offset, layout.lf_tire_slip_ratio)
            .unwrap_or(0.0),
        rf_tire_slip_ratio: read_f32_var(base_ptr, offset, layout.rf_tire_slip_ratio)
            .unwrap_or(0.0),
        lr_tire_slip_ratio: read_f32_var(base_ptr, offset, layout.lr_tire_slip_ratio)
            .unwrap_or(0.0),
        rr_tire_slip_ratio: read_f32_var(base_ptr, offset, layout.rr_tire_slip_ratio)
            .unwrap_or(0.0),
        lf_tire_rps: read_f32_var(base_ptr, offset, layout.lf_tire_speed).unwrap_or(0.0),
        rf_tire_rps: read_f32_var(base_ptr, offset, layout.rf_tire_speed).unwrap_or(0.0),
        lr_tire_rps: read_f32_var(base_ptr, offset, layout.lr_tire_speed).unwrap_or(0.0),
        rr_tire_rps: read_f32_var(base_ptr, offset, layout.rr_tire_speed).unwrap_or(0.0),
        lap_current: read_i32_var(base_ptr, offset, layout.lap_current).unwrap_or(0),
        lap_best_time: read_f32_var(base_ptr, offset, layout.lap_best_time).unwrap_or(0.0),
        fuel_level: read_f32_var(base_ptr, offset, layout.fuel_level).unwrap_or(0.0),
        on_pit_road: if read_bool_var(base_ptr, offset, layout.on_pit_road).unwrap_or(false) {
            1
        } else {
            0
        },
        ..IRacingData::default()
    };

    copy_string_var(base_ptr, offset, layout.car_path, &mut data.car_path);
    copy_string_var(base_ptr, offset, layout.track_name, &mut data.track_name);

    Ok(data)
}

#[cfg(windows)]
fn read_f32_var(
    base_ptr: *const u8,
    base_offset: usize,
    binding: Option<VarBinding>,
) -> Option<f32> {
    let binding = binding?;
    let byte_offset = base_offset.checked_add(binding.offset)?;
    Some(match binding.var_type {
        IRSDK_VAR_TYPE_FLOAT => unsafe_read_unaligned::<f32>(base_ptr, byte_offset),
        IRSDK_VAR_TYPE_DOUBLE => unsafe_read_unaligned::<f64>(base_ptr, byte_offset) as f32,
        IRSDK_VAR_TYPE_INT | IRSDK_VAR_TYPE_BITFIELD => {
            unsafe_read_unaligned::<i32>(base_ptr, byte_offset) as f32
        }
        IRSDK_VAR_TYPE_BOOL => {
            if unsafe_read_unaligned::<u8>(base_ptr, byte_offset) == 0 {
                0.0
            } else {
                1.0
            }
        }
        _ => return None,
    })
}

#[cfg(windows)]
fn read_i32_var(
    base_ptr: *const u8,
    base_offset: usize,
    binding: Option<VarBinding>,
) -> Option<i32> {
    let binding = binding?;
    let byte_offset = base_offset.checked_add(binding.offset)?;
    Some(match binding.var_type {
        IRSDK_VAR_TYPE_INT | IRSDK_VAR_TYPE_BITFIELD => {
            unsafe_read_unaligned::<i32>(base_ptr, byte_offset)
        }
        IRSDK_VAR_TYPE_BOOL => {
            if unsafe_read_unaligned::<u8>(base_ptr, byte_offset) == 0 {
                0
            } else {
                1
            }
        }
        IRSDK_VAR_TYPE_FLOAT => unsafe_read_unaligned::<f32>(base_ptr, byte_offset) as i32,
        IRSDK_VAR_TYPE_DOUBLE => unsafe_read_unaligned::<f64>(base_ptr, byte_offset) as i32,
        _ => return None,
    })
}

#[cfg(windows)]
fn read_bool_var(
    base_ptr: *const u8,
    base_offset: usize,
    binding: Option<VarBinding>,
) -> Option<bool> {
    let binding = binding?;
    let byte_offset = base_offset.checked_add(binding.offset)?;
    Some(match binding.var_type {
        IRSDK_VAR_TYPE_BOOL => unsafe_read_unaligned::<u8>(base_ptr, byte_offset) != 0,
        IRSDK_VAR_TYPE_INT | IRSDK_VAR_TYPE_BITFIELD => {
            unsafe_read_unaligned::<i32>(base_ptr, byte_offset) != 0
        }
        IRSDK_VAR_TYPE_FLOAT => unsafe_read_unaligned::<f32>(base_ptr, byte_offset) != 0.0,
        IRSDK_VAR_TYPE_DOUBLE => unsafe_read_unaligned::<f64>(base_ptr, byte_offset) != 0.0,
        _ => return None,
    })
}

#[cfg(windows)]
fn copy_string_var(
    base_ptr: *const u8,
    base_offset: usize,
    binding: Option<VarBinding>,
    target: &mut [u8],
) {
    let Some(binding) = binding else {
        return;
    };
    if binding.var_type != IRSDK_VAR_TYPE_CHAR || binding.count == 0 {
        return;
    }

    let Some(byte_offset) = base_offset.checked_add(binding.offset) else {
        return;
    };
    let copy_len = target.len().min(binding.count);

    // SAFETY: caller passes a mapped view; copy length is bounded by target size.
    unsafe {
        ptr::copy_nonoverlapping(
            base_ptr.wrapping_add(byte_offset),
            target.as_mut_ptr(),
            copy_len,
        );
    }

    if !target.contains(&0) && !target.is_empty() {
        target[target.len() - 1] = 0;
    }
}

#[cfg(windows)]
fn unsafe_read_unaligned<T: Copy>(base_ptr: *const u8, byte_offset: usize) -> T {
    // SAFETY: caller ensures pointer points to mapped shared memory and type matches layout.
    unsafe { ptr::read_unaligned(base_ptr.wrapping_add(byte_offset) as *const T) }
}

#[cfg(windows)]
fn build_iracing_layout(base_ptr: *const u8, header: &IRSDKHeader) -> Result<IRacingLayout> {
    let num_vars = usize::try_from(header.num_vars)
        .with_context(|| format!("Invalid IRSDK variable count: {}", header.num_vars))?;
    let var_header_offset = usize::try_from(header.var_header_offset).with_context(|| {
        format!(
            "Invalid IRSDK variable header offset: {}",
            header.var_header_offset
        )
    })?;
    let buf_len = usize::try_from(header.buf_len)
        .with_context(|| format!("Invalid IRSDK buffer length: {}", header.buf_len))?;

    let mut layout = IRacingLayout::default();
    let var_header_size = mem::size_of::<IRSDKVarHeader>();

    for index in 0..num_vars {
        let entry_offset = var_header_offset
            .checked_add(index.saturating_mul(var_header_size))
            .ok_or_else(|| anyhow!("IRSDK variable header offset overflow"))?;
        // SAFETY: reading from mapped shared memory by header-defined offset.
        let var_header = unsafe {
            ptr::read_unaligned(base_ptr.wrapping_add(entry_offset) as *const IRSDKVarHeader)
        };
        let Some(binding) = var_binding_from_header(&var_header, buf_len) else {
            continue;
        };

        let name = extract_string(&var_header.name);
        assign_var_binding(&mut layout, &name, binding);
    }

    Ok(layout)
}

#[cfg(windows)]
fn var_binding_from_header(var_header: &IRSDKVarHeader, buf_len: usize) -> Option<VarBinding> {
    let offset = usize::try_from(var_header.offset).ok()?;
    let count = usize::try_from(var_header.count).ok()?;
    if count == 0 {
        return None;
    }

    let elem_size = irsdk_var_type_size(var_header.var_type)?;
    let byte_len = elem_size.checked_mul(count)?;
    let end = offset.checked_add(byte_len)?;
    if end > buf_len {
        return None;
    }

    Some(VarBinding {
        var_type: var_header.var_type,
        offset,
        count,
        _unit: var_header.unit,
    })
}

#[cfg(windows)]
fn irsdk_var_type_size(var_type: i32) -> Option<usize> {
    Some(match var_type {
        IRSDK_VAR_TYPE_CHAR | IRSDK_VAR_TYPE_BOOL => 1,
        IRSDK_VAR_TYPE_INT | IRSDK_VAR_TYPE_BITFIELD | IRSDK_VAR_TYPE_FLOAT => 4,
        IRSDK_VAR_TYPE_DOUBLE => 8,
        _ => return None,
    })
}

fn assign_var_binding(layout: &mut IRacingLayout, name: &str, binding: VarBinding) {
    if matches_irsdk_name(name, &["SessionTime"]) {
        layout.session_time = Some(binding);
    } else if matches_irsdk_name(name, &["SessionFlags"]) {
        layout.session_flags = Some(binding);
    } else if matches_irsdk_name(name, &["Speed"]) {
        layout.speed = Some(binding);
    } else if matches_irsdk_name(name, &["RPM"]) {
        layout.rpm = Some(binding);
    } else if matches_irsdk_name(name, &["Gear"]) {
        layout.gear = Some(binding);
    } else if matches_irsdk_name(name, &["Throttle"]) {
        layout.throttle = Some(binding);
    } else if matches_irsdk_name(name, &["Brake"]) {
        layout.brake = Some(binding);
    } else if matches_irsdk_name(name, &["SteeringWheelAngle"]) {
        layout.steering_wheel_angle = Some(binding);
    } else if matches_irsdk_name(name, &["SteeringWheelTorque"]) {
        layout.steering_wheel_torque = Some(binding);
    } else if matches_irsdk_name(name, &["SteeringWheelPctTorqueSign"]) {
        layout.steering_wheel_pct_torque_sign = Some(binding);
    } else if matches_irsdk_name(name, &["SteeringWheelMaxForceNm"]) {
        layout.steering_wheel_max_force_nm = Some(binding);
    } else if matches_irsdk_name(name, &["SteeringWheelLimiter"]) {
        layout.steering_wheel_limiter = Some(binding);
    } else if matches_irsdk_name(name, &["LFwheelSpeed", "LFWheelSpeed", "LFspeed"]) {
        layout.lf_tire_speed = Some(binding);
    } else if matches_irsdk_name(name, &["RFwheelSpeed", "RFWheelSpeed", "RFspeed"]) {
        layout.rf_tire_speed = Some(binding);
    } else if matches_irsdk_name(name, &["LRwheelSpeed", "LRWheelSpeed", "LRspeed"]) {
        layout.lr_tire_speed = Some(binding);
    } else if matches_irsdk_name(name, &["RRwheelSpeed", "RRWheelSpeed", "RRspeed"]) {
        layout.rr_tire_speed = Some(binding);
    } else if matches_irsdk_name(name, &["LFSlipRatio", "LFSlip", "LFTreadSlip"]) {
        layout.lf_tire_slip_ratio = Some(binding);
    } else if matches_irsdk_name(name, &["RFSlipRatio", "RFSlip", "RFTreadSlip"]) {
        layout.rf_tire_slip_ratio = Some(binding);
    } else if matches_irsdk_name(name, &["LRSlipRatio", "LRSlip", "LRTreadSlip"]) {
        layout.lr_tire_slip_ratio = Some(binding);
    } else if matches_irsdk_name(name, &["RRSlipRatio", "RRSlip", "RRTreadSlip"]) {
        layout.rr_tire_slip_ratio = Some(binding);
    } else if matches_irsdk_name(name, &["Lap"]) {
        layout.lap_current = Some(binding);
    } else if matches_irsdk_name(name, &["LapBestLapTime"]) {
        layout.lap_best_time = Some(binding);
    } else if matches_irsdk_name(name, &["FuelLevel", "FuelLevelPct"]) {
        layout.fuel_level = Some(binding);
    } else if matches_irsdk_name(name, &["OnPitRoad"]) {
        layout.on_pit_road = Some(binding);
    } else if matches_irsdk_name(name, &["CarPath"]) {
        layout.car_path = Some(binding);
    } else if matches_irsdk_name(name, &["TrackName"]) {
        layout.track_name = Some(binding);
    }
}

fn matches_irsdk_name(name: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

#[cfg(windows)]
fn to_wide_null_terminated(value: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn open_irsdk_data_valid_event() -> Option<HANDLE> {
    let wide_name = to_wide_null_terminated(IRACING_DATA_VALID_EVENT_NAME);

    // SAFETY: Win32 call with a valid null-terminated UTF-16 event name.
    let handle = unsafe { OpenEventW(SYNCHRONIZE, 0, wide_name.as_ptr()) };
    if handle.is_null() { None } else { Some(handle) }
}

fn calculate_tick_interval(tick_rate: i32) -> Duration {
    if tick_rate <= 0 {
        return IRSDK_DEFAULT_TICK_RATE;
    }

    if (1.0_f64 / tick_rate as f64).is_finite() {
        Duration::from_secs_f64(1.0 / tick_rate as f64)
    } else {
        IRSDK_DEFAULT_TICK_RATE
    }
}

fn duration_to_wait_ms(duration: Duration) -> u32 {
    let millis = (duration.as_millis() as f64 * 1.8).clamp(1.0, u32::MAX as f64);
    millis as u32
}

#[cfg(windows)]
async fn wait_for_data_valid_event(handle: usize, timeout: Duration) -> Result<bool> {
    let timeout_ms = duration_to_wait_ms(timeout);
    let wait_result = task::spawn_blocking(move || {
        let handle = handle as HANDLE;
        // SAFETY: waiting on event handle acquired via OpenEventW.
        unsafe { WaitForSingleObject(handle, timeout_ms) }
    })
    .await
    .map_err(|error| anyhow!("Failed waiting for iRacing data-valid event: {error}"))?;

    match wait_result {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => Err(anyhow!("iRacing data-valid event wait failed")),
        _ => Err(anyhow!(
            "iRacing data-valid event returned unexpected status: {wait_result}"
        )),
    }
}

#[cfg(windows)]
fn read_session_info_yaml(
    shared_memory: Option<&SharedMemoryHandle>,
    offset: i32,
    len: i32,
) -> Option<String> {
    let shared_memory = shared_memory?;
    let offset = usize::try_from(offset).ok()?;
    let len = usize::try_from(len).ok()?;
    if len == 0 {
        return None;
    }

    let bytes = {
        let base = shared_memory.base_ptr.wrapping_add(offset);
        // SAFETY: session-info block and length are supplied by the shared memory header.
        unsafe { std::slice::from_raw_parts(base, len) }
    };
    let end = bytes.iter().position(|value| *value == 0).unwrap_or(len);
    Some(decode_iso_8859_1_string(&bytes[..end]))
}

#[cfg(test)]
fn resolve_ffb_scalar(data: &IRacingData, layout: &IRacingLayout) -> Option<f32> {
    resolve_ffb_scalar_with_source(data, layout).1
}

fn resolve_ffb_scalar_with_source(
    data: &IRacingData,
    layout: &IRacingLayout,
) -> (FfbScalarSource, Option<f32>) {
    if layout.steering_wheel_pct_torque_sign.is_some()
        && data.steering_wheel_pct_torque_sign.is_finite()
    {
        return (
            FfbScalarSource::PctTorqueSign,
            Some((data.steering_wheel_pct_torque_sign / 100.0).clamp(-1.0, 1.0)),
        );
    }

    if layout.steering_wheel_max_force_nm.is_some()
        && data.steering_wheel_max_force_nm.is_finite()
        && data.steering_wheel_max_force_nm.abs() > f32::EPSILON
    {
        return (
            FfbScalarSource::MaxForceNm,
            Some((data.steering_wheel_torque / data.steering_wheel_max_force_nm).clamp(-1.0, 1.0)),
        );
    }

    (FfbScalarSource::Unknown, None)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SlipRatioSource {
    Explicit,
    DerivedFromWheelSpeeds,
}

fn resolve_slip_ratio(
    data: &IRacingData,
    layout: &IRacingLayout,
) -> Option<(f32, SlipRatioSource)> {
    let max_abs_slip = [
        (layout.lf_tire_slip_ratio, data.lf_tire_slip_ratio),
        (layout.rf_tire_slip_ratio, data.rf_tire_slip_ratio),
        (layout.lr_tire_slip_ratio, data.lr_tire_slip_ratio),
        (layout.rr_tire_slip_ratio, data.rr_tire_slip_ratio),
    ]
    .into_iter()
    .filter_map(|(binding, value)| {
        if binding.is_some() && value.is_finite() {
            Some(value.abs())
        } else {
            None
        }
    })
    .max_by(f32::total_cmp);

    if let Some(max_abs) = max_abs_slip {
        return Some((max_abs.clamp(0.0, 1.0), SlipRatioSource::Explicit));
    }

    let derived_max_abs_slip = [
        (layout.lf_tire_speed, data.lf_tire_rps),
        (layout.rf_tire_speed, data.rf_tire_rps),
        (layout.lr_tire_speed, data.lr_tire_rps),
        (layout.rr_tire_speed, data.rr_tire_rps),
    ]
    .into_iter()
    .filter_map(|(binding, value)| {
        if binding.is_some() {
            derive_slip_ratio_from_tire_rps(value, data.speed)
        } else {
            None
        }
    })
    .max_by(f32::total_cmp);

    if let Some(max_abs) = derived_max_abs_slip {
        return Some((max_abs, SlipRatioSource::DerivedFromWheelSpeeds));
    }
    None
}

fn derive_slip_ratio_from_tire_rps(tire_rps: f32, vehicle_speed_ms: f32) -> Option<f32> {
    if !tire_rps.is_finite() || !vehicle_speed_ms.is_finite() {
        return None;
    }
    if tire_rps.abs() < f32::EPSILON {
        return None;
    }

    let vehicle_speed_ms = vehicle_speed_ms.abs();
    let wheel_surface_speed_ms =
        tire_rps.abs() * 2.0 * std::f32::consts::PI * IRSDK_DEFAULT_TIRE_RADIUS_M;

    if wheel_surface_speed_ms < IRSDK_MIN_TIRE_SURFACE_SPEED_MPS {
        return None;
    }

    let reference_speed = wheel_surface_speed_ms
        .max(vehicle_speed_ms)
        .max(IRSDK_MIN_TIRE_SURFACE_SPEED_MPS);
    Some(((wheel_surface_speed_ms - vehicle_speed_ms).abs() / reference_speed).clamp(0.0, 1.0))
}

fn decode_iso_8859_1_string(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| *byte as char).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FfbScalarSource {
    PctTorqueSign,
    MaxForceNm,
    Unknown,
}

impl FfbScalarSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::PctTorqueSign => "pct_torque_sign",
            Self::MaxForceNm => "max_force_nm",
            Self::Unknown => "unknown",
        }
    }
}

/// iRacing shared-memory payload (simplified local view).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct IRacingData {
    session_time: f32,
    session_flags: u32,
    speed: f32,
    rpm: f32,
    gear: i8,
    throttle: f32,
    brake: f32,
    steering_wheel_angle: f32,
    steering_wheel_torque: f32,
    steering_wheel_pct_torque_sign: f32,
    steering_wheel_max_force_nm: f32,
    steering_wheel_limiter: f32,
    lf_tire_slip_ratio: f32,
    rf_tire_slip_ratio: f32,
    lr_tire_slip_ratio: f32,
    rr_tire_slip_ratio: f32,
    lf_tire_rps: f32,
    rf_tire_rps: f32,
    lr_tire_rps: f32,
    rr_tire_rps: f32,
    lap_current: i32,
    lap_best_time: f32,
    fuel_level: f32,
    on_pit_road: i32,
    car_path: [u8; 64],
    track_name: [u8; 64],
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct IRacingLegacyData {
    session_time: f32,
    session_flags: u32,
    speed: f32,
    rpm: f32,
    gear: i8,
    throttle: f32,
    brake: f32,
    steering_wheel_angle: f32,
    steering_wheel_torque: f32,
    lf_tire_rps: f32,
    rf_tire_rps: f32,
    lr_tire_rps: f32,
    rr_tire_rps: f32,
    lap_current: i32,
    lap_best_time: f32,
    fuel_level: f32,
    on_pit_road: i32,
    car_path: [u8; 64],
    track_name: [u8; 64],
}

impl Default for IRacingData {
    fn default() -> Self {
        Self {
            session_time: 0.0,
            session_flags: 0,
            speed: 0.0,
            rpm: 0.0,
            gear: 0,
            throttle: 0.0,
            brake: 0.0,
            steering_wheel_angle: 0.0,
            steering_wheel_torque: 0.0,
            steering_wheel_pct_torque_sign: 0.0,
            steering_wheel_max_force_nm: 0.0,
            steering_wheel_limiter: 0.0,
            lf_tire_slip_ratio: 0.0,
            rf_tire_slip_ratio: 0.0,
            lr_tire_slip_ratio: 0.0,
            rr_tire_slip_ratio: 0.0,
            lf_tire_rps: 0.0,
            rf_tire_rps: 0.0,
            lr_tire_rps: 0.0,
            rr_tire_rps: 0.0,
            lap_current: 0,
            lap_best_time: 0.0,
            fuel_level: 0.0,
            on_pit_road: 0,
            car_path: [0; 64],
            track_name: [0; 64],
        }
    }
}

impl Default for IRacingLegacyData {
    fn default() -> Self {
        Self {
            session_time: 0.0,
            session_flags: 0,
            speed: 0.0,
            rpm: 0.0,
            gear: 0,
            throttle: 0.0,
            brake: 0.0,
            steering_wheel_angle: 0.0,
            steering_wheel_torque: 0.0,
            lf_tire_rps: 0.0,
            rf_tire_rps: 0.0,
            lr_tire_rps: 0.0,
            rr_tire_rps: 0.0,
            lap_current: 0,
            lap_best_time: 0.0,
            fuel_level: 0.0,
            on_pit_road: 0,
            car_path: [0; 64],
            track_name: [0; 64],
        }
    }
}

#[cfg(windows)]
impl Drop for SharedMemoryHandle {
    fn drop(&mut self) {
        // SAFETY: these handles/pointers were created by MapViewOfFile/OpenFileMappingW.
        unsafe {
            if !self.base_ptr.is_null() {
                UnmapViewOfFile(self.base_ptr as *const _);
            }
            if let Some(event_handle) = self.data_valid_event {
                CloseHandle(event_handle);
            }
            if !self.handle.is_null() {
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn to_raw_bytes<T: Copy>(value: &T) -> Vec<u8> {
        let size = mem::size_of::<T>();
        let ptr = value as *const T as *const u8;
        // SAFETY: reading a plain-old-data struct as a byte slice.
        unsafe { std::slice::from_raw_parts(ptr, size).to_vec() }
    }

    fn write_struct<T: Copy>(image: &mut [u8], offset: usize, value: &T) -> TestResult {
        let size = mem::size_of::<T>();
        let end = offset.checked_add(size).ok_or("offset overflow")?;
        if end > image.len() {
            return Err("buffer too small for struct write".into());
        }

        let raw = to_raw_bytes(value);
        image[offset..end].copy_from_slice(&raw);
        Ok(())
    }

    fn read_iracing_data_from_image(image: &[u8]) -> Result<(IRacingData, i32)> {
        if image.len() < mem::size_of::<IRSDKHeader>() {
            return Err(anyhow!("image too small for IRSDK header"));
        }

        // SAFETY: length was validated above; unaligned read avoids UB.
        let header = unsafe { ptr::read_unaligned(image.as_ptr() as *const IRSDKHeader) };
        validate_irsdk_header(&header)?;

        let (_, latest) = select_latest_var_buffer(&header)
            .ok_or_else(|| anyhow!("IRSDK header contains no valid buffers"))?;
        let offset = usize::try_from(latest.buf_offset).context("negative buffer offset")?;
        let end = offset
            .checked_add(mem::size_of::<IRacingData>())
            .ok_or_else(|| anyhow!("buffer end overflow"))?;
        if end > image.len() {
            return Err(anyhow!(
                "buffer offset out of range: offset={}, end={}, len={}",
                offset,
                end,
                image.len()
            ));
        }

        // SAFETY: bounds checked above; unaligned read avoids UB.
        let data = unsafe { ptr::read_unaligned(image[offset..].as_ptr() as *const IRacingData) };
        Ok((data, latest.tick_count))
    }

    #[test]
    fn test_iracing_adapter_creation() -> TestResult {
        let adapter = IRacingAdapter::new();
        assert_eq!(adapter.game_id(), "iracing");
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
        Ok(())
    }

    #[test]
    fn test_select_latest_var_buffer_chooses_highest_tick() -> TestResult {
        let mut header = IRSDKHeader {
            num_buf: 3,
            ..Default::default()
        };
        header.var_buf[0] = IRSDKVarBuf {
            tick_count: 10,
            buf_offset: 128,
            ..Default::default()
        };
        header.var_buf[1] = IRSDKVarBuf {
            tick_count: 42,
            buf_offset: 256,
            ..Default::default()
        };
        header.var_buf[2] = IRSDKVarBuf {
            tick_count: 21,
            buf_offset: 512,
            ..Default::default()
        };

        let (index, selected) = select_latest_var_buffer(&header).ok_or("no selected buffer")?;
        assert_eq!(index, 1);
        assert_eq!(selected.tick_count, 42);
        assert_eq!(selected.buf_offset, 256);
        Ok(())
    }

    #[test]
    fn test_read_from_image_uses_newest_rotating_buffer() -> TestResult {
        let header_size = mem::size_of::<IRSDKHeader>();
        let buffer_a_offset = header_size + 64;
        let buffer_b_offset = header_size + 64 + mem::size_of::<IRacingData>() + 64;
        let image_len = buffer_b_offset + mem::size_of::<IRacingData>() + 64;
        let mut image = vec![0u8; image_len];

        let mut header = IRSDKHeader {
            num_buf: 2,
            num_vars: 1,
            var_header_offset: 0,
            buf_len: i32::try_from(mem::size_of::<IRacingData>())?,
            ..Default::default()
        };
        header.var_buf[0] = IRSDKVarBuf {
            tick_count: 100,
            buf_offset: i32::try_from(buffer_a_offset)?,
            ..Default::default()
        };
        header.var_buf[1] = IRSDKVarBuf {
            tick_count: 101,
            buf_offset: i32::try_from(buffer_b_offset)?,
            ..Default::default()
        };

        let older = IRacingData {
            session_time: 12.0,
            speed: 40.0,
            ..Default::default()
        };
        let newer = IRacingData {
            session_time: 13.0,
            speed: 55.0,
            ..Default::default()
        };

        write_struct(&mut image, 0, &header)?;
        write_struct(&mut image, buffer_a_offset, &older)?;
        write_struct(&mut image, buffer_b_offset, &newer)?;

        let (selected, tick) = read_iracing_data_from_image(&image)?;
        assert_eq!(tick, 101);
        assert_eq!(selected.session_time, 13.0);
        assert_eq!(selected.speed, 55.0);
        Ok(())
    }

    #[test]
    fn test_normalize_iracing_data() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut layout = IRacingLayout::default();
        layout.steering_wheel_pct_torque_sign = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));
        layout.steering_wheel_limiter = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));

        let car_name = b"gt3_bmw\0";
        let track_name = b"spa\0";
        let mut car_path = [0u8; 64];
        let mut track_name_arr = [0u8; 64];
        car_path[..car_name.len()].copy_from_slice(car_name);
        track_name_arr[..track_name.len()].copy_from_slice(track_name);

        let data = IRacingData {
            rpm: 6000.0,
            speed: 50.0,
            gear: 4,
            steering_wheel_torque: 25.0,
            steering_wheel_pct_torque_sign: 25.0,
            steering_wheel_limiter: 73.0,
            steering_wheel_max_force_nm: 0.0,
            throttle: 0.8,
            brake: 0.2,
            session_flags: IRSDK_SESSION_FLAG_CHECKERED,
            car_path,
            track_name: track_name_arr,
            ..Default::default()
        };

        let mut warned_unscaled_ffb = false;
        let normalized = adapter.normalize_iracing_data(&data, &layout, &mut warned_unscaled_ffb);

        assert_eq!(normalized.rpm, 6000.0);
        assert_eq!(normalized.speed_ms, 50.0);
        assert_eq!(normalized.gear, 4);
        assert_eq!(normalized.ffb_scalar, 0.25);
        assert_eq!(normalized.car_id, Some("gt3_bmw".to_string()));
        assert_eq!(normalized.track_id, Some("spa".to_string()));
        assert!(!normalized.flags.yellow_flag);
        assert!(normalized.flags.checkered_flag);
        assert_eq!(
            normalized.extended.get("ffb_limiter_pct"),
            Some(&TelemetryValue::Float(73.0))
        );
        assert_eq!(
            normalized.extended.get("ffb_scalar_source"),
            Some(&TelemetryValue::String("pct_torque_sign".to_string()))
        );
        assert!(normalized.extended.contains_key("throttle"));
        assert!(normalized.extended.contains_key("brake"));
        Ok(())
    }

    #[test]
    fn test_extract_string() -> TestResult {
        let bytes = b"test_string\0extra_data";
        let result = extract_string(bytes);
        assert_eq!(result, "test_string");

        let bytes_no_null = b"no_null_terminator";
        let result = extract_string(bytes_no_null);
        assert_eq!(result, "no_null_terminator");
        Ok(())
    }

    #[test]
    fn test_extract_string_decodes_iso_8859_1() -> TestResult {
        let bytes = [b'g', b't', b'3', b'_', b'\xE9', 0];
        let result = extract_string(&bytes);
        assert_eq!(result, "gt3_é");
        Ok(())
    }

    #[test]
    fn test_normalize_raw_data() -> TestResult {
        let adapter = IRacingAdapter::new();
        let data = IRacingData::default();
        let raw_bytes = to_raw_bytes(&data);

        let result = adapter.normalize(&raw_bytes);
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_normalize_legacy_minimum_iracing_raw() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut legacy = IRacingLegacyData::default();
        legacy.rpm = 5200.0;
        legacy.speed = 33.0;
        legacy.gear = 5;
        legacy.lf_tire_rps = 12.0;
        legacy.rf_tire_rps = 11.0;
        legacy.lr_tire_rps = 12.0;
        legacy.rr_tire_rps = 11.0;
        legacy.car_path[..8].copy_from_slice(b"gt4_test");

        let minimum_raw = to_raw_bytes(&legacy);
        let normalized = adapter.normalize(&minimum_raw)?;

        assert_eq!(normalized.rpm, 5200.0);
        assert_eq!(normalized.speed_ms, 33.0);
        assert_eq!(normalized.gear, 5);
        assert_eq!(normalized.car_id, Some("gt4_test".to_string()));
        Ok(())
    }

    #[test]
    fn test_normalize_legacy_iracing_raw() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut legacy = IRacingLegacyData::default();

        legacy.rpm = 6200.0;
        legacy.speed = 45.0;
        legacy.gear = 4;
        let legacy_name = b"legacy_gt3\0";
        let legacy_track = b"legacy_track\0";
        legacy.car_path[..legacy_name.len()].copy_from_slice(legacy_name);
        legacy.track_name[..legacy_track.len()].copy_from_slice(legacy_track);

        let normalized = adapter.normalize(&to_raw_bytes(&legacy))?;

        assert_eq!(normalized.rpm, 6200.0);
        assert_eq!(normalized.speed_ms, 45.0);
        assert_eq!(normalized.gear, 4);
        assert_eq!(normalized.car_id, Some("legacy_gt3".to_string()));
        assert_eq!(normalized.track_id, Some("legacy_track".to_string()));
        assert_eq!(normalized.ffb_scalar, 0.0);

        Ok(())
    }

    #[test]
    fn test_normalize_iracing_raw_accepts_trailing_bytes() -> TestResult {
        let adapter = IRacingAdapter::new();
        let mut data = IRacingData::default();
        data.rpm = 6000.0;
        data.speed = 55.0;
        data.gear = 3;

        let mut raw = to_raw_bytes(&data);
        raw.extend_from_slice(&[0xFA, 0xCE, 0x00, 0x11, 0x22, 0x33]);

        let normalized = adapter.normalize(&raw)?;
        let normalized_base = adapter.normalize(&to_raw_bytes(&data))?;

        assert_eq!(normalized.rpm, normalized_base.rpm);
        assert_eq!(normalized.speed_ms, normalized_base.speed_ms);
        assert_eq!(normalized.gear, normalized_base.gear);
        assert_eq!(normalized.ffb_scalar, normalized_base.ffb_scalar);
        assert_eq!(normalized.flags, normalized_base.flags);
        Ok(())
    }

    #[test]
    fn test_normalize_invalid_data() -> TestResult {
        let adapter = IRacingAdapter::new();
        let invalid_data = vec![0u8; 10];
        let result = adapter.normalize(&invalid_data);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_normalize_session_flags_are_canonical_bits() -> TestResult {
        let adapter = IRacingAdapter::new();
        let data = IRacingData {
            session_flags: IRSDK_SESSION_FLAG_CHECKERED
                | IRSDK_SESSION_FLAG_YELLOW
                | IRSDK_SESSION_FLAG_GREEN,
            ..IRacingData::default()
        };

        let mut warned_unscaled_ffb = false;
        let normalized = adapter.normalize_iracing_data(
            &data,
            &IRacingLayout::default(),
            &mut warned_unscaled_ffb,
        );

        assert!(normalized.flags.checkered_flag);
        assert!(normalized.flags.yellow_flag);
        assert!(normalized.flags.green_flag);
        assert!(!normalized.flags.red_flag);
        assert!(!normalized.flags.blue_flag);
        assert_eq!(
            telemetry_integer_value(&normalized, "session_flags_raw"),
            Some(
                (IRSDK_SESSION_FLAG_CHECKERED
                    | IRSDK_SESSION_FLAG_YELLOW
                    | IRSDK_SESSION_FLAG_GREEN) as i32
            )
        );
        Ok(())
    }

    fn telemetry_integer_value(telemetry: &NormalizedTelemetry, key: &str) -> Option<i32> {
        match telemetry.extended.get(key) {
            Some(TelemetryValue::Integer(value)) => Some(*value),
            _ => None,
        }
    }

    #[test]
    fn test_layout_signature_changes_are_detected() -> TestResult {
        let mut header = IRSDKHeader::default();
        header.num_vars = 10;
        header.var_header_offset = 128;
        header.num_buf = 3;
        header.buf_len = 256;

        let mut last_signature = None;
        let first_signature = irsdk_layout_signature(&header);
        assert_ne!(Some(first_signature), last_signature);

        last_signature = Some(first_signature);
        assert_eq!(Some(first_signature), last_signature);

        header.var_header_offset = 192;
        assert_ne!(Some(irsdk_layout_signature(&header)), last_signature);

        Ok(())
    }

    fn make_binding(var_type: i32) -> VarBinding {
        VarBinding {
            var_type,
            offset: 0,
            count: 1,
            _unit: [0u8; 32],
        }
    }

    #[test]
    fn test_resolve_ffb_scalar_prefers_pct_torque_sign() -> TestResult {
        let mut layout = IRacingLayout::default();
        layout.steering_wheel_pct_torque_sign = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));
        layout.steering_wheel_max_force_nm = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));
        layout.steering_wheel_torque = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));

        let data = IRacingData {
            steering_wheel_pct_torque_sign: 42.0,
            steering_wheel_torque: 80.0,
            steering_wheel_max_force_nm: 0.0,
            ..IRacingData::default()
        };

        let resolved = resolve_ffb_scalar(&data, &layout);
        assert_eq!(resolved, Some(0.42));
        let (source, scalar) = resolve_ffb_scalar_with_source(&data, &layout);
        assert_eq!(source, FfbScalarSource::PctTorqueSign);
        assert_eq!(scalar, Some(0.42));
        Ok(())
    }

    #[test]
    fn test_resolve_ffb_scalar_falls_back_to_torque_max_force() -> TestResult {
        let mut layout = IRacingLayout::default();
        layout.steering_wheel_torque = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));
        layout.steering_wheel_max_force_nm = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));

        let data = IRacingData {
            steering_wheel_torque: 40.0,
            steering_wheel_max_force_nm: 80.0,
            ..IRacingData::default()
        };

        let resolved = resolve_ffb_scalar(&data, &layout);
        assert_eq!(resolved, Some(0.5));
        let (source, scalar) = resolve_ffb_scalar_with_source(&data, &layout);
        assert_eq!(source, FfbScalarSource::MaxForceNm);
        assert_eq!(scalar, Some(0.5));
        Ok(())
    }

    #[test]
    fn test_resolve_ffb_scalar_is_none_without_percent_or_maxforce_binding() -> TestResult {
        let mut layout = IRacingLayout::default();
        layout.steering_wheel_torque = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));
        let data = IRacingData {
            steering_wheel_torque: 32.0,
            ..IRacingData::default()
        };

        let resolved = resolve_ffb_scalar(&data, &layout);
        assert_eq!(resolved, None);
        let (source, scalar) = resolve_ffb_scalar_with_source(&data, &layout);
        assert_eq!(source, FfbScalarSource::Unknown);
        assert_eq!(scalar, None);
        Ok(())
    }

    #[test]
    fn test_resolve_ffb_scalar_is_none_when_no_binding_available() -> TestResult {
        let layout = IRacingLayout::default();
        let data = IRacingData {
            steering_wheel_torque: 32.0,
            ..IRacingData::default()
        };

        let resolved = resolve_ffb_scalar(&data, &layout);
        assert_eq!(resolved, None);
        Ok(())
    }

    #[test]
    fn test_assign_var_binding_maps_steering_wheel_limiter() {
        let mut layout = IRacingLayout::default();
        assign_var_binding(
            &mut layout,
            "SteeringWheelLimiter",
            make_binding(IRSDK_VAR_TYPE_FLOAT),
        );
        assert!(layout.steering_wheel_limiter.is_some());
    }

    #[test]
    fn test_resolve_slip_ratio_prefers_max_explicit_wheel_ratio() -> TestResult {
        let mut layout = IRacingLayout::default();
        layout.lf_tire_slip_ratio = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));
        layout.rf_tire_slip_ratio = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));
        layout.lr_tire_slip_ratio = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));
        layout.rr_tire_slip_ratio = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));

        let data = IRacingData {
            lf_tire_slip_ratio: 0.2,
            rf_tire_slip_ratio: -0.8,
            lr_tire_slip_ratio: 0.1,
            rr_tire_slip_ratio: 0.5,
            ..IRacingData::default()
        };

        let resolved = resolve_slip_ratio(&data, &layout);
        assert_eq!(resolved, Some((0.8, SlipRatioSource::Explicit)));
        Ok(())
    }

    #[test]
    fn test_resolve_slip_ratio_falls_back_to_wheel_speed_derivation() -> TestResult {
        let mut layout = IRacingLayout::default();
        layout.lf_tire_speed = Some(make_binding(IRSDK_VAR_TYPE_FLOAT));

        let data = IRacingData {
            lf_tire_rps: 20.0,
            speed: 30.0,
            ..IRacingData::default()
        };

        let resolved = resolve_slip_ratio(&data, &layout);
        assert_eq!(
            resolved,
            Some((
                derive_slip_ratio_from_tire_rps(20.0, 30.0)
                    .ok_or("fallback should produce ratio")?,
                SlipRatioSource::DerivedFromWheelSpeeds,
            ))
        );
        Ok(())
    }

    #[test]
    fn test_resolve_slip_ratio_absent_without_binding() -> TestResult {
        let data = IRacingData {
            lf_tire_slip_ratio: 0.4,
            rf_tire_slip_ratio: 0.9,
            ..IRacingData::default()
        };

        let resolved = resolve_slip_ratio(&data, &IRacingLayout::default());
        assert_eq!(resolved, None);
        Ok(())
    }

    #[tokio::test]
    async fn test_is_game_running() -> TestResult {
        let adapter = IRacingAdapter::new();

        #[cfg(not(windows))]
        {
            let result = adapter.is_game_running().await?;
            assert!(!result);
        }

        #[cfg(windows)]
        {
            let _ = adapter.is_game_running().await?;
        }

        Ok(())
    }
}
