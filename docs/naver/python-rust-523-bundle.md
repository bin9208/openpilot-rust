# #523 Carrot Routes bundle caller 전환

2026-10-24. `a134b32c5` 기준의 독립 caller 변경 후보다.
관련 작업은 [#523](https://github.com/bin9208/openpilot-rust/issues/523)이며,
전체 Python 제거와 수용은 [#296](https://github.com/bin9208/openpilot-rust/issues/296)에서 계속 추적한다.

## 변경 범위

`rust/tools/run-developer-tool`의 정적 allowlist를 다음 feature map으로 확장했다.

| 명령 | Cargo feature |
| --- | --- |
| `developer-auth`, `developer-ssh`, `setup-ssh-keys` | `developer-ssh-native` |
| `build-route-vault-bundle` | `route-vault-bundle-native` |

Cargo에 `--features "$features"`를 전달한다. 기존 명령의 feature, Rust 1.94.0 준비,
작업별 공간 admission, 명시적 native host/target, 빌드 환경 정리, stderr 출력,
지원하지 않는 명령의 오류와 `exec "$binary" "$@"` 전달은 그대로 유지한다.
이 변경은 bundle 생산 코드나 source 선택·radar detection/replay 로직을 수정하지 않는다.

README와 한/영 사용자 문서에는 저장소 루트에서 실행하는 명령을 안내한다.

```sh
rust/tools/run-developer-tool build-route-vault-bundle /tmp/route-vault-bundle --ref HEAD
```

출력 디렉터리는 존재하지 않는 새 경로여야 한다. helper는 현재 체크아웃의 CLI를
빌드하며 CLI는 지정 ref의 커밋된 replay Python 소스, 전체 Cereal schema, DBC와
고정 viewer/server 파일을 읽고 `SOURCE_COMMIT`을 기록한다. 미커밋 소스와 모델은 포함하지 않는다.
생성 context의 Python 파일은 기존 서비스가 쓰는 출력 데이터이며 원본 제거 증거가 아니다.

`.github/workflows/carrot-route-vault-publish.yaml`은 다음 호출 한 줄만 교체했다.

```sh
rust/tools/run-developer-tool build-route-vault-bundle "$RUNNER_TEMP/vault-bundle" --ref "$GITHUB_SHA"
```

기존 push/dispatch trigger, `bin9208/openpilot`/`carrot-wip` job 조건, Docker build/smoke,
publish/tag 조건과 NAS 경로는 수정하지 않는다. 이 독립 저장소의 비활성 workflow를
활성화하거나 image/NAS 배포를 실행하지 않았다.

## 기존 실제 검증과 이번 변경의 구분

부모가 수용한 [40087242278/job120852565401](https://github.com/bin9208/openpilot-rust/actions/runs/40087242278/job/120852565401)은
`0e28b8601`의 실제 원본/native CLI committed context 비교와 strict Clippy에 성공했다.
각 context는 411개 파일, 8,621,062 bytes이며 source/index 전후도 일치했다.
기존 library 1개·integration 17개는 변경 없는 검토 cohort의 앞선 성공을 재사용했다.
이를 이번 bootstrap 경로에서 18개를 새로 실행한 결과로 기록하지 않는다.

해당 artifact는 `12558055881`, 6,331,785 bytes이며 SHA256은
`2913e7a3e3b7abbdd0696b99fcb6606dcd50ec6e19a0e6c94db0ff3ed746b3d2`다.
부모의 853 ZIP CRC 검증과 raw 405개 original/native file pair의 정확 대조를 수용한다.
당시 uploader 기본값 때문에 hidden vendor 6개 파일, 69,990 bytes는 artifact에서 빠졌다.
따라서 raw artifact가 411개 파일 전부를 보존했다고 주장하지 않는다.
부모는 다음 focused workflow에서 hidden file 보존과 새 helper bootstrap을 검증할 예정이다.

이번 작성 범위는 helper, README, 한/영 사용자 문서, 이 기록과 legacy publish의 호출
한 줄뿐이다. focused bundle workflow, Cargo 등록, main record는 부모가 관리한다.
원본 `tools/carrot_route_vault/build_bundle.py`는 보존하며 Python 제거·inventory credit을 주지 않는다.

## 검증 상태와 인계

- 로컬 `bash -n rust/tools/run-developer-tool`, YAML syntax와 diff/공백 확인은 통과했다.
  helper는 정적 feature map/선택 이외의 bytes가 같고 publish workflow는 허용된 호출
  한 줄만 다른 것을 소스로 확인했다. trigger와 job 조건도 같다.
- 새 helper의 실제 host build/실행, 기존 auth/SSH feature map의 기능 확인,
  helper를 통한 실제 원본/native context 비교 및 hidden output artifact 수용은 **UNRUN**이다.
- 기능 검증은 부모가 GitHub Actions에서 수행한다. capture harness와 fixture binding이 바뀌어 integration17개는 fresh capture가 필요하다. 과거 성공을 새 helper 결과로 간주하지 않는다.
- image publish, Docker/NAS 실행, 장치·계정·SSH 작업, commit/push는 수행하지 않았다.

source/detection 로직이 바뀌지 않은 caller 전환이므로 이번 작업에는 NAS 배포를 포함하지 않는다.


## 공개 helper 검증의 공간·trace 보완

공개 helper는 기존 test target/debug와 다른 target/host/debug를 빌드한다.
후속 caller 단계는 실제 device별로 기존3GiB host build 추정과128MiB context를 합산해
승인하며 helper의3072MiB guard도 유지한다. 앞선1+17 성공만으로 새 host build 공간이
승인됐다고 간주하지 않는다.

raw `strace -xx`의 모든 execve와 PID별 unfinished/resumed·EOF를 엄격히 판독한다.
첫 고정 Bash helper 성공, Git 성공, final ELF 성공1회 및 정확한 argv 전달을 요구한다.
실패한 PATH 탐색을 포함해 Python 이름/확장자를 거부하며, 성공 실행 파일은 알려진
SHA 고정 Bash helper 외에는 regular ELF magic/hash를 확인한다. 이름을 바꾼 interpreter나
ELF 내부까지 포함한 무제한 Python 부재 증명은 아니며 해당 receipt도 false로 기록한다.
negative16개와 positive2개의 순수 control은 별도 범위로 기록한다.
이 controls와 실제 공개 helper 실행은 아직 **UNRUN**이며 Actions에서 확인한다.


## 121aa5 실제 계약·원본 출력 결과와 trace 미수용

[40093557875/job120871017872](https://github.com/bin9208/openpilot-rust/actions/runs/40093557875/job/120871017872),
source `121aa5b86ddf427697a8cabb25d2ed337d80f429`에서 fresh library1/integration17 및 strict Clippy가 성공했다.
실제 원본23회(1+12+7+3)의 JSON, copied script SHA 전후 일치를 확보했다.

artifact12560845618(4250230B)의 SHA256은
`b6ecc542f61d83ee1bfa518d3926ccfb45e6d2f52240eb7f85fd42c8cdc3f50a`다.
부모가889CRC, source/index/venv/script 전후 봉인을 확인했다.
raw original/native **411개 파일·8621062B·숨김vendor6개**를 모두 byte단위로 대조했다.
이번에는 hidden 원문도 archive에 포함돼 지난405개 raw 검토의 한계를 해소했다.

공개 helper 실행 후 전체 context/argv 감사 중 trace metadata parser가 실제
`SIGUSR1 {si_signo=SIGUSR1, si_code=SI_TKILL, si_pid=..., si_uid=1000}`를 거부했다.
따라서 job 전체는 FAIL이며 actual helper trace 전체 수용 또는 원본 삭제로 세지 않는다.
이 관측된 형식만 정확히 분류하고 기존 실패 exec/PID pairing/EOF/ELF/argv 검증과 unknown 거부를
유지하는 수정 후 actual caller를 재실행한다. 성공한1+17 및23원문은 source/provider pin으로 재사용한다.


## d2c18c37e 공개 caller 실제 수용

[40097057916/job120881407562](https://github.com/bin9208/openpilot-rust/actions/runs/40097057916/job/120881407562)는
source `d2c18c37eb502f3d8e5a6d0314c709804df92f38`에서 성공했다. 121aa5의 library1/기존17계약과
original23원문을 source/Cargo overlay로 재사용하고, strict Clippy와 실제 public helper를 새로 확인했다.
전체411파일/8,621,062B가 원본과 byte 동일하며 hidden6개도 포함한다. 실제 native ELF SHA는
`15e216fe54cfc6d8a9ba76a4f471a2b5453de053cd92920f0b12ef16fedb6a9b`다.

artifact12561452899(8,497,394B, SHA256
`8903a728e2fe37bd807dabdb471deb8273893b3b8b4573d9e626b8322ad6fd2d`)의873개 CRC 및
source/index/venv files·links 전후4쌍을 부모가 감사했다. 완전 판독한 trace는1193exec시도/232실패,
412Git성공/최종ELF1성공, 실제 SIGUSR1/SI_TKILL metadata154건이다. Python명 exec시도는0이다.
알 수 없는 signal·변조 metadata/미완료·실패 Python시도 등 기존 및 추가 negative도 거부했다.
이 한정 trace를 이름 변경 interpreter/ELF내부 동작까지 포함한 전체 Python 부재로 확대하지 않는다.

원본 `build_bundle.py`는 아직 남아 있다. 다음은 기존17본문/assertion과 raw23을 그대로 고정한
원본 없는 replay·재출현 거부 및 삭제 후 실제 caller 연결이다. 현재 inventory credit은0이며
Docker/NAS workflow 활성화·이미지 발행·배포는 수행하지 않았다.

### frozen 원본 부재 후보

실제121aa5 캡처23개184,435B를 원문 그대로 `tests/fixtures/route-vault-bundle-original/`에
고정했다. manifest SHA는 `76240cd14a66a55b7ef91459b49919cdbd582d19161437221a6f65f7d22a362b`다.
기존17개 test 본문/assertion은 부모도 byte 동일성을 확인했다. 새 helper는 ROOT/DEST/CWD의
검토된 byte span만 바꾸며 argv/ref/두 GitHEAD/destination 전후 kind·mode·rawdata를 대조한다.
원본 source를 복사/실행하거나 expected를 새 candidate 출력으로 만들지 않는다.

실제 source/data/manifest와 컴파일된 test2 source, 현재 test ELF 및 compile-bound integration
CLI ELF를 NOFOLLOW 경로/FD·identity·전체SHA로 정상 assertion 뒤까지 보존한다. 원본의 file/
dir/broken symlink 재출현, 잘못된 span·변조·탈출 및 held파일 교체를 새 negative1에서 거부한다.
helper는 `tests/route_vault_bundle/frozen.rs`에 두어 독립 integration target 자동 발견을 피했다.

현재 정확한 원본 `tools/carrot_route_vault/build_bundle.py` 하나를 삭제한 후보이며 credit은0이다.
다음 CI는 native18/고유raw23/Clippy와 삭제 후 실제 public helper를 실행한다. d2의 원본411개 raw
출력 및 전체 directory roster를 읽어13source roots/고정7개 Gitblob 불변을 먼저 요구하고,
410파일은 전체 byte/hash를, `SOURCE_COMMIT`1파일은 새 정확 HEAD를 별도로 검증한다.
과거 원문은 고치지 않는다. 기존 strict exec parser/negative controls 및 growth·source/provider
전후 봉인은 보존한다. 진단 metadata의 Python 의존성은 execution-edge pending으로 계속 남는다.

새18기능/Clippy/실제삭제caller는 미실행이다. 로컬은 edition2021 rustfmt/YAML·Bash·embedded
Python 문법/diff 및 원문23·기존17 동일성만 확인했다. Docker/NAS 발행·활성화는 수행하지 않았다.
