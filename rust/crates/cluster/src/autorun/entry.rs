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
