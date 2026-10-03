use crate::{binary::Cursor, framing::Framer, Error};
use openpilot_cereal::log_capnp::event;
use std::collections::BTreeMap;

pub struct Packet {
    pub service: &'static str,
    pub bytes: Vec<u8>,
}
pub struct GlonassString {
    pub bytes: Vec<u8>,
    pub superframe: u16,
    pub time: f64,
}
#[derive(Default)]
pub struct Parser {
    pub framer: Framer,
    pub(crate) gps: BTreeMap<u8, BTreeMap<u8, Vec<u8>>>,
    pub(crate) glonass: BTreeMap<u8, BTreeMap<u8, GlonassString>>,
}
impl Parser {
    pub fn parse_frame(
        &mut self,
        frame: &[u8],
        publication_ns: u64,
    ) -> Result<Option<Packet>, Error> {
        let header = frame.get(..6).ok_or(Error::Malformed("header"))?;
        let kind = u16::from_be_bytes([header[2], header[3]]);
        let payload = frame
            .get(6..frame.len().saturating_sub(2))
            .ok_or(Error::Malformed("payload"))?;
        let mut message = capnp::message::Builder::new_default();
        let mut root = message.init_root::<event::Builder<'_>>();
        root.set_valid(true);
        root.set_log_mono_time(publication_ns);
        let mut cursor = Cursor::new(payload);
        let service = match kind {
            0x0107 => {
                crate::reports::nav(&mut cursor, root.reborrow().init_gps_location_external())?;
                "gpsLocationExternal"
            }
            0x0215 => {
                crate::reports::raw(
                    &mut cursor,
                    root.reborrow().init_ublox_gnss().init_measurement_report(),
                )?;
                "ubloxGnss"
            }
            0x0135 => {
                crate::reports::sat(
                    &mut cursor,
                    root.reborrow().init_ublox_gnss().init_sat_report(),
                )?;
                "ubloxGnss"
            }
            0x0a09 => {
                crate::reports::hardware(
                    &mut cursor,
                    root.reborrow().init_ublox_gnss().init_hw_status(),
                )?;
                "ubloxGnss"
            }
            0x0a0b => {
                crate::reports::hardware2(
                    &mut cursor,
                    root.reborrow().init_ublox_gnss().init_hw_status2(),
                )?;
                "ubloxGnss"
            }
            0x0213 => {
                if payload.len() < 8 || payload.len() != 8 + 4 * usize::from(payload[4]) {
                    return Ok(None);
                }
                let gnss = payload[0];
                if gnss > 6 {
                    return Err(Error::Malformed("GNSS enum"));
                }
                let words: Vec<_> = payload[8..]
                    .chunks_exact(4)
                    .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                    .collect();
                let emitted = match gnss {
                    0 => crate::gps::ephemeris(
                        &mut self.gps,
                        payload[1],
                        &words,
                        root.reborrow().init_ublox_gnss(),
                    )?,
                    6 => crate::glonass::ephemeris(
                        &mut self.glonass,
                        (payload[1], payload[3], self.framer.last_log_time),
                        &words,
                        root.reborrow().init_ublox_gnss(),
                    )?,
                    1..=5 => false,
                    _ => return Err(Error::Malformed("GNSS enum")),
                };
                if !emitted {
                    return Ok(None);
                }
                "ubloxGnss"
            }
            _ => return Ok(None),
        };
        Ok(Some(Packet {
            service,
            bytes: capnp::serialize::write_message_to_words(&message),
        }))
    }
}
