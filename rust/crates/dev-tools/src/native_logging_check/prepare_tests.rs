use super::*;
use flate2::{write::GzEncoder, Compression};
use std::os::unix::fs::PermissionsExt;

// Tiny authored archives exercise rejection paths, never qualify R11 data.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
fn small_zip(rows: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut central = Vec::new();
    for (name, body, mode) in rows {
        let start = data.len() as u32;
        let crc = crc32(body);
        let size = body.len() as u32;
        let name_len = name.len() as u16;
        data.extend_from_slice(b"PK\x03\x04");
        for value in [20_u16, 8, 0, 0, 0x21] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        for _ in 0..3 {
            data.extend_from_slice(&0_u32.to_le_bytes());
        }
        data.extend_from_slice(&name_len.to_le_bytes());
        data.extend_from_slice(&0_u16.to_le_bytes());
        data.extend_from_slice(name.as_bytes());
        data.extend_from_slice(body);
        data.extend_from_slice(b"PK\x07\x08");
        for value in [crc, size, size] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        central.extend_from_slice(b"PK\x01\x02");
        for value in [0x0314_u16, 20, 8, 0, 0, 0x21] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, size, size] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name_len, 0, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        central.extend_from_slice(&(mode << 16).to_le_bytes());
        central.extend_from_slice(&start.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let start = data.len() as u32;
    let size = central.len() as u32;
    data.extend_from_slice(&central);
    data.extend_from_slice(b"PK\x05\x06");
    for value in [0_u16, 0, rows.len() as u16, rows.len() as u16] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&size.to_le_bytes());
    data.extend_from_slice(&start.to_le_bytes());
    data.extend_from_slice(&0_u16.to_le_bytes());
    data
}
fn small_tar(rows: &[(&str, tar::EntryType, &[u8])]) -> (Vec<u8>, TarShape) {
    let mut builder = tar::Builder::new(Vec::new());
    let mut payload = 0;
    for (name, kind, body) in rows {
        let mut header = tar::Header::new_ustar();
        header.set_mode(0o600);
        header.set_entry_type(*kind);
        header.set_size(body.len() as u64);
        // Raw names deliberately reach our checks instead of Builder's sanitizer.
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_cksum();
        builder.append(&header, *body).unwrap();
        payload += body.len() as u64;
    }
    let plain = builder.into_inner().unwrap();
    let shape = TarShape {
        count: rows.len(),
        payload,
        plain: plain.len() as u64,
    };
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&plain).unwrap();
    (encoder.finish().unwrap(), shape)
}
#[test]
fn mapping_preserves_seventy_tar_thirty_seven_zip_and_one_source() {
    let (direct, nested) = mappings().unwrap();
    assert_eq!(direct.len(), 70);
    assert_eq!(nested.len(), 37);
    assert_eq!(MEMBERS.len(), 108);
    assert_eq!(MEMBERS.iter().map(|(_, size, _)| size).sum::<u64>(), 700585);
    assert_eq!(direct["proof/historical-api.json"], "historical/api.json");
    assert_eq!(
        direct["proof/commands/protocol-context/result.json"],
        "recovered/protocol-result.json"
    );
    assert_eq!(
        nested["native/console-0/source/packets.json"],
        "historical/console-0/packets.json"
    );
    assert!(!direct
        .values()
        .chain(nested.values())
        .any(|name| name == REFERENCE));
    assert_eq!(SOURCE_FILES.len(), 19);
}
#[test]
fn archive_pin_rejects_wrong_size_hash_last_byte_and_truncation() {
    let bytes = b"authored inert control";
    let sha = format!("{:x}", Sha256::digest(bytes));
    pinned(bytes, bytes.len() as u64, &sha).unwrap();
    for (data, size, pin) in [
        (bytes.as_slice(), bytes.len() as u64 + 1, sha.as_str()),
        (bytes.as_slice(), bytes.len() as u64, RECOVERY_ZIP),
        (&bytes[..bytes.len() - 1], bytes.len() as u64, sha.as_str()),
    ] {
        assert_eq!(
            pinned(data, size, pin).unwrap_err().to_string(),
            "logging archive/member size or SHA256 differs"
        );
    }
    let mut changed = bytes.to_vec();
    *changed.last_mut().unwrap() ^= 1;
    assert!(pinned(&changed, bytes.len() as u64, &sha).is_err());
}
#[test]
fn zip_rejects_duplicate_unsafe_and_nonregular_members() {
    assert_eq!(crc32(b"123456789"), 0xcbf43926);
    let good = small_zip(&[("safe", b"body", 0o100600)]);
    let selected = BTreeMap::from([("safe".to_owned(), 4)]);
    assert_eq!(
        read_zip(&good, 1, 0, 4, &selected).unwrap()["safe"],
        b"body"
    );
    let duplicate = small_zip(&[("safe", b"body", 0o100600), ("safe", b"body", 0o100600)]);
    assert_eq!(
        read_zip(&duplicate, 2, 0, 8, &selected)
            .unwrap_err()
            .to_string(),
        "duplicate logging ZIP member"
    );
    for name in ["../safe", "/safe", "a/./safe", "a\\safe", "safe/"] {
        assert_eq!(
            read_zip(&small_zip(&[(name, b"body", 0o100600)]), 1, 0, 4, &selected)
                .unwrap_err()
                .to_string(),
            "invalid retained member path"
        );
    }
    for mode in [0o120777, 0o040700, 0o010600] {
        assert_eq!(
            read_zip(&small_zip(&[("safe", b"body", mode)]), 1, 0, 4, &selected)
                .unwrap_err()
                .to_string(),
            "logging ZIP type/flags/method differs"
        );
    }
}
#[test]
fn zip_rejects_extra_truncated_crc_and_local_path_or_descriptor_changes() {
    let good = small_zip(&[("safe", b"body", 0o100600)]);
    let selected = BTreeMap::from([("safe".to_owned(), 4)]);
    let mut extra = good.clone();
    extra.push(0);
    assert!(read_zip(&extra, 1, 0, 4, &selected).is_err());
    assert!(read_zip(&good[..good.len() - 1], 1, 0, 4, &selected).is_err());
    let mut local = good.clone();
    local[30] = b'x';
    assert_eq!(
        read_zip(&local, 1, 0, 4, &selected)
            .unwrap_err()
            .to_string(),
        "logging ZIP local header/path differs"
    );
    let mut crc = good.clone();
    crc[34] ^= 1;
    assert!(read_zip(&crc, 1, 0, 4, &selected).is_err());
    let mut descriptor = good.clone();
    descriptor[42] ^= 1;
    assert_eq!(
        read_zip(&descriptor, 1, 0, 4, &selected)
            .unwrap_err()
            .to_string(),
        "logging ZIP descriptor differs"
    );
    assert_eq!(
        read_zip(&good, 1, 0, 3, &selected).unwrap_err().to_string(),
        "logging ZIP expansion exceeds bound"
    );
    assert_eq!(
        read_zip(&good, 1, 0, 4, &BTreeMap::from([("other".to_owned(), 4)]))
            .unwrap_err()
            .to_string(),
        "selected logging ZIP member missing"
    );
}
#[test]
fn tar_rejects_links_special_paths_duplicates_and_missing_selection() {
    let selected = BTreeMap::from([("safe".to_owned(), 4)]);
    let (good, shape) = small_tar(&[("safe", tar::EntryType::Regular, b"body")]);
    assert_eq!(read_tar(&good, shape, &selected).unwrap()["safe"], b"body");
    for kind in [
        tar::EntryType::Symlink,
        tar::EntryType::Link,
        tar::EntryType::Directory,
        tar::EntryType::Fifo,
    ] {
        let (data, shape) = small_tar(&[("safe", kind, b"")]);
        assert_eq!(
            read_tar(&data, shape, &selected).unwrap_err().to_string(),
            "logging TAR must contain only USTAR regular members"
        );
    }
    for name in ["../safe", "/safe", "a/./safe"] {
        let (data, shape) = small_tar(&[(name, tar::EntryType::Regular, b"body")]);
        assert_eq!(
            read_tar(&data, shape, &selected).unwrap_err().to_string(),
            "invalid retained member path"
        );
    }
    let (duplicate, duplicate_shape) = small_tar(&[
        ("safe", tar::EntryType::Regular, b"body"),
        ("safe", tar::EntryType::Regular, b"body"),
    ]);
    assert_eq!(
        read_tar(&duplicate, duplicate_shape, &selected)
            .unwrap_err()
            .to_string(),
        "duplicate/excessive logging TAR member"
    );
    assert!(read_tar(&good, TarShape { count: 2, ..shape }, &selected).is_err());
    assert!(read_tar(
        &good,
        TarShape {
            count: 1,
            payload: 4,
            plain: shape.plain
        },
        &BTreeMap::from([("other".to_owned(), 4)])
    )
    .is_err());
}
#[test]
fn gzip_rejects_corruption_concatenation_and_trailing_bytes() {
    let selected = BTreeMap::from([("safe".to_owned(), 4)]);
    let (good, shape) = small_tar(&[("safe", tar::EntryType::Regular, b"body")]);
    let mut extra = good.clone();
    extra.push(0);
    assert_eq!(
        read_tar(&extra, shape, &selected).unwrap_err().to_string(),
        "trailing/concatenated logging gzip stream"
    );
    let mut concatenated = good.clone();
    concatenated.extend_from_slice(&good);
    assert_eq!(
        read_tar(&concatenated, shape, &selected)
            .unwrap_err()
            .to_string(),
        "trailing/concatenated logging gzip stream"
    );
    let mut corrupt = good.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(read_tar(&corrupt, shape, &selected).is_err());
}
#[test]
fn prepare_rejects_occupied_context_before_archive_or_source_reads() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().canonicalize().unwrap();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
    let output = parent.join("context");
    fs::write(&output, b"foreign bytes").unwrap();
    let args = [
        "--root".into(),
        parent.clone().into_os_string(),
        "--archive".into(),
        parent.join("missing.zip").into_os_string(),
        "--output".into(),
        output.clone().into_os_string(),
    ];
    assert_eq!(
        run(&args).unwrap_err().to_string(),
        "logging evidence output already exists"
    );
    assert_eq!(fs::read(output).unwrap(), b"foreign bytes");
    assert!(!parent.join("context.prepare.json").exists());
}
#[test]
fn prepare_rejects_archive_symlink_hardlink_and_occupied_receipt() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().canonicalize().unwrap();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
    let target = parent.join("target.zip");
    let archive = parent.join("archive.zip");
    let output = parent.join("context");
    fs::write(&target, b"authored inert archive control").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let args = [
        "--root".into(),
        parent.clone().into_os_string(),
        "--archive".into(),
        archive.clone().into_os_string(),
        "--output".into(),
        output.clone().into_os_string(),
    ];
    symlink(&target, &archive).unwrap();
    assert!(run(&args).is_err());
    fs::remove_file(&archive).unwrap();
    fs::hard_link(&target, &archive).unwrap();
    assert_eq!(
        run(&args).unwrap_err().to_string(),
        "retained input type/owner/mode/links/size invalid"
    );
    assert!(!output.exists());
    let receipt = parent.join("context.prepare.json");
    fs::write(&receipt, b"foreign receipt").unwrap();
    assert_eq!(
        run(&args).unwrap_err().to_string(),
        "logging evidence output already exists"
    );
    assert_eq!(fs::read(receipt).unwrap(), b"foreign receipt");
    assert_eq!(fs::read(target).unwrap(), b"authored inert archive control");
}

#[test]
fn context_writer_rejects_symlink_parent_existing_leaf_and_unsafe_name() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().canonicalize().unwrap();
    let context = parent.join("context");
    let outside = parent.join("outside");
    fs::create_dir(&context).unwrap();
    fs::create_dir(&outside).unwrap();
    let target = outside.join("sentinel");
    fs::write(&target, b"foreign bytes").unwrap();
    symlink(&outside, context.join("alias")).unwrap();
    let directory = File::open(&context).unwrap();
    assert!(write_bytes(&directory, "alias/sentinel", b"replace").is_err());
    fs::write(context.join("occupied"), b"keep").unwrap();
    assert!(write_bytes(&directory, "occupied", b"replace").is_err());
    assert_eq!(
        write_bytes(&directory, "../outside/sentinel", b"replace")
            .unwrap_err()
            .to_string(),
        "invalid retained member path"
    );
    assert_eq!(fs::read(context.join("occupied")).unwrap(), b"keep");
    assert_eq!(fs::read(target).unwrap(), b"foreign bytes");
}
