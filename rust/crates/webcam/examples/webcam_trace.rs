use openpilot_webcam::{capture::Capture, pixels, Error};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

fn main() -> Result<(), Error> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode, width, height] if mode == "pixels" => {
            let width = width
                .parse()
                .map_err(|_| Error::Contract("invalid width"))?;
            let height = height
                .parse()
                .map_err(|_| Error::Contract("invalid height"))?;
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input)?;
            pixels::rotate(&mut input, width, height)?;
            std::io::stdout().write_all(&pixels::nv12(&input, width, height)?)?;
        }
        [mode, input, output] if mode == "capture" => {
            fs::create_dir(Path::new(output))?;
            let mut capture = Capture::path(input)?;
            let info = capture.info()?;
            let mut count = 0_u32;
            while let Some(bytes) = capture.read()? {
                fs::write(Path::new(output).join(format!("{count}.nv12")), bytes)?;
                count = count
                    .checked_add(1)
                    .ok_or(Error::Contract("frame ID overflow"))?;
            }
            println!(
                "{}",
                serde_json::json!({"info":info,"frames":count,"opened_after_eof":capture.opened()?})
            );
        }
        _ => {
            return Err(Error::Contract(
                "usage: webcam_trace pixels WIDTH HEIGHT | capture INPUT OUTPUT",
            ))
        }
    }
    Ok(())
}
