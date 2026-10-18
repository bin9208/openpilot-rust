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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::{cell::RefCell, rc::Rc};
    fn hex(payload: &[u8]) -> String {
        use std::fmt::Write;
        payload.iter().fold(String::new(), |mut text, byte| {
            write!(text, "{byte:02x}").unwrap();
            text
        })
    }
    struct Fake {
        calls: Rc<RefCell<Vec<Value>>>,
        sequence: u64,
        fault: String,
    }
    impl Channel for Fake {
        fn send(
            &mut self,
            payload: &[u8],
            expect_reply: bool,
        ) -> Result<u64, openpilot_cluster_wifi::Error> {
            self.calls
                .borrow_mut()
                .push(json!(["send", hex(payload), expect_reply]));
            let phase =
                ["sync", "brightness", "close", "off"][usize::try_from(self.sequence).unwrap()];
            if self.fault == format!("{phase}_send") {
                return Err(io::Error::other("send failed").into());
            }
            self.sequence += 1;
            Ok(self.sequence)
        }
        fn result(&mut self) -> Result<(u64, Vec<u8>), openpilot_cluster_wifi::Error> {
            self.calls.borrow_mut().push(json!(["result"]));
            let phase =
                ["sync", "brightness", "close", "off"][usize::try_from(self.sequence - 1).unwrap()];
            if self.fault == format!("{phase}_result") {
                return Err(io::Error::other("result failed").into());
            }
            Ok((
                self.sequence + u64::from(self.fault == format!("{phase}_sequence")),
                vec![u8::try_from(self.sequence).unwrap()],
            ))
        }
        fn close(&mut self) {
            self.calls.borrow_mut().push(json!(["close"]));
        }
    }
    struct Effects(Rc<RefCell<Vec<Value>>>);
    impl Runtime for Effects {
        fn milliseconds(&self) -> Result<u32, Error> {
            Ok(3210)
        }
        fn sleep(&mut self, duration: Duration) {
            self.0.borrow_mut().push(json!([
                "sleep_ms",
                u64::try_from(duration.as_millis()).unwrap()
            ]));
        }
        fn gpu_active(&self) -> bool {
            false
        }
        fn reset_protected(&self) -> bool {
            false
        }
        fn connected(&mut self, value: bool) {
            self.0.borrow_mut().push(json!(["connected", value]));
        }
        fn warning(&mut self, text: &str) {
            panic!("unexpected warning: {text}");
        }
        fn message(&mut self, text: &str) {
            self.0.borrow_mut().push(json!(["print", text]));
        }
    }
    #[test]
    fn original_open_close_commands_encryption_delays_and_failure_cleanup() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/data/wifi_lifecycle.json")).unwrap();
        for (index, case) in fixture["input"].as_array().unwrap().iter().enumerate() {
            let calls = Rc::new(RefCell::new(Vec::new()));
            let mut effects = Effects(calls.clone());
            let registration = Registration::parse(r#"{"version":1,"device":"001122334455","key":"abababababababababababababababababababababababababababababababab"}"#).unwrap();
            let mut wifi = Wifi::new(registration);
            let fault = case["fault"].as_str().unwrap();
            let result = wifi.open_with(&mut effects, case["brightness"].as_i64().unwrap(), || {
                calls.borrow_mut().push(json!(["connect"]));
                if fault == "connect" {
                    return Err(Error::Io(io::Error::other("connect failed")));
                }
                Ok(Fake {
                    calls: calls.clone(),
                    sequence: 0,
                    fault: fault.into(),
                })
            });
            wifi.close(&mut effects);
            let mut result = match result {
                Ok(()) => json!({"opened":true}),
                Err(error) => json!({"error":error.to_string()}),
            };
            result["calls"] = json!(*calls.borrow());
            result["pending"] = json!(wifi.queue.pending());
            result["connected"] = json!(wifi.queue.connected());
            result["disconnected"] = json!(wifi.queue.disconnected());
            assert_eq!(
                result, fixture["expected"][index],
                "source lifecycle {index}"
            );
        }
    }
}
