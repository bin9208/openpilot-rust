//! Inert R11 data admission; historical executables and Python are never run.
use super::*;
use flate2::bufread::GzDecoder;
use serde::Serialize;
use std::io::Cursor;

const ZIP_BYTES: u64 = 5_896_947;
const GZIP_BYTES: u64 = 5_895_693;
const R3_BYTES: u64 = 817_106;
const R3_SHA: &str = "6eddbcdae19c54cd6c19ad740ae1b881b59cb16261e9f9d7bd3878382dc1b6a5";
const RECEIPT_SHA: &str = "1ca298fe80f758e834a7ad2c53fbfa48f213c868705c89c650a22cd08f5c7384";
const TAR_LIMIT: u64 = 64 * 1024 * 1024;
const INNER_ZIP: &str = "proof/historical-r3.zip";
const REFERENCE: &str = "reference/native_logging_reference.cc";

fn pinned(bytes: &[u8], size: u64, sha: &str) -> Result<()> {
    require(
        bytes.len() as u64 == size && format!("{:x}", Sha256::digest(bytes)) == sha,
        "logging archive/member size or SHA256 differs",
    )
}

// As in the existing wheel stagers, count raw CD records before relying on the
// ZIP crate's name index. Also bind local headers, data descriptors and EOF.
fn part(bytes: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    bytes
        .get(at..at.checked_add(len).ok_or("logging ZIP offset overflow")?)
        .ok_or_else(|| "truncated logging ZIP record".into())
}
fn word(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(part(bytes, at, 2)?.try_into()?))
}
fn dword(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(part(bytes, at, 4)?.try_into()?))
}
fn zip_directory(bytes: &[u8], count: usize, method: u16) -> Result<Vec<String>> {
    let end = bytes
        .len()
        .checked_sub(22)
        .ok_or("truncated logging ZIP EOF")?;
    require(
        part(bytes, end, 4)? == b"PK\x05\x06"
            && word(bytes, end + 4)? == 0
            && word(bytes, end + 6)? == 0
            && usize::from(word(bytes, end + 8)?) == count
            && usize::from(word(bytes, end + 10)?) == count
            && word(bytes, end + 20)? == 0,
        "logging ZIP count/disk/EOF differs",
    )?;
    let start = usize::try_from(dword(bytes, end + 16)?)?;
    require(
        start.checked_add(usize::try_from(dword(bytes, end + 12)?)?) == Some(end),
        "logging ZIP central directory extent differs",
    )?;
    let mut cursor = start;
    let mut local = 0;
    let mut names = Vec::new();
    let mut unique = BTreeSet::new();
    for _ in 0..count {
        require(
            part(bytes, cursor, 4)? == b"PK\x01\x02",
            "logging ZIP CD signature differs",
        )?;
        let flags = word(bytes, cursor + 8)?;
        let crc = dword(bytes, cursor + 16)?;
        let packed = usize::try_from(dword(bytes, cursor + 20)?)?;
        let size = dword(bytes, cursor + 24)?;
        let name_size = usize::from(word(bytes, cursor + 28)?);
        let extra = usize::from(word(bytes, cursor + 30)?);
        let comment = usize::from(word(bytes, cursor + 32)?);
        let mode = dword(bytes, cursor + 38)? >> 16;
        let raw_name = part(bytes, cursor + 46, name_size)?;
        let name = std::str::from_utf8(raw_name)?.to_owned();
        member_name(&name)?;
        require(unique.insert(name.clone()), "duplicate logging ZIP member")?;
        require(
            flags == 8
                && word(bytes, cursor + 10)? == method
                && word(bytes, cursor + 34)? == 0
                && matches!(mode & 0o170000, 0 | 0o100000)
                && dword(bytes, cursor + 38)? & 0x10 == 0,
            "logging ZIP type/flags/method differs",
        )?;
        require(
            usize::try_from(dword(bytes, cursor + 42)?)? == local
                && part(bytes, local, 4)? == b"PK\x03\x04"
                && word(bytes, local + 6)? == flags
                && word(bytes, local + 8)? == method
                && usize::from(word(bytes, local + 26)?) == name_size
                && part(bytes, local + 30, name_size)? == raw_name,
            "logging ZIP local header/path differs",
        )?;
        let local_extra = usize::from(word(bytes, local + 28)?);
        let data = local + 30 + name_size + local_extra;
        let descriptor = data
            .checked_add(packed)
            .ok_or("logging ZIP size overflow")?;
        require(
            part(bytes, descriptor, 4)? == b"PK\x07\x08"
                && dword(bytes, descriptor + 4)? == crc
                && usize::try_from(dword(bytes, descriptor + 8)?)? == packed
                && dword(bytes, descriptor + 12)? == size,
            "logging ZIP descriptor differs",
        )?;
        for (offset, expected) in [(14, crc), (18, packed as u32), (22, size)] {
            let value = dword(bytes, local + offset)?;
            require(
                value == 0 || value == expected,
                "logging ZIP local size/CRC differs",
            )?;
        }
        local = descriptor + 16;
        require(local <= start, "logging ZIP data overlaps directory")?;
        cursor += 46 + name_size + extra + comment;
        require(cursor <= end, "logging ZIP central record exceeds EOF")?;
        names.push(name);
    }
    require(
        local == start && cursor == end,
        "logging ZIP extra bytes/records",
    )?;
    Ok(names)
}
fn read_zip(
    bytes: &[u8],
    count: usize,
    method: u16,
    expanded: u64,
    selected: &BTreeMap<String, u64>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let names = zip_directory(bytes, count, method)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    require(archive.len() == count, "logging ZIP indexed count differs")?;
    let mut result = BTreeMap::new();
    let mut total = 0_u64;
    let mut buffer = [0; 65536];
    for (index, name) in names.iter().enumerate() {
        let mut entry = archive.by_index(index)?;
        require(
            entry.name_raw() == name.as_bytes() && entry.is_file(),
            "logging ZIP indexed member differs",
        )?;
        total = total
            .checked_add(entry.size())
            .ok_or("logging ZIP expansion overflow")?;
        require(total <= expanded, "logging ZIP expansion exceeds bound")?;
        let keep = selected.get(name);
        if let Some(size) = keep {
            require(entry.size() == *size, "selected logging ZIP size differs")?;
        }
        let mut data = Vec::new();
        let mut copied = 0_u64;
        loop {
            let n = entry.read(&mut buffer)?; // Reading EOF checks every member's CRC.
            if n == 0 {
                break;
            }
            copied += n as u64;
            require(
                copied <= entry.size() && copied <= expanded,
                "logging ZIP member grew",
            )?;
            if keep.is_some() {
                data.extend_from_slice(&buffer[..n]);
            }
        }
        require(copied == entry.size(), "truncated logging ZIP member")?;
        if keep.is_some() {
            result.insert(name.clone(), data);
        }
    }
    require(
        result.len() == selected.len(),
        "selected logging ZIP member missing",
    )?;
    Ok(result)
}

