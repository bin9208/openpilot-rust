# #630 bootlog xattr 버퍼 재사용 분리 (#353)

2026-10-29 KST. 추적: [#630](https://github.com/bin9208/openpilot-rust/issues/630),
[#353](https://github.com/bin9208/openpilot-rust/issues/353).
상태: 담당 패치 작성·로컬 읽기/문법/서식/diff 완료, 최신 후보 CI 미실행.
commit/push/Actions 실행/이슈 상태 변경은 수행하지 않았다.

## 기준과 재사용 범위

- 공유 workdir: `/home/fma1/.codex/worktrees/support-services-630/openpilot-rust`.
- 브랜치: `codex/feat-630-independent-support`.
- 시작 HEAD/`origin/dev`: `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`.
- 읽기 원본: `origin/codex/feat-296-repository-python-retirement`,
  `126fdbea36598566d1040be0a35a365c909be4e4`.
- 이전 수정: `42d49f879403d3a0e025d9fdba2d3cb51b459887`.
  부모: `0bdc822d3d8594bc2f74f191ec1738e64a4adac3`.
- 이전 부모와 현재 dev의 `snapshot.rs` blob은 모두
  `e6ee69ffd929d720571e6283294e057bb22b0015`다. 따라서 동일 버퍼 변경이 필요하다.
- 담당 Rust 파일3개만 읽기 원본에서 복원했다. 전체 merge/cherry-pick은 하지 않았다.
  `snapshot.rs` blob `f52a85f8805177676b6a41668199c3b6d3d84edf`,
  `tests/snapshot_xattrs.rs` blob `cc8b419556e44a7e9bd4cbfaba8ccf17ee99162d`는
  이전 수정 및 읽기 원본과 같다. `snapshot_tests.rs`는 기존 blob
  `853dd5c3fdafc12b12377bc93e6e406c38b3050f`에 지연 할당 회귀 하나만 더해
  `efe44982c2a72f2e5fbd7ee877830436e740da6f`다.

## 변경·보존 계약

`Snapshot::capture`에 private `XattrScratch { names, value }`를 만들고
재귀 `copy_tree` → `copy_stat` → `copy_xattrs`까지 mutable reference로 전달한다.
각 버퍼는 첫 사용에65,536 bytes로 resize하고 이후 길이를 줄이지 않아 재사용한다.
무속성 파일에서는 value 버퍼를 할당하지 않는다. 캡처 반환/실패 때 두 버퍼를
해제하므로 worker로 넘어가지 않는다.

재귀 순서, metadata에 의한 링크 역참조, 실패 누적, FIFO 거부, 문자 장치 읽기,
파일 close 결과, timestamp → xattr → mode 적용 순서를 유지한다.
`listxattr`의 `NOTSUP/NODATA/INVAL`, get/set의 추가 `PERM/ACCESS` 무시와
나머지 오류 전파도 같다. names/value는 syscall 반환 길이까지만 사용한다.
복사/시작 실패의 snapshot 보존, 자식 nonzero exit 후 정리, detached worker,
Params 선택, launcher와 retention 정책은 바뀌지 않았다.

기존 독립 회귀는16파일·2디렉터리·66속성 동안 두 포인터/용량 유지,
empty/short/binary/2,048-byte 값, 이전 names/value tail 격리, missing source,
파일 mode/ns mtime, 링크 역참조, 원본 변경 후 snapshot 불변성과 실패 보존이다.
추가 회귀 `xattr_value_scratch_is_lazy_until_the_first_attribute`는 초기 용량0,
무속성 복사 뒤 value 용량0, 첫 empty 속성 뒤 names 포인터 유지와 value 할당·
정확한 빈 값 복사를 검증한다. xattr 미지원은 skip하지 않는다.

[기존 최신 댓글](https://github.com/bin9208/openpilot-rust/issues/353#issuecomment-6245974056)과
`126fdbea…:docs/naver/rust-bootlog-xattr-reuse-353.md`는 missing-API RED,
당시9테스트·strict Clippy·원본10비교 성공을 기록한다. 이는 과거 근거이며 최신 후보나
추가 회귀의 실행 결과가 아니다. allocator 호출 수나 실기 CPU/메모리 절감은 미측정이다.

## 로컬 결과

모든 명령 workdir는 위 공유 디렉터리로 명시했다. 다음 검사만 실행해 성공했다.

```sh
rustfmt --edition 2021 --check rust/crates/bootlog/src/snapshot.rs rust/crates/bootlog/src/snapshot_tests.rs rust/crates/bootlog/tests/snapshot_xattrs.rs
git diff --check -- rust/crates/bootlog docs/naver/support-630-bootlog.md
```

미변경 `rust/tools/check_bootlog_snapshot.py`, `rust/tools/bootlog_snapshot_source.py`는
`python3`의 `ast.parse`로 문법을 확인했다. 새 테스트 전체는
`git diff --no-index -- /dev/null <담당파일>`로 검토했다(차이 존재 반환1은 정상).
cargo test/build/clippy와 기능·원본 비교는 로컬에서 실행하지 않았다.
최종 읽기 대조에서 runtime/통합 테스트의 원본 바이트 일치, 기존 unit 회귀 보존과
지연 할당 회귀1개 추가를 확인했다. 문서의 sh 블록5개는 `bash -n`, RED Python은
AST parse로 문법만 확인했다. 신규 파일3개도 `git diff --no-index --check`로 확인했다.
`gh issue view --comments`의 classic project 오류는 JSON 필드를 명시해 해결했다.

## GitHub Actions 실행 명령 — 미실행

상위가 등록 `bin-actions`에서 최신 통합 후보 SHA를 고정한 독립 checkout에 실행한다.
아래 CI 블록 workdir는 `$GITHUB_WORKSPACE`다. 원본 source/submodule,
Rust1.94.0/Clippy, capnproto/libcapnp, clang/libclang, FFmpeg 개발 라이브러리,
C++ 도구체인과 기존 hardware-runtime Python 의존성을 준비해야 한다.
Params 바인딩은 libzmq 런타임/헤더와 기존 json11 staging helper가 필요하다.
기존 `.github/workflows/rust.yml`의 `hardware-runtime` 준비 단계를 재사용한다.
현 파일은 `ubuntu-24.04`를 지정하므로 이 담당 패치에서 전체 workflow dispatch를
요청하지 않는다. 공용 CI/provider/helper는 이 작업에서 수정하지 않았다.

### missing-API RED — 격리된 CI checkout에서만

후보 회귀를 둔 채 baseline `snapshot.rs`에 test module 선언만 연결한다.
아래는 예상 실패에 `XattrScratch`의 `E0433` 진단이 실제 있어야 RED로 기록한다.
의존성 준비 실패 등 임의 nonzero를 성공으로 받지 않는다. finally에서 후보를 복원한다.

```sh
cd "$GITHUB_WORKSPACE"
python3 - <<'PY'
import os
from pathlib import Path
import subprocess
root = Path(os.environ['GITHUB_WORKSPACE'])
path = root / 'rust/crates/bootlog/src/snapshot.rs'
candidate = path.read_bytes()
baseline = subprocess.check_output([
    'git', 'show',
    'e445c9d19c3041bc79d8624589e2eb6ba7d061c6:rust/crates/bootlog/src/snapshot.rs',
], cwd=root)
try:
    path.write_bytes(b'#[cfg(test)]\n#[path = "snapshot_tests.rs"]\nmod tests;\n' + baseline)
    result = subprocess.run([
        'cargo', '+1.94.0', 'test', '--manifest-path', str(root / 'rust/Cargo.toml'),
        '-p', 'openpilot-bootlog', '--lib', '--locked', 'xattr_scratch', '--', '--nocapture',
    ], cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    (Path(os.environ['RUNNER_TEMP']) / 'bootlog-missing-api-red.log').write_text(result.stdout)
    assert result.returncode != 0, 'baseline unexpectedly accepted the scratch API'
    assert 'XattrScratch' in result.stdout and 'error[E0433]' in result.stdout, result.stdout
finally:
    path.write_bytes(candidate)
PY
```

### 최신 후보 회귀·strict Clippy·실행 파일

```sh
cd "$GITHUB_WORKSPACE"
git rev-parse HEAD
cargo +1.94.0 test --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-bootlog --locked -- --nocapture
cargo +1.94.0 clippy --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-bootlog --all-targets --locked -- -D warnings
cargo +1.94.0 build --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-bootlog --example bootlog_snapshot -p openpilot-process-supervision --bin openpilot-process-child --locked
```

### 미변경 원본 helper와 filesystem/process 10시나리오 비교

```sh
cd "$GITHUB_WORKSPACE"
export PYTHONPATH="$GITHUB_WORKSPACE:$GITHUB_WORKSPACE/rust/tools${PYTHONPATH:+:$PYTHONPATH}"
python3 "$GITHUB_WORKSPACE/rust/tools/build_params_python.py" --output "$RUNNER_TEMP/bootlog-630-params"
python3 "$GITHUB_WORKSPACE/rust/tools/check_bootlog_snapshot.py" \
  "$GITHUB_WORKSPACE/rust/target/debug/examples/bootlog_snapshot" \
  "$RUNNER_TEMP"/bootlog-630-params/params_pyx*.so \
  "$RUNNER_TEMP/bootlog-630-snapshots" \
  --launcher "$GITHUB_WORKSPACE/rust/target/debug/openpilot-process-child"
```

비교는 일반 사용자로 실행한다. root이면 unreadable/cleanup 실패 조건이 달라진다.
출력은 새 디렉터리를 사용한다. comparator의15초/5초 대기와 모든 assert를 유지한다.
10개는 `ordinary`, `nonzero_child`, `missing_binary`, `nonexec_binary`, `broken_link`,
`fifo`, `character_device_link`, `cleanup_failure`, `unreadable_directory`,
`detached_parent_exit`다. `bootlog_snapshot_source.py`는 원본
`openpilot/system/manager/helpers.py::save_bootlog`를 AST 추출해 실행한다.
복제 oracle이나 mock worker로 바꾸지 않는다.

읽기 확인한 미변경 SHA256:

| 파일 | SHA256 |
| --- | --- |
| `rust/tools/check_bootlog_snapshot.py` | `948b1855341a87cb218edd1a48fdaecaa9390f13e1078e4f55dfd44ccbbc8118` |
| `rust/tools/bootlog_snapshot_source.py` | `3d4ed20981bfac2d51950be49c33d412ee6916ec8c647ec4085a448d507893e8` |
| `openpilot/system/manager/helpers.py` | `9be4a7b86b9953bd3e503d669ce9c70bb03475731a6322e47def1fe59368fd4a` |

### ARM

등록 `bin-actions`의 기존 aarch64 QEMU/PRoot 환경 안에서 위 회귀·Clippy·실행 파일
빌드·10비교 명령을 동일하게 실행한다. 검사 프로세스 `uname -m`이 `aarch64`이고
원래 바인딩/launcher도 그 환경에서 빌드되어야 한다. generic GNU cross-build만으로
filesystem/process 실행을 대체하지 않는다. 기존 generic aarch64 게이트도 보존한다.
기존 ARM provider/FFmpeg 설정을 사용한 집중 cross-build 명령:

```sh
cd "$GITHUB_WORKSPACE"
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo +1.94.0 build --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" \
  -p openpilot-bootlog -p openpilot-process-supervision --bins --examples \
  --target aarch64-unknown-linux-gnu --release --locked
```

## 미해결·상위 인계

- 런타임 소스에 필요한 공용 변경/새 패키지 의존성은 발견하지 않았다.
  Cargo.toml/Cargo.lock/workspace/공용 CI/helper/provider/bootstrap/installer/모델/안전
  경로는 이 담당 작업에서 변경하지 않았고 다른 담당자의 변경은 보존했다.
- 상위는 최신 통합 후보 SHA의 RED/GREEN·원본10비교·strict Clippy·ARM 및
  필수 PR/병합 후 결과를 Actions URL로 기록해야 한다. 현재 CI URL은 없다.
- CI 준비 실패는 해당 provider/바인딩 경로와 정확 오류를 #598 소유자에게 전달한다.
  원본 비교 생략이나 timeout/허용오차 완화로 통과시키지 않는다.
- 상위가 일괄 검토·커밋·PR/정상 dev 통합한다. 실기 부팅/기록/업로드 수용과
  성능 측정은 사용자 소유이며 이번 소스 분리로 완료하지 않는다.

Docs-Not-Needed: 사용자 설정·외부 동작 변화 없는 내부 버퍼 재사용과 검증 기록.
