use openpilot_pandad::{
    health::{CanHealth, Health},
    state::{Diagnostic, Effects, Identity, Input, Publisher, Snapshot},
};
use std::convert::Infallible;

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Safety(usize, u16),
    Power(usize, bool),
    Publish(bool, Vec<bool>),
    List,
    Exit,
    Heartbeat(usize, bool),
}

struct Recorder {
    ids: Vec<Identity>,
    health: Vec<Health>,
    healthy: bool,
    can_available: bool,
    calls: Vec<Call>,
    listed: Vec<Vec<u8>>,
}

impl Effects for Recorder {
    type Error = Infallible;
    fn identities(&self) -> &[Identity] {
        &self.ids
    }
    fn health(&mut self, index: usize) -> Result<Option<Health>, Self::Error> {
        Ok(Some(self.health[index]))
    }
    fn can_health(&mut self, _: usize, _: u16) -> Result<Option<CanHealth>, Self::Error> {
        Ok(self.can_available.then(CanHealth::default))
    }
    fn healthy(&self, _: usize) -> bool {
        self.healthy
    }
    fn set_safety(&mut self, index: usize, model: u16) -> Result<(), Self::Error> {
        self.calls.push(Call::Safety(index, model));
        Ok(())
    }
    fn set_power_saving(&mut self, index: usize, value: bool) -> Result<(), Self::Error> {
        self.calls.push(Call::Power(index, value));
        Ok(())
    }
    fn publish(&mut self, states: &[Snapshot], valid: bool) -> Result<(), Self::Error> {
        self.calls.push(Call::Publish(
            valid,
            states
                .iter()
                .map(|state| state.health.ignition_line != 0)
                .collect(),
        ));
        Ok(())
    }
    fn list_usb(&mut self) -> Result<Vec<Vec<u8>>, Self::Error> {
        self.calls.push(Call::List);
        Ok(self.listed.clone())
    }
    fn heartbeat(&mut self, index: usize, engaged: bool) -> Result<(), Self::Error> {
        self.calls.push(Call::Heartbeat(index, engaged));
        Ok(())
    }
    fn request_exit(&mut self) {
        self.calls.push(Call::Exit);
    }
    fn diagnostic(&mut self, _: Diagnostic<'_>) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn recorder() -> Recorder {
    Recorder {
        ids: vec![Identity {
            hardware_type: 6,
            serial: b"internal".to_vec(),
        }],
        health: vec![Health::default()],
        healthy: true,
        can_available: true,
        calls: Vec::new(),
        listed: Vec::new(),
    }
}

#[test]
fn offroad_reconnection_still_sends_heartbeat_after_publication_and_exit_request() {
    let mut io = recorder();
    io.listed = vec![b"internal".to_vec(), b"new".to_vec()];
    let mut state = Publisher::default();
    assert_eq!(
        state.update(Input::default(), &mut io).unwrap(),
        Some(false)
    );
    assert_eq!(
        io.calls,
        [
            Call::Safety(0, 19),
            Call::Power(0, true),
            Call::Safety(0, 19),
            Call::Publish(true, vec![false]),
            Call::List,
            Call::Exit,
            Call::Heartbeat(0, false)
        ]
    );
}

#[test]
fn c3_pair_uses_red_ignition_and_preserves_dos_can_ignition() {
    let mut io = recorder();
    io.ids.push(Identity {
        hardware_type: 7,
        serial: b"red".to_vec(),
    });
    io.health[0].ignition_line = 1;
    io.health.push(Health::default());
    let mut state = Publisher::default();
    assert_eq!(
        state
            .update(
                Input {
                    onroad: true,
                    ..Input::default()
                },
                &mut io
            )
            .unwrap(),
        Some(false)
    );
    assert!(io.calls.contains(&Call::Publish(true, vec![false, false])));
    io.health[0].ignition_can = 1;
    assert_eq!(
        state
            .update(
                Input {
                    onroad: true,
                    ..Input::default()
                },
                &mut io
            )
            .unwrap(),
        Some(true)
    );
}

#[test]
fn incomplete_can_health_prevents_commands_publication_and_heartbeat() {
    let mut io = recorder();
    io.can_available = false;
    let mut state = Publisher::default();
    assert_eq!(state.update(Input::default(), &mut io).unwrap(), None);
    assert!(io.calls.is_empty());
}

#[test]
fn unhealthy_ignition_state_is_published_invalid_and_reconnects_without_enumerating() {
    let mut io = recorder();
    io.healthy = false;
    let mut state = Publisher::default();
    assert_eq!(
        state
            .update(
                Input {
                    engaged: true,
                    ..Input::default()
                },
                &mut io
            )
            .unwrap(),
        Some(false)
    );
    assert!(io.calls.contains(&Call::Publish(false, vec![false])));
    assert!(!io.calls.contains(&Call::List));
    assert!(io.calls.ends_with(&[Call::Exit, Call::Heartbeat(0, true)]));
}
