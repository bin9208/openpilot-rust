#!/usr/bin/env bash
# 기존 planner-runtime step의 native controller 호출만 소유한다. SDK/transport는 caller가 봉인한다.
set -euo pipefail
umask 077
[[ $# == 1 && ( ${1:-} == runtime || ${1:-} == memory ) ]] || { echo 'expected runtime or memory' >&2; exit 64; }
mode=$1
source_dir=${CI296_SOURCE:?}
job=${CI_JOB_TEMP:?}
host=${NATIVE_TOOLS_HOST:?}
[[ ${GITHUB_ACTIONS:-}:${GITHUB_REPOSITORY:-}:${GITHUB_JOB:-} == true:bin9208/openpilot-rust:planner-runtime ]]
[[ $PWD == "$source_dir" && $GITHUB_WORKSPACE == "$source_dir" && $RUNNER_TEMP == "$job" ]]
[[ $job == /tmp/openpilot-ci.?????? && ! -L $job && $(realpath -e -- "$job") == "$job" ]]
[[ $HOME == "$job/home" && $CARGO_HOME == "$job/cargo-home" ]]
[[ $(realpath -e -- "$source_dir") == "$source_dir" && ! -L $source_dir ]]
[[ $(git --no-optional-locks -C "$source_dir" rev-parse HEAD) == "$GITHUB_SHA" ]]
[[ $(cat "$job/owner") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" ]]
case $(uname -m) in
  x86_64) [[ $host == x86_64-unknown-linux-gnu ]]; seconds=2700 ;;
  aarch64)
    [[ $host == aarch64-unknown-linux-gnu && ${CI296_NATIVE_ARM:-} == 1 &&
       ${RUNNER_ENVIRONMENT:-}:${RUNNER_OS:-}:${RUNNER_ARCH:-} == github-hosted:Linux:ARM64 &&
       ${OPENPILOT_CI_EXECUTION_MODE:-} == native-host &&
       ${OPENPILOT_CI_HOST_ARCH:-}:${OPENPILOT_CI_TARGET_ARCH:-} == aarch64:aarch64 ]] || {
      echo 'Planner requires provisioned native ARM host transport' >&2; exit 1;
    }
    seconds=5400 ;;
  *) echo 'unsupported Planner host' >&2; exit 1 ;;
esac
[[ $mode != memory || $host == x86_64-unknown-linux-gnu ]]
# 실제 leaf는 기존 private venv를 사용한다. stdlib 전용 PLANNER_REFERENCE_PYTHON과 섞지 않는다.
planner_python=$(command -v python)
[[ $planner_python == "$job/venv/bin/python" && -x $planner_python ]]
sdk=${RUSTUP_HOME:?}/toolchains/1.94.0-$host
case ${OPENPILOT_CI_PREPARED_RUST_MODE:-} in
  native-arm-530-v1)
    [[ $host == aarch64-unknown-linux-gnu && $RUSTUP_HOME == "$job/arm530-sdk/rustup-home" &&
       $sdk == "${OPENPILOT_CI_PREPARED_RUST_ROOT:-}" &&
       ${OPENPILOT_CI_PREPARED_RUST_MANIFEST:-} == "$job/proof/arm530-sdk/sdk.before.sha256" &&
       ${RUSTUP_AUTO_INSTALL:-} == 0 ]] || {
      echo 'Planner prepared ARM SDK mode, root, manifest or installer policy differs' >&2; exit 1;
    } ;;
  '')
    # host 및 기존 standalone ARM installer 경로다. reusable ARM의 prepared 누락은 거부한다.
    [[ $RUSTUP_HOME == "$job/rustup-home" &&
       -z ${OPENPILOT_CI_PREPARED_RUST_ROOT:-} && -z ${OPENPILOT_CI_PREPARED_RUST_MANIFEST:-} &&
       ( $host == x86_64-unknown-linux-gnu || -z ${CI296_NATIVE_ARM_CALLER:-} ) ]] || {
      echo 'Planner requires the provisioned job Rust home without prepared SDK metadata' >&2; exit 1;
    } ;;
  *) echo 'unsupported Planner prepared SDK mode' >&2; exit 1 ;;
esac
[[ -x $sdk/bin/cargo && -x $sdk/bin/rustc && -x $sdk/bin/rustdoc ]]
export CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export OPENBLAS_NUM_THREADS=1 RUSTUP_AUTO_INSTALL=0
proof=$job/proof/planner-native-controller-523
controller=$source_dir/rust/target/$host/debug/check-plannerd-ci
if [[ $mode == runtime ]]; then mkdir "$proof"; else [[ -d $proof && ! -L $proof ]]; fi

