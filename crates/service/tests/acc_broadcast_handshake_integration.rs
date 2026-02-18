use racing_wheel_service::telemetry::{ACCAdapter, TelemetryAdapter};
use tokio::net::UdpSocket;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const REGISTER_COMMAND_APPLICATION: u8 = 1;
const PROTOCOL_VERSION: u8 = 4;
const MSG_REGISTRATION_RESULT: u8 = 1;

#[tokio::test]
async fn test_acc_is_game_running_on_registration_success() -> TestResult {
    let server = UdpSocket::bind("127.0.0.1:0").await?;
    let endpoint = server.local_addr()?;

    let server_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        let (len, source) = server
            .recv_from(&mut buf)
            .await
            .map_err(|error| error.to_string())?;
        assert_valid_register_packet(&buf[..len])?;

        let response = build_registration_result_packet(77, true, false, "")?;
        let _sent = server
            .send_to(&response, source)
            .await
            .map_err(|error| error.to_string())?;
        Ok::<(), String>(())
    });

    let adapter = ACCAdapter::with_address(endpoint);
    let running = adapter.is_game_running().await?;
    assert!(running, "expected ACC endpoint to be reported as running");

    match server_task.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.into()),
        Err(error) => Err(format!("server task failed: {error}").into()),
    }
}

#[tokio::test]
async fn test_acc_is_game_running_on_registration_rejection() -> TestResult {
    let server = UdpSocket::bind("127.0.0.1:0").await?;
    let endpoint = server.local_addr()?;

    let server_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        let (len, source) = server
            .recv_from(&mut buf)
            .await
            .map_err(|error| error.to_string())?;
        assert_valid_register_packet(&buf[..len])?;

        let response = build_registration_result_packet(0, false, true, "readonly")?;
        let _sent = server
            .send_to(&response, source)
            .await
            .map_err(|error| error.to_string())?;
        Ok::<(), String>(())
    });

    let adapter = ACCAdapter::with_address(endpoint);
    let running = adapter.is_game_running().await?;
    assert!(
        running,
        "registration response should indicate ACC endpoint is live even when rejected"
    );

    match server_task.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.into()),
        Err(error) => Err(format!("server task failed: {error}").into()),
    }
}

fn assert_valid_register_packet(packet: &[u8]) -> Result<(), String> {
    if packet.len() < 2 {
        return Err("register packet too short".to_string());
    }

    if packet[0] != REGISTER_COMMAND_APPLICATION {
        return Err(format!(
            "unexpected command type: got {}, expected {}",
            packet[0], REGISTER_COMMAND_APPLICATION
        ));
    }

    if packet[1] != PROTOCOL_VERSION {
        return Err(format!(
            "unexpected protocol version: got {}, expected {}",
            packet[1], PROTOCOL_VERSION
        ));
    }

    let (display_name, mut offset) = read_acc_string(packet, 2)?;
    if display_name != "OpenRacing" {
        return Err(format!(
            "unexpected display name: got '{display_name}', expected 'OpenRacing'"
        ));
    }

    let (_connection_password, next_offset) = read_acc_string(packet, offset)?;
    offset = next_offset;

    if offset + 4 > packet.len() {
        return Err("missing update interval".to_string());
    }
    let interval_ms = i32::from_le_bytes([
        packet[offset],
        packet[offset + 1],
        packet[offset + 2],
        packet[offset + 3],
    ]);
    if interval_ms <= 0 {
        return Err(format!("invalid update interval: {interval_ms}"));
    }
    offset += 4;

    let (_command_password, final_offset) = read_acc_string(packet, offset)?;
    if final_offset != packet.len() {
        return Err("unexpected trailing bytes in register packet".to_string());
    }

    Ok(())
}

fn build_registration_result_packet(
    connection_id: i32,
    success: bool,
    readonly: bool,
    error: &str,
) -> Result<Vec<u8>, String> {
    let mut packet = Vec::with_capacity(64);
    packet.push(MSG_REGISTRATION_RESULT);
    packet.extend_from_slice(&connection_id.to_le_bytes());
    packet.push(if success { 1 } else { 0 });
    packet.push(if readonly { 1 } else { 0 });
    write_acc_string(&mut packet, error)?;
    Ok(packet)
}

fn read_acc_string(packet: &[u8], offset: usize) -> Result<(String, usize), String> {
    if offset + 2 > packet.len() {
        return Err("missing string length".to_string());
    }

    let len = u16::from_le_bytes([packet[offset], packet[offset + 1]]) as usize;
    let start = offset + 2;
    let end = start + len;

    if end > packet.len() {
        return Err("string out of bounds".to_string());
    }

    let value = String::from_utf8(packet[start..end].to_vec())
        .map_err(|error| format!("invalid utf-8 string: {error}"))?;
    Ok((value, end))
}

fn write_acc_string(buffer: &mut Vec<u8>, value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    let length = u16::try_from(bytes.len()).map_err(|_| "string too long".to_string())?;
    buffer.extend_from_slice(&length.to_le_bytes());
    buffer.extend_from_slice(bytes);
    Ok(())
}
