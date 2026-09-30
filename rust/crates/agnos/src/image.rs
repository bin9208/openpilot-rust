use crate::{
    cache,
    decompress::{self, Decompressor},
    manifest::{partition_path, unlink_if_present, Partition, Paths},
    Error, Observer,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

pub fn raw_hash(path: &Path, size: u64) -> Result<String, Error> {
    let mut input = OpenOptions::new().read(true).write(true).open(path)?;
    let mut hash = Sha256::new();
    let mut remaining = size;
    let mut buffer = vec![0; 1024 * 1024];
    while remaining > 0 {
        let desired = remaining.min(buffer.len() as u64) as usize;
        let mut used = 0;
        while used < desired {
            let read = input.read(&mut buffer[used..desired])?;
            if read == 0 {
                break;
            }
            used += read;
        }
        hash.update(&buffer[..used]);
        remaining -= desired as u64;
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn verify(paths: &Paths, slot: u32, partition: &Partition, force: bool) -> Result<bool, Error> {
    let path = partition_path(paths, slot, partition)?;
    let (Some(size), Some(hash)) = (partition.size, partition.hash_raw.as_deref()) else {
        return Ok(false);
    };
    if force || partition.full_check {
        return Ok(raw_hash(&path, size)?.eq_ignore_ascii_case(hash));
    }
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    file.seek(SeekFrom::Start(partition.size()?))?;
    let mut bytes = Vec::new();
    file.take(64).read_to_end(&mut bytes)?;
    Ok(bytes == partition.raw_hash()?.to_lowercase().as_bytes())
}
pub fn clear_hash(paths: &Paths, slot: u32, partition: &Partition) -> Result<(), Error> {
    let mut output = File::create(partition_path(paths, slot, partition)?)?;
    output.seek(SeekFrom::Start(partition.size()?))?;
    output.write_all(&[0; 64])?;
    rustix::fs::sync();
    Ok(())
}
pub fn compressed(
    paths: &Paths,
    slot: u32,
    partition: &Partition,
    observer: &mut dyn Observer,
) -> Result<(), Error> {
    let path = partition_path(paths, slot, partition)?;
    let cache = cache::download(paths, partition, observer)?;
    let mut source = Decompressor::new(&partition.url, cache.as_deref())?;
    let mut output = File::create(path)?;
    let mut hash = Sha256::new();
    let mut last = 0;
    decompress::extract(&mut source, partition.sparse, |bytes| {
        hash.update(bytes);
        output.write_all(bytes)?;
        let progress = (output.stream_position()? as f64 / partition.size()? as f64 * 100.) as i64;
        if progress != last {
            last = progress;
            observer.progress(&format!("Installing {}", partition.name), progress);
        }
        Ok(())
    })?;
    let actual = format!("{:x}", hash.finalize());
    if !actual.eq_ignore_ascii_case(partition.raw_hash()?) {
        return Err(Error::Contract(format!("Raw hash mismatch '{actual}'")));
    }
    if !source.hash_hex().eq_ignore_ascii_case(&partition.hash) {
        return Err(Error::Contract("Uncompressed hash mismatch".into()));
    }
    if output.stream_position()? != partition.size()? {
        return Err(Error::Contract("Uncompressed size mismatch".into()));
    }
    rustix::fs::sync();
    drop(output);
    drop(source);
    if let Some(path) = cache {
        unlink_if_present(&path)?;
    }
    Ok(())
}
pub fn flash(
    paths: &Paths,
    slot: u32,
    partition: &Partition,
    standalone: bool,
    observer: &mut dyn Observer,
) -> Result<(), Error> {
    observer.log(
        "info",
        &format!("Downloading and writing {}", partition.name),
    );
    observer.progress(&format!("Checking {}", partition.name), 0);
    if verify(paths, slot, partition, false)? {
        observer.log("info", &format!("Already flashed {}", partition.name));
        return Ok(());
    }
    if !partition.full_check {
        clear_hash(paths, slot, partition)?;
    }
    if partition.casync_caibx.is_some() && !standalone {
        crate::casync::extract_image(paths, slot, partition, observer)?;
    } else {
        compressed(paths, slot, partition, observer)?;
    }
    if !partition.full_check {
        let mut output = File::create(partition_path(paths, slot, partition)?)?;
        output.seek(SeekFrom::Start(partition.size()?))?;
        output.write_all(partition.raw_hash()?.to_lowercase().as_bytes())?;
    }
    Ok(())
}