positive_mib() {
  [[ $1 =~ ^[0-9]{1,9}$ ]] && ((10#$1 > 0))
}
admit() {
  local label=$1 path=$2 mib=$3 available
  positive_mib "$mib"
  available=$(df --output=avail -B1 -- "$path" | awk 'NR==2 && NF==1 && $1~/^[0-9]+$/ {print $1}')
  [[ $available =~ ^[0-9]+$ ]]
  printf '%s\t%s\t%s\t%s\n' "$label" "$path" "$available" "$((10#$mib * 1024 * 1024))" >> "$proof/$mode-admission.tsv"
  ((available >= 10#$mib * 1024 * 1024))
}
finish() {
  local status=$? observed=0
  trap - EXIT
  set +e
  df -PB1 -- "$source_dir" "$job" > "$proof/$mode-disk-after.txt" || observed=1
  [[ $status != 0 || $observed == 0 ]] || status=1
  printf 'mode=%s\nsource=%s\nexit_code=%s\n' "$mode" "$GITHUB_SHA" "$status" > "$proof/$mode-outcome.txt" || { [[ $status != 0 ]] || status=1; }
  exit "$status"
}
trap finish EXIT
df -PB1 -- "$source_dir" "$job" > "$proof/$mode-disk-before.txt"
awk '/^(MemTotal|MemAvailable):/ {print}' /proc/meminfo > "$proof/$mode-memory-before.txt"
printf '%s\n' "$planner_python" > "$proof/$mode-python.txt"

growth_args=()
if [[ $mode == runtime ]]; then
  leaves=(abi policy radar gap owner main fault volkswagen stock-longitudinal radar-off ipc lifecycle)
else
  leaves=(abi owner fault)
fi
for leaf in "${leaves[@]}"; do
  # 기존 360-frame 파일/JSON, 40x4 IPC, bounded lifecycle 출력의 보수적 예상 증가량이다.
  # 실제 peak 측정값은 아니다. 각 명령은 native inspect로 자기 출력 filesystem을 다시 검사한다.
  case $leaf in
    abi|policy|radar|gap) estimate=128 ;;
    ipc|lifecycle) estimate=256 ;;
    *) estimate=1024 ;;
  esac
  variable=PLANNER_${leaf^^}_GROWTH_MIB
  variable=${variable//-/_}
  if [[ -v $variable ]]; then estimate=${!variable}; fi
  positive_mib "$estimate"
  growth_args+=(--growth-mib "$leaf=$estimate")
  printf '%s\t%s\t%s\n' "$leaf" "$estimate" "$seconds" >> "$proof/$mode-operations.tsv"
done

if [[ $mode == runtime ]]; then
  # native-json11과 같은 host/target 및 compiler 환경 정리. 새 빌드의 병렬수만 1이다.
  admit controller-build "$source_dir" "${PLANNER_CONTROLLER_BUILD_GROWTH_MIB:-2048}"
  "$sdk/bin/rustc" -vV > "$proof/rustc.txt"
  grep -Fx 'release: 1.94.0' "$proof/rustc.txt"
  grep -Fx "host: $host" "$proof/rustc.txt"
  env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS -u CARGO_BUILD_TARGET -u RUSTC_WRAPPER -u RUSTC_WORKSPACE_WRAPPER \
    -u CC -u CXX -u CFLAGS -u CXXFLAGS -u CPPFLAGS -u LDFLAGS \
    CARGO_TARGET_DIR="$source_dir/rust/target" RUSTC="$sdk/bin/rustc" RUSTDOC="$sdk/bin/rustdoc" \
    "$sdk/bin/cargo" build --manifest-path "$source_dir/rust/Cargo.toml" -p openpilot-dev-tools \
      --locked --features ci-receipt-metadata-native --bin check-plannerd-ci --target "$host"
  [[ -x $controller && ! -L $controller ]]
  sha256sum -- "$controller" > "$proof/controller.sha256"
  printf '%s:%s\n' "$GITHUB_SHA" "$host" > "$proof/controller-source.txt"
  "$controller" --root "$source_dir" --python "$planner_python" --bin-dir "$source_dir/rust/target/debug" \
    --artifact "$job/planner-artifact" --source-native "$job/planner-source" --binding "${PLANNER_PARAMS_BINDING:?}" \
    --output "$job/planner-comparison" --timeout-seconds "$seconds" "${growth_args[@]}"
else
  # runtime에서 방금 빌드한 같은 source/host 바이너리를 재사용한다. 재빌드/installer는 없다.
  [[ $(cat "$proof/controller-source.txt") == "$GITHUB_SHA:$host" ]]
  sha256sum --check --strict -- "$proof/controller.sha256"
  [[ -x $controller && ! -L $controller ]]
  admit sanitizer-wrapper "$job" 128
  "$planner_python" -P "$source_dir/rust/tools/check_plannerd_native_memory.py" \
    --bin-dir "$source_dir/rust/target/debug" --artifact "$job/planner-sanitized" \
    --source-native "$job/planner-source" --binding "${PLANNER_PARAMS_BINDING:?}" \
    --asan-library "$(gcc -print-file-name=libasan.so)" --output "$job/planner-native-memory" \
    --controller "$controller" --python "$planner_python" --timeout-seconds "$seconds" "${growth_args[@]}"
fi