#[derive(Clone, Copy)]
struct TarShape {
    count: usize,
    payload: u64,
    plain: u64,
}
fn read_tar(
    gzip: &[u8],
    shape: TarShape,
    selected: &BTreeMap<String, u64>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let decoder = GzDecoder::new(Cursor::new(gzip));
    let mut archive = tar::Archive::new(decoder.take(TAR_LIMIT + 1));
    let mut names = BTreeSet::new();
    let mut result = BTreeMap::new();
    let mut payload = 0_u64;
    let mut buffer = [0; 65536];
    for row in archive.entries()?.raw(true) {
        let mut entry = row?;
        require(
            entry.header().entry_type() == tar::EntryType::Regular
                && entry.header().as_ustar().is_some(),
            "logging TAR must contain only USTAR regular members",
        )?;
        let name = std::str::from_utf8(&entry.path_bytes())?.to_owned();
        member_name(&name)?;
        require(
            names.insert(name.clone()) && names.len() <= shape.count,
            "duplicate/excessive logging TAR member",
        )?;
        payload = payload
            .checked_add(entry.size())
            .ok_or("logging TAR size overflow")?;
        require(
            payload <= shape.payload && payload <= TAR_LIMIT,
            "logging TAR payload exceeds bound",
        )?;
        let keep = selected.get(&name);
        if let Some(size) = keep {
            require(entry.size() == *size, "selected logging TAR size differs")?;
        }
        let mut data = Vec::new();
        let mut copied = 0_u64;
        loop {
            let n = entry.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            require(copied <= entry.size(), "logging TAR member grew")?;
            if keep.is_some() {
                data.extend_from_slice(&buffer[..n]);
            }
        }
        require(copied == entry.size(), "truncated logging TAR member")?;
        if keep.is_some() {
            result.insert(name, data);
        }
    }
    let mut remaining = archive.into_inner();
    loop {
        let n = remaining.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        require(
            buffer[..n].iter().all(|b| *b == 0),
            "nonzero logging TAR tail",
        )?;
    }
    require(
        remaining.limit() > 0
            && TAR_LIMIT + 1 - remaining.limit() == shape.plain
            && names.len() == shape.count
            && payload == shape.payload
            && result.len() == selected.len(),
        "logging TAR count/size/selection differs",
    )?;
    let compressed = remaining.into_inner().into_inner();
    require(
        compressed.position() == gzip.len() as u64,
        "trailing/concatenated logging gzip stream",
    )?;
    Ok(result)
}

