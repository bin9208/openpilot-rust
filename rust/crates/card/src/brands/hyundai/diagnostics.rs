use super::{
    parser_inputs::{Channel, Inputs},
    python_set::IntegerSet,
    Error,
};
use openpilot_can::{parser::Parser, Packet};

#[derive(Default)]
pub struct Diagnostics {
    pub prints: Vec<String>,
    pub warnings: Vec<String>,
    seen: [IntegerSet; 3],
}

fn index(channel: Channel) -> usize {
    match channel {
        Channel::Pt => 0,
        Channel::Cam => 1,
        Channel::Alt => 2,
    }
}

impl Diagnostics {
    pub fn seen(&mut self, input: (&Parser, Channel), packets: &[Packet]) -> Result<(), Error> {
        let (parser, channel) = input;
        if parser.controls_ready {
            for frame in packets
                .iter()
                .flat_map(|packet| &packet.frames)
                .filter(|frame| frame.bus == parser.bus)
            {
                self.seen[index(channel)].insert(frame.address)?;
            }
        }
        Ok(())
    }

    pub fn monitor(&mut self, count: u32, alt: bool) {
        let line = match count {
            101 => Some(format!(
                "cp_cam.seen_addresses = {}",
                self.seen[1].source_repr()
            )),
            102 => Some(format!(
                "cp.seen_addresses = {}",
                self.seen[0].source_repr()
            )),
            103 => Some(format!(
                "cp_alt.seen_addresses = {}",
                if alt {
                    self.seen[2].source_repr()
                } else {
                    "None".into()
                }
            )),
            _ => None,
        };
        if let Some(line) = line {
            self.prints.push(line);
        }
    }

    pub fn retain_warnings(&mut self, parser: &mut Parser) {
        for diagnostic in std::mem::take(&mut parser.diagnostics) {
            self.warnings.push(diagnostic.message);
        }
    }
}

pub fn constructor(inputs: &mut Inputs, fd: bool) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static LEGACY: AtomicBool = AtomicBool::new(false);
    static CANFD: AtomicBool = AtomicBool::new(false);
    let loaded = if fd { &CANFD } else { &LEGACY };
    if !loaded.swap(true, Ordering::Relaxed) {
        inputs.diagnostics.prints.push(format!(
            "DBC: {}",
            if fd {
                "hyundai_canfd_generated"
            } else {
                "hyundai_kia_generic"
            }
        ));
        if fd {
            inputs
                .diagnostics
                .prints
                .push("Using Hyundai CAN FD checksum".into());
        }
    }
}
