//! #560 GHA 전용, std-only 의미 통제. Cargo/프로젝트 bootstrap을 실행하지 않는다.
//! verify는 이 파일의 일곱 고정 recipe와 -xx exec-only trace만 읽는다.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{c_char, c_int, CString};
use std::fs;
use std::io::{BufRead, BufReader};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::ptr;
use std::time::{Duration, Instant};

unsafe extern "C" {
    fn fork() -> c_int;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    fn execve(path: *const c_char, argv: *const *const c_char, env: *const *const c_char) -> c_int;
    fn execveat(
        fd: c_int,
        path: *const c_char,
        argv: *const *const c_char,
        env: *const *const c_char,
        flags: c_int,
    ) -> c_int;
    fn _exit(status: c_int) -> !;
}

fn need(ok: bool, message: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn identity(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn owned_read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(0x20000 | 0x800 | 0x80000)
        .open(path)
        .map_err(|e| e.to_string())?;
    let before = file.metadata().map_err(|e| e.to_string())?;
    need(
        before.is_file() && before.nlink() == 1 && before.len() <= limit,
        "owned input shape",
    )?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    need(
        bytes.len() as u64 == before.len()
            && identity(&before) == identity(&file.metadata().map_err(|e| e.to_string())?)
            && identity(&before)
                == identity(&fs::symlink_metadata(path).map_err(|e| e.to_string())?),
        "held input changed",
    )?;
    Ok(bytes)
}
fn proc_stat(pid: u32) -> Result<(String, u32, u32, u32, u64), String> {
    let raw = fs::read_to_string(format!("/proc/{pid}/stat")).map_err(|e| e.to_string())?;
    need(
        raw.len() <= 8192 && raw.starts_with(&format!("{pid} (")),
        "proc bound/PID",
    )?;
    let fields: Vec<_> = raw
        .rsplit_once(") ")
        .ok_or("proc comm")?
        .1
        .split_whitespace()
        .collect();
    need(fields.len() >= 20, "proc fields")?;
    let num = |at: usize| {
        fields[at]
            .parse::<u32>()
            .map_err(|_| "proc integer".to_string())
    };
    Ok((
        fields[0].into(),
        num(1)?,
        num(2)?,
        num(3)?,
        fields[19].parse().map_err(|_| "proc start")?,
    ))
}

// 메타데이터 명령이다. fork/exec/ptrace를 하지 않고 STOP 중인 실제 launch만 승인한다.
fn approve(args: &[std::ffi::OsString]) -> Result<(), String> {
    need(args.len() == 14, "approve arguments")?;
    let dir = Path::new(&args[2]);
    let before_dir = fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
    need(
        before_dir.is_dir()
            && before_dir.mode() & 0o777 == 0o700
            && fs::canonicalize(dir).map_err(|e| e.to_string())? == dir,
        "private launch root",
    )?;
    let ready_path = dir.join("ready");
    let ready = owned_read(&ready_path, 1024)?;
    let ready_meta = fs::symlink_metadata(&ready_path).map_err(|e| e.to_string())?;
    need(
        ready_meta.uid() == before_dir.uid() && ready_meta.mode() & 0o777 == 0o600,
        "ready owner/mode",
    )?;
    let text = std::str::from_utf8(&ready).map_err(|_| "ready utf8")?;
    let fields: Vec<_> = text.trim_end_matches('\n').split(' ').collect();
    need(
        text.ends_with('\n')
            && !text[..text.len() - 1].contains('\n')
            && fields.len() == 13
            && fields[..2] == ["COLD560_LAUNCH", "1"],
        "ready schema",
    )?;
    let u = |at: usize| {
        fields[at]
            .parse::<u64>()
            .map_err(|_| "ready integer".to_string())
    };
    let pid: u32 = u(2)?.try_into().map_err(|_| "tracer PID")?;
    let parent: u32 = u(6)?.try_into().map_err(|_| "parent PID")?;
    let leader: u32 = args[3]
        .to_str()
        .ok_or("leader utf8")?
        .parse()
        .map_err(|_| "leader PID")?;
    let leader_start: u64 = args[4]
        .to_str()
        .ok_or("leader start utf8")?
        .parse()
        .map_err(|_| "leader start")?;
    let tracer = proc_stat(pid)?;
    let timeout = proc_stat(parent)?;
    let owner = proc_stat(leader)?;
    need(
        tracer.0 == "T"
            && tracer.1 == parent
            && tracer.2 == leader
            && tracer.3 == leader
            && tracer.4 == u(3)?
            && !matches!(timeout.0.as_str(), "T" | "Z" | "X")
            && timeout.1 == leader
            && timeout.2 == leader
            && timeout.3 == leader
            && timeout.4 == u(7)?
            && !matches!(owner.0.as_str(), "Z" | "X")
            && owner.2 == leader
            && owner.3 == leader
            && owner.4 == leader_start
            && leader_start <= timeout.4
            && timeout.4 <= tracer.4,
        "fresh timeout chain",
    )?;
    for (who, field, expected) in [(pid, 4usize, &args[5]), (parent, 8usize, &args[6])] {
        let m = fs::metadata(format!("/proc/{who}/exe")).map_err(|e| e.to_string())?;
        need(
            m.is_file()
                && m.dev() == u(field)?
                && m.ino() == u(field + 1)?
                && expected.as_os_str().as_bytes() == format!("{}:{}", m.dev(), m.ino()).as_bytes(),
            "actual executable identity",
        )?;
    }
    for (who, expected) in [(pid, &args[7]), (parent, &args[8])] {
        let actual = fs::read(format!("/proc/{who}/cmdline")).map_err(|e| e.to_string())?;
        need(
            actual.len() <= 65536 && actual == owned_read(Path::new(expected), 65536)?,
            "exact launch argv",
        )?;
    }
    for (field, arg, length) in [(10, 9, 40), (11, 10, 64), (12, 11, 64)] {
        need(
            fields[field].len() == length
                && fields[field]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && args[arg].as_os_str().as_bytes() == fields[field].as_bytes(),
            "launch source/provider SHA binding",
        )?;
    }
    need(args[12] == "strict", "approve protocol")?;
    let fault = args[13].to_str().ok_or("fault utf8")?;
    // 실제 승인 후 provider 자기검증이 거부해야 할 네 controlled 오염 경계.
    let approved = match fault {
        "none" => ready.clone(),
        "missing" => Vec::new(),
        "wrong-start" | "wrong-parent" | "wrong-provider" => {
            let mut changed: Vec<String> = fields.iter().map(|v| (*v).into()).collect();
            let at = match fault {
                "wrong-start" => 3,
                "wrong-parent" => 6,
                _ => 5,
            };
            changed[at] = (u(at)?.checked_add(1).ok_or("fault overflow")?).to_string();
            (changed.join(" ") + "\n").into_bytes()
        }
        _ => return Err("unknown approval fault".into()),
    };
    need(
        identity(&before_dir) == identity(&fs::symlink_metadata(dir).map_err(|e| e.to_string())?)
            && proc_stat(pid)? == tracer
            && proc_stat(parent)?.1 == leader
            && proc_stat(parent)?.4 == timeout.4,
        "launch changed before approval",
    )?;
    // ready was created before this directory snapshot; approval is the only new entry.
    if fault != "missing" {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(0x20000 | 0x800 | 0x80000)
            .open(dir.join("approve"))
            .map_err(|e| e.to_string())?;
        file.write_all(&approved).map_err(|e| e.to_string())?;
        need(
            file.metadata().map_err(|e| e.to_string())?.uid() == before_dir.uid(),
            "approval owner",
        )?;
    }
    need(proc_stat(pid)? == tracer, "tracer changed before release")?;
    println!(
        "COLD560_APPROVE tracer={pid} parent={parent} leader={leader} start={} fault={fault}",
        tracer.4
    );
    Ok(())
}

struct Exec {
    path: CString,
    args: Vec<CString>,
    env: [CString; 1],
    argv: Vec<*const c_char>,
    envp: [*const c_char; 2],
}
impl Exec {
    fn new(path: &[u8], scenario: &str) -> Self {
        let args: Vec<_> = [path, b"leaf", scenario.as_bytes()]
            .map(|s| CString::new(s).unwrap())
            .into();
        let env = [CString::new("COLD560_FIXTURE=1").unwrap()];
        let argv = args
            .iter()
            .map(|s| s.as_ptr())
            .chain([ptr::null()])
            .collect();
        let envp = [env[0].as_ptr(), ptr::null()];
        Self {
            path: CString::new(path).unwrap(),
            args,
            env,
            argv,
            envp,
        }
    }
    // fork child는 FFI와 _exit 외 allocation/lock/stdio를 사용하지 않는다.
    unsafe fn enter(&self, at: bool) -> c_int {
        if at {
            unsafe {
                execveat(
                    -100,
                    self.path.as_ptr(),
                    self.argv.as_ptr(),
                    self.envp.as_ptr(),
                    0,
                )
            }
        } else {
            unsafe { execve(self.path.as_ptr(), self.argv.as_ptr(), self.envp.as_ptr()) }
        }
    }
}

fn wait(child: c_int) -> Result<(), String> {
    let mut status = -1;
    need(
        child > 0 && unsafe { waitpid(child, &mut status, 0) } == child && status == 0,
        "child exit/wait",
    )
}

fn scenario(name: &str, receipt: &Path) -> Result<(), String> {
    let own = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = Exec::new(own.as_os_str().as_bytes(), name);
    // Keep CString owners live over vfork and nonleader exec.
    need(
        target.args.len() == 3 && target.env.len() == 1,
        "exec storage",
    )?;
    match name {
        "fork" => {
            let child = unsafe { fork() };
            if child == 0 {
                unsafe {
                    target.enter(false);
                    _exit(91)
                }
            }
            wait(child)?;
        }
        "vfork" => {
            // Rust extern ABI에 returns_twice를 가정하지 않는다. 실제 VFORK event는
            // helper가 strace --debug 원문에서 별도 요구하며 부재 시 FAIL한다.
            let status = std::process::Command::new(&own)
                .args(["leaf", "vfork"])
                .env_clear()
                .env("COLD560_FIXTURE", "1")
                .status()
                .map_err(|e| e.to_string())?;
            need(status.success(), "Command spawn/wait")?;
        }
        "thread" => {
            std::thread::spawn(|| 42)
                .join()
                .map_err(|_| "thread join")?;
        }
        "nonleader" => {
            // Construct pointers in the executing thread, without sharing raw pointers.
            let path = own.into_os_string().into_encoded_bytes();
            let join = std::thread::spawn(move || {
                let target = Exec::new(&path, "nonleader");
                unsafe {
                    target.enter(false);
                    _exit(92)
                }
            });
            join.join().map_err(|_| "nonleader exec")?;
            return Err("nonleader exec returned".into());
        }
        "execve" | "execveat" => {
            let missing = own
                .parent()
                .ok_or("fixture parent")?
                .join("missing-executable");
            let failed = Exec::new(missing.as_os_str().as_bytes(), name);
            let at = name == "execveat";
            let rc = unsafe { failed.enter(at) };
            need(
                rc == -1 && std::io::Error::last_os_error().raw_os_error() == Some(2),
                "failed exec ENOENT",
            )?;
            unsafe { target.enter(at) };
            return Err("successful exec returned".into());
        }
        "root-first" => {
            let child = unsafe { fork() };
            need(child >= 0, "root-first fork")?;
            if child > 0 {
                unsafe { _exit(0) }
            }
            // Synchronize on tracer's actual root-terminal event, not a sleep race.
            let end = Instant::now() + Duration::from_secs(5);
            loop {
                let bytes = fs::read(receipt).map_err(|e| e.to_string())?;
                need(bytes.len() <= 16384, "receipt bound")?;
                if bytes
                    .split(|b| *b == b'\n')
                    .any(|l| l.starts_with(b"COLD560_INHERIT root_exit pid="))
                {
                    break;
                }
                need(Instant::now() < end, "root exit not observed")?;
                std::thread::yield_now();
            }
            let grandchild = unsafe { fork() };
            if grandchild == 0 {
                unsafe {
                    target.enter(false);
                    _exit(93)
                }
            }
            wait(grandchild)?;
        }
        _ => return Err("unknown scenario".into()),
    }
    println!("COLD560_FIXTURE done {name}");
    Ok(())
}

// All C strings in the controlled trace are -xx. No ellipsis/unknown escape support.
struct Cursor<'a>(&'a [u8]);
impl Cursor<'_> {
    fn ws(&mut self) {
        while self.0.first() == Some(&b' ') {
            self.0 = &self.0[1..];
        }
    }
    fn eat(&mut self, token: &[u8]) -> bool {
        self.ws();
        if self.0.starts_with(token) {
            self.0 = &self.0[token.len()..];
            true
        } else {
            false
        }
    }
    fn token(&mut self, token: &[u8]) -> Result<(), String> {
        need(self.eat(token), "trace token")
    }
    fn string(&mut self) -> Result<Vec<u8>, String> {
        self.token(b"\"")?;
        let mut value = Vec::new();
        while !self.0.starts_with(b"\"") {
            need(
                self.0.starts_with(b"\\x") && self.0.len() >= 4,
                "hex Cstring",
            )?;
            let digits = std::str::from_utf8(&self.0[2..4]).map_err(|_| "hex utf8")?;
            let byte = u8::from_str_radix(digits, 16).map_err(|_| "hex byte")?;
            need(byte != 0, "NUL in exec string")?;
            value.push(byte);
            self.0 = &self.0[4..];
        }
        self.token(b"\"")?;
        Ok(value)
    }
    fn array(&mut self) -> Result<Vec<Vec<u8>>, String> {
        self.token(b"[")?;
        let mut result = Vec::new();
        if !self.eat(b"]") {
            loop {
                result.push(self.string()?);
                if self.eat(b"]") {
                    break;
                }
                self.token(b",")?;
            }
        }
        Ok(result)
    }
}

