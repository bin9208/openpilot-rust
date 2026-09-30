use openpilot_registration::{registration_expiration, system_time_unix_seconds};
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn pre_epoch_fraction_floors_before_signing() {
    assert_eq!(
        system_time_unix_seconds(UNIX_EPOCH - Duration::from_nanos(1)).unwrap(),
        -1
    );
    assert_eq!(
        system_time_unix_seconds(UNIX_EPOCH - Duration::new(1, 1)).unwrap(),
        -2
    );
    assert_eq!(registration_expiration(-1).unwrap(), 3599);
}

#[test]
fn calendar_and_one_hour_expiry_limits_match_python() {
    assert_eq!(
        system_time_unix_seconds(UNIX_EPOCH - Duration::from_secs(62_135_596_800)).unwrap(),
        -62_135_596_800
    );
    assert!(system_time_unix_seconds(UNIX_EPOCH - Duration::new(62_135_596_800, 1)).is_err());
    assert_eq!(
        system_time_unix_seconds(UNIX_EPOCH + Duration::new(253_402_300_799, 999_999_999)).unwrap(),
        253_402_300_799
    );
    assert!(system_time_unix_seconds(UNIX_EPOCH + Duration::from_secs(253_402_300_800)).is_err());
    assert_eq!(
        registration_expiration(253_402_297_199).unwrap(),
        253_402_300_799
    );
    assert!(registration_expiration(253_402_297_200).is_err());
    assert!(registration_expiration(i64::MAX).is_err());
}
