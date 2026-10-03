//! Minimal Linux spidev UAPI ownership; all SPI protocol and retry policy is Rust.
#[cfg(feature = "native-skip-miri")]
#[allow(unsafe_code)]
mod bridge;
#[cfg(feature = "native-skip-miri")]
mod native;
#[cfg(feature = "native-skip-miri")]
pub use native::NativeKernel;
#[cfg(feature = "native-skip-miri")]
mod firmware;
#[cfg(feature = "native-skip-miri")]
pub use firmware::FirmwareKernel;

#[cfg(feature = "native-skip-miri")]
pub fn set_scheduler(policy: i32, priority: i32) -> std::io::Result<()> {
    let result = bridge::ffi::set_scheduler(policy, priority);
    if result.result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(result.error_number))
    }
}

#[cfg(all(test, feature = "native-skip-miri"))]
mod scheduler_tests {
    #[test]
    fn invalid_policy_reports_kernel_errno_without_changing_policy() {
        assert_eq!(
            super::set_scheduler(-1, 0).unwrap_err().raw_os_error(),
            Some(22)
        );
    }
}
