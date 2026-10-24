# Carrot Navi 비교기의 역사 자료

`retained-runtime-job.yml`은 실행하지 않는 source 정책 자료다. workflow로
등록하거나 그 안의 shell/Python 명령을 실행하지 않는다.

- 원문 commit: `e8744f656444f1e8a5c49527ed15fb56e4406673`
- 원문 경로: `.github/workflows/rust.yml`
- 원문 Git blob: `c8bc67ce0dc1b1e809d584c2e41222651c1e2798`
- 추출: `carrot-navi-runtime` 원문에서 comparator build/caller 두 step만 제외.
  들여쓰기와 끝 newline을 보존한 7,333B다.
- 기존 `RETAINED_JOB_SHA`:
  `fe4b08ff2bab5d8d199c827c6ed13ac392576ed710544a0adef1e6f6160e34cf`

`BUILD_SHA`와 `CALLER_SHA`도 변경하지 않았다. 현재 parsed run의 641B와
53B를 각각 기존 pin으로 확인한다. 나머지 도메인 run과 action, job 설정은
고정 역사 model과 전체 비교하며, YAML의 block/flow/alias 표현만 구분한다.

현재 private 배치는 `a4b1db51cdf0d10a55f647c184dc2b639a726657`의 검토된
11개 추가 단계만 허용한다. 기존 `validate_local_routing`으로 source/fork/
private layout을 먼저 검증한 뒤 각 단계의 위치·이름과 **전체 metadata/run**을
canonical JSON SHA256
`80845db9512f58300af32f597109177631b208cad7384c4628c647ae69668534`
로 확인한다. canonical 형식은 parser의 문자열 scalar를 유지하고 object key만
재귀 정렬한 UTF-8 compact JSON이다. array 순서와 문자열 byte는 바꾸지 않는다.
이 pin은 infrastructure 소스의 경계이며 새 runtime expected/output이 아니다.

추가로 허용하는 차이는 정확한 private checkout/env/action/upload 경로와 trust
조건, 그리고 `a37416418ed1b166f112aa33e9008f0d883d87af`의 dependency 준비
변경 두 가지다. 광역 cache 삭제 한 줄 제거 및 apt 두 호출의 architecture 옵션
추가만 허용하며 package/version, growth guard와 이후 모든 명령은 보존한다.
검증하지 않은 단계를 이름만으로 제외하거나 현재 source로 역사 pin을 갱신하지 않는다.

`original-embedded.py`와 `saved-members.sha256`는 기존 oracle/관측 member
자료다. 이 변경은 두 파일이나 72 controls·42 caller 기대값을 갱신하지 않는다.
기존 성공 기록은 run `39826752939`, `39832413264` 및 해당 #296 evidence/
handoff 문서에 있으며 현재 source의 실행 성공으로 소급하지 않는다.

로컬 검증은 source·fixture SHA·rustfmt·diff에 한정했다. 새 policy 함수의
양성/음성 실행, compile/test/Clippy, 원본 traffic 실행은 모두 **UNRUN**이다.
후속 Actions의 source 정책 selector는 다음 두 개다.

- `maintained_policy_rejects_embedded_oracle_and_changed_traffic`
- `maintained_policy_accepts_yaml_aliases_and_rejects_unreviewed_overlays`

각각 다음 명령의 `<selector>`에 위 이름을 넣어 확인한다.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --features carrot-navi-compare-native --test carrot_navi_compare <selector> -- --exact
```

두 selector는 source 정책만 검사한다. held capture/oracle을 실행하는 별도의
기존 caller 검사는 선택하지 않으며 그 실행 조건·입력·본문은 그대로 유지한다.
