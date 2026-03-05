use racing_wheel_telemetry_connection::{
    ConnectionState, ConnectionStateEvent, DisconnectionConfig, DisconnectionTracker,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn state_helpers_match_contract() -> TestResult {
    assert!(ConnectionState::Connected.is_connected());
    assert!(ConnectionState::Disconnected.is_disconnected());
    assert!(ConnectionState::Error.is_disconnected());
    assert!(ConnectionState::Connecting.is_transitioning());
    assert!(ConnectionState::Reconnecting.is_transitioning());
    Ok(())
}

#[test]
fn event_semantics_connection_and_disconnection() -> TestResult {
    let connect_event = ConnectionStateEvent::new(
        "forza",
        ConnectionState::Disconnected,
        ConnectionState::Connected,
        None,
    );
    assert!(connect_event.is_connection());

    let disconnect_event = ConnectionStateEvent::new(
        "forza",
        ConnectionState::Connected,
        ConnectionState::Disconnected,
        None,
    );
    assert!(disconnect_event.is_disconnection());
    Ok(())
}

#[test]
fn tracker_transitions_and_reconnect_policy() -> TestResult {
    let config = DisconnectionConfig {
        timeout_ms: 1,
        auto_reconnect: true,
        max_reconnect_attempts: 1,
        reconnect_delay_ms: 1,
    };
    let mut tracker = DisconnectionTracker::new("acc", config);

    tracker.record_data_received();
    assert_eq!(tracker.state(), ConnectionState::Connected);

    std::thread::sleep(std::time::Duration::from_millis(2));
    assert_eq!(tracker.check_disconnection(), ConnectionState::Disconnected);
    assert!(tracker.should_reconnect());

    tracker.mark_reconnecting();
    assert_eq!(tracker.reconnect_attempts(), 1);
    assert!(!tracker.should_reconnect());
    Ok(())
}
