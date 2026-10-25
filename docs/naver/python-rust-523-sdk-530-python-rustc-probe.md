# #530 Carrot33923 Python 설치 중 Rustup prefix 생성 경로 — 2026-10-26

source `33923f8fc0b954b4797480e64b984200ac45a138`의 [run40216388430/job121238317986](https://github.com/bin9208/openpilot-rust/actions/runs/40216388430/job/121238317986)을 읽기 전용으로 조사했다. active WF/main/SDK helper와 다른 작업자의 코드는 수정하지 않았다. 부모의 SDK 재배치·namespace 격리 검토에 제공할 코드 근거이며 로컬 설치·버전 명령·기능 실행·CI 재실행은0이다.

## 실제 로그와 시간 경계

private `ci/carrot33923full.log`는169531B/SHA256 `75d1e336c25479e2894a08ccf8fb1d3402168d2d1b142291cf0c7c1d8804dc26`다. step11 setup-python은 success, step13 fresh SDK는 failure, step14 legacy shim 노출은 skipped다. 따라서 이번 실제 prefix를 step14의 restore가 만들었다는 설명은 성립하지 않는다.

| UTC | 실제 기록 |
| --- | --- |
| 17:25:42.7367541 | setup-python이 archive installation script 실행 |
| 17:25:43.2476591 | `Upgrading pip...` |
| 17:25:44.951294751 | 이후 stat에서 확인한 rustup-home directory의 mtime/ctime |
| 17:25:45.5262015 | 기존 pip25.0.1 발견 |
| 17:25:46.4510812 | pip26.2.2 설치 완료 |
| 17:25:50.0942602 | `/tmp/openpilot-ci.mCwYf4/rustup-home`, uid/gid1001:1001, mode755 확인 |

fresh-prefix probe는 첫 RUSTUP_HOME 부재 검사(line9)에서 정상적으로 거부했다. 로그의 stat 형식은 `%y/%z`이므로 위 시각은 mtime/ctime이며 mkdir syscall 또는 birth time의 직접 관측으로 쓰지 않는다. 이 실행의 CARGO_HOME 생성 여부나 전체 child roster는 이 probe만으로 확정하지 않는다.

## 확인한 공식 원문의 생성 코드 경로

1. 실제 action SHA는 `actions/setup-python@ece7cb06caefa5fff74198d8649806c4678c61a1`이다. [install-python.ts183–204행](https://github.com/actions/setup-python/blob/ece7cb06caefa5fff74198d8649806c4678c61a1/src/install-python.ts#L183-L204)은 installer child의 env에 `process.env`를 그대로 넣고 Linux LD_LIBRARY_PATH만 추가한 뒤 `bash ./setup.sh`를 실행한다. job의 RUSTUP_HOME/CARGO_HOME도 전달된다.
2. 로그의 정확한 [Python archive](https://github.com/actions/python-versions/releases/download/3.12.14-31661455385/python-3.12.14-linux-24.04-x64.tar.gz)를 inert 다운로드해95088884B/SHA256 `5a03168292516f6dd6dcf630f4bf5369b108abd7c699a7e1db37f1e622257fff`가 release asset512394035의 digest와 같음을 확인했다. archive의 setup.sh2025B/SHA256 `a54c9c20f9329e020652a5f4c5ac30892d4eb33e1fbed6211291e46013af9bce`는52–57행에서 기존 pip를 사용해 pip upgrade를 실행한다. profile source나 Rust 설치 명령은 없으며 archive의 Python 바이너리/스크립트를 로컬에서 실행하지 않았다.
3. 같은 archive의 pip25.0.1 METADATA와 실제 `pip/_internal/network/session.py`18771B/SHA256 `9ac3387acd7a2e698d118364ad8ca0f1f4dcee00076ca165b5ac1f28fdbb2ce2`를 읽었다. 이는 [공식 pip25.0.1 source](https://github.com/pypa/pip/blob/25.0.1/src/pip/_internal/network/session.py#L181-L194)와 byte 동일하다. PipSession345행은 HTTP User-Agent 생성 함수를 호출하고,181–194행은 PATH에 rustc가 있으면 `rustc --version`을 timeout0.5초로 실행한다. stdout/stderr를 잡고 모든 예외를 무시하므로 실패한 Rust probe가 pip 로그에 보이지 않아도 prefix 부작용은 남을 수 있다. setup.sh에 이미 있는 `--disable-pip-version-check`는 이 User-Agent probe를 제거하지 않는다.
4. 해당 [runner 이미지 명세](https://github.com/actions/runner-images/blob/ubuntu24/20261012.334/images/ubuntu/Ubuntu2404-Readme.md#L138-L142)는 Rustup1.29.1을 포함한다. [동일 이미지 install-rust.sh](https://github.com/actions/runner-images/blob/ubuntu24/20261012.334/images/ubuntu/scripts/build/install-rust.sh)는 Rustup shim을 provision하고 HOME의 `.cargo/bin`을 환경 PATH에 더한다. private HOME/RUSTUP_HOME 변경은 기존 PATH의 Rust shim을 자동 제거하지 않는다.
5. Rustup1.29.1의 [proxy_mode.rs35행](https://github.com/rust-lang/rustup/blob/1.29.1/src/cli/proxy_mode.rs#L35)은 rustc proxy의 toolchain 선택 전에 Cfg를 연다. [config.rs319–324행](https://github.com/rust-lang/rustup/blob/1.29.1/src/config.rs#L319-L324)은 `process.rustup_home()`를 구하고 곧바로 `ensure_dir_exists`를 호출한다. [utils/raw.rs17–26행](https://github.com/rust-lang/rustup/blob/1.29.1/src/utils/raw.rs#L17-L26)은 디렉터리가 없으면 `fs::create_dir_all`을 실행한다. toolchain 실행 성공이 필요하지 않다. Cargo.lock의 home0.5.12 crate checksum `cc627f471c528ff0c4a49e1d5e60450c8f6461dd6d10ba9dcd3a61d3dff7728d`도 대조했다. 그 env.rs100–112행은 비어 있지 않은 절대 RUSTUP_HOME을 HOME보다 우선해 그대로 반환한다.

따라서 **setup.sh의 pip upgrade → pip User-Agent의 rustc probe → PATH의 Rustup shim → Cfg 초기화의 RUSTUP_HOME mkdir**라는 구체적인 생성 코드 경로를 확인했다. 실제 시각·상속 env·기존 pip 버전·runner Rustup 버전과 부합한다. 공식 setup.sh가 직접 Rust를 설치하거나 login profile이 반드시 생성했다는 설명은 필요하지 않다. Bash의 BASH_ENV는 별도 경로이나, 확인한 프로젝트 shell-env는 temp/source 환경만 조정하고 Rust probe를 실행하지 않는다.

다만 해당 실행의 `shutil.which` 결과인 rustc 절대 경로/파일 hash와 PID별 exec/mkdir trace는 없다. 그러므로 이 코드 경로의 존재와 로그 정합성은 확인했지만 생성 주체를 syscall로 직접 귀속한 결과와는 구분한다. profile 전체나 보이지 않은 프로세스가 전혀 실행되지 않았다고 확대하지 않는다.

## 부모 수정안에 대한 소스 검토

- SDK step을 앞당기는 것만으로 모든 후속 도구가 prefix에 쓰지 않는다고 가정하지 않는다. PATH가 shim을 선택하면 Python installer 안의 probe도 준비 SDK namespace에 접근할 수 있다. 실제 전체 SDK 사후 봉인은 계속 필요하다.
- setup-python step에 준비 SDK와 다른 **job-private RUSTUP_HOME/CARGO_HOME**을 주는 안은 위 env 전달 코드와 일치한다. setup.sh가 이 변수를 다시 설정하지 않고 pip의 rustc subprocess도 기본 env를 상속하므로, shim이 선택되더라도 Rustup home 선택을 별도 namespace로 보낼 수 있다. 기존 auto-install 금지 제약을 유지하고, 이 step의 override를 job 전체 SDK env에 영구 반영하지 않아야 한다. 허용된 작업별 성장/소유·정리 정책 안에서 부모가 구현·검증한다. 실제 수정과 Actions 수용은 이번 조사 범위 밖이다.
- legacy rust-path는 SDK/bin에 rustup이 없다는 사실만으로 restore를 실행하지 않는다. source33923의 locate는 rustup을 PATH에서 못 찾으면 private CARGO_HOME/bin으로 fallback하고 rustup/cargo 두 executable을 검사한다. fresh SDK가 private client 둘을 준비했다는 조건에서 ready=true이므로 Restore 조건은 false다. 이는 정적 분기 검토이며, 이번 run에서 skipped였던 step을 실제 성공한 것으로 쓰지 않는다.

fresh guard·whole SDK 사후 봉인은 그대로 유지한다. 기존 partial prefix adopt, 삭제 후 재시도 우회, 환경 provenance 위장이나 검증 완화는 제안하지 않는다. 원문은 private `ci/sdk530-python33923/`에 실행하지 않은 자료로 보존했고 새 tracked 기록은 이 문서뿐이다.

## 부모 수정 후보

CarrotMan의 `setup-python`과 기존 venv 준비 두 단계에만 별도 `python-rustup-home`/`python-cargo-home` 및 `RUSTUP_AUTO_INSTALL=0`을 전달한다. 이 override는 step 단위이며 job의 실제 SDK 홈은 바꾸지 않는다. Rustup1.29.1 원문 `Cfg::should_auto_install`은 값0일 때 자동 설치를 거부한다. 작은 probe metadata는 기존 Python 준비512MiB 및 소유 job 회수 안에 포함되며 새로운 SDK 다운로드 예산을 추가하지 않는다. 이미 관측을 얻은 printf/stat 두 줄은 제거하되 원래 SDK 세 prefix 부재 검사·공식 init/proxy 해시·whole SDK 전후 봉인은 유지한다.

shared native policy는 두 Python 단계와 정확한 격리 환경을 요구한다. 기존13개 SDK escape에 setup의 SDK 홈 침범/auto-install 허용과 venv의 Cargo 홈 침범/auto-install 허용4개를 더해17개로 검증한다. 기존 ARM 원문 회귀의 x64 reference에도 정확한 step 환경을 반영한다. 수정한 main의 Carrot job과 분리 SDK workflow는 main SHA pin 단계 하나를 제외하면 전체 객체가 동일하다. 실제 fresh SDK·전체 Carrot 기능/전후 봉인의 새 GHA 수용 전에는 #530을 닫지 않는다.

## 격리 적용 후 actual SDK와 공식 proxy 형태

sourcef1c730b02의 [40217812565](https://github.com/bin9208/openpilot-rust/actions/runs/40217812565)는 strict fresh 검사를 통과하고 공식1.94.0 SDK를 설치·봉인했다. 이후 SDK after와 source/index after도 성공했으나 private proxy의 regular/nonsymlink 조건(line24)에서 멈춰 Carrot 기능에는 도달하지 않았다. 따라서 이전 prefix 실패와 같은 원인으로 합치지 않는다.

Rustup1.29.1의 [self_update.rs787–840행](https://github.com/rust-lang/rustup/blob/1.29.1/src/cli/self_update.rs#L787-L840)은 `RUSTUP_HARDLINK_PROXIES`가 없으면 proxy를 symlink 우선으로 만들고, 있으면 hardlink를 사용한다. 기존 caller가 공식 기본 symlink 형태를 전부 거부하는 계약과 맞지 않았다. 새 후보는 private SDK 준비 호출에만 공식 `RUSTUP_HARDLINK_PROXIES=1`을 전달한다. 기존 regular/nonsymlink·소유 UID·공식 init과 전체 byte 동일성 비교를 유지하며, 생성 후 proxy를 임의 복사·재작성하거나 기존 부분 prefix를 채택하지 않는다. SDK escape는 이 설정 제거1개를 더해18개다. 새 actual proxy/기능 수용은 GHA 대기다.
