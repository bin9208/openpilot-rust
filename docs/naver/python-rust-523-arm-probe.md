# #523 현재 native ARM probe 검증 후보

2026-10-25 KST. 이전 PR343의 full musl build는 별도 catalog publisher의 `renameat2` 링크에서
중단돼 native procLog probe에는 미도달이었다. 해당 catalog 결함은 #529의 독립 musl/QEMU
검증으로 해소됐으며 이번에 다시 실행하지 않는다.

새 `.github/workflows/python-rust-523-arm-probe.yml`은 현재 source의 musl release
`openpilot-proclogd` 한 bin과 host `check-aarch64`만 빌드한다. 원래 checker가 QEMU를 두 번
호출해 Params/msgq self-test와 synthetic procLog3frames를 검사한다. mem/CPU/PID/PSS/RSS/nice/
process-name assertion과 static AArch64 ELF 계약을 그대로 사용한다. 원래11unit·성공32cohort·
#529·전체 musl workspace·FFmpeg/graphics/model의 새 성공을 주장하지 않는다.

prepared Rust1.94.0 전체와 실제 musl std/CRT를 읽기 전용으로 봉인한다. 정확한 Zig0.16.0 /
cargo-zigbuild0.23.4 wheel의 native ELF와 전체 Zig sysroot를 private tools로 준비한다.
Python package를 설치하거나 Python loader를 실행하지 않는다. 생성된 wrapper의 empty→ready
issuer 경계와 ready→probe/Clippy 후 불변, 실제 host compiler·shell·QEMU·ELF·DSO 및 source/index를
기록한다. Cargo JSONL 전체를 읽어 정확한 manifest/src/name/kind/profile/features/path의 bin 하나씩
선택한다. build 성공 로그만으로 실행 수용하지 않고 report/stdout/원래 self-test를 모두 요구한다.

source2GiB, tools2GiB/proof512MiB, native build8GiB와 Clippy 추가512MiB의 작업별 admission을
실제 filesystem에 합산한다. allocated/logical 증가 감시와 명령별 제한을 유지한다.8GiB는 새로
좁힌 두 bin 범위의 보수 계획치로 실제 cold peak 보장은 아니다. 기존 전체 ARM12GiB gate는
수정하지 않는다. actual 실패도 upload 후 보존하며, immutable/source seal과 artifact id/digest가
성공한 소유 경로만 정리한다.

독립 source 검토에서 남은 P1/P2는 없었다. 로컬 YAML/Bash 문법·diff만 통과했다. 새 build,
QEMU2회/3frames·Clippy·현재 공간 적합성은 아직 **UNRUN**이다. `physical_arm_cpu:false`,
`device_validation:not_run`이며 ARM/AGNOS 전체 전달이나 실기 수용으로 확대하지 않는다.
