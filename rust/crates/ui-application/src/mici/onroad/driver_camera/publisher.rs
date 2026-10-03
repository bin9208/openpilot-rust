use super::Preview;
use crate::{params::Read, Error};
use num_traits::ToPrimitive;
impl Preview {
    pub(super) fn start(&mut self) -> Result<(), Error> {
        self.context.params.put_bool("IsDriverViewEnabled", true)?;
        self.context.device.borrow_mut().set_override_timeout(
            Some(300),
            (self.context.now_monotonic)(),
            self.context.ui.borrow().ignition,
        );
        self.context.params.remove("DriverTooDistracted")?;
        self.publisher = None;
        if !self.context.ui.borrow().started && !self.context.params.boolean("IsOnroad")? {
            self.publisher = Some(
                openpilot_msgq::Publisher::transient_for_runtime(
                    "selfdriveState",
                    openpilot_messaging::services::lookup("selfdriveState")
                        .ok_or(Error::Contract("selfdriveState service missing"))?
                        .queue_size,
                )
                .map_err(|error| Error::Io(std::io::Error::other(error)))?,
            );
            self.publish()?;
        }
        Ok(())
    }
    pub(super) fn publish(&mut self) -> Result<(), Error> {
        if self.publisher.is_none() {
            return Ok(());
        }
        if self.context.ui.borrow().started || self.context.params.boolean("IsOnroad")? {
            self.publisher = None;
            return Ok(());
        }
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder<'_>>();
        event.set_valid(false);
        event.set_log_mono_time(
            ((self.context.now_monotonic)() * 1e9)
                .to_u64()
                .ok_or(Error::Contract("invalid driver preview timestamp"))?,
        );
        event.init_selfdrive_state();
        let bytes = capnp::serialize::write_message_to_words(&message);
        if let Some(publisher) = &mut self.publisher {
            if !publisher
                .send_if_current(&bytes)
                .map_err(|error| Error::Io(std::io::Error::other(error)))?
            {
                self.publisher = None;
            }
        }
        Ok(())
    }
}
