# route-vault bundle 원본 관측 fixture

2026-10-24. 원본의 실제 관측23행을 보존한 frozen 테스트 입력이다.
새 loader/materialize/negative 및 retained17+negative1=18개 검사는 **UNRUN**이다.
로컬은 inert artifact 읽기·Rust syntax/fmt·JSON syntax·source diff만 수행했다.
원본·archived ELF·native 기능·빌드·Clippy·LLVM은 실행하지 않았다.

## 원문과 실제 수용 범위

- capture source: `121aa5b86ddf427697a8cabb25d2ed337d80f429`
- [실제 capture run40093557875/job120871017872](https://github.com/bin9208/openpilot-rust/actions/runs/40093557875/job/120871017872)
- artifact `12560845618`, `bundle-121aa5.zip`, 4,250,230B
- 전체 ZIP SHA256 `b6ecc542f61d83ee1bfa518d3926ccfb45e6d2f52240eb7f85fd42c8cdc3f50a`
- 이 capture ZIP은889 members이며 중복 없이889 CRC를 읽어 확인했다.
- `bundle-integration.log`의 실제17 PASS와 source/index/venv regular/symlink4쌍의
  byte 일치를 확인했다. 각 증거 member SHA는 manifest에 남긴다.
- 원문 capture harness SHA256은
  `5d888f50c5e57483d55b5ab87fe3f8678406cb8bc875651d765573d225390f4f`다.

23개 `original-3139316-*.json`은 ZIP member bytes를 그대로 복사했다.
원문 JSON의 key 순서·공백·raw stdout/stderr·traceback·provider 경로·source metadata를
재직렬화하거나 표준화하지 않았다. 합계184,435B이며 각 raw file의 bytes/SHA/원래
archive member/row ordinal과 경로 span만 새 manifest에 고정했다.
원본51줄 파일 본문을 복사·encode·실행하지 않는다. 관측 traceback의 source line 일부는
허용된 stderr 원문으로 그대로 남는다. fixture repo 전체나 `.git`은 넣지 않는다.

원문23행은 committed bytes1, bad-ref/existing/quotePath-false3, missing fixed7,
argparse12로 나뉜다. 초기 fixture commit은 모두
`b87351bb8a7398b168879bac861fcd910ed2ef1a`다. 원본 copy가 초기 commit 뒤의 untracked
도구였으므로 Repo::new에서 copy만 제거해도 authored tree/actor/date/Git commit은 같다.
missing fixed의 후속 HEAD7개는 해당 원문에서 직접 비교한다.

부모가 별도 수용한 d2 run40097057916/job120881407562/artifact12561452899는
873 members/CRC이며, SHA256은
`8903a728e2fe37bd807dabdb471deb8273893b3b8b4573d9e626b8322ad6fd2d`다.
그411쌍 public helper/실제 ELF 증거는 부모 감사 범위다. 여기서는 d2를 새 capture로
바꾸거나 그 archive 실행 결과를 새 frozen18 성공으로 수용하지 않는다.

## 허용된 바인딩과 출력 재현

- selector는 test name, 전체 raw argv, reference, initial/current HEAD,
  destination scope 종류와 before snapshot을 모두 맞춰 원문 한 행만 고른다.
- `ROOT`, `DEST`, `CWD`의 원문 anchor와 명시 offset만 바인딩한다.
  metadata는 DEST13곳/ROOT30곳이며 별도 CWD 치환은0곳이다. ROOT=CWD 또는
  DEST=CWD인 원문의 alias 관계도 현재 인자에서 같아야 한다.
- 미지원 token, 잘못된/겹친 offset, 동적 경로 span 누락은 거부한다.
  source checkout 경로·script 이름·ref·commit·오류 문구를 바꾸지 않는다.
  상대 파일 이름·출력 파일 bytes·SOURCE_COMMIT 내용도 치환하지 않는다.
- before snapshot 전체가 같아야 한다. 실제23행의 before는 missing 또는 빈 directory이며,
  input repo와 겹치는 출력은 지원하지 않는다.
- 새 출력은 authored private0700 TempDir의 direct child만 허용한다.
  모든 parent를 FD/openat/NOFOLLOW로 따라가며 새 파일은 EXCL로 만든다.
  directory/file의 원래 mode/raw name/raw bytes 전체를 복원하고 다시 비교한다.
  관측하지 않은 symlink/special 출력·경로 정규화·기존 파일 덮어쓰기는 거부한다.
- stdout/stderr/code는 고정 raw outcome을 읽는다. 현재 native 출력으로 기대값을 만들지 않는다.

## source 및 실행 범위

manifest와23raw·명시한 native source10개·test source2개·현재 test ELF 및
`env!("CARGO_BIN_EXE_build-route-vault-bundle")`의 compile-bound integration CLI ELF는
열린 FD의 name/identity/전체 SHA를 전후 확인한다. 두 ELF는 각각128MiB로 제한하고
ELF magic도 확인한다. 별도 host target의 public helper ELF로 대체하지 않는다.
Cargo.toml은 **전체 raw SHA**를 고정한다.
test source는 실제 컴파일된 include_bytes와 비교하여 manifest hash 순환을 피한다.
원본 경로는 file/directory/broken symlink를 모두 거부한다. 부모가 원본 삭제를 담당한다.

negative1은 잘못된 token/offset/누락/겹침, raw 경로 탈출·정규화, SHA 변조,
held 파일 변경, 원본 경로의 file/dir/broken symlink 재출현을 거부한다.
통제용 bytes는 synthetic 거부 입력이며 원본 expected가 아니다.

CI 최소 selector는 기존 `--test route_vault_bundle`이며 required feature는
`route-vault-bundle-native` 그대로다. 새 Cargo 등록·bin·feature가 필요하지 않다.
helper는 `tests/route_vault_bundle/frozen.rs`의 하위 module이며 독립 test target으로
자동 발견되지 않는다. 수용 단위는 route_vault_bundle target의
**18 PASS와23개 고유 BUNDLE_FROZEN_ROW**다. 각 Repo 수명에서도 예상1/12/0회 소비를 확인한다.

다음 CI에서 원본 부재 상태의18개·23개 소비와 source/index/SDK/provider/helper caller 경계를
함께 확인해야 한다. whole Python 부재·caller closure·source retirement·inventory credit은
이 로컬 승격만으로 부여하지 않는다. 원본과의 새 실행 비교도 주장하지 않는다.