type Mapping = BTreeMap<String, String>;
fn mappings() -> Result<(Mapping, Mapping)> {
    let mut direct = BTreeMap::new();
    let mut nested = BTreeMap::new();
    for (name, _, _) in MEMBERS {
        if *name == REFERENCE {
            continue;
        }
        let (target, path) = if *name == "historical/api.json" {
            (&mut direct, "proof/historical-api.json".to_owned())
        } else if let Some(rest) = name.strip_prefix("historical/") {
            let path = if rest.starts_with("console-") {
                let (case, file) = rest.split_once('/').ok_or("logging console path missing")?;
                format!("native/{case}/source/{file}")
            } else {
                format!("native/{rest}")
            };
            (&mut nested, path)
        } else if let Some(rest) = name.strip_prefix("recovered/") {
            let path = if rest == "protocol-result.json" {
                "proof/commands/protocol-context/result.json".to_owned()
            } else if rest.starts_with("console-") || rest.starts_with("rate/") {
                let (case, file) = rest
                    .split_once('/')
                    .ok_or("logging recovered path missing")?;
                format!("proof/fresh-context/{case}/source/{file}")
            } else if let Some(transport) = rest.strip_prefix("transport/") {
                format!("proof/fresh-context/transport/source/{transport}")
            } else if rest.starts_with("providers.") {
                format!("proof/fresh-context/{rest}")
            } else {
                format!("proof/{rest}")
            };
            (&mut direct, path)
        } else {
            return Err("unmapped retained member".into());
        };
        require(
            target.insert(path, (*name).to_owned()).is_none(),
            "duplicate logging import mapping",
        )?;
    }
    require(
        direct.len() == 70 && nested.len() == 37 && MEMBERS.len() == 108,
        "logging import mapping count differs",
    )?;
    Ok((direct, nested))
}
fn sizes(mapping: &BTreeMap<String, String>) -> Result<BTreeMap<String, u64>> {
    mapping
        .iter()
        .map(|(archive, context)| {
            let (_, size, _) = MEMBERS
                .iter()
                .find(|(name, _, _)| *name == context.as_str())
                .ok_or("logging import pin missing")?;
            Ok((archive.clone(), *size))
        })
        .collect()
}
fn import(archive: &[u8], reference: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    pinned(archive, ZIP_BYTES, RECOVERY_ZIP)?;
    let outer = read_zip(
        archive,
        2,
        0,
        8 * 1024 * 1024,
        &BTreeMap::from([
            ("full-proof.tar.gz".to_owned(), GZIP_BYTES),
            ("recovery-receipt.json".to_owned(), 972),
        ]),
    )?;
    let gzip = &outer["full-proof.tar.gz"];
    pinned(gzip, GZIP_BYTES, PROOF_GZIP)?;
    pinned(&outer["recovery-receipt.json"], 972, RECEIPT_SHA)?;
    let (direct, nested) = mappings()?;
    let mut chosen = sizes(&direct)?;
    require(
        chosen.insert(INNER_ZIP.to_owned(), R3_BYTES).is_none(),
        "nested logging ZIP overlaps data",
    )?;
    let mut tar = read_tar(
        gzip,
        TarShape {
            count: 788,
            payload: 41_796_503,
            plain: 42_362_880,
        },
        &chosen,
    )?;
    let inner = tar.remove(INNER_ZIP).ok_or("nested logging ZIP missing")?;
    pinned(&inner, R3_BYTES, R3_SHA)?;
    let zipped = read_zip(&inner, 2185, 8, 4 * 1024 * 1024, &sizes(&nested)?)?;
    let mut data = BTreeMap::new();
    for (mapping, bytes) in [(&direct, tar), (&nested, zipped)] {
        for (path, raw) in bytes {
            let name = mapping.get(&path).ok_or("logging selected path unlisted")?;
            require(
                data.insert(name.clone(), raw).is_none(),
                "duplicate logging context member",
            )?;
        }
    }
    data.insert(REFERENCE.to_owned(), reference.to_owned());
    let mut total = 0;
    for (name, size, sha) in MEMBERS {
        pinned(
            data.get(*name).ok_or("logging context member missing")?,
            *size,
            sha,
        )?;
        total += *size;
    }
    require(
        data.len() == 108 && total == 700585,
        "logging context roster/bytes differs",
    )?;
    Ok(data)
}

