use crate::{
    manifest::{file_checksum, unlink_if_present, Partition, Paths},
    transport, Error, Observer,
};
use std::{
    fs::{self, OpenOptions},
    io::{Seek, Write},
    path::PathBuf,
};

pub fn download(
    paths: &Paths,
    partition: &Partition,
    observer: &mut dyn Observer,
) -> Result<Option<PathBuf>, Error> {
    let Some(hash) = &partition.compressed_hash else {
        return Ok(None);
    };
    fs::create_dir_all(&paths.cache)?;
    let final_path = paths
        .cache
        .join(format!("{}-{hash}.img.xz", partition.name));
    let partial = paths
        .cache
        .join(format!("{}-{hash}.img.xz.part", partition.name));
    if final_path.is_file() {
        if file_checksum(&final_path)?.eq_ignore_ascii_case(hash) {
            return Ok(Some(final_path));
        }
        observer.log(
            "warning",
            &format!("Discarding invalid cached {} image", partition.name),
        );
        fs::remove_file(&final_path)?;
    }
    let mut offset = if partial.is_file() {
        partial.metadata()?.len()
    } else {
        0
    };
    let mut expected = partition.compressed_size;
    if expected.is_some_and(|size| offset >= size) {
        if Some(offset) == expected && file_checksum(&partial)?.eq_ignore_ascii_case(hash) {
            fs::rename(&partial, &final_path)?;
            return Ok(Some(final_path));
        }
        observer.log(
            "warning",
            &format!("Discarding invalid partial {} image", partition.name),
        );
        fs::remove_file(&partial)?;
        offset = 0;
    }
    observer.log(
        "info",
        &format!("Downloading {} cache from byte {offset}", partition.name),
    );
    observer.progress(&format!("Connecting for {}", partition.name), 0);
    let mut response = transport::get(&partition.url, offset, 10, 60)?;
    if offset != 0 && response.status != 206 {
        observer.log(
            "warning",
            &format!(
                "Server ignored resume for {}; restarting the cached download",
                partition.name
            ),
        );
        offset = 0;
    }
    if expected.is_none() {
        expected = response
            .content_length
            .map(|size| {
                size.parse::<u64>()
                    .map_err(|_| Error::Contract("invalid Content-Length".into()))
                    .and_then(|size| {
                        size.checked_add(offset)
                            .ok_or_else(|| Error::Contract("download length overflow".into()))
                    })
            })
            .transpose()?;
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create(true)
        .append(offset != 0)
        .truncate(offset == 0)
        .open(&partial)?;
    let mut buffer = vec![0; 1024 * 1024];
    let mut last = -1;
    loop {
        let size = transport::body_read(&mut response.body, &mut buffer)?;
        if size == 0 {
            break;
        }
        output.write_all(&buffer[..size])?;
        if let Some(total) = expected.filter(|v| *v != 0) {
            let progress = (output.stream_position()? as f64 / total as f64 * 100.) as i64;
            if progress != last {
                last = progress;
                observer.progress(&format!("Downloading {}", partition.name), progress);
            }
        }
    }
    output.flush()?;
    output.sync_all()?;
    drop(output);
    let actual_size = partial.metadata()?.len();
    if let Some(total) = expected.filter(|size| *size != actual_size) {
        return Err(Error::connection(format!(
            "Incomplete {} download: {actual_size} of {total} bytes",
            partition.name
        )));
    }
    let actual_hash = file_checksum(&partial)?;
    if !actual_hash.eq_ignore_ascii_case(hash) {
        unlink_if_present(&partial)?;
        return Err(Error::connection(format!(
            "Compressed {} cache hash mismatch: {actual_hash}",
            partition.name
        )));
    }
    fs::rename(partial, &final_path)?;
    Ok(Some(final_path))
}
