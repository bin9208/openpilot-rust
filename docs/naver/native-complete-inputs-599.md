# 전체 native 입력 Layout과 99개 빌드 연결 — #599/#600/#598

이 변경은 준비된 모델·라이브러리·데이터를 실제 전체 빌드와 봉인 경로에 연결한다.
코드와 CI 연결은 작성했으며 **새 source의 기능/전체 빌드 성공은 아직 미확인**이다.
단계별 준비·계획 성공을 정상 시작·기기 수용으로 해석하지 않는다.

## 하나의 Layout에 필요한 전체 입력

`rust/tools/native_runtime_layout.py`는 다음18개 역할을 모두 요구한다.

| 종류 | 필수 역할 |
| --- | --- |
| 기존 C/C++ provider | ffmpeg, raylib, opencv, numerics, geos, brotli, acados, sdl, turbojpeg, ffmpeg-tools, freesans |
| source에 결속한 결과 | panda, mpc, dbc, usbgpu |
| 생성/외부 모델 및 source 자산 | models, source-lfs, usbgpu-model |

ordinary Git source 자산9개는 현재 Git blob에서 별도로 준비한다. 기존17개 source 자산은
ordinary9+LFS4+Panda firmware4로 채운다. 모델 cache·compiler/header tree·기존 Rust 실행 파일과
과거 runtime manifest를 현재 runtime의 실행 파일/manifest로 넣지 않는다.
과거 manifest와 provider provenance는 역할별 license/provenance 위치에 보존한다.

각 역할은 실제 준비기가 내놓은 root/receipt와 **독립 SHA pin**을 사용한다. receipt만 새로
capture해서 기대값으로 삼지 않는다. 전체 tree의 파일·mode·hash·길이·alias·추가/누락을 검사하고,
현재 고정 profile의 member/hash/provenance와 다시 대조한다. NumPy/OpenBLAS prefix 하나는
기존 네 곳(card/controlsd/torqued/radarcan)에 hardlink한다. FFmpeg/tools, acados/MPC, SDL/font의
알려진 겹침은 동일한 파일 내용과 mode일 때만 재사용한다. 충돌은 거부한다.

combined prefix는 `rust/bin` 내부 대상에 flat 상대 경로를 사용하고, 나머지 source 자산은
`source-assets/` 아래에 둔다. 내부 모델의 기존 flat mapping 계약도 유지한다. 입력 alias는 검증한
동일 파일의 읽기 전용 hardlink로 materialize하며 compiler SDK의 원래 alias 구조는 바꾸지 않는다.
source/입력 cache와 output store는 분리하고, 대형 입력은 같은 filesystem의 hardlink로 묶는다.

현재 source의 Python producer bytes, 모든 역할 pin과 payload tree, ordinary Git bytes로
composition identity를 만든다. 현재 요청 commit은 보고서에 기록하며 원래 제작 provenance에
덮어쓰지 않는다. 불변 stage/lock/fsync/atomic publication을 기존 helper로 재사용한다.
반복에는 복사·link 생성0을 확인한다. 이는 같은 owner의 적극적인 변조를 격리하는 attestation은 아니다.

## eGPU 모델과 생성 데이터의 독립 cache

기존 a3d0e3a4 runtime에서 shader/firmware/HCQ/provenance/license를 고정 목록으로 추출한다.
원래48개 파일 중776,634,338byte PKL은 component에서 제외한다. component는 원래47개 파일과
원래 runtime manifest48개 member, raw payload3,949,690bytes다. 기존 archive/manifest SHA는 유지한다.

별도 `rust/native-usbgpu-model.json`은 기존 NAS 모델의 SHA256
`e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b`와 길이/URL을 고정한다.
기존 core asset cache를 사용하므로 이 모델만 최초 준비하고 반복 전송·저장0을 검사한다.
shader component 변경 때문에 같은 대형 모델을 다시 내려받는 구성으로 묶지 않는다.

