use super::{packet, write::NoAck, Display, Runtime};
use crate::Error;
use openpilot_usbgpu::transport::Setup;

impl<R: Runtime> Display<R> {
    pub(super) fn endpoints(&self, ack: bool) -> Result<(), Error> {
        if self.connection.as_ref().is_some_and(|connection| {
            connection.output().is_some() && (!ack || connection.input().is_some())
        }) {
            Ok(())
        } else {
            Err(Error::Contract("USB endpoints are not open"))
        }
    }

    pub(super) fn clear_halt(&mut self) {
        if let Some(connection) = self.connection.as_mut() {
            for endpoint in [connection.output(), connection.input()]
                .into_iter()
                .flatten()
            {
                // The original cleanup ignores per-endpoint clear errors and still attempts the other endpoint.
                if let Err(_error) = connection.setup(Setup::ClearHalt, i32::from(endpoint)) {}
            }
        }
    }

    pub(super) fn drain(&mut self, attempts: u32, timeout: u32) {
        if let Some(connection) = self.connection.as_mut() {
            let Some(input) = connection.input() else {
                return;
            };
            for _ in 0..attempts {
                if connection.bulk(input, &mut [0; 512], timeout).is_err() {
                    return;
                }
            }
        }
    }

    pub(super) fn write(&mut self, payload: &mut [u8]) -> Result<(), Error> {
        let connection = self
            .connection
            .as_mut()
            .ok_or(Error::Contract("USB OUT endpoint is not open"))?;
        let output = connection
            .output()
            .ok_or(Error::Contract("USB OUT endpoint is not open"))?;
        connection.bulk(output, payload, 2000)?;
        Ok(())
    }

    pub(super) fn read_ack(&mut self) -> Result<Vec<u8>, Error> {
        let connection = self
            .connection
            .as_mut()
            .ok_or(Error::Contract("USB endpoints are not open"))?;
        let mut response = [0; 512];
        let input = connection
            .input()
            .ok_or(Error::Contract("USB endpoints are not open"))?;
        let count = connection.bulk(input, &mut response, 2000)?;
        Ok(response[..count].to_vec())
    }

    pub(super) fn io_error(&mut self, message: &'static str, cause: Error) -> Error {
        if super::write::disconnected(&cause) {
            self.connection = None;
            self.runtime.connected(false);
            Error::Disconnected(Box::new(cause))
        } else {
            Error::DisplayIo {
                message,
                cause: Box::new(cause),
            }
        }
    }

    pub(super) fn command_ack(
        &mut self,
        id: u8,
        fields: &[(usize, i64)],
    ) -> Result<Vec<u8>, Error> {
        let mut payload = packet::command(id, self.runtime.milliseconds()?, fields)?;
        self.endpoints(true)?;
        let start = self.profile.start();
        self.clear_halt();
        self.drain(3, 20);
        self.profile.add("usb.write_checked.prepare", start);
        let result = (|| {
            let start = self.profile.start();
            self.write(&mut payload)?;
            self.profile.add("usb.write_checked.write", start);
            let start = self.profile.start();
            let response = self.read_ack()?;
            self.profile.add("usb.write_checked.read_ack", start);
            Ok(response)
        })();
        result.map_err(|cause| self.io_error("TURZX USB command timed out", cause))
    }

    pub(super) fn optional_command(&mut self, id: u8, fields: &[(usize, i64)], pacing: NoAck) {
        let result = self
            .runtime
            .milliseconds()
            .and_then(|timestamp| packet::command(id, timestamp, fields))
            .and_then(|mut payload| {
                self.endpoints(false)?;
                let start = self.profile.start();
                self.clear_halt();
                self.drain(3, 20);
                self.profile.add("usb.write_no_ack.prepare", start);
                let start = self.profile.start();
                self.write(&mut payload)?;
                self.profile.add("usb.write_no_ack.write", start);
                Ok(())
            });
        match result {
            Ok(()) => {
                if !pacing.gap.is_zero() {
                    self.runtime.sleep(pacing.gap);
                }
                self.drain(pacing.drains, 20);
            }
            Err(cause) => {
                let error = self.io_error("TURZX USB command write failed", cause);
                self.runtime.warning(&format!(
                    "Warning: optional TURZX USB command skipped: {error}"
                ));
            }
        }
    }
}
