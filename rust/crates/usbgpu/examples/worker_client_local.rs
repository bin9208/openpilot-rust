use openpilot_usbgpu::{
    client::{Client, Frame},
    Error,
};
use std::path::Path;
fn run() -> Result<(), Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(Error::Contract(
            "expected owned worker fixture and metadata",
        ));
    }
    let mut client = Client::load(Path::new(&args[0]), Path::new(&args[1]), [1344, 760])?;
    let image = vec![0x32; client.info.frame_size];
    let input = [0.; 12];
    let frame = || Frame {
        main: &image,
        extra: &image,
        inputs: &input,
        transforms: [[1., 0., 0., 0., 1., 0., 0., 0., 1.]; 2],
    };
    if client.run_with_prepare(frame(), true)?.is_some() {
        return Err(Error::Contract("local warm preparation published output"));
    }
    let first = client.run(frame())?[0];
    let previous = client.raw_output().to_vec();
    if client.run_with_prepare(frame(), true)?.is_some() || previous != client.raw_output() {
        return Err(Error::Contract("local prepare changed published output"));
    }
    let next = client.run(frame())?[0];
    let plane = image
        .get(..1622016)
        .ok_or(Error::Contract("owned plane bytes missing"))?;
    let rejected = client
        .run(Frame {
            main: plane,
            extra: plane,
            inputs: &input,
            transforms: [[0.; 9]; 2],
        })
        .is_err();
    let stopped = client.run(frame()).is_err();
    if first != 1. || next != 2. || !rejected || !stopped {
        return Err(Error::Contract("local client controls failed"));
    }
    println!(
        "{}",
        serde_json::json!({"first":first,"next":next,"prepare_only_published":false,"previous_output_preserved":true,
        "plane_only_rejected":rejected,"subsequent_rejected":stopped,"received_buffer_bytes":image.len()})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
