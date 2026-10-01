use crate::{dr, framing, measurement, poly, position, reader::Reader, reports::Position, Error};
use openpilot_cereal::log_capnp::event;
pub struct Publication {
    pub topic: &'static str,
    pub bytes: Vec<u8>,
    pub has_fix: bool,
}
pub struct Log<'a> {
    pub pending: u8,
    pub kind: u16,
    pub timestamp: u64,
    pub payload: &'a [u8],
}
impl<'a> Log<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let mut reader = Reader::new(bytes);
        let pending = reader.u8()?;
        let outer = reader.u16()?;
        let inner = reader.u16()?;
        let kind = reader.u16()?;
        let timestamp = reader.u64()?;
        if usize::from(outer) != bytes.len() - 3 || usize::from(inner) != bytes.len() - 3 {
            return Err(Error::Protocol("diagnostic log extent"));
        }
        Ok(Self {
            pending,
            kind,
            timestamp,
            payload: &bytes[15..],
        })
    }
    pub fn publication(&self, monotonic_ns: u64) -> Result<Option<Publication>, Error> {
        if !framing::LOG_TYPES.contains(&self.kind) {
            return Ok(None);
        }
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<event::Builder>();
        event.set_log_mono_time(monotonic_ns);
        event.set_valid(true);
        let (topic, has_fix) = if self.kind == 0x1476 {
            let data = Position::decode(self.payload)?;
            if data.u_pos_source != 2 || data.w_gps_week_number == u16::MAX {
                return Ok(None);
            }
            (
                "gpsLocation",
                position::fill(event.init_gps_location(), &data)?,
            )
        } else {
            let mut gnss = event.init_qcom_gnss();
            gnss.set_log_ts(self.timestamp);
            match self.kind {
                0x1477 => measurement::fill(gnss.init_measurement_report(), self.payload, true)?,
                0x1480 => measurement::fill(gnss.init_measurement_report(), self.payload, false)?,
                0x14de => dr::fill(gnss.init_dr_measurement_report(), self.payload)?,
                0x14e1 => poly::fill(gnss.init_dr_sv_poly(), self.payload)?,
                _ => return Err(Error::Protocol("unhandled selected log type")),
            }
            ("qcomGnss", false)
        };
        Ok(Some(Publication {
            topic,
            has_fix,
            bytes: capnp::serialize::write_message_to_words(&message),
        }))
    }
}
