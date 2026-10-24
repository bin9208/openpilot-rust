//! Older standalone stress comparison keeps strict stderr and arbitrary runners.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    camera_sensor_check::lexical,
    camera_stress_check::{self, Generic},
    qa_process::{NativeRunner, Runner},
    seal::Result,
};
use std::{
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
};
pub struct Options {
    pub reference: PathBuf,
    pub native: PathBuf,
    pub parser: PathBuf,
    pub fixture: PathBuf,
    pub output: PathBuf,
    pub runner: Vec<OsString>,
    pub profile: OsString,
}
pub fn check(
    opt: &Options,
    runner: &mut dyn Runner,
    clock: &mut dyn FnMut() -> Result<u128>,
    printed: &mut dyn Write,
) -> Result<()> {
    let common = camera_stress_check::Options {
        reference: opt.reference.clone(),
        native: opt.native.clone(),
        parser: opt.parser.clone(),
        fixture: opt.fixture.clone(),
        output: opt.output.clone(),
        qemu: None,
        sysroot: None,
    };
    camera_stress_check::check_profile(
        &common,
        runner,
        clock,
        printed,
        Some(Generic {
            runner: &opt.runner,
            profile: &opt.profile,
        }),
    )
}
fn is_option(arg: &std::ffi::OsStr) -> bool {
    let Some(s) = arg.to_str() else {
        let bytes = arg.as_encoded_bytes();
        return bytes.starts_with(b"-") && !bytes.contains(&b' ');
    };
    if s == "-" || s.contains(' ') || !s.starts_with('-') {
        return false;
    }
    let n = &s.strip_suffix('\n').unwrap_or(s)[1..];
    let numeric = !n.is_empty()
        && (crate::argparse_scan::decimal(n)
            || n.split_once('.').is_some_and(|(a, b)| {
                !b.is_empty()
                    && crate::argparse_scan::decimal(a)
                    && crate::argparse_scan::decimal(b)
            }));
    !numeric
}
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let mut args_out = vec![];
    let mut runner = vec![];
    let mut i = 0;
    let mut terminated = false;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            terminated = true;
        }
        let bytes = arg.as_encoded_bytes();
        let name = bytes.split(|&c| c == b'=').next().unwrap();
        if !terminated && name.len() > 3 && b"--runner".starts_with(name) {
            runner.clear();
            args_out.push("--runner".into());
            if let Some(offset) = bytes.iter().position(|&c| c == b'=') {
                use std::os::unix::ffi::OsStringExt;
                runner.push(OsString::from_vec(bytes[offset + 1..].to_vec()));
                i += 1;
                continue;
            }
            i += 1;
            while i < args.len() && !is_option(&args[i]) {
                runner.push(args[i].clone());
                i += 1;
            }
        } else {
            args_out.push(arg.clone());
            i += 1;
        }
    }
    let parsed = scan_os(
        &args_out,
        &[
            OptionSpec::value("--reference", true),
            OptionSpec::value("--native", true),
            OptionSpec::value("--parser", true),
            OptionSpec::value("--fixture", true),
            OptionSpec::value("--output", true),
            OptionSpec::flag("--runner", None),
            OptionSpec::value("--profile", false),
        ],
    )?;
    if parsed.help {
        println!("Compare original camera stress policy and parser with a runner");
        return Ok(());
    }
    let p = |key| lexical(Path::new(parsed.values.get(key).unwrap()));
    let options = Options {
        reference: p("--reference"),
        native: p("--native"),
        parser: p("--parser"),
        fixture: p("--fixture"),
        output: p("--output"),
        runner,
        profile: parsed
            .values
            .get("--profile")
            .cloned()
            .unwrap_or_else(|| "host".into()),
    };
    let mut clock = || {
        let t = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
        Ok(t.tv_sec as u128 * 1_000_000_000 + t.tv_nsec as u128)
    };
    check(
        &options,
        &mut NativeRunner,
        &mut clock,
        &mut std::io::stdout().lock(),
    )
}
