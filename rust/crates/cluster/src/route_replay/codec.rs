use std::io::{self, Read};

pub(super) fn decode_bzip(mut data: &[u8]) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut completed = false;
    while !data.is_empty() {
        let start = output.len();
        let mut decoder = bzip2::bufread::BzDecoder::new(data);
        match decoder.read_to_end(&mut output) {
            Ok(_) => {
                data = decoder.into_inner();
                completed = true;
            }
            Err(error) if completed && error.kind() == io::ErrorKind::InvalidInput => {
                output.truncate(start);
                break;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(output)
}

pub(super) fn decode_zstd(data: &[u8]) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    match zstd::stream::read::Decoder::new(data)?.read_to_end(&mut output) {
        Ok(_) => Ok(output),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(output),
        Err(error) => Err(error),
    }
}
