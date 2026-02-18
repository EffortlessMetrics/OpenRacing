use racing_wheel_service::telemetry::{ACCAdapter, TelemetryAdapter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const FIXTURE_REGISTRATION_RESULT_SUCCESS: &[u8] =
    include_bytes!("fixtures/acc/registration_result_success.bin");
const FIXTURE_REALTIME_CAR_UPDATE_CAR_7: &[u8] =
    include_bytes!("fixtures/acc/realtime_car_update_car_7.bin");

#[test]
fn test_acc_normalize_realtime_car_update_fixture() -> TestResult {
    let adapter = ACCAdapter::new();
    let normalized = adapter.normalize(FIXTURE_REALTIME_CAR_UPDATE_CAR_7)?;

    assert_eq!(normalized.car_id.as_deref(), Some("car_7"));
    assert_eq!(normalized.speed_ms, Some(50.0));
    assert_eq!(normalized.gear, Some(4));
    Ok(())
}

#[test]
fn test_acc_normalize_registration_result_fixture_is_rejected() {
    let adapter = ACCAdapter::new();
    let result = adapter.normalize(FIXTURE_REGISTRATION_RESULT_SUCCESS);
    assert!(
        result.is_err(),
        "registration result should not normalize into realtime telemetry"
    );
}
