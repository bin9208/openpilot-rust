# 현재 Git OID로 결속하는 LFS 실행 입력 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
재사용 [#600](https://github.com/bin9208/openpilot-rust/issues/600).
소스 자산17개 중 일반 Git9개·LFS4개·생성 Panda firmware4개를 구분한다.
이 단계는 현재 소스의 LFS4개를 별도 준비하며 checkout이나 정상 세대를 수정하지 않는다.

## 입력과 신뢰 경계

rust/native-source-lfs.json의 HTTPS locator anchor는
11f0de76cd4c97941109339003f8f98d9614f2d0이다. locator와 현재 소비 source SHA를 구분한다.
배포 당시의 source를 현재 runtime으로 다시 표시하거나 이전 Rust binary를 가져오지 않는다.

| 소스 경로의 끝부분 | 현재 Git LFS SHA-256 | bytes |
| --- | --- | --- |
| web/assets/alert_camera.svg | f1086279a32f49f68de1db4e89a93aa2aae779395cfb908e5f6a0cbf2bf7dae6 | 989 |
| web/assets/alert_police.svg | d0dea6745d5cd5cf8206ced256310fed587555b3309d80cad46a4c066a931401 | 707 |
| xiaoge/assets/lane.onnx | d3761c185daf33897ff3ff5edf28115c5dc829c06c538033accb616d6c67528c | 13194895 |
| xiaoge/assets/v_asm_model.onnx | 00247ede5159dff9a0768c171095711d40f1ee109ac7b5e24adce344fe4ba6f9 | 9625905 |

합계22822496 bytes다. 고정 GitHub media locator의 원격 HEAD는 HTTP200 및 같은 ETag/size를
반환했다. 원격 메타데이터를 payload 검증으로 대신하지 않는다. 실제 준비는 각 전체 파일의
OID/size를 확인한다. namespace와 locator commit은 읽기 감사/기록이며 현재 Git pointer가
내용의 기준이다. pointer 변경 시 같은 이전 파일을 조용히 사용하는 fallback은 없다.

새 source_assets 모듈은 installer의 소스 자산 allowlist17개를 조사해 실제 committed LFS
pointer 집합을 구한다. 선언한 plan이 이 전체 집합 및 모든 OID/size와 같아야 한다.
평범한 Git blob·미추적 생성 firmware·누락/추가/다른 pin·dirty/변경 중 source는 거부한다.
source와 plan은 준비 전후 및 원자 발행 전에 다시 검사한다. 저장소 내부/별칭/겹치는 cache,
prefix/object store도 거부한다. 현재 source의 일반 Git9개와 생성 firmware를 이 다운로드에
넣지 않는다. 기존 seal의 current Git LFS OID·실제 source asset 검사도 유지한다.

필수 검사는 준비 feature와 분리한다. 전체 generation의 빌드 전/봉인 전 Layout에 모든
현재 LFS가 같은 OID/size/읽기 전용 파일로 매핑돼 있어야 한다. 봉인·부팅/fallback에서는
현재 pointer가 결과 object roster에 빠짐없이 기록되었는지 확인한다. 기존 실제 runtime/
manifest/object byte 검증은 유지하고, boot 검증에 compiler/provider cache를 요구하지 않는다.

## 내용별 재사용

raw cache는 SHA로 구분한다. 필요한 파일만 HTTPS로 받아 전체 hash/size를 확인하고,
store 소유 immutable object로 한 번 복사한다. prefix는 이 object를 hardlink한다.
변경되지 않은 다른 파일은 object를 검증해 재사용하며 새 prefix 때문에 payload를 복사하지 않는다.
prefix와 object store는 같은 filesystem이어야 한다. raw cache는 별도로 둘 수 있다.

정상 prefix의 기대 Tree는 현재 pointer와 plan에서 계산한다. 현재 디렉터리를 snapshot해
스스로 신뢰하지 않는다. 실제 전체 파일/모드/추가 파일·receipt hash를 확인하며, 검증된
prefix가 있으면 raw/object cache가 없어도 전송/복사/link0으로 재사용한다. source SHA는
report에 현재 값으로 기록하며 내용별 key에는 넣지 않는다. 문서만 변경해도 같은 prefix다.
동일 OID가 두 경로에 있으면 다운로드/저장은 한 번만 계산하고 각 경로를 검증한다.

손상된 발행 prefix/receipt는 덮어써서 수리하지 않는다. 실패 시 기존 source/prefix를
보존한다. 중단된 object import의 directory/lock만 남고 blob이 없으면 정상 재시도가 가능하다.
원자 NOREPLACE 발행·fsync·비차단 lock과 기존 object import 검증을 사용한다.
준비는 checkout 파일을 hydrate하거나 runtime을 활성화하지 않는다.

## 실행·검사 방법

기능/빌드는 Actions에서만 실행한다. CLI의 새 mode는 prepare-source-assets feature이며
기본 빌드에 포함된다. archive feature 없이 이 mode만 빌드하는 CI도 별도로 검사한다.

```sh
openpilot-runtime-inputs --prepare-source-lfs --root CLEAN_SOURCE \
  --plan rust/native-source-lfs.json --cache ASSET_CACHE \
  --store PREFIX_STORE --objects OBJECT_STORE
```

부모 디렉터리는 미리 존재해야 하고 세 저장소는 source 밖의 겹치지 않는 절대 경로다.
copied_bytes는 새 immutable payload와 provenance 복사량, linked_bytes는 논리적 link byte다.
expected_additional_bytes는 필요한 raw/object miss 합계와 metadata8MiB를 더한 작업 예상치다.
모든 cache가 비어 있을 때 이 plan 예상치는54033600 bytes이며 고정25/35GiB floor를 쓰지 않는다.
이미 검증된 prefix 재사용은 추가 예상0이다. 예상치를 실측 peak 사용량으로 표현하지 않는다.

foundation x64/ARM의 소유 Git fixture는 문서 변경 재사용·한 객체만 변경한 전송/복사,
다른 object inode 재사용·이전 prefix 보존, 누락/잘못된 pin/일반 Git/firmware/dirty source 거부,
cache 제거 후 prefix 검증, 위조 receipt/추가 파일 거부, 실패/변경 중 source의 미발행,
중단 import 재개/lock 충돌, 중복 OID의 단일 전송을 확인한다. 가짜 payload를 실제 모델
동작 증거로 사용하지 않는다. 원래 no-default 검사와 새 feature-only 검사도 모두 유지한다.
추가 generation 회귀는 누락/경로 이탈 매핑과 boot roster 누락/잘못된 모드·크기를 거부하고,
cache가 없어도 기록된 current OID/size를 검사한다.

genuine host는 실제 HTTPS4개를 준비하고 독립적으로 현재 pointer/실제 파일 hash를 대조한다.
source-built ARM CLI는 host raw cache만 소비해 별도 object/prefix를 준비하며 host와 같은
Tree·현재 source를 확인한다. 초기 ARM/반복 전송0, 반복 복사/link0과 잘못된 pointer plan의
staging 전 거부를 확인한다. 이 단계는 기존8개에 source LFS Tree를 더한9개다.
후속 [Panda 입력](native-panda-inputs-599.md)1개를 더한 현재 전체 계획은10개다.

```sh
gh run list --repo bin9208/openpilot-rust --workflow runtime-source-build.yml --branch BRANCH
gh run list --repo bin9208/openpilot-rust --workflow runtime-native-environment.yml --branch BRANCH
gh run view RUN_ID --repo bin9208/openpilot-rust --log-failed
```

artifact의 native-source-lfs JSON/report/Tree receipt와 commands log를 정확 source SHA로
확인한다. ONNX payload를 별도 검증 artifact로 중복 업로드하지 않는다. 이전 raw cache를
복원해도 소비 파일을 검증하며 cache의 존재 자체를 신뢰하지 않는다. 새 기능 결과는 대기다.

로컬은 Rustfmt·Python AST/TOML/YAML/JSON·Git pointer/HTTP HEAD/diff 읽기만 수행한다.
새 helper/테스트/Cargo/모델은 로컬에서 실행하지 않는다. 등록된 bin-actions의 ARM 결과는
x64 위 QEMU/PRoot AGNOS user space이며 물리 ARM CPU/AGNOS kernel 부팅/기기 수용이 아니다.
이것은 source bytes 준비이며 Python 모델 생성·실제 Xiaoge/모델 회귀나 Panda firmware 준비,
전체 Layout/99개 native build/seal/default update/start/record/upload·복구/실측 자원·성능,
정상 PR·병합 후 검사·공개 seed/component/최종 한영 문서·사용자 comma3X 수용을 대신하지 않는다.
