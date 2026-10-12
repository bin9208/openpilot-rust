use std::sync::Arc;

pub(crate) struct DecodedFrame {
    pub width: i64,
    pub height: i64,
    pub planes: Option<[Arc<[u8]>; 3]>,
    pub strides: Option<[i64; 3]>,
    pub hardware: Option<Arc<super::HardwareFrame>>,
    pub sequence: Option<u64>,
}
pub(crate) trait Decoder {
    fn decode(
        &mut self,
        packet: &[u8],
        sequence: u64,
        width: i64,
        height: i64,
    ) -> Result<Option<DecodedFrame>, String>;
    fn reset(&mut self) -> Result<(), String>;
}
pub(crate) type Factory = Arc<dyn Fn() -> Result<Box<dyn Decoder>, String> + Send + Sync>;

#[cfg(feature = "native-media")]
pub(crate) fn factory() -> Factory {
    Arc::new(|| Ok(Box::new(Native::default())))
}
#[cfg(not(feature = "native-media"))]
pub(crate) fn factory() -> Factory {
    Arc::new(|| Err("cluster H264 decoder requires the native-media FFmpeg provider".into()))
}
#[cfg(feature = "native-media")]
struct Native {
    hardware: Option<super::vidc::Decoder>,
    software: Option<openpilot_encoderd::native::decoder::SoftwareDecoder>,
    hardware_allowed: bool,
    hardware_started: bool,
    software_started: bool,
}
#[cfg(feature = "native-media")]
impl Default for Native {
    fn default() -> Self {
        Self {
            hardware: None,
            software: None,
            hardware_allowed: true,
            hardware_started: false,
            software_started: false,
        }
    }
}
#[cfg(feature = "native-media")]
fn hardware_config(width: i64, height: i64) -> Result<super::vidc::Config, String> {
    let text = std::env::var("CLUSTER_H264_DECODE_TIMEOUT_MS").unwrap_or_else(|_| "250".into());
    let timeout = decode_timeout(&text)?;
    Ok(super::vidc::Config {
        width: i32::try_from(width).map_err(|error| error.to_string())?,
        height: i32::try_from(height).map_err(|error| error.to_string())?,
        device_path: std::env::var("CLUSTER_H264_DECODER_DEVICE")
            .unwrap_or_else(|_| super::vidc::DEFAULT_DEVICE.into()),
        timeout,
        debug: std::env::var("CLUSTER_H264_DECODER_DEBUG").as_deref() == Ok("1"),
        ..super::vidc::Config::default()
    })
}
#[cfg(feature = "native-media")]
fn decode_timeout(text: &str) -> Result<std::time::Duration, String> {
    let text = openpilot_ui_application::params::typed::integer_text(text)
        .ok_or_else(|| "invalid CLUSTER_H264_DECODE_TIMEOUT_MS integer".to_owned())?;
    let millis = if text.starts_with('-') || text == "0" {
        1
    } else {
        let bits = text
            .trim_start_matches('+')
            .bytes()
            .fold(0_u32, |value, byte| {
                value.wrapping_mul(10).wrapping_add(u32::from(byte - b'0'))
            });
        i32::from_ne_bytes(bits.to_ne_bytes()).max(0)
    };
    Ok(std::time::Duration::from_millis(
        u64::try_from(millis).map_err(|error| error.to_string())?,
    ))
}
#[cfg(feature = "native-media")]
impl Decoder for Native {
    fn decode(
        &mut self,
        packet: &[u8],
        sequence: u64,
        width: i64,
        height: i64,
    ) -> Result<Option<DecodedFrame>, String> {
        if self.hardware_allowed && width > 0 && height > 0 {
            if std::env::var("CLUSTER_HARDWARE_H264_DECODE").as_deref() == Ok("0") {
                self.hardware_allowed = false;
            } else {
                let hardware = (|| {
                    if self.hardware.is_none() {
                        let mut decoder =
                            super::vidc::Decoder::new(hardware_config(width, height)?)
                                .map_err(|error| error.to_string())?;
                        decoder.open().map_err(|error| error.to_string())?;
                        self.hardware = Some(decoder);
                    }
                    let decoder = self
                        .hardware
                        .as_mut()
                        .ok_or_else(|| "native VIDC decoder missing".to_owned())?;
                    if !self.hardware_started {
                        eprintln!(
                            "Navi H264 decoder: Qualcomm VIDC NV12 DMA-BUF tid={}",
                            rustix::thread::gettid().as_raw_nonzero()
                        );
                        self.hardware_started = true;
                    }
                    if packet.is_empty() {
                        return Ok(None);
                    }
                    decoder.decode(packet, sequence).map_err(|error| {
                        if decoder.disabled_reason().is_empty() {
                            error.to_string()
                        } else {
                            decoder.disabled_reason().into()
                        }
                    })
                })();
                match hardware {
                    Ok(frame) => {
                        return Ok(frame.map(|frame| DecodedFrame {
                            width: i64::from(frame.layout.width),
                            height: i64::from(frame.layout.height),
                            sequence: Some(frame.sequence()),
                            hardware: Some(frame),
                            planes: None,
                            strides: None,
                        }));
                    }
                    Err(error) => {
                        self.hardware = None;
                        self.hardware_allowed = false;
                        eprintln!("Qualcomm VIDC H264 decode disabled; using FFmpeg: {error}");
                    }
                }
            }
        }
        if self.software.is_none() {
            self.software = Some(
                openpilot_encoderd::native::decoder::SoftwareDecoder::new()
                    .map_err(|error| error.to_string())?,
            );
            if !self.software_started {
                eprintln!(
                    "Navi H264 decoder: FFmpeg software tid={}",
                    rustix::thread::gettid().as_raw_nonzero()
                );
                self.software_started = true;
            }
        }
        let frame = self
            .software
            .as_mut()
            .ok_or_else(|| "native FFmpeg decoder missing".to_owned())?
            .decode(packet)
            .map_err(|error| error.to_string())?;
        Ok(frame.map(|frame| DecodedFrame {
            width: i64::from(frame.width),
            height: i64::from(frame.height),
            planes: Some(frame.planes.map(Into::into)),
            strides: Some(frame.strides.map(i64::from)),
            hardware: None,
            sequence: None,
        }))
    }
    fn reset(&mut self) -> Result<(), String> {
        self.hardware = None;
        if let Some(software) = &mut self.software {
            software.reset().map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "native-media"))]
mod tests {
    use super::decode_timeout;
    #[test]
    fn timeout_preserves_python_integer_minimum_and_c_int_wrapping() {
        for (text, millis) in [
            ("250", 250),
            ("٠٢٥٠", 250),
            ("+2_50", 250),
            ("0", 1),
            ("-5", 1),
            ("2147483647", 2_147_483_647),
            ("2147483648", 0),
            ("4294967296", 0),
            ("4294967297", 1),
            ("18446744073709551615", 0),
        ] {
            assert_eq!(decode_timeout(text).unwrap().as_millis(), millis, "{text}");
        }
        assert!(decode_timeout("2.5").is_err());
    }
}