`native-usbgpu-source-contract.json`은 원래 데이터 제작기5개, AMD metadata, source의 model/state
계약을 포함한 Git8개를 결속한다. 원래 a3d와 현재 입력이 같음을 읽기 감사로 확인했다.
entry SHA256은 `7d6796225465d627c4cd57895d3c8505e1cf3c3de52fa00c859244e516d5738c`다.
원래 runtime archive/hardware/생성기 provenance와 license는 그대로 보존한다. 현재 소스에서
GPU kernel을 새로 생성했다고 표시하지 않는다. source/version이 달라지면 새 검증 전 재사용을 거부한다.

## 실제 실행과 검사 방법

core 명령은 현재 source의 internal model·LFS·Panda·MPC·DBC·eGPU와 ordinary 자산을
빌드 전에 확인한다. Config.build와 seal도 같은 완전한 gate를 사용한다.

```sh
openpilot-runtime-inputs --verify-runtime-layout --root CLEAN_SOURCE \
  --store COMPOSED_PREFIX --plan RUNTIME_DESTINATION_MAPPING_JSON
```

Python 모델 준비는 유지하며 composition은 준비된 immutable 출력을 사용한다. 고정 Python으로
`-I -B -S -X pycache_prefix=EMPTY_PRIVATE_PATH`를 지정한다. 실제 spec은 genuine CI가 준비된
18개 역할에서 만들며, report와 store는 source/입력 cache 밖에 둔다.

```sh
PYTHON -I -B -S -X pycache_prefix=EMPTY_PRIVATE_PATH \
  rust/tools/native_runtime_layout.py --root CLEAN_SOURCE --spec COMPLETE_SPEC_JSON \
  --store COMPOSITION_STORE --verifier openpilot-runtime-inputs --report NEW_REPORT_JSON
```

genuine Actions는 이 실제 Layout을 compiler pin9개·기존13그룹99개 recipe와 결속한다.
`native_full_generation` CI 예제는 실제99개를 빌드하고 **그 빌드의** manager/model consumer/
sealer/ELF audit으로 봉인한다. 원래 source SHA의 소유 checkout을 사용하고 이전 binary로
채우지 않는다. 실제 source generation을 검증한 뒤 이 job의 composite staging cache만 잠시
숨겨 cache 없이도 봉인된 source/output 검증이 되는지 확인한다. 다른 cache는 보존한다.
예제는 production memory-scope frontend의 대체 진입점으로 설치하지 않는다.

같은 고정 소유 checkout에서 반복, docs commit, 작은 logger source commit을 추가로 검사한다.
반복/docs는99개 전부 Cargo fresh 및 새 binary 저장0, 작은 변경은 logger 그룹만 재빌드하고
다른 그룹의 fresh/hash 동일을 요구한다. CI 전용 source commit과 실제 GitHub source를 구분하며
원본 checkout은 바꾸지 않는다. 실행 결과가 나오기 전에는 재사용 통과로 기록하지 않는다.

로컬은 AST/JSON/YAML/Bash syntax·rustfmt·diff/읽기 감사만 수행한다. 기능 검사는 Actions다.
기존 `runtime-source-build.yml` foundation의 source/data/CLI/unit/Clippy와
`runtime-native-environment.yml`의 실제 준비·ARM composition·99개 빌드/봉인/재사용을 확인한다.
모든 job은 bin-actions이며 기존 명령/pin·360분 budget·이미 실행 중인 검사를 유지한다.
실제 실패는 해당 group/manifest/ABI 원인을 수정하며 검사를 생략하거나 강제 성공시키지 않는다.

per-command wall time, 관측 disk 증가량, `/usr/bin/time` single-child maximum RSS를 기록한다.
이것은 aggregate process-tree/kernel memory peak가 아니며 별도 측정이 계속 필요하다.
QEMU/PRoot ARM user space는 물리 ARM CPU·AGNOS kernel 또는 comma 3X 정상 부팅 검증이 아니다.
공개 component/seed 전송과 default bootstrap, 실제 startup/업데이트/복구·녹화·기존 업로드,
필수 PR/병합 후 검사·성능 근거·사용자 기기 수용은 남아 있다. #598 완료를 주장하지 않는다.
