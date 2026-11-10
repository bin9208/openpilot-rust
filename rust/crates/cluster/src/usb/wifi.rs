//! S3 adapter uses the same native TURZX packet builder as USB. It owns a
//! separately authenticated stream and never drains arbitrary TCP bytes.
use super::{packet, Runtime};
use crate::Error;
use openpilot_cluster_wifi::{Channel, Connection, Queue, Registration, TcpSocket};
use std::{io, time::Duration};

pub(super) struct Wifi<C: Channel = Connection<TcpSocket>> {
    registration: Registration,
    queue: Queue<C>,
    pub h264_active: bool,
}
impl<C: Channel> Wifi<C> {
    pub fn new(registration: Registration) -> Self {
        Self {
            registration,
            queue: Queue::new(None),
            h264_active: false,
        }
    }
    pub fn open_with(
        &mut self,
        runtime: &mut impl Runtime,
        brightness: i64,
        connect: impl FnOnce() -> Result<C, Error>,
    ) -> Result<(), Error> {
        let result = (|| {
            self.queue = Queue::new(Some(connect()?));
            let payload = packet::command(10, runtime.milliseconds()?, &[])?;
            runtime.message("Sending sync command (ID 10)...");
            self.transaction(&payload, false, true, runtime)?;
            runtime.sleep(Duration::from_millis(200));
            let payload = packet::command(14, runtime.milliseconds()?, &[(8, brightness)])?;
            self.transaction(&payload, false, brightness == 0, runtime)?;
            if brightness != 0 {
                runtime.sleep(Duration::from_millis(200));
            }
            Ok::<_, Error>(())
        })();
        if result.is_err() {
            self.close(runtime);
        }
        result.map_err(|cause| Error::DisplayIo {
            message: "S3 display connection failed; waiting for rediscovery",
            cause: Box::new(cause),
        })?;
        runtime.connected(true);
        Ok(())
    }
    pub fn connected(&self) -> bool {
        self.queue.connected()
    }
    pub fn transaction(
        &mut self,
        payload: &[u8],
        frame: bool,
        expect_reply: bool,
        runtime: &mut impl Runtime,
    ) -> Result<Vec<u8>, Error> {
        self.queue
            .transaction(payload, frame, expect_reply, |value| {
                runtime.connected(value);
            })
            .map_err(|error| Error::Io(io::Error::other(error)))
    }
    pub fn close(&mut self, runtime: &mut impl Runtime) {
        runtime.connected(false);
        if self.h264_active && self.queue.connected() && !self.queue.disconnected() {
            self.h264_active = false;
            for id in [123, 122, 122] {
                if let Ok(payload) = runtime
                    .milliseconds()
                    .and_then(|t| packet::command(id, t, &[]))
                {
                    let _ = self.transaction(&payload, false, false, runtime);
                }
            }
        }
        if self.queue.connected() && !self.queue.disconnected() {
            if let Ok(timestamp) = runtime.milliseconds() {
                if let Ok(payload) = packet::command(10, timestamp, &[]) {
                    if self.transaction(&payload, false, true, runtime).is_ok() {
                        runtime.sleep(Duration::from_millis(200));
                        if let Ok(timestamp) = runtime.milliseconds() {
                            if let Ok(payload) = packet::command(14, timestamp, &[(8, 0)]) {
                                let _ = self.transaction(&payload, false, true, runtime);
                            }
                        }
                    }
                }
            }
        }
        self.queue.close();
        self.h264_active = false;
    }
}

impl Wifi<Connection<TcpSocket>> {
    pub fn open(&mut self, runtime: &mut impl Runtime, brightness: i64) -> Result<(), Error> {
        let registration = self.registration.clone();
        self.open_with(runtime, brightness, || {
            let host = openpilot_cluster_wifi::discover(
                &registration,
                Duration::from_secs(2),
                &["255.255.255.255"],
            )
            .map_err(|error| Error::Io(io::Error::other(error)))?;
            openpilot_cluster_wifi::connect(&registration, &host, openpilot_cluster_wifi::PORT)
                .map_err(|error| Error::Io(io::Error::other(error)))
        })
    }
}

