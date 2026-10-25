# #523 source100 buffer 원본 corpus inert 감사

작성: 2026-10-26. 상태: **source100 actual native6·original1·strict Clippy 성공 및 원문 감사 완료. 후속 frozen reader 구현·GHA UNRUN, production caller 전환·원본 제거 수용은 별도 미완료**.

감사 대상 source는 `1000464f57239a1d15bfba3bdb6c01550f12d8a0`이다.
[run40231559285/job121285061989](https://github.com/bin9208/openpilot-rust/actions/runs/40231559285/job/121285061989)과
[artifact12613507032](https://github.com/bin9208/openpilot-rust/actions/runs/40231559285/artifacts/12613507032)를 읽었다.
현 작업 트리나 Hypatia의 후속 stage collector를 이 source의 증거로 섞지 않았다.
추가 로컬 기능·Cargo·프로젝트 CLI·Python oracle·모델·컴파일러 실행은 **0회**다.
ZIP/JSON/로그/고정 Git blob 읽기와 hash·경로·CRC·구조 비교만 수행했고, artifact의
`oracle.py`는 데이터로만 읽었다. 최초 감사 단계에서는 새 문서 이외의 source/WF/test/pin/ledger를
수정하지 않았다. 아래 후속 구현은 사용자에게 별도로 승인받은 범위다.

## 원격 결과와 ZIP 인증

- API의 head SHA, ZIP의 `SOURCE_COMMIT`, `runner-and-owner.txt`, observation의 `github_sha`가 모두 위 source와 일치한다.
- run은 `push`, attempt1, `.github/workflows/python-rust-523-native-units.yml`이며 해당 job과 모든 단계는 `completed/success`다. 실제 runner는 `bin-actions`, labels는 `self-hosted/Linux/X64/dev-md-local`이다.
- job 실행 시각은 `2026-10-25T20:45:20Z`~`20:47:06Z`다. 이 시간은 빌드·수집·봉인·업로드를 포함한 job 시간이며 buffer 알고리즘 성능치가 아니다.
- API digest 및 독립 전체 ZIP SHA256: `b8eb67c19d759265cb25e5fca54aee1979c1b79fe11dc3435a098e934cdea97e`.
- ZIP은 정확 **1595256B**, 47 regular-file member, 비압축 합계 **7290291B**다. 모든 member를 메모리로 읽어 CRC와 개별 SHA256을 확인했다. 추출·실행은 하지 않았다.
- 모든 member name은 상대 canonical POSIX 경로다. 절대경로·빈 component·`.`·`..`·backslash·NUL·중복 이름·symlink·특수 파일은 없다. 이 판정은 ZIP member 경로에 대한 것이다.
- 캐시는 `.analysis/scratch/2026-10-25-523-completion/ci/12613507032.zip` 하나이며 요청한 **10MiB** 상한 이내다.
- 완료 job log도 메모리로 읽었다: **109180B**, SHA256 `e6a43b3e4978c96050ea36cffdd320e51eec28d778301e31c1b60322e8a5c29d`. 별도 파일로 저장하지 않았다.

## 실제 검사 결과와 원본 호출 수

`native-exit-codes.txt`에는 아래 세 label이 정확 한 번씩 있고 모두0이다.

| label | rc | 실제 보존 로그 |
| --- | ---: | --- |
| `compiler-buffer-units` | 0 | 6 passed, 0 failed, 1 ignored; 기존 original selector는 이 호출에서 실행하지 않음 |
| `original-buffers` | 0 | `original_python_buffer_tables_match_native` 1 passed, 0 failed, 0 ignored, 6 filtered out |
| `compiler-buffer-clippy` | 0 | 대상 `--lib --test compiler_buffers -- -D warnings`, 정상 종료 |

두 test 실행 로그의 test ELF 경로는 동일한 `compiler_buffers-286b7c8751b68352`다.
완료 job log에서 결과 행이 두 번 보이는 것은 `tail` 뒤 `grep`이 같은 결과 행을 다시
출력하기 때문이다. artifact의 각 원시 label log에는 결과가 한 번씩 있다.

source100 WF의 buffers branch는 기존 native6, original1, Clippy를 `run_once`로 각각
한 번 호출한다. 재시도·skip·실패 성공 승격은 없다. original1 안에서는 `Command::new`
한 곳에서 child 하나를 spawn하고 같은 `request_bytes`를 `write_all`한 뒤 `wait`한다.
이 child가 descriptor32와 recipe147 전체를 처리하고 stdout 하나를 출력한다.
수집 receipt의 request hash, 보존 request, 같은 child의 stdout/stderr hash가 일치한다.
이는 source와 단일 관측 receipt를 결합한 호출 수 근거이며, 시스템 전체 exec syscall
trace가 보존된 것은 아니다.

Python은 canonical `/usr/bin/python3.12`, **8020928B**, SHA256
`e50d468e8b0adfb05733f5b87b3cff34829c4a8c1aea50c865aa8bdfe4bb150f`이며
admission version proof는 `Python 3.12.3\n`이다. original child argv는
`/usr/bin/python3.12 -I -B -c <보존 oracle.py 원문> <기존 root 인자>`다.
별도 WF admission의 `-I -B -S --version` 1회와 자원 admission용 Python 호출은 존재하므로
**전체 job의 Python 프로세스가 한 개뿐이었다고 주장하지 않는다**. collector 안에서
version 조회나 두 번째 original 수집을 추가하지 않았다.

원래 argv의 마지막 root 인자는 아래와 같이 `../../..` 표기를 포함한다.

```text
/home/actions/actions-runner/_work/openpilot-rust/openpilot-rust/.diag296-source-40231559285-1.AaaMkI/source/rust/crates/model-runtime/../../..
```

이를 정규화하면 receipt의 canonical `source_root`인
`/home/actions/actions-runner/_work/openpilot-rust/openpilot-rust/.diag296-source-40231559285-1.AaaMkI/source`와 같다.
source100 collector는 먼저 이 root와 admitted `DIAG_SOURCE`의 canonical 일치를 확인한다.
모든 witness 경로는 canonical absolute다. **보존 argv 문자열 자체가 no-`..` 형식이라는
주장은 하지 않는다.** ZIP member traversal이나 서로 다른 source root를 발견한 것은 아니며,
실제 argv 원문을 정규화해 덮어쓰지 않았다.

## raw 디렉터리와 cap

artifact의 최상위가 job `proof/`의 내용이다. raw 하위 구조는 정확 다음 7파일이다.

```text
compiler-buffer-original/
  observation.json
  oracle.py
  python.sha256
  python.version
  request.json
  stderr.bin
  stdout.bin
```

7파일 모두 ZIP mode `0100600`이고 합계 **375016B**, 최대 파일은 stdout의 **270242B**다.
관측의 `record_limit_bytes=1048576`, `total_limit_bytes=8388608`과 모두 맞는다.
source100은 디렉터리를 새0700, 파일을 `create_new`/0600/`O_NOFOLLOW`로 만들며,
기존 디렉터리나 파일을 덮어쓰지 않는다. ZIP에 별도 directory entry는 없으므로
디렉터리 mode의 독립 ZIP metadata가 있다고 확대하지 않는다.

child의 `pre_exec`는 `RLIMIT_FSIZE` soft/hard를1MiB, `RLIMIT_CORE`를0으로 설정한다.
stdout/stderr는 pipe 무제한 capture가 아니라 같은 child의 새 regular file로 직접 받는다.
부모는 전체8MiB 및 각 record1MiB를 확인한 뒤 receipt를 쓴다. 해당 run은
`returncode=0`, `exit_code=0`, `signal=null`, `file_size_limit_signal=false`, `input_error=null`이다.
신호 종료·truncation·stdin 오류를 성공으로 숨긴 흔적은 없고 stderr도0B다.
이번 자료는 한계 초과 실패를 실제 주입해 검증한 음성 시험의 증거는 아니다.

| raw 파일 | bytes | SHA256 |
| --- | ---: | --- |
| `observation.json` | 39344 | `e8e52475eba99d6e13287ecb957ec650c8e4d64b1e517caea1fd3b08fdddf257` |
| `oracle.py` | 4694 | `2b4747903018e15f3f99eabf62a6fb052f4899bfd0bee1ac44ce7bb5501cf371` |
| `python.sha256` | 86 | `74a2bf284a5116d43270762ab1766fe74421da338fa0889fcd2c188202eae773` |
| `python.version` | 14 | `5b3e43dc38ca01e3f5a7854ba50d4330864b0c1e0f646650b2c78cf072acc366` |
| `request.json` | 60636 | `809ff0aba501e93a4bab0d86fd8066a77d72881580cfc72408e1c1ea19fd15fb` |
| `stderr.bin` | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `stdout.bin` | 270242 | `fc019a80d96e11a95cc8d1573b94cbbb4ea06f30b56fdf71efa6f29d5e149593` |

## 179경계와 기대값 독립성

`request.json`은 정확 `descriptors`32·`cases`147의 두 top-level key만 갖는다.
`stdout.bin`은 정확 `source_sha256`·`opened_devices`·`descriptors`·`traces`를 갖는다.
중복 JSON key나 비정상 NaN/Infinity 상수는 없다. 원시 bytes를 다시 직렬화한 파일로
바꾸지 않고 그 bytes의 hash를 감사했다.

- descriptor는 single30(15 device spelling×allocated2)·multi1·other1이다. 출력32개에서 `(descriptor, host_buffer)` 조합은 `(true,true)`12, `(true,false)`5, `(false,false)`15다.
- recipe는 host69·offline78이다. trace147개가 동일 순서로 존재하며 각 recipe의 operation 수와 trace row 수가 모두 일치한다.
- 총 **959 operation/trace row**: read298·write405·view256. 결과는 `ok`946·`error`13이다. 이13개는 입력에 포함된 원래 오류 경계 결과이며 original 프로세스 실패를 뜻하지 않는다.
- 실제 `opened_devices`는 `['CPU']`다. descriptor의 allocated 상태는 source에 명시된 synthetic sentinel 조작이다. GPU/QCOM allocator나 모델 실행을 입증하지 않는다.

원래 request의 안정된 index 구간은 다음과 같다. 표의 구간은 양 끝 index를 포함한다.
새 기대값을 생성하지 않고 source100의 생성 순서와 보존 입력을 대조했다.

| request 위치 | 원래 경계 |
| --- | --- |
| `descriptors[0..29]` | 15 device spelling의 allocated false/true |
| `descriptors[30..31]` | MultiBuffer·기타 object 거절 |
| `cases[0..63]`, `[68..131]` | host/offline 각각 6 interior byte의 64 write mask, edge read와 중복 alias view |
| `cases[64..66]`, `[132..134]` | overlap·부분 read/write·전체 write 뒤 read |
| `cases[67]`, `[135]` | allocation encounter 순서, all-zero 생략, raw `00 00 00 80` 및 중복 내용 |
| `cases[136..138]` | missing/short/long initializer의 lazy 오류와 전체 write 뒤 성공 |
| `cases[139..142]` | CPU/NPY/PYTHON/QCOM:7 offline descriptor |
| `cases[143..145]` | 다른 base identity, alias initializer identity, zero-length의 검증 순서 |
| `cases[146]` | host 원문 bytes 재수집과 이미 write한 부분 보존 |

`oracle.py` **4694B/SHA `2b4747903018e15f3f99eabf62a6fb052f4899bfd0bee1ac44ce7bb5501cf371`**는
source100 테스트의 `ORIGINAL_BUFFERS` 문자열과 bytes 전체가 같다. 이것을 실행하지 않았다.
GHA에서는 고정 source의 원래 schema·buffer table class/method를 AST로 추출하여 genuine
Tinygrad Buffer에 적용한다. 원래 method body를 native expected로 교체하지 않는다.
`Compiler.compile`/`compile_cached`는 컴파일 금지 함수로 막고 device open은 CPU만 허용한다.
입력 request에는 base/data/initializer/operation과 descriptor 조건이 있으며 native trace나
native 계산 출력은 Python으로 전달하지 않는다. Rust `native_trace`와 descriptor helper는
그 후 원본 stdout의 기대값과 비교한다. 즉 입력 설계는 공통이지만 기대값의 생산자는
원래 Python method다.

보존 stdout의 여섯 original source SHA는 receipt before/after 및 아래 source100 Git blob과
모두 일치한다. 위 실제 original1 PASS는 그 뒤 기존32 descriptor·147 trace의 native
assertions까지 성공했다는 증거다. 이번 inert 감사에서 buffer 알고리즘을 다시 실행하거나
원본과 native의 결과를 새로 만들지는 않았다.

## source/index/Python/SDK 봉인

observation의 before/after map은 각각 **27개**이고 전체 JSON 객체가 동일하다.
22개 source + admitted Python + job owner + SOURCE_COMMIT + Python hash/version proof다.
모든 source witness의 길이·SHA를 해당 source100 Git blob과 별도로 대조했다.
선택 witness는 canonical regular file, nlink1이며 dev/inode/size/mode/uid/gid/mtime/ctime도
전후 같다. Python admission proof의 top-level/하위 복사본은 bytes 전체가 같고,
Python full SHA와 owner `40231559285:1:native-units` hash도 맞는다.

`index-before.bin`/`index-after.bin` 및 `index-stage-before.bin`/`index-stage-after.bin`은
각각 바이트 단위로 같다. **19554개 path**는 중복·traversal이 없고, stage 전부0이다.
mode/blob ID/path를 source100 `git ls-tree -r`와 독립 비교하여 모두 일치했다.
전체 source tar의 전후 기록 digest는
`3ab1f3d5271bd5fec44309b8bee4627cc6453520e3d2668c7533aeb951e36314`로 같다.
이 tar digest는 GHA가 계산한 receipt이며 로컬에서 전체 source tar를 재생성한 것은 아니다.
`focused-source.sha256`의15개 source SHA도 source100 Git blob과 대조했다.
최종 source/index seal 단계와 SDK seal 단계는 API에서 success다.

SDK는 기존 global `/home/actions/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu`다.
`prepared-rustc.txt`는 rustc1.94.0/commit `4a4ef493e3a1488c6e321570238084b38948f6db`,
`x86_64-unknown-linux-gnu`, LLVM21.1.8이고 cargo1.94.0이다.
이는 Rust SDK의 LLVM 버전으로, 모델 LLVM18 실행 증거와 혼동하지 않는다.

whole SDK file manifest는 **294 regular files**, roster는 그294파일과28디렉터리의
**322항목**이며 모든 file에 SHA 행이 정확 하나 있다. 경로는 SDK root 안에서 canonical이고
symlink·특수 파일·거절 record는 없다. 네 official program pin(rustc/cargo/rustfmt/clippy-driver),
whole file hash manifest, identity/mode/time을 포함한 roster가 각각 전후 bytes 동일하다.
SDK binary 자체는 artifact에 없으므로 이 감사가 원격 SDK294파일을 다시 읽어 hash했다는
주장은 하지 않는다. 실제 GHA의 SDK 검증 성공과 보존된 전후 증거를 함께 확인한 것이다.

`compile-profile.txt`는 jobs1/incremental0/dev_debug0/test_debug0,
`--no-default-features --features native-skip-miri`를 기록한다.
WF는 wrapper/RUSTFLAGS/LD_PRELOAD/LD_LIBRARY_PATH를 해제하고 위 SDK의 절대 cargo를 쓴다.
전후 source·SDK 성공 기록을 buffer source 제거 성공으로 승격하지 않는다.

다섯 자원 admission은 모두 pass이며 failures가 비어 있다. 최초 같은 FS의 available은
34217885696B, 합산 remaining은8049684480B이고 source2GiB/target4GiB/registry1GiB/proof512MiB
예산을 함께 본다. 마지막 available33981722624B, remaining7813648384B다.
메모리 available은 각 admission에서4132845568~4135829504B, 계획은2GiB이며
`measured_peak_guarantee=false`다. peak RAM 측정이나 다른 큰 모델 작업의4GB 수용을 뜻하지 않는다.

## freeze 준비 판단과 남은 범위

**이179경계의 독립 원본 corpus를 준비하는 데 필요한 관측 원문은 갖춰졌다.**
성공한 불변 original 실행을 다시 반복하지 않고, 위 archive/source/request/stdout/oracle 및
22 source pin을 그대로 고정해 후속 frozen fixture를 만들 수 있다. 안정된 연결은
`descriptors[i] → stdout.descriptors[i]`, `cases[i] → stdout.traces[i]`다.
959개 trace row의 순서·views·allocations·weights·오류 결과를 모두 보존해야 하며,
request hash나 raw stdout을 native 출력으로 재생성해서는 안 된다.

최초 감사 단계에서는 새 frozen fixture/manifest/consumer를 만들거나 실행하지 않았다.
그 뒤 승인된 고정 raw reader 구현은 아래에 분리해 기록한다. 해당 consumer와
source-absence·변조·오류 전파·소유/정리 경계의 실제 GHA 수용은 남아 있다.
source100 이후 collector 변화는 별도 source 증거로 구분한다.

아래 한계는 계속 남는다.

- observation 자체는 `collector_only=true`, `retirement_credit=0`이다. native6/original1 PASS와 수집 receipt를 독립적으로 결합해야 하며 수집만으로 source 삭제를 수용하지 않는다.
- 22 witness는 선택한 원본/import·CPU copy 경계다. 전체 Python stdlib/import/DSO closure, syscall exec roster, native test ELF hash는 이 artifact에 없다.
- argv root의 `../../..` 표기는 실제 보존 원문이다. 새 검증기가 모든 문자열의 lexical no-`..`까지 요구한다면 별도 정규 경로 필드와 구분해야 하며 과거 argv를 수정해 인증해서는 안 된다.
- CPU uint8 copy 및 table/descriptor/initializer 의미의179입력이다. kernel/model/compiler 실행, 실제 QCOM/GPU allocation, #476 driver live prestate/whole2212, 플랫폼 전체 수용 증거가 아니다.
- archive API 만료는 `2026-11-08T20:46:55Z`다. 인증된 로컬 캐시가 있지만 장기 freeze의 저장·Git 통합은 부모 후속 소유다.

## 선택 source22 전체 pin

각 항목은 before/after identity가 같고 source100 Git blob의 길이·SHA와 일치했다.

| source path | bytes | SHA256 |
| --- | ---: | --- |
| `rust/Cargo.lock` | 159864 | `9586516b4adda7abc3f739f2c3360a6cd2370e8f2d785a6b36a4adfcf54f6b9a` |
| `rust/crates/model-runtime/Cargo.toml` | 1605 | `53d766c5fa51dab5e4be7c7b793e82d591a0bf7970784b68638c9b5159e56866` |
| `rust/crates/model-runtime/src/compiler_buffers.rs` | 3332 | `1588afe7d4d6d69bac36df96653405802d569e730812dd001ffc874ebcdf42da` |
| `rust/crates/model-runtime/src/export.rs` | 24746 | `7dc0993039db5247aa0e71d7c122c46b393e30a8e02724f8367a3b26ec6f6e8c` |
| `rust/crates/model-runtime/src/lib.rs` | 1118 | `9c18a2434644ea72ebbd5d2d23c706084391775995e0567c6a0d1809f8f1d741` |
| `rust/crates/model-runtime/tests/compiler_buffers.rs` | 31176 | `31c68a1b54d0162837329b374752a765b22a96710a385458f527a0c6fd9dd950` |
| `rust/tools/model_export/buffers.py` | 2577 | `9429df49f408cda89f5b862fc0d51982668234cc18cb28ca0e83fd3d1a5bc7de` |
| `rust/tools/model_export/offline_buffers.py` | 1575 | `9ab6c8c777b04d374750779fabfa75bb2b2af64fb6f8ce3ff8edd7a14138de4f` |
| `rust/tools/model_export/schema.py` | 2611 | `720abcc396852a395689c4d37f9487822f951166a36b5efe8586dce5f656c6a9` |
| `tinygrad_repo/tinygrad/__init__.py` | 674 | `fc24d5d0df1b1a14bbee0bd2641a61ca5f34ec7d83f1b0b755adf6eb83d5c0be` |
| `tinygrad_repo/tinygrad/device.py` | 20639 | `7c28ae3854a3f009fc6eddfb52f03b3d382a23b581d94a3c4741a12bdc79f008` |
| `tinygrad_repo/tinygrad/dtype.py` | 20162 | `de4bb7980fac9c09f851374fc6e8c57441071fca6d98dc92c6e984170bc58008` |
| `tinygrad_repo/tinygrad/helpers.py` | 32632 | `24c347d2137c92e1dc35d78cd9df45b38d31cc4d34729a35215f8822b3f2f6fb` |
| `tinygrad_repo/tinygrad/renderer/__init__.py` | 4405 | `22b219d73973e2591201662ae4a5c4ebd8688e51313a0ad4f55d50ae513828f7` |
| `tinygrad_repo/tinygrad/renderer/cstyle.py` | 37506 | `13d806a4674ec87ffc94959b5825bb5915a7581b9c6652a3ddfe8a52120470be` |
| `tinygrad_repo/tinygrad/runtime/autogen/libc.py` | 124407 | `89004d7b83537131ef45a55191eb1ce2708ef5e3f649abf656abdeeb193d42db` |
| `tinygrad_repo/tinygrad/runtime/ops_cpu.py` | 8395 | `012f3d932abba9e14f82aedefe0ca52243eba1728f847c38cae54ae05b171a16` |
| `tinygrad_repo/tinygrad/runtime/support/elf.py` | 5263 | `ed0340b507c8fb23f1fa1ae1b3ca1297006d70c289a016233d9f1d5291801aa8` |
| `tinygrad_repo/tinygrad/runtime/support/hcq.py` | 32420 | `86ccce42fce7fe59f4cd2e345d9186b7241e44a00763ee51712c16a10b521bff` |
| `tinygrad_repo/tinygrad/runtime/support/memory.py` | 15268 | `0457d300a3f149ba11b2da97240f7c7b6c0bb952e7ac4779dea2cfc0c44c81e2` |
| `tinygrad_repo/tinygrad/uop/__init__.py` | 4829 | `8de567435d8c37f5472650e96409660063890d7a1a6dea32054396da6b6749ac` |
| `tinygrad_repo/tinygrad/uop/ops.py` | 94967 | `d4b454cc4577a23be8d688aed874b9911992332e21bea7d7567974c9dc7c197a` |

## ZIP47 전체 inventory

아래 SHA는 archive가 말하는 값의 복사가 아니라 이번 감사에서 각 member 원문을 읽어
계산한 SHA256이다. raw7 이외 source/index/SDK/log의 재사용도 이 전체 roster로 식별한다.

| ZIP member | bytes | SHA256 |
| --- | ---: | --- |
| `SOURCE_COMMIT` | 41 | `99c5b59fd351307b021692eeec8962a56b51a57643aa472af2119b7eeab28af2` |
| `admission-after-native.json` | 1653 | `91b17f93f3ef27aade6647a83b34fec8820a0ea674a49951c82339c2548d7f87` |
| `admission-before-sdk.json` | 1640 | `7c405a827c9cae94fa901bd3d67d8568b6884ab4481af93ef282b4c3d2ee3cc5` |
| `admission-compiler-buffer-clippy.json` | 1663 | `47ff9584d5841e4b4f38bbe2e9f94f57d1b6fc927ef2e0f0e96114745c4694a1` |
| `admission-compiler-buffer-units.json` | 1651 | `c4a717d9009929ec91befde7798f5817191b7cb7a14f2a16cca7510709b71684` |
| `admission-original-buffers.json` | 1657 | `65504b31da30b7813d569b1e15fa2c1a5c4a1e397a611685e982606aea797876` |
| `buffer-oracle-python.sha256` | 86 | `74a2bf284a5116d43270762ab1766fe74421da338fa0889fcd2c188202eae773` |
| `buffer-oracle-python.version` | 14 | `5b3e43dc38ca01e3f5a7854ba50d4330864b0c1e0f646650b2c78cf072acc366` |
| `compile-profile.txt` | 274 | `a901e102c72cbe7f64bb44173402723120fda19d33e3148def1c35cc6c9e5aee` |
| `compiler-buffer-clippy.log` | 1007 | `0e545d87e251d5100abeec4b0058ef22f618c15b507511d44737b5d7638c2d0e` |
| `compiler-buffer-original/observation.json` | 39344 | `e8e52475eba99d6e13287ecb957ec650c8e4d64b1e517caea1fd3b08fdddf257` |
| `compiler-buffer-original/oracle.py` | 4694 | `2b4747903018e15f3f99eabf62a6fb052f4899bfd0bee1ac44ce7bb5501cf371` |
| `compiler-buffer-original/python.sha256` | 86 | `74a2bf284a5116d43270762ab1766fe74421da338fa0889fcd2c188202eae773` |
| `compiler-buffer-original/python.version` | 14 | `5b3e43dc38ca01e3f5a7854ba50d4330864b0c1e0f646650b2c78cf072acc366` |
| `compiler-buffer-original/request.json` | 60636 | `809ff0aba501e93a4bab0d86fd8066a77d72881580cfc72408e1c1ea19fd15fb` |
| `compiler-buffer-original/stderr.bin` | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `compiler-buffer-original/stdout.bin` | 270242 | `fc019a80d96e11a95cc8d1573b94cbbb4ea06f30b56fdf71efa6f29d5e149593` |
| `compiler-buffer-units.log` | 3119 | `5fcda97e290e9aa6084e78aa47e7e2531e41bd493adaa73bf96ae12bc7e4c549` |
| `disk-after.txt` | 249 | `5fcfe2d3107fbbef791630cbc1a7a11b9d3de248ae73997705451e152cfb55ca` |
| `disk-before-checkout.txt` | 249 | `ef2d05a51d1f07ac6a919bd7894612bc32b8355f1306c92266cadf90e3ccefc7` |
| `focused-source.sha256` | 1563 | `d4266bdc3bc8e5d9f098dd7558d27806c3d91d870fb6656e3424cf862a4c2ace` |
| `index-after.bin` | 1174517 | `6a05cc47bb556c0ee24ae92a3f56d21db6630b2a0e93c87825c22356cce19be8` |
| `index-before.bin` | 1174517 | `6a05cc47bb556c0ee24ae92a3f56d21db6630b2a0e93c87825c22356cce19be8` |
| `index-stage-after.bin` | 2152217 | `4b96876ff35c161e3eab6997e78a513e027fb42ad88cb2c2e149f67ffb7e1607` |
| `index-stage-before.bin` | 2152217 | `4b96876ff35c161e3eab6997e78a513e027fb42ad88cb2c2e149f67ffb7e1607` |
| `memory-after.txt` | 1586 | `0a919219b0daad405fb55bd5610174028aebbaa219177f0ee8418aab99d4d45c` |
| `memory-before-checkout.txt` | 1586 | `711134fc80db45560c510727ed9a328cb2be7cb0166207e1c92c9d03d099b962` |
| `native-exit-codes.txt` | 68 | `fed94c5783ee945ddad7b7bd6d0d31f72dcb583d7bbac92a0f2df7f416c56f47` |
| `original-buffers.log` | 1000 | `7dd63cbff44fa00a3a1a81df3afd5ebc69cdaedf752fc7de9a003c05d2b4f472` |
| `outcomes.txt` | 289 | `f720b0f9bf8ddc438b4251c8fb9c6c909b47a9e9442af654c59c0278d705dc0a` |
| `prepared-cargo.txt` | 36 | `6a36a8bf8bd42aa197681c3cbafaf785b3a6cf5ed0361cc7240a31c458d10db9` |
| `prepared-rust-files.after.sha256` | 56647 | `34ca36a4a7d3ec78878ae59c690755c9f3e513c816a05e5f2a71ab06f1b1191a` |
| `prepared-rust-files.before.sha256` | 56647 | `34ca36a4a7d3ec78878ae59c690755c9f3e513c816a05e5f2a71ab06f1b1191a` |
| `prepared-rust-programs.after.sha256` | 574 | `02dc3249995ea4f61a85de89ebaff8c4f2e642f1a8567d76ffc3078472ac536a` |
| `prepared-rust-programs.sha256` | 574 | `02dc3249995ea4f61a85de89ebaff8c4f2e642f1a8567d76ffc3078472ac536a` |
| `prepared-rust-rejected.after.bin` | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `prepared-rust-rejected.before.bin` | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `prepared-rust-roster.after.bin` | 63514 | `1216a3a311069910f96a8f3d4cc7167b9514cd9ce16e5035da6d9d3169bb3942` |
| `prepared-rust-roster.before.bin` | 63514 | `1216a3a311069910f96a8f3d4cc7167b9514cd9ce16e5035da6d9d3169bb3942` |
| `prepared-rustc.txt` | 196 | `b75985398e5509a90a7d5d68f8ec6690675f703c5cfa11893b04fc9c3fceca69` |
| `runner-and-owner.txt` | 142 | `5de7c9d292b9202e02a485edc0b7b1763187b0623130ee0f8aa92c1939b1e422` |
| `source-after.sha256` | 68 | `4a5b807bbb25a29b4e023c67b0302cb7ace0767c58dd96cb4dbcb48e93a09903` |
| `source-before.sha256` | 68 | `4a5b807bbb25a29b4e023c67b0302cb7ace0767c58dd96cb4dbcb48e93a09903` |
| `source-growth-after-checkout.json` | 87 | `f024d2eb241256053ddb0b9d5f9e2cf1a45b4b2d00779cf0e3d9c77c871a2921` |
| `source-growth-before-checkout.json` | 73 | `a6b5f3666747b9a405b1f070e85c7cf27c6b6b67277fe84feb33310d2795296d` |
| `source-ownership.txt` | 260 | `7c054ceedcbd530e68f425bcedfe8cb335d4b7d4bec23fcdb294e9772184b6ad` |
| `source-preflight.status` | 12 | `bde294368bfed77c2cddf8cec271d398aee9cdbab3b26e1059281bd33adb0120` |


## 2026-10-26 승인 후속: live original1을 frozen1로 교체

사용자가 test 소유권을 이전하고 위 source100 raw179만 읽는 구현을 승인했다.
`rust/crates/model-runtime/tests/compiler_buffers.rs`의
`original_python_buffer_tables_match_native`와 live `BufferObservation` collector, Python snippet
문자열, 원본22파일 read·환경 admission·spawn/setrlimit/wait 코드를 제거했다.
새 `frozen_original_buffer_tables_match_native`는 일반 `#[test]`이고 `#[ignore]`가 없다.
`native-skip-miri` feature에서 **기존 native6 + frozen1 = 총7, ignored0** 정의다.
이 수는 source/rustfmt 확인 결과이며 새 test의 actual PASS를 주장하지 않는다.

### 원문 보존과 reader 계약

새 `tests/fixtures/compiler-buffer-original/`에는 인증 ZIP의 raw7과 출처 README만 있다.
원문 **375016B**를 재인코딩·pretty-print·줄바꿈 변경 없이 복사했다. 일곱 SHA는 위 raw 표와
같다. 유일한 mapping은 archive `oracle.py` → fixture `oracle.txt`이며 길이4694B와
SHA `2b4747903018e15f3f99eabf62a6fb052f4899bfd0bee1ac44ce7bb5501cf371`가 같다.
추가 `.py` 파일은0이고 `.txt`는 실행하지 않는 provenance 데이터다.
과거 `observation.json` 안의 argv와 과거 경로도 원문 그대로 남겼다.

Rust reader는 seven `include_bytes!`로 고정 자료만 읽는다. 현재 원본 파일이나 Python을
찾아보는 fallback은 없다. 다음을 native 결과 비교 전에 검증한다.

- 7개 record 각각의 정확 길이·SHA256, 각1MiB/전체8MiB cap 및 실제 합계375016B.
- 기존 입력 생성기의 직렬화 bytes와 고정 `request.json` 전체 일치. expected는 원본 `stdout.bin`에서만 읽는다.
- source100 SHA, 역사적 root/argv, schema1/kind, collector-only/retirement-credit0 및 원본 rc0·signal 없음·stderr0·stdin 오류 없음.
- Python3.12.3 proof와 canonical interpreter 경로·8020928B/full SHA. 과거 owner/SOURCE_COMMIT/proof도 receipt의 길이·SHA/path와 연결한다.
- 과거 source22의 정확 path/길이/SHA와 regular/nlink1, before/after27 witness 전체 일치. source100 당시 test 자체 SHA도 과거 pin으로 검증하며 변경된 현재 test를 다시 열지 않는다.
- request의 descriptor32/recipe147, stdout의 source6/opened CPU/descriptor32/trace147, 총959 row 및 각 recipe의 operation 수.
- 보존 `oracle.txt` 전체 bytes와 과거 `argv[4]` 일치. 자료를 실행하거나 현재 source의 Python method를 import하지 않는다.

이 reader의 런타임 파일/환경/subprocess API는0이다. 따라서 역사적 Python 설치나 source22가
현재 호스트에 있어야 한다는 의존성이 없다. 현재 source/SDK 봉인은 부모의 기존 WF가
새 실행 source에 대해 별도로 수행한다. 과거 observation의 `retirement_credit=0`을 바꾸지 않았다.

### 기존 본문 불변 검사

source100 Git 원문과 아래 구간의 bytes 전체가 일치했다. 기능을 실행한 검사가 아니라
소스 구간 비교다. 기존 native6·helper·입력·비교 assertions를 새 expected로 고쳐 맞추지 않았다.

| 불변 구간 | bytes | SHA256 |
| --- | ---: | --- |
| native6 전체 및 `buffer` helper·기존 imports | 4283 | `2f47b8880374e9e3c66f3bfe39b0097970d0dadfa0ea211abe0c10240c6c14ea` |
| `bytes`·`native_trace`·`op` helper 전체 | 2900 | `67b986dfd42000245e15381b5207d386898a1e8e6ab1c5946335bdf9370e5610` |
| descriptor32/recipe147 기존 입력 생성부 | 3852 | `7ac1a477c6334da803ccc1524aa037ca49b319ae0b68c62fa34f48fd6ad5fd5d` |
| opened CPU 및32/147 native 결과 비교부 전체 | 1100 | `72349bcd97d45be8286dba6ff3f7278459131344b977ea51db2834cf95dc5795` |

새 원문은 인증 archive와 7파일 각각 bytes 전체를 비교했다. rustfmt1.94.0의 프로젝트
edition2021 구문·서식 확인과 diff 검토만 수행했다. 로컬 Cargo/test/Clippy, native helper,
원본 Python, snippet, 모델/컴파일러 실행은0이다.

### 부모 WF 인계

WF/source pin/ledger/stage/commit/push/GHA는 부모 범위로 남겼다. 현
`.github/workflows/python-rust-523-native-units.yml` buffers branch에는 아직 과거
`6 passed; 1 ignored` 조건과 `original_python_buffer_tables_match_native --ignored` 호출이
남아 있으므로 새 test와 함께 통합하기 전에 변경해야 한다. live selector를 그대로 두면
0 test 실행이 되며 기존1-pass grep이 거절한다. 임시 우회·skip으로 green을 만들면 안 된다.

아래는 인계용 argv이며 로컬에서 실행하지 않았다. 첫 ordinary 호출이 frozen1까지 포함하므로
같은 frozen selector를 별도 성공 반복으로 실행할 필요는 없다.

```bash
run_once compiler-buffer-units "$sdk/bin/cargo" test --manifest-path rust/Cargo.toml --locked \
  --no-default-features --features native-skip-miri -p openpilot-model-runtime --test compiler_buffers
grep -F 'test result: ok. 7 passed; 0 failed; 0 ignored;' "$CI_JOB_TEMP/proof/compiler-buffer-units.log"

run_once compiler-buffer-clippy "$sdk/bin/cargo" clippy --manifest-path rust/Cargo.toml --locked \
  --no-default-features --features native-skip-miri -p openpilot-model-runtime \
  --lib --test compiler_buffers -- -D warnings
```

이 branch의 `buffer_python` 검색·SHA/version admission·`BUFFER_REFERENCE_PYTHON`·
`original-buffers` 별도 호출 및 Python proof 재검사는 더 이상 reader에 필요하지 않다.
부모가 해당 오래된 branch wiring을 제거하고 새 count/actual source pin과 execution edge를
기록해야 한다. 기존 job 소유권·자원 admission·현재 source/index·SDK 전후 봉인은 유지한다.
exact frozen test 이름은 `frozen_original_buffer_tables_match_native`다.

### 정확 변경 파일 및 남은 consumer

이 후속 구현에서 쓴 파일은 test1, 아래 fixture8, 본 문서1뿐이다.

```text
rust/crates/model-runtime/tests/compiler_buffers.rs
rust/crates/model-runtime/tests/fixtures/compiler-buffer-original/README.md
rust/crates/model-runtime/tests/fixtures/compiler-buffer-original/observation.json
rust/crates/model-runtime/tests/fixtures/compiler-buffer-original/oracle.txt
rust/crates/model-runtime/tests/fixtures/compiler-buffer-original/python.sha256
rust/crates/model-runtime/tests/fixtures/compiler-buffer-original/python.version
rust/crates/model-runtime/tests/fixtures/compiler-buffer-original/request.json
rust/crates/model-runtime/tests/fixtures/compiler-buffer-original/stderr.bin
rust/crates/model-runtime/tests/fixtures/compiler-buffer-original/stdout.bin
docs/naver/python-rust-523-buffer-corpus-100-audit.md
```

production `rust/tools/model_export/buffers.py`·`offline_buffers.py`는 삭제·수정하지 않았다.
source상 남아 있는 직접 import/consumer는 아래와 같다. actual caller 전환과 메타데이터
계산 수용은 별도 소유 작업이며 이번 frozen test 전환을 그 완료 증거로 쓰지 않는다.

| consumer | 남은 연결 |
| --- | --- |
| `rust/tools/model_export/capture.py` | `BufferTable`, `host_buffer` 및 CPU capture 경계 |
| `rust/tools/model_export/amd.py` | `BufferTable` |
| `rust/tools/model_export/qcom.py` | `BufferTable` |
| `rust/tools/model_export/offline_schedule.py` | `OfflineBufferTable`, `descriptor` |
| `rust/tools/model_export/offline_constants.py` | `descriptor` |
| `rust/tools/model_export/offline_buffers.py` | 기존 `BufferTable` 상속 |

현재 reader는 더 이상 이 import들을 실행하지 않는다. 전체 production frontend 제거,
Hypatia stage collector/actual-caller 전환, CI pin/edge 통합과 새7-test actual GHA 수용이 남는다.


## workspace feature 합성 검토

원문 request.json의 SHA/bytes는 그대로 고정한다. 현재 생성 입력과의 동등성은 JSON 구조로
검사하여 `serde_json/preserve_order` 합성이 object key 직렬화 순서만 바꿀 때 실패하지 않게 한다.
array의 순서·숫자/boolean 종류·각 값은 그대로 비교하고 원문 stdout/179경계는 변경하지 않는다.
부모는 default feature의 frozen7/Clippy 뒤 preserve_order feature에서 같은 frozen1과 strict
Clippy를 추가로 확인한다. 전체 source/원문 data·기존 native6/입력 생성/비교 본문은 유지한다.

## 삭제된 live 비교의 native frozen 실제 수용 — source7a928

[run40236805936/job121300012677](https://github.com/bin9208/openpilot-rust/actions/runs/40236805936/job/121300012677),
source `7a928cefe22e3c81bcb941f6ed20eaa80eeabc59`의 buffers job은 success다.
기존 native6+원문 frozen1의 일반7개/ignored0과 `serde_json/preserve_order` union의 정확 frozen1,
두 구성의 strict Clippy가 모두exit0이다. 원본179개 generator/assertion과 raw7개의 bytes는
source100 관측을 유지하며 object key 순서만 구조적으로 비교한다. 새로운 원본 실행은0이다.

artifact12614893457은1581957B/SHA256
`089496cef778632e44ad4f2d4136747294b7f71b6b3b3bd099985d85cbead3ea`다.
40 safe unique regular member/전체CRC, expanded6923024B를 inert 확인했다.
SOURCE_COMMIT과 네 exit code0, index19571경로/source tar·stage index 및 prepared SDK294파일/
roster/rejected의 전후6쌍이 같다. 정확 Git tree와 index 경로도 맞는다. 별도 SDK program seal은
원래 after helper의 검증을 통과했다. jobs1/debug0/기존 memory·growth admission과 업로드/소유
정리도 성공했다. model/external numerics provider execution=false, SDK install0이다.

이 결과로 `compiler_buffers.rs`의 기존 live Python179 비교 실행경계 **1개만** replaced-local로
인정한다. 기준3580과 production buffer/offline compiler 파일은 바꾸지 않으며 baseline credit0이다.
추가 장부162개는 replaced-local31/pending127/native-existing1/non-executing-text2/removed1이다.
전체 native-units run의 다른 tools 컴파일 실패와 이 buffers 성공은 각각 분리해 기록한다.
