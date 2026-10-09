use super::{bridge::Bridge, domain, media, wire};
use crate::{Error, Value};
use std::time::{Duration, Instant};

impl Bridge {
    pub(super) fn receive(&mut self) -> Result<(), Error> {
        if let Some(socket) = &mut self.state_socket {
            if let Some(bytes) = socket
                .receive(Duration::ZERO)
                .map_err(|error| Error::Source(error.to_string()))?
            {
                let state = domain::state(&bytes)?;
                let wire = wire::text(&Value::object([
                    ("type", Value::text("carrotNaviState")),
                    ("version", Value::integer(1)),
                    ("state", state.clone()),
                ]))?;
                self.last_state = Some(state);
                self.last_wire = Some(wire.clone());
                self.state_at = Some(Instant::now());
                self.state_count += 1;
                self.error.clear();
                self.clients.state(&wire);
            }
        }
        if let Some(socket) = &mut self.media_socket {
            for _ in 0..64 {
                let Some(bytes) = socket
                    .receive(Duration::ZERO)
                    .map_err(|error| Error::Source(error.to_string()))?
                else {
                    break;
                };
                let Some(media) = media::parse(&bytes)? else {
                    continue;
                };
                self.media_count += 1;
                self.error.clear();
                let packets = self.pipeline.receive(media, self.clients.wants_map())?;
                self.clients.media(packets);
            }
        }
        Ok(())
    }
}
