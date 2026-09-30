//! Image-only casync paths used by AGNOS; directory/tar extraction remains outside this port.
use crate::{
    decompress::Decompressor,
    image,
    manifest::{partition_path, Partition, Paths},
    transport, Error, Observer,
};
use sha2::{Digest, Sha512_256};
use std::{
    fs::{File, OpenOptions},
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::Path,
};

pub use crate::casync_index::{dictionary, parse, parse_bytes, Chunk};
enum Reader {
    File(File),
    Remote {
        store: String,
        agent: transport::Client,
    },
}
struct Source {
    name: &'static str,
    reader: Reader,
    chunks: std::collections::HashMap<[u8; 32], Chunk>,
}
fn decompress(bytes: Vec<u8>) -> Result<Vec<u8>, Error> {
    let mut source = Decompressor::from_source(Box::new(Cursor::new(bytes)), None)?;
    let mut out = Vec::new();
    loop {
        let data = source.read(1024 * 1024)?;
        if data.is_empty() {
            break;
        }
        out.extend(data);
    }
    Ok(out)
}
impl Reader {
    fn read(&mut self, chunk: &Chunk, observer: &mut dyn Observer) -> Result<Vec<u8>, Error> {
        match self {
            Self::File(file) => {
                file.seek(SeekFrom::Start(chunk.offset))?;
                let mut data = Vec::new();
                file.take(chunk.length).read_to_end(&mut data)?;
                Ok(data)
            }
            Self::Remote { store, agent } => {
                let sha = chunk
                    .sha
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                let path = Path::new(store)
                    .join(&sha[..4])
                    .join(format!("{sha}.cacnk"));
                let bytes = if path.is_file() {
                    std::fs::read(path)?
                } else {
                    let url = path
                        .to_str()
                        .ok_or_else(|| Error::Contract("chunk URL is not UTF-8".into()))?;
                    let mut response = None;
                    for attempt in 0..3 {
                        match transport::all(agent, url) {
                            Ok(value) => {
                                response = Some(value);
                                break;
                            }
                            Err(error) => {
                                if attempt == 2 {
                                    return Err(error);
                                }
                                observer.sleep(60);
                            }
                        }
                    }
                    let (status, bytes) =
                        response.ok_or_else(|| Error::Contract("chunk download failed".into()))?;
                    transport::check_status(status)?;
                    bytes
                };
                decompress(bytes)
            }
        }
    }
}
fn reconstruct(
    target: &[Chunk],
    sources: &mut [Source],
    out_path: &Path,
    observer: &mut dyn Observer,
    name: &str,
    total: u64,
) -> Result<Vec<(&'static str, u64)>, Error> {
    let mut out = OpenOptions::new()
        .read(true)
        .write(true)
        .create(!out_path.exists())
        .truncate(false)
        .open(out_path)?;
    let mut stats: Vec<(&'static str, u64)> = Vec::new();
    let mut done = 0;
    let mut last = 0;
    for chunk in target {
        let mut found = false;
        for source in &mut *sources {
            if let Some(stored) = source.chunks.get(&chunk.sha) {
                let data = source.reader.read(stored, observer)?;
                if u64::try_from(data.len()).ok() != Some(chunk.length)
                    || Sha512_256::digest(&data)[..] != chunk.sha
                {
                    continue;
                }
                out.seek(SeekFrom::Start(chunk.offset))?;
                out.write_all(&data)?;
                if let Some((_, bytes)) = stats.iter_mut().find(|(name, _)| *name == source.name) {
                    *bytes += chunk.length;
                } else {
                    stats.push((source.name, chunk.length));
                }
                done += chunk.length;
                let progress = (done as f64 / total as f64 * 100.) as i64;
                if progress != last {
                    last = progress;
                    observer.progress(&format!("Installing {name}"), progress);
                }
                found = true;
                break;
            }
        }
        if !found {
            return Err(Error::Contract(
                "Desired chunk not found in provided stores".into(),
            ));
        }
    }
    Ok(stats)
}
pub fn extract_image(
    paths: &Paths,
    slot: u32,
    partition: &Partition,
    observer: &mut dyn Observer,
) -> Result<(), Error> {
    let path = partition_path(paths, slot, partition)?;
    let path_text = path
        .to_str()
        .ok_or_else(|| Error::Contract("partition path is not UTF-8".into()))?;
    let last = path_text
        .chars()
        .last()
        .ok_or_else(|| Error::Contract("empty partition path".into()))?;
    let seed_path = format!(
        "{}{}",
        &path_text[..path_text.len() - last.len_utf8()],
        if last == 'a' { "b" } else { "a" }
    );
    let target = parse(
        partition
            .casync_caibx
            .as_deref()
            .ok_or_else(|| Error::Contract("missing casync_caibx".into()))?,
    )?;
    let mut sources = Vec::new();
    let seed = (|| {
        let hash = image::raw_hash(Path::new(&seed_path), partition.size()?)?;
        let url = format!(
            "{}{name}-{hash}.caibx",
            paths.caibx_url,
            name = partition.name
        );
        observer.log("info", &format!("casync fetching {url}"));
        // The source opens the seed reader before fetching its index.
        let reader = Reader::File(File::open(&seed_path)?);
        match parse(&url) {
            Ok(chunks) => Ok(Some(Source {
                name: "seed",
                reader,
                chunks: dictionary(&chunks),
            })),
            Err(Error::Request { .. }) => {
                observer.log("error", &format!("casync failed to load {url}"));
                Ok(None)
            }
            Err(error) => Err(error),
        }
    })();
    match seed {
        Ok(Some(source)) => sources.push(source),
        Ok(None) => {}
        Err(_) => observer.log("exception", "casync failed to hash seed partition"),
    }
    sources.push(Source {
        name: "target",
        reader: Reader::File(File::open(&path)?),
        chunks: dictionary(&target),
    });
    sources.push(Source {
        name: "remote",
        reader: Reader::Remote {
            store: partition
                .casync_store
                .clone()
                .ok_or_else(|| Error::Contract("missing casync_store".into()))?,
            agent: transport::agent(60, 60),
        },
        chunks: dictionary(&target),
    });
    let stats = reconstruct(
        &target,
        &mut sources,
        &path,
        observer,
        &partition.name,
        partition.size()?,
    )?;
    observer.log(
        "error",
        &format!(
            "casync done {{{}}}",
            stats
                .iter()
                .map(|(name, size)| format!("\"{name}\": {size}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    rustix::fs::sync();
    if !image::verify(paths, slot, partition, true)? {
        return Err(Error::Contract(format!(
            "Raw hash mismatch '{}'",
            partition.raw_hash()?.to_lowercase()
        )));
    }
    Ok(())
}
