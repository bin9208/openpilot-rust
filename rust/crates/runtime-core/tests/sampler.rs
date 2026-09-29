#![cfg(target_os = "linux")]
use std::process::Command;

#[test]
fn rejects_bad_sampling_intervals() {
    for arg in ["0", "-1", "60001", "oops"] {
        let out = Command::new(env!("CARGO_BIN_EXE_cpu-sample"))
            .arg(arg)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("interval"));
    }
    assert!(!Command::new(env!("CARGO_BIN_EXE_cpu-sample"))
        .args(["1", "extra"])
        .output()
        .unwrap()
        .status
        .success());
}

#[test]
fn reads_actual_linux_processes_without_publishing() {
    let out = Command::new(env!("CARGO_BIN_EXE_cpu-sample"))
        .arg("20")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("pid\tname\tcpu_percent_one_core\tlast_processor\tthreads\n"));
    assert!(text.lines().count() > 1);
    for line in text.lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let cpu: f64 = fields[2].parse().unwrap();
        assert!(cpu.is_finite() && cpu >= 0.0);
    }
}
