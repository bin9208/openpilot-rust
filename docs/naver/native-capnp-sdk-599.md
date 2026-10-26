# native Cap’n Proto SDK 입력 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
원본 AGNOS의 기본 PATH에서 `capnp --version`이127로 끝난 실제 결과에 대응한다.

## 원본과 입력

기존 CI가 설치하는 Ubuntu Noble의
[source package는1.0.1-4](https://packages.ubuntu.com/source/noble/capnproto)다.
같은 배포판의 ARM C/C++ package를
[공식 Ubuntu Ports](https://ports.ubuntu.com/ubuntu-ports/pool/universe/c/capnproto/)에서
읽어 다음 원본 bytes/hash를 고정했다. runtime release의 현재 source SHA와 분리된 입력이다.

| package | bytes | SHA-256 |
| --- | ---: | --- |
| capnproto1.0.1-4 arm64 | 240820 | `9d693ded6a718a142063ccf0bb3250fa4e15b846980362ec6c38cff2a711a81c` |
| libcapnp-1.0.1 1.0.1-4 arm64 | 1332456 | `09809a4118ac05804b8a28c08ebf6a3805fc8ec8bb67e696b14aa5ac812a22a1` |
| libcapnp-dev1.0.1-4 arm64 | 2096082 | `4859f547de40975ea5e9847448701fa338bb2e5317667ea8bdc90bdb2837844c` |

합계3669358 bytes다. `rust/native-capnp-assets.json`은 기존 asset cache CLI의 plan이며
같은 입력 재준비에서 세 자산의 `transferred_bytes=0`/`stored_bytes=0`을 요구한다.
기존 C/C++ compiler·library·header를 재사용하고 별도 언어 전환을 추가하지 않는다.

## 실제 compiler 경로와 그룹별 검증

`openpilot-cereal`의 기존 기본값은 PATH의 `capnp`다. native SDK를 명시할 때만
`OPENPILOT_CAPNP`의 절대 실행 파일을 사용한다. env/path 변경은 Cargo 재생성 입력이며
원래 schema와 생성 후 normalization 검사는 유지한다. 상대/잘못된 파일 경로는 실패한다.

native build driver는 명시한 compiler의 실제 canonical 파일 hash·실행 권한을 조사하고
그룹의 `native_tools`와 cache identity에 넣어 빌드 후 다시 확인한다. SDK 전체 library/
header tree도 recipe의 pinned `input_trees`에 연결해야 한다. 실행 시 필요한 prefix의
library 경로는 동일 input set의 명시한 환경에 포함한다. executable hash만으로 전체 SDK의
동적 library closure가 검증되었다고 주장하지 않는다.

보조 도구를 쓰는 그룹만 그 입력을 가질 수 있다. prepared 검증은 source/Rust compiler/
Cargo/toolchain/recipe/출력 일치를 계속 요구하면서 그룹별 도구 목록의 차이를 허용한다.
여러 그룹의 **동일 실행 파일 경로에 서로 다른 hash**가 기록되면 거부한다. 최종 Layout의
각 실제 도구가 pinned file/tree에 포함되는 검사와 전체 ABI 검사는 그대로 유지한다.
실제 Cargo 회귀에는 한 그룹만 소유 보조 도구를 사용하는 경우와 상충 hash 거부를 추가했다.
추가 회귀는 소유 tool이 만든 실제 build-script 입력42→43을 확인한다. 같은 경로/크기의
tool을 바꾸고 기존 pin을 유지하면 실패해야 하며, 새 pin을 고정한 뒤에만 새 cache에서
실제43을 빌드한다. 이전 binary는42를 유지해야 한다. 이 소유 script는 genuine capnp의
대역으로 원본 수치/프로토콜 동등성을 주장하는 데 사용하지 않는다.

## Actions의 SDK·원본 wire 검증

`prepare_native_capnp.py`는 bin-actions의 자기 job temp에서만 실행한다. 원본 package
SHA/크기와 `Package`/`Version`/`Architecture=arm64`를 확인하고, `dpkg-deb --extract`로
별도 SDK prefix를 만든다. 시스템 package를 설치하거나 maintainer script를 실행하지
않고 guest APT·공유 sources도 변경하지 않는다. 원래 license/copyright·header·library를
보존하며 실제 ARM ELF를 확인한 뒤 기존 input-tree CLI로 전체 목록을 고정한다.

compiler 입력의 provenance에는 vendor 내용만 넣는다. project commit과 host 해제 도구
hash는 별도 실행 증거에 저장한다. 문서만 바뀐 새 commit이 그 SDK의 tree identity를
바꾸지 않게 한다. 일반 파일은실행0555/data0444로 정리하고 원래 link는 전체 tree
검사를 유지한다. 이 단계의 `compiler_executed=false`는 실제 범위다.

## 프로젝트 준비 명령과 검증 경계

같은 세 package plan을 프로젝트의 `openpilot-runtime-inputs --prepare-capnp`가 읽는다.
소스 helper가 이미 준비된 개발/검증 환경에서 다음처럼 호출한다. 경로는 소유한 소스
밖의 cache/store를 지정하고, 최초 helper bootstrap이나 기본 시작 연결로 오해하지 않는다.

```sh
openpilot-runtime-inputs --prepare-capnp --plan rust/native-capnp-assets.json \
  --cache "$NATIVE_ASSET_CACHE" --store "$NATIVE_SDK_STORE" > "$CAPNP_PREPARATION_REPORT"
```

준비기는 dpkg/설치 script를 호출하지 않는다. SHA·크기가 검증된 원본에서 좁은 Debian
ar v2 / control.tar.zst / data.tar.zst 형식만 읽고 Package/Version/Architecture를 확인한다.
모든 usr 파일·상대 파일 link·원래 copyright와 control을 보존한다. 탈출/절대·directory
link/상충 경로/set-id/특수 파일·추가 ar member를 거부한다. zstd window128MiB, control1MiB,
전체 payload64MiB, member/receipt 한도를 적용하고 예상 해제 증가량을 공간에 더한다.

같은 내용은 원본 압축 bytes로 기대 tree를 매번 다시 유도한다. 기존 receipt와 payload를
함께 고쳐도 재인증하지 못한다. 준비 중 lock을 유지하고 별도 임시 경로에 fsync한 후
새 내용 prefix만 게시한다. 정상 재사용은 HTTP body0/tree copy0, 다른 package 변경은
그 원본만 내려받고 이전 prefix를 보존해야 한다. URL·공간 정책·project SHA는 동일한
vendor 내용의 identity에 들어가지 않는다. report의 copied_bytes/elapsed_ms는 준비량과
경과 시간이며 전체 빌드의 peak RAM/디스크로 표현하지 않는다.

report의 root와 tree_receipt/tree_sha256을 해당 group의 pinned input tree에 연결한다.
이 prefix는 package가 의존하는 시스템 glibc/libstdc++/OpenSSL 전체의 별도 검증을
대체하지 않는다. `prepare-capnp` Cargo feature는 direct tool의 기본값이며, launcher/
updater/dev-tools가 사용하는 default-features=false 설정에는 추가되지 않는다.

foundation에 정상/반복/한 package 변경, receipt 재작성 후 변조, 잘못된 경로·link·package·
해제 한도·잘린 stream, 자원 부족·동시 owner·raw cache 변조의 압축 fixture4개를 추가했다.
fixture ELF 모양 파일은 실행하지 않는다. 실제 vendor 검증은 Actions에서 독립 dpkg-deb
tree의 usr 전체와 경로/hash/bytes/mode/link를 비교한다. 기존 Rust prefix 준비도 같은
strict tree helper를 재사용하며 기존 원본 유도/압축 fixture 회귀를 유지한다.

genuine AGNOS 추가 단계는 별도 prefix의 실제 capnp1.0.1을 실행하고, 고정 Rust
toolchain으로 실제 `openpilot-cereal` 전체 schema와 작은 `native_roundtrip` 예제를
빌드·실행한다. carState/Unicode logMessage의 header·validity·payload를 확인하고 원래
전체 Python cereal/pycapnp2.1.0가 만든 wire bytes2개와 정확 비교한다. 축소 schema나
candidate 출력으로 기대값을 만들지 않는다. 동일 입력 반복은 offline/fresh/같은 실행 파일
hash여야 하며 SDK tree도 전후 동일해야 한다. host·guest·emulation·원본 확장 모듈과
schema/source/compiler hash를 기록한다. 예제는 일반 daemon을 시작하지 않는다.
그 전에 실제 source-built ARM inputs CLI가 같은 raw cache를 사용해 SDK를 처음/반복
준비하고 host의 검증된 tree와 같은 SHA인지 확인한다. 이 guest 준비의 원본 전송은
처음/반복 모두0, tree 복사는 처음만 발생해야 한다. 실제 compiler 실행은 이후 단계다.

이 변경의 기능·원본 비교·ARM 실행은 새 정확 source의 Actions 결과 전까지 미검증이다.
프로젝트 SDK 준비 명령은 장치의 자동 SDK/bootstrap과 기본 시작으로 아직 연결하지
않았다. 모든 native header/library/tool의 closure, 기타 provider/기존 Python 모델 준비,
전체99개 build/seal/기본 시작·업데이트·복구와 comma3X 실제 기록·업로드 수용은 남는다.
이 두 packet이나 schema 빌드만으로 전체 runtime 동등성/기기 수용을 체크하지 않는다.
