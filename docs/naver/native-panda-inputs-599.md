# 현재 소스와 결속한 독립 Panda firmware — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
재사용 [#600](https://github.com/bin9208/openpilot-rust/issues/600).
Panda의 독립 안전 firmware는 기존 C 구현과 서명 정책을 유지한다.
이 단계는 기존에 검증·배포된 출력의 입력 동일성을 검사하며 새 firmware를 생성하지 않는다.

## 원래 입력·출력

원래 source는 a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31이다.
현재 소스의 panda, opendbc 심볼릭 링크, 실제 opendbc_repo, pyproject.toml, uv.lock을
Git namespace로 검사한다. opendbc/safety만 git ls-tree에 넘기면 심볼릭 링크 아래 실제
입력이 빠지므로 opendbc_repo 전체도 포함한다. 제외 경로 없이871개 entry의 정렬된
mode/type/Git object/path와 NUL을 해시한 값은
1387444d47db8c8b36bfde4170d61540c2554b964678facd0ed8036137a46ebc이다.
원래 커밋과 현재 커밋의 전체 namespace가 같음을 읽기 감사로 확인했다.
기존 provenance에 기록된251개 파일도 모두 현재 파일의 SHA-256과 같다.

rust/native-panda-source-contract.json의 SHA-256은
206916e6a5722e7159e0607b2dcef1dd93f31124a2d9462ef875c235d3e1ae58이다.
이 contract는 원래 source와 전체 입력 namespace, firmware4개·LICENSE·provenance의
출력 SHA를 고정한다. 문서나 Rust 변경은 입력 집합 밖이지만 panda/opendbc 파일 추가,
변경·삭제·링크 변경과 dirty/검사 중 source 변경은 재사용을 거부한다.

| 출력 | bytes | SHA-256 |
| --- | --- | --- |
| bootstub.panda.bin | 13796 | 52ac7ba40cb123fe52ef1ba609417eb602ad924c9bff2cdc6d1009a464908670 |
| bootstub.panda_h7.bin | 18608 | 0c77563bec36e9836706d7dd18b1fcd1e4f74651bdc5b2d17950daf588cac7ed |
| panda.bin.signed | 58160 | 9af58e47888c3d445536ce151218bdec7dec4b9fe4e56d9f5fb5ded3e3470a88 |
| panda_h7.bin.signed | 85928 | db99218a885a7438ed4a8923fd146f2af961abbc533ab937d8c5ee310080ffeb |

원래 버전 DEV-1de1c7b1f187-DEBUG, Arm GNU Toolchain13.2.rel1/13.2.1,
기록된 compiler SHA784012433f49b420ca5d63611e36ef1dab7764dde3bb1d164da39ea67b27d300,
기존 DEBUG/ALLOW_DEBUG·공개 debug 인증서·RSA1024 raw SHA1·SETLEN=1/VERS=2를 보존한다.
과거 compiler 경로·명령·source_inputs·signing/scope를 현재 커밋으로 바꾸지 않는다.
현재 compiler 실행이나 새 firmware 빌드·release signing을 했다고 표현하지 않는다.

## 별도 자산과 필수 검사

native-panda-provider.json의 고정 profile은 panda-firmware-runtime이다.
원래 archive1155969168 bytes/SHA9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291,
원래 runtime manifest SHA8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee를
검사해 firmware4개·원래 라이선스/빌드 기록2개·manifest1개만 선택한다.
7개 payload 합계447865 bytes, inventory SHA는
4a6a1bed302950a8120ab0bedfb30c511263a19a50dbb3653434c0b1be7df767이다.
전부444이며 project Rust executable은 포함하지 않는다.
provider에는 현재 소스의 source-contract sidecar를 추가한다.

공통 strict provider 경로를 재사용한다. 이 profile은 source-bound role과 --source-root가
필수이며 현재 namespace 확인 전에 다운로드/staging하지 않는다. 독립 component namespace는
native-provider-panda-firmware-<component SHA>/panda-firmware-runtime.tar.gz이다.
원래 archive는 CI의 producer 입력이고 소비자는 작은 component만 사용한다.
아직 공개 component 발행·독립 pin 배포·공개 다운로드를 완료한 상태가 아니다.

전체 native 빌드에서 openpilot-pandad를 선택하면 빌드 전/봉인 전 Layout에 contract와
모든 출력이 같은 SHA로 매핑되어야 한다. 실제 봉인 후보 및 boot/fallback에서도 retained
source와 실제4개 firmware/라이선스/원래 provenance/sidecar를 다시 검사한다.
기존 source/object/manifest/ABI 검사는 유지하며 boot에 compiler/provider cache가 필요 없다.
입력이 바뀌면 새로 검증한 firmware와 contract를 준비해야 한다. 자동 flash나 기존 세대
변경으로 이 실패를 회피하지 않는다. 외부 라이브러리 profile로 취급해 입력 검사를 생략할 수 없다.

## CI와 검사 방법

foundation의 기존 runtime-build test/Clippy/no-default 검사에 네 가지 소유 Git fixture를
추가했다. 문서/Rust 변경의 재사용, safety를 포함한 입력 추가·변경/dirty 거부, 누락·경로 이탈·
다른 출력/sidecar 거부, provider cache 없는 runtime 검사와 입력 범위 축소 거부를 확인한다.
이 fixture의 메타데이터용 가짜 bytes를 실제 firmware 동작 증거로 사용하지 않는다.
기존 MPC profile의 pin·Tree·출력은 유지하고 같은 source-binding 선택부를 공유한다.

genuine host helper는 작업별 예상4×payload+16MiB=18568676 bytes를 확인하고 export/준비/
재사용을 실행한다. 원래 archive 추가 전송0, 소비자의 원래 archive 다운로드 없음,
반복 HTTP body/복사0, 원래/current source 구분을 검사한다. source-built ARM helper는
별도 prefix에서 같은 검사를 수행하며 source-root 없는 요청은 staging 전에 거부한다.
RuntimeInputs는 기존9개와 Panda1개를 합한10개이며 compiler identity와 분리한다.

독립 Python 표준 라이브러리 fixture는 원래 입력251개 hash, 원래 출력 hash/size/mode,
현재 firmware_source_version 함수의 결과 및 네 출력의 버전 문자열을 검사한다.
두 signed image의 length/VERS2와 raw RSA1024/SHA1 서명을 기존 공개 debug 키로 확인한다.
private signing key를 읽지 않으며 펌웨어를 실행·플래시하거나 장치에 연결하지 않는다.
provider의 aarch64 target은 소비 AGNOS 환경이고 firmware CPU는 Cortex-M4/M7이다.

```sh
gh run list --repo bin9208/openpilot-rust --workflow runtime-source-build.yml --branch BRANCH
gh run list --repo bin9208/openpilot-rust --workflow runtime-native-environment.yml --branch BRANCH
gh run view RUN_ID --repo bin9208/openpilot-rust --log-failed
```

정확 SHA의 native-panda export/initial/reused JSON, source_binding, Tree receipt,
native-panda-firmware-check.json과 명령 기록을 확인한다. 새 기능 결과는 대기다.
로컬은 구문/서식/diff 및 Git/캐시 파일 읽기 감사만 수행했다.
bin-actions x64 위 QEMU/PRoot ARM user space는 물리 ARM CPU/AGNOS kernel 부팅이 아니다.
실제 Panda boot/USB/SPI·안전 회귀 전체/실기 flash, Python 모델 환경·새 모델 생성,
전체 Layout/99개 native build와 기본 업데이트/시작/녹화/업로드·복구, 실측 자원/성능,
정상 PR/병합 후 검사·최종 한영 문서·사용자 comma3X 수용은 여전히 별도 완료 조건이다.
