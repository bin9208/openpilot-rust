use super::{DiagnosticLevel, Error, ParallelQuery, QueryIo, Target};
use crate::{
    isotp::{CanClient, IsoTpMessage},
    query_io::BufferedIo,
};
use std::collections::BTreeMap;

struct Pending {
    target: Target,
    address: i64,
    message: IsoTpMessage,
    counter: usize,
    done: bool,
    deadline: f64,
    responded: bool,
}

impl ParallelQuery {
    /// Runs the complete source request sequence and strips only its final prefix.
    /// # Errors
    /// Returns source constructor/request failures and transport-boundary failures.
    /// Individual malformed ISO-TP responses are handled as failed targets, like source.
    pub fn get_data(
        &mut self,
        timeout: f64,
        total_timeout: f64,
        io: &mut impl QueryIo,
    ) -> Result<Vec<(Target, Vec<u8>)>, Error> {
        io.receive(false)?;
        let mut buffers = BTreeMap::new();
        let mut pending = Vec::new();
        for (target, address) in &self.addresses {
            let client = CanClient::new(target.0, Some(*address), self.bus, target.1, None);
            pending.push(Pending {
                target: *target,
                address: *address,
                message: IsoTpMessage::new(client, 0., false, 0.01)?,
                counter: 0,
                done: false,
                deadline: 0.,
                responded: false,
            });
        }
        for address in &self.functional_addrs {
            let client = CanClient::new(*address, None, self.bus, None, None);
            let mut message = IsoTpMessage::new(client, 0., false, 0.01)?;
            let request = self.request.first().ok_or(Error::RequestIndex)?;
            message.send(
                request,
                false,
                &mut BufferedIo {
                    io,
                    buffers: &mut buffers,
                    address: 0,
                    subaddress: None,
                },
            )?;
        }
        for state in &mut pending {
            let request = self.request.first().ok_or(Error::RequestIndex)?;
            state.message.send(
                request,
                !self.functional_addrs.is_empty(),
                &mut BufferedIo {
                    io,
                    buffers: &mut buffers,
                    address: state.address,
                    subaddress: state.target.1,
                },
            )?;
        }
        let start = io.now();
        for state in &mut pending {
            state.deadline = start + timeout;
        }
        let mut results: Vec<(Target, Vec<u8>)> = Vec::new();
        loop {
            for packet in io.receive(true)? {
                for frame in packet {
                    if frame.bus == self.bus
                        && self
                            .addresses
                            .iter()
                            .any(|(_, address)| *address == i64::from(frame.address))
                    {
                        buffers
                            .entry(i64::from(frame.address))
                            .or_insert_with(Vec::new)
                            .push(frame);
                    }
                }
            }
            for state in &mut pending {
                let response = state.message.recv(
                    None,
                    &mut BufferedIo {
                        io,
                        buffers: &mut buffers,
                        address: state.address,
                        subaddress: state.target.1,
                    },
                );
                let (data, in_progress) = match response {
                    Ok(value) => value,
                    Err(_) => {
                        io.log(
                            DiagnosticLevel::Exception,
                            &format!(
                                "Error processing UDS response: {}",
                                target_repr(state.target)
                            ),
                        );
                        state.done = true;
                        continue;
                    }
                };
                if in_progress {
                    state.responded = true;
                    state.deadline = io.now() + timeout;
                }
                let Some(data) = data else { continue };
                if data.is_empty() {
                    io.log(
                        DiagnosticLevel::Error,
                        &format!("iso-tp query empty response: {}", target_repr(state.target)),
                    );
                    state.done = true;
                    continue;
                }
                let expected = self
                    .response
                    .get(state.counter)
                    .ok_or(Error::RequestIndex)?;
                if data.starts_with(expected) {
                    match self.request.get(state.counter + 1) {
                        Some(request) => {
                            state.deadline = io.now() + timeout;
                            state.message.send(
                                request,
                                false,
                                &mut BufferedIo {
                                    io,
                                    buffers: &mut buffers,
                                    address: state.address,
                                    subaddress: state.target.1,
                                },
                            )?;
                            state.counter += 1;
                        }
                        None => {
                            let value = data[expected.len()..].to_vec();
                            match results
                                .iter_mut()
                                .find(|(target, _)| *target == state.target)
                            {
                                Some((_, result)) => *result = value,
                                None => results.push((state.target, value)),
                            }
                            state.done = true;
                        }
                    }
                } else if data.get(2) == Some(&0x78) {
                    state.deadline = io.now() + self.pending_timeout;
                    io.log(
                        DiagnosticLevel::Error,
                        &format!(
                            "iso-tp query response pending: {}",
                            target_repr(state.target)
                        ),
                    );
                } else {
                    state.done = true;
                    let data: String = data.iter().map(|byte| format!("{byte:02x}")).collect();
                    io.log(
                        DiagnosticLevel::Error,
                        &format!(
                            "iso-tp query bad response: {} - 0x{data}",
                            target_repr(state.target)
                        ),
                    );
                }
            }
            let now = io.now();
            for state in &mut pending {
                if now - state.deadline > 0. {
                    if !state.done {
                        let reason = if state.counter > 0 {
                            Some("after receiving partial response")
                        } else if state.responded {
                            Some("while receiving response")
                        } else {
                            None
                        };
                        if let Some(reason) = reason {
                            io.log(
                                DiagnosticLevel::Error,
                                &format!(
                                    "iso-tp query timeout {reason}: {}",
                                    target_repr(state.target)
                                ),
                            );
                        }
                    }
                    state.done = true;
                }
            }
            if pending.iter().all(|state| state.done) {
                break;
            }
            if now - start > total_timeout {
                io.log(
                    DiagnosticLevel::Error,
                    "iso-tp query timeout while receiving data",
                );
                break;
            }
        }
        Ok(results)
    }
}

fn target_repr(target: Target) -> String {
    format!(
        "({}, {})",
        target.0,
        target
            .1
            .map(|value| value.to_string())
            .unwrap_or_else(|| "None".into())
    )
}
