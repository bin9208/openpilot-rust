use crate::Error;

pub(super) fn transparent() -> Result<Vec<u8>, Error> {
    const WIDTH: u32 = 464;
    const HEIGHT: u32 = 1920;
    let rows = vec![0; (464 * 4 + 1) * 1920];
    let mut compressed = vec![0; zlib_rs::compress_bound(rows.len())];
    let (compressed, code) =
        zlib_rs::compress_slice(&mut compressed, &rows, zlib_rs::DeflateConfig::new(4));
    if code != zlib_rs::ReturnCode::Ok {
        return Err(Error::Contract("TURZX overlay compression failed"));
    }
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend(WIDTH.to_be_bytes());
    ihdr.extend(HEIGHT.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    chunk(&mut png, *b"IHDR", &ihdr)?;
    chunk(&mut png, *b"sRGB", &[0])?;
    chunk(&mut png, *b"gAMA", &45_455_u32.to_be_bytes())?;
    let mut physical = Vec::with_capacity(9);
    physical.extend(3779_u32.to_be_bytes());
    physical.extend(3779_u32.to_be_bytes());
    physical.push(1);
    chunk(&mut png, *b"pHYs", &physical)?;
    chunk(&mut png, *b"IDAT", compressed)?;
    chunk(&mut png, *b"IEND", &[])?;
    Ok(png)
}

fn chunk(png: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) -> Result<(), Error> {
    let length =
        u32::try_from(data.len()).map_err(|_| Error::Contract("TURZX PNG chunk size overflow"))?;
    png.extend(length.to_be_bytes());
    png.extend(kind);
    png.extend(data);
    let crc = zlib_rs::crc32::crc32(zlib_rs::crc32::crc32(0, &kind), data);
    png.extend(crc.to_be_bytes());
    Ok(())
}
