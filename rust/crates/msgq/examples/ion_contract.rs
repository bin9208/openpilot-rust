#[cfg(feature = "visionipc-ion")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use openpilot_msgq::{
        RawVisionImage, VisionClient, VisionLayout, VisionMetadata, VisionServer, VisionStream,
    };
    let server = VisionServer::new("ioncontract")?;
    let layout = VisionLayout {
        width: 8,
        height: 4,
        stride: 16,
        uv_offset: 64,
        len: 96,
    };
    let images = server.create_stream(VisionStream::Road, 1, layout)?;
    let mut initial = [0xff; 96];
    images[0].copy_into(&mut initial)?;
    if initial != [0; 96] {
        return Err("new ION image is not zeroed".into());
    }
    let bytes: Vec<_> = (0_u8..96).map(|value| value.wrapping_add(42)).collect();
    images[0].write(0, &bytes)?;
    server.start_listener()?;
    let mut client = VisionClient::new("ioncontract", VisionStream::Road, false)?;
    if !client.connect()? {
        return Err("could not connect to fixture server".into());
    }
    images[0].publish(VisionMetadata {
        width: 0,
        height: 0,
        stride: 0,
        uv_offset: 0,
        len: 0,
        frame_id: 42,
        timestamp_sof: 42000,
        timestamp_eof: 42100,
        valid: true,
        received: false,
        index: 0,
        fd: -1,
    })?;
    let frame = client
        .receive(std::time::Duration::from_secs(2))?
        .ok_or("fixture frame missing")?;
    let descriptor = frame.descriptor()?;
    let mut copied = vec![0; 96];
    frame.copy_into(&mut copied)?;
    if copied != bytes || descriptor.buffer_frame_id != 42 || frame.metadata().frame_id != 42 {
        return Err("fixture frame differs".into());
    }
    let checksum: u64 = copied
        .iter()
        .enumerate()
        .map(|(index, byte)| (index as u64 + 1) * u64::from(*byte))
        .sum();
    drop(client);
    drop(images);
    drop(server);
    let raw = RawVisionImage::new(64)?;
    let mut initial = [0xff; 64];
    raw.copy_into(&mut initial)?;
    if initial != [0; 64] {
        return Err("new raw ION image is not zeroed".into());
    }
    raw.write(0, &[7; 64])?;
    let mut copied = [0; 64];
    raw.copy_into(&mut copied)?;
    if copied != [7; 64] {
        return Err("fixture raw image differs".into());
    }
    drop(raw);
    println!("{{\"frame_id\":42,\"checksum\":{checksum},\"raw_sum\":448}}");
    Ok(())
}

fn main() {
    #[cfg(feature = "visionipc-ion")]
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
    #[cfg(not(feature = "visionipc-ion"))]
    {
        eprintln!("ion_contract requires visionipc-ion");
        std::process::exit(2);
    }
}
