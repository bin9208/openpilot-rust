use std::{
    fs,
    io::{self, Read},
    path::Path,
};

fn bzip(mut input: &[u8]) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut completed = false;
    while !input.is_empty() {
        let mut decoder = bzip2::bufread::BzDecoder::new(input);
        let mut member = Vec::new();
        match decoder.read_to_end(&mut member) {
            Ok(_) => {
                output.extend(member);
                completed = true;
                input = decoder.into_inner();
            }
            Err(error) if completed && error.kind() == io::ErrorKind::InvalidInput => break,
            Err(error) => return Err(error),
        }
    }
    Ok(output)
}
pub fn decompress(path: &Path) -> io::Result<Vec<u8>> {
    let data = fs::read(path)?;
    if path.extension().is_some_and(|value| value == "bz2") || data.starts_with(b"BZh9") {
        bzip(&data)
    } else if path.extension().is_some_and(|value| value == "zst")
        || data.starts_with(&[0x28, 0xb5, 0x2f, 0xfd])
    {
        let mut output = Vec::new();
        match zstd::stream::read::Decoder::new(data.as_slice())?.read_to_end(&mut output) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {}
            Err(error) => return Err(error),
        }
        Ok(output)
    } else {
        Ok(data)
    }
}
