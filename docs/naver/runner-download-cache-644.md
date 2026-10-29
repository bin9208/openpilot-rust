# self-hosted 러너 다운로드 캐시 (#644)

2026-10-30 사용자 요청: 세 러너에서 의존성·플러그인과 고정 입력을 작업마다
다시 내려받는 비용을 줄인다. [이슈 #644](https://github.com/bin9208/openpilot-rust/issues/644).
공통 CI [PR #606](https://github.com/bin9208/openpilot-rust/pull/606)의 작업별 격리를
유지하는 범위이며 #599의 고정 입력 캐시는 같은 도구를 재사용할 수 있다.

## 원인과 변경

- 공통 `native-apt-shell`은 매 작업의 `/tmp/openpilot-ci.*` 아래에 빈 APT lists와
  metadata를 만들었다. 이제 원래 공식 source bytes·native architecture로 키를
  정한 지속 디렉터리의 lists·metadata·deb archives를 사용한다. `apt-get update`는
  계속 실행해 저장소 변경과 서명을 검증하며, 변경되지 않은 목록은 HTTP 재검증으로
  재사용한다. 같은 source의 작업은 파일 잠금으로 직렬화하고 dpkg 잠금도 기다린다.
- `job-temp`가 등록된 self-hosted 환경에서 pip·uv·npm·Gradle 다운로드 캐시를
  `~/.cache/openpilot-ci-v1/<host architecture>/`에 연결한다. 같은 계정·호스트의
  러너는 이 경로를 함께 사용한다. 서로 다른 호스트는 각 호스트에 한 번씩 받는다.
  이미 채워진 안전한 표준 cache 디렉터리가 있으면 그 경로를 우선 사용한다.
  명시된 캐시 override, Cargo/Rustup 설치 경로, 작업별 target/output은 보존한다.
- `runner_download_cache.py restore|save`는 고정 native 입력을 이름·정확 key로
  저장하고 작업별 폴더로 복원한다. GitHub cache 업로드·다운로드를 하지 않는다.
  다른 consumer가 캐시를 수정하지 않도록 hardlink 대신 독립 copy/reflink를 쓴다.
  동시 저장은 잠금·원자 rename으로 보호하고 저장 copy 예상 증가량이 부족하면
  캐시 저장만 생략한다. 기존 입력 도구의 SHA/길이/형식 검증은 그대로 필요하다.
- Actions 플러그인은 runner가 매 job 시작 때 `_actions`를 정리하므로 별도 archive
  캐시가 필요하다. `runner_action_cache.py`는 현재 참조를 GitHub API로 확인한 정확
  SHA의 archive를 처음 한 번 받고 digest receipt와 함께 저장한다. 공식 runner의
  `ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE`가 읽는 `<owner>_<repo>/<sha>.tar.gz` 형식이다.
  기존 `.env` 값을 보존하며 캐시 변수를 추가한다. 이 변수는 Listener 시작 때
  읽으므로 systemd 서비스가 있으면 마지막 검증 job의 완료 marker와 Worker 부재를
  확인한 별도 임시 unit이 해당 서비스만 재시작한다. 그 전에는 현재 실행을
  중단하지 않는다. 수동 Listener도 원래 Listener의 환경을 메모리에서 보존한 별도
  대기 프로세스가 동일 조건을 확인한 뒤 `run.sh`로 다시 시작한다. job token이나
  임시 `BASH_ENV`를 새 Listener에 상속하지 않는다. tag가
  갱신되어 SHA가 다르면 runner는 원래 다운로드로 돌아가며 이전 코드를 쓰지 않는다.
  근거: [공식 ActionManager](https://github.com/actions/runner/blob/main/src/Runner.Worker/ActionManager.cs),
  [Listener 환경 로딩](https://github.com/actions/runner/blob/main/src/Runner.Listener/Program.cs).
  활성화 뒤에는 현재 Worker가 실제 해석한 action SHA도 읽어 캐시에 추가하므로
  버전 tag가 이동한 경우에도 처음 한 번 이후의 archive 다운로드를 줄인다.

명시적 cross source/APT_CONFIG와 QEMU guest의 APT는 기존 선택을 유지한다.
hosted runner의 실행도 바꾸지 않는다. cache fallback prefix 대신 정확 key만
사용해 로컬 입력의 버전 혼합을 피한다. 필요 없는 캐시를 작업 종료마다 지우는
정책은 추가하지 않았다.

## 검증 계획과 상태

로컬은 AST/YAML/Bash 구문과 `git diff --check`만 확인한다. 실제 copy/reflink,
동시 writer, source/architecture key 구분, identity 변조·symlink 거부, 명시 override
보존은 별도 Actions 회귀에서 확인한다.

러너별 seed job과 다른 offline job으로 실제 디스크 hit를 확인하고 `six==1.17.0`
wheel을 `--no-index --no-deps`로 새 작업 폴더에 설치한다. 각 job의 실제 runner와
SHA·경로를 artifact에 남긴다. 별도 job의 `apt-get update`에서 실제 `Hit:`도 확인한다.
이는 의존성 재사용 검증이며 ARM/AGNOS 전체 빌드나 실기 검증 결과가 아니다.

진행 중인 다른 source의 검증은 취소하지 않는다. 캐시 검증용 branch push에는
최신 사용자 지시대로 별도의 Fast checks push workflow를 실행하지 않는다.
검사 결과·적용 커밋·남은 통합 사항은 실제 Actions 완료 후 이 문서에 기록한다.

2026-10-30 중간 증거: `162a51f9514ad86ae3a091522b6c7acfe85d10a6`의
[회귀 job](https://github.com/bin9208/openpilot-rust/actions/runs/40947859838/job/123707850153)은
캐시7개·Cargo target 격리2개·workflow/APT 정책6개·원래 명령/route12개, 총27개 검사를
통과했다. 앞선 `a1852f6af`는 원래 frozen trigger 단언이 제거된 Fast checks push를
계속 요구해 실패했으며 해당 한 예외만 사용자 지시에 맞춰 수정했다. source/명령
hash 단언은 완화하지 않았다. 러너별 실사용 검증은 기존 작업 종료 뒤 진행한다.

러너별 준비→별도 job의 offline 검증은 서로 독립된 reusable workflow로 연결한다.
먼저 끝난 러너가 다른 러너의 긴 빌드 종료를 기다리며 비어 있지 않도록 했다.

## bin-actions2 실사용 증거

- `daadfbda17d380dc0c9a96d0aa699009b7b4958d`의
  [offline job](https://github.com/bin9208/openpilot-rust/actions/runs/40949858432/job/123715382339):
  실제 디스크 hit, `six==1.17.0`의 `--no-index --no-deps` 새 폴더 설치 성공,
  cache service 전송0 byte를 확인했다. APT의 원래 공식 archive/security4개 모두
  `Hit:`였으며 전체 목록을 다시 받지 않았다. 한국어 `기존:`을 영어로 잘못 판정한
  앞선 검사는 검증 step에만 `LC_ALL=C`를 설정해 교정했다.
- 작업 종료 후 수동 Listener를 재시작한 뒤 `93d7e3b52c126ccefe650da9d00369f14b26c05a`의
  [다음 seed job](https://github.com/bin9208/openpilot-rust/actions/runs/40950014023/job/123716497072)에서
  `restart_required=false`와 실제 `ActionManager`의 checkout/upload-artifact archive
  hit2건을 확인했다. 원래 버전2.338.0도 이 archive cache를 지원함을 공식 source로
  확인했다. 다른 러너의 실행 중 Worker는 중단하지 않았다.

이슈 #644가 나머지 러너·공통 PR 필수 검사와 routing 라벨 복구 상태의 최신 기록이다.
부분 증거를 세 러너 전체의 완료나 ARM/AGNOS 런타임 검증으로 확대하지 않는다.
