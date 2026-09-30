use crate::{
    clock::{message_time, Ratekeeper},
    sensor::{Event, Kind, Sensor},
    Bus, Clock, Error,
};
pub trait Sink {
    fn send(&mut self, kind: Kind, event: &Event, log_time: u64) -> Result<(), Error>;
    fn log(&mut self, level: &str, text: &str, error: Option<&Error>) -> Result<(), Error>;
}
pub enum Poll {
    Timeout,
    Other,
    Data(Vec<u8>),
}
pub struct Interrupt {
    offset: i128,
}
impl Interrupt {
    pub fn new(clock: &mut impl Clock) -> Self {
        Self {
            offset: clock.realtime_ns() - clock.monotonic_ns(),
        }
    }
    pub fn step<B: Bus>(
        &mut self,
        poll: Poll,
        sensors: &mut [&mut Sensor<B>],
        clock: &mut impl Clock,
        sink: &mut impl Sink,
    ) -> Result<(), Error> {
        let bytes = match poll {
            Poll::Timeout => return sink.log("error", "poll timed out", None),
            Poll::Other => return sink.log("error", "no poll events set", None),
            Poll::Data(bytes) => bytes,
        };
        if bytes.len() < 16 {
            return Err(Error::Contract("short GPIO event"));
        }
        let timestamp = u64::from_ne_bytes(
            bytes[..8]
                .try_into()
                .map_err(|_| Error::Contract("short GPIO event"))?,
        );
        let current = clock.realtime_ns() - clock.monotonic_ns();
        if (current - self.offset).abs() > 10_000_000 {
            sink.log(
                "warning",
                &format!("time jumped: {current} {}", self.offset),
                None,
            )?;
            self.offset = current;
            return Ok(());
        }
        let timestamp = i128::from(timestamp) - current;
        for sensor in sensors {
            if !sensor.kind.interrupt() {
                continue;
            }
            let result = (|| {
                let event = sensor.get_event(clock, Some(timestamp))?;
                if sensor.valid(clock) {
                    sink.send(sensor.kind, &event, message_time(clock)?)?;
                }
                Ok(())
            })();
            match result {
                Ok(()) | Err(Error::DataNotReady) => {}
                Err(error) => sink.log(
                    "exception",
                    &format!("Error processing {}", sensor.kind.service()),
                    Some(&error),
                )?,
            }
        }
        Ok(())
    }
}
pub fn polling_step<B: Bus>(
    sensor: &mut Sensor<B>,
    clock: &mut impl Clock,
    sink: &mut impl Sink,
    rate: &mut Ratekeeper,
) -> Result<(), Error> {
    let result = (|| {
        let event = sensor.get_event(clock, None)?;
        if !sensor.valid(clock) {
            return Ok(false);
        }
        sink.send(sensor.kind, &event, message_time(clock)?)?;
        Ok(true)
    })();
    match result {
        Ok(false) => return Ok(()),
        Ok(true) => {}
        Err(error) => sink.log(
            "exception",
            &format!("Error in {} polling loop", sensor.kind.service()),
            Some(&error),
        )?,
    }
    rate.keep_time(clock);
    Ok(())
}
