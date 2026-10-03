use openpilot_pandad::{
    health::{CanHealth, Health},
    state::{Diagnostic, Effects, Identity, Input, Snapshot},
    state_wire,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io;

#[derive(Deserialize)]
pub struct PandaInput {
    health: Option<Vec<u8>>,
    can: [Option<Vec<u8>>; 3],
    healthy: bool,
}

#[derive(Deserialize)]
pub struct Step {
    pub input: Input,
    now_ns: u64,
    pandas: Vec<PandaInput>,
    listed: Vec<Vec<u8>>,
}

#[derive(Deserialize)]
pub struct Request {
    pub identities: Vec<Identity>,
    pub steps: Vec<Step>,
}

pub struct Fixture {
    pub identities: Vec<Identity>,
    pub step: Step,
    pub actions: Vec<Value>,
    pub exit: bool,
}

fn packet<const N: usize>(raw: Option<&[u8]>) -> io::Result<Option<[u8; N]>> {
    raw.map(|raw| {
        raw.try_into()
            .map_err(|_| io::Error::other("invalid fixture packet length"))
    })
    .transpose()
}

fn serial_text(serial: &[u8]) -> io::Result<&str> {
    std::str::from_utf8(serial).map_err(io::Error::other)
}

impl Effects for Fixture {
    type Error = io::Error;
    fn identities(&self) -> &[Identity] {
        &self.identities
    }
    fn health(&mut self, index: usize) -> io::Result<Option<Health>> {
        self.actions.push(json!(["read", index, 0xd2, 0, 0, 58, 0]));
        Ok(packet(self.step.pandas[index].health.as_deref())?
            .map(|bytes| Health::from_packet(&bytes)))
    }
    fn can_health(&mut self, index: usize, bus: u16) -> io::Result<Option<CanHealth>> {
        self.actions
            .push(json!(["read", index, 0xc2, bus, 0, 64, 0]));
        Ok(
            packet(self.step.pandas[index].can[usize::from(bus)].as_deref())?
                .map(|bytes| CanHealth::from_packet(&bytes)),
        )
    }
    fn healthy(&self, index: usize) -> bool {
        self.step.pandas[index].healthy
    }
    fn set_safety(&mut self, index: usize, model: u16) -> io::Result<()> {
        self.actions
            .push(json!(["write", index, 0xdc, model, 0, 0]));
        Ok(())
    }
    fn set_power_saving(&mut self, index: usize, value: bool) -> io::Result<()> {
        self.actions
            .push(json!(["write", index, 0xe7, u16::from(value), 0, 0]));
        Ok(())
    }
    fn publish(&mut self, states: &[Snapshot], valid: bool) -> io::Result<()> {
        let bytes =
            state_wire::encode(states, valid, self.step.now_ns).map_err(io::Error::other)?;
        self.actions.push(json!(["publish", "pandaStates", bytes]));
        Ok(())
    }
    fn list_usb(&mut self) -> io::Result<Vec<Vec<u8>>> {
        self.actions.push(json!(["list"]));
        Ok(self.step.listed.clone())
    }
    fn heartbeat(&mut self, index: usize, engaged: bool) -> io::Result<()> {
        self.actions
            .push(json!(["write", index, 0xf3, u16::from(engaged), 0, 0]));
        Ok(())
    }
    fn request_exit(&mut self) {
        self.exit = true;
    }
    fn diagnostic(&mut self, event: Diagnostic<'_>) -> io::Result<()> {
        let (level, text) = match event {
            Diagnostic::Checksum {
                index: _,
                serial,
                total,
                delta,
                baseline,
                reset,
            } => {
                let text = format!(
                    "SPI checksum: serial={}, total={total}, delta={delta}, baseline={}",
                    serial_text(serial)?,
                    u8::from(baseline)
                );
                let text = match reset {
                    Some(reset) => format!("{text}, reset={}", u8::from(reset)),
                    None => text,
                };
                (30, text)
            }
            Diagnostic::HealthUnavailable => (40, "Failed to get ignition_opt".into()),
            Diagnostic::UnhealthyReconnect => (
                40,
                "Reconnecting, communication to pandas not healthy".into(),
            ),
            Diagnostic::NewPanda { serial } => (
                30,
                format!("Reconnecting to new panda: {}", serial_text(serial)?),
            ),
        };
        self.actions.push(json!(["log", level, text]));
        Ok(())
    }
}
