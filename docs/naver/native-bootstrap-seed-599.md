# 첫 입력 준비 도구의 고정 seed — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
Rust compiler와 Cargo 원본을 준비하는 Rust 도구를 처음 얻는 순환을 분리한다.
고정된 작은 `openpilot-runtime-inputs` seed를 재사용하고 현재 프로젝트 런타임은 소스에서
빌드한다. 소비하는 프로젝트 commit마다 seed를 새로 발행할 필요는 없다.

현재 변경은 후보 추출·검증·소비 검사를 구현한 단계다. 검증된 공개 seed release,
그 descriptor SHA의 독립된 Git pin과 전체 native build/start 연결은 아직 남는다.
`rust/tools/ensure_runtime.sh`의 기존 기본 경로와 런타임 선택은 이 변경에서 바꾸지 않았다.

## 신뢰 경계와 저장

`tools/native/bootstrap_seed.py`는 AGNOS에 있는 Python 표준 라이브러리로 첫 도구를
준비한다. 별도 pip 설치나 먼저 설치된 프로젝트 Rust helper를 요구하지 않는다.
descriptor는 다음 내용을 묶는다.

- 정확한 seed source commit/tree와 준비 도구 역할, AArch64/AGNOS19.8-carrot-bt1.
- Cargo.lock·고정 Rust/Cargo 입력 plan·검증된 Cargo tree SHA, 실제 rustc/Cargo hash와 feature.
- helper ELF와 원래 프로젝트/Rust 배포본/해당 Cargo 의존성의 license notice SHA·정확 byte 수.
- 실제 기본 loader의 interpreter/library 경로·canonical 대상·hash/크기/mode와 readelf/ldd 도구.

소비자는 descriptor의 **독립적으로 고정한 SHA**를 제공한다. release 옆에서 받은 checksum을
자동으로 신뢰하지 않는다. `manifest.sha256`은 producer의 후보 식별/검토용 파일이며,
공개 사용 전에 검증된 값을 Git의 bootstrap pin으로 반영해야 한다.
descriptor의 source는 seed의 원래 소스다. 현재 runtime source로 다시 표기하지 않는다.

helper64MiB/notice16MiB/descriptor1MiB 한도와 정확 SHA·크기·ELF header를 확인한다.
내용별 lock·별도 임시 경로·fsync·rename-NOREPLACE로 게시하고, 기존 hit도 전체 고정 파일과
descriptor를 다시 검증한다. descriptor와 helper를 함께 다시 써도 외부 pin을 대체하지 못한다.
반복은 전송0/stored0이며, 같은 descriptor의 잠긴 소유 staging만 회수한다. 게시된 세대와
다른 descriptor/다른 이름의 데이터는 제거하지 않는다. 실패 시 기존 runtime/로그를 건드리지 않는다.

준비만 수행한 보고서는 `seed_executed=false`, `compiler_executed=false`,
`runtime_activated=false`다. x64에서 파일을 준비한 결과를 ARM 실행으로 표현하지 않는다.
`--verify-native`는 실제 AArch64/AGNOS, 고정 ABI 파일과 도구, 옮긴 helper의 기본 loader mapping을
재검증한다. 이 seed ABI는 모든 runtime/provider/header/compiler SDK의 검증을 대체하지 않는다.

## 개발 검증과 실행 형태

아래는 검증된 descriptor/pin이 있는 개발 환경의 준비 명령이다. 기기 설치/첫 수용 안내가
아니며, 현재 공개 seed가 준비되었다는 의미도 아니다. cache의 부모는 미리 존재해야 한다.

```sh
python3 tools/native/bootstrap_seed.py prepare \
  --manifest "$VERIFIED_SEED_DESCRIPTOR" --manifest-sha256 "$PINNED_SEED_DESCRIPTOR_SHA256" \
  --cache "$NATIVE_BOOTSTRAP_CACHE" --verify-native
```

source-built producer의 `export`는 실제 AGNOS에서 clean source/실제 native helper/검증된 Cargo
vendor/Rust 배포본 license를 입력받는다. 준비 도구가 사용하는 Cargo dependency graph의
normal/build 의존성을 따라 notice를 보존하고 source·binary·ABI가 전후 달라지면 실패한다.
출력은 helper/notice/manifest/checksum 후보다. 코드가 release를 생성하거나 장치에 설치하지 않는다.

foundation에는 ELF 모양의 소유 fixture4개가 있다. 독립 descriptor pin, 고정 seed 재사용,
helper/로컬 descriptor 변조, 잘못된 역할·source·URL·크기·body·ELF, 공간·동시 owner·staging/
캐시 symlink·목록 손상과 기존 입력 보존을 검사한다. fixture 파일은 실행하지 않는다.
별도 HTTPS 검사는 기존의 고정 공개 text403 bytes를 사용하며 실제 공개 seed 다운로드가 아니다.

genuine AGNOS workflow는 기존 명령·입력 pin·모든 원래 회귀를 유지한다. 그 결과 뒤에 실제
source-built ARM helper를 추출하고 별도로 고정한 descriptor로 후보 파일을 처음/반복 준비한다.
옮긴 helper의 실제 ABI를 확인하고, 그 helper로 새 Rust prefix와 기존 고정 Cargo 입력을 준비한다.
새 prefix의 compiler/Cargo로 실제 std 프로그램2개를 빌드·실행해43/42, 반복 fresh/같은 hash를
확인한다. producer와 consumer는 다른 경로의 실행 파일을 사용한다.

이 검사에서 seed 파일은 소유 artifact 경로로 공급하므로 `transferred_bytes=0`이다.
공개 GitHub release의 실제 seed 전송/재시도·캐시 재사용은 발행 후 별도로 확인해야 한다.
기존 준비용 라이선스/입력과 자산 cache/전송 측정, host/guest/source/emulation 기록은 유지한다.
QEMU/PRoot 사용자 공간이며 물리 ARM CPU/AGNOS kernel/실기 성능으로 표현하지 않는다.

현재는 새 정확 source의 Actions 실행 전이며 기능 성공을 가정하지 않는다. 전체 build worker의
aggregate 자원·취소/전원 중단/복구는 기존 #600 guard와 연결해 검증해야 한다. 전체 native OS
SDK/provider·기존 Python 모델 준비, full99개 source build/seal/기본 시작·업데이트/복구·녹화·
기존 업로드, 정확 source PR/병합 후 검사와 사용자 comma3X 수용까지 #598은 열린 상태다.
