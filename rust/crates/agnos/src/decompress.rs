use crate::{transport, Error};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
use xz2::stream::{Action, Status, Stream};

pub struct Decompressor {
    source: Box<dyn Read + Send>,
    status: Option<u16>,
    stream: Stream,
    input: Vec<u8>,
    position: usize,
    eof: bool,
    pub hash: Sha256,
}
impl Decompressor {
    pub fn new(url: &str, cache: Option<&Path>) -> Result<Self, Error> {
        match cache {
            Some(path) => Self::from_source(Box::new(File::open(path)?), None),
            None => {
                let response = transport::get_raw(&transport::agent(10, 60), url, 0)?;
                Self::from_source(response.body, Some(response.status))
            }
        }
    }
    pub fn from_source(source: Box<dyn Read + Send>, status: Option<u16>) -> Result<Self, Error> {
        Ok(Self {
            source,
            status,
            stream: Stream::new_auto_decoder(u64::MAX, 0)?,
            input: Vec::new(),
            position: 0,
            eof: false,
            hash: Sha256::new(),
        })
    }
    pub fn read(&mut self, length: usize) -> Result<Vec<u8>, Error> {
        let mut output = vec![0; length];
        let mut used = 0;
        while used < length && !self.eof {
            let before_in = self.stream.total_in();
            let before_out = self.stream.total_out();
            let status = self.stream.process(
                &self.input[self.position..],
                &mut output[used..],
                Action::Run,
            )?;
            let consumed = (self.stream.total_in() - before_in) as usize;
            let produced = (self.stream.total_out() - before_out) as usize;
            self.position += consumed;
            used += produced;
            if status == Status::StreamEnd {
                self.eof = true;
            }
            if consumed == 0 && produced == 0 && !self.eof {
                if self.position != self.input.len() {
                    return Err(Error::Contract("LZMA decoder made no progress".into()));
                }
                self.input.resize(1024 * 1024, 0);
                let size = if let Some(status) = self.status {
                    transport::check_status(status)?;
                    transport::body_read(&mut self.source, &mut self.input)?
                } else {
                    self.source.read(&mut self.input)?
                };
                self.input.truncate(size);
                self.position = 0;
                if size == 0 {
                    self.eof = true;
                }
            }
        }
        output.truncate(used);
        self.hash.update(&output);
        Ok(output)
    }
    pub fn hash_hex(&self) -> String {
        format!("{:x}", self.hash.clone().finalize())
    }
    fn exact<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        self.read(N)?
            .try_into()
            .map_err(|_| Error::Contract("truncated sparse header".into()))
    }
}
pub fn extract(
    source: &mut Decompressor,
    sparse: bool,
    mut write: impl FnMut(&[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    if !sparse {
        loop {
            let bytes = source.read(1024 * 1024)?;
            if bytes.is_empty() {
                return Ok(());
            }
            write(&bytes)?;
        }
    }
    if u32::from_ne_bytes(source.exact()?) != 0xed26ff3a
        || u16::from_ne_bytes(source.exact()?) != 1
        || u16::from_ne_bytes(source.exact()?) != 0
    {
        return Err(Error::Contract("invalid sparse image header".into()));
    }
    source.read(4)?;
    let block_size = u32::from_ne_bytes(source.exact()?);
    source.read(4)?;
    let chunks = u32::from_ne_bytes(source.exact()?);
    source.read(4)?;
    for _ in 0..chunks {
        let header: [u8; 12] = source.exact()?;
        let kind = u16::from_ne_bytes([header[0], header[1]]);
        let blocks = u32::from_ne_bytes([header[4], header[5], header[6], header[7]]);
        match kind {
            0xcac1 => {
                let size = (blocks as usize)
                    .checked_mul(block_size as usize)
                    .ok_or_else(|| Error::Contract("sparse length overflow".into()))?;
                write(&source.read(size)?)?;
            }
            0xcac2 => {
                let filler = source.read(4)?.repeat(block_size as usize / 4);
                for _ in 0..blocks {
                    write(&filler)?;
                }
            }
            0xcac3 => write(&[])?,
            _ => return Err(Error::Contract("Unhandled sparse chunk type".into())),
        }
    }
    Ok(())
}
