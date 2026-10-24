# #523 bundle 원본 출력 캡처

기록일: 2026-10-24. 상태: **소스 계측 작성, 실제 캡처·기능·빌드·Clippy UNRUN**.
상위 작업: https://github.com/bin9208/openpilot-rust/issues/523.

`rust/crates/dev-tools/tests/route_vault_bundle.rs`의 기존 17개 테스트와 모든
assertion·비교 순서를 유지하고 실제 원본 실행에 JSON 기록을 붙였다. 기존 lib 1개와
integration 17개는 다음 bin-actions batch에서 새로 실행해야 한다. fixture binding과
계측이 변경되었으므로 과거 18개 성공 결과를 이번 소스의 통과로 재사용하지 않는다.

## 실제 원본과 입력 고정

원본은 `tools/carrot_route_vault/build_bundle.py` 51줄이며 SHA256은
`05551fc470864a4c70758a66b1a156c2b6d9e95e8deeb1f2a74ac13fb5259aa9`이다.
내용을 변경하거나 재구현하지 않고 private fixture의 같은 경로로 복사해 실행한다.
복사 전 원본 hash와 매 호출 전후 복사본 hash를 검사한다.

fixture Git 구성 명령은 actor/email·author/committer date
(`2000-01-01T00:00:00+0000`)·UTC/locale·외부 Git 설정 영향을 고정한다. 초기 branch,
SHA1 object format, 빈 template, hook/signing/속성 설정도 명시한다. PATH와 그 밖의
환경은 유지하며 원본 및 native 내부 Git 명령의 기존 실행 방식은 유지한다.

원본 script 복사는 `git add`와 초기 commit **뒤**에 수행한다. script는 untracked
실행 도구이고 원본의 `SOURCES` 및 일곱 fixed file selector에 포함되지 않는다.
따라서 이 순서·날짜 변경은 선택 파일 동작을 바꾸지 않으며, 향후 script 삭제 자체가
동일 authored fixture의 Git tree/commit을 바꾸지 않는다. 현재 원본은 보존한다.
초기 fixture HEAD와 각 실행 직전 실제 HEAD를 각각 기록하므로, fixed file 제거 후
만든 후속 commit도 초기 commit과 구별한다. 실제 commit 값은 CI 캡처 전 미확정이다.

## 기록 경계

각 원본 실행은 PID와 전역 counter를 포함하는 별도 `original-*.json`을
`create_new`·0600으로 생성한다. test thread 이름으로 행을 묶는다.

- 실제 exit code, stdout/stderr 전체 bytes hex, raw traceback을 저장한다.
- raw argv/ref, source commit/root/SHA, fixture initial/current HEAD/root,
  dest scope, 실제 provider argv/cwd/library path/PATH를 보존한다.
- 출력은 dest 전후의 전체 roster·종류·경로 bytes hex·파일 bytes hex·SHA256과
  symlink target만 기록한다. 입력 repo 전체·`.git`·복사한 원본 본문은 기록하지 않는다.
- parser 12건의 출력 경계는 기존 snapshot assertion과 같은 별도의 빈 실행 cwd다.
  `destination_scope_kind=parser-empty-cwd`로 구분한다. 유효한 ref를 해석했다고
  주장하지 않고 `input_reference=null`과 원래 argv 전체를 보존한다.
- 현재 17개에는 repo root 자체를 dest로 쓰는 원본 호출이 없다. 입력 repo와 출력
  scope가 겹치면 거부한다. 향후 해당 case가 필요하면 별도 경계를 먼저 정의해야 한다.

각 전후 snapshot은 root 포함 최대 128 entries, 파일당 1 MiB, 파일 합계 2 MiB로
제한하며 초과하면 실패한다. symlink는 따라가지 않는다. stdout/stderr는 기존
`NativeRunner`의 stream별 16 MiB 한계를 유지하고, timeout도 기존 20초다.
runner error는 별도 오류 행과 null 결과로 기록한 뒤 테스트를 실패시킨다.
누락/잘린 출력을 정상 원본 결과로 채택하지 않는다. 경로·commit·출력 hash를
정규화하거나 마스킹하지 않는다. 미래 frozen loader/expected 데이터는 작성하지 않았다.

## 부모 CI 통합 계약

부모가 준비한 Python 3.12.15 및 provider/maps/hash 전후 검증을 사용한다.
테스트 자체는 provider 전체의 불변성이나 버전 검증을 대신하지 않는다.

| 필수 ENV | 계약 |
| --- | --- |
| `BUNDLE_ORACLE_PYTHON` | 준비된 실제 interpreter의 절대 경로 |
| `OPENPILOT_ORACLE_PYTHON_LIBRARY_PATH` | 명시적 Python runtime library 경로; child `LD_LIBRARY_PATH`로 전달 |
| `BUNDLE_ORIGINAL_CAPTURE_DIR` | 부모가 미리 만든 canonical absolute·현재 UID 소유·0700 private proof directory |

원본은 `-I -B -S -X pycache_prefix=<매 호출의 새 private 경로>`로 실행한다.
원본 경로와 interpreter는 절대 경로이며 script는 원래 `__file__` root에서 실행한다.
JSON 원본 관측 행의 기대 개수는 다음과 같다.

| 기존 테스트 | 실제 원본 호출/행 |
| --- | ---: |
| `synthetic_original_native_committed_bytes_types_filter_and_worktree_exclusion` | 1 |
| `synthetic_original_native_invalid_ref_existing_destination_and_partial_failure` | 3 |
| `synthetic_original_native_all_seven_fixed_failures_match_partial_effects` | 7 |
| `actual_original_and_native_argparse_exit_and_creation_effects_match` | 12 |
| 합계 | **23** |

나머지 13개는 기존 native/주입 경계 assertion을 유지한다. 새 테스트·ignored case는 없다.
최소 focused selector는 다음과 같다. **아래 명령은 부모 CI용이며 로컬에서 실행하지 않았다.**

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features route-vault-bundle-native --lib route_vault_bundle::
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features route-vault-bundle-native --test route_vault_bundle -- --nocapture
```

로컬은 Rust syntax/fmt 및 diff만 확인한다. 실제 23행 관측·17개 성공·provider/source
전후 seal은 부모의 다음 CI 결과로 확인해야 한다. 원본 삭제·caller closure·NAS 배포나
image publication 완료를 주장하지 않는다. workflow/Cargo/생산 코드는 이 작업에서 변경하지 않는다.
