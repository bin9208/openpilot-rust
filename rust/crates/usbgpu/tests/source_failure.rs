//! Terminal exception vectors evaluated by the pinned original source functions.
use openpilot_usbgpu::model_delivery::failure;

#[test]
fn terminal_gpu_failures_match_pinned_source() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/source_failure_vectors.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let detail = case["detail"].as_str().unwrap();
        for phase in ["load", "boot_validation", "inference"] {
            let key = if phase == "inference" {
                "bootloader_inference"
            } else {
                "bootloader_load"
            };
            assert_eq!(
                failure::gpu_bootloader_timeout(detail, phase),
                case[key].as_bool().unwrap(),
                "{phase}: {case}"
            );
        }
        assert_eq!(
            failure::gpu_worker_busy(detail),
            case["busy"].as_bool().unwrap(),
            "{case}"
        );
        assert_eq!(
            failure::gpu_transport_failure(detail),
            case["transport"].as_bool().unwrap(),
            "{case}"
        );
        for (runtime, key) in [
            (
                "90970e01f2ba70d553294c0f9d5c6fe1678827f750705cab679a7b19b7bcb0b2",
                "usb_link_pinned",
            ),
            ("unverified", "usb_link_unknown"),
        ] {
            assert_eq!(
                failure::gpu_usb_link_failure(detail, runtime),
                case[key].as_bool().unwrap(),
                "{case}"
            );
        }
    }
}

#[test]
fn native_worker_keeps_typed_bootloader_and_output_faults_classifiable_after_ipc() {
    use openpilot_usbgpu::{worker, Error};
    let error = Error::Timeout {
        operation: "BL not ready".into(),
        milliseconds: 2000,
        last: Some(0),
        expected: 2147483648,
    };
    let detail = worker::diagnostic(&error);
    assert!(failure::gpu_bootloader_timeout(&detail, "load"));
    assert!(!failure::gpu_bootloader_timeout(&detail, "inference"));
    let other = Error::Timeout {
        operation: "model memory".into(),
        milliseconds: 2000,
        last: Some(0),
        expected: 1,
    };
    assert!(!failure::gpu_bootloader_timeout(
        &worker::diagnostic(&other),
        "load"
    ));
    assert_eq!(
        worker::diagnostic(&Error::Contract("invalid precompiled model output")),
        "ValueError: invalid precompiled model output"
    );
}

#[test]
fn native_hcq_transport_and_short_transfer_keep_source_failure_identity() {
    use openpilot_usbgpu::{worker, Error};
    for operation in [
        "HCQ USB control transfer failed",
        "HCQ USB bulk transfer failed",
    ] {
        let error = Error::UsbApi {
            operation,
            code: -1,
            message: "Input/Output Error".into(),
        };
        assert!(failure::gpu_transport_failure(&worker::diagnostic(&error)));
    }
    let short = Error::UsbShortTransfer {
        operation: "HCQ USB bulk short transfer",
        actual: 4,
        expected: 8,
    };
    let detail = worker::diagnostic(&short);
    assert!(failure::gpu_usb_link_failure(
        &detail,
        "90970e01f2ba70d553294c0f9d5c6fe1678827f750705cab679a7b19b7bcb0b2"
    ));
    assert!(!failure::gpu_usb_link_failure(&detail, "unverified"));
    let other = Error::UsbApi {
        operation: "model file",
        code: -1,
        message: "Input/Output Error".into(),
    };
    assert!(!failure::gpu_transport_failure(&worker::diagnostic(&other)));
}

#[test]
fn md_v3_adds_only_exact_verified_runtime_to_latched_usb_failure_admission() {
    // Source 8864280c test_precompiled_model.py: both runtimes, same terminal
    // +/-1 grammar; unknown hashes and unrelated/ambiguous errors stay blocked.
    let runtimes = [
        "90970e01f2ba70d553294c0f9d5c6fe1678827f750705cab679a7b19b7bcb0b2",
        "c7c58622652231359ecea1a4253b63e157d002242bf596ede1b853f941f937ac",
    ];
    for runtime in runtimes {
        for error in [
            "AMD failed with -1",
            "RuntimeError: AMD failed with -1",
            "RuntimeError: AMD:0 failed with -1",
            "RuntimeError: AMD failed with 1",
            "RuntimeError: AMD:0 failed with 1",
            "traceback\nRuntimeError: AMD:17 failed with -1\n",
        ] {
            assert!(
                failure::gpu_usb_link_failure(error, runtime),
                "{runtime}: {error}"
            );
        }
        for error in [
            "RuntimeError: AMD failed with 2",
            "ValueError: AMD failed with -1",
            "RuntimeError: AMD failed with -10",
            "RuntimeError: AMD failed with -1\nValueError: checksum mismatch",
            "RuntimeError: quoted AMD failed with -1",
            "RuntimeError: NV failed with -1",
            "RuntimeError: AMD failed with +1",
        ] {
            assert!(
                !failure::gpu_usb_link_failure(error, runtime),
                "{runtime}: {error}"
            );
        }
    }
    for runtime in [
        "",
        "unverified",
        "c7c58622652231359ecea1a4253b63e157d002242bf596ede1b853f941f937ab",
    ] {
        assert!(!failure::gpu_usb_link_failure(
            "RuntimeError: AMD:0 failed with -1",
            runtime
        ));
    }
}
