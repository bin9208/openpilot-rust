use openpilot_pandad::{
    can::Frame,
    can_io::{BulkTransport, CanIo, Outgoing},
    device::{Control, Transport},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    io::{self, BufRead},
};

#[derive(Deserialize)]
struct Reply {
    data: Vec<u8>,
    count: i32,
    healthy: bool,
}
#[derive(Deserialize)]
struct SendFrame {
    address: u32,
    src: u8,
    data: Vec<u8>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Operation {
    Send { frames: Vec<SendFrame> },
    Receive,
}
#[derive(Deserialize)]
struct Request {
    offset: u32,
    write_result: i32,
    replies: VecDeque<Reply>,
    operations: Vec<Operation>,
}
struct Recorder {
    replies: RefCell<VecDeque<Reply>>,
    calls: RefCell<Vec<Value>>,
    healthy: Cell<bool>,
    write_result: i32,
}

impl Transport for Recorder {
    type Error = io::Error;
    fn control_read(&self, _: Control, _: &mut [u8]) -> Result<i32, Self::Error> {
        Err(io::Error::other("unexpected control read"))
    }
    fn control_write(&self, command: Control) -> Result<i32, Self::Error> {
        self.calls
            .borrow_mut()
            .push(json!({"op":"control","request":command.request,
            "value":command.value,"index":command.index,"timeout":command.timeout_ms}));
        Ok(0)
    }
}

impl BulkTransport for Recorder {
    fn bulk_read(
        &self,
        endpoint: u8,
        output: &mut [u8],
        timeout_ms: u32,
    ) -> Result<i32, Self::Error> {
        self.calls.borrow_mut().push(
            json!({"op":"read","endpoint":endpoint,"length":output.len(),"timeout":timeout_ms}),
        );
        if endpoint == 0xab {
            return Ok(0);
        }
        let reply = self
            .replies
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| io::Error::other("unexpected bulk read"))?;
        if reply.data.len() > output.len() {
            return Err(io::Error::other("receive fixture overflow"));
        }
        output[..reply.data.len()].copy_from_slice(&reply.data);
        self.healthy.set(reply.healthy);
        Ok(reply.count)
    }
    fn bulk_write(&self, endpoint: u8, input: &[u8], timeout_ms: u32) -> Result<i32, Self::Error> {
        self.calls
            .borrow_mut()
            .push(json!({"op":"write","endpoint":endpoint,"data":input,"timeout":timeout_ms}));
        Ok(self.write_result)
    }
    fn comms_healthy(&self) -> bool {
        self.healthy.get()
    }
}

fn run(request: Request) -> Result<Value, Box<dyn std::error::Error>> {
    let recorder = Recorder {
        replies: RefCell::new(request.replies),
        calls: RefCell::new(Vec::new()),
        healthy: Cell::new(true),
        write_result: request.write_result,
    };
    let mut io = CanIo::new(request.offset, std::env::var_os("PANDAD_MAXOUT").is_some());
    let mut received: Vec<Frame> = Vec::new();
    let mut results = Vec::new();
    for operation in request.operations {
        let value = match operation {
            Operation::Send { frames } => {
                let frames = frames
                    .iter()
                    .map(|frame| Outgoing {
                        address: frame.address,
                        src: frame.src,
                        data: &frame.data,
                    })
                    .collect::<Vec<_>>();
                io.send(&recorder, &frames)?;
                Value::Null
            }
            Operation::Receive => json!(io.receive(&recorder, &mut received, || {
                recorder
                    .calls
                    .borrow_mut()
                    .push(json!({"op":"log","level":40,"message":"Panda CAN checksum failed"}));
            })?),
        };
        results.push(json!({"value":value,"frames":received,"calls":*recorder.calls.borrow(),"remaining":io.remaining()}));
    }
    Ok(json!({"results":results,"unused_replies":recorder.replies.borrow().len()}))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        println!("{}", run(serde_json::from_str(&line?)?)?);
    }
    Ok(())
}
