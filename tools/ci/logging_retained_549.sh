#!/usr/bin/env bash
# 원본 보존 후보 실행. 전체 lifecycle/closed-trace/caller/absence 퇴역 gate는 별도다.
set -euo pipefail
umask 077
# 무인자는 기존 retained recipe다. 새 mode 외의 인자는 fallback 없이 거부한다.
case $# in
  0) mode=retained ;;
  1) [[ $1 == legacy-runtime2 || $1 == source-guard ]] || { echo 'expected no arguments, legacy-runtime2 or source-guard' >&2; exit 64; }; mode=$1 ;;
  *) echo 'expected no arguments, legacy-runtime2 or source-guard' >&2; exit 64 ;;
esac
source_policy=${NATIVE_LOGGING_SOURCE_POLICY-present}
case $source_policy in present|absent) ;; *) echo 'NATIVE_LOGGING_SOURCE_POLICY must be present or absent' >&2; exit 64 ;; esac
source_policy_args=(); source_policy_env=()
if [[ $source_policy == absent ]]; then
  source_policy_args=(--source-policy absent)
  source_policy_env=(NATIVE_LOGGING_SOURCE_POLICY=absent)
fi
test "$GITHUB_ACTIONS" = true
test "$GITHUB_REPOSITORY" = bin9208/openpilot-rust
test "$GITHUB_EVENT_NAME" = push
test "$GITHUB_REF" = refs/heads/codex/feat-523-python-rust
test "$RUNNER_ENVIRONMENT:$RUNNER_OS:$RUNNER_ARCH" = github-hosted:Linux:X64
test "$(uname -m)" = x86_64
test "$PWD" = "$DIAG_SOURCE"
test "$GITHUB_WORKSPACE" = "$DIAG_SOURCE"
test "$(git --no-optional-locks rev-parse HEAD)" = "$GITHUB_SHA"
test "$(cat "$CI_JOB_TEMP/owner")" = "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB"
test "$CARGO_TARGET_DIR" = "$DIAG_SOURCE/rust/target"
test "$RUSTUP_HOME" = "$CI_JOB_TEMP/rustup-home"
export GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1
export CARGO_BUILD_JOBS=1
unset NATIVE_LOGGING_LIFECYCLE_RESUME NATIVE_LOGGING_TEST_BUILDER NATIVE_LOGGING_TEST_REFERENCE
source=$DIAG_SOURCE
proof=$CI_JOB_TEMP/proof/logging
[[ $mode != legacy-runtime2 ]] || proof=$CI_JOB_TEMP/proof/logging-legacy-runtime2
[[ $mode != source-guard ]] || proof=$CI_JOB_TEMP/proof/logging-source-absence
sdk=$RUSTUP_HOME/toolchains/1.94.0-x86_64-unknown-linux-gnu/bin
mkdir "$proof" "$proof/build" "$proof/trace" "$proof/bin" "$proof/lifecycle"
phase=admission
source_policy_baseline=$CI_JOB_TEMP/proof/logging-source-policy.parents.tsv
source_policy_metadata=$CI_JOB_TEMP/proof/logging-source-policy.json
source_policy_failure=$CI_JOB_TEMP/proof/logging-source-policy.failed
# 실제 absent phase의 checkpoint guard다. CLI 내부 SourceGuard의 NOFOLLOW/ENOENT 검사를 대신하지 않는다.
source_policy_ready=false
source_policy_paths=() source_policy_fds=() source_policy_ids=()
source_policy_observe() {
  local i path current nodes index_entries head_entries
  [[ $source_policy_ready == true && ! -e $source_policy_failure && ! -L $source_policy_failure ]] || return 1
  for i in "${!source_policy_paths[@]}"; do
    path=${source_policy_paths[i]}
    [[ -d $path && ! -L $path && $(realpath -e -- "$path") == "$path" ]] || return 1
    current=$(stat -c '%d:%i:%f:%u:%g' -- "$path") || return 1
    [[ $current == "${source_policy_ids[i]}" &&
       $(stat -Lc '%d:%i:%f:%u:%g' -- "/proc/$$/fd/${source_policy_fds[i]}") == "$current" ]] || return 1
  done
  # complete name scan은 모든 오류를 거부한다. dangling symlink도 이름이 있으면 재등장이다.
  nodes=$(/usr/bin/find -P "$source/rust/tools" -mindepth 1 -maxdepth 1 \
    \( -name check_native_logging.py -o -name native_logging_transport.py \) -printf '%f\n') || return 1
  [[ -z $nodes ]] || return 1
  index_entries=$(/usr/bin/git --no-optional-locks -C "$source" ls-files -- rust/tools/check_native_logging.py rust/tools/native_logging_transport.py) || return 1
  head_entries=$(/usr/bin/git --no-optional-locks -C "$source" ls-tree -r --name-only HEAD -- rust/tools/check_native_logging.py rust/tools/native_logging_transport.py) || return 1
  [[ -z $index_entries && -z $head_entries ]] || return 1
  for i in "${!source_policy_paths[@]}"; do
    [[ ! -L ${source_policy_paths[i]} &&
       $(stat -c '%d:%i:%f:%u:%g' -- "${source_policy_paths[i]}") == "${source_policy_ids[i]}" &&
       $(stat -Lc '%d:%i:%f:%u:%g' -- "/proc/$$/fd/${source_policy_fds[i]}") == "${source_policy_ids[i]}" ]] || return 1
  done
}
source_policy_checkpoint() {
  [[ $source_policy == absent ]] || return 0
  if ! source_policy_observe; then
    # 첫 관측 실패는 다른 checkpoint/프로세스가 성공해도 지우거나 덮어쓰지 않는다.
    (set -C; printf 'source_policy=absent\nsource_commit=%s\nphase=%s\nresult=FAIL\n' "$GITHUB_SHA" "${phase:-admission}" > "$source_policy_failure") 2>/dev/null || true
    printf 'logging source absence checkpoint failed\n' >&2
    return 1
  fi
}
source_policy_init() {
  local path fd identity rows= mode_value actual expected
  [[ $source_policy == absent ]] || return 0
  for path in "$source" "$source/rust" "$source/rust/tools"; do
    [[ -d $path && ! -L $path && $(realpath -e -- "$path") == "$path" && $(stat -c %u -- "$path") == "$UID" ]] || return 1
    mode_value=$(stat -c %a -- "$path") || return 1
    (( (8#$mode_value & 0022) == 0 )) || return 1
    exec {fd}< "$path" || return 1
    identity=$(stat -Lc '%d:%i:%f:%u:%g' -- "/proc/$$/fd/$fd") || return 1
    [[ $(stat -c '%d:%i:%f:%u:%g' -- "$path") == "$identity" ]] || return 1
    source_policy_paths+=("$path"); source_policy_fds+=("$fd"); source_policy_ids+=("$identity")
    printf -v rows '%s%s\t%s\n' "$rows" "$path" "$identity"
  done
  expected=$(printf '{"schema":1,"source_commit":"%s","source_policy":"absent","scope":"sampled source checkpoints; CLI owns NOFOLLOW/ENOENT checks","postdelete_full_acceptance":false,"R4_acceptance":false,"retirement_credit":0}' "$GITHUB_SHA")
  for path in "$source_policy_baseline" "$source_policy_metadata"; do
    if [[ -e $path || -L $path ]]; then
      [[ -f $path && ! -L $path && $(stat -c '%u:%h' -- "$path") == "$UID:1" && $(stat -c %s -- "$path") -le 4096 ]] || return 1
    fi
  done
  if [[ ! -e $source_policy_baseline ]]; then (set -C; printf '%s' "$rows" > "$source_policy_baseline") || return 1; fi
  if [[ ! -e $source_policy_metadata ]]; then (set -C; printf '%s\n' "$expected" > "$source_policy_metadata") || return 1; fi
  actual=$(cat -- "$source_policy_baseline") || return 1
  [[ "$actual" == "${rows%$'\n'}" && $(cat -- "$source_policy_metadata") == "$expected" ]] || return 1
  source_policy_ready=true
  source_policy_checkpoint
}
untraced_status=0
paired_active=
finish() {
  result=$?
  trap - EXIT
  set +e
  paired_final_result=0
  if [[ -n $paired_active ]]; then
    (set -e; paired_snapshot "$paired_active" failed-after) > "$proof/paired-554/$paired_active/final-snapshot.stdout" 2> "$proof/paired-554/$paired_active/final-snapshot.stderr"
    paired_final_result=$?
    printf '%s\n' "$paired_final_result" > "$proof/paired-554/$paired_active/final-snapshot.exit"
    if ((result == 0 && paired_final_result != 0)); then result=$paired_final_result; fi
  fi
  source_policy_checkpoint
  source_policy_result=$?
  if ((result == 0 && source_policy_result != 0)); then result=$source_policy_result; fi
  verify_held > "$proof/held-final.stdout" 2> "$proof/held-final.stderr"
  held_result=$?
  if ((result == 0 && held_result != 0)); then result=$held_result; fi
  if ((result == 0 && untraced_status != 0)); then result=$untraced_status; phase=lifecycle-untraced-failed; fi
  printf '{"source_policy":"%s","phase":"%s","exit_code":%s,"held_final_exit_code":%s,"untraced_exit_code":%s,"paired_final_exit_code":%s,"retirement_credit":0,"closed_trace_admitted":false,"old27_admitted":false}\n' "$source_policy" "$phase" "$result" "$held_result" "$untraced_status" "$paired_final_result" > "$proof/outcome.json"
  exit "$result"
}
trap finish EXIT
admit() {
  local label=$1 growth_mib=$2 available target_bytes=0 proof_bytes
  source_policy_checkpoint
  available=$(df -PB1 "$source" | awk 'NR==2 {print $4}')
  if test -d "$CARGO_TARGET_DIR"; then target_bytes=$(du -sx --block-size=1 "$CARGO_TARGET_DIR" | cut -f1); fi
  proof_bytes=$(du -sx --block-size=1 "$CI_JOB_TEMP/proof" | cut -f1)
  [[ $available =~ ^[0-9]+$ && $target_bytes =~ ^[0-9]+$ && $proof_bytes =~ ^[0-9]+$ ]]
  printf '%s\t%s\t%s\t%s\t%s\n' "$label" "$growth_mib" "$available" "$target_bytes" "$proof_bytes" >> "$proof/growth.tsv"
  ((available >= growth_mib * 1024 * 1024 + 512 * 1024 * 1024))
  ((target_bytes <= 8 * 1024 * 1024 * 1024 && proof_bytes <= 2 * 1024 * 1024 * 1024))
  if [[ $mode == source-guard ]]; then source_guard_resource "$label" 0 "$((growth_mib * 1024 * 1024))"; fi
}
run() {
  local label=$1 seconds=$2 command_status side started_cs ended_cs; shift 2
  phase=$label
  admit "$label" 512
  if [[ $label == lifecycle-runtime_directory_int || $label == git-preload-false-a1 || $label == git-preload-true-b || $label == git-preload-false-a2 ]]; then
    side=traced
    if [[ $label != lifecycle-runtime_directory_int ]]; then side=git-${label#git-preload-}; fi
    printf '%s\0' "$@" > "$proof/paired-554/$side/command.argv0"
    paired_active=$side
    paired_snapshot "$side" before
    if [[ $side == traced ]]; then seconds=$(paired_bound); fi
    printf '%s\n' "$seconds" > "$proof/paired-554/$side/bound-seconds.txt"
    if [[ $side != traced ]]; then
      started_cs=$(monotonic_cs)
      printf '%s\n' "$started_cs" > "$proof/paired-554/$side/start-cs.txt"
    fi
    # 함수 전체를 if로 호출하지 않는다. 자식의 실제 status를 admit보다 먼저 보존한다.
    set +e
    (set -e; ulimit -f 524288; /usr/bin/timeout --signal=TERM --kill-after=30s "${seconds}s" "$@") > "$proof/build/$label.stdout" 2> "$proof/build/$label.stderr"
    command_status=$?
    set -e
    printf '%s\n' "$command_status" > "$proof/paired-554/$side/command.exit"
    if [[ $side != traced ]]; then
      ended_cs=$(monotonic_cs)
      printf '%s\n' "$ended_cs" > "$proof/paired-554/$side/end-cs.txt"
      ((ended_cs >= started_cs))
      printf '{"start_cs":%s,"end_cs":%s,"elapsed_cs":%s,"resolution_seconds":0.01}\n' \
        "$started_cs" "$ended_cs" "$((ended_cs - started_cs))" > "$proof/paired-554/$side/elapsed.json"
    fi
    if ((command_status != 0)); then return "$command_status"; fi
    paired_snapshot "$side" after
    paired_active=
  else
    (ulimit -f 524288; /usr/bin/timeout --signal=TERM --kill-after=30s "${seconds}s" "$@") > "$proof/build/$label.stdout" 2> "$proof/build/$label.stderr"
  fi
  admit "$label-after" 0
}
# 실제 양성 report/binding에서 정책 인자 또는 필드 하나만 바꾼다. 기존 trace 음성4/3과 분리한다.
source_policy_controls() {
  local prefix=$1 binding=$2 report=$3 directory=$4
  mkdir -m 700 -- "$directory"
  run source-policy-schema-controls 60 "${runtime[@]}" /usr/bin/bash --noprofile --norc -euo pipefail -c '
    prefix=$1; binding=$2; report=$3; policy=$4; directory=$5; source_policy=$6
    count=0
    reject() {
      local name=$1 selected=$2 status=0; shift 2
      /usr/bin/jq -e --arg prefix "$prefix" --slurpfile binding "$selected" "$@" -f "$policy" "$report" \
        > "$directory/$name.stdout" 2> "$directory/$name.stderr" || status=$?
      printf "%s\n" "$status" > "$directory/$name.exit"
      test "$status" -eq 5 && test ! -s "$directory/$name.stdout"
      grep -F -- "source policy/scope binding differs" "$directory/$name.stderr"
      count=$((count+1))
    }
    for value in null false '\''""'\'' '\''"unknown"'\''; do
      reject "source-arg-$count" "$binding" --argjson source_policy "$value"
      reject "scope-arg-$count" "$binding" --arg source_policy "$source_policy" --argjson policy_scope "$value"
    done
    for mutation in '\''del(.source_policy)'\'' '\''.source_policy=null'\'' '\''.source_policy=false'\'' \
      '\''.source_policy=""'\'' '\''.source_policy="unknown"'\''; do
      /usr/bin/jq "$mutation" "$binding" > "$directory/binding-$count.json"
      reject "binding-$count" "$directory/binding-$count.json" --arg source_policy "$source_policy"
    done
    reject historical-current-binding "$binding" --arg source_policy "$source_policy" --arg policy_scope historical-788
    test "$count" -eq 14
    printf '\''{"schema":1,"controls":14,"each_source_policy_exact_error":true,"actual_raw_changed":false,"R4_acceptance":false,"retirement_credit":0}\n'\'' \
      > "$directory/complete.json"
  ' source-policy-schema-controls "$prefix" "$binding" "$report" "$source/tools/ci/logging_retained_exec_policy_549.jq" "$directory" "$source_policy"
  hold "$directory/complete.json"
}
declare -a held_paths=() held_fds=() held_ids=() held_hashes=()
hold() {
  local path fd identity hash
  path=$(realpath -e -- "$1")
  test -f "$path" && test ! -L "$path"
  exec {fd}< "$path"
  identity=$(stat -Lc '%d:%i:%u:%f:%h:%s:%y:%z' -- "/proc/$$/fd/$fd")
  test "$(stat -Lc '%d:%i:%u:%f:%h:%s:%y:%z' -- "$path")" = "$identity"
  hash=$(sha256sum -- "/proc/$$/fd/$fd" | cut -d' ' -f1)
  held_paths+=("$path"); held_fds+=("$fd"); held_ids+=("$identity"); held_hashes+=("$hash")
  printf '%s\t%s\t%s\t%s\n' "$fd" "$hash" "$identity" "$path" >> "$proof/held-before.tsv"
}
verify_held() {
  local i fd path
  for i in "${!held_paths[@]}"; do
    fd=${held_fds[i]}; path=${held_paths[i]}
    test "$(stat -Lc '%d:%i:%u:%f:%h:%s:%y:%z' -- "$path")" = "${held_ids[i]}" || return 1
    test "$(stat -Lc '%d:%i:%u:%f:%h:%s:%y:%z' -- "/proc/$$/fd/$fd")" = "${held_ids[i]}" || return 1
    test "$(sha256sum -- "/proc/$$/fd/$fd" | cut -d' ' -f1)" = "${held_hashes[i]}" || return 1
    test "$(sha256sum -- "$path" | cut -d' ' -f1)" = "${held_hashes[i]}" || return 1
  done
}
elf() {
  local name=$1 path=$2
  hold "$path"
  readelf -h "$path" > "$proof/$name.elf.txt"
  grep -Eq 'Class: +ELF64$' "$proof/$name.elf.txt"
  grep -Eq 'Machine: +Advanced Micro Devices X86-64$' "$proof/$name.elf.txt"
  ldd "$path" > "$proof/$name.ldd.txt"
  ! grep -q 'not found' "$proof/$name.ldd.txt"
  while IFS= read -r library; do hold "$library"; done < <(awk '$2=="=>" && $3 ~ /^\// {print $3} $1 ~ /^\// {print $1}' "$proof/$name.ldd.txt" | sort -u)
}
artifact() {
  local label=$1 name=$2 kind=$3 source_path=$4 test_profile=$5
  jq -ers --arg name "$name" --arg kind "$kind" --arg src "$source_path" --argjson is_test "$test_profile" '
    if (.[-1].reason != "build-finished" or .[-1].success != true) then error("missing successful Cargo EOF") else . end |
    [.[] | select(.reason=="compiler-artifact" and .target.name==$name and (.target.kind|index($kind))!=null and .target.src_path==$src and .profile.test==$is_test and .executable!=null) | .executable] |
    if length==1 and (.[0]|startswith("/")) then .[0] else error("ambiguous Cargo executable") end' "$proof/build/$label.stdout"
}
# #549: 전회 성공 provider/JSONL의 cross-call seal. 이전 FD가 계속 열려 있다고 주장하지 않는다.
legacy_file() {
  local path=$1 limit=${2:-1048576} file_mode
  [[ $path == /* && $(realpath -e -- "$path") == "$path" && -f $path && ! -L $path ]]
  [[ $(stat -c '%u:%h' -- "$path") == "$UID:1" ]]
  file_mode=$(stat -c %a -- "$path")
  (( (8#$file_mode & 0022) == 0 ))
  (( $(stat -c %s -- "$path") <= limit ))
  hold "$path"
}
legacy_identity() { stat -Lc '%d:%i:%u:%f:%h:%s:%y:%z' -- "$1"; }
legacy_role() {
  local role=$1 selected=$2 actual=$3 log=$4 name=$5 kind=$6 src=$7 tested=$8
  local n=-1 i
  for i in "${!held_paths[@]}"; do [[ ${held_paths[i]} != "$actual" ]] || n=$i; done
  ((n >= 0))
  jq -cn --arg role "$role" --arg selected "$selected" --arg path "$actual" \
    --arg identity "${held_ids[n]}" --arg sha "${held_hashes[n]}" --arg log "$log" \
    --arg selected_identity "$(legacy_identity "$selected")" --arg log_sha "$(sha256sum "$log" | cut -d' ' -f1)" \
    --arg name "$name" --arg kind "$kind" --arg src "$src" --argjson tested "$tested" \
    '{role:$role,selected:$selected,path:$path,identity:$identity,sha256:$sha,selected_identity:$selected_identity,
      log:$log,log_sha256:$log_sha,name:$name,kind:$kind,src:$src,tested:$tested}'
}
legacy_publish_reuse() {
  local reuse=$proof/legacy-reuse.json rows=$proof/legacy-reuse.roles.jsonl log
  phase=legacy-reuse-publication
  for log in checker-build probe-build fixture-build; do legacy_file "$proof/build/$log.stdout" 536870912; done
  hold "$probe_built"
  legacy_role checker "$checker" "$checker" "$proof/build/checker-build.stdout" check-native-logging bin \
    "$source/rust/crates/dev-tools/src/bin/check_native_logging.rs" false > "$rows"
  legacy_role probe "$probe_built" "$probe" "$proof/build/probe-build.stdout" native_logging_probe example \
    "$source/rust/crates/logging/examples/native_logging_probe.rs" false >> "$rows"
  legacy_role fixture "$fixture" "$fixture" "$proof/build/fixture-build.stdout" native_logging_lifecycle test \
    "$source/rust/crates/dev-tools/tests/native_logging_lifecycle.rs" true >> "$rows"
  verify_held
  (set -C; cat "$proof/held-before.tsv" > "$proof/legacy-reuse.held.tsv")
  legacy_file "$proof/legacy-reuse.held.tsv"
  (set -C; jq -n --arg source "$source" --arg sha "$GITHUB_SHA" --arg prior "$proof" \
    --arg owner "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" \
    --arg source_policy "$source_policy" --arg root_identity "$(stat -c '%d:%i:%f:%u:%g' "$source")" \
    --arg proof_identity "$(stat -c '%d:%i:%f:%u:%g' "$proof")" --arg held_roster "$proof/legacy-reuse.held.tsv" \
    --arg held_sha "$(sha256sum "$proof/legacy-reuse.held.tsv" | cut -d' ' -f1)" --slurpfile roles "$rows" \
    '{schema:1,mode:"retained",source_policy:$source_policy,source:$source,source_commit:$sha,prior:$prior,owner:$owner,
      root_identity:$root_identity,proof_identity:$proof_identity,roles:$roles,held_roster:$held_roster,held_roster_sha256:$held_sha,
      prior_fd_continuity_claimed:false,historical_reuse:false,R4_acceptance:false,retirement_credit:0}' > "$reuse")
  legacy_file "$reuse"
  phase=candidate-observed
}
legacy_tree() {
  local root=$1 stem=$2
  LC_ALL=C find -P "$root" -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%y\t%p\t%l\0' | LC_ALL=C sort -z > "$stem.roster"
  LC_ALL=C find -P "$root" -type f -print0 | LC_ALL=C sort -z | xargs -0 -r sha256sum > "$stem.sha256"
  LC_ALL=C find -P "$root" -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%y\t%p\t%l\0' | LC_ALL=C sort -z > "$stem.stable"
  cmp "$stem.roster" "$stem.stable"
}
legacy_seal() {
  local moment=$1
  [[ $(stat -c '%d:%i:%f:%u:%g' "$source") == "$legacy_source_identity" ]]
  [[ $(stat -c '%d:%i:%f:%u:%g' "$DIAG_SOURCE_OWNER") == "$DIAG_SOURCE_IDENTITY_FULL" ]]
  test "$(cat "$DIAG_SOURCE_OWNER/.diag296-owner")" = "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB"
  git --no-optional-locks diff --exit-code > "$proof/source-$moment.diff"
  git --no-optional-locks diff --cached --exit-code > "$proof/index-$moment.diff"
  test "$(git --no-optional-locks rev-parse HEAD)" = "$GITHUB_SHA"
  git --no-optional-locks ls-files -z > "$proof/index-$moment.bin"
  cmp "$CI_JOB_TEMP/proof/index-before.bin" "$proof/index-$moment.bin"
  tar --null --no-recursion --verbatim-files-from -T "$CI_JOB_TEMP/proof/index-before.bin" -cf - \
    --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner | sha256sum > "$proof/source-$moment.sha256"
  cmp "$CI_JOB_TEMP/proof/source-before.sha256" "$proof/source-$moment.sha256"
}
legacy_sdk_seal() {
  local moment=$1 sdk_root=${sdk%/bin}
  [[ $CI_PREPARED_RUST_ROOT == "$sdk_root" && $CI_PREPARED_RUST_MANIFEST == "$CI_JOB_TEMP/proof/prepared-rust-files.before.sha256" ]]
  sha256sum --check --strict --quiet "$CI_JOB_TEMP/proof/prepared-rust-programs.sha256"
  sha256sum --check --strict --quiet "$CI_PREPARED_RUST_MANIFEST"
  LC_ALL=C find "$sdk_root" -printf '%D:%i\t%m:%U:%G\t%T@\t%C@\t%y\t%p\t%l\0' | LC_ALL=C sort -z > "$proof/sdk-$moment.roster.bin"
  cmp "$CI_JOB_TEMP/proof/prepared-rust-roster.before.bin" "$proof/sdk-$moment.roster.bin"
  if [[ $moment == after && $legacy_headers_ready == true ]]; then
    legacy_tree /usr/include "$proof/headers-after"
    cmp "$proof/headers-before.roster" "$proof/headers-after.roster"
    cmp "$proof/headers-before.sha256" "$proof/headers-after.sha256"
    legacy_tree "$legacy_gcc_root" "$proof/gcc-after"
    cmp "$proof/gcc-before.roster" "$proof/gcc-after.roster"
    cmp "$proof/gcc-before.sha256" "$proof/gcc-after.sha256"
  fi
}
legacy_finish() {
  local result=$? sealed=0 source_result sdk_result held_result resource_result alias_result=0 i
  trap - EXIT
  set +e
  (set -e; ulimit -f 524288; legacy_seal after) > "$proof/source-final.stdout" 2> "$proof/source-final.stderr"
  source_result=$?
  (set -e; ulimit -f 524288; legacy_sdk_seal after) > "$proof/sdk-final.stdout" 2> "$proof/sdk-final.stderr"
  sdk_result=$?
  if ((source_result != 0 || sdk_result != 0)); then sealed=1; fi
  verify_held > "$proof/held-final.stdout" 2> "$proof/held-final.stderr"
  held_result=$?
  for i in "${!legacy_alias_paths[@]}"; do
    [[ $(readlink -e "${legacy_alias_paths[i]}") == "${legacy_alias_targets[i]}" && \
      $(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "${legacy_alias_paths[i]}") == "${legacy_alias_ids[i]}" ]] || alias_result=1
  done
  (set -e; admit legacy-runtime2-exit 0) > "$proof/resource-final.stdout" 2> "$proof/resource-final.stderr"
  resource_result=$?
  if ((result == 0 && (sealed != 0 || held_result != 0 || alias_result != 0 || resource_result != 0))); then result=1; fi
  printf '{"schema":1,"mode":"legacy-runtime2","source_policy":"%s","phase":"%s","exit_code":%s,"source_sdk_final_exit_code":%s,"source_final_exit_code":%s,"sdk_final_exit_code":%s,"held_final_exit_code":%s,"provider_alias_final_exit_code":%s,"resource_final_exit_code":%s,"old27_admitted":false,"R4_acceptance":false,"retirement_credit":0}\n' \
    "$source_policy" "$phase" "$result" "$sealed" "$source_result" "$sdk_result" "$held_result" "$alias_result" "$resource_result" > "$proof/outcome.json"
  exit "$result"
}
legacy_alias() {
  local path=$1
  legacy_alias_paths+=("$path"); legacy_alias_targets+=("$(readlink -e "$path")")
  legacy_alias_ids+=("$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path")")
  elf "provider-${#legacy_alias_paths[@]}" "$path"
}
reuse_retained() {
  local prior=$CI_JOB_TEMP/proof/logging receipt log role selected actual identity hash row path prior_fd
  local name kind src tested
  phase=legacy-reuse-admission
  export PATH="$sdk:/usr/bin:/bin"
  test "$CARGO_HOME" = "$CI_JOB_TEMP/cargo-home"
  [[ $CI_JOB_TEMP == /tmp/openpilot-ci.?????? && ${CI_JOB_TEMP%/*} == /tmp ]]
  [[ $DIAG_SOURCE_OWNER == "$DIAG_ACTIONS_WORKSPACE/.logging549-source-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}."?????? ]]
  for path in "$CI_JOB_TEMP" "$CI_JOB_TEMP/proof" "$DIAG_SOURCE_OWNER" "$source" "$prior"; do
    [[ -d $path && ! -L $path && $(realpath -e "$path") == "$path" && $(stat -c '%u:%a' "$path") == "$UID:700" ]]
  done
  legacy_source_identity=$(stat -c '%d:%i:%f:%u:%g' "$source")
  DIAG_SOURCE_IDENTITY_FULL=$(stat -c '%d:%i:%f:%u:%g' "$DIAG_SOURCE_OWNER")
  [[ $(stat -c '%d:%i' "$DIAG_SOURCE_OWNER") == "$DIAG_SOURCE_IDENTITY" ]]
  [[ $source == "$DIAG_SOURCE_OWNER/source" && $(realpath -e "$source") == "$source" && ! -L $source ]]
  [[ $(stat -c '%u:%a' "$source") == "$UID:700" && $(stat -c '%u:%a' "$proof") == "$UID:700" ]]
  legacy_headers_ready=false
  declare -ga legacy_alias_paths=() legacy_alias_targets=() legacy_alias_ids=()
  if [[ $mode == source-guard ]]; then trap source_guard_finish EXIT; else trap legacy_finish EXIT; fi
  unset GH_TOKEN GITHUB_TOKEN
  runtime=(/usr/bin/env -i PATH=/usr/bin:/bin HOME="$HOME" TMPDIR="$TMPDIR" LANG=C.UTF-8 LC_ALL=C.UTF-8 TZ=UTC \
    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
    GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.preloadIndex GIT_CONFIG_VALUE_0=true \
    GITHUB_ACTIONS=true RUNNER_ENVIRONMENT=github-hosted RUNNER_OS=Linux RUNNER_ARCH=X64 GITHUB_REPOSITORY=bin9208/openpilot-rust)
  receipt=$prior/legacy-reuse.json
  for path in "$receipt" "$prior/outcome.json" "$prior/legacy-reuse.held.tsv" \
    "$CI_JOB_TEMP/proof/index-before.bin" "$CI_JOB_TEMP/proof/source-before.sha256" \
    "$CI_PREPARED_RUST_MANIFEST" "$CI_JOB_TEMP/proof/prepared-rust-programs.sha256" "$CI_JOB_TEMP/proof/prepared-rust-roster.before.bin"; do
    legacy_file "$path" 33554432
  done
  jq -e --arg source_policy "$source_policy" '.source_policy==$source_policy and .phase=="candidate-observed" and .exit_code==0 and .held_final_exit_code==0 and .untraced_exit_code==0 and .paired_final_exit_code==0' "$prior/outcome.json" > "$proof/prior-success.txt"
  jq -e --arg source_policy "$source_policy" --arg source "$source" --arg sha "$GITHUB_SHA" --arg prior "$prior" \
    --arg owner "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" --arg root_identity "$legacy_source_identity" \
    --arg proof_identity "$(stat -c '%d:%i:%f:%u:%g' "$prior")" \
    --arg held_roster "$prior/legacy-reuse.held.tsv" --arg held_sha "$(sha256sum "$prior/legacy-reuse.held.tsv" | cut -d' ' -f1)" '
    .schema==1 and .mode=="retained" and .source_policy==$source_policy and .source==$source and .source_commit==$sha and .prior==$prior and
    .owner==$owner and .root_identity==$root_identity and .proof_identity==$proof_identity and
    .held_roster==$held_roster and .held_roster_sha256==$held_sha and
    .prior_fd_continuity_claimed==false and .historical_reuse==false and .R4_acceptance==false and .retirement_credit==0 and
    ([.roles[].role]==["checker","probe","fixture"]) and (.roles|length)==3
    ' "$receipt" > "$proof/reuse-admission.txt"
  # 모든 전회 held provider/source를 원래 identity/hash와 대조한 뒤 새 FD로 hold한다.
  test -s "$prior/legacy-reuse.held.tsv"
  while IFS=$'\t' read -r prior_fd hash identity path; do
    [[ $prior_fd =~ ^[0-9]+$ && $hash =~ ^[0-9a-f]{64}$ && $path == /* && $(realpath -e "$path") == "$path" && -f $path && ! -L $path ]]
    [[ $(legacy_identity "$path") == "$identity" && $(sha256sum "$path" | cut -d' ' -f1) == "$hash" ]]
    hold "$path"
  done < "$prior/legacy-reuse.held.tsv"
  legacy_seal before
  legacy_sdk_seal before
  for role in checker probe fixture; do
    case $role in
      checker) log=checker-build; name=check-native-logging; kind=bin; src=$source/rust/crates/dev-tools/src/bin/check_native_logging.rs; tested=false ;;
      probe) log=probe-build; name=native_logging_probe; kind=example; src=$source/rust/crates/logging/examples/native_logging_probe.rs; tested=false ;;
      fixture) log=fixture-build; name=native_logging_lifecycle; kind=test; src=$source/rust/crates/dev-tools/tests/native_logging_lifecycle.rs; tested=true ;;
    esac
    legacy_file "$prior/build/$log.stdout" 536870912
    selected=$(jq -ers --arg name "$name" --arg kind "$kind" --arg src "$src" --argjson tested "$tested" '
      if .[-1].reason!="build-finished" or .[-1].success!=true then error("prior Cargo EOF") else . end |
      [.[]|select(.reason=="compiler-artifact" and .target.name==$name and .target.kind==[$kind] and .target.src_path==$src and .profile.test==$tested and .executable!=null)] |
      if length==1 and (.[0].fresh|type)=="boolean" then .[0].executable else error("prior Cargo target") end
      ' "$prior/build/$log.stdout")
    actual=$selected; [[ $role != probe ]] || actual=$prior/bin/native_logging_probe
    [[ $selected == "$CARGO_TARGET_DIR/"* && $(realpath -e "$selected") == "$selected" && -x $selected && ! -L $selected ]]
    [[ $(sha256sum "$selected" | cut -d' ' -f1) == "$(sha256sum "$actual" | cut -d' ' -f1)" ]]
    row=$(jq -ce --arg role "$role" '.roles[]|select(.role==$role)' "$receipt")
    jq -e --arg selected "$selected" --arg actual "$actual" --arg log "$prior/build/$log.stdout" \
      --arg identity "$(legacy_identity "$actual")" --arg selected_identity "$(legacy_identity "$selected")" \
      --arg hash "$(sha256sum "$actual" | cut -d' ' -f1)" --arg log_sha "$(sha256sum "$prior/build/$log.stdout" | cut -d' ' -f1)" \
      --arg name "$name" --arg kind "$kind" --arg src "$src" --argjson tested "$tested" '
      .selected==$selected and .path==$actual and .log==$log and .identity==$identity and .selected_identity==$selected_identity and
      .sha256==$hash and .log_sha256==$log_sha and .name==$name and .kind==$kind and .src==$src and .tested==$tested
      ' <<< "$row" > "$proof/reuse-$role.txt"
    elf "reused-$role" "$actual"
    case $role in checker) checker=$actual;; probe) probe=$actual;; fixture) fixture=$actual;; esac
  done
}
legacy_runtime2() {
  local prior=$CI_JOB_TEMP/proof/logging receipt log role selected actual identity hash row path prior_fd
  local name kind src tested n i cxx queried reference json11_prefix selector started ended
  local signal label expected_exit remaining command_started command_ended case_root
  reuse_retained
  # cached Cargo target을 fresh로 꾸미지 않는다. 새 builder target만 빈 namespace에 만든다.
  # 기존 admit의 단일 du가 nested builder target까지 포함하므로 8GiB를 중복/누락 없이 검사한다.
  for path in rust/crates/dev-tools/src/native_logging_build.rs rust/crates/dev-tools/src/json11_stage.rs uv.lock \
    openpilot/common/swaglog.cc openpilot/common/swaglog.h openpilot/common/timing.h rust/tools/native_logging_reference.cc; do hold "$source/$path"; done
  legacy_builder_target=$CARGO_TARGET_DIR/legacy-runtime2-builder
  [[ ! -e $legacy_builder_target && ! -L $legacy_builder_target ]]
  mkdir "$legacy_builder_target"
  admit legacy-builder-cold 8192
  run legacy-builder-build 1800 /usr/bin/env -u GH_TOKEN -u GITHUB_TOKEN CARGO_TARGET_DIR="$legacy_builder_target" \
    RUSTC="$sdk/rustc" RUSTDOC="$sdk/rustdoc" "$sdk/cargo" build --manifest-path "$source/rust/Cargo.toml" \
    -p openpilot-dev-tools --locked --no-default-features --features native-logging-check-native --bin build-native-logging-reference --message-format=json
  builder=$(artifact legacy-builder-build build-native-logging-reference bin "$source/rust/crates/dev-tools/src/bin/build_native_logging_reference.rs" false)
  [[ $builder == "$legacy_builder_target/debug/build-native-logging-reference" && $(realpath -e "$builder") == "$builder" && ! -L $builder ]]
  jq -se --arg path "$builder" '[.[]|select(.reason=="compiler-artifact" and .executable==$path)]|length==1 and .[0].fresh==false' \
    "$proof/build/legacy-builder-build.stdout" > "$proof/builder-fresh.txt"
  elf builder "$builder"
  for path in "$source/tools/ci/logging_retained_549.sh" "$source/.github/workflows/python-rust-523-logging.yml"; do hold "$path"; done
  for path in /usr/bin/jq /usr/bin/git /usr/bin/strace /usr/bin/timeout; do elf "tool-$(basename "$path")" "$path"; done
  for path in /usr/bin/bash /usr/bin/awk /usr/bin/c++ /usr/bin/curl /usr/bin/as /usr/bin/ld; do legacy_alias "$path"; done
  cxx=$(readlink -e /usr/bin/c++)
  for name in cc1plus collect2 as ld; do
    run "compiler-$name" 30 "${runtime[@]}" "$cxx" "-print-prog-name=$name"
    queried=$(cat "$proof/build/compiler-$name.stdout")
    [[ $queried == /* ]] || queried=$(PATH=/usr/bin:/bin command -v "$queried")
    [[ $queried == /* && -f $queried ]]
    legacy_alias "$queried"
    [[ $name != cc1plus ]] || legacy_gcc_root=$(dirname "$(readlink -e "$queried")")
  done
  for name in Scrt1.o crti.o crtn.o crtbeginS.o crtendS.o libstdc++.so libgcc.a libgcc_s.so libc.so libc_nonshared.a libpthread.a; do
    run "link-$name" 30 "${runtime[@]}" "$cxx" "-print-file-name=$name"
    queried=$(cat "$proof/build/link-$name.stdout")
    [[ $queried == /* && -f $queried ]]
    hold "$queried"
  done
  legacy_alias /usr/lib/x86_64-linux-gnu/libzmq.so.5
  run compiler-version 30 "${runtime[@]}" "$cxx" --version
  run compiler-search 30 "${runtime[@]}" "$cxx" -print-search-dirs
  legacy_alias /usr/bin/pkg-config
  run zmq-pkg-config 30 "${runtime[@]}" /usr/bin/pkg-config --cflags --libs libzmq
  phase=legacy-provider-snapshot
  legacy_tree /usr/include "$proof/headers-before"
  legacy_tree "$legacy_gcc_root" "$proof/gcc-before"
  legacy_headers_ready=true
  reference=$proof/reference-build/native_logging_reference
  run reference-build 660 "${runtime[@]}" /usr/bin/strace --seccomp-bpf --kill-on-exit -ff -ttt -x -v -s 65536 \
    -e trace=process,%file,%desc,%signal,%network,%memory,setsid,setpgid,prctl,unshare,setns -e raw=read,pread64 \
    -o "$proof/trace/reference-build" "$builder" --root "$source" --output "$proof/reference-build" --zmq-include /usr/include --architecture x86_64
  printf '%s\n' "$reference" > "$proof/reference-path.expected"
  cmp "$proof/reference-path.expected" "$proof/build/reference-build.stdout"
  elf fresh-reference "$reference"
  [[ $(stat -c %s "$proof/reference-build/comma_deps_json11-20170411.0.post103-py3-none-manylinux_2_28_x86_64.whl") == 42604 ]]
  printf '%s  %s\n' a309d401dd22b758455dafd3c28b72c4bd88c7482ddd70fe8653e53efc745a83 \
    "$proof/reference-build/comma_deps_json11-20170411.0.post103-py3-none-manylinux_2_28_x86_64.whl" | sha256sum --check --strict - > "$proof/reference-wheel.check"
  find -P "$proof/reference-build/json11" -mindepth 1 -maxdepth 1 -type d -name '*.data' -print0 > "$proof/reference-prefixes.bin"
  mapfile -d '' -t prefixes < "$proof/reference-prefixes.bin"
  [[ ${#prefixes[@]} == 1 ]]
  json11_prefix=${prefixes[0]}/purelib/json11/install
  jq -n --arg source "$source" --arg output "$proof/reference-build" --arg json11 "$json11_prefix" '
    ["c++","-std=c++17","-O2","-pthread","-I"+$output+"/overlay","-I"+$source+"/openpilot",
      "-I"+$json11+"/include","-I/usr/include",$source+"/rust/tools/native_logging_reference.cc",
      $source+"/openpilot/common/swaglog.cc",$json11+"/lib/libjson11.a","-l:libzmq.so.5",
      "-Wl,--wrap=zmq_send","-Wl,--wrap=zmq_setsockopt","-o",$output+"/native_logging_reference"]' > "$proof/reference-command.expected.json"
  jq -e --slurpfile command "$proof/reference-command.expected.json" '
    .command==$command[0] and .json11_version=="20170411.0.post103" and .json11_architecture=="x86_64" and
    .json11_sha256=="a309d401dd22b758455dafd3c28b72c4bd88c7482ddd70fe8653e53efc745a83"
    ' "$proof/reference-build/provenance.json" > "$proof/reference-provenance.check"
  jq -e '.sources|keys==["openpilot/common/swaglog.cc","openpilot/common/swaglog.h","openpilot/common/timing.h","rust/tools/native_logging_reference.cc"]' \
    "$proof/reference-build/provenance.json" > "$proof/reference-sources-roster.check"
  for path in openpilot/common/swaglog.cc openpilot/common/swaglog.h openpilot/common/timing.h rust/tools/native_logging_reference.cc; do
    hold "$source/$path"
    jq -e --arg path "$path" --arg sha "$(sha256sum "$source/$path" | cut -d' ' -f1)" '.sources[$path]==$sha' \
      "$proof/reference-build/provenance.json" > "$proof/source-$(basename "$path").check"
  done
  legacy_tree "$proof/reference-build" "$proof/reference-before"
  selector=actual_cli_int_term_reap_held_producer_and_preserve_runtime_foreign_replacement
  legacy_env=(NATIVE_LOGGING_TEST_BINARY="$probe" NATIVE_LOGGING_TEST_BUILDER="$builder" NATIVE_LOGGING_TEST_REFERENCE="$reference"
    NATIVE_LOGGING_LIFECYCLE_EVIDENCE="$proof/lifecycle" NATIVE_LOGGING_LIFECYCLE_RESUME=39833849895 "${source_policy_env[@]}")
  read -r started _ < /proc/uptime
  for signal in INT TERM; do
    label=legacy-runtime2-${signal,,}
    expected_exit=130; [[ $signal != TERM ]] || expected_exit=143
    find -P "$proof/lifecycle" -mindepth 1 -maxdepth 1 -print0 | LC_ALL=C sort -z > "$proof/$label.roots.before.bin"
    printf '%s\0' "${runtime[@]}" "${legacy_env[@]}" "NATIVE_LOGGING_LIFECYCLE_SIGNAL=$signal" \
      "$fixture" --ignored --exact "$selector" --test-threads=1 --color=never > "$proof/fixture-command-${signal,,}.argv0"
    read -r command_started _ < /proc/uptime
    remaining=$(/usr/bin/awk -v start="$started" -v now="$command_started" 'BEGIN {
      if (start!~/^[0-9]+[.][0-9]+$/ || now!~/^[0-9]+[.][0-9]+$/ || now<start || now-start>=600) exit 1;
      printf "%.2f",600-(now-start)
    }')
    # run의 admission 뒤 자식 시작 시점에 다시 잔여 시간을 계산한다. 두 case에600초씩 주지 않는다.
    run "$label" "$remaining" "${runtime[@]}" "${legacy_env[@]}" "NATIVE_LOGGING_LIFECYCLE_SIGNAL=$signal" \
      /usr/bin/bash --noprofile --norc -euo pipefail -c '
        read -r now _ < /proc/uptime
        remaining=$(/usr/bin/awk -v start="$1" -v now="$now" '\''BEGIN {
          if (start!~/^[0-9]+[.][0-9]+$/ || now!~/^[0-9]+[.][0-9]+$/ || now<start || now-start>=600) exit 1;
          printf "%.2f",600-(now-start)
        }'\'')
        printf "%s\n" "$remaining" > "$2"
        shift 2
        exec /usr/bin/timeout --signal=TERM --kill-after=30s "${remaining}s" "$@"
      ' legacy-runtime2-deadline "$started" "$proof/$label.bound-seconds.txt" \
      /usr/bin/strace --seccomp-bpf --kill-on-exit -ff -ttt -x -v -s 65536 \
      -e trace=process,%file,%desc,%signal,%network,%memory,setsid,setpgid,prctl,unshare,setns -e raw=read,pread64 \
      -o "$proof/trace/$label" "$fixture" --ignored --exact "$selector" --test-threads=1 --color=never
    read -r command_ended _ < /proc/uptime
    [[ $(grep -Ec '^test result: ok\. 1 passed; 0 failed; 0 ignored;' "$proof/build/$label.stdout") == 1 ]]
    [[ $(grep -Fxc "test $selector ... ok" "$proof/build/$label.stdout") == 1 ]]
    find -P "$proof/lifecycle" -mindepth 1 -maxdepth 1 -print0 | LC_ALL=C sort -z > "$proof/$label.roots.after.bin"
    LC_ALL=C comm -z -13 "$proof/$label.roots.before.bin" "$proof/$label.roots.after.bin" > "$proof/$label.new-root.bin"
    mapfile -d '' -t case_roots < "$proof/$label.new-root.bin"
    [[ ${#case_roots[@]} == 1 ]]
    case_root=${case_roots[0]}
    [[ $case_root == "$proof/lifecycle/logging-lifecycle-"?????? && ${case_root##*/} =~ ^logging-lifecycle-[A-Za-z0-9]{6}$ && \
      -d $case_root && ! -L $case_root && $(realpath -e "$case_root") == "$case_root" && $(stat -c '%u:%a' "$case_root") == "$UID:700" ]]
    legacy_file "$case_root/case.json"
    jq -e --argjson exit "$expected_exit" '.mode=="runtime" and .replacement=="directory" and .returncode==$exit and
      .held.endpoint=="runtime" and .actual_producer_reaped==true and .owned_directory_removed==true' \
      "$case_root/case.json" > "$proof/$label.case.check"
    jq -cn --arg source "$GITHUB_SHA" --arg label "$label" --arg signal "$signal" --arg selector "$selector" \
      --arg root "$case_root" --argjson exit "$expected_exit" --argjson start "$command_started" --argjson end "$command_ended" \
      '{source_commit:$source,label:$label,signal:$signal,selector:$selector,case_root:$root,returncode:$exit,
        libtest_passed:1,logical_cases:1,start_seconds:$start,end_seconds:$end}' >> "$proof/commands.jsonl"
  done
  read -r ended _ < /proc/uptime
  awk -v start="$started" -v end="$ended" 'BEGIN {exit !(start~/^[0-9]+[.][0-9]+$/ && end~/^[0-9]+[.][0-9]+$/ && end>=start && end-start<=600)}'
  printf '%s\t%s\n' "$started" "$ended" > "$proof/runtime2-monotonic.tsv"
  jq -se --arg source "$GITHUB_SHA" 'length==2 and ([.[].signal]==["INT","TERM"]) and
    ([.[].label]==["legacy-runtime2-int","legacy-runtime2-term"]) and ([.[].case_root]|unique|length)==2 and
    all(.[]; .source_commit==$source and .libtest_passed==1 and .logical_cases==1)' "$proof/commands.jsonl" > "$proof/commands.check"
  find -P "$proof/lifecycle" -mindepth 1 -maxdepth 1 -print0 > "$proof/case-roots.bin"
  mapfile -d '' -t case_roots < "$proof/case-roots.bin"
  [[ ${#case_roots[@]} == 2 ]]
  for path in "${case_roots[@]}"; do
    [[ $path == "$proof/lifecycle/logging-lifecycle-"?????? && ${path##*/} =~ ^logging-lifecycle-[A-Za-z0-9]{6}$ && \
      $(realpath -e "$path") == "$path" && -d $path && ! -L $path ]]
    [[ $(stat -c '%u:%a' "$path") == "$UID:700" ]]
    legacy_file "$path/case.json"
    jq -e --arg root "$path" '.held.output==($root+"/output/transport/runtime-prefix") and
      (.held.pid|type)=="number" and .held.pid>1 and (.held.pid|floor)==.held.pid' "$path/case.json" > "$path/held-binding.check"
  done
  jq -se 'length==2 and all(.[]; .mode=="runtime" and .replacement=="directory" and .held.endpoint=="runtime" and
    .actual_producer_reaped==true and .owned_directory_removed==true and .shutdown_seconds>=0) and
    ([.[].returncode]|sort)==[130,143] and ([.[].held.pid]|unique|length)==2' \
    "${case_roots[0]}/case.json" "${case_roots[1]}/case.json" > "$proof/cases.check"
  legacy_tree "$proof/reference-build" "$proof/reference-after"
  cmp "$proof/reference-before.roster" "$proof/reference-after.roster"
  cmp "$proof/reference-before.sha256" "$proof/reference-after.sha256"
  mkdir "$proof/inspect"
  for name in reference-build legacy-runtime2-int legacy-runtime2-term; do
    run "inspect-$name" 180 "${runtime[@]}" "$checker" inspect-trace-summary --directory "$proof/trace" --prefix "$name" --output "$proof/inspect/$name.json"
    jq -e '.schema==2 and .raw_immutable==true and .lexically_checked_all_events==true and .requires_exit_status_zero==true and
      .full_event_index==false and .closed_pid==false and .closed_cwd==false and .closed_fd==false and .closed_dso==false and
      .full_acceptance==false and .retirement_credit==false' "$proof/inspect/$name.json" > "$proof/inspect/$name.check"
    hold "$proof/inspect/$name.json"
  done
  verify_held
  admit legacy-runtime2-final 0
  phase=legacy-runtime2-observed
  jq -n --arg source_policy "$source_policy" --arg source "$GITHUB_SHA" --arg selector "$selector" --arg reference "$reference" \
    --arg reference_sha "$(sha256sum "$reference" | cut -d' ' -f1)" \
    '{schema:1,mode:"legacy-runtime2",source_policy:$source_policy,source_commit:$source,selector:$selector,logical_cases:2,logical_commands:2,libtest_passed:2,
      signal_order:["INT","TERM"],trace_prefixes:["legacy-runtime2-int","legacy-runtime2-term"],aggregate_limit_seconds:600,
      resume:"39833849895",fresh_reference:$reference,fresh_reference_sha256:$reference_sha,
      prior_fd_continuity_claimed:false,old17_reexecuted:false,collision8_reexecuted:false,retained14_reexecuted:false,
      closed_pid:false,closed_cwd:false,closed_fd:false,closed_dso:false,old27_admitted:false,R4_acceptance:false,retirement_credit:0}' > "$proof/complete.json"
}
# source-guard만의 독립 copy/CLI20 통제. 실제 source/present 기능은 변경하지 않는다.
source_guard_resource() {
  local LC_ALL=C
  local label=$1 planned=$2 proof_growth=${3:-0} actual=0 origin free path device identity required fs_pass
  local copy=$CI_JOB_TEMP/logging-source-absence-copy copy_identity=absent copy_device proof_device
  local source_cap=2147483648 copy_cap=2147483648 reserve=536870912 source_pass copy_pass old_pass result=0
  local source_identity job_identity proof_identity seen=: record row diagnostic record_size
  local -a paths=("$DIAG_SOURCE_OWNER" "$source" "$CI_JOB_TEMP" "$CI_JOB_TEMP/proof") rows=()
  [[ $label =~ ^[a-zA-Z0-9_-]{1,96}$ && $phase =~ ^[a-zA-Z0-9_-]{1,96}$ ]]
  [[ $planned =~ ^[0-9]{1,12}$ && $proof_growth =~ ^[0-9]{1,12}$ ]]
  source_identity=$(stat -c '%d:%i' "$DIAG_SOURCE_OWNER")
  job_identity=$(stat -c '%d:%i' "$CI_JOB_TEMP")
  proof_identity=$(stat -c '%d:%i' "$CI_JOB_TEMP/proof")
  copy_device=${job_identity%%:*}; proof_device=${proof_identity%%:*}
  origin=$(du -sx --block-size=1 --exclude="$CARGO_TARGET_DIR" "$DIAG_SOURCE_OWNER" | cut -f1)
  if [[ -e $copy || -L $copy ]]; then
    [[ -d $copy && ! -L $copy && $(realpath -e "$copy") == "$copy" && $(stat -c %u "$copy") == "$UID" ]]
    copy_identity=$(stat -c '%d:%i' "$copy")
    copy_device=${copy_identity%%:*}
    actual=$(du -sx --block-size=1 "$copy" | cut -f1)
    [[ $(stat -c '%d:%i' "$copy") == "$copy_identity" ]]
    paths+=("$copy")
  fi
  [[ $(stat -c '%d:%i' "$DIAG_SOURCE_OWNER") == "$source_identity" ]]
  [[ $origin =~ ^[0-9]{1,12}$ && $actual =~ ^[0-9]{1,12}$ ]]
  source_pass=$((origin <= source_cap)); copy_pass=$((actual + planned <= copy_cap))
  old_pass=$((origin + actual + planned <= source_cap))
  result=$((!source_pass || !copy_pass))
  # 기존 source는 추가 성장0이다. FS가 같으면 copy/proof의 추가 성장만 합산한다.
  for path in "${paths[@]}"; do
    identity=$(stat -c '%d:%i' "$path"); device=${identity%%:*}
    [[ $seen != *":$device:"* ]] || continue
    seen+="$device:"
    free=$(df -PB1 "$path" | awk 'NR==2 {print $4}')
    [[ $free =~ ^[0-9]{1,15}$ ]]
    required=$((reserve + (device == copy_device ? planned : 0) + (device == proof_device ? proof_growth : 0)))
    fs_pass=$((free >= required)); result=$((result || !fs_pass))
    printf -v row 'filesystem\t%s\t%s\t%s\t%s\t%s\t%s' "$path" "$device" "${identity#*:}" "$free" "$required" "$fs_pass"
    rows+=("$row")
  done
  printf -v record 'schema\t1\nphase\t%s\nlabel\t%s\nsource_owner\t%s\nsource_identity\t%s\ncopy_root\t%s\ncopy_identity\t%s\njob_identity\t%s\nproof_identity\t%s\norigin_allocated\t%s\ncopy_allocated\t%s\ncopy_planned\t%s\nsource_planned\t0\nproof_planned_growth\t%s\nsource_cap\t%s\ncopy_cap\t%s\ncombined_max\t4294967296\nfinish_reserve\t%s\nold_combined2GiB_would_pass\t%s\nsource_cap_pass\t%s\ncopy_cap_pass\t%s\ncheck_result\t%s\n' \
    "$phase" "$label" "$DIAG_SOURCE_OWNER" "$source_identity" "$copy" "$copy_identity" "$job_identity" "$proof_identity" \
    "$origin" "$actual" "$planned" "$proof_growth" "$source_cap" "$copy_cap" "$reserve" "$old_pass" "$source_pass" "$copy_pass" "$result"
  record_size=${#record}
  for row in "${rows[@]}"; do record_size=$((record_size + ${#row} + 1)); done
  ((record_size <= 8192))
  source_guard_resource_count=${source_guard_resource_count:-0}
  [[ $source_guard_resource_count =~ ^[0-9]{1,3}$ ]]
  ((10#$source_guard_resource_count < 256))
  source_guard_resource_count=$((10#$source_guard_resource_count + 1))
  diagnostic=$proof/copy-admission-$source_guard_resource_count.tsv
  # cap/space 거부로 반환하기 전에 원문 숫자와 각 결과를 create_new로 남긴다.
  (set -C; { printf '%s' "$record"; printf '%s\n' "${rows[@]}"; } > "$diagnostic")
  if ((result != 0)); then
    printf 'source/copy admission rejected; see %s\n' "$diagnostic" >&2
    return 1
  fi
  for path in "$source" "$CI_JOB_TEMP"; do
    device=$(stat -c %d "$path")
    free=$(df -PB1 "$path" | awk 'NR==2 {print $4}')
    [[ $free =~ ^[0-9]+$ ]]
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$label" "$path" "$device" "$free" "$origin" "$((actual+planned))" >> "$proof/copy-growth.tsv"
  done
}
source_guard_copy_seal() {
  local moment=$1 root=$CI_JOB_TEMP/logging-source-absence-copy/source leaf
  [[ $source_guard_copy_ready == true ]]
  [[ $(stat -c '%d:%i:%f:%u:%g' "${root%/source}") == "$source_guard_copy_identity" ]]
  for leaf in "$root" "$root/rust" "$root/rust/tools"; do
    [[ -d $leaf && ! -L $leaf && $(realpath -e "$leaf") == "$leaf" && $(stat -c %u "$leaf") == "$UID" ]]
  done
  for leaf in rust/tools/check_native_logging.py rust/tools/native_logging_transport.py; do
    # dangling symlink도 present다. lookup 불능은 find/후속 전체 snapshot에서 strict 실패한다.
    [[ ! -e $root/$leaf && ! -L $root/$leaf ]]
  done
  # 통제가 생성/제거하는 directory mtime/ctime만 제외한다. 파일 identity/hash/전체 roster는 유지한다.
  find -P "$root" -printf '%D:%i\t%m:%U:%G:%n\t%y\t%p\t%l\0' | LC_ALL=C sort -z > "$proof/copy-$moment.roster.bin"
  find -P "$root" -type f -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%p\0' | LC_ALL=C sort -z > "$proof/copy-$moment.files.bin"
  find -P "$root" -type f -print0 | LC_ALL=C sort -z | xargs -0 -r sha256sum > "$proof/copy-$moment.sha256"
  find -P "$root" -type f -printf '%D:%i\t%m:%U:%G:%n\t%s\t%T@\t%C@\t%p\0' | LC_ALL=C sort -z > "$proof/copy-$moment.stable.bin"
  cmp "$proof/copy-$moment.files.bin" "$proof/copy-$moment.stable.bin"
  if [[ $moment == after ]]; then
    for leaf in roster.bin files.bin sha256; do cmp "$proof/copy-before.$leaf" "$proof/copy-after.$leaf"; done
  fi
}
source_guard_finish() {
  local result=$? source_result sdk_result held_result copy_result resource_result
  trap - EXIT
  set +e
  (set -e; ulimit -f 524288; legacy_seal after) > "$proof/source-final.stdout" 2> "$proof/source-final.stderr"; source_result=$?
  (set -e; ulimit -f 524288; legacy_sdk_seal after) > "$proof/sdk-final.stdout" 2> "$proof/sdk-final.stderr"; sdk_result=$?
  verify_held > "$proof/held-final.stdout" 2> "$proof/held-final.stderr"; held_result=$?
  (set -e; ulimit -f 524288; source_guard_copy_seal after) > "$proof/copy-final.stdout" 2> "$proof/copy-final.stderr"; copy_result=$?
  (set -e; admit source-guard-exit 0) > "$proof/resource-final.stdout" 2> "$proof/resource-final.stderr"; resource_result=$?
  if ((result == 0 && (source_result || sdk_result || held_result || copy_result || resource_result))); then result=1; fi
  printf '{"schema":1,"mode":"source-guard","source_policy":"%s","phase":"%s","exit_code":%s,"source_final_exit_code":%s,"sdk_final_exit_code":%s,"held_final_exit_code":%s,"copy_final_exit_code":%s,"resource_final_exit_code":%s,"postdelete_full_acceptance":false,"R4_acceptance":false,"retirement_credit":0}\n' \
    "$source_policy" "$phase" "$result" "$source_result" "$sdk_result" "$held_result" "$copy_result" "$resource_result" > "$proof/outcome.json"
  exit "$result"
}
# source-guard의 고정 Git 관측 네 개만 진단한다. index 기준을 갱신하지 않는다.
source_guard_index_identity() { stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' -- "$1"; }
source_guard_index_record() {
  local row current
  [[ $sg_index_records -lt 32 && ! -L $sg_index_journal && ! -L $sg_index_dir &&
     $(stat -c '%d:%i:%f:%u:%g' "$sg_index_dir") == "$sg_index_dir_identity" ]] || return 1
  current=$(source_guard_index_identity "/proc/$$/fd/$sg_index_journal_fd") || return 1
  [[ $current == "$sg_index_journal_identity" && $(source_guard_index_identity "$sg_index_journal") == "$current" ]] || return 1
  printf -v row '%s\t' "$@"
  row=${row%$'\t'}
  (( $(stat -Lc %s "/proc/$$/fd/$sg_index_journal_fd") + ${#row} + 1 <= 32768 )) || return 1
  printf '%s\n' "$row" >&"$sg_index_journal_fd" || return 1
  sg_index_journal_identity=$(source_guard_index_identity "/proc/$$/fd/$sg_index_journal_fd") || return 1
  [[ $(source_guard_index_identity "$sg_index_journal") == "$sg_index_journal_identity" ]] || return 1
  sg_index_records=$((sg_index_records + 1))
}
source_guard_index_capture() {
  local stage=$1 side=$2 path=$3 output=$4 parent expected_parent before after bytes fd raw_identity digest read_code=0
  sg_index_capture_error=unsafe-input
  [[ $stage =~ ^[a-z-]{1,32}$ && ( $side == source || $side == copy ) &&
     $output == "$sg_index_dir/${output##*/}" && ! -e $output && ! -L $output &&
     ! -L $sg_index_dir && $(stat -c '%d:%i:%f:%u:%g' "$sg_index_dir") == "$sg_index_dir_identity" ]] || return 1
  parent=${path%/index}
  if [[ $side == source ]]; then expected_parent=$sg_index_source_parent; else expected_parent=$sg_index_copy_parent; fi
  [[ -d $parent && ! -L $parent && $(realpath -e "$parent") == "$parent" &&
     $(stat -c '%d:%i:%f:%u:%g' "$parent") == "$expected_parent" &&
     -f $path && ! -L $path && $(stat -c '%u:%h' "$path") == "$UID:1" ]] || return 1
  before=$(source_guard_index_identity "$path") || return 1
  bytes=$(stat -c %s "$path") || return 1
  [[ $bytes =~ ^[0-9]+$ ]] && ((bytes > 0 && bytes <= 4194304)) || return 1
  source_guard_index_record reading "$stage" "$side" "$path" "$before" "$bytes" "${output##*/}" || return 1
  # 기존 collision capture와 동일한 NOFOLLOW/NONBLOCK 단일 input FD다.
  # source FD fstat의 직접 관측으로 과장하지 않고 path full identity 전후를 함께 검사한다.
  sg_index_capture_error=read-failed
  (ulimit -f 4097 && /usr/bin/dd if="$path" of="$output" bs=65536 count="$((bytes+1))" \
    iflag=nofollow,nonblock,fullblock,count_bytes oflag=nofollow,nonblock conv=excl status=none) \
    2> "$sg_index_dir/$stage-$side.stderr" || read_code=$?
  sg_index_capture_error="read-failed:$read_code"
  [[ $(stat -c %s "$sg_index_dir/$stage-$side.stderr") -le 4096 && $read_code == 0 &&
     -f $output && ! -L $output && $(stat -c '%u:%h:%s' "$output") == "$UID:1:$bytes" ]] || return 1
  sg_index_capture_error=input-changed
  after=$(source_guard_index_identity "$path") || return 1
  [[ -f $path && ! -L $path && ! -L $parent && $(realpath -e "$parent") == "$parent" &&
     $before == "$after" && $(stat -c '%d:%i:%f:%u:%g' "$parent") == "$expected_parent" ]] || return 1
  # 원문 input은 다시 열어 hash하지 않는다. exclusive 캡처를 같은 held FD로 hash한다.
  sg_index_capture_error=capture-changed
  raw_identity=$(source_guard_index_identity "$output") || return 1
  exec {fd}< "$output" || return 1
  if [[ $(source_guard_index_identity "/proc/$$/fd/$fd") != "$raw_identity" ]]; then exec {fd}<&-; return 1; fi
  digest=$(sha256sum "/proc/$$/fd/$fd") || { exec {fd}<&-; return 1; }
  digest=${digest%% *}
  [[ $digest =~ ^[0-9a-f]{64}$ && $(source_guard_index_identity "/proc/$$/fd/$fd") == "$raw_identity" &&
     $(source_guard_index_identity "$output") == "$raw_identity" && ! -L $path &&
     $(source_guard_index_identity "$path") == "$before" ]] || { exec {fd}<&-; return 1; }
  exec {fd}<&-
  sg_index_observed_identity=$before; sg_index_observed_sha=$digest; sg_index_observed_capture_identity=$raw_identity
}
source_guard_index_budget() {
  local label=$1 pending=$2 used
  used=$(du -sx --block-size=1 "$CI_JOB_TEMP/proof" | cut -f1) || return 1
  [[ $used =~ ^[0-9]+$ ]] && ((used + pending <= 2147483648)) || return 1
  source_guard_resource "$label" 0 "$pending"
}
source_guard_index_begin() {
  local capture_code=0
  sg_index_dir=$proof/index-journal
  mkdir -m 700 -- "$sg_index_dir"
  sg_index_dir_identity=$(stat -c '%d:%i:%f:%u:%g' "$sg_index_dir")
  sg_index_journal=$sg_index_dir/journal.tsv
  (set -C; : > "$sg_index_journal")
  exec {sg_index_journal_fd}>> "$sg_index_journal"
  sg_index_journal_identity=$(source_guard_index_identity "$sg_index_journal")
  [[ $(source_guard_index_identity "/proc/$$/fd/$sg_index_journal_fd") == "$sg_index_journal_identity" ]]
  sg_index_records=0; sg_index_failed=false; sg_index_copy_identity=; sg_index_checks=0
  sg_index_source_parent=$(stat -c '%d:%i:%f:%u:%g' "$source/.git")
  source_guard_index_budget index-before-copy 8421376
  source_guard_index_record schema 1 input_open NOFOLLOW_NONBLOCK input_fd_fstat_directly_observed false capture_fd_hash_identity_checked true
  source_guard_index_capture before-copy source "$source/.git/index" "$sg_index_dir/source-before.raw" || capture_code=$?
  if ((capture_code != 0)); then
    sg_index_failed=true
    source_guard_index_record before-copy cp-not-started 0 capture-error "$sg_index_capture_error"
    return 1
  fi
  sg_index_source_identity=$sg_index_observed_identity; sg_index_source_sha=$sg_index_observed_sha
  source_guard_index_record before-copy cp-not-started 0 source "$source/.git/index" "$sg_index_source_identity" "$sg_index_source_sha" source-before.raw "$sg_index_observed_capture_identity" baseline
  hold "$sg_index_dir/source-before.raw"
}
source_guard_index_check() {
  local stage=$1 command_code=$2 source_identity source_hash source_raw_identity copy_identity copy_hash copy_raw_identity command result=equal capture_code=0 expected_code=0
  shift 2
  [[ $sg_index_failed == false && $sg_index_checks -lt 6 ]]
  sg_index_checks=$((sg_index_checks + 1))
  printf -v command '%q ' "$@"
  if [[ $stage == after-diff-files ]]; then
    [[ $root == "$CI_JOB_TEMP/logging-source-absence-copy/source" && $# == 10 &&
       $1 == git && $2 == --no-optional-locks && $3 == -c && $4 == diff.autoRefreshIndex=false &&
       $5 == -C && $6 == "$root" && $7 == diff && $8 == --exit-code && $9 == --name-only && ${10} == -z ]]
    if [[ $source_policy == present ]]; then expected_code=1; else expected_code=0; fi
    source_guard_index_record expected-exit "$stage" "$expected_code" actual-exit "$command_code" "$command"
  fi
  source_guard_index_budget "index-$stage" 16809984
  if [[ $stage == after-copy ]]; then
    sg_index_copy_parent=$(stat -c '%d:%i:%f:%u:%g' "$root/.git") || {
      sg_index_failed=true
      source_guard_index_record "$stage" "$command" "$command_code" capture-error copy-parent-unavailable
      return 1
    }
  fi
  source_guard_index_capture "$stage" source "$source/.git/index" "$sg_index_dir/source-current.raw" || capture_code=$?
  if ((capture_code == 0)); then
    source_identity=$sg_index_observed_identity; source_hash=$sg_index_observed_sha; source_raw_identity=$sg_index_observed_capture_identity
    source_guard_index_capture "$stage" copy "$root/.git/index" "$sg_index_dir/copy-current.raw" || capture_code=$?
  fi
  if ((capture_code != 0)); then
    sg_index_failed=true
    source_guard_index_record "$stage" "$command" "$command_code" capture-error "$sg_index_capture_error"
    return 1
  fi
  copy_identity=$sg_index_observed_identity; copy_hash=$sg_index_observed_sha; copy_raw_identity=$sg_index_observed_capture_identity
  if [[ $source_identity != "$sg_index_source_identity" || $source_hash != "$sg_index_source_sha" ||
        $copy_hash != "$sg_index_source_sha" || ( -n $sg_index_copy_identity && $copy_identity != "$sg_index_copy_identity" ) ]]; then result=mismatch; fi
  if ((command_code != expected_code)); then
    if [[ $result == equal ]]; then result=command-failed; else result=mismatch-and-command-failed; fi
  fi
  source_guard_index_record "$stage" "$command" "$command_code" source "$source/.git/index" "$source_identity" "$source_hash" source-current.raw "$source_raw_identity" "$result"
  source_guard_index_record "$stage" "$command" "$command_code" copy "$root/.git/index" "$copy_identity" "$copy_hash" copy-current.raw "$copy_raw_identity" "$result"
  if [[ $result != equal ]]; then
    sg_index_failed=true
    # rename은 진단 캡처에만 적용한다. 원본/복사본 index는 절대 복원·재생성하지 않는다.
    [[ ! -e $sg_index_dir/first-source-after.raw && ! -L $sg_index_dir/first-source-after.raw &&
       ! -e $sg_index_dir/first-copy-after.raw && ! -L $sg_index_dir/first-copy-after.raw ]]
    mv -n -- "$sg_index_dir/source-current.raw" "$sg_index_dir/first-source-after.raw"
    mv -n -- "$sg_index_dir/copy-current.raw" "$sg_index_dir/first-copy-after.raw"
    [[ ! -e $sg_index_dir/source-current.raw && ! -L $sg_index_dir/source-current.raw &&
       ! -e $sg_index_dir/copy-current.raw && ! -L $sg_index_dir/copy-current.raw ]]
    source_guard_index_record retained-first-after source-current.raw first-source-after.raw "$(source_guard_index_identity "$sg_index_dir/first-source-after.raw")" \
      copy-current.raw first-copy-after.raw "$(source_guard_index_identity "$sg_index_dir/first-copy-after.raw")"
    printf 'copied Git index strict failure at %s; see %s\n' "$stage" "$sg_index_journal" >&2
    return 1
  fi
  if [[ -z $sg_index_copy_identity ]]; then
    sg_index_copy_identity=$copy_identity
    [[ ! -e $sg_index_dir/copy-before.raw && ! -L $sg_index_dir/copy-before.raw ]]
    mv -n -- "$sg_index_dir/copy-current.raw" "$sg_index_dir/copy-before.raw"
    [[ ! -e $sg_index_dir/copy-current.raw && ! -L $sg_index_dir/copy-current.raw ]]
    source_guard_index_record retained-baseline copy-current.raw copy-before.raw "$(source_guard_index_identity "$sg_index_dir/copy-before.raw")" "$copy_hash"
    hold "$sg_index_dir/copy-before.raw"
  else rm -- "$sg_index_dir/copy-current.raw"; fi
  rm -- "$sg_index_dir/source-current.raw"
}
source_guard_index_git() {
  local stage=$1 output=$2 command_code=0
  shift 2
  "$@" > "$output" || command_code=$?
  source_guard_index_check "$stage" "$command_code" "$@"
  # actual status는 journal에 남는다. 고정 diff의 expected1만 strict 검사 후 성공이다.
  return 0
}
source_guard_index_end() {
  [[ $sg_index_failed == false && $sg_index_checks == 6 ]]
  source_guard_index_record complete strict-checks 0 raw-baselines-never-updated true
  exec {sg_index_journal_fd}>&-
  hold "$sg_index_journal"
}
source_guard() {
  local prior=$CI_JOB_TEMP/proof/logging legacy=$CI_JOB_TEMP/proof/logging-legacy-runtime2
  local path hash identity prior_fd selected context builder size estimate git_bytes copy_git_code=0
  local copy=$CI_JOB_TEMP/logging-source-absence-copy root=$CI_JOB_TEMP/logging-source-absence-copy/source
  local selector=source_absence::source_absence_rejects_original_nodes_before_producers
  source_guard_copy_ready=false
  reuse_retained
  trap source_guard_finish EXIT
  phase=source-guard-reuse
  for path in "$legacy/outcome.json" "$legacy/complete.json" "$legacy/build/legacy-builder-build.stdout" "$legacy/held-before.tsv"; do legacy_file "$path" 536870912; done
  jq -e --arg source_policy "$source_policy" '.source_policy==$source_policy and .mode=="legacy-runtime2" and .phase=="legacy-runtime2-observed" and .exit_code==0 and .source_sdk_final_exit_code==0 and .held_final_exit_code==0 and .provider_alias_final_exit_code==0 and .resource_final_exit_code==0' "$legacy/outcome.json" > "$proof/legacy-success.check"
  jq -e --arg source_policy "$source_policy" --arg sha "$GITHUB_SHA" '.source_policy==$source_policy and .source_commit==$sha and .logical_cases==2 and .logical_commands==2 and .signal_order==["INT","TERM"] and .R4_acceptance==false and .retirement_credit==0' "$legacy/complete.json" > "$proof/legacy-cases.check"
  while IFS=$'\t' read -r prior_fd hash identity path; do
    [[ $prior_fd =~ ^[0-9]+$ && $hash =~ ^[a-f0-9]{64}$ && $path == /* && $(realpath -e "$path") == "$path" && -f $path && ! -L $path ]]
    [[ $(legacy_identity "$path") == "$identity" && $(sha256sum "$path" | cut -d' ' -f1) == "$hash" ]]
    hold "$path"
  done < "$legacy/held-before.tsv"
  builder=$CARGO_TARGET_DIR/legacy-runtime2-builder/debug/build-native-logging-reference
  selected=$(jq -ers --arg path "$builder" --arg src "$source/rust/crates/dev-tools/src/bin/build_native_logging_reference.rs" '
    if .[-1].reason!="build-finished" or .[-1].success!=true then error("builder Cargo EOF") else . end |
    [.[]|select(.reason=="compiler-artifact" and .target.name=="build-native-logging-reference" and .target.kind==["bin"] and .target.src_path==$src and .profile.test==false and .executable==$path)] |
    if length==1 and .[0].fresh==false then .[0].executable else error("prior fresh builder required") end' "$legacy/build/legacy-builder-build.stdout")
  [[ $selected == "$builder" ]]
  elf source-guard-builder "$builder"
  context=$prior/context/context.json
  legacy_file "$context"
  for path in /usr/bin/dd /usr/bin/mv /usr/bin/rm; do elf "source-guard-index-${path##*/}" "$path"; done
  legacy_tree "$source/.git" "$proof/original-git-before"
  if [[ $source_policy == present ]]; then
  printf '%s  %s\n' adee1d45e299bfd6448e8f5584b194b2dbbb53622ee3c1d57c02538d0b0d1fc4 "$source/rust/tools/check_native_logging.py" \
    288a4fd0f9aca05d82a3f415e6632ef3e62b92f2d73f01d7a2f1b5263cdb7ad1 "$source/rust/tools/native_logging_transport.py" | sha256sum --check --strict - > "$proof/originals-present.txt"
  else source_policy_checkpoint; fi
  [[ ! -e $copy && ! -L $copy ]]
  # 실제 Git/index와 tracked payload만 복사한다. target/SDK/archive를 복제하지 않는다.
  git --no-optional-locks ls-files -z > "$proof/copy-index.bin"
  cmp "$CI_JOB_TEMP/proof/index-before.bin" "$proof/copy-index.bin"
  : > "$proof/copy-selected.bin"
  estimate=1048576
  while IFS= read -r -d '' path; do
    [[ $path != /* && $path != .git && $path != .git/* && $path != ../* && $path != */../* ]]
    case $path in rust/tools/check_native_logging.py|rust/tools/native_logging_transport.py) continue;; esac
    printf '%s\0' "$path" >> "$proof/copy-selected.bin"
    size=$(stat -c %s -- "$source/$path")
    [[ $size =~ ^[0-9]+$ ]]
    estimate=$((estimate + ((size+4095)/4096+2)*4096))
  done < "$proof/copy-index.bin"
  [[ -d $source/.git && ! -L $source/.git ]]
  find -P "$source/.git" -type l -print > "$proof/git-links.txt"
  test ! -s "$proof/git-links.txt"
  git_bytes=$(du -sx --block-size=1 "$source/.git" | cut -f1)
  [[ $git_bytes =~ ^[0-9]+$ ]]
  # metadata directory/파일마다 추가 block도 보수적으로 admit한다.
  size=$(find -P "$source/.git" -printf x | wc -c)
  estimate=$((estimate + git_bytes + size*8192))
  source_guard_resource before-copy "$estimate" 67108864
  mkdir -m 700 -- "$copy" "$root"
  source_guard_copy_identity=$(stat -c '%d:%i:%f:%u:%g' "$copy")
  (ulimit -f 524288; tar --hard-dereference --null --no-recursion --verbatim-files-from -T "$proof/copy-selected.bin" -cf -) | tar --same-permissions -xf - -C "$root"
  source_guard_index_begin
  cp -a -- "$source/.git" "$root/.git" || copy_git_code=$?
  source_guard_index_check after-copy "$copy_git_code" cp -a -- "$source/.git" "$root/.git"
  # 실제 copy mode를 기록하되 전체 canonical tar 동일성 검사는 그대로 유지한다.
  (set -C; stat -c $'%n\t%a\t%f\t%u\t%g\t%s' --     "$source/rust/Cargo.toml" "$root/rust/Cargo.toml"     "$source/tools/ci/logging_retained_549.sh" "$root/tools/ci/logging_retained_549.sh" > "$proof/copy-source-modes.tsv")
  test "$(stat -c %s "$proof/copy-source-modes.tsv")" -le 4096
  hold "$proof/copy-source-modes.tsv"
  source_guard_index_git after-rev-parse "$proof/copied-head.txt" git --no-optional-locks -C "$root" rev-parse HEAD
  [[ $(cat "$proof/copied-head.txt") == "$GITHUB_SHA" ]]
  source_guard_index_git after-ls-files "$proof/copied-index.bin" git --no-optional-locks -C "$root" ls-files -z
  cmp "$proof/copy-index.bin" "$proof/copied-index.bin"
  source_guard_index_git after-diff-cached "$proof/copy-cached.diff" git --no-optional-locks -C "$root" diff --cached --exit-code
  source_guard_index_git after-diff-files "$proof/copy-deletions.bin" git --no-optional-locks -c diff.autoRefreshIndex=false -C "$root" diff --exit-code --name-only -z
  if [[ $source_policy == present ]]; then
    printf 'rust/tools/check_native_logging.py\0rust/tools/native_logging_transport.py\0' > "$proof/copy-deletions.expected.bin"
  else
    : > "$proof/copy-deletions.expected.bin"
  fi
  cmp "$proof/copy-deletions.expected.bin" "$proof/copy-deletions.bin"
  # canonical tar와 Git metadata bytes를 source->copy 대조한다. raw index 변경도 숨기지 않는다.
  for path in "$source" "$root"; do
    (cd "$path"; tar --hard-dereference --null --no-recursion --verbatim-files-from -T "$proof/copy-selected.bin" -cf - --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner) | sha256sum | cut -d' ' -f1 >> "$proof/copied-source.sha256"
  done
  test "$(sort -u "$proof/copied-source.sha256" | wc -l)" -eq 1
  (cd "$source/.git"; find -P . -type f -print0 | LC_ALL=C sort -z | xargs -0 -r sha256sum) > "$proof/git-source.sha256"
  source_guard_index_check before-git-manifest 0 checkpoint-only no-command-executed
  (cd "$root/.git"; sha256sum --check --strict --quiet "$proof/git-source.sha256")
  source_guard_index_end
  jq -er '.source_files|if length==19 then to_entries[]|[.key,.value.sha256,(.value.size|tostring)]|@tsv else error("current19 required") end' "$context" > "$proof/current19.tsv"
  while IFS=$'\t' read -r path hash size; do
    [[ $path != /* && $path != ../* && $path != */../* && $hash =~ ^[a-f0-9]{64}$ && $size =~ ^[0-9]+$ ]]
    [[ -f $root/$path && ! -L $root/$path && $(stat -c %s "$root/$path") == "$size" ]]
    printf '%s  %s\n' "$hash" "$root/$path" | sha256sum --check --strict --quiet -
  done < "$proof/current19.tsv"
  source_guard_copy_ready=true
  source_guard_copy_seal before
  admit source-guard-copied 0
  NATIVE_LOGGING_RETAINED_BINARY_SHA256=$(sha256sum "$probe" | cut -d' ' -f1)
  NATIVE_LOGGING_RETAINED_CONTEXT_SHA256=$(sha256sum "$context" | cut -d' ' -f1)
  guard_env=(CI_JOB_TEMP="$CI_JOB_TEMP" GITHUB_SHA="$GITHUB_SHA" GITHUB_RUN_ID="$GITHUB_RUN_ID" GITHUB_RUN_ATTEMPT="$GITHUB_RUN_ATTEMPT" GITHUB_JOB="$GITHUB_JOB"
    NATIVE_LOGGING_SOURCE_ABSENCE_ROOT="$root" NATIVE_LOGGING_LIFECYCLE_EVIDENCE="$proof/lifecycle"
    NATIVE_LOGGING_RETAINED_BINARY="$probe" NATIVE_LOGGING_RETAINED_CONTEXT="$context"
    NATIVE_LOGGING_RETAINED_BINARY_SHA256="$NATIVE_LOGGING_RETAINED_BINARY_SHA256" NATIVE_LOGGING_RETAINED_CONTEXT_SHA256="$NATIVE_LOGGING_RETAINED_CONTEXT_SHA256"
    NATIVE_LOGGING_RETAINED_SOURCE_COMMIT="$GITHUB_SHA" NATIVE_LOGGING_TEST_BUILDER="$builder")
  run source-absence 600 "${runtime[@]}" "${guard_env[@]}" /usr/bin/strace --seccomp-bpf --kill-on-exit -ff -ttt -x -v -s 65536 \
    -e trace=process,%file,%desc,%signal,%network,%memory,setsid,setpgid,prctl,unshare,setns -e raw=read,pread64 \
    -o "$proof/trace/source-absence" "$fixture" --ignored --exact "$selector" --test-threads=1 --color=never
  test "$(grep -Ec '^test result: ok\. 1 passed; 0 failed; 0 ignored;' "$proof/build/source-absence.stdout")" -eq 1
  grep -Fx "test $selector ... ok" "$proof/build/source-absence.stdout" > "$proof/selector.check"
  cmp "$proof/inputs.before.json" "$proof/inputs.after.json"
  legacy_file "$proof/inputs.before.json"; legacy_file "$proof/inputs.after.json"; legacy_file "$proof/complete.json"
  jq -e --arg source "$GITHUB_SHA" --arg selector "$selector" '.schema==1 and .source_commit==$source and .selector==$selector and .actual_cli_rejections==20 and .paths==2 and .node_variants==5 and .modes==["retained","legacy"] and .providers_unchanged==true and .source_parents_unchanged==true and .outputs_absent==true and .zero_child_exec_requires_external_trace==true and .absent_root_full_functional_acceptance==false and .R4_acceptance==false and .retirement_credit==0' "$proof/complete.json" > "$proof/complete.check"
  find -P "$proof/lifecycle" -mindepth 1 -maxdepth 1 -print | LC_ALL=C sort > "$proof/case-roots.txt"
  test "$(wc -l < "$proof/case-roots.txt")" -eq 20
  : > "$proof/cases.jsonl"
  while IFS= read -r path; do
    [[ $path == "$proof/lifecycle/logging-lifecycle-"?????? && $(realpath -e "$path") == "$path" && ! -L $path && $(stat -c '%u:%a' "$path") == "$UID:700" ]]
    legacy_file "$path/source-absence-case.json"
    legacy_file "$path/checker.stdout"; legacy_file "$path/checker.stderr"
    test ! -s "$path/checker.stdout"
    jq -ce . "$path/source-absence-case.json" >> "$proof/cases.jsonl"
  done < "$proof/case-roots.txt"
  jq -se --slurpfile complete "$proof/complete.json" 'sort_by(.root)==($complete[0].cases|sort_by(.root))' "$proof/cases.jsonl" > "$proof/cases.check"
  # raw에서 expected를 만들지 않는다. Rust source의 고정 순서와 env_clear whitelist를 사용한다.
  printf '%s\0' "${runtime[@]:2}" "${guard_env[@]}" | jq -Rs 'split("\u0000")[:-1]' > "$proof/fixture-env.json"
  printf '%s\0' PATH=/usr/bin:/bin HOME="$HOME" TMPDIR="$TMPDIR" TZ=UTC LANG=C.UTF-8 LC_ALL=C.UTF-8 \
    GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.preloadIndex GIT_CONFIG_VALUE_0=true | jq -Rs 'split("\u0000")[:-1]' > "$proof/checker-env.json"
  jq -n --arg proof "$proof" --arg copy "$root" --arg source "$source" --arg commit "$GITHUB_SHA" --arg fixture "$fixture" \
    --arg checker "$checker" --arg probe "$probe" --arg builder "$builder" --arg context "$context" --arg selector "$selector" \
    --arg source_policy "$source_policy" --arg binary_sha "$NATIVE_LOGGING_RETAINED_BINARY_SHA256" --argjson uid "$UID" \
    --slurpfile prior "$prior/exec-policy/binding.json" --slurpfile complete "$proof/complete.json" \
    --slurpfile fixture_env "$proof/fixture-env.json" --slurpfile checker_env "$proof/checker-env.json" '
    {schema:1,mode:"source-guard",source_policy:$source_policy,source:$source,proof:$proof,copy:$copy,commit:$commit,fixture:$fixture,checker:$checker,
     probe:$probe,builder:$builder,context:$context,selector:$selector,binary_sha:$binary_sha,uid:$uid,providers:$prior[0].providers,
     cases:$complete[0].cases,fixture_env:$fixture_env[0],checker_env:$checker_env[0]}' > "$proof/binding.json"
  legacy_file "$proof/binding.json"
  legacy_file "$prior/exec-policy/binding.json"
  legacy_file "$source/tools/ci/logging_retained_exec_policy_549.jq"
  run inspect-source-absence 180 "${runtime[@]}" "$checker" inspect-trace-summary --directory "$proof/trace" --prefix source-absence --output "$proof/inspect.json"
  legacy_file "$proof/inspect.json" 67108864
  run exec-source-absence 60 "${runtime[@]}" /usr/bin/jq -e --arg source_policy "$source_policy" --arg prefix source-absence --slurpfile binding "$proof/binding.json" \
    -f "$source/tools/ci/logging_retained_exec_policy_549.jq" "$proof/inspect.json"
  mv "$proof/build/exec-source-absence.stdout" "$proof/exec-policy.json"
  legacy_file "$proof/exec-policy.json"
  for control in missing-structure wrong-argv wrong-env; do
    case $control in
      missing-structure)
        mutation='.events|=map(select(.operation as $op|(["fork","vfork","clone","clone3","setpgid","setsid","unshare","setns"]|index($op))==null))'
        expected_error='process/session detail cardinality differs' ;;
      wrong-argv)
        mutation='(.events[]|select(.exec.program.bytes==($checker|explode))|.exec.argv)+=[{bytes:[120],source:{start:0,end:1}}]'
        expected_error='source guard argv/env/error/terminal/zero-child differs' ;;
      wrong-env)
        mutation='(.events[]|select(.exec.program.bytes==($checker|explode))|.exec.environment.entries)+=[{bytes:[88,61,49],source:{start:0,end:3}}]'
        expected_error='source guard argv/env/error/terminal/zero-child differs' ;;
    esac
    /usr/bin/jq --arg checker "$checker" "$mutation" "$proof/inspect.json" > "$proof/control-$control.input.json"
    run "policy-control-$control" 60 "${runtime[@]}" /usr/bin/bash --noprofile --norc -euo pipefail -c '
      status=0
      /usr/bin/jq -e --arg source_policy "$6" --arg prefix source-absence --slurpfile binding "$1" -f "$2" "$3" > "$4.stdout" 2> "$4.stderr" || status=$?
      printf "%s\n" "$status" > "$4.exit"
      test "$status" -eq 5 && test ! -s "$4.stdout"
      grep -F -- "$5" "$4.stderr"
    ' source-policy-control "$proof/binding.json" "$source/tools/ci/logging_retained_exec_policy_549.jq" "$proof/control-$control.input.json" "$proof/control-$control" "$expected_error" "$source_policy"
  done
  printf '{"schema":1,"controls":3,"each_guard_exact_error":true,"actual_raw_changed":false,"R4_acceptance":false}\n' > "$proof/controls.json"
  legacy_file "$proof/controls.json"
  source_policy_controls source-absence "$proof/binding.json" "$proof/inspect.json" "$proof/source-policy-controls"
  legacy_tree "$source/.git" "$proof/original-git-after"
  cmp "$proof/original-git-before.roster" "$proof/original-git-after.roster"
  cmp "$proof/original-git-before.sha256" "$proof/original-git-after.sha256"
  verify_held
  phase=source-guard-observed
}
source_policy_init
if [[ $source_policy == absent ]]; then hold "$source_policy_baseline"; hold "$source_policy_metadata"; fi
if [[ $mode == source-guard ]]; then source_guard; exit; fi

if [[ $mode == legacy-runtime2 ]]; then legacy_runtime2; exit; fi

for path in "$source/tools/ci/logging_retained_549.sh" "$source/tools/ci/logging_retained_exec_policy_549.jq" "$source/.github/workflows/python-rust-523-logging.yml" "$sdk/cargo" "$sdk/rustc" "$sdk/clippy-driver" /usr/bin/git /usr/bin/strace /usr/bin/timeout; do hold "$path"; done
elf exec-policy-jq /usr/bin/jq
if [[ $source_policy == present ]]; then
printf '%s  %s\n' adee1d45e299bfd6448e8f5584b194b2dbbb53622ee3c1d57c02538d0b0d1fc4 rust/tools/check_native_logging.py \
  288a4fd0f9aca05d82a3f415e6632ef3e62b92f2d73f01d7a2f1b5263cdb7ad1 rust/tools/native_logging_transport.py | sha256sum --check --strict - > "$proof/originals-present.txt"
else source_policy_checkpoint; fi
admit initial-build 8192
common=(--manifest-path "$source/rust/Cargo.toml" -p openpilot-dev-tools --locked --no-default-features --features native-logging-check-native)
run checker-build 1800 "$sdk/cargo" build "${common[@]}" --bin check-native-logging --message-format=json
run probe-build 1800 "$sdk/cargo" build --manifest-path "$source/rust/Cargo.toml" --locked -p openpilot-logging --example native_logging_probe --message-format=json
run units-build 1800 "$sdk/cargo" test "${common[@]}" --lib --no-run --message-format=json
run fixture-build 1800 "$sdk/cargo" test "${common[@]}" --test native_logging_lifecycle --no-run --message-format=json
run strict-clippy 1800 "$sdk/cargo" clippy "${common[@]}" --lib --bin check-native-logging --test native_logging_lifecycle -- -D warnings
run probe-clippy 1800 "$sdk/cargo" clippy --manifest-path "$source/rust/Cargo.toml" --locked -p openpilot-logging --example native_logging_probe -- -D warnings
checker=$(artifact checker-build check-native-logging bin "$source/rust/crates/dev-tools/src/bin/check_native_logging.rs" false)
probe_built=$(artifact probe-build native_logging_probe example "$source/rust/crates/logging/examples/native_logging_probe.rs" false)
unit=$(artifact units-build openpilot_dev_tools lib "$source/rust/crates/dev-tools/src/lib.rs" true)
fixture=$(artifact fixture-build native_logging_lifecycle test "$source/rust/crates/dev-tools/tests/native_logging_lifecycle.rs" true)
test "$checker" = "$CARGO_TARGET_DIR/debug/check-native-logging"
probe=$proof/bin/native_logging_probe
install -m 700 -- "$probe_built" "$probe"
test "$(stat -c %h "$probe")" = 1
test "$(sha256sum "$probe" | cut -d' ' -f1)" = "$(sha256sum "$probe_built" | cut -d' ' -f1)"
elf checker "$checker"; elf probe "$probe"; elf unit "$unit"; elf fixture "$fixture"
run download-original-data 120 gh api --method GET repos/bin9208/openpilot-rust/actions/artifacts/12477566277/zip
archive=$proof/recovery.zip
mv "$proof/build/download-original-data.stdout" "$archive"
test "$(stat -c %s "$archive")" = 5896947
printf '%s  %s\n' 05ff6420f05f8b51f457b85ecf34ad514bc75b6b6147e4abf5d8f78d7f85462f "$archive" | sha256sum --check --strict -
hold "$archive"
# 인증 다운로드 뒤의 실행/trace에는 허용한 비밀 없는 환경만 전달한다.
unset GH_TOKEN GITHUB_TOKEN
runtime=(/usr/bin/env -i PATH=/usr/bin:/bin HOME="$HOME" TMPDIR="$TMPDIR" LANG=C.UTF-8 LC_ALL=C.UTF-8 TZ=UTC GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.preloadIndex GIT_CONFIG_VALUE_0=true GITHUB_ACTIONS=true RUNNER_ENVIRONMENT=github-hosted RUNNER_OS=Linux RUNNER_ARCH=X64 GITHUB_REPOSITORY=bin9208/openpilot-rust)
printf '{"schema":1,"source_commit":"%s","qualification_profile":"fixed-true-554-v1","preload_index":"true","historical_false_receipts_relabelled":false,"production_preload_changed":false,"R4_acceptance":false,"retirement_credit":0}\n' "$GITHUB_SHA" > "$proof/git-preload-profile.json"
hold "$proof/git-preload-profile.json"
# #554: 동일 Git provider의 의미 비교다. ptrace 성능 개선은 아직 가설이다.
admit git-preload-fixture 16
run git-preload-equivalence 90 "${runtime[@]}" /usr/bin/bash --noprofile --norc -euo pipefail -s -- "$proof/git-preload-554" <<'GIT_PRELOAD_554'
umask 077
base=$1
[[ $base == /* && $(realpath -m -- "$base") == "$base" ]]
available=$(df -PB1 -- "${base%/*}" | awk 'NR==2 {print $4}')
[[ $available =~ ^[0-9]+$ ]]
((available >= (16 + 512) * 1024 * 1024))
mkdir -m 700 -- "$base"
test "$(stat -c '%u:%a' -- "$base")" = "$UID:700"
test "$GIT_CONFIG_COUNT:$GIT_CONFIG_KEY_0:$GIT_CONFIG_VALUE_0" = 1:core.preloadIndex:true
mkdir -m 700 -- "$base/fixture"
usage() {
  local label=$1 bytes
  bytes=$(du -sx --block-size=1 -- "$base" | cut -f1) || return 1
  [[ $bytes =~ ^[0-9]+$ ]] || return 1
  printf '%s\t%s\n' "$label" "$bytes" >> "$base/allocated.tsv" || return 1
  ((bytes <= 16 * 1024 * 1024))
}
finish_fixture() {
  local result=$?
  trap - EXIT
  if ! usage final; then
    if ((result == 0)); then result=1; fi
  fi
  exit "$result"
}
trap finish_fixture EXIT
printf '{"proof_filesystem_available_bytes":%s,"fixture_cap_bytes":16777216,"reserve_bytes":536870912}\n' "$available" > "$base/admission.json"
cd "$base/fixture"
/usr/bin/git init --quiet --initial-branch=preload-554 --template=
for ((i=0; i<2048; i++)); do
  printf -v name 'tracked-%04d' "$i"
  printf 'fixed identical preload fixture bytes\n' > "$name"
done
usage authored-2048
/usr/bin/git add -- .
GIT_AUTHOR_NAME=preload-554 GIT_AUTHOR_EMAIL=preload-554@example.invalid \
  GIT_COMMITTER_NAME=preload-554 GIT_COMMITTER_EMAIL=preload-554@example.invalid \
  /usr/bin/git commit --quiet -m 'private preload equivalence fixture'
/usr/bin/git ls-files --stage > "$base/index.txt"
test "$(wc -l < "$base/index.txt")" -eq 2048
awk 'NF != 4 || $1 != "100644" || $3 != "0" {exit 1}' "$base/index.txt"
awk '{print $2}' "$base/index.txt" | sort -u > "$base/blob-oids.txt"
test "$(wc -l < "$base/blob-oids.txt")" -eq 1
test "$(/usr/bin/git config --bool --get core.fileMode)" = true
usage indexed-2048
matched=0
compare_status() {
  local state=$1 expected=$2 setting status
  printf '%s' "$expected" > "$base/$state.expected"
  usage "$state-before"
  for setting in true false; do
    if GIT_CONFIG_VALUE_0=$setting /usr/bin/git status --porcelain --untracked-files=normal \
        > "$base/$state-$setting.stdout" 2> "$base/$state-$setting.stderr"; then
      status=0
    else
      status=$?
    fi
    printf '%s\n' "$status" > "$base/$state-$setting.exit"
    test "$status" -eq 0
  done
  cmp "$base/$state-true.stdout" "$base/$state-false.stdout"
  cmp "$base/$state.expected" "$base/$state-true.stdout"
  cmp "$base/$state-true.stderr" "$base/$state-false.stderr"
  test ! -s "$base/$state-true.stderr"
  sha256sum -- "$base/$state-true.stdout" "$base/$state-false.stdout" >> "$base/status.sha256"
  matched=$((matched + 1))
  usage "$state-after"
}
compare_status clean ''
printf 'changed tracked fixture bytes\n' > tracked-0000
compare_status tracked-content $' M tracked-0000\n'
printf 'fixed identical preload fixture bytes\n' > tracked-0000
printf 'untracked fixture bytes\n' > untracked-only
compare_status untracked $'?? untracked-only\n'
rm -- untracked-only
chmod 700 tracked-0001
compare_status tracked-mode $' M tracked-0001\n'
test "$matched" -eq 4
usage complete
printf '{"schema":1,"tracked_files":2048,"unique_blobs":1,"states":4,"status_pairs":4,"status_bytes_equal":true,"fixture_cap_bytes":16777216,"performance_claim":false,"runtime_acceptance":false,"root_cause_fixed":false}\n' > "$base/complete.json"
GIT_PRELOAD_554
verify_held
hold "$proof/git-preload-554/complete.json"
hold "$proof/git-preload-554/status.sha256"
run git-preload-env-controls 30 "${runtime[@]}" "$fixture" --exact retained::fixed_git_preload_environment_rejects_missing_or_mutated_schema --test-threads=1 --color=never
test "$(grep -Ec '^test result: ok\. 1 passed; 0 failed; 0 ignored;' "$proof/build/git-preload-env-controls.stdout")" -eq 1
# source와 분리한16MiB 자료에서 cache invalidation을 먼저 대조한다.
admit git-cache-fixture 16
run git-cache-equivalence 120 "${runtime[@]}" /usr/bin/bash --noprofile --norc -euo pipefail -s -- "$proof/git-cache-554" <<'GIT_CACHE_554'
umask 077
base=$1
[[ $base == /* && $(realpath -m -- "$base") == "$base" ]]
available=$(df -PB1 -- "${base%/*}" | awk 'NR==2 {print $4}')
[[ $available =~ ^[0-9]+$ ]]
((available >= (16 + 512) * 1024 * 1024))
mkdir -m 700 -- "$base"
test "$(stat -c '%u:%a' -- "$base")" = "$UID:700"
test "$GIT_OPTIONAL_LOCKS:$GIT_CONFIG_COUNT:$GIT_CONFIG_KEY_0:$GIT_CONFIG_VALUE_0" = 0:1:core.preloadIndex:true
usage() {
  local bytes
  bytes=$(du -sx --block-size=1 -- "$base" | cut -f1) || return 1
  [[ $bytes =~ ^[0-9]+$ ]] || return 1
  printf '%s\n' "$bytes" >> "$base/allocated.tsv" || return 1
  ((bytes <= 16 * 1024 * 1024))
}
finish_fixture() {
  local result=$?
  trap - EXIT
  if ! usage; then if ((result == 0)); then result=1; fi; fi
  exit "$result"
}
trap finish_fixture EXIT
printf '{"available_bytes":%s,"fixture_cap_bytes":16777216,"reserve_bytes":536870912}\n' "$available" > "$base/admission.json"
mkdir -m 700 -- "$base/fixture"
cd "$base/fixture"
/usr/bin/git init --quiet --initial-branch=cache-554 --template=
mkdir nested
printf 'fixed tracked bytes\n' > tracked
printf 'fixed nested anchor\n' > nested/tracked
/usr/bin/git add -- .
GIT_AUTHOR_NAME=cache-554 GIT_AUTHOR_EMAIL=cache-554@example.invalid \
  GIT_COMMITTER_NAME=cache-554 GIT_COMMITTER_EMAIL=cache-554@example.invalid \
  /usr/bin/git commit --quiet -m 'private untracked cache fixture'
test "$(/usr/bin/git config --bool --get core.fileMode)" = true
if /usr/bin/git config --get-all core.untrackedCache > "$base/config.stdout" 2> "$base/config.stderr"; then exit 1; else test "$?" -eq 1; fi
test ! -s "$base/config.stdout" && test ! -s "$base/config.stderr"
sha256sum .git/config > "$base/config.sha256"
/usr/bin/git ls-files --stage -z > "$base/entries.before.bin"
/usr/bin/git update-index --test-untracked-cache > "$base/fs-check.stdout" 2> "$base/fs-check.stderr"
/usr/bin/git update-index --untracked-cache
GIT_OPTIONAL_LOCKS=1 /usr/bin/git status --porcelain --untracked-files=normal > "$base/initial-prime.stdout" 2> "$base/initial-prime.stderr"
test ! -s "$base/initial-prime.stdout" && test ! -s "$base/initial-prime.stderr"
matched=0
compare_status() {
  local state=$1 expected=$2 setting status before
  printf '%s' "$expected" > "$base/$state.expected"
  usage
  before=$(sha256sum .git/index | cut -d' ' -f1)
  for setting in off on prime; do
    local -a command=(/usr/bin/git status --porcelain --untracked-files=normal)
    if [[ $setting == off ]]; then command=(/usr/bin/git -c core.untrackedCache=false status --porcelain --untracked-files=normal); fi
    if GIT_OPTIONAL_LOCKS=$([[ $setting == prime ]] && printf 1 || printf 0) "${command[@]}" \
        > "$base/$state-$setting.stdout" 2> "$base/$state-$setting.stderr"; then status=0; else status=$?; fi
    printf '%s\n' "$status" > "$base/$state-$setting.exit"
    test "$status" -eq 0
    cmp "$base/$state.expected" "$base/$state-$setting.stdout"
    test ! -s "$base/$state-$setting.stderr"
    if [[ $setting != prime ]]; then test "$(sha256sum .git/index | cut -d' ' -f1)" = "$before"; fi
  done
  cmp "$base/$state-off.stdout" "$base/$state-on.stdout"
  cmp "$base/$state-off.stderr" "$base/$state-on.stderr"
  cmp "$base/$state-off.exit" "$base/$state-on.exit"
  sha256sum -- "$base/$state-"{off,on,prime}.stdout >> "$base/status.sha256"
  matched=$((matched + 1))
  usage
}
compare_status clean ''
printf 'changed tracked bytes\n' > tracked
compare_status tracked-content $' M tracked\n'
printf 'fixed tracked bytes\n' > tracked
chmod 700 tracked
compare_status tracked-mode $' M tracked\n'
chmod 600 tracked
printf 'untracked bytes\n' > untracked-only
compare_status untracked-add $'?? untracked-only\n'
rm -- untracked-only
compare_status untracked-delete ''
mkdir nested/inner
printf 'nested untracked bytes\n' > nested/inner/value
compare_status nested-add $'?? nested/inner/\n'
mv -- nested/inner nested/renamed
compare_status nested-rename $'?? nested/renamed/\n'
rm -- nested/renamed/value
rmdir -- nested/renamed
compare_status nested-remove ''
test "$matched" -eq 8
/usr/bin/git ls-files --stage -z > "$base/entries.after.bin"
cmp "$base/entries.before.bin" "$base/entries.after.bin"
sha256sum --check --strict "$base/config.sha256"
usage
printf '{"schema":1,"states":8,"status_pairs":8,"authored_status_equal":true,"fs_check_passed":true,"fixture_cap_bytes":16777216,"source_prime":false,"performance_claim":false,"runtime_acceptance":false,"retirement_credit":0}\n' > "$base/complete.json"
GIT_CACHE_554
verify_held
hold "$proof/git-cache-554/complete.json"
hold "$proof/git-cache-554/status.sha256"
context=$proof/context
run prepare-context 180 "${runtime[@]}" "$checker" prepare-context --root "$source" --archive "$archive" --output "$context"
jq -e '.schema==1 and .members==108 and .member_bytes==700585 and .current_source_files==19 and .archive_members==107 and .r11_tar_members==70 and .nested_r3_members==37 and .fixed_current_reference_source_members==1 and .historical_executable_executions==0 and .python_executions==0 and .current_native_functional_acceptance==false and .historical_pass_inherited==false and .complete_cargo_dso_closure==false and .R4_acceptance==false and .retirement_credit==false' "$context.prepare.json" > "$proof/import-admission.txt"
hold "$context.prepare.json"
context=$context/context.json
hold "$context"
verify_held
run strace-help 30 "${runtime[@]}" /usr/bin/strace --help
grep -F -- --kill-on-exit "$proof/build/strace-help.stdout" > "$proof/exitkill-supported.txt"
run retained-core27 180 "${runtime[@]}" "$unit" native_logging_check::retained::tests:: --test-threads=1 --color=never
test "$(grep -Ec '^test result: ok\. 27 passed; 0 failed; 0 ignored;' "$proof/build/retained-core27.stdout")" -eq 1
cat > "$proof/retained-core27.expected.txt" <<'NAMES'
native_logging_check::retained::tests::comparison_cannot_pass_empty_missing_reordered_or_changed_payloads
native_logging_check::retained::tests::console_bridge_replaces_only_validated_callsite_prefix_and_preserves_all_else
native_logging_check::retained::tests::context_root_must_be_private_before_missing_index_is_opened
native_logging_check::retained::tests::dangling_output_and_aliased_parent_are_rejected_without_touching_target
native_logging_check::retained::tests::directory_anchor_detects_renamed_input_parent_even_if_leaf_bytes_match
native_logging_check::retained::tests::dispatch_requires_the_first_literal_not_a_later_argument
native_logging_check::retained::tests::duplicate_manifest_keys_and_unknown_source_hooks_are_not_last_value_wins
native_logging_check::retained::tests::exact_member_roster_rejects_missing_extra_or_forged_pins
native_logging_check::retained::tests::held_input_detects_content_change_inode_replacement_and_hardlink_alias
native_logging_check::retained::tests::held_input_rejects_parent_symlink_fifo_and_over_bound_without_reading_them
native_logging_check::retained::tests::help_needs_neither_source_context_provider_nor_output
native_logging_check::retained::tests::imported_wire_validation_rejects_raw_record_level_lifetime_and_source_bypass
native_logging_check::retained::tests::known_archived_cpp_and_rust_provider_pins_are_rejected_before_launch
native_logging_check::retained::tests::member_names_reject_traversal_alias_spellings_and_executable_roles
native_logging_check::retained::tests::native_empty_response_preserves_filtered_one_instead_of_claiming_cpp_identity
native_logging_check::retained::tests::occupied_output_precedes_missing_context_provider_source_or_git
native_logging_check::retained::tests::pinned_content_is_rechecked_not_accepted_as_a_new_baseline_after_admission
native_logging_check::retained::tests::receipt_fd_cannot_be_redirected_by_renaming_and_replacing_output_directory
native_logging_check::retained::tests::retained_parser_refuses_builder_reference_and_source_hook_options
native_logging_check::retained::tests::source_policy_absent_rejects_nodes_and_keeps_reappearance_sticky
native_logging_check::retained::tests::source_policy_admission_keeps_required_help_unknown_and_occupied_priorities
native_logging_check::retained::tests::source_policy_parent_alias_replacement_and_special_leaf_fail_closed
native_logging_check::retained::tests::source_policy_present_detects_rewrite_replacement_and_hardlink_alias
native_logging_check::retained::tests::source_policy_present_markers_and_absent_root_are_guard_metadata_only
native_logging_check::retained::tests::source_policy_schema_defaults_and_rejects_unknown_values
native_logging_check::retained::tests::success_receipt_create_new_cannot_overwrite_foreign_bytes
native_logging_check::retained::tests::unlisted_symlink_or_executable_in_context_never_becomes_an_ignored_role
NAMES
sed -nE 's/^test (native_logging_check::retained::tests::[a-z0-9_]+) \.\.\. ok$/\1/p' "$proof/build/retained-core27.stdout" | LC_ALL=C sort > "$proof/retained-core27.actual.txt"
cmp "$proof/retained-core27.expected.txt" "$proof/retained-core27.actual.txt"
run prepare-controls 180 "${runtime[@]}" "$unit" native_logging_check::retained::prepare::tests:: --test-threads=1 --color=never
test "$(grep -Ec '^test result: ok\. 9 passed; 0 failed; 0 ignored;' "$proof/build/prepare-controls.stdout")" -eq 1
cat > "$proof/prepare-controls.expected.txt" <<'NAMES'
native_logging_check::retained::prepare::tests::archive_pin_rejects_wrong_size_hash_last_byte_and_truncation
native_logging_check::retained::prepare::tests::context_writer_rejects_symlink_parent_existing_leaf_and_unsafe_name
native_logging_check::retained::prepare::tests::gzip_rejects_corruption_concatenation_and_trailing_bytes
native_logging_check::retained::prepare::tests::mapping_preserves_seventy_tar_thirty_seven_zip_and_one_source
native_logging_check::retained::prepare::tests::prepare_rejects_archive_symlink_hardlink_and_occupied_receipt
native_logging_check::retained::prepare::tests::prepare_rejects_occupied_context_before_archive_or_source_reads
native_logging_check::retained::prepare::tests::tar_rejects_links_special_paths_duplicates_and_missing_selection
native_logging_check::retained::prepare::tests::zip_rejects_duplicate_unsafe_and_nonregular_members
native_logging_check::retained::prepare::tests::zip_rejects_extra_truncated_crc_and_local_path_or_descriptor_changes
NAMES
sed -nE 's/^test (native_logging_check::retained::prepare::tests::[a-z0-9_]+) \.\.\. ok$/\1/p' "$proof/build/prepare-controls.stdout" | LC_ALL=C sort > "$proof/prepare-controls.actual.txt"
cmp "$proof/prepare-controls.expected.txt" "$proof/prepare-controls.actual.txt"
run trace-controls 180 "${runtime[@]}" "$unit" native_logging_check::trace::tests:: --test-threads=1 --color=never
test "$(grep -Ec '^test result: ok\. 24 passed; 0 failed; 0 ignored;' "$proof/build/trace-controls.stdout")" -eq 1
cat > "$proof/trace-controls.expected.txt" <<'NAMES'
native_logging_check::trace::tests::ancestor_symlink_and_existing_output_cannot_redirect_or_overwrite
native_logging_check::trace::tests::dispatch_and_help_require_only_the_first_literal_and_no_files
native_logging_check::trace::tests::duplicate_pid_spelling_and_unsafe_prefix_are_rejected
native_logging_check::trace::tests::held_inputs_detect_same_length_rewrite_replacement_and_added_roster
native_logging_check::trace::tests::hex_exec_failed_attempt_and_execveat_are_data_without_policy_acceptance
native_logging_check::trace::tests::left_shift_missing_or_malformed_operands_and_boundaries_are_rejected
native_logging_check::trace::tests::left_shift_tokens_preserve_raw_spans_and_both_inspection_scopes
native_logging_check::trace::tests::left_shift_unfinished_resume_keeps_original_fragment_pairing
native_logging_check::trace::tests::line_and_sparse_file_bounds_fail_before_unbounded_reads
native_logging_check::trace::tests::malformed_cstrings_unknown_escapes_and_exec_ellipsis_are_rejected
native_logging_check::trace::tests::malformed_delimiters_nested_pending_and_post_terminal_calls_are_rejected
native_logging_check::trace::tests::malformed_timestamp_or_clock_reversal_is_rejected
native_logging_check::trace::tests::memory_network_session_calls_and_opaque_payload_are_lexical_events
native_logging_check::trace::tests::missing_newline_terminal_or_unresolved_pending_is_rejected
native_logging_check::trace::tests::model_and_report_byte_limits_reject_further_data
native_logging_check::trace::tests::orphan_wrong_operation_and_cross_pid_resumes_are_rejected
native_logging_check::trace::tests::output_seal_rejects_same_inode_rewrite_replacement_and_added_alias
native_logging_check::trace::tests::plain_exec_bytes_environment_and_raw_spans_remain_inspection_only
native_logging_check::trace::tests::raw_read_hex_arguments_and_returns_keep_source_ranges_and_opaque_body
native_logging_check::trace::tests::same_pid_resume_preserves_both_physical_fragments
native_logging_check::trace::tests::signal_cpu_comments_and_no_return_exit_are_not_successful_syscalls
native_logging_check::trace::tests::single_left_angle_and_malformed_descriptors_stay_rejected
native_logging_check::trace::tests::symlink_hardlink_fifo_and_public_directory_are_rejected
native_logging_check::trace::tests::terminal_signal_keeps_interrupted_call_aborted_and_never_successful
NAMES
sed -nE 's/^test (native_logging_check::trace::tests::[a-z0-9_]+) \.\.\. ok$/\1/p' "$proof/build/trace-controls.stdout" | LC_ALL=C sort > "$proof/trace-controls.actual.txt"
cmp "$proof/trace-controls.expected.txt" "$proof/trace-controls.actual.txt"
run summary-controls 180 "${runtime[@]}" "$unit" native_logging_check::trace::summary_tests:: --test-threads=1 --color=never
test "$(grep -Ec '^test result: ok\. 7 passed; 0 failed; 0 ignored;' "$proof/build/summary-controls.stdout")" -eq 1
cat > "$proof/summary-controls.expected.txt" <<'NAMES'
native_logging_check::trace::summary_tests::summary_cli_is_explicit_and_full_report_contract_remains_schema_one
native_logging_check::trace::summary_tests::summary_keeps_every_exec_signal_terminal_and_all_event_counts
native_logging_check::trace::summary_tests::summary_keeps_successful_and_failed_process_session_details_with_exact_counts
native_logging_check::trace::summary_tests::summary_preserves_event_retained_model_and_key_bounds
native_logging_check::trace::summary_tests::summary_rejects_malformed_unretained_calls_resumes_and_post_terminal_rows
native_logging_check::trace::summary_tests::summary_shares_held_raw_hash_and_identity_rejection
native_logging_check::trace::summary_tests::summary_validates_many_metadata_rows_without_exporting_their_details
NAMES
sed -nE 's/^test (native_logging_check::trace::summary_tests::[a-z0-9_]+) \.\.\. ok$/\1/p' "$proof/build/summary-controls.stdout" | LC_ALL=C sort > "$proof/summary-controls.actual.txt"
cmp "$proof/summary-controls.expected.txt" "$proof/summary-controls.actual.txt"
cat "$proof/retained-core27.actual.txt" "$proof/prepare-controls.actual.txt" "$proof/trace-controls.actual.txt" "$proof/summary-controls.actual.txt" > "$proof/selected-units.actual.txt"
test "$(wc -l < "$proof/selected-units.actual.txt")" -eq 67
test "$(sort -u "$proof/selected-units.actual.txt" | wc -l)" -eq 67
printf '{"schema":1,"selected_units":67,"retained_units":27,"prepare_units":9,"trace_units":24,"summary_units":7,"ignored":0,"exact_rosters":true}\n' > "$proof/selected-units.json"
hold "$proof/selected-units.json"
# Cargo가 더 이상 실행되지 않는 시점에만 owned source index의 cache metadata를 준비한다.
# raw index는 mutable이며, tracked entries/source/config/held/SDK의 불변 검사와 분리한다.
verify_held
run git-source-cache-prime 240 "${runtime[@]}" /usr/bin/bash --noprofile --norc -euo pipefail -s -- \
  "$source" "$DIAG_SOURCE_OWNER" "$DIAG_SOURCE_IDENTITY" "$CI_JOB_TEMP" "$GITHUB_SHA" \
  "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" "$CI_PREPARED_RUST_ROOT" "$CI_PREPARED_RUST_MANIFEST" "$DIAG_ACTIONS_WORKSPACE" <<'GIT_SOURCE_CACHE_554'
umask 077
root=$1; source_owner=$2; source_identity=$3; job=$4; commit=$5; owner=$6; sdk_root=$7; sdk_manifest=$8; workspace=$9
[[ $job == /tmp/openpilot-ci.?????? && $(realpath -e -- "$job") == "$job" && ! -L $job ]]
test "$(stat -c '%u:%a' -- "$job")" = "$UID:700"
test -f "$job/owner" && test ! -L "$job/owner"
test "$(cat "$job/owner")" = "$owner"
IFS=: read -r run_id attempt job_id <<< "$owner"
[[ $run_id =~ ^[0-9]+$ && $attempt =~ ^[0-9]+$ && -n $job_id ]]
test -d "$workspace" && test ! -L "$workspace"
test "$(realpath -e -- "$workspace")" = "$workspace"
[[ $source_owner == "$workspace/.logging549-source-${run_id}-${attempt}."?????? && $root == "$source_owner/source" && $PWD == "$root" ]]
test "$(stat -c '%u:%a' -- "$source_owner")" = "$UID:700"
for directory in "$source_owner" "$root" "$root/.git"; do
  test -d "$directory" && test ! -L "$directory"
  test "$(realpath -e -- "$directory")" = "$directory"
  test "$(stat -c %u -- "$directory")" = "$UID"
  mode=$(stat -c %a -- "$directory"); (( (8#$mode & 0022) == 0 ))
  test "$(stat -c %d -- "$directory")" = "$(stat -c %d -- "$root")"
done
test "$(stat -c '%d:%i' -- "$source_owner")" = "$source_identity"
test -f "$source_owner/.diag296-owner" && test ! -L "$source_owner/.diag296-owner"
test "$(cat "$source_owner/.diag296-owner")" = "$owner"
test "$GIT_OPTIONAL_LOCKS:$GIT_CONFIG_COUNT:$GIT_CONFIG_KEY_0:$GIT_CONFIG_VALUE_0" = 0:1:core.preloadIndex:true
[[ $commit =~ ^[a-f0-9]{40}$ ]]
git_dir=$root/.git
test "$(/usr/bin/git rev-parse --absolute-git-dir)" = "$git_dir"
test "$(realpath -e -- "$(/usr/bin/git rev-parse --git-common-dir)")" = "$git_dir"
index=$git_dir/index
test "$(/usr/bin/git rev-parse --path-format=absolute --git-path index)" = "$index"
test ! -e "$index.lock" && test ! -L "$index.lock"
git_identity=$(stat -c '%d:%i:%u:%g:%f' -- "$git_dir")
out=$job/proof/logging/git-source-cache-554
mkdir -m 700 -- "$out"
for path in "$git_dir/config" "$index"; do
  test -f "$path" && test ! -L "$path"
  test "$(stat -c '%u:%h:%d' -- "$path")" = "$UID:1:$(stat -c %d -- "$root")"
  mode=$(stat -c %a -- "$path"); (( (8#$mode & 0022) == 0 ))
done
exec {config_fd}< "$git_dir/config"
config_identity=$(stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "/proc/$$/fd/$config_fd")
config_hash=$(sha256sum -- "/proc/$$/fd/$config_fd" | cut -d' ' -f1)
config_seal() {
  test ! -L "$git_dir/config"
  test "$(stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "$git_dir/config")" = "$config_identity"
  test "$(stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "/proc/$$/fd/$config_fd")" = "$config_identity"
  test "$(sha256sum -- "/proc/$$/fd/$config_fd" | cut -d' ' -f1)" = "$config_hash"
  test "$(sha256sum -- "$git_dir/config" | cut -d' ' -f1)" = "$config_hash"
}
config_seal
require_unset() {
  local label=$1 status; shift
  if "$@" /usr/bin/git config --get-all core.untrackedCache > "$out/$label.stdout" 2> "$out/$label.stderr"; then status=0; else status=$?; fi
  printf '%s\n' "$status" > "$out/$label.exit"
  test "$status" -eq 1
  test ! -s "$out/$label.stdout" && test ! -s "$out/$label.stderr"
}
require_unset runtime-config
# 동결 fixture의 env_clear 뒤 실제 config lookup도 확인한다. 새 runtime 키를 전달하지 않는다.
fixture_env=(/usr/bin/env -i PATH="$PATH" HOME="$HOME" TMPDIR="$TMPDIR" TZ="$TZ" LANG="$LANG" LC_ALL="$LC_ALL" \
  GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.preloadIndex GIT_CONFIG_VALUE_0=true)
require_unset fixture-config "${fixture_env[@]}"
raw_index() {
  local label=$1 fd identity hash reader=$BASHPID
  test -f "$index" && test ! -L "$index"
  test "$(stat -c '%u:%h:%d' -- "$index")" = "$UID:1:$(stat -c %d -- "$root")"
  mode=$(stat -c %a -- "$index"); (( (8#$mode & 0022) == 0 ))
  exec {fd}< "$index"
  identity=$(stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "/proc/$reader/fd/$fd")
  test "$(stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "$index")" = "$identity"
  cp -- "/proc/$reader/fd/$fd" "$out/index-$label.bin"
  hash=$(sha256sum -- "/proc/$reader/fd/$fd" | cut -d' ' -f1)
  test "$(sha256sum -- "$out/index-$label.bin" | cut -d' ' -f1)" = "$hash"
  test "$(stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "$index")" = "$identity"
  test "$(stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "/proc/$reader/fd/$fd")" = "$identity"
  printf '%s\t%s\n' "$identity" "$hash" > "$out/index-$label.tsv"
  exec {fd}<&-
}
seals() {
  local label=$1 allocated
  test "$(stat -c '%d:%i:%u:%g:%f' -- "$git_dir")" = "$git_identity"
  config_seal
  test "$(/usr/bin/git --no-optional-locks rev-parse HEAD)" = "$commit"
  /usr/bin/git --no-optional-locks ls-files -z > "$out/files-$label.bin"
  /usr/bin/git --no-optional-locks ls-files --stage -z > "$out/entries-$label.bin"
  /usr/bin/git --no-optional-locks ls-files -v -z > "$out/flags-$label.bin"
  cmp "$job/proof/index-before.bin" "$out/files-$label.bin"
  tar --null --no-recursion --verbatim-files-from -T "$out/files-$label.bin" -cf - --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner | sha256sum > "$out/source-$label.sha256"
  cmp "$job/proof/source-before.sha256" "$out/source-$label.sha256"
  # source 내 target은 기존 독립8GiB build cap에서 검사하므로 이중 계산하지 않는다.
  allocated=$(du -sx --block-size=1 --exclude="$root/rust/target" -- "$source_owner" | cut -f1)
  [[ $allocated =~ ^[0-9]+$ ]]
  printf '%s\n' "$allocated" > "$out/source-$label.bytes"
  ((allocated <= 2 * 1024 * 1024 * 1024))
  /usr/bin/git --no-optional-locks diff --exit-code
  /usr/bin/git --no-optional-locks diff --cached --exit-code
  test "$sdk_root" = "$job/rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu"
  test "$(realpath -e -- "$sdk_root")" = "$sdk_root"
  test "$sdk_manifest" = "$job/proof/prepared-rust-files.before.sha256"
  sha256sum --check --strict --quiet "$sdk_manifest"
  LC_ALL=C find "$sdk_root" -printf '%D:%i\t%m:%U:%G\t%T@\t%C@\t%y\t%p\t%l\0' | LC_ALL=C sort -z > "$out/sdk-$label.bin"
  cmp "$job/proof/prepared-rust-roster.before.bin" "$out/sdk-$label.bin"
}
status() {
  local label=$1 code; shift
  if "$@" > "$out/$label.stdout" 2> "$out/$label.stderr"; then code=0; else code=$?; fi
  printf '%s\n' "$code" > "$out/$label.exit"
  test "$code" -eq 0
}
finish_cache() {
  local result=$?
  trap - EXIT
  if ((result != 0)); then
    # mutation 이후 실패해도 raw index 사후 관측을 시도하며 원래 실패를 보존한다.
    set +e
    (set -e; raw_index failed) > "$out/failed-index.stdout" 2> "$out/failed-index.stderr"
    printf '%s\n' "$?" > "$out/failed-index.exit"
  fi
  exit "$result"
}
trap finish_cache EXIT
seals before
raw_index before
before_hash=$(sha256sum -- "$index" | cut -d' ' -f1)
status cache-off /usr/bin/git -c core.untrackedCache=false status --porcelain --untracked-files=normal
test ! -s "$out/cache-off.stdout" && test ! -s "$out/cache-off.stderr"
test "$(sha256sum -- "$index" | cut -d' ' -f1)" = "$before_hash"
/usr/bin/git update-index --test-untracked-cache > "$out/fs-check.stdout" 2> "$out/fs-check.stderr"
/usr/bin/git update-index --untracked-cache > "$out/enable.stdout" 2> "$out/enable.stderr"
# sourceではこれだけが選択的index書き込みを許すstatusである。
status prime /usr/bin/env GIT_OPTIONAL_LOCKS=1 /usr/bin/git status --porcelain --untracked-files=normal
raw_index primed
primed_hash=$(sha256sum -- "$index" | cut -d' ' -f1)
status cache-on /usr/bin/git status --porcelain --untracked-files=normal
for channel in stdout stderr exit; do
  cmp "$out/cache-off.$channel" "$out/prime.$channel"
  cmp "$out/cache-off.$channel" "$out/cache-on.$channel"
done
test "$(sha256sum -- "$index" | cut -d' ' -f1)" = "$primed_hash"
test ! -e "$index.lock" && test ! -L "$index.lock"
require_unset runtime-config-after
require_unset fixture-config-after "${fixture_env[@]}"
seals after
raw_index after
for roster in files entries flags; do cmp "$out/$roster-before.bin" "$out/$roster-after.bin"; done
printf '{"schema":1,"source_commit":"%s","fs_check_passed":true,"source_prime_statuses":1,"status_channels_equal":true,"tracked_source_entries_flags_equal":true,"source_config_unchanged":true,"sdk_equal":true,"raw_index_immutable":false,"fixture_child_optional_locks_zero":false,"performance_claim":false,"runtime_acceptance":false,"retirement_credit":0}\n' "$commit" > "$out/complete.json"
GIT_SOURCE_CACHE_554
verify_held
hold "$proof/git-source-cache-554/complete.json"
export NATIVE_LOGGING_RETAINED_BINARY=$probe NATIVE_LOGGING_RETAINED_CONTEXT=$context
export NATIVE_LOGGING_RETAINED_SOURCE_COMMIT=$GITHUB_SHA
export NATIVE_LOGGING_RETAINED_BINARY_SHA256 NATIVE_LOGGING_RETAINED_CONTEXT_SHA256
NATIVE_LOGGING_RETAINED_BINARY_SHA256=$(sha256sum "$probe" | cut -d' ' -f1)
NATIVE_LOGGING_RETAINED_CONTEXT_SHA256=$(sha256sum "$context" | cut -d' ' -f1)
export NATIVE_LOGGING_LIFECYCLE_EVIDENCE=$proof/lifecycle
lifecycle_env=(NATIVE_LOGGING_RETAINED_BINARY="$NATIVE_LOGGING_RETAINED_BINARY" NATIVE_LOGGING_RETAINED_CONTEXT="$NATIVE_LOGGING_RETAINED_CONTEXT" NATIVE_LOGGING_RETAINED_SOURCE_COMMIT="$NATIVE_LOGGING_RETAINED_SOURCE_COMMIT" NATIVE_LOGGING_RETAINED_BINARY_SHA256="$NATIVE_LOGGING_RETAINED_BINARY_SHA256" NATIVE_LOGGING_RETAINED_CONTEXT_SHA256="$NATIVE_LOGGING_RETAINED_CONTEXT_SHA256" NATIVE_LOGGING_LIFECYCLE_EVIDENCE="$NATIVE_LOGGING_LIFECYCLE_EVIDENCE" "${source_policy_env[@]}")
monotonic_cs() {
  local stamp unused whole fraction
  read -r stamp unused < /proc/uptime
  [[ $stamp =~ ^[0-9]+\.[0-9]{2}$ ]]
  whole=${stamp%.*}; fraction=${stamp#*.}
  printf '%s\n' "$((10#$whole * 100 + 10#$fraction))"
}
# #554 A: provider와 source를 그대로 두고 계측 유무만 관측한다.
paired_bound() {
  local remaining bound_cs now
  now=$(monotonic_cs) || return 1
  [[ $now =~ ^[0-9]+$ ]] || return 1
  remaining=$((60000 - (now - started)))
  ((remaining > 0)) || return 1
  bound_cs=$remaining; ((bound_cs <= 9500)) || bound_cs=9500
  printf '%s.%02d' "$((bound_cs / 100))" "$((bound_cs % 100))"
}
paired_snapshot() (
  set -e
  ulimit -f 524288
  local side=$1 moment=$2 destination reader=$BASHPID index_fd identity hash path
  destination=$proof/paired-554/$side/$moment
  admit "paired-$side-$moment" 512
  verify_held
  test "$PWD" = "$source"
  test "$(realpath -e -- "$source")" = "$source" && test ! -L "$source"
  test "$(stat -c '%d:%i' -- "$DIAG_SOURCE_OWNER")" = "$DIAG_SOURCE_IDENTITY"
  test "$(cat "$DIAG_SOURCE_OWNER/.diag296-owner")" = "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB"
  if [[ $side == untraced ]]; then
    printf '%s\0' "${runtime[@]}" "${untraced_env[@]}" > "$destination.environment.argv0"
  elif [[ $side == git-false-a1 || $side == git-true-b || $side == git-false-a2 ]]; then
    printf '%s\0' "${git_preload_runtime[@]}" > "$destination.environment.argv0"
  else
    test "$side" = traced
    printf '%s\0' "${runtime[@]}" "${lifecycle_env[@]}" > "$destination.environment.argv0"
  fi
  cmp "$proof/paired-554/$side/environment.argv0" "$destination.environment.argv0"
  test -d "$source/.git" && test ! -L "$source/.git"
  test "$(realpath -e -- "$source/.git")" = "$source/.git"
  test "$("${runtime[@]}" /usr/bin/timeout 10s /usr/bin/git --no-optional-locks rev-parse HEAD)" = "$GITHUB_SHA"
  "${runtime[@]}" /usr/bin/timeout 10s /usr/bin/git --no-optional-locks ls-files -z > "$destination.files.bin"
  "${runtime[@]}" /usr/bin/timeout 10s /usr/bin/git --no-optional-locks ls-files --stage -z > "$destination.entries.bin"
  "${runtime[@]}" /usr/bin/timeout 10s /usr/bin/git --no-optional-locks ls-files -v -z > "$destination.flags.bin"
  cmp "$CI_JOB_TEMP/proof/index-before.bin" "$destination.files.bin"
  for path in files entries flags; do
    cmp "$proof/git-source-cache-554/$path-after.bin" "$destination.$path.bin"
  done
  /usr/bin/timeout 30s tar --null --no-recursion --verbatim-files-from -T "$destination.files.bin" -cf - --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner | sha256sum > "$destination.source.sha256"
  cmp "$CI_JOB_TEMP/proof/source-before.sha256" "$destination.source.sha256"
  stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "$source" "$source/.git" "$source/.git/config" > "$destination.source-config.identity"
  sha256sum -- "$source/.git/config" > "$destination.config.sha256"
  for path in "$fixture" "$checker" "$probe" "$context"; do
    printf '%s\t' "$path"
    stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "$path"
    sha256sum -- "$path"
  done > "$destination.inputs.txt"
  # Git는 cache/stat metadata를 갱신할 수 있다. raw index는 각 시점만 안정적으로 읽는다.
  path=$source/.git/index
  test -f "$path" && test ! -L "$path"
  test "$(stat -c '%u:%h:%d' -- "$path")" = "$UID:1:$(stat -c %d -- "$source")"
  test ! -e "$source/.git/index.lock" && test ! -L "$source/.git/index.lock"
  exec {index_fd}< "$path"
  identity=$(stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "/proc/$reader/fd/$index_fd")
  test "$(stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "$path")" = "$identity"
  cp -- "/proc/$reader/fd/$index_fd" "$destination.index.bin"
  hash=$(sha256sum -- "/proc/$reader/fd/$index_fd" | cut -d' ' -f1)
  test "$(sha256sum -- "$destination.index.bin" | cut -d' ' -f1)" = "$hash"
  test "$(stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "$path")" = "$identity"
  test "$(stat -Lc '%d:%i:%u:%g:%f:%h:%s:%y:%z' -- "/proc/$reader/fd/$index_fd")" = "$identity"
  printf '%s\t%s\n' "$identity" "$hash" > "$destination.index.tsv"
  exec {index_fd}<&-
  test "$(du -sx --block-size=1 --exclude="$source/rust/target" -- "$DIAG_SOURCE_OWNER" | cut -f1)" -le $((2 * 1024 * 1024 * 1024))
  verify_held
  admit "paired-$side-$moment-after" 0
)
mkdir -m 700 "$proof/paired-554" "$proof/paired-554/untraced" "$proof/paired-554/traced" "$proof/paired-554/untraced/evidence"
hold "$source/.git/config"
untraced_env=("${lifecycle_env[@]}")
test "${#untraced_env[@]}" -eq 6
test "${untraced_env[5]}" = "NATIVE_LOGGING_LIFECYCLE_EVIDENCE=$proof/lifecycle"
untraced_env[5]="NATIVE_LOGGING_LIFECYCLE_EVIDENCE=$proof/paired-554/untraced/evidence"
printf '%s\0' "${runtime[@]}" "${untraced_env[@]}" > "$proof/paired-554/untraced/environment.argv0"
printf '%s\0' "${runtime[@]}" "${lifecycle_env[@]}" > "$proof/paired-554/traced/environment.argv0"
untraced_command=("${runtime[@]}" "${untraced_env[@]}" "$fixture" --ignored --exact retained::runtime_directory_int --test-threads=1 --color=never)
printf '%s\0' "${untraced_command[@]}" > "$proof/paired-554/untraced/command.argv0"
# #554 A/B/A 진단과 fixed-true qualification은 별개다. 세 관측은 각1회이며 빠른 값 선택은 없다.
git_preload_slots=(false-a1 true-b false-a2)
mkdir -m 700 "$proof/paired-554/git-false-a1" "$proof/paired-554/git-true-b" "$proof/paired-554/git-false-a2" "$proof/git-preload-status-554"
mkdir -m 700 "$proof/git-preload-status-554/qa"
elf git-preload-git /usr/bin/git
elf git-preload-strace /usr/bin/strace
grep -F -- --argv0 "$proof/build/strace-help.stdout" > "$proof/git-preload-status-554/argv0-supported.txt"
for slot in "${git_preload_slots[@]}"; do
  case $slot in false-a1|false-a2) setting=false;; true-b) setting=true;; *) exit 1;; esac
  # env_clear 뒤 실제 checker/Git의6+3키 및 QA context만 복제한다. OPTIONAL_LOCKS=0을 새로 넣지 않는다.
  git_preload_runtime=(/usr/bin/env -i PATH=/usr/bin:/bin HOME="$HOME" TMPDIR="$TMPDIR" LANG=C.UTF-8 LC_ALL=C.UTF-8 TZ=UTC
    GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.preloadIndex GIT_CONFIG_VALUE_0="$setting"
    NATIVE_LOGGING_QA_HOLD=runtime NATIVE_LOGGING_QA_RECORDS="$proof/git-preload-status-554/qa")
  printf '%s\0' "${git_preload_runtime[@]}" > "$proof/paired-554/git-$slot/environment.argv0"
  printf '%s\0' "${git_preload_runtime[@]:2}" | jq -Rs 'split("\u0000")[:-1]' > "$proof/paired-554/git-$slot/expected-environment.json"
  run "git-preload-$slot" 10 "${git_preload_runtime[@]}" /usr/bin/strace --argv0=git --seccomp-bpf --kill-on-exit \
    -ff -ttt -x -v -s 65536 -e trace=process,%file,%desc,%signal,%network,%memory,setsid,setpgid,prctl,unshare,setns \
    -e raw=read,pread64 -o "$proof/trace/git-preload-$slot" /usr/bin/git status --porcelain --untracked-files=normal
done
phase=git-preload-channels
for channel in stdout stderr; do
  cmp "$proof/build/git-preload-false-a1.$channel" "$proof/build/git-preload-true-b.$channel"
  cmp "$proof/build/git-preload-false-a1.$channel" "$proof/build/git-preload-false-a2.$channel"
  # stderr에는 tracer 진단도 모인다. nonempty이면 분리/정규화해 허용하지 않는다.
  for slot in "${git_preload_slots[@]}"; do test ! -s "$proof/build/git-preload-$slot.$channel"; done
done
cmp "$proof/paired-554/git-false-a1/command.exit" "$proof/paired-554/git-true-b/command.exit"
cmp "$proof/paired-554/git-false-a1/command.exit" "$proof/paired-554/git-false-a2/command.exit"
phase=git-preload-index-bind
same_raw_index_bytes=true
same_raw_index_identity=true
for slot in "${git_preload_slots[@]}"; do
  for moment in before after; do
    if cmp -s -- "$proof/paired-554/git-false-a1/before.index.bin" "$proof/paired-554/git-$slot/$moment.index.bin"; then :; else
      compare_status=$?; ((compare_status == 1)); same_raw_index_bytes=false
    fi
    if cmp -s -- "$proof/paired-554/git-false-a1/before.index.tsv" "$proof/paired-554/git-$slot/$moment.index.tsv"; then :; else
      compare_status=$?; ((compare_status == 1)); same_raw_index_identity=false
    fi
  done
done
comparison_disposition=HOLD
if [[ $same_raw_index_bytes == true && $same_raw_index_identity == true ]]; then comparison_disposition=eligible_observation_only; fi
# raw index는 복원하지 않는다. 각 read의 불변 검사는 필수이며 slot 간 차이는 성능 비교 HOLD다.
printf '{"schema":1,"same_raw_index_bytes":%s,"same_raw_index_identity":%s,"performance_comparison":"%s","index_restored":false}\n' \
  "$same_raw_index_bytes" "$same_raw_index_identity" "$comparison_disposition" > "$proof/git-preload-status-554/raw-index-comparison.json"
hold "$proof/git-preload-status-554/raw-index-comparison.json"
for slot in "${git_preload_slots[@]}"; do
  case $slot in false-a1|false-a2) setting=false;; true-b) setting=true;; *) exit 1;; esac
  run "inspect-git-preload-$slot" 180 "${runtime[@]}" "$checker" inspect-trace-summary --directory "$proof/trace" \
    --prefix "git-preload-$slot" --output "$proof/git-preload-status-554/$slot.summary.json"
  phase=git-preload-$slot-bind
  jq -e --arg root "$source" --arg source "$GITHUB_SHA" --arg setting "$setting" --arg slot "$slot" \
    --slurpfile expected "$proof/paired-554/git-$slot/expected-environment.json" \
    --slurpfile elapsed "$proof/paired-554/git-$slot/elapsed.json" '
    if (.schema==2 and .raw_immutable==true and .requires_exit_status_zero==true and
      .full_event_index==false and .lexically_checked_all_events==true and .event_count>0 and
      .event_count<=1000000 and .physical_lines>=.event_count and .physical_lines<=2000000 and
      ((.event_counts|to_entries|map(.value)|add)==.event_count) and .closed_pid==false and
      .closed_cwd==false and .closed_fd==false and .closed_dso==false and .full_acceptance==false and
      .retirement_credit==false) then . else error("Git trace summary scope differs") end |
    [.events[] | select(.exec!=null)] as $attempts |
    if ($attempts|length)==1 then . else error("Git exec attempt roster differs") end |
    $attempts[0] as $call |
    if ($call.operation=="execve" and $call.outcome=="exec_succeeded" and
      $call.exec.program.bytes==("/usr/bin/git"|explode) and
      ($call.exec.argv|map(.bytes|implode))==["git","status","--porcelain","--untracked-files=normal"] and
      $call.exec.environment.complete==true and
      ($call.exec.environment.entries|map(.bytes|implode)|sort)==($expected[0]|sort))
      then . else error("Git actual argv/environment differs") end |
    [.events[] | select(.pid==$call.pid and .kind=="terminal_exit")] as $terminal |
    if ($terminal|length)==1 then . else error("Git normal terminal missing") end |
    {schema:1,source_commit:$source,root:$root,slot:$slot,preload_index:$setting,command_exit:0,
     exec_pid:$call.pid,exec_timestamp:$call.source[0].timestamp,terminal_timestamp:$terminal[0].source[0].timestamp,
     exec_to_terminal_seconds:(($terminal[0].source[0].timestamp|tonumber)-($call.source[0].timestamp|tonumber)),
     collector_elapsed:$elapsed[0],raw_files:.raw_files,selected_raw_bytes:.selected_raw_bytes,
     event_count:.event_count,physical_lines:.physical_lines,event_counts:.event_counts,
     diagnostic_only:true,bpf_activation_proven:false,full_event_index:false,closed_pid:false,closed_cwd:false,
     closed_fd:false,closed_dso:false,R4_acceptance:false,runtime_acceptance:false,retirement_credit:0}' \
    "$proof/git-preload-status-554/$slot.summary.json" > "$proof/git-preload-status-554/$slot.observation.json"
done
phase=git-preload-aba-bind
jq -s -e --slurpfile index "$proof/git-preload-status-554/raw-index-comparison.json" '
    def delta($left;$right):
      (($left.event_counts|keys)+($right.event_counts|keys)|unique) as $keys |
      {collector_elapsed_cs:($right.collector_elapsed.elapsed_cs-$left.collector_elapsed.elapsed_cs),
       exec_to_terminal_seconds:($right.exec_to_terminal_seconds-$left.exec_to_terminal_seconds),
       event_count:($right.event_count-$left.event_count),
       event_counts:(reduce $keys[] as $key ({}; .[$key]=(($right.event_counts[$key]//0)-($left.event_counts[$key]//0))))};
    . as $observations |
    if (length==3 and (map(.slot)==["false-a1","true-b","false-a2"]) and
        (map(.preload_index)==["false","true","false"]) and
        all(.[]; .source_commit==$observations[0].source_commit and .root==$observations[0].root)) then
    {schema:2,source_commit:.[0].source_commit,root:.[0].root,order:["false-a1","true-b","false-a2"],
     preload_order:["false","true","false"],status_observations:3,status_comparisons:2,
     status_channels_equal:true,observations:.,raw_index_comparison:$index[0],diagnostic_only:true,
     qualification_profile:"fixed-true-554-v1",qualification_preload_index:"true",qualification_profile_changed:true,
     historical_false_receipts_relabelled:false,production_preload_changed:false,full_lifecycle_causality_proven:false,
     true_b_minus_false_a1:delta(.[0];.[1]),true_b_minus_false_a2:delta(.[2];.[1]),false_a2_minus_false_a1:delta(.[0];.[2]),
     performance_claim:false,root_cause_fixed:false,runtime_acceptance:false,R4_acceptance:false,retirement_credit:0}
    else error("Git diagnostic A/B/A differs") end' "$proof/git-preload-status-554/"{false-a1,true-b,false-a2}.observation.json \
    > "$proof/git-preload-status-554/complete.json"
verify_held
hold "$proof/git-preload-status-554/complete.json"
admit git-preload-observed 0
# read/pread64는 FD/주소/길이/결과를 raw hex로 유지한다. 반복 해시의 대형 버퍼 사본만 펼치지 않는다.
# 입력·packet 원문은 별도 context/실제 결과에서 봉인한다. exec/argv/path 및 mapping은 계속 해석 출력한다.
# -x는 비ASCII 문자열의 byte escape를 유지한다. FD 번호·실제 path/metadata 인자는 그대로 두고
# -yy의 반복 파생 FD-path/protocol annotation만 생략한다. closed FD/cwd/DSO 수용은 계속 false다.
started=$(monotonic_cs)
phase=lifecycle-runtime_directory_int-untraced
paired_active=untraced
paired_snapshot untraced before
bound=$(paired_bound)
printf '%s\n' "$bound" > "$proof/paired-554/untraced/bound-seconds.txt"
set +e
(set -e; ulimit -f 524288; /usr/bin/timeout --signal=TERM --kill-after=30s "${bound}s" "${untraced_command[@]}") > "$proof/build/lifecycle-runtime_directory_int-untraced.stdout" 2> "$proof/build/lifecycle-runtime_directory_int-untraced.stderr"
untraced_status=$?
set -e
printf '%s\n' "$untraced_status" > "$proof/paired-554/untraced/command.exit"
# timeout/실행 불가/신호는 일반 test 실패와 구분해 즉시 전파한다. EXIT가 사후 관측을 시도한다.
if ((untraced_status >= 124)); then exit "$untraced_status"; fi
paired_snapshot untraced after
paired_active=
if ((untraced_status == 0)); then
  test "$(grep -Ec '^test result: ok\. 1 passed; 0 failed; 0 ignored;' "$proof/build/lifecycle-runtime_directory_int-untraced.stdout")" -eq 1
fi
for name in first_int first_term first_eof runtime_directory_int runtime_directory_term escaped_descendant_int escaped_descendant_term escaped_descendant_eof output_file output_directory output_live_directory output_symlink output_dangling_symlink forbidden_executable_options; do
  remaining=$((60000 - ($(monotonic_cs) - started)))
  ((remaining > 0))
  bound_cs=$remaining; ((bound_cs <= 9500)) || bound_cs=9500
  printf -v bound '%s.%02d' "$((bound_cs / 100))" "$((bound_cs % 100))"
  run "lifecycle-$name" "$bound" "${runtime[@]}" "${lifecycle_env[@]}" /usr/bin/strace --seccomp-bpf --kill-on-exit -ff -ttt -x -v -s 65536 -e trace=process,%file,%desc,%signal,%network,%memory,setsid,setpgid,prctl,unshare,setns -e raw=read,pread64 -o "$proof/trace/$name" "$fixture" --ignored --exact "retained::$name" --test-threads=1 --color=never
  test "$(grep -Ec '^test result: ok\. 1 passed; 0 failed; 0 ignored;' "$proof/build/lifecycle-$name.stdout")" -eq 1
  verify_held
done
(( $(monotonic_cs) - started <= 60000 ))
run retained-once 360 "${runtime[@]}" /usr/bin/strace --seccomp-bpf --kill-on-exit -ff -ttt -x -v -s 65536 -e trace=process,%file,%desc,%signal,%network,%memory,setsid,setpgid,prctl,unshare,setns -e raw=read,pread64 -o "$proof/trace/retained-once" "$checker" retained --root "$source" --binary "$probe" --context "$context" --output "$proof/native" --source-commit "$GITHUB_SHA" --binary-sha256 "$NATIVE_LOGGING_RETAINED_BINARY_SHA256" "${source_policy_args[@]}"
jq -e --arg source_policy "$source_policy" --arg source "$GITHUB_SHA" '.source_policy==$source_policy and .fresh_native_packets==250 and .fresh_native_responses==697 and .retained_cpp_packets==249 and .retained_cpp_responses==694 and .cpp_executions==0 and .source_commit==$source and .R4_acceptance==false and .lifecycle_reuse_granted==false and (.historical_to_recovered_console|length)==7 and (.recovered_to_fresh_console|length)==7 and all(.historical_to_recovered_console[]; .projected_equal==true) and all(.recovered_to_fresh_console[]; .projected_equal==true)' "$proof/native/report.json" > "$proof/raw-comparison.txt"
for stem in providers context-inputs source-state; do cmp "$proof/native/$stem.before.json" "$proof/native/$stem.after.json"; done
# 정책 expected는 raw exec에서 복사하지 않는다. source command 배열과 fixture가 실제
# assertion 뒤 create_new한 작은 receipt/invocation만 현재 held provider에 연결한다.
# paired untraced1의 evidence는 별도 root다. 아래16개는 traced14 logical selector만의 receipt다.
phase=exec-policy-binding
policy=$source/tools/ci/logging_retained_exec_policy_549.jq
[[ -f $policy && ! -L $policy && $(realpath -e -- "$policy") == "$policy" ]]
mkdir -m 700 -- "$proof/exec-policy"
policy_json() {
  local path=$1 fd
  [[ $path == "$proof/"* && -f $path && ! -L $path && $(realpath -e -- "$path") == "$path" ]]
  test "$(stat -c '%u:%h' -- "$path")" = "$UID:1"
  (($(stat -c %s -- "$path") <= 1048576))
  hold "$path"
  fd=${held_fds[${#held_fds[@]}-1]}
  /usr/bin/jq -ce . "/proc/$$/fd/$fd"
}
policy_output() {
  local root=$1 full=$2 destination=$3 role
  local -a roles=(console-0/rust)
  if [[ $full == true ]]; then
    roles=(console-0/rust console-1/rust console-2/rust console-3/rust console-4/rust console-5/rust console-6/rust rate/rust transport/rust/backpressure transport/rust/threads transport/rust/console-failure transport/rust/SIGINT transport/rust/SIGTERM transport/runtime-prefix)
  fi
  : > "$destination.invocations.jsonl"
  for role in "${roles[@]}"; do
    policy_json "$root/$role/invocation.json" > "$destination.value.json"
    /usr/bin/jq -c --arg role "$role" '{role:$role,value:.}' "$destination.value.json" >> "$destination.invocations.jsonl"
  done
  if [[ $full == true ]]; then policy_json "$root/transport/runtime-endpoint.json" > "$destination.runtime.json";
  else printf 'null\n' > "$destination.runtime.json"; fi
  policy_json "$root/providers.before.json" > "$destination.provider.json"
  /usr/bin/jq -cn --slurpfile inv "$destination.invocations.jsonl" --slurpfile runtime "$destination.runtime.json" \
    --slurpfile provider "$destination.provider.json" '{invocations:$inv,runtime_endpoint:$runtime[0],provider_before:$provider[0]}' > "$destination.json"
}
find -P "$proof/lifecycle" -mindepth 1 -maxdepth 1 -printf '%p\n' | LC_ALL=C sort > "$proof/exec-policy/roots.before.txt"
test "$(wc -l < "$proof/exec-policy/roots.before.txt")" -eq 16
: > "$proof/exec-policy/cases.jsonl"
while IFS= read -r root; do
  [[ $root == "$proof/lifecycle/logging-lifecycle-"?????? && ${root##*/} =~ ^logging-lifecycle-[A-Za-z0-9]{6}$ ]]
  [[ -d $root && ! -L $root && $(realpath -e -- "$root") == "$root" ]]
  test "$(stat -c '%u:%a' -- "$root")" = "$UID:700"
  identity=$(stat -c '%d:%i:%f:%u:%g' -- "$root")
  receipts=()
  for leaf in retained-case.json retained-collision.json retained-control.json; do
    if [[ -e $root/$leaf || -L $root/$leaf ]]; then receipts+=("$root/$leaf"); fi
  done
  test "${#receipts[@]}" -eq 1
  base=$proof/exec-policy/${root##*/}
  policy_json "${receipts[0]}" > "$base.receipt.json"
  if [[ ${receipts[0]} == */retained-case.json ]]; then
    full=$(/usr/bin/jq -er '.hold|if .=="runtime" then "true" elif .=="first" then "false" else error("unknown hold") end' "$base.receipt.json")
    policy_output "$root/output" "$full" "$base"
  else printf '{"invocations":[],"runtime_endpoint":null,"provider_before":null}\n' > "$base.json"; fi
  /usr/bin/jq -cn --arg root "$root" --slurpfile receipt "$base.receipt.json" --slurpfile observed "$base.json" \
    '$observed[0]+{root:$root,receipt:$receipt[0]}' >> "$proof/exec-policy/cases.jsonl"
  test "$(stat -c '%d:%i:%f:%u:%g' -- "$root")" = "$identity"
done < "$proof/exec-policy/roots.before.txt"
find -P "$proof/lifecycle" -mindepth 1 -maxdepth 1 -printf '%p\n' | LC_ALL=C sort > "$proof/exec-policy/roots.after.txt"
cmp "$proof/exec-policy/roots.before.txt" "$proof/exec-policy/roots.after.txt"
# 고유16 logical case. 별도 untraced receipt로 누락을 채우거나 key 중복을 skip하지 않는다.
/usr/bin/jq -se '
  map(.receipt.case_key)|sort==(["retained-first-int","retained-first-term","retained-first-eof",
  "retained-runtime-directory-int","retained-runtime-directory-term",
  "retained-escaped-descendant-int","retained-escaped-descendant-term","retained-escaped-descendant-eof",
  "retained-output-file","retained-output-directory","retained-output-live-directory","retained-output-symlink",
  "retained-output-dangling-symlink","retained-reject-builder","retained-reject-reference","retained-reject-source-hook"]|sort)
  ' "$proof/exec-policy/cases.jsonl" > "$proof/exec-policy/roster.check"
policy_output "$proof/native" true "$proof/exec-policy/native"
printf '%s\0' "${runtime[@]:2}" "${lifecycle_env[@]}" | /usr/bin/jq -Rs 'split("\u0000")[:-1]' > "$proof/exec-policy/fixture-env.json"
printf '%s\0' "${runtime[@]:2}" | /usr/bin/jq -Rs 'split("\u0000")[:-1]' > "$proof/exec-policy/direct-env.json"
printf '%s\0' PATH=/usr/bin:/bin HOME="$HOME" TMPDIR="$TMPDIR" TZ=UTC LANG=C.UTF-8 LC_ALL=C.UTF-8 \
  GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.preloadIndex GIT_CONFIG_VALUE_0=true |
  /usr/bin/jq -Rs 'split("\u0000")[:-1]' > "$proof/exec-policy/checker-env.json"
for pair in "probe:$probe" "checker:$checker"; do
  label=${pair%%:*}; path=${pair#*:}
  [[ ! -L $path && $(realpath -e -- "$path") == "$path" ]]
  read -r dev ino size owner mode < <(stat -c '%d %i %s %u %f' -- "$path")
  /usr/bin/jq -cn --arg path "$path" --arg sha "$(sha256sum -- "$path" | cut -d' ' -f1)" \
    --argjson dev "$dev" --argjson ino "$ino" --argjson size "$size" --argjson uid "$owner" --argjson mode "$((16#$mode))" \
    '{path:$path,dev:$dev,ino:$ino,size:$size,uid:$uid,mode:$mode,sha256:$sha}' > "$proof/exec-policy/$label.json"
done
test "$(realpath -e -- "$fixture")" = "$fixture"
test "$(readlink -e -- /usr/bin/git)" = /usr/bin/git
policy_json "$proof/native/report.json" > "$proof/exec-policy/native.report.json"
/usr/bin/jq -cn --arg source "$source" --arg proof "$proof" --arg checker "$checker" --arg fixture "$fixture" --arg probe "$probe" \
  --arg context "$context" --arg commit "$GITHUB_SHA" --arg binary_sha "$NATIVE_LOGGING_RETAINED_BINARY_SHA256" \
  --arg source_policy "$source_policy" --arg context_sha "$NATIVE_LOGGING_RETAINED_CONTEXT_SHA256" --argjson uid "$UID" \
  --slurpfile cases "$proof/exec-policy/cases.jsonl" --slurpfile native "$proof/exec-policy/native.json" \
  --slurpfile fixture_env "$proof/exec-policy/fixture-env.json" --slurpfile direct_env "$proof/exec-policy/direct-env.json" \
  --slurpfile checker_env "$proof/exec-policy/checker-env.json" --slurpfile provider "$proof/exec-policy/probe.json" \
  --slurpfile native_report "$proof/exec-policy/native.report.json" \
  --slurpfile checker_provider "$proof/exec-policy/checker.json" '
  {schema:1,source_policy:$source_policy,source:$source,proof:$proof,checker:$checker,fixture:$fixture,probe:$probe,git:"/usr/bin/git",
   context:$context,commit:$commit,binary_sha:$binary_sha,context_sha:$context_sha,uid:$uid,
   fixture_env:$fixture_env[0],direct_env:$direct_env[0],checker_env:$checker_env[0],cases:$cases,native:$native[0],
   native_report:$native_report[0],providers:{probe:$provider[0],checker:$checker_provider[0]}}' > "$proof/exec-policy/binding.json"
verify_held
hold "$proof/exec-policy/binding.json"
# 같은 lexer로 모든 원문 줄을 검사하되 세부 syscall Event는 집계로 출력한다.
# 전체 raw는 그대로 보존한다. full event index와 PID/cwd/FD/DSO 정책 수용은 아니다.
mkdir "$proof/inspect"
for name in first_int first_term first_eof runtime_directory_int runtime_directory_term escaped_descendant_int escaped_descendant_term escaped_descendant_eof output_file output_directory output_live_directory output_symlink output_dangling_symlink forbidden_executable_options retained-once; do
  run "inspect-$name" 180 "${runtime[@]}" "$checker" inspect-trace-summary --directory "$proof/trace" --prefix "$name" --output "$proof/inspect/$name.json"
  jq -e '.schema==2 and .raw_immutable==true and .requires_exit_status_zero==true and .full_event_index==false and .lexically_checked_all_events==true and .event_count>0 and .event_count<=1000000 and .physical_lines>=.event_count and .physical_lines<=2000000 and ((.event_counts|to_entries|map(.value)|add)==.event_count) and .closed_pid==false and .closed_cwd==false and .closed_fd==false and .closed_dso==false and .full_acceptance==false and .retirement_credit==false' "$proof/inspect/$name.json" > "$proof/inspect/$name.scope.txt"
  hold "$proof/inspect/$name.json"
  run "exec-policy-$name" 60 "${runtime[@]}" /usr/bin/jq -e --arg source_policy "$source_policy" --arg prefix "$name" --slurpfile binding "$proof/exec-policy/binding.json" \
    -f "$policy" "$proof/inspect/$name.json"
  mv -- "$proof/build/exec-policy-$name.stdout" "$proof/exec-policy/$name.json"
  hold "$proof/exec-policy/$name.json"
  if [[ $name == first_int ]]; then
    # 실제 positive report의 한 요소만 바꾼 음성이다. raw/expected/원문 fixture는 건드리지 않는다.
    for control in missing-structure wrong-argv wrong-env extra-exec; do
      case "$control" in
        missing-structure)
          mutation='.events|=map(select(.operation as $op|(["fork","vfork","clone","clone3","setpgid","setsid","unshare","setns"]|index($op))==null))'
          expected_error='process/session detail cardinality differs' ;;
        wrong-argv)
          mutation='(.events[]|select(.exec.program.bytes==($checker|explode))|.exec.argv)+=[{bytes:[120],source:{start:0,end:1}}]'
          expected_error='checker source argv differs' ;;
        wrong-env)
          mutation='(.events[]|select(.exec.program.bytes==($checker|explode))|.exec.environment.entries)+=[{bytes:[88,61,49],source:{start:0,end:3}}]'
          expected_error='ordered exec/argv/env/terminal/creation differs' ;;
        extra-exec)
          mutation='.events+=[.events[]|select(.exec.program.bytes==($checker|explode))]'
          expected_error='unsupported exec attempt' ;;
      esac
      /usr/bin/jq --arg checker "$checker" "$mutation" "$proof/inspect/$name.json" > "$proof/exec-policy/control-$control.input.json"
      # run 함수는 조건문 안에서 호출하지 않는다. 실제 exit5+해당 guard의 오류를 함께 요구한다.
      run "exec-policy-control-$control" 60 "${runtime[@]}" /usr/bin/bash --noprofile --norc -euo pipefail -c '
        status=0
        /usr/bin/jq -e --arg source_policy "$6" --arg prefix first_int --slurpfile binding "$1" -f "$2" "$3" > "$4.stdout" 2> "$4.stderr" || status=$?
        printf "%s\n" "$status" > "$4.exit"
        test "$status" -eq 5 && test ! -s "$4.stdout"
        grep -F -- "$5" "$4.stderr"
      ' policy-control "$proof/exec-policy/binding.json" "$policy" "$proof/exec-policy/control-$control.input.json" \
        "$proof/exec-policy/control-$control" "$expected_error" "$source_policy"
    done
    printf '{"schema":1,"controls":4,"each_guard_exact_error":true,"actual_raw_changed":false,"R4_acceptance":false}\n' > "$proof/exec-policy/controls.json"
    hold "$proof/exec-policy/controls.json"
    source_policy_controls first_int "$proof/exec-policy/binding.json" "$proof/inspect/first_int.json" "$proof/exec-policy/source-policy-controls"
  fi
done
verify_held
printf '%s\n' 'current held files and bytes unchanged' > "$proof/held-after.txt"
admit final 0
if ((untraced_status != 0)); then phase=lifecycle-untraced-failed; exit "$untraced_status"; fi
phase=candidate-observed
legacy_publish_reuse