fn verify(
    trace: &Path,
    name: &str,
    own: &Path,
    envfile: &Path,
    receipt: &Path,
) -> Result<(), String> {
    let own = own.as_os_str().as_bytes().to_vec();
    let mut initial_env = fs::read(envfile).map_err(|e| e.to_string())?;
    need(
        initial_env.len() < 65536 && initial_env.pop() == Some(0),
        "initial env bound/NUL",
    )?;
    let initial_env: Vec<Vec<u8>> = initial_env.split(|b| *b == 0).map(<[u8]>::to_vec).collect();
    let receipt_arg = receipt.as_os_str().as_bytes().to_vec();
    let initial_args = vec![
        own.clone(),
        b"scenario".to_vec(),
        name.as_bytes().to_vec(),
        receipt_arg,
    ];
    let leaf_args = vec![own.clone(), b"leaf".to_vec(), name.as_bytes().to_vec()];
    let mut missing = Path::new(std::ffi::OsStr::from_bytes(&own))
        .parent()
        .ok_or("own parent")?
        .as_os_str()
        .as_bytes()
        .to_vec();
    missing.extend_from_slice(b"/missing-executable");
    let failed_args = vec![missing.clone(), b"leaf".to_vec(), name.as_bytes().to_vec()];
    let mut counts = [0usize; 3];
    let mut pending: BTreeMap<u32, (String, String)> = BTreeMap::new();
    let mut switched: BTreeMap<u32, u32> = BTreeMap::new();
    let mut initial_pid = None;
    let mut ended = BTreeSet::new();
    let mut physical = 0;
    let mut total = 0;
    let file = fs::File::open(trace).map_err(|e| e.to_string())?;
    need(
        file.metadata().map_err(|e| e.to_string())?.len() <= 8 * 1024 * 1024,
        "trace file bound",
    )?;
    for line in BufReader::new(file).split(b'\n') {
        let line = line.map_err(|e| e.to_string())?;
        physical += 1;
        total += line.len();
        need(
            physical <= 10000 && total <= 8 * 1024 * 1024 && line.len() <= 65536,
            "trace bounds",
        )?;
        let line = std::str::from_utf8(&line).map_err(|_| "trace utf8")?;
        let (pid, body) = line.split_once(char::is_whitespace).ok_or("PID/body")?;
        need(
            !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()),
            "PID digits",
        )?;
        let pid: u32 = pid.parse().map_err(|_| "PID range")?;
        need(pid > 0 && !ended.contains(&pid), "PID reused")?;
        let body = body.trim_start();
        if let Some(old) = body
            .strip_prefix("+++ superseded by execve in pid ")
            .and_then(|s| s.strip_suffix(" +++"))
        {
            let old: u32 = old.parse().map_err(|_| "superseded PID")?;
            need(
                name == "nonleader"
                    && initial_pid == Some(pid)
                    && switched.get(&old).is_none_or(|target| *target == pid),
                "superseded recipe/PID",
            )?;
            let head = pending.remove(&old).ok_or("superseded missing exec")?;
            need(
                pending.insert(pid, head).is_none(),
                "superseded pending conflict",
            )?;
            switched.remove(&old);
            continue;
        }
        if let Some((head, changed)) = body.rsplit_once(" <pid changed to ") {
            let target: u32 = changed
                .strip_suffix(" ...>")
                .ok_or("changed suffix")?
                .parse()
                .map_err(|_| "changed PID")?;
            let op = head.split_once('(').ok_or("changed opcode")?.0;
            need(
                name == "nonleader"
                    && initial_pid == Some(target)
                    && matches!(op, "execve" | "execveat")
                    && !pending.contains_key(&pid)
                    && switched.insert(pid, target).is_none(),
                "changed PID recipe",
            )?;
            pending.insert(pid, (op.into(), head.into()));
            continue;
        }
        let complete;
        let body = if let Some(tail) = body.strip_prefix("<... ") {
            let (op, tail) = tail.split_once(" resumed>").ok_or("resume grammar")?;
            let (saved_op, head) = pending.remove(&pid).ok_or("orphan resume")?;
            need(saved_op == op, "resume opcode/PID")?;
            complete = head + tail;
            complete.as_str()
        } else if let Some(head) = body.strip_suffix("<unfinished ...>") {
            let op = head.split_once('(').ok_or("unfinished opcode")?.0;
            need(
                matches!(op, "execve" | "execveat") && !pending.contains_key(&pid),
                "unfinished duplicate/opcode",
            )?;
            pending.insert(pid, (op.into(), head.into()));
            continue;
        } else {
            body
        };
        if body == "+++ exited with 0 +++" {
            need(!pending.contains_key(&pid), "exit pending")?;
            ended.insert(pid);
            continue;
        }
        if let Some(info) = body
            .strip_prefix("--- SIGCHLD {si_signo=SIGCHLD, ")
            .and_then(|s| s.strip_suffix("} ---"))
        {
            // SIGCHLD metadata only. Never treat it as an exec completion.
            let mut fields = info.split(", ");
            for key in [
                "si_code=CLD_EXITED",
                "si_pid=",
                "si_uid=",
                "si_status=0",
                "si_utime=",
                "si_stime=",
            ] {
                let field = fields.next().ok_or("SIGCHLD fields")?;
                if key.ends_with('=') {
                    let value = field.strip_prefix(key).ok_or("SIGCHLD key")?;
                    let digits = if let Some((digits, comment)) = value.split_once(" /* ") {
                        let number = comment.strip_suffix(" s */").ok_or("SIGCHLD comment")?;
                        let (a, b) = number.split_once('.').ok_or("SIGCHLD fraction")?;
                        need(
                            !a.is_empty()
                                && !b.is_empty()
                                && a.bytes().chain(b.bytes()).all(|b| b.is_ascii_digit()),
                            "SIGCHLD comment number",
                        )?;
                        digits
                    } else {
                        value
                    };
                    need(
                        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
                        "SIGCHLD number",
                    )?;
                } else {
                    need(field == key, "SIGCHLD value")?;
                }
            }
            need(fields.next().is_none(), "SIGCHLD extra")?;
            continue;
        }
        let mut c = Cursor(body.as_bytes());
        let at = if c.eat(b"execveat(") {
            c.token(b"AT_FDCWD,")?;
            true
        } else {
            c.token(b"execve(")?;
            false
        };
        let path = c.string()?;
        c.token(b",")?;
        let args = c.array()?;
        c.token(b",")?;
        let env = c.array()?;
        if at {
            c.token(b", 0")?;
        }
        c.token(b") =")?;
        let ok = if c.eat(b"0") {
            true
        } else {
            c.token(b"-1 ENOENT (No such file or directory)")?;
            false
        };
        c.ws();
        need(c.0.is_empty(), "trace trailing data")?;
        if path == own && args == initial_args && env == initial_env && ok && !at {
            need(initial_pid.replace(pid).is_none(), "duplicate initial exec")?;
            counts[0] += 1;
        } else if path == own
            && args == leaf_args
            && env == [b"COLD560_FIXTURE=1".to_vec()]
            && ok
            && at == (name == "execveat")
        {
            counts[1] += 1;
        } else if path == missing
            && args == failed_args
            && env == [b"COLD560_FIXTURE=1".to_vec()]
            && !ok
            && at == (name == "execveat")
            && matches!(name, "execve" | "execveat")
        {
            counts[2] += 1;
        } else {
            return Err("unlisted exec path/argv/env/result".into());
        }
    }
    need(
        pending.is_empty()
            && switched.is_empty()
            && total > 0
            && fs::read(trace).map_err(|e| e.to_string())?.last() == Some(&b'\n'),
        "trace unresolved/truncated EOF",
    )?;
    let expected = [
        1,
        usize::from(name != "thread"),
        usize::from(matches!(name, "execve" | "execveat")),
    ];
    need(counts == expected, "exec exact counts")?;
    println!(
        "COLD560_SEMANTIC {name} initial={} leaf={} failed={} PASS",
        counts[0], counts[1], counts[2]
    );
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let result = match args.get(1).and_then(|s| s.to_str()) {
        Some("approve") => approve(&args),
        Some("ready") if args.len() == 3 => owned_read(&Path::new(&args[2]).join("ready"), 1024)
            .and_then(|bytes| {
                std::io::stdout()
                    .write_all(&bytes)
                    .map_err(|e| e.to_string())
            }),
        Some("leaf")
            if args.len() == 3 && std::env::var("COLD560_FIXTURE").as_deref() == Ok("1") =>
        {
            println!("COLD560_FIXTURE leaf {}", args[2].to_string_lossy());
            Ok(())
        }
        Some("scenario") if args.len() == 4 => {
            scenario(args[2].to_str().unwrap_or(""), Path::new(&args[3]))
        }
        Some("verify") if args.len() == 7 => verify(
            Path::new(&args[2]),
            args[3].to_str().unwrap_or(""),
            Path::new(&args[4]),
            Path::new(&args[5]),
            Path::new(&args[6]),
        ),
        _ => Err("exact fixture command required".into()),
    };
    if let Err(error) = result {
        eprintln!("cold560 fixture: {error}");
        std::process::exit(1);
    }
}
