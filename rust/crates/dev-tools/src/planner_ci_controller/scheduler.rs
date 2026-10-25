//! plannerd_ipc.py의 실제 송신 시각 정책. stdio endpoint와 공개 API가 같은 함수를 쓴다.
use crate::seal::Result;
use std::{
    fs,
    io::{BufRead, Write},
    path::Path,
};

pub const PERIOD: f64 = 0.05;

pub fn next_model_target(nominal: f64, previous_sent: f64, previous_model_done: f64) -> f64 {
    // Python max(a, b)는 b > a일 때만 교체한다. 동률의 signed zero와
    // unordered NaN에서도 첫 피연산자를 유지해야 한다.
    let mut target = nominal;
    let candidate = previous_sent + PERIOD;
    if candidate > target {
        target = candidate;
    }
    if previous_model_done - previous_sent >= PERIOD {
        let candidate = previous_model_done + PERIOD;
        if candidate > target {
            target = candidate;
        }
    }
    target
}

/// 지속 연결의 요청 한 행은 nominal/previous_sent/previous_model_done 세 실수다.
/// std의 round-trip float 입출력을 써 JSON parser feature에 시각 정밀도를 의존하지 않는다.
pub fn serve(input: impl BufRead, mut output: impl Write) -> Result<()> {
    serve_recorded(input, &mut output, &mut std::io::sink())
}

pub fn serve_recorded(
    input: impl BufRead,
    mut output: impl Write,
    mut evidence: impl Write,
) -> Result<()> {
    for line in input.lines() {
        let line = line?;
        let values = line
            .split_whitespace()
            .map(str::parse::<f64>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let [nominal, previous_sent, previous_model_done] = values.as_slice() else {
            return Err("scheduler requires exactly three timestamps per line".into());
        };
        if !values.iter().all(|value| value.is_finite()) {
            return Err("scheduler timestamps must be finite".into());
        }
        let target = next_model_target(*nominal, *previous_sent, *previous_model_done);
        writeln!(
            evidence,
            "{nominal:?}\t{previous_sent:?}\t{previous_model_done:?}\t{target:?}"
        )?;
        evidence.flush()?;
        writeln!(output, "{target:?}")?;
        output.flush()?;
    }
    Ok(())
}

pub fn verify_evidence(path: &Path, expected: usize) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let mut count = 0;
    for line in text.lines() {
        let values = line
            .split_whitespace()
            .map(str::parse::<f64>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let [nominal, sent, done, target] = values.as_slice() else {
            return Err("invalid scheduler evidence row".into());
        };
        if !values.iter().all(|value| value.is_finite())
            || next_model_target(*nominal, *sent, *done).to_bits() != target.to_bits()
        {
            return Err("scheduler evidence target differs from the maintained policy".into());
        }
        count += 1;
    }
    if count != expected {
        return Err(format!("native scheduler calls: expected {expected}, got {count}; IPC caller wiring is required").into());
    }
    Ok(())
}
