//! Property-based tests for telemetry disconnection handling
//!
//! Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
//! **Validates: Requirements 12.6**
//!
//! For any game disconnection event, the telemetry system SHALL transition to
//! disconnected state without crashing and notify the FFB engine.

use proptest::prelude::*;

use crate::telemetry::game_telemetry::{
    ConnectionState, ConnectionStateEvent, DisconnectionConfig, DisconnectionTracker,
};

/// Strategy for generating valid disconnection timeout values
/// Timeouts should be positive and reasonable (1ms to 10 seconds)
fn timeout_strategy() -> impl Strategy<Value = u64> {
    1u64..10_000
}

/// Strategy for generating valid reconnection configuration
fn disconnection_config_strategy() -> impl Strategy<Value = DisconnectionConfig> {
    (
        timeout_strategy(), // timeout_ms
        any::<bool>(),      // auto_reconnect
        0u32..10,           // max_reconnect_attempts (0 = unlimited)
        100u64..5000,       // reconnect_delay_ms
    )
        .prop_map(
            |(timeout_ms, auto_reconnect, max_reconnect_attempts, reconnect_delay_ms)| {
                DisconnectionConfig {
                    timeout_ms,
                    auto_reconnect,
                    max_reconnect_attempts,
                    reconnect_delay_ms,
                }
            },
        )
}

/// Strategy for generating game IDs
fn game_id_strategy() -> impl Strategy<Value = String> {
    (
        prop::char::range('a', 'z'),
        prop::collection::vec(
            prop_oneof![
                prop::char::range('a', 'z'),
                prop::char::range('0', '9'),
                Just('_')
            ],
            0..15,
        ),
    )
        .prop_map(|(first, rest)| {
            let mut id = String::with_capacity(1 + rest.len());
            id.push(first);
            id.extend(rest);
            id
        })
}

