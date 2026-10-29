# 공개 입력 pin으로 production source_entry 실행 — #613/#598

이 변경은 실제 실행 전의 검증 driver다. 현재 브랜치에는
`rust/native-bootstrap-inputs.json`이 없고 factory sudo admission도 Actions 결과가
필요하므로, 전체 source entry 성공을 주장하지 않는다. 기능·빌드·namespace·probe는
로컬에서 실행하지 않았다. 기기 접속이나 설치는 수행하지 않는다.

## 실행 경로

`native-source-entry.yml`은 `workflow_dispatch`만 받는다. 등록된 `bin-actions`의
독립 Actions checkout에서 정확 HEAD/tree와 clean 상태를 확인하고 **커밋된 Git blob**의
입력 index SHA를 dispatch의 독립 SHA와 비교한다. 후보 artifact, 임시 index commit,
fixture provider, 런타임 release를 대체 입력으로 받지 않는다. index가 없으면 image
준비 전 실패한다. 실제 GitHub 공개 metadata body 17개(seed/SDK/provider15개)를
HTTPS로 받아 SHA/byte와 현재 SDK plan/provider profile을 확인한다. seed의
`current_tools=1`, `request_owner=2`, `generation_boot=1`도 빌드 전에 요구한다.
production entry도 공개 입력을 다시 독립 검증한다. 별도 producer artifact의 공개·pin
검증은 기존 publication workflow의 책임이며 여기서 발행을 수행하지 않는다.

고정 factory raw image와 static X64 QEMU를 기존 transport helper로 검증한다.
`debugfs rdump` 뒤 원래 인증 image inode의 root 소유/mode04755와 sudo body SHA를
확인해 **그 sudo 한 파일의 SUID만** 복원한다. 새 child만 job cgroup으로 이동시키고,
identity map0..65535를 만든 private user/mount/cgroup namespace에서 private binfmt
`CF` 규칙을 사용한다. 원래 ARM `/usr/bin/python3 -I -B -S`, UID/GID1000과 `comma`
passwd/group, 실제 `sudo -n -l` 및 원래 admission payload의 root 권한 전환을 요구한다.
원래 sudoers는 편집하지 않는다. 공유 binfmt/controller의 전후 상태도 비교한다.

source는 `/native/source`에 readonly bind하고 job 전용 state를 `/native/state`에
bind한다. 실제 CLI로 다음을 실행한다.

1. build/seal 전체 실행. 원래 seed current-tools, current runtime-inputs,
   native-build-scope의 세 sudo gate를 모두 그대로 호출한다.
2. 같은 source·같은 machine policy·같은 실제 cache 경로로 CLI를 다시 실행한다.
   production 결과의 source/index, build/seal, stores와
   `/native/state/cache/cargo-build` 경로를 확인한다.
3. 실제 `--launch --verify-package --paths /native/state/launch-paths.json` 실행.
   machine의 독립 `boot_seed`는 검증한 공개 index의 seed와 같고 실제 Params 경로도
   launcher의 data/params와 일치시킨다. 최초 startup record가 없으면 이 호출에서
   원래 production 정책대로 한 번 더 build 요청 후 startup record를 보존한다.
   generation ID는 seal에 포함되는 요청별 증거 때문에 같다고 가정하지 않는다.

`--verify-package`는 원래 launcher의 패키지 검증 후 manager 시작 전에 반환한다.
따라서 manager 정상 부팅, hardware/model/녹화/업로드, 실제 AGNOS kernel/ARM CPU,
update/recovery 및 docs/작은 변경 재사용은 이 driver가 검증했다고 표시하지 않는다.
정상 startup의 20초 deadline과 정책은 바꾸지 않는다. `build_timeout_seconds`는
CI 작업 전체의 외부 제한이며 startup deadline이 아니다. 최초/반복/launch 성공 후에만
`source_entry_full_build_executed`와 production package launch flag를 true로 기록한다.

## 작업별 admission과 cache identity

dispatch의 `operation_estimates_json`에는 다음 필드가 모두 필요하다. 임의 숫자의
기본값이나 고정25/35GiB 기준은 없다. 실행자가 실제 producer 계측/보수적 상한을
대조하여 이번 전체 source build에 맞는 값을 제공해야 한다. 기기 예산으로 재사용하지 않는다.

- `schema=1`
- `maximum_memory_bytes`, `minimum_available_memory_bytes`
- `maximum_growth_bytes`, `minimum_free_bytes`
- `expected_source_growth_bytes`, `expected_compile_growth_bytes`
- `remaining_object_growth_bytes`, `compiler_growth_bytes`
- `build_timeout_seconds` (120..18000; image 준비와 정리는 job330분 안에서 별도)
- `budgets`: production의 schema1, 현재 catalog의 모든 group별
  `group_growth_bytes`, `expected_source_growth_bytes`, `remaining_object_growth_bytes`,
  `estimate_basis`

image 압축본/sparse/raw/추출본과 도구, 위 전체 build 증가량·여유를 사전 확인한다.
각 production gate는 자체 원래 resource policy를 계속 검사한다. root setup은 job의
실제 소유/inode/lease를 확인한다. 마지막에는 자기 cgroup만 kill/rmdir하고 artifact를
올린 뒤 기존 job-temp cleanup을 사용한다. 다른 process/cache/worktree는 삭제하지 않는다.

Genuine API 검사에서 사용하는 `native-complete-cargo-cache`를 이름만 바꿔 연결하지
않는다. source/target/툴체인/provider 절대 경로와 build recipe identity가 같다는 증거가
없기 때문이다. 이번 driver는 job 간 Cargo cache를 복원하지 않는다. 같은 namespace의
동일 production cache 경로에서 반복 실행했다는 사실만 기록하며, origin 전송0 또는
전체 후보 재다운로드 제거 성공을 추론하지 않는다. 전체 재사용 수용에는 정확한
다운로드/compile receipt 비교와 docs/작은 수정 실행이 추가로 필요하다.

## 최소 공유 helper 확장 제안 / 미확정 blocker

기존 transport helper/workflow는 이 변경에서 편집하지 않았다. 기존 `child`는 payload가
고정되어 있고 `probe`는 admission120초로 제한된다. 그래서 새 driver는 fork/identity-map/
private mount/owned cleanup orchestration을 독립 구현하고 인증 image·SUID·QEMU·command·
controller helper를 읽어 재사용한다. 후속 정리 시 공통 helper에 `binds`, **고정된 신뢰
payload argv**, 작업 timeout, 결과 파일과 owned cleanup context를 받는 작은 API를
추출하면 이 중복을 없앨 수 있다. 지금 admission 코드의 의미나 진행 중 CI를 바꾸는
refactor는 하지 않는다.

실제 namespace/binfmt/cgroup mount와 factory sudo의 지원은 admission Actions 결과로
확인해야 한다. 정책 차단이면 공유 host 설정을 바꾸지 않고 정확 실패를 남긴다. 공개
metadata의 실제 발행/Git pin, 전체 build에 맞는 작업별 예산, admission 통과가 첫
실행의 선행 조건이다. 최초 실행에서 새 실제 실패가 나오면 그 원인만 보정한다.
최종 기기 수용은 [#528](https://github.com/bin9208/openpilot-rust/issues/528)에 남긴다.

## 이번 로컬 확인

Python AST, workflow YAML 및 포함 Bash 문법, diff만 확인한다. 기능 테스트, build,
namespace/probe, CI dispatch/cancel/push/외부 comment는 실행하지 않는다.
