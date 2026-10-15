use super::{Encoder, Output, RunRequest};
use crate::Error;

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("H264 pipeline initialization failed: {0}")]
    H264Initialization(#[source] Error),
    #[error(transparent)]
    Runtime(#[from] Error),
}

pub trait Entrypoint {
    /// # Errors
    /// Distinguishes an H264 initialization failure from other entrypoint failures.
    fn run(&mut self, request: RunRequest) -> Result<(), Failure>;
}

#[must_use]
pub const fn encoder_name(encoder: Encoder) -> &'static str {
    match encoder {
        Encoder::Auto => "auto",
        Encoder::Jpeg => "jpeg",
        Encoder::Hardware => "hardware",
        Encoder::Software => "software",
    }
}

/// # Errors
/// Only automatic USB H264 initialization advances the source encoder sequence.
pub fn run_once(
    entrypoint: &mut impl Entrypoint,
    hud_mode: i64,
    configured_encoder: Encoder,
    output: Output,
    gpu_active: bool,
    board: bool,
) -> Result<(), Failure> {
    let sequence = if output.usb() {
        configured_encoder.sequence(board)
    } else {
        std::slice::from_ref(&configured_encoder)
    };
    for (index, &active_encoder) in sequence.iter().enumerate() {
        if output.usb() {
            println!(
                "[cluster_autorun] starting HUD encoder {} (setting={}:{})",
                encoder_name(active_encoder),
                configured_encoder.setting(),
                encoder_name(configured_encoder),
            );
        } else {
            println!("[cluster_autorun] starting HUD window fallback");
        }
        let result = entrypoint.run(RunRequest {
            hud_mode,
            configured_encoder,
            active_encoder,
            output,
            usbgpu_active: gpu_active,
        });
        match result {
            Ok(()) => return Ok(()),
            Err(Failure::H264Initialization(error))
                if output.usb()
                    && matches!(configured_encoder, Encoder::Auto)
                    && index + 1 < sequence.len() =>
            {
                println!(
                    "[cluster_autorun] HUD encoder {} failed; falling back to {}",
                    encoder_name(active_encoder),
                    encoder_name(sequence[index + 1]),
                );
                eprintln!("{error}");
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[must_use]
pub fn disconnected(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    let mut seen = Vec::new();
    while let Some(error) = current {
        if seen.iter().any(|&previous| std::ptr::eq(previous, error)) {
            break;
        }
        seen.push(error);
        let message = error.to_string().to_lowercase();
        if [
            "usb display disconnected",
            "no such device",
            "device has been disconnected",
        ]
        .iter()
        .any(|marker| message.contains(marker))
        {
            return true;
        }
        current = error.source();
    }
    false
}

/// Shorten only transport errors under the two source S3 failure wrappers.
#[must_use]
pub fn s3_transport_failure_reason(error: &(dyn std::error::Error + 'static)) -> Option<String> {
    use std::error::Error as _;
    let mut current = Some(error);
    let mut seen = Vec::new();
    let mut s3 = false;
    let mut transport = None;
    while let Some(error) = current {
        if seen.iter().any(|&previous| std::ptr::eq(previous, error)) {
            break;
        }
        seen.push(error);
        s3 |= matches!(
            error.to_string().as_str(),
            "S3 display disconnected; reconnect required"
                | "S3 display connection failed; waiting for rediscovery"
        );
        let io = if let Some(io) = error.downcast_ref::<std::io::Error>() {
            Some(io)
        } else if let Some(crate::Error::Io(io)) = error.downcast_ref::<crate::Error>() {
            Some(io)
        } else if let Some(crate::runtime::Error::Io(io)) =
            error.downcast_ref::<crate::runtime::Error>()
        {
            Some(io)
        } else if let Some(openpilot_cluster_wifi::Error::Io(io)) =
            error.downcast_ref::<openpilot_cluster_wifi::Error>()
        {
            Some(io)
        } else {
            None
        };
        if let Some(io) = io.filter(|io| {
            !io.get_ref().is_some_and(|cause| {
                cause.is::<crate::Error>()
                    || cause.is::<crate::runtime::Error>()
                    || cause.is::<openpilot_cluster_wifi::Error>()
                    || cause.is::<openpilot_cluster_wifi::TransactionError>()
            })
        }) {
            transport = Some(io);
        }
        current = if let Some(error) = error.downcast_ref::<crate::Error>() {
            match error {
                crate::Error::DisplayIo { cause, .. } | crate::Error::Disconnected(cause) => {
                    Some(cause.as_ref())
                }
                crate::Error::Io(io) => Some(io as &(dyn std::error::Error + 'static)),
                _ => error.source(),
            }
        } else if let Some(error) = error.downcast_ref::<crate::runtime::Error>() {
            match error {
                crate::runtime::Error::Cluster(cause) => {
                    Some(cause as &(dyn std::error::Error + 'static))
                }
                crate::runtime::Error::Io(io) => Some(io as &(dyn std::error::Error + 'static)),
                _ => error.source(),
            }
        } else if let Some(openpilot_cluster_wifi::Error::Io(io)) =
            error.downcast_ref::<openpilot_cluster_wifi::Error>()
        {
            Some(io as &(dyn std::error::Error + 'static))
        } else if let Some(io) = error.downcast_ref::<std::io::Error>() {
            io.get_ref()
                .map(|cause| cause as &(dyn std::error::Error + 'static))
        } else {
            error.source()
        };
    }
    let transport = transport.filter(|_| s3)?;
    let name = match transport.kind() {
        std::io::ErrorKind::TimedOut => "TimeoutError",
        std::io::ErrorKind::WouldBlock => "BlockingIOError",
        std::io::ErrorKind::Interrupted => "InterruptedError",
        std::io::ErrorKind::ConnectionReset => "ConnectionResetError",
        std::io::ErrorKind::ConnectionAborted => "ConnectionAbortedError",
        std::io::ErrorKind::ConnectionRefused => "ConnectionRefusedError",
        std::io::ErrorKind::BrokenPipe => "BrokenPipeError",
        std::io::ErrorKind::NotFound => "FileNotFoundError",
        std::io::ErrorKind::PermissionDenied => "PermissionError",
        _ => "OSError",
    };
    let detail = transport
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!(
        "{name}: {}",
        detail.chars().take(160).collect::<String>()
    ))
}

#[cfg(test)]
mod s3_retry_tests {
    use super::*;
    #[test]
    fn only_known_s3_wrapper_with_transport_error_gets_short_retry_reason() {
        let io = || std::io::Error::new(std::io::ErrorKind::ConnectionReset, "  peer\n reset  ");
        assert_eq!(s3_transport_failure_reason(&io()), None);
        let wrapped = crate::Error::DisplayIo {
            message: "S3 display connection failed; waiting for rediscovery",
            cause: Box::new(crate::Error::Io(io())),
        };
        assert_eq!(
            s3_transport_failure_reason(&wrapped).as_deref(),
            Some("ConnectionResetError: peer reset")
        );
        let nested = crate::runtime::Error::Io(std::io::Error::other(wrapped));
        assert_eq!(
            s3_transport_failure_reason(&nested).as_deref(),
            Some("ConnectionResetError: peer reset")
        );
        let protocol = crate::Error::DisplayIo {
            message: "S3 display connection failed; waiting for rediscovery",
            cause: Box::new(crate::Error::Contract("bad identity")),
        };
        assert_eq!(s3_transport_failure_reason(&protocol), None);
        let protocol_in_io = crate::Error::DisplayIo {
            message: "S3 display connection failed; waiting for rediscovery",
            cause: Box::new(crate::Error::Io(std::io::Error::other(
                openpilot_cluster_wifi::Error::Authentication,
            ))),
        };
        assert_eq!(s3_transport_failure_reason(&protocol_in_io), None);
    }
}
