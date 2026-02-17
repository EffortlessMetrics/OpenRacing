//! iRacing telemetry adapter with Windows shared-memory access.
//!
//! This adapter opens `Local\\IRSDKMemMapFileName` with `FILE_MAP_READ`,
//! reads the IRSDK header, and selects the newest rotating telemetry buffer.

use crate::telemetry::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    TelemetryValue,
};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use std::mem;
use std::ptr;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

#[cfg(windows)]
use winapi::um::{
    handleapi::CloseHandle,
    memoryapi::{FILE_MAP_READ, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile},
    winnt::HANDLE,
};

#[cfg(windows)]
const IRACING_MAP_NAME: &str = "Local\\IRSDKMemMapFileName";
const IRSDK_MAX_BUFS: usize = 4;
const IRSDK_STABLE_READ_ATTEMPTS: usize = 3;
const IRSDK_MAX_VARS: i32 = 4096;
const IRSDK_VAR_NAME_LEN: usize = 32;

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
    lf_tire_speed: Option<VarBinding>,
    rf_tire_speed: Option<VarBinding>,
    lr_tire_speed: Option<VarBinding>,
    rr_tire_speed: Option<VarBinding>,
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
    tick_count: i32,
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

            self.shared_memory = Some(SharedMemoryHandle {
                handle,
                base_ptr,
                layout,
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
                    tick_count: latest_buf_after.tick_count,
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
            let mut last_tick_count = None;
            let epoch = Instant::now();

            if let Err(e) = adapter.initialize_shared_memory() {
                error!("Failed to initialize iRacing shared memory: {}", e);
                return;
            }

            info!("Started iRacing telemetry monitoring");

            loop {
                match adapter.read_telemetry_data() {
                    Ok(sample) => {
                        if last_tick_count == Some(sample.tick_count) {
                            tokio::time::sleep(update_rate).await;
                            continue;
                        }
                        last_tick_count = Some(sample.tick_count);

                        let frame = TelemetryFrame::new(
                            adapter.normalize_iracing_data(&sample.data),
                            monotonic_ns_since(epoch, Instant::now()),
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
                    }
                }

                tokio::time::sleep(update_rate).await;
            }

            info!("Stopped iRacing telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        if raw.len() != mem::size_of::<IRacingData>() {
            return Err(anyhow!(
                "Invalid iRacing raw size: expected {}, got {}",
                mem::size_of::<IRacingData>(),
                raw.len()
            ));
        }

        // SAFETY: size is validated above; unaligned read avoids UB for arbitrary slices.
        let data = unsafe { ptr::read_unaligned(raw.as_ptr() as *const IRacingData) };
        Ok(self.normalize_iracing_data(&data))
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.check_iracing_running().await)
    }
}

impl IRacingAdapter {
    fn normalize_iracing_data(&self, data: &IRacingData) -> NormalizedTelemetry {
        let flags = TelemetryFlags {
            yellow_flag: (data.session_flags & 0x00000001) != 0,
            red_flag: (data.session_flags & 0x00000002) != 0,
            blue_flag: (data.session_flags & 0x00000004) != 0,
            checkered_flag: (data.session_flags & 0x00000008) != 0,
            green_flag: (data.session_flags & 0x00000010) != 0,
            in_pits: data.on_pit_road != 0,
            ..Default::default()
        };

        let slip_ratio = if data.speed > 1.0 {
            let avg_tire_speed =
                (data.lf_tire_rps + data.rf_tire_rps + data.lr_tire_rps + data.rr_tire_rps) / 4.0;
            let wheel_speed = avg_tire_speed * 0.31;
            ((wheel_speed - data.speed).abs() / data.speed).min(1.0)
        } else {
            0.0
        };

        let car_id = extract_string(&data.car_path);
        let track_id = extract_string(&data.track_name);

        NormalizedTelemetry::default()
            .with_ffb_scalar(data.steering_wheel_torque / 100.0)
            .with_rpm(data.rpm)
            .with_speed_ms(data.speed)
            .with_slip_ratio(slip_ratio)
            .with_gear(data.gear)
            .with_car_id(car_id)
            .with_track_id(track_id)
            .with_flags(flags)
            .with_extended(
                "fuel_level".to_string(),
                TelemetryValue::Float(data.fuel_level),
            )
            .with_extended(
                "lap_current".to_string(),
                TelemetryValue::Integer(data.lap_current),
            )
            .with_extended(
                "lap_best_time".to_string(),
                TelemetryValue::Float(data.lap_best_time),
            )
            .with_extended(
                "session_time".to_string(),
                TelemetryValue::Float(data.session_time),
            )
            .with_extended("throttle".to_string(), TelemetryValue::Float(data.throttle))
            .with_extended("brake".to_string(), TelemetryValue::Float(data.brake))
            .with_extended(
                "steering_wheel_angle".to_string(),
                TelemetryValue::Float(data.steering_wheel_angle),
            )
    }
}

/// Extract null-terminated string from byte array.
fn extract_string(bytes: &[u8]) -> String {
    match bytes.iter().position(|&b| b == 0) {
        Some(pos) => String::from_utf8_lossy(&bytes[..pos]).into_owned(),
        None => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn monotonic_ns_since(epoch: Instant, now: Instant) -> u64 {
    now.checked_duration_since(epoch)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
        .min(u64::MAX as u128) as u64
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

#[cfg(windows)]
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
    } else if matches_irsdk_name(name, &["LFwheelSpeed", "LFWheelSpeed", "LFspeed"]) {
        layout.lf_tire_speed = Some(binding);
    } else if matches_irsdk_name(name, &["RFwheelSpeed", "RFWheelSpeed", "RFspeed"]) {
        layout.rf_tire_speed = Some(binding);
    } else if matches_irsdk_name(name, &["LRwheelSpeed", "LRWheelSpeed", "LRspeed"]) {
        layout.lr_tire_speed = Some(binding);
    } else if matches_irsdk_name(name, &["RRwheelSpeed", "RRWheelSpeed", "RRspeed"]) {
        layout.rr_tire_speed = Some(binding);
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

#[cfg(windows)]
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
            if !self.handle.is_null() {
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
            throttle: 0.8,
            brake: 0.2,
            session_flags: 0x00000001,
            car_path,
            track_name: track_name_arr,
            ..Default::default()
        };

        let normalized = adapter.normalize_iracing_data(&data);

        assert_eq!(normalized.rpm, Some(6000.0));
        assert_eq!(normalized.speed_ms, Some(50.0));
        assert_eq!(normalized.gear, Some(4));
        assert_eq!(normalized.ffb_scalar, Some(0.25));
        assert_eq!(normalized.car_id, Some("gt3_bmw".to_string()));
        assert_eq!(normalized.track_id, Some("spa".to_string()));
        assert!(normalized.flags.yellow_flag);
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
    fn test_normalize_raw_data() -> TestResult {
        let adapter = IRacingAdapter::new();
        let data = IRacingData::default();
        let raw_bytes = to_raw_bytes(&data);

        let result = adapter.normalize(&raw_bytes);
        assert!(result.is_ok());
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
