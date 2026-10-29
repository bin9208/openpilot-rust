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
  명시된 캐시 override, Cargo/Rustup 설치 경로, 작업별 target/output은 보존한다.
- `runner_download_cache.py restore|save`는 고정 native 입력을 이름·정확 key로
  저장하고 작업별 폴더로 복원한다. GitHub cache 업로드·다운로드를 하지 않는다.
  다른 consumer가 캐시를 수정하지 않도록 hardlink 대신 독립 copy/reflink를 쓴다.
  동시 저장은 잠금·원자 rename으로 보호하고 저장 copy 예상 증가량이 부족하면
  캐시 저장만 생략한다. 기존 입력 도구의 SHA/길이/형식 검증은 그대로 필요하다.
- 전체 Actions 플러그인 설치를 새로 만들지 않는다. 기존 runner `_actions`와 tool
  cache는 runner가 관리하며 패키지 캐시와 함께 유지된다. 빌드 산출물 전체의
  재사용은 이번 변경 범위가 아니다.

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
