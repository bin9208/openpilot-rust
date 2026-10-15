//! Source terminal exception grammar. Mentions earlier in a traceback never qualify.
// Source 8864280c verifies identical latched +/-1 USB meanings for these two
// isolated Tinygrad runtimes only. Unknown runtimes remain numerical failures.
const VERIFIED_USB_RUNTIMES: [&str; 2] = [
    "90970e01f2ba70d553294c0f9d5c6fe1678827f750705cab679a7b19b7bcb0b2",
    "c7c58622652231359ecea1a4253b63e157d002242bf596ede1b853f941f937ac",
];

fn terminal(detail: &str) -> &str {
    detail.trim_end().rsplit('\n').next().unwrap_or("")
}
fn runtime_terminal(detail: &str) -> &str {
    terminal(detail)
        .strip_prefix("RuntimeError: ")
        .unwrap_or(terminal(detail))
}
fn decimal(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|v| v.is_ascii_digit())
}

#[must_use]
pub fn gpu_bootloader_timeout(detail: &str, phase: &str) -> bool {
    if !matches!(phase, "load" | "boot_validation") {
        return false;
    }
    let Some(tail) = terminal(detail).strip_prefix("TimeoutError: BL not ready. Timed out after ")
    else {
        return false;
    };
    let Some((ms, condition)) = tail.split_once(" ms, condition not met: ") else {
        return false;
    };
    let Some((actual, expected)) = condition.split_once(" != ") else {
        return false;
    };
    decimal(ms) && decimal(actual) && decimal(expected)
}

#[must_use]
pub fn gpu_worker_busy(detail: &str) -> bool {
    runtime_terminal(detail) == "previous eGPU worker is still exiting"
}

#[must_use]
pub fn gpu_transport_failure(detail: &str) -> bool {
    let lower = terminal(detail).to_ascii_lowercase();
    let line = lower.strip_prefix("runtimeerror: ").unwrap_or(&lower);
    let Some((operation, error)) = line.split_once(": ") else {
        return false;
    };
    let operation_matches = matches!(
        operation,
        "libusb_control_transfer"
            | "libusb_bulk_transfer"
            | "libusb_interrupt_transfer"
            | "libusb_open"
            | "libusb_claim_interface"
    ) || operation
        .strip_prefix("bulk out 0x")
        .or_else(|| operation.strip_prefix("bulk in 0x"))
        .and_then(|tail| tail.strip_suffix(" failed"))
        .is_some_and(|address| {
            !address.is_empty() && address.bytes().all(|v| v.is_ascii_hexdigit())
        });
    operation_matches
        && matches!(
            error,
            "no such device (it may have been disconnected)"
                | "input/output error"
                | "operation timed out"
                | "pipe error"
                | "resource busy"
                | "system call interrupted"
                | "system call interrupted (perhaps due to signal)"
        )
}

#[must_use]
pub fn gpu_usb_link_failure(detail: &str, runtime_sha256: &str) -> bool {
    if !VERIFIED_USB_RUNTIMES.contains(&runtime_sha256) {
        return false;
    }
    let Some((device, code)) = runtime_terminal(detail).split_once(" failed with ") else {
        return false;
    };
    matches!(code, "-1" | "1")
        && (device == "AMD" || device.strip_prefix("AMD:").is_some_and(decimal))
}
