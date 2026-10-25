//! 기존 prepared SDK 감사를 원문 그대로 사용하는 경량 테스트 패키지.

#[cfg(all(test, unix))]
fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[cfg(all(test, unix))]
#[path = "../../dev-tools/tests/ci_disk_bootstrap/prepared_audit.rs"]
mod prepared_audit;