#[derive(Serialize)]
struct PreparedPin {
    sha256: String,
    size: u64,
}
#[derive(Serialize)]
struct PreparedIndex {
    members: BTreeMap<String, PreparedPin>,
    proof_gzip_sha256: &'static str,
    recovery_artifact: u64,
    recovery_run: u64,
    recovery_zip_sha256: &'static str,
    schema: u32,
    source_files: BTreeMap<String, PreparedPin>,
}
fn write_bytes(root: &File, name: &str, bytes: &[u8]) -> Result<()> {
    member_name(name)?;
    let mut parent = root.try_clone()?;
    let mut parts = name.split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_some() {
            match rustix::fs::mkdirat(
                &parent,
                part,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR | rustix::fs::Mode::XUSR,
            ) {
                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                Err(error) => return Err(error.into()),
            }
            parent = File::from(rustix::fs::openat(
                &parent,
                part,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::DIRECTORY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )?);
            let meta = parent.metadata()?;
            require(
                meta.uid() == fs::metadata("/proc/self")?.uid() && meta.mode() & 0o077 == 0,
                "logging context directory is not owned private0700",
            )?;
        } else {
            let mut file = File::from(rustix::fs::openat(
                &parent,
                part,
                rustix::fs::OFlags::WRONLY
                    | rustix::fs::OFlags::CREATE
                    | rustix::fs::OFlags::EXCL
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            )?);
            file.write_all(bytes)?;
        }
    }
    Ok(())
}
fn validate_context(index: &Path, root: &Path) -> Result<Value> {
    let inputs = Inputs::open(index, root)?;
    let _corpus = Corpus::load(&inputs)?;
    inputs.unchanged()
}
fn run(args: &[OsString]) -> Result<Value> {
    let parsed = scan_os_required_first(
        args,
        &[
            OptionSpec::value("--archive", true),
            OptionSpec::value("--root", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        println!("usage: check-native-logging prepare-context --root ROOT --archive R11_ZIP --output ABSENT_CONTEXT");
        return Ok(Value::Null);
    }
    let output = PathBuf::from(&parsed.values["--output"]);
    absent_output(&output)?;
    let leaf = output
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("logging context name invalid")?;
    let receipt = output.with_file_name(format!("{leaf}.prepare.json"));
    absent_output(&receipt)?;
    let parent = output.parent().ok_or("logging context parent missing")?;
    let (publication_parent, publication_anchors) = walk(parent, true)?;
    let publication_before = node(&publication_parent.metadata()?);
    let root = PathBuf::from(&parsed.values["--root"]);
    let archive = PathBuf::from(&parsed.values["--archive"]);
    absolute_path(&root)?;
    absolute_path(&archive)?;
    let (source_root, source_anchors) = walk(&root, true)?;
    let root_before = node(&source_root.metadata()?);
    require(
        source_root.metadata()?.uid() == fs::metadata("/proc/self")?.uid()
            && source_root.metadata()?.mode() & 0o022 == 0,
        "logging source root owner/mode invalid",
    )?;
    let provider = HeldBinary::open(&std::env::current_exe()?)?;
    let executing = fs::metadata("/proc/self/exe")?;
    let held = provider.file.metadata()?;
    require(
        (executing.dev(), executing.ino()) == (held.dev(), held.ino()),
        "logging preparation provider is not the running ELF",
    )?;
    let provider_before = provider.snapshot()?;
    let archive = HeldFile::open(&archive, 8 * 1024 * 1024)?;
    let archive_before = archive.snapshot()?;
    let reference = HeldFile::open(&root.join(CPP_SITE), 2581)?;
    let reference_before = reference.snapshot()?;
    let mut sources = Vec::new();
    let mut source_files = BTreeMap::new();
    for name in SOURCE_FILES {
        let file = HeldFile::open(&root.join(name), 2 * 1024 * 1024)?;
        let before = file.snapshot()?;
        source_files.insert(
            (*name).to_owned(),
            PreparedPin {
                sha256: before["sha256"]
                    .as_str()
                    .ok_or("logging source SHA missing")?
                    .to_owned(),
                size: before["size"]
                    .as_u64()
                    .ok_or("logging source size missing")?,
            },
        );
        sources.push((file, before));
    }
    let data = import(&archive.bytes()?, &reference.bytes()?)?;
    let members = MEMBERS
        .iter()
        .map(|(name, size, sha)| {
            (
                (*name).to_owned(),
                PreparedPin {
                    sha256: (*sha).to_owned(),
                    size: *size,
                },
            )
        })
        .collect();
    // Struct field order and BTreeMap keys fix canonical sorted JSON, regardless
    // of serde_json's preserve_order feature union. Historical raw stays intact.
    let index = PreparedIndex {
        members,
        proof_gzip_sha256: PROOF_GZIP,
        recovery_artifact: 12477566277,
        recovery_run: 39894886676,
        recovery_zip_sha256: RECOVERY_ZIP,
        schema: 1,
        source_files,
    };
    let mut index_bytes = serde_json::to_vec_pretty(&index)?;
    index_bytes.push(b'\n');
    require(
        index_bytes.len() <= 128 * 1024,
        "prepared logging index exceeds bound",
    )?;
    let temp = tempfile::Builder::new()
        .prefix(".logging-context-")
        .tempdir_in(parent)?;
    let (directory, _) = walk(temp.path(), true)?;
    rustix::fs::fchmod(
        &directory,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR | rustix::fs::Mode::XUSR,
    )?;
    let directory_before = node(&directory.metadata()?);
    let (directory, directory_anchors) = walk(temp.path(), true)?;
    require(
        node(&directory.metadata()?) == directory_before,
        "logging preparation temporary root replaced",
    )?;
    for (name, bytes) in &data {
        write_bytes(&directory, name, bytes)?;
    }
    write_bytes(&directory, "context.json", &index_bytes)?;
    validate_context(&temp.path().join("context.json"), &root)?;
    let verify = || -> Result<()> {
        require(
            provider.snapshot()? == provider_before,
            "logging preparation current ELF changed",
        )?;
        require(
            archive.snapshot()? == archive_before && reference.snapshot()? == reference_before,
            "logging preparation archive/reference changed",
        )?;
        for (file, before) in &sources {
            require(
                file.snapshot()? == *before,
                "logging preparation current source changed",
            )?;
        }
        let (now, anchors) = walk(&root, true)?;
        require(
            anchors == source_anchors && node(&now.metadata()?) == root_before,
            "logging preparation source root changed",
        )?;
        let (now, anchors) = walk(parent, true)?;
        require(
            anchors == publication_anchors && node(&now.metadata()?) == publication_before,
            "logging preparation publication parent changed",
        )
    };
    verify()?;
    let (now, anchors) = walk(temp.path(), true)?;
    require(
        anchors == directory_anchors && node(&now.metadata()?) == directory_before,
        "logging preparation temporary root changed",
    )?;
    absent_output(&output)?;
    rustix::fs::renameat_with(
        &publication_parent,
        temp.path()
            .file_name()
            .ok_or("logging temporary name missing")?,
        &publication_parent,
        output.file_name().ok_or("logging context name missing")?,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    let _published = temp.keep();
    let inputs = validate_context(&output.join("context.json"), &root)?;
    verify()?;
    let result = json!({"schema":1,"result":"prepared inert logging context only",
        "context":output.join("context.json"),"context_sha256":format!("{:x}",Sha256::digest(&index_bytes)),
        "context_bytes":index_bytes.len(),"members":108,"member_bytes":700585,"current_source_files":19,
        "archive_members":107,"r11_tar_members":70,"nested_r3_members":37,"fixed_current_reference_source_members":1,
        "recovery_run":39894886676_u64,"recovery_artifact":12477566277_u64,
        "recovery_zip_sha256":RECOVERY_ZIP,"proof_gzip_sha256":PROOF_GZIP,
        "archive_before":archive_before,"archive_after":archive.snapshot()?,
        "reference_before":reference_before,"reference_after":reference.snapshot()?,
        "provider_before":provider_before,"provider_after":provider.snapshot()?,
        "validated_inputs":inputs,"historical_executable_executions":0,"python_executions":0,
        "current_native_functional_acceptance":false,"historical_pass_inherited":false,
        "complete_cargo_dso_closure":false,"github_artifact_authentication_external":true,
        "R4_acceptance":false,"lifecycle_reuse_granted":false,"retirement_credit":false,
        "limits":["same-UID visible checks; no hostile same-UID or privileged mutation exclusion",
            "source19 is an import roster; prepare implementation and whole source/ELF/DSO/SDK seals remain caller-owned"]});
    save(
        &publication_parent,
        receipt
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("logging receipt name invalid")?,
        &result,
    )?;
    Ok(result)
}
pub(in crate::native_logging_check) fn selected(args: &[OsString]) -> bool {
    args.first().is_some_and(|arg| arg == "prepare-context")
}
pub(in crate::native_logging_check) fn cli(args: &[OsString]) -> Result<()> {
    let result = run(args)?;
    if !result.is_null() {
        println!("{result}");
    }
    Ok(())
}

#[cfg(test)]
#[path = "prepare_tests.rs"]
mod tests;
