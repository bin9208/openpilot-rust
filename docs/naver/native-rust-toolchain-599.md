# 내용별 native Rust 도구체인 준비 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599),
[캐시·세대 #600](https://github.com/bin9208/openpilot-rust/issues/600).

기존 `openpilot-runtime-inputs`의 `--prepare-rust` 모드는 전체 runtime release와
독립적으로 Rust1.94.0 AArch64 GNU compiler·Cargo·표준 라이브러리를 준비한다.
`rust/native-rust-toolchain.json`의 pin은 공식
[Rust1.94.0 manifest](https://static.rust-lang.org/dist/channel-rust-1.94.0.toml)를
2026-10-27 KST에 읽어 확인했다. manifest 날짜는2026-03-05, 원문 SHA는
`aaa177def36e01d539bee6bde95295230b9ce378f81057845db8d0ebe97898ee`다.

| 자산 | 압축 bytes | SHA-256 |
| --- | ---: | --- |
| rustc | 61188172 | `7261b2fcdb88aef8b21f4b9f915d9a75c959674193f5b38e13fab4569684fc5b` |
| cargo | 10398516 | `d57ab7b23b52cba46e58c78327904d0c7db19892988677b0d59495c8af4bb790` |
| rust-std | 28485880 | `c781b3ef4fefa5508fbe05820eddc95e46351d905a30921cc020febd9c596a2e` |

합계100072568 bytes의 HTTP body다. 각 원본 HEAD의 크기는 위 값과 같았다.
이 읽기 감사는 새 준비기의 기능 시험이나 compiler 실행을 뜻하지 않는다.

## 준비·재사용 계약

- 기존 SHA/길이 검증 blob cache와 per-object lock을 재사용한다. 캐시 hit도 검증하고
  잘못된 기존 blob을 지우거나 신뢰해서 덮어쓰지 않는다.
- 인증한 세 압축 파일에서 실제 component 파일을 직접 준비한다. `install.sh`를 실행하지
  않는다. 원본 LICENSE/COPYRIGHT, component manifest와 고정 자산 provenance를 보존한다.
- 절대/탈출/중복 경로, 외부 파일 link, directory link, link cycle, special node, hardlink,
  set-id, 상충하는 파일을 거부한다. file link는 toolchain 내부 file로만 끝나야 한다.
- xz decoder 메모리는128MiB로 제한한다. component의 총 해제 파일 크기, 해제 stream,
  member 수·경로 길이와 기존 tree receipt16MiB 한도도 확인한다. 각 작업의 압축/해제
  예상 증가량과 명시한 여유를 admission하며 고정25/35GiB floor는 없다.
- compiler/Cargo/rustdoc의 ARM ELF와 target libstd 존재를 확인한다. executable은0555,
  data는0444로 준비하고, 새 generation을 fsync/원자적 rename 후 발행한다.
  실행 중인 runtime·기존 도구체인·로그를 전환하거나 삭제하지 않는다.
- 반복 준비도 인증한 **압축 원문**에서 예상 tree를 다시 유도해 실제 파일·권한·link와
  대조한다. 변경 compiler에 맞춰 로컬 tree/receipt를 함께 다시 써도 통과할 수 없다.
  정상 hit는 body 전송0, toolchain tree 복사0이다. 검증을 위한 읽기는 계속 수행한다.
- URL/공간 예측 변경은 같은 실제 toolchain의 compiler identity를 바꾸지 않는다.
  내용 변경은 새 내용 ID에 별도 prefix를 만들고 이전 prefix를 보존한다.

## 개발 명령과 기존 full plan 연결

준비된 source-built helper가 있는 Actions/AGNOS 개발 환경에서 사용한다. 현재 로컬
기능/빌드 실행 금지는 유지한다. 아래 cache/store의 부모는 미리 존재하는 자기 작업
디렉터리여야 하며 출력 report도 Git 소스 밖의 소유 경로에 둔다.

```sh
openpilot-runtime-inputs --prepare-rust --plan rust/native-rust-toolchain.json \
  --cache "$NATIVE_ASSET_CACHE" --store "$NATIVE_TOOLCHAIN_STORE" > "$RUST_PREPARATION_REPORT"
```

report의 `root/bin`은 Config의 `toolchain`, `root`는 Layout의 `toolchain_root`다.
report의 `tree_receipt`→`tree_sha256`을 신뢰한 bindings의 `files`에 고정하고,
`trees[root]=tree_receipt`로 common input set에 넣는다. 기존 generation 검사가 실제
compiler 위치/hash와 전체 Rust tree가 각 build group에 묶였는지 계속 확인한다.

`copied_bytes`는 toolchain tree에 복사한 payload, `elapsed_ms`는 준비·검증의 경과
시간이다. filesystem peak나 전체 빌드 peak memory로 표시하지 않는다.
`compiler_executed=false`, `runtime_activated=false`는 준비 도구의 실제 범위다.

압축 처리 의존성은 Cargo의 `prepare-rust` feature에 한정한다. 직접 빌드하는 도구는
이 feature를 기본 포함하고, launcher/updater/dev-tools가 사용하는 기존 build/selection
라이브러리는 default feature를 끄므로 준비기를 위해 tar/xz 의존성을 추가하지 않는다.

## 검증과 미완료 조건

foundation의 기존 actual Cargo/Clippy 검사에 압축 fixture4개 회귀를 추가했다.
정상 준비·body/copy0·component 하나 변경의 나머지 전송0·이전 prefix 보존,
receipt 재작성 후 compiler 변조 거부, 탈출/잘못된 link·경로·낮은 용량·동시 owner·
잘못된 원문/해제 한도 거부를 검사한다. fixture ELF 모양의 파일은 실행하지 않으며
실제 Rust compiler 검증으로 표현하지 않는다.

genuine AGNOS workflow는 공식 세 자산을 실제 CLI로 준비/반복하고, 그 prefix를 원본
AGNOS user space에 별도로 연결한다. 원래 Rustup pin·명령·4case는 유지하고 실제
프로젝트 build driver가 새 prefix의 compiler/Cargo/std로 두 Rust 프로그램을 빌드·
실행/재사용하게 한다. 같은 크기·권한의 stdlib1byte 변조는 compiler 호출 전 실패해야 하며
기존 binary는 계속43/42를 출력해야 한다. source/host/guest/emulation과 정확 SHA를 기록한다.

새 source의 Actions 결과를 읽기 전까지 이 준비기는 **검증 대기**다. 초기 source helper
bootstrap, offline Cargo dependency 준비, native C/C++ SDK의 전체 header/library/tool closure,
provider별 입력·Python 모델 준비, 전체 runtime build/seal/기본 시작·업데이트·복구와
comma3X 실제 녹화·기존 업로드 수용은 여전히 남는다. 이 prefix 준비만으로 #598을 닫지 않는다.
