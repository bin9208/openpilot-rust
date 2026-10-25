//! GHA SDK rustc로만 새로 빌드할 기록 전용 ELF. SSH/네트워크/자손 실행은 없다.
use std::{
    env,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::PathBuf,
};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        env::var_os("DEVELOPER_SSH_ENTRY_MARKERS").ok_or("marker directory missing")?,
    );
    let nonce = env::var("DEVELOPER_SSH_ENTRY_NONCE")?;
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err("invalid recorder nonce".into());
    }
    let m = fs::symlink_metadata(&root)?;
    let uid = fs::metadata("/proc/self")?.uid();
    if !root.is_absolute()
        || root.canonicalize()? != root
        || !m.is_dir()
        || m.uid() != uid
        || m.mode() & 0o777 != 0o700
        || fs::read_dir(&root)?.next().is_some()
    {
        return Err("marker directory must be empty owned private0700".into());
    }
    let mut stat = String::new();
    fs::File::open("/proc/self/stat")?
        .take(8193)
        .read_to_string(&mut stat)?;
    if stat.len() > 8192 {
        return Err("process identity exceeds bound".into());
    }
    let (_, tail) = stat.rsplit_once(") ").ok_or("process identity malformed")?;
    let fields: Vec<_> = tail.split_whitespace().collect();
    if fields.len() < 20 {
        return Err("process identity truncated".into());
    }
    let ppid: u32 = fields[1].parse()?;
    let pgid: u32 = fields[2].parse()?;
    let session: u32 = fields[3].parse()?;
    let start: u64 = fields[19].parse()?;
    let mut bytes = b"DSSHENTRY1\0".to_vec();
    for value in [std::process::id(), ppid, pgid, session] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(start.to_le_bytes());
    fn blob(output: &mut Vec<u8>, value: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        if value.len() > 8192 {
            return Err("recorder field exceeds bound".into());
        }
        output.extend(u32::try_from(value.len())?.to_le_bytes());
        output.extend(value);
        Ok(())
    }
    blob(&mut bytes, nonce.as_bytes())?;
    blob(&mut bytes, env::current_exe()?.as_os_str().as_bytes())?;
    blob(&mut bytes, env::current_dir()?.as_os_str().as_bytes())?;
    let args: Vec<_> = env::args_os().collect();
    if args.is_empty() || args.len() > 32 {
        return Err("recorder argc exceeds bound".into());
    }
    bytes.extend(u32::try_from(args.len())?.to_le_bytes());
    for arg in args {
        blob(&mut bytes, arg.as_bytes())?;
    }
    if bytes.len() > 65536 {
        return Err("recorder exceeds bound".into());
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(root.join("exec.bin"))?;
    file.write_all(&bytes)?;
    file.flush()?;
    let after = fs::symlink_metadata(&root)?;
    if (m.dev(), m.ino(), m.mode(), m.uid(), m.gid())
        != (
            after.dev(),
            after.ino(),
            after.mode(),
            after.uid(),
            after.gid(),
        )
        || root.canonicalize()? != root
    {
        return Err("marker directory identity changed".into());
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("developer-ssh entry recorder: {error}");
        std::process::exit(91);
    }
}
