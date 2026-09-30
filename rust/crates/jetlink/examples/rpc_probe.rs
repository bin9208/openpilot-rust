use openpilot_jetlink::{
    contract::{self, Identity},
    owner::{Backend, Server},
    rpc::ProxyClient,
    Deadline, Error,
};
use std::{
    io::{self, Read, Write},
    path::Path,
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};
struct Synthetic;
impl Backend for Synthetic {
    fn connect(&mut self) -> Result<Identity, Error> {
        Ok(Identity::new())
    }
    fn infer(
        &mut self,
        _: u32,
        _: &[u8],
        _: &[f32],
        _: Deadline,
        _: bool,
    ) -> Result<Vec<f32>, Error> {
        Ok(vec![0.25; contract::OUTPUT_FLOATS])
    }
    fn dead(&self) -> bool {
        false
    }
    fn close(&mut self) {}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).ok_or("expected mode")?;
    let path = Path::new(args.get(2).ok_or("expected path")?);
    match mode.as_str() {
        "server" => {
            let mut server = Server::spawn(Synthetic, path)?;
            let end = Instant::now() + Duration::from_secs(2);
            while !server.ready.load(Ordering::Acquire) {
                if Instant::now() > end {
                    return Err("owner not ready".into());
                }
                thread::yield_now();
            }
            println!("READY");
            io::stdout().flush()?;
            let mut input = [0; 1];
            io::stdin().read_exact(&mut input)?;
            server.stop(Duration::from_secs(1))?;
            println!("STOPPED");
        }
        "client" => {
            let mut client = ProxyClient::connect(path)?;
            let output = client.infer(
                42,
                &vec![13; contract::WARPED_BYTES],
                &[0.125; 12],
                Deadline::after(Duration::from_millis(50))?,
                true,
            )?;
            println!(
                "{}",
                serde_json::json!({"count":output.len(),"first":output.first(),"last":output.last(),"dead":client.dead()})
            );
            client.close();
        }
        _ => return Err("unknown mode".into()),
    }
    Ok(())
}
