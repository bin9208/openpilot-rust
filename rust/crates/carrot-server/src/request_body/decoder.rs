use super::DecodeFailure;
use brotli_decompressor::{BrotliDecompressStream, BrotliResult, BrotliState, StandardAlloc};
use hyper::{header, HeaderMap};
use zlib_rs::{Inflate, InflateFlush, Status};

const FEED_LIMIT: usize = 32 * 1024 * 1024;
type Brotli = BrotliState<StandardAlloc, StandardAlloc, StandardAlloc>;

pub(crate) enum Decoder {
    Identity,
    Zlib {
        state: Inflate,
        label: String,
        first: bool,
        seen: bool,
        ended: bool,
    },
    Brotli {
        state: Box<Brotli>,
        ended: bool,
    },
}

fn invalid(label: &str) -> DecodeFailure {
    DecodeFailure::Payload {
        message: format!("400, message:\n  Can not decode content-encoding: {label}"),
    }
}

fn append(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), DecodeFailure> {
    if output.len().saturating_add(bytes.len()) > FEED_LIMIT {
        return Err(DecodeFailure::Payload {
            message: format!("400, message:\n  Decompressed data exceeds the configured limit of {FEED_LIMIT} bytes"),
        });
    }
    output.extend_from_slice(bytes);
    Ok(())
}

impl Decoder {
    pub(super) const fn is_identity(&self) -> bool {
        matches!(self, Self::Identity)
    }
    pub(crate) fn new(headers: &HeaderMap) -> Self {
        let Some(value) = headers.get(header::CONTENT_ENCODING) else {
            return Self::Identity;
        };
        if !value.as_bytes().is_ascii() {
            return Self::Identity;
        }
        let Ok(label) = std::str::from_utf8(value.as_bytes()) else {
            return Self::Identity;
        };
        if label == "br" {
            return Self::Brotli {
                state: Box::new(BrotliState::new(
                    StandardAlloc::default(),
                    StandardAlloc::default(),
                    StandardAlloc::default(),
                )),
                ended: false,
            };
        }
        if !["gzip", "deflate", "br"].contains(&label.to_ascii_lowercase().as_str()) {
            return Self::Identity;
        }
        Self::Zlib {
            state: Inflate::new(true, if label == "gzip" { 31 } else { 15 }),
            label: label.to_owned(),
            first: true,
            seen: false,
            ended: false,
        }
    }

    pub(crate) fn feed(&mut self, input: &[u8]) -> Result<Vec<u8>, DecodeFailure> {
        match self {
            Self::Identity => Ok(input.to_vec()),
            Self::Zlib {
                state,
                label,
                first,
                seen,
                ended,
            } => {
                if input.is_empty() || *ended {
                    return Ok(Vec::new());
                }
                *seen = true;
                if *first && label == "deflate" && input[0] & 0xf != 8 {
                    *state = Inflate::new(false, 15);
                }
                *first = false;
                let mut output = Vec::new();
                let mut offset = 0;
                loop {
                    let before_in = state.total_in();
                    let before_out = state.total_out();
                    let mut buffer = [0u8; 16384];
                    let result =
                        state.decompress(&input[offset..], &mut buffer, InflateFlush::NoFlush);
                    let consumed = usize::try_from(state.total_in() - before_in)
                        .map_err(|_| invalid(label))?;
                    let produced = usize::try_from(state.total_out() - before_out)
                        .map_err(|_| invalid(label))?;
                    append(&mut output, &buffer[..produced])?;
                    offset += consumed;
                    match result {
                        Ok(Status::StreamEnd) => {
                            *ended = true;
                            return Ok(output);
                        }
                        Ok(Status::Ok | Status::BufError) if consumed != 0 || produced != 0 => {}
                        Ok(Status::Ok | Status::BufError) => return Ok(output),
                        Err(_) => return Err(invalid(label)),
                    }
                }
            }
            Self::Brotli { state, ended } => {
                if input.is_empty() {
                    return Ok(Vec::new());
                }
                if *ended {
                    return Err(invalid("br"));
                }
                let mut available_in = input.len();
                let mut offset = 0;
                let mut total_out = 0;
                let mut output = Vec::new();
                loop {
                    let mut buffer = [0u8; 16384];
                    let mut available_out = buffer.len();
                    let mut output_offset = 0;
                    let result = BrotliDecompressStream(
                        &mut available_in,
                        &mut offset,
                        input,
                        &mut available_out,
                        &mut output_offset,
                        &mut buffer,
                        &mut total_out,
                        state,
                    );
                    append(&mut output, &buffer[..output_offset])?;
                    match result {
                        BrotliResult::ResultSuccess if offset == input.len() => {
                            *ended = true;
                            return Ok(output);
                        }
                        BrotliResult::NeedsMoreOutput => {}
                        BrotliResult::NeedsMoreInput => return Ok(output),
                        BrotliResult::ResultSuccess | BrotliResult::ResultFailure => {
                            return Err(invalid("br"));
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn finish(&self) -> Result<(), DecodeFailure> {
        if let Self::Zlib {
            label,
            seen: true,
            ended: false,
            ..
        } = self
        {
            if label == "deflate" {
                return Err(DecodeFailure::Parser {
                    message: "deflate".into(),
                });
            }
        }
        Ok(())
    }
}
