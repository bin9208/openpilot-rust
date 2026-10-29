# 공통 gate 회귀의 env 기준 선택 — #635

기준 소스는 `0ff1d176324a9c196df51df4e9d6f9c6e7037b9c`이며 기존 perf 작업
`9d78d07523f60a32d8a723daf4677980673382fe`는 별도 브랜치에 그대로 보존한다.
부모 main checkout은 수정하지 않는다.

실제 old606917 planner job123497008119에서 `test_plannerd_ci.py`가 fast의
첫 step에 env가 있다고 가정해 KeyError로 실패했다. runner 준비 step 추가로
순서가 바뀌었을 뿐 실제 gate나 수용 기준을 바꿀 문제는 아니다.

main `924710ae8ac25e52dace839cc9ba89f2fd0fc4e6`의 원문 두 줄을 그대로 사용해
planner는 PLANNER_MEMORY, Xiaoge는 XIAOGE_MEMORY env가 있는 step을 선택한다.
Card도 CARD env가 있는 step을 같은 단일 원소 unpack 방식으로 선택한다.
선택 대상이 없거나 둘 이상이면 실패하므로 gate 검증이 조용히 생략되지 않는다.

기존 needs·matrix assert, success/failure/cancelled/skipped/빈 값의 실행 검사,
Card의 `test "$CARD" = success` assert와 모든 fixture·workflow 실행부는 유지한다.
workflow·프로덕션 guard·환경·pin·threshold는 변경하지 않는다.

로컬은 세 Python AST와 정확한 diff만 확인했다. planner/Xiaoge selector가 main924
원문과 같은지, 이 선택자 변경을 되돌리면 세 파일이 기준 소스와 byte 단위로 같은지
대조했다. 기능·빌드·pytest·workflow 실행과 원격 조작은 하지 않았다. 새 정확 SHA의
실제 회귀 통과 여부는 부모 Actions 검증이 남아 있다.

## 2026-10-30 일반 X64 검사 18개를 hosted로 분산

사용자가 세 self-hosted runner와 GitHub-hosted Actions의 병렬 사용을 승인했다.
`bin-actions2`와 `bin-actions3`는 같은 PC이므로 일반 X64 검사를 추가로 hosted에
배치한다. 기준은 `f7333e84320a800bcf62437600ebb28aeef936a5`다.

`.github/workflows/rust.yml`에서 다음 18개 job의 `runs-on`만 `ubuntu-24.04`로 바꾼다.

- `encoder-runtime`, `camera-runtime`, `panda-runtime`, `selfdrive-runtime`, `radar-runtime`, `navd-runtime`
- `carrot-navi-runtime`, `startup-runtime`, `startup-services`, `sensor-audio`, `gnss-runtime`, `controls-runtime`
- `estimation-runtime`, `ui-connectivity`, `platform-runtime`, `hardware-runtime`, `logger-runtime`, `athena-runtime`

기존 hosted job과 native ARM, 무거운 `ui-runtime` 및 다른 workflow는 유지한다.
원래 argv·needs·pin·조건·env·gate를 바꾸지 않는다. 기존 routing 회귀의
`HOSTED_JOBS['rust.yml']`에도 동일 18개만 추가하고 frozen command hash·baseline·
skip·needs 단언은 유지한다. 공통 `local-arm`의 hosted 허용 경로를 그대로 사용한다.

로컬은 YAML·Python AST·diff만 확인했다. 전체 workflow 자료를 대조해 지정된 18개
`runs-on` 외의 값이 같은지 확인하고, Python AST도 허용목록 외에는 같은지 대조한다.
기능·빌드·테스트·push·dispatch·취소는 수행하지 않는다. 새 정확 SHA의 실제 실행
결과는 부모 Actions 검증에 남긴다.

## 2026-10-30 dev #631 정상 병합과 workspace 명령 대조

PR #606의 기준 dev가 `3fd7fd051b0af237fb8f75a4260e48707e3ae7c8`로 바뀌어
충돌한 상태에서 `b81fd07385b7674e10859898eacbf2d745c71afe`에 해당 커밋을
`git merge --no-ff --no-commit`으로 정상 병합한다. 지원 서비스 9개의 구현·검사·
기록은 dev 원문을 보존하고 dev-md 작업은 가져오지 않는다.

실제 충돌은 `.github/workflows/rust.yml`의 `LOCAL_CI_ARCH` 6곳과
`test_rust_isolation.py`의 webcam 신뢰 소스 조건 1곳이다. 기존 arch·runner 라우팅·
신뢰 소스 조건과 dev의 `CARGO_BUILD_JOBS` 부재 단언을 함께 보존한다.
workspace Clippy와 test의 `--features openpilot-modem/helper-lifetime-tests`,
실제 `openpilot-process-child` 빌드 및 WIFI/MODEM 자식 경로·Python 환경 설정도
dev의 전체 script 그대로 유지한다.

frozen `BASE=e445c9d19c3041bc79d8624589e2eb6ba7d061c6`와 JSON fixture·원래
해시는 변경하지 않는다. `.github/workflows/rust.yml`의 `workspace`에 한해 Git의
e445 원본 두 script와 dev3fd7의 대응 script를 읽어 정확히 일치하는 새 script만
원래 script에 대응시킨다. feature flag를 일반적으로 지우는 정규화는 추가하지 않는다.
별도 단언으로 현재의 두 전체 script가 dev3fd7과 같아야 하며, 기존 hash·skip·needs·
matrix 검사는 유지한다.

로컬에서 변경 Python 5개 AST, YAML, 두 workspace script와 병합된 shell 파일 2개의
Bash 구문 및 diff를 확인했다. 전체 workflow 자료는 기존 #606에 dev의 두 script만
반영한 결과와 같으며, 지원 서비스 관련 44개 파일은 dev와 byte 단위로 같다.
기존 회귀의 다른 함수·단언은 AST가 같고 frozen 검사는 두 adapter 호출을 제외하면
AST가 같다. 기능·빌드·테스트·push·dispatch·취소는 수행하지 않았다. 이 정상 병합의
정확 SHA에 대한 PR/Actions 실행 결과는 부모가 별도로 확인한다.
