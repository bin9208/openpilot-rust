# 실제 AGNOS의 native compiler 환경 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598).
모델 준비 Python과 기존 C/C++ provider를 유지한다. 이 기록은 전체 runtime의 완성이나
기기 설치 수용을 뜻하지 않는다.

## 원본 입력과 실행 범위

저장소의 `openpilot/system/hardware/tici/agnos.json`과 `agnos-tici.json`은 같은 system
이미지를 고정한다. 2026-10-26 릴리스 API도 압축 크기와 SHA를 동일하게 반환했다.

- 압축 크기: `1010655092` bytes.
- 압축 SHA-256: `077d42186def98025ffad9723774c96f45da10c87cc6fedbc58d0cf4fa65ffd4`.
- sparse SHA-256: `a67410a888105ecce7bcc05e17cda77fd10572aa4380e9de242a7c92f3497114`.
- raw 크기: `4718592000` bytes.
- raw SHA-256: `375c5d22335770ac08750660bb5b29a3331c550d6a9875b35f36b88af792ea44`.
- 원본: [AGNOS 19.8-carrot-bt1 release](https://github.com/ajouatom/agnos-builder/releases/tag/agnos-19.8-carrot-bt1).

`runtime-native-environment.yml`의 새 job은 등록된 bin-actions에서 실행한다. 원본 manifest를
바꾸지 않고 기존 content cache CLI로 실제 이미지 본문을 준비한다. 동일 자산 재준비는
hash 재검증 후 HTTP body 전송0이어야 한다. Actions cache도 같은 압축 SHA로 구분하며,
복원된 blob을 무조건 신뢰하지 않는다.

검증한 이미지의 압축·sparse·raw hash를 차례로 확인한 뒤 debugfs로 원래 파일시스템을
분리해 추출한다. 이후 native ARM Rustup/Rust 1.94, 원래 C++ compiler와 표준 라이브러리를
QEMU user/고정 PRoot에서 실행한다. host architecture/kernel, guest architecture, image/
QEMU/PRoot/Rustup/compiler hash와 명령·시간·실패를 남긴다. 실제 AGNOS kernel 부팅이나
물리 ARM CPU/기기 실행으로 표현하지 않는다. Ubuntu ARM userspace의 #605 검사와 구분한다.

compiler 및 Python·header 생성 도구의 실제 유무/버전을 먼저 기록하고, 의존성 없는 작은
Rust binary 두 개와 C++ 표준 라이브러리 프로그램을 native build·실행한다. Cargo 초기
artifact는 fresh=false, 같은 입력 반복은 fresh=true 및 같은 출력 hash여야 한다. 전체
프로젝트 runtime, 모델 준비 의존성, 카메라/GPU/녹화/업로드를 이 작은 probe로 대체하지 않는다.

## 실제 프로젝트 build driver 연결 (검증 대기)

별도 #599 후속은 같은 고정 AGNOS 이미지 안에서 실제 Actions source SHA의
`openpilot-runtime-build` crate와 CLI를 native Rust 1.94로 빌드한다. source는 별도 guest
mount로 연결하고 Git SHA 및 최종 clean 상태를 대조한다. Cargo.lock을 유지하고 source를
수정하지 않으며 target/Cargo cache/Git fixture는 자기 job temp에만 쓴다. compiler·헤더를
host cross SDK로 바꾸거나 guest에 APT 패키지를 설치하지 않는다.

만든 CLI가 AArch64 ELF인지 확인하고 실제 guest에서 실행한다. 별도 소유 Git fixture의
두 작은 binary를 초기/반복/문서 변경/하나의 소스 변경으로 빌드한다. 반복과 문서 변경은
모두 fresh/동일 hash여야 하며, 작은 변경은 실제 출력이43으로 바뀌고 무관한 binary는
42/fresh/동일 hash를 유지해야 한다. 실제 native C/C++ compiler/helper hash와 source/
recipe/toolchain/결과 보고서는 원문 JSON으로 보존한다. CLI 자체 source SHA/ELF hash는
fixture source SHA와 구분한다.

이것은 실제 프로젝트 build engine의 AGNOS user-space 검증이며 전체 runtime의
provider/generated 입력·ABI 봉인·세대 전환이나 AGNOS kernel/기기 실행을 뜻하지 않는다.
최종 전체 roster/recipe, Python 모델 준비와 기존 provider, 장치 자원 예산·정상 부팅·녹화·
기존 업로드 수용은 계속 남아 있다. 정확 source의 새 Actions 결과를 확인해야 한다.

## 공간·증거와 남은 작업

압축 입력 준비, sparse 해제, raw 변환, root 추출, toolchain 준비 전 각각 그 작업의 예상
증가량을 확인한다. sparse는 검증한 raw 크기와 제한된 header 여유를 넘으면 중단한다.
검증 뒤 재생성 가능한 중간 이미지 파일만 이 job 소유 temp에서 정리한다. 다른 cache,
소스, 실행 중 검사는 수정하지 않는다. 마지막 공통 #502/#605 temp helper만 소유 확인 뒤
job 디렉터리를 제거한다.

`native-agnos-environment-599.json`, 원문 command log, 초기/재사용 Cargo JSON, 실제 이미지
전송 receipt를 artifact로 보존한다. 현재 RSS는 **단일 child의 최대 RSS**이고, free-space
차이는 관찰된 filesystem 변화다. aggregate process-tree peak, 디스크 peak, 실제 장치
전체 runtime 비용으로 보고하지 않는다. 전체 resource 계측은 여전히 완료 조건이다.

초기 source `a8ef20ef8a3e60ef4848288713f029cb2482a16d`의
[40334144255](https://github.com/bin9208/openpilot-rust/actions/runs/40334144255)는 X64의
runtime-build/installer 및 assets binary 컴파일에는 성공했지만 자산의 논리 이름에 `.`이
포함되어 Plan 검증에서 실패했다. 압축 이미지 다운로드 및 AGNOS compiler 실행에는 아직
도달하지 않았다. 자산 키만 허용된 `agnos-system-19-8-carrot-bt1`로 수정하며 원본 URL/
버전/SHA/길이 pin은 그대로 유지했다. 새로운 정확 source의 Actions 결과를 확인해야 한다.

최종 사용자 수용 장치는 **comma 3X**로 선택되었다. 결과를 읽어 누락된 native
headers/toolchain/도구와 기존
Python 모델 준비 환경을 고정하고, 트랙별 전체 runtime roster·provider receipt·recipe에
연결해야 한다. full AGNOS generation build/seal/start/update/recovery와 사용자 기기의 정상
부팅·기록·기존 업로드 수용 전까지 #599와 #598을 닫지 않는다.

전체89개/11그룹의 기존 runtime 목록과 새 자체 빌드·봉인 도구10개를 추적 파일로
옮기고, 실제 Cargo metadata 대조 및 입력별 recipe 생성기를 추가했다. 구현/검증 범위와
사용 명령은 [전체 native runtime 계획](native-runtime-plan-599.md)에 기록한다.

Rust compiler/Cargo/std 원본 세 자산은 별도 내용별 준비기로 전체 tree provenance까지
연결한다. 현재 새 Actions 결과 대기이며 구현·pin·실행 구분은
[native Rust 도구체인 준비](native-rust-toolchain-599.md)에 기록한다.
