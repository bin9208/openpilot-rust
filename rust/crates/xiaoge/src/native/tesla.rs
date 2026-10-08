use super::{platform, Error};
use openpilot_can::{dbc::Dbc, parser::Parser, Frame, Packet};
use openpilot_cereal::{car_capnp::car_params, log_capnp::event};
use openpilot_logging::Value;
use openpilot_msgq::Subscriber;
use openpilot_params::Params;
use std::{path::Path, sync::Arc, time::Duration};

#[derive(Default)]
pub struct Tesla {
    brand_checked: bool,
    parser: Option<Parser>,
    subscriber: Option<Subscriber>,
    updated: f64,
}

impl Tesla {
    pub fn collect(
        &mut self,
        params: &Params,
        root: &Path,
    ) -> Result<Option<[(&'static str, Value); 2]>, Error> {
        if !self.brand_checked {
            let bytes = match params.get("CarParams") {
                Ok(Some(bytes)) if !bytes.is_empty() => bytes,
                Ok(_) | Err(openpilot_params::Error::Io(_)) => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            let reader = capnp::serialize::read_message(bytes.as_slice(), Default::default())?;
            let car = reader.get_root::<car_params::Reader>()?;
            self.brand_checked = true;
            if car.get_brand()?.as_bytes() == b"tesla" {
                let dbc = Dbc::load(&root.join("opendbc_repo/opendbc/dbc/tesla_model3_party.dbc"))?;
                let mut parser = Parser::new(Arc::new(dbc), 2, platform::timestamp()?);
                parser.add("DAS_road", Some(f64::NAN), false, platform::timestamp()?)?;
                let capacity = openpilot_messaging::services::lookup("can")
                    .ok_or(Error::Contract("CAN service missing"))?
                    .queue_size;
                self.subscriber = Some(Subscriber::for_runtime("can", false, capacity)?);
                self.parser = Some(parser);
            }
        }
        let (Some(parser), Some(subscriber)) = (&mut self.parser, &mut self.subscriber) else {
            return Ok(None);
        };
        while let Some(bytes) = subscriber.receive(Duration::ZERO)? {
            let message = capnp::serialize::read_message(bytes.as_slice(), Default::default())?;
            let event = message.get_root::<event::Reader>()?;
            let event::Can(frames) = event.which().map_err(capnp::Error::from)? else {
                return Err(Error::Contract("unexpected CAN event"));
            };
            let mut selected = Vec::new();
            for frame in frames? {
                if frame.get_address() == 605 && frame.get_src() == 2 {
                    selected.push(Frame {
                        address: 605,
                        bus: 2,
                        data: frame.get_dat()?.to_vec(),
                    });
                }
            }
            if !selected.is_empty()
                && parser
                    .update(&[Packet {
                        mono_time: event.get_log_mono_time(),
                        frames: selected,
                    }])?
                    .contains(&605)
            {
                self.updated = platform::monotonic()?;
            }
        }
        if platform::monotonic()? - self.updated > 1.0 {
            return Ok(None);
        }
        let color = parser.signal("DAS_road", "DAS_trafficLightColor")?;
        let color = num_traits::ToPrimitive::to_i64(&color)
            .ok_or(Error::Contract("invalid Tesla traffic-light color"))?;
        Ok(Some([
            (
                "stopLineDist",
                Value::Float(parser.signal("DAS_road", "DAS_stopLineDist")?),
            ),
            ("trafficLightColor", Value::Integer(i128::from(color))),
        ]))
    }
}