/// Strategy for generating connection states
fn connection_state_strategy() -> impl Strategy<Value = ConnectionState> {
    prop_oneof![
        Just(ConnectionState::Disconnected),
        Just(ConnectionState::Connecting),
        Just(ConnectionState::Connected),
        Just(ConnectionState::Reconnecting),
        Just(ConnectionState::Error),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    // **Validates: Requirements 12.6**
    //
    // For any timeout configuration, when telemetry stops arriving for longer
    // than the timeout, the tracker SHALL transition to Disconnected state.
    #[test]
    fn prop_disconnection_timeout_triggers_state_change(
        game_id in game_id_strategy(),
        timeout_ms in 10u64..100, // Use small timeouts for testing
    ) {
        let config = DisconnectionConfig::with_timeout(timeout_ms);
        let mut tracker = DisconnectionTracker::new(game_id, config);

        // Record initial data to become connected
        tracker.record_data_received();
        prop_assert_eq!(
            tracker.state(),
            ConnectionState::Connected,
            "Should be connected after receiving data"
        );

        // Wait for timeout to elapse
        std::thread::sleep(std::time::Duration::from_millis(timeout_ms + 20));

        // Check disconnection - should transition to Disconnected
        let state = tracker.check_disconnection();
        prop_assert_eq!(
            state,
            ConnectionState::Disconnected,
            "Should transition to Disconnected after timeout"
        );
    }

    // Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    // **Validates: Requirements 12.6**
    //
    // For any valid configuration, the DisconnectionTracker SHALL be created
    // without crashing and start in Disconnected state.
    #[test]
    fn prop_tracker_creation_never_crashes(
        game_id in game_id_strategy(),
        config in disconnection_config_strategy(),
    ) {
        // Creating a tracker should never crash
        let tracker = DisconnectionTracker::new(game_id.clone(), config);

        // Initial state should always be Disconnected
        prop_assert_eq!(
            tracker.state(),
            ConnectionState::Disconnected,
            "Initial state should be Disconnected"
        );

        // Reconnect attempts should be zero
        prop_assert_eq!(
            tracker.reconnect_attempts(),
            0,
            "Initial reconnect attempts should be 0"
        );
    }

    // Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    // **Validates: Requirements 12.6**
    //
    // For any sequence of record_data_received calls, the tracker SHALL
    // transition to Connected state and remain there while data keeps arriving.
    #[test]
    fn prop_data_received_maintains_connected_state(
        game_id in game_id_strategy(),
        num_data_events in 1usize..50,
    ) {
        let config = DisconnectionConfig::with_timeout(1000); // 1 second timeout
        let mut tracker = DisconnectionTracker::new(game_id, config);

        for i in 0..num_data_events {
            tracker.record_data_received();

            prop_assert_eq!(
                tracker.state(),
                ConnectionState::Connected,
                "Should remain Connected after data event {} of {}",
                i + 1,
                num_data_events
            );

            // Check disconnection should not change state while data is arriving
            let state = tracker.check_disconnection();
            prop_assert_eq!(
                state,
                ConnectionState::Connected,
                "check_disconnection should not change state while data is fresh"
            );
        }
    }

    // Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    // **Validates: Requirements 12.6**
    //
    // For any reconnection configuration, the should_reconnect method SHALL
    // correctly respect the auto_reconnect and max_reconnect_attempts settings.
    #[test]
    fn prop_reconnection_respects_configuration(
        game_id in game_id_strategy(),
        auto_reconnect in any::<bool>(),
        max_attempts in 1u32..10,
    ) {
        let config = DisconnectionConfig {
            timeout_ms: 100,
            auto_reconnect,
            max_reconnect_attempts: max_attempts,
            reconnect_delay_ms: 100,
        };
        let mut tracker = DisconnectionTracker::new(game_id, config);

        if !auto_reconnect {
            // When auto_reconnect is false, should never reconnect
            prop_assert!(
                !tracker.should_reconnect(),
                "should_reconnect should be false when auto_reconnect is disabled"
            );
        } else {
            // When auto_reconnect is true, should reconnect until max attempts
            prop_assert!(
                tracker.should_reconnect(),
                "should_reconnect should be true initially when auto_reconnect is enabled"
            );

            // Exhaust reconnection attempts
            for _ in 0..max_attempts {
                tracker.mark_reconnecting();
            }
            tracker.set_state(ConnectionState::Disconnected, None);

            prop_assert!(
                !tracker.should_reconnect(),
                "should_reconnect should be false after exhausting max_reconnect_attempts"
            );

            // Reset should allow reconnection again
            tracker.reset_reconnect_attempts();
            prop_assert!(
                tracker.should_reconnect(),
                "should_reconnect should be true after reset_reconnect_attempts"
            );
        }
    }

    // Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    // **Validates: Requirements 12.6**
    //
    // For any error condition, marking the tracker as error SHALL transition
    // to Error state without crashing.
    #[test]
    fn prop_error_state_transition_never_crashes(
        game_id in game_id_strategy(),
        error_reason in ".*",
    ) {
        let mut tracker = DisconnectionTracker::with_defaults(game_id);

        // Mark error should never crash
        tracker.mark_error(error_reason.clone());

        prop_assert_eq!(
            tracker.state(),
            ConnectionState::Error,
            "State should be Error after mark_error"
        );

        // Error state should be considered disconnected
        prop_assert!(
            tracker.state().is_disconnected(),
            "Error state should be considered disconnected"
        );
    }

    // Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    // **Validates: Requirements 12.6**
    //
    // For any ConnectionStateEvent, the is_disconnection and is_connection
    // methods SHALL correctly identify the event type.
    #[test]
    fn prop_event_type_identification_is_correct(
        game_id in game_id_strategy(),
        previous_state in connection_state_strategy(),
        new_state in connection_state_strategy(),
    ) {
        let event = ConnectionStateEvent::new(
            game_id,
            previous_state,
            new_state,
            None,
        );

        // is_disconnection: previous was connected, new is disconnected
        let expected_disconnection = previous_state.is_connected() && new_state.is_disconnected();
        prop_assert_eq!(
            event.is_disconnection(),
            expected_disconnection,
            "is_disconnection should be {} for {:?} -> {:?}",
            expected_disconnection,
            previous_state,
            new_state
        );

        // is_connection: previous was not connected, new is connected
        let expected_connection = !previous_state.is_connected() && new_state.is_connected();
        prop_assert_eq!(
            event.is_connection(),
            expected_connection,
            "is_connection should be {} for {:?} -> {:?}",
            expected_connection,
            previous_state,
            new_state
        );
    }

    // Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    // **Validates: Requirements 12.6**
    //
    // For any sequence of state transitions, the tracker SHALL maintain
    // consistent state and never crash.
    #[test]
    fn prop_state_transitions_are_consistent(
        game_id in game_id_strategy(),
        operations in prop::collection::vec(0u8..5, 1..30),
    ) {
        let mut tracker = DisconnectionTracker::with_defaults(game_id);

        for op in operations {
            match op {
                0 => {
                    // Record data received
                    tracker.record_data_received();
                    prop_assert_eq!(
                        tracker.state(),
                        ConnectionState::Connected,
                        "Should be Connected after record_data_received"
                    );
                }
                1 => {
                    // Mark connecting
                    tracker.mark_connecting();
                    prop_assert_eq!(
                        tracker.state(),
                        ConnectionState::Connecting,
                        "Should be Connecting after mark_connecting"
                    );
                }
                2 => {
                    // Mark reconnecting
                    let prev_attempts = tracker.reconnect_attempts();
                    tracker.mark_reconnecting();
                    prop_assert_eq!(
                        tracker.state(),
                        ConnectionState::Reconnecting,
                        "Should be Reconnecting after mark_reconnecting"
                    );
                    prop_assert_eq!(
                        tracker.reconnect_attempts(),
                        prev_attempts + 1,
                        "Reconnect attempts should increment"
                    );
                }
                3 => {
                    // Mark error
                    tracker.mark_error("test error".to_string());
                    prop_assert_eq!(
                        tracker.state(),
                        ConnectionState::Error,
                        "Should be Error after mark_error"
                    );
                }
                4 => {
                    // Set disconnected
                    tracker.set_state(ConnectionState::Disconnected, None);
                    prop_assert_eq!(
                        tracker.state(),
                        ConnectionState::Disconnected,
                        "Should be Disconnected after set_state"
                    );
                }
                _ => {}
            }
        }
    }

    // Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    // **Validates: Requirements 12.6**
    //
    // For any DisconnectionConfig, the timeout and reconnect_delay methods
    // SHALL return correct Duration values.
    #[test]
    fn prop_config_duration_methods_are_correct(
        config in disconnection_config_strategy(),
    ) {
        let expected_timeout = std::time::Duration::from_millis(config.timeout_ms);
        let expected_delay = std::time::Duration::from_millis(config.reconnect_delay_ms);

        prop_assert_eq!(
            config.timeout(),
            expected_timeout,
            "timeout() should return Duration from timeout_ms"
        );
        prop_assert_eq!(
            config.reconnect_delay(),
            expected_delay,
            "reconnect_delay() should return Duration from reconnect_delay_ms"
        );
    }
}

// ============================================================================
// Additional Unit Tests for Disconnection Handling Edge Cases
// ============================================================================

#[cfg(test)]
mod unit_tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Test that time_since_last_data returns None before any data
    #[test]
    fn test_time_since_last_data_initially_none() -> TestResult {
        let tracker = DisconnectionTracker::with_defaults("test_game");

        assert!(
            tracker.time_since_last_data().is_none(),
            "time_since_last_data should be None before any data received"
        );

        Ok(())
    }

    /// Test that time_since_last_data returns Some after data received
    #[test]
    fn test_time_since_last_data_after_data() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");

        tracker.record_data_received();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let elapsed = tracker.time_since_last_data();
        assert!(
            elapsed.is_some(),
            "time_since_last_data should be Some after data received"
        );
        assert!(
            elapsed.map(|d| d.as_millis() >= 10).unwrap_or(false),
            "Elapsed time should be at least 10ms"
        );

        Ok(())
    }

    /// Test that is_timed_out returns false before any data
    #[test]
    fn test_is_timed_out_false_before_data() -> TestResult {
        let config = DisconnectionConfig::with_timeout(10);
        let tracker = DisconnectionTracker::new("test_game", config);

        // Wait longer than timeout
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Should not be timed out because no data was ever received
        assert!(
            !tracker.is_timed_out(),
            "is_timed_out should be false when no data has ever been received"
        );

        Ok(())
    }

    /// Test unlimited reconnection attempts (max_reconnect_attempts = 0)
    #[test]
    fn test_unlimited_reconnection_attempts() -> TestResult {
        let config = DisconnectionConfig {
            timeout_ms: 100,
            auto_reconnect: true,
            max_reconnect_attempts: 0, // Unlimited
            reconnect_delay_ms: 100,
        };
        let mut tracker = DisconnectionTracker::new("test_game", config);

        // Should always allow reconnection
        for i in 0..100 {
            assert!(
                tracker.should_reconnect(),
                "should_reconnect should be true for attempt {}",
                i
            );
            tracker.mark_reconnecting();
            tracker.set_state(ConnectionState::Disconnected, None);
        }

        Ok(())
    }

    /// Test ConnectionState helper methods
    #[test]
    fn test_connection_state_helpers() -> TestResult {
        // is_connected
        assert!(ConnectionState::Connected.is_connected());
        assert!(!ConnectionState::Disconnected.is_connected());
        assert!(!ConnectionState::Connecting.is_connected());
        assert!(!ConnectionState::Reconnecting.is_connected());
        assert!(!ConnectionState::Error.is_connected());

        // is_disconnected
        assert!(ConnectionState::Disconnected.is_disconnected());
        assert!(ConnectionState::Error.is_disconnected());
        assert!(!ConnectionState::Connected.is_disconnected());
        assert!(!ConnectionState::Connecting.is_disconnected());
        assert!(!ConnectionState::Reconnecting.is_disconnected());

        // is_transitioning
        assert!(ConnectionState::Connecting.is_transitioning());
        assert!(ConnectionState::Reconnecting.is_transitioning());
        assert!(!ConnectionState::Connected.is_transitioning());
        assert!(!ConnectionState::Disconnected.is_transitioning());
        assert!(!ConnectionState::Error.is_transitioning());

        Ok(())
    }

    /// Test that events have valid timestamps
    #[test]
    fn test_event_timestamps_are_valid() -> TestResult {
        let event = ConnectionStateEvent::new(
            "test_game",
            ConnectionState::Disconnected,
            ConnectionState::Connected,
            Some("test".to_string()),
        );

        // Timestamp should be non-zero (after UNIX epoch)
        assert!(event.timestamp_ns > 0, "Event timestamp should be positive");

        // Timestamp should be reasonable (after year 2020)
        let year_2020_ns: u64 = 1_577_836_800_000_000_000; // 2020-01-01 00:00:00 UTC
        assert!(
            event.timestamp_ns > year_2020_ns,
            "Event timestamp should be after year 2020"
        );

        Ok(())
    }

    /// Test that duplicate state transitions don't emit duplicate events
    #[tokio::test]
    async fn test_no_duplicate_events_on_same_state() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        let mut receiver = tracker.subscribe();

        // First data received - should emit event
        tracker.record_data_received();

        let event = tokio::time::timeout(std::time::Duration::from_millis(100), receiver.recv())
            .await?
            .ok_or("Expected event")?;

        assert!(event.is_connection());

        // Second data received - should NOT emit event (already connected)
        tracker.record_data_received();

        // Try to receive - should timeout (no event)
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(50), receiver.recv()).await;

        assert!(result.is_err(), "Should not receive duplicate event");

        Ok(())
    }
}
