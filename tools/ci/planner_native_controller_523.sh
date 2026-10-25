#!/usr/bin/env bash
# 기존 planner-runtime step의 native controller 호출만 소유한다. SDK/transport는 caller가 봉인한다.
set -euo pipefail
umask 077
[[ $# == 1 && ( ${1:-} == runtime || ${1:-} == memory || ${1:-} == ipc-check || ${1:-} == ipc ) ]] || { echo 'expected runtime, memory, ipc-check or ipc' >&2; exit 64; }
mode=$1
profile=${PLANNER_NATIVE_IPC_PROFILE:-}
case $profile in
  '') controller_features=ci-receipt-metadata-native ;;
  planner-ipc-native-v1) controller_features=ci-receipt-metadata-native,planner-ipc-native ;;
  *) echo 'unsupported Planner native IPC profile' >&2; exit 1 ;;
esac
[[ $mode == runtime || $mode == memory || $profile == planner-ipc-native-v1 ]]
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
ipc_group_ready=false

positive_mib() {
  [[ $1 =~ ^[0-9]{1,9}$ ]] && ((10#$1 > 0))
}
admit() {
  if (($# == 0)); then set -- ipc-runtime "$job" 512; fi
  local label=$1 path=$2 mib=$3 available
  positive_mib "$mib"
  available=$(df --output=avail -B1 -- "$path" | awk 'NR==2 && NF==1 && $1~/^[0-9]+$/ {print $1}')
  [[ $available =~ ^[0-9]+$ ]]
  printf '%s\t%s\t%s\t%s\n' "$label" "$path" "$available" "$((10#$mib * 1024 * 1024))" >> "$proof/$mode-admission.tsv"
  ((available >= 10#$mib * 1024 * 1024))
}
sdk_snapshot() {
  local phase=$1
  find -P "$sdk" -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%y\t%p\t%l\0' | sort -z > "$proof/sdk.$phase.roster" || return 1
  find -P "$sdk" -type f -print0 | sort -z | xargs -0 -r sha256sum > "$proof/sdk.$phase.sha256" || return 1
  find -P "$sdk" -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%y\t%p\t%l\0' | sort -z > "$proof/sdk.$phase.stable" || return 1
  cmp "$proof/sdk.$phase.roster" "$proof/sdk.$phase.stable"
}
finish() {
  local status=$? observed=0 cleanup=0 waited=not-started nonlive=true leader=
  trap - EXIT INT TERM HUP
  set +e
  if [[ $ipc_group_ready == true ]]; then
    waited=${owned_waited:-not-started}
    if [[ -n $active ]]; then
      leader=$active
      # go 이전 handshake 실패도 직접 자식의 PGID/session/startticks가 확인될 때만 회수한다.
      if [[ -z $identity ]]; then identity=$(pid_identity "$leader") || { cleanup=1; nonlive=false; }; fi
      if kill_owned; then waited=0; wait -f "$leader" || waited=$?; else cleanup=1; nonlive=false; fi
      group_quiet "$leader" '' "$out/logs/final.group" || { cleanup=1; nonlive=false; }
    fi
    [[ $groups_ok == true ]] || { cleanup=1; nonlive=false; }
    ((cleanup == 0)) || observed=1
    verify_held || observed=1
    printf 'original_exit=%s\ncleanup_exit=%s\ncleanup_wait=%s\nowned_groups_nonlive=%s\nall_adopted_children_reaped=false\n' \
      "$status" "$cleanup" "$waited" "$nonlive" > "$out/finalizer.status" || observed=1
  fi
  if [[ -f $proof/controller.sha256 ]]; then
    sha256sum --check --strict "$proof/controller.sha256" > "$proof/$mode-controller-after.txt" || observed=1
  elif [[ $mode != runtime ]]; then observed=1; fi
  if [[ $profile == planner-ipc-native-v1 ]]; then
    sdk_snapshot "$mode.after" &&
      cmp "$proof/sdk.before.sha256" "$proof/sdk.$mode.after.sha256" &&
      cmp "$proof/sdk.before.roster" "$proof/sdk.$mode.after.roster" || observed=1
  fi
  df -PB1 -- "$source_dir" "$job" > "$proof/$mode-disk-after.txt" || observed=1
  [[ $status != 0 || $observed == 0 ]] || status=1
  printf 'mode=%s\nsource=%s\nexit_code=%s\n' "$mode" "$GITHUB_SHA" "$status" > "$proof/$mode-outcome.txt" || { [[ $status != 0 ]] || status=1; }
  exit "$status"
}
trap finish EXIT
df -PB1 -- "$source_dir" "$job" > "$proof/$mode-disk-before.txt"
awk '/^(MemTotal|MemAvailable):/ {print}' /proc/meminfo > "$proof/$mode-memory-before.txt"
printf '%s\n' "$planner_python" > "$proof/$mode-python.txt"
if [[ $profile == planner-ipc-native-v1 && $mode == runtime ]]; then
  [[ ! -L $sdk && $(readlink -e -- "$sdk") == "$sdk" ]]
  sdk_snapshot before
fi
if [[ $mode != runtime ]]; then
  [[ $(cat "$proof/controller-source.txt") == "$GITHUB_SHA:$host" &&
     $(cat "$proof/controller-profile.txt") == "$controller_features" ]]
  sha256sum --check --strict -- "$proof/controller.sha256"
  [[ -x $controller && ! -L $controller ]]
fi

growth_args=()
if [[ $mode == runtime ]]; then
  leaves=(abi policy radar gap owner main fault volkswagen stock-longitudinal radar-off ipc lifecycle)
elif [[ $mode == memory ]]; then
  leaves=(abi owner fault)
else
  leaves=()
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
      --locked --features "$controller_features" --bin check-plannerd-ci --target "$host"
  [[ -x $controller && ! -L $controller ]]
  sha256sum -- "$controller" > "$proof/controller.sha256"
  printf '%s:%s\n' "$GITHUB_SHA" "$host" > "$proof/controller-source.txt"
  printf '%s\n' "$controller_features" > "$proof/controller-profile.txt"
  "$controller" --root "$source_dir" --python "$planner_python" --bin-dir "$source_dir/rust/target/debug" \
    --artifact "$job/planner-artifact" --source-native "$job/planner-source" --binding "${PLANNER_PARAMS_BINDING:?}" \
    --output "$job/planner-comparison" --timeout-seconds "$seconds" "${growth_args[@]}"
elif [[ $mode == memory ]]; then
  # runtime에서 방금 빌드한 같은 source/host 바이너리를 재사용한다. 재빌드/installer는 없다.
  [[ $(cat "$proof/controller-source.txt") == "$GITHUB_SHA:$host" ]]
  sha256sum --check --strict -- "$proof/controller.sha256"
  [[ -x $controller && ! -L $controller ]]
  admit sanitizer-wrapper "$job" 128
  "$controller" memory --root "$source_dir" \
    --bin-dir "$source_dir/rust/target/debug" --artifact "$job/planner-sanitized" \
    --source-native "$job/planner-source" --binding "${PLANNER_PARAMS_BINDING:?}" \
    --asan-library "$(gcc -print-file-name=libasan.so)" --output "$job/planner-native-memory" \
    --python "$planner_python" --timeout-seconds "$seconds" "${growth_args[@]}"
elif [[ $mode == ipc-check ]]; then
  admit ipc-qualification "$source_dir" 2048
  failed=0
  for check in observed clippy; do
    if [[ $check == observed ]]; then
      args=(test --lib planner_ci_controller::planner_ipc_native::observed_tests:: -- --test-threads=1 --nocapture)
    else
      args=(clippy --lib --bin check-plannerd-ci -- -D warnings)
    fi
    # 동일 feature/SDK/target으로 검사하고 integration target의 CLI 재빌드를 피한다.
    command=("$sdk/bin/cargo" "${args[0]}" --manifest-path "$source_dir/rust/Cargo.toml" -p openpilot-dev-tools --locked
      --features "$controller_features" --target "$host" "${args[@]:1}")
    status=0
    env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS -u CARGO_BUILD_TARGET -u RUSTC_WRAPPER -u RUSTC_WORKSPACE_WRAPPER \
      -u CC -u CXX -u CFLAGS -u CXXFLAGS -u CPPFLAGS -u LDFLAGS \
      CARGO_TARGET_DIR="$source_dir/rust/target" RUSTC="$sdk/bin/rustc" RUSTDOC="$sdk/bin/rustdoc" \
      "${command[@]}" > "$proof/ipc-$check.stdout" 2> "$proof/ipc-$check.stderr" || status=$?
    printf '%s\n' "$status" > "$proof/ipc-$check.status"
    tail -n 40 "$proof/ipc-$check.stdout" "$proof/ipc-$check.stderr"
    [[ $status == 0 ]] || failed=1
  done
  grep -Eq '^test result: ok\. 4 passed; 0 failed; 0 ignored;' "$proof/ipc-observed.stdout" || failed=1
  [[ $failed == 0 ]]
else
  case $host in
    x86_64-unknown-linux-gnu) audit_sha=384bcef1bafc962019557d7b89ef5dac4c85ee2d1683906c1942458b04118e54; ipc_seconds=600 ;;
    aarch64-unknown-linux-gnu) audit_sha=7dd3abad75e472a7241d16143b155dd3ad6e8bb826359bd0afedca1360689efc; ipc_seconds=1200 ;;
  esac
  [[ ${PLANNER_NATIVE_IPC_CORPUS:?} == "$job/planner-ipc-original" &&
     ${PLANNER_NATIVE_IPC_AUDIT:?} == "$job/planner-ipc-original/audit.json" &&
     ${PLANNER_NATIVE_IPC_AUDIT_SHA256:?} == "$audit_sha" ]]
  printf '%s  %s\n' "$audit_sha" "$PLANNER_NATIVE_IPC_AUDIT" | sha256sum --check --strict -
  shopt -s nullglob
  bindings=("$job/planner-original/ipc/msgq"/ipc_pyx.*.so)
  [[ ${#bindings[@]} == 1 ]]
  [[ -f ${bindings[0]} && ! -L ${bindings[0]} &&
     $(readlink -e -- "${bindings[0]}") == "${bindings[0]}" ]]
  compiler=$(readlink -e -- "$(command -v g++)")
  [[ -f $compiler && -x $compiler && ! -L $compiler ]]
  admit ipc-output "$job" 512
  [[ -d $PLANNER_NATIVE_IPC_CORPUS && ! -L $PLANNER_NATIVE_IPC_CORPUS &&
     $(readlink -e -- "$PLANNER_NATIVE_IPC_CORPUS") == "$PLANNER_NATIVE_IPC_CORPUS" &&
     $(stat -c '%u:%a' -- "$PLANNER_NATIVE_IPC_CORPUS") == "$(id -u):700" ]]
  # 감사 원본 ZIP bytes는 기존 이번-run capture와 별도 proof로 보존한다.
  historical=$proof/ipc-historical
  mkdir "$historical"
  for name in audit.json capture.zip receipt.zip; do
    [[ -f $PLANNER_NATIVE_IPC_CORPUS/$name && ! -L $PLANNER_NATIVE_IPC_CORPUS/$name &&
       $(stat -c %u -- "$PLANNER_NATIVE_IPC_CORPUS/$name") == "$(id -u)" &&
       $(stat -c %s -- "$PLANNER_NATIVE_IPC_CORPUS/$name") -le 67108864 ]]
    cp -- "$PLANNER_NATIVE_IPC_CORPUS/$name" "$historical/$name"
    cmp "$PLANNER_NATIVE_IPC_CORPUS/$name" "$historical/$name"
  done
  out=$proof/ipc-owned
  mkdir "$out" "$out/logs"
  fail() { echo "Planner native IPC: $*" >&2; exit 1; }
  library=$source_dir/tools/ci/cold_tracer_launch_560.sh
  library_before=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' -- "$library")
  exec {library_fd}<"$library"
  library_held=/proc/$BASHPID/fd/$library_fd
  [[ $(stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' -- "$library_held") == "$library_before" ]]
  printf '%s  %s\n' 1d5d0ab9d736e5cf24844a38ac6b35e9a02a0a279b7abb2d25e6fff82aff926a "$library_held" | sha256sum --check --strict -
  . "$library_held"
  [[ $(stamp "$library") == "$library_before" && $(stamp "$library_held") == "$library_before" ]]
  exec {library_fd}<&-
  declare -A held_fd=() held_id=() held_sha=() aliases=() alias_id=()
  active= identity=; groups_ok=true
  launch_label=
  extra_env=()
  # 현재 private venv/PYTHONPATH/SDK를 보존하며 token은 새 IPC 실행에 넘기지 않는다.
  unset GITHUB_TOKEN GH_TOKEN
  mapfile -d '' -t base_env < <(/usr/bin/env -0)
  deadline=$((SECONDS+ipc_seconds+45))
  ipc_group_ready=true
  trap 'exit 130' INT
  trap 'exit 143' TERM
  trap 'exit 129' HUP
  for path in "$controller" "$library" "${BASH_SOURCE[0]}" "$PLANNER_NATIVE_IPC_AUDIT" "$compiler"; do hold "$path"; done
  for name in audit.json capture.zip receipt.zip; do
    hold "$PLANNER_NATIVE_IPC_CORPUS/$name"; hold "$historical/$name"
  done
  [[ ${held_sha[$controller]} == "$(cut -d ' ' -f 1 "$proof/controller.sha256")" ]]
  evidence=$proof/ipc-native-scheduler.ndjson
  start_owned ipc-native "$((ipc_seconds+15))" "$controller" ipc --root "$source_dir" --python "$planner_python" \
    --binary "$source_dir/rust/target/debug/openpilot-plannerd" --artifact "$job/planner-artifact" \
    --binding "${PLANNER_PARAMS_BINDING:?}" --source-native "$job/planner-source" \
    --original-ipc-binding "${bindings[0]}" --scheduler "$controller" --compiler "$compiler" \
    --corpus "$PLANNER_NATIVE_IPC_CORPUS" --corpus-audit "$PLANNER_NATIVE_IPC_AUDIT" --corpus-audit-sha256 "$audit_sha" \
    --output "$job/planner-comparison/ipc-native" --evidence "$evidence" \
    --timeout-seconds "$ipc_seconds" --growth-mib 512 --peer-build-growth-mib 128
  group_quiet "$active" "$active" "$out/logs/ipc-native.group" || fail 'live or unknown IPC group member'
  complete_owned ipc-native
  [[ $owned_code == 0 && $owned_waited == 0 && $(wc -l < "$evidence") == 160 ]]
  hold "$evidence"; hold "$job/planner-comparison/ipc-native/receipt.json"
fi
