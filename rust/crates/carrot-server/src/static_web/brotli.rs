#![allow(unsafe_code)]

use crate::Error;
use libloading::Library;

type MaxSize = unsafe extern "C" fn(usize) -> usize;
type Compress = unsafe extern "C" fn(i32, i32, i32, usize, *const u8, *mut usize, *mut u8) -> i32;
type Decompress = unsafe extern "C" fn(usize, *const u8, *mut usize, *mut u8) -> i32;

pub(crate) struct Brotli {
    _encoder: Library,
    _decoder: Library,
    maximum_size: MaxSize,
    compress: Compress,
    decompress: Decompress,
}

impl Brotli {
    pub fn load() -> Option<Self> {
        Self::load_libraries("libbrotlienc.so.1", "libbrotlidec.so.1")
    }

    fn load_libraries(encoder: &str, decoder: &str) -> Option<Self> {
        // SAFETY: these system Brotli libraries have the C ABI declared in
        // brotli/encode.h and brotli/decode.h and no Rust callbacks or state.
        let encoder = unsafe { Library::new(encoder) }.ok()?;
        // SAFETY: the decoder uses the same versioned system ABI as above.
        let decoder = unsafe { Library::new(decoder) }.ok()?;
        // SAFETY: symbol signature matches BrotliEncoderMaxCompressedSize;
        // the Library handles are retained in Self until all calls finish.
        let maximum_size =
            *unsafe { encoder.get::<MaxSize>(b"BrotliEncoderMaxCompressedSize\0") }.ok()?;
        // SAFETY: symbol signature matches BrotliEncoderCompress in encode.h.
        let compress = *unsafe { encoder.get::<Compress>(b"BrotliEncoderCompress\0") }.ok()?;
        // SAFETY: symbol signature matches BrotliDecoderDecompress in decode.h.
        let decompress =
            *unsafe { decoder.get::<Decompress>(b"BrotliDecoderDecompress\0") }.ok()?;
        Some(Self {
            _encoder: encoder,
            _decoder: decoder,
            maximum_size,
            compress,
            decompress,
        })
    }

    pub fn compress(&self, source: &[u8]) -> Result<Vec<u8>, Error> {
        // SAFETY: this function accepts every size_t and retains no pointers.
        let capacity = unsafe { (self.maximum_size)(source.len()) };
        if capacity == 0 {
            return Err(Error::Source("Brotli compressed size overflow".into()));
        }
        let mut output = vec![0; capacity];
        let mut size = output.len();
        // SAFETY: source is readable for source.len(), output is exclusively
        // writable for size bytes, the initialized size pointer is valid, and
        // quality11/window22/generic mode are permitted by the Brotli C ABI.
        let result = unsafe {
            (self.compress)(
                11,
                22,
                0,
                source.len(),
                source.as_ptr(),
                &mut size,
                output.as_mut_ptr(),
            )
        };
        if result != 1 || size > output.len() {
            return Err(Error::Source("Brotli compression failed".into()));
        }
        output.truncate(size);
        Ok(output)
    }

    pub fn matches(&self, encoded: &[u8], source: &[u8]) -> bool {
        let mut output = vec![0; source.len()];
        let mut size = output.len();
        // SAFETY: encoded is readable for encoded.len(), output is writable
        // for size bytes without aliasing, and the decoder honors that bound.
        // Both slices and the retained decoder Library live through the call.
        let result = unsafe {
            (self.decompress)(
                encoded.len(),
                encoded.as_ptr(),
                &mut size,
                output.as_mut_ptr(),
            )
        };
        result == 1 && size == source.len() && output == source
    }
}

#[cfg(test)]
mod tests {
    use super::Brotli;

    #[test]
    fn unavailable_codec_when_libraries_are_missing() {
        // Given: unavailable native dependency names.
        // When: the optional codec is loaded.
        let codec = Brotli::load_libraries("/missing/brotli-encoder", "/missing/brotli-decoder");
        // Then: callers receive the source's absent-codec path.
        assert!(codec.is_none());
    }

    #[test]
    fn roundtrip_when_native_codec_handles_empty_and_binary_inputs() {
        // Given: the system codec and different source buffer sizes.
        let codec = Brotli::load().expect("host Brotli dependency");
        let sources = [Vec::new(), vec![0], (0..=255).collect(), vec![42; 8192]];
        // When: each source is compressed with the source quality.
        let results: Vec<_> = sources
            .iter()
            .map(|source| codec.compress(source))
            .collect();
        // Then: real native decoding confirms the complete original bytes.
        for (source, encoded) in sources.iter().zip(results) {
            assert!(codec.matches(&encoded.expect("compression"), source));
        }
        assert!(!codec.matches(b"invalid stream", b"source"));
    }
}
