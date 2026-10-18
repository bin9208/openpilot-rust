//! Mechanical raw raylib process used by the independent application reference.
use crate::{seal::Result, ui_forms_input::Canvas};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};
pub struct Raw {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    closed: bool,
}
impl Raw {
    pub fn open(helper: &Path, library: &Path) -> Result<Self> {
        let mut child = Command::new(helper)
            .arg(library)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = child.stdin.take().ok_or("application raw stdin missing")?;
        let output = BufReader::new(
            child
                .stdout
                .take()
                .ok_or("application raw stdout missing")?,
        );
        let mut raw = Self {
            child,
            input,
            output,
            closed: false,
        };
        raw.reply()?;
        Ok(raw)
    }
    fn reply(&mut self) -> Result<String> {
        let mut line = String::new();
        self.output.read_line(&mut line)?;
        if line == "OK\n" {
            return Ok(String::new());
        }
        if let Some(value) = line.strip_prefix("OK ") {
            return Ok(value.trim_end().into());
        }
        if let Some(value) = line.trim_end().strip_prefix("ERR ") {
            let bytes = (0..value.len())
                .step_by(2)
                .map(|at| {
                    Ok(u8::from_str_radix(
                        value
                            .get(at..at + 2)
                            .ok_or("malformed application raw error")?,
                        16,
                    )?)
                })
                .collect::<Result<Vec<_>>>()?;
            return Err(String::from_utf8_lossy(&bytes).into_owned().into());
        }
        Err(format!("application raw helper ended or invalid reply {line:?}").into())
    }
    pub fn request(&mut self, command: &str) -> Result<String> {
        writeln!(self.input, "{command}")?;
        self.input.flush()?;
        self.reply()
    }
    pub fn close(&mut self) -> Result<()> {
        self.request("quit")?;
        let status = self.child.wait()?;
        self.closed = true;
        if status.success() {
            Ok(())
        } else {
            Err("application raw cleanup failed".into())
        }
    }
}
impl Canvas for Raw {
    fn call(&mut self, command: &str) -> Result<()> {
        if self.request(command)?.is_empty() {
            Ok(())
        } else {
            Err("unexpected application raw payload".into())
        }
    }
    fn measure(&mut self, text: &str, size: f64) -> Result<f64> {
        Ok(self.measure_vector(text, size)?.0)
    }
    fn measure_vector(&mut self, text: &str, size: f64) -> Result<(f64, f64)> {
        let reply = self.request(&format!(
            "measure {} {size}",
            crate::ui_forms_input::hex(text)
        ))?;
        let values = reply
            .split_whitespace()
            .map(str::parse::<f32>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if values.len() != 2 {
            return Err("application font measurement payload".into());
        }
        Ok((f64::from(values[0]), f64::from(values[1])))
    }
}
impl Drop for Raw {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
/// Produce a separate helper without modifying the Forms boundary or importing
/// candidate graphics policy. The pinned replacement point rejects source drift.
pub fn build(output: &Path, raylib: &Path) -> Result<PathBuf> {
    let source = include_str!("../tests/fixtures/ui-forms/raw-raylib.cc");
    if format!("{:x}", Sha256::digest(source.as_bytes()))
        != "216030c03d807c46ad51e2b843185038bd6ee1dc3dbc49f8246cb115823d97b1"
    {
        return Err(
            "shared mechanical raylib boundary changed; review application extensions".into(),
        );
    }
    let marker = "} else\n        throw std::runtime_error(\"unknown forms command\");";
    if source.matches(marker).count() != 1 {
        return Err("application raw dispatch marker changed".into());
    }
    let additions = include_str!("../tests/fixtures/ui-application-lifecycle/raw-input.inc");
    let replacement =
        format!("{additions} else throw std::runtime_error(\"unknown application raw command\");");
    fs::create_dir_all(output)?;
    let path = output.join("raw-application.cc");
    let generated = source.replace(marker, &replacement);
    let member = "  Font font{};";
    let cleanup = "    if (window) {";
    if generated.matches(member).count() != 1 || generated.matches(cleanup).count() != 1 {
        return Err("application raw resource marker changed".into());
    }
    let generated = generated.replace(member, "  Image application_image{};\n  Font font{};");
    let generated = generated.replace(cleanup, "    if (application_image.data) get<void (*)(Image)>(\"UnloadImage\")(application_image);\n    if (window) {");
    let main = "int main(int argc, char **argv) {";
    if generated.matches(main).count() != 1 {
        return Err("application raw main marker changed".into());
    }
    let generated = generated.replace(
        main,
        &format!(
            "{}\n{main}\n  ApplicationX11 application_x11;",
            include_str!("../tests/fixtures/ui-application-lifecycle/raw-x11.inc")
        ),
    );
    fs::write(&path, generated)?;
    let binary = output.join("ui-application-raw");
    let status = Command::new(std::env::var_os("CXX").unwrap_or_else(|| "c++".into()))
        .args(["-std=c++17", "-O2"])
        .arg("-I")
        .arg(raylib.join("include"))
        .arg(&path)
        .args(["-ldl", "-o"])
        .arg(&binary)
        .status()?;
    if !status.success() {
        return Err("application raw helper compilation failed".into());
    }
    Ok(binary)
}

pub fn build_cli() -> Result<()> {
    let args = crate::argparse_scan::scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            crate::argparse_scan::OptionSpec::value("--output", true),
            crate::argparse_scan::OptionSpec::value("--raylib-root", false),
        ],
    )?;
    if args.help {
        println!("usage: build-ui-application-reference --output OUTPUT [--raylib-root ROOT]");
        return Ok(());
    }
    let raylib = args
        .values
        .get("--raylib-root")
        .cloned()
        .or_else(|| std::env::var_os("STARTUP_UI_RAYLIB_ROOT"))
        .ok_or("set STARTUP_UI_RAYLIB_ROOT")?;
    openpilot_disk_space::inspect(Path::new(&args.values["--output"]), 16 * 1024 * 1024)?
        .require()?;
    println!(
        "{}",
        build(Path::new(&args.values["--output"]), Path::new(&raylib))?.display()
    );
    Ok(())
}
