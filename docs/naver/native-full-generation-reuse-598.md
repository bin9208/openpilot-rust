# #598 Genuine 전체 generation 재사용 판정 보정

2026-10-30 정적 review 후 보정. 기존 `native_full_generation`의 logger group만 허용하는
조건은 잠재 검증 결함이며, 이것이 실제 CI에서 실패했다는 증거는 아직 없다.
이 commit은 example 검증 코드와 이 기록만 포함한다. production runtime/library,
inputs/catalog/gates/workflow는 변경하지 않는다. 공개 pin driver 보정 commit과 분리해
부모가 현재 main source에 독립적으로 가져올 수 있게 한다.

## 실제 의존성과 서로 다른 두 판정

- `loggerd/build.rs`는 `src` 전체를 추적한다. main.rs help 변경으로 재실행하면 새
  `LOGGERD_SOURCE_COMMIT`이 library에도 들어간다. `loggerd/src/metadata.rs`가 이 stamp를
  사용하고, 직접 소비자인 bootlog의 `capture()`가 `Environment::init_data()`로 기록한다.
- 모든 `rust/crates/*/Cargo.toml`의 normal/target path dependency를 역으로 따라간 실제
  nonoptional closure는 `loggerd→bootlog→manager`다. catalog는 각각 `07`·`08`·`09`에
  해당 binary를 둔다. source dependency 회귀는 기존 optional `toml` 의존성을 사용하는
  `prepare-cargo` feature에서 실제 manifest graph와 catalog를 대조한다.
- small-logger의 세 binary는 실제 Cargo report에서 모두 fresh=false여야 한다.
  manager는 bootlog snapshot 경로를 소비하므로 재컴파일/재링크와 최종 ELF hash 변화는
  구분한다. logger·bootlog object 변경은 필수이고 manager object는 동일해도 허용한다.
  실제 `rebuilt_groups`와 `changed_binaries`를 별도 evidence로 기록한다.
- 영향 group 안의 무관 binary도 예외가 아니다. 나머지96개는 모두 fresh=true이며
  object가 이전 결과와 같아야 한다. repeat/docs는 전 group fresh·모든 object 동일·
  새 binary 저장0을 계속 요구한다. group 전체를 무제한 허용하지 않는다.

## 기존 help 실행의 loader 계약

Genuine example은 sealed logger가 아니라 실제 Cargo report의 logger executable을
실행한다. 따라서 sealed의 `rust/bin/lib`를 임의로 적용하지 않는다.
`catalog_bindings.rs`가 생성하고 실제 build recipe가 사용한 group `07`의
`LD_LIBRARY_PATH`(Cap'n Proto/FFmpeg provider library 경로)를 그대로 전달한다.
각 절대 경로는 report의 검증한 compiler-provider tree 안에 있어야 하며,
SDK의 `/` tree만으로 임의 경로를 인정하지 않는다. 상속 환경의 임의 LD 경로도 추가하지 않는다.
기존 `--help` 실행·문자열·정상 exit 요구는 유지하며 loader 계약 누락/경로 이탈은 거부한다.

초기99 binaries/13 groups, source·보존 store·seal/ABI·staging cache 제거 후 검증,
runtime 비활성/미시작 및 로그 보존 경계는 그대로다. reuse가 seal 전에 반환하는
기존 engineering 범위도 확대해 표현하지 않는다.

## 작성한 회귀와 남은 검증

실제 catalog의99/13 roster를 사용하는 순수 receipt 회귀에 manager 재컴파일+ELF 동일,
manager ELF 변경, 필수 logger/bootlog 변경 누락, 세 소비자 rebuild 누락,
영향/비영향 group의 무관 binary 변경·fresh=false 거부를 포함했다.
loader는 원래 검증한 recipe 환경을 전달하는지와 누락/빈값/상대경로/미검증경로 거부를 확인한다.

로컬은 rustfmt·diff 검토만 수행한다. unit/기능/build·원격 push/CI·기기 접속은 실행하지
않으며 실제 성공을 주장하지 않는다. Actions의 정확 source 결과와 원본 report가 이후 필요하다.
