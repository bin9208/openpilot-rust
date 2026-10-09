use openpilot_usbgpu::{
    hcq_vm::{Function, Host, Memory, Program},
    Error,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, BufRead, Write},
};
#[derive(Deserialize)]
struct Parameter {
    slot: usize,
    name: String,
    bytes: usize,
}
struct Rpc {
    input: io::BufReader<io::Stdin>,
    remaining: usize,
}
impl Host for Rpc {
    fn poll(&mut self) -> Result<(), Error> {
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or(Error::Contract("fixture instruction budget exhausted"))?;
        Ok(())
    }
    fn call(
        &mut self,
        function: Function,
        args: &[u64],
        memory: &mut Memory,
    ) -> Result<u64, Error> {
        let (address, size, input, request) = match function {
            Function::Control => {
                let input = args[1] & 0x80 != 0;
                (
                    args[5],
                    args[6],
                    input,
                    json!({"kind":"control","handle":args[0],"type":args[1],"request":args[2],"value":args[3],"index":args[4],"size":args[6],"timeout":args[7]}),
                )
            }
            Function::Bulk => (
                args[2],
                args[3],
                args[1] & 0x80 != 0,
                json!({"kind":"bulk","handle":args[0],"endpoint":args[1],"size":args[3],"timeout":args[5]}),
            ),
        };
        let size =
            usize::try_from(size).map_err(|_| Error::Contract("fixture transfer size overflow"))?;
        let mut request = request;
        if !input {
            request["data"] = json!(memory.read(address, size)?);
        }
        println!("{request}");
        io::stdout().flush()?;
        let mut line = String::new();
        self.input.read_line(&mut line)?;
        let response: Value = serde_json::from_str(&line)?;
        if input {
            let data: Vec<u8> = serde_json::from_value(response["data"].clone())?;
            if data.len() != size {
                return Err(Error::Contract("fixture response size mismatch"));
            }
            memory.write(address, &data)?;
        }
        if function == Function::Bulk && args[4] != 0 {
            memory.write(
                args[4],
                &u32::try_from(size)
                    .map_err(|_| Error::Contract("fixture transfer count overflow"))?
                    .to_le_bytes(),
            )?;
        }
        response["result"]
            .as_u64()
            .ok_or(Error::Contract("fixture return missing"))
    }
}
fn run() -> Result<Value, Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(Error::Contract(
            "expected nodes, parameters, initial ring position",
        ));
    }
    let program = Program::parse(&fs::read(&args[0])?)?;
    let parameters: Vec<Parameter> = serde_json::from_slice(&fs::read(&args[1])?)?;
    let position = args[2]
        .parse::<u64>()
        .map_err(|_| Error::Contract("invalid ring position"))?;
    let mut memory = Memory::default();
    let mut addresses = vec![0; 64];
    for p in &parameters {
        let mut data = vec![0; p.bytes];
        if p.slot == 0 || p.slot == 3 {
            data[..8].copy_from_slice(&(if p.slot == 0 { 1u64 } else { 2u64 }).to_le_bytes());
        } else if p.name.starts_with("usb_host") {
            data[..8].copy_from_slice(&0x1234_u64.to_le_bytes());
        } else if p.name.starts_with("put_value") {
            data[..8].copy_from_slice(&position.to_le_bytes());
        } else if p.name.starts_with("inputs") || p.name.starts_with("addr") {
            for (i, word) in data.chunks_exact_mut(8).enumerate() {
                word.copy_from_slice(
                    &(0x80000000
                        + u64::try_from(p.slot).map_err(|_| Error::Contract("slot overflow"))?
                            * 0x100000
                        + u64::try_from(i).map_err(|_| Error::Contract("word overflow"))? * 4096)
                        .to_le_bytes(),
                );
            }
        }
        *addresses
            .get_mut(p.slot)
            .ok_or(Error::Contract("parameter slot out of range"))? = memory.allocate(data)?;
    }
    let mut execution = program.bind(&mut memory, &addresses)?;
    execution.run(
        &mut memory,
        &mut Rpc {
            input: io::BufReader::new(io::stdin()),
            remaining: 1_000_000,
        },
    )?;
    let mut outputs = Vec::new();
    for p in parameters.iter().filter(|p| p.slot != 0 && p.slot != 3) {
        outputs.push(json!({"slot":p.slot,"sha256":format!("{:x}",Sha256::digest(memory.read(addresses[p.slot],p.bytes)?))}));
    }
    Ok(json!({"buffers":outputs}))
}
fn main() {
    match run() {
        Ok(done) => println!("{}", json!({"done":done})),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
