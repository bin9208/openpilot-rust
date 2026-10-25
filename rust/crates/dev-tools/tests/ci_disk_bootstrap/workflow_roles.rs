//! source 계약 검사에서만 두 main proxy를 같은 commit의 실제 역할로 해석한다.
//! 다른 여섯 ARM 역할은 main의 X64 역할과 중복 합산하지 않는다.
use openpilot_dev_tools::ci_policy::validate_native_arm_reuse;
use serde_json::Value;

pub fn resolve(main: &Value, native: &Value) -> Result<Value, String> {
    // 고정 local callee, 두 proxy, 전체 필수 결과와 실제 stage의 trust를 먼저 검증한다.
    validate_native_arm_reuse(main, native)?;
    let mut resolved = main.clone();
    for (role, expected) in [("radar-arm", 11), ("carrot-navi-arm", 3)] {
        let job = &native["jobs"][role];
        let steps = job["steps"].as_array().ok_or("missing native ARM steps")?;
        let count = steps
            .iter()
            .filter_map(|step| step["run"].as_str())
            .map(|run| {
                run.matches("\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\"")
                    .count()
            })
            .sum::<usize>();
        if count != expected {
            return Err(format!(
                "{role}: inherited disk guard roster changed ({count} != {expected})"
            ));
        }
        resolved["jobs"][role] = job.clone();
    }
    Ok(resolved)
}
