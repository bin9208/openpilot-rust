use super::error;
use crate::Error;
use brotli_decompressor::{BrotliDecompressStream, BrotliResult, BrotliState, StandardAlloc};
use zlib_rs::{Inflate, InflateError, InflateFlush, Status};

pub(crate) fn brotli(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut state = BrotliState::new(
        StandardAlloc::default(),
        StandardAlloc::default(),
        StandardAlloc::default(),
    );
    let mut available_in = input.len();
    let mut offset = 0;
    let mut total_out = 0;
    let mut output = Vec::new();
    loop {
        let mut chunk = [0_u8; 16384];
        let mut available_out = chunk.len();
        let mut output_offset = 0;
        let result = BrotliDecompressStream(
            &mut available_in,
            &mut offset,
            input,
            &mut available_out,
            &mut output_offset,
            &mut chunk,
            &mut total_out,
            &mut state,
        );
        output.extend_from_slice(&chunk[..output_offset]);
        match result {
            BrotliResult::ResultSuccess if offset == input.len() => return Ok(output),
            BrotliResult::NeedsMoreOutput => continue,
            BrotliResult::ResultSuccess
            | BrotliResult::NeedsMoreInput
            | BrotliResult::ResultFailure => {
                return Err(error("brotli: decoder failed"));
            }
        }
    }
}

pub(crate) fn zlib(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut state = Inflate::new(true, 15);
    let mut output = Vec::new();
    loop {
        let before_in = state.total_in();
        let before_out = state.total_out();
        let mut chunk = [0_u8; 16384];
        let result = state.decompress(
            &input[before_in as usize..],
            &mut chunk,
            InflateFlush::NoFlush,
        );
        let produced = (state.total_out() - before_out) as usize;
        output.extend_from_slice(&chunk[..produced]);
        match result {
            Ok(Status::StreamEnd) => return Ok(output),
            Ok(Status::Ok | Status::BufError) if state.total_in() != before_in || produced != 0 => {
                continue;
            }
            Ok(Status::Ok | Status::BufError) => {
                return Err(error(
                    "Error -5 while decompressing data: incomplete or truncated stream",
                ));
            }
            Err(failure) => {
                let code = match failure {
                    InflateError::NeedDict { .. } => 2,
                    InflateError::StreamError => -2,
                    InflateError::DataError => -3,
                    InflateError::MemError => -4,
                };
                let suffix = state
                    .error_message()
                    .map(|message| format!(": {message}"))
                    .unwrap_or_default();
                return Err(error(format!(
                    "Error {code} while decompressing data{suffix}"
                )));
            }
        }
    }
}
