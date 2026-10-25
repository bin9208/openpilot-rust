#!/usr/bin/env bash
# 원본 보존 후보 실행. 전체 lifecycle/closed-trace/caller/absence 퇴역 gate는 별도다.
set -euo pipefail
umask 077
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
sdk=$RUSTUP_HOME/toolchains/1.94.0-x86_64-unknown-linux-gnu/bin
mkdir "$proof" "$proof/build" "$proof/trace" "$proof/bin" "$proof/lifecycle"
phase=admission
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
  verify_held > "$proof/held-final.stdout" 2> "$proof/held-final.stderr"
  held_result=$?
  if ((result == 0 && held_result != 0)); then result=$held_result; fi
  if ((result == 0 && untraced_status != 0)); then result=$untraced_status; phase=lifecycle-untraced-failed; fi
  printf '{"phase":"%s","exit_code":%s,"held_final_exit_code":%s,"untraced_exit_code":%s,"paired_final_exit_code":%s,"retirement_credit":0,"closed_trace_admitted":false,"old27_admitted":false}\n' "$phase" "$result" "$held_result" "$untraced_status" "$paired_final_result" > "$proof/outcome.json"
  exit "$result"
}
trap finish EXIT
admit() {
  local label=$1 growth_mib=$2 available target_bytes=0 proof_bytes
  available=$(df -PB1 "$source" | awk 'NR==2 {print $4}')
  if test -d "$CARGO_TARGET_DIR"; then target_bytes=$(du -sx --block-size=1 "$CARGO_TARGET_DIR" | cut -f1); fi
  proof_bytes=$(du -sx --block-size=1 "$CI_JOB_TEMP/proof" | cut -f1)
  [[ $available =~ ^[0-9]+$ && $target_bytes =~ ^[0-9]+$ && $proof_bytes =~ ^[0-9]+$ ]]
  printf '%s\t%s\t%s\t%s\t%s\n' "$label" "$growth_mib" "$available" "$target_bytes" "$proof_bytes" >> "$proof/growth.tsv"
  ((available >= growth_mib * 1024 * 1024 + 512 * 1024 * 1024))
  ((target_bytes <= 8 * 1024 * 1024 * 1024 && proof_bytes <= 1024 * 1024 * 1024))
}
run() {
  local label=$1 seconds=$2 command_status side started_cs ended_cs; shift 2
  phase=$label
  admit "$label" 512
  if [[ $label == lifecycle-runtime_directory_int || $label == git-preload-false || $label == git-preload-true ]]; then
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
for path in "$source/tools/ci/logging_retained_549.sh" "$source/.github/workflows/python-rust-523-logging.yml" "$sdk/cargo" "$sdk/rustc" "$sdk/clippy-driver" /usr/bin/git /usr/bin/strace /usr/bin/timeout; do hold "$path"; done
printf '%s  %s\n' adee1d45e299bfd6448e8f5584b194b2dbbb53622ee3c1d57c02538d0b0d1fc4 rust/tools/check_native_logging.py \
  288a4fd0f9aca05d82a3f415e6632ef3e62b92f2d73f01d7a2f1b5263cdb7ad1 rust/tools/native_logging_transport.py | sha256sum --check --strict - > "$proof/originals-present.txt"
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
runtime=(/usr/bin/env -i PATH=/usr/bin:/bin HOME="$HOME" TMPDIR="$TMPDIR" LANG=C.UTF-8 LC_ALL=C.UTF-8 TZ=UTC GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.preloadIndex GIT_CONFIG_VALUE_0=false GITHUB_ACTIONS=true RUNNER_ENVIRONMENT=github-hosted RUNNER_OS=Linux RUNNER_ARCH=X64 GITHUB_REPOSITORY=bin9208/openpilot-rust)
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
test "$GIT_CONFIG_COUNT:$GIT_CONFIG_KEY_0:$GIT_CONFIG_VALUE_0" = 1:core.preloadIndex:false
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
test "$GIT_OPTIONAL_LOCKS:$GIT_CONFIG_COUNT:$GIT_CONFIG_KEY_0:$GIT_CONFIG_VALUE_0" = 0:1:core.preloadIndex:false
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
run retained-core21 180 "${runtime[@]}" "$unit" native_logging_check::retained::tests:: --test-threads=1 --color=never
test "$(grep -Ec '^test result: ok\. 21 passed; 0 failed; 0 ignored;' "$proof/build/retained-core21.stdout")" -eq 1
cat > "$proof/retained-core21.expected.txt" <<'NAMES'
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
native_logging_check::retained::tests::success_receipt_create_new_cannot_overwrite_foreign_bytes
native_logging_check::retained::tests::unlisted_symlink_or_executable_in_context_never_becomes_an_ignored_role
NAMES
sed -nE 's/^test (native_logging_check::retained::tests::[a-z0-9_]+) \.\.\. ok$/\1/p' "$proof/build/retained-core21.stdout" | LC_ALL=C sort > "$proof/retained-core21.actual.txt"
cmp "$proof/retained-core21.expected.txt" "$proof/retained-core21.actual.txt"
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
test "$(grep -Ec '^test result: ok\. 20 passed; 0 failed; 0 ignored;' "$proof/build/trace-controls.stdout")" -eq 1
cat > "$proof/trace-controls.expected.txt" <<'NAMES'
native_logging_check::trace::tests::ancestor_symlink_and_existing_output_cannot_redirect_or_overwrite
native_logging_check::trace::tests::dispatch_and_help_require_only_the_first_literal_and_no_files
native_logging_check::trace::tests::duplicate_pid_spelling_and_unsafe_prefix_are_rejected
native_logging_check::trace::tests::held_inputs_detect_same_length_rewrite_replacement_and_added_roster
native_logging_check::trace::tests::hex_exec_failed_attempt_and_execveat_are_data_without_policy_acceptance
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
native_logging_check::trace::tests::symlink_hardlink_fifo_and_public_directory_are_rejected
native_logging_check::trace::tests::terminal_signal_keeps_interrupted_call_aborted_and_never_successful
NAMES
sed -nE 's/^test (native_logging_check::trace::tests::[a-z0-9_]+) \.\.\. ok$/\1/p' "$proof/build/trace-controls.stdout" | LC_ALL=C sort > "$proof/trace-controls.actual.txt"
cmp "$proof/trace-controls.expected.txt" "$proof/trace-controls.actual.txt"
run summary-controls 180 "${runtime[@]}" "$unit" native_logging_check::trace::summary_tests:: --test-threads=1 --color=never
test "$(grep -Ec '^test result: ok\. 6 passed; 0 failed; 0 ignored;' "$proof/build/summary-controls.stdout")" -eq 1
cat > "$proof/summary-controls.expected.txt" <<'NAMES'
native_logging_check::trace::summary_tests::summary_cli_is_explicit_and_full_report_contract_remains_schema_one
native_logging_check::trace::summary_tests::summary_keeps_every_exec_signal_terminal_and_all_event_counts
native_logging_check::trace::summary_tests::summary_preserves_event_retained_model_and_key_bounds
native_logging_check::trace::summary_tests::summary_rejects_malformed_unretained_calls_resumes_and_post_terminal_rows
native_logging_check::trace::summary_tests::summary_shares_held_raw_hash_and_identity_rejection
native_logging_check::trace::summary_tests::summary_validates_many_metadata_rows_without_exporting_their_details
NAMES
sed -nE 's/^test (native_logging_check::trace::summary_tests::[a-z0-9_]+) \.\.\. ok$/\1/p' "$proof/build/summary-controls.stdout" | LC_ALL=C sort > "$proof/summary-controls.actual.txt"
cmp "$proof/summary-controls.expected.txt" "$proof/summary-controls.actual.txt"
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
test "$GIT_OPTIONAL_LOCKS:$GIT_CONFIG_COUNT:$GIT_CONFIG_KEY_0:$GIT_CONFIG_VALUE_0" = 0:1:core.preloadIndex:false
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
  GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.preloadIndex GIT_CONFIG_VALUE_0=false)
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
lifecycle_env=(NATIVE_LOGGING_RETAINED_BINARY="$NATIVE_LOGGING_RETAINED_BINARY" NATIVE_LOGGING_RETAINED_CONTEXT="$NATIVE_LOGGING_RETAINED_CONTEXT" NATIVE_LOGGING_RETAINED_SOURCE_COMMIT="$NATIVE_LOGGING_RETAINED_SOURCE_COMMIT" NATIVE_LOGGING_RETAINED_BINARY_SHA256="$NATIVE_LOGGING_RETAINED_BINARY_SHA256" NATIVE_LOGGING_RETAINED_CONTEXT_SHA256="$NATIVE_LOGGING_RETAINED_CONTEXT_SHA256" NATIVE_LOGGING_LIFECYCLE_EVIDENCE="$NATIVE_LOGGING_LIFECYCLE_EVIDENCE")
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
  elif [[ $side == git-false || $side == git-true ]]; then
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
# #554 B: 실제 source의 Git만 false→true 각1회 관측한다. fixture의 false-schema는 바꾸지 않는다.
mkdir -m 700 "$proof/paired-554/git-false" "$proof/paired-554/git-true" "$proof/git-preload-status-554"
mkdir -m 700 "$proof/git-preload-status-554/qa"
elf git-preload-git /usr/bin/git
elf git-preload-strace /usr/bin/strace
grep -F -- --argv0 "$proof/build/strace-help.stdout" > "$proof/git-preload-status-554/argv0-supported.txt"
for setting in false true; do
  # env_clear 뒤 실제 checker/Git의6+3키 및 QA context만 복제한다. OPTIONAL_LOCKS=0을 새로 넣지 않는다.
  git_preload_runtime=(/usr/bin/env -i PATH=/usr/bin:/bin HOME="$HOME" TMPDIR="$TMPDIR" LANG=C.UTF-8 LC_ALL=C.UTF-8 TZ=UTC
    GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.preloadIndex GIT_CONFIG_VALUE_0="$setting"
    NATIVE_LOGGING_QA_HOLD=runtime NATIVE_LOGGING_QA_RECORDS="$proof/git-preload-status-554/qa")
  printf '%s\0' "${git_preload_runtime[@]}" > "$proof/paired-554/git-$setting/environment.argv0"
  printf '%s\0' "${git_preload_runtime[@]:2}" | jq -Rs 'split("\u0000")[:-1]' > "$proof/paired-554/git-$setting/expected-environment.json"
  run "git-preload-$setting" 10 "${git_preload_runtime[@]}" /usr/bin/strace --argv0=git --seccomp-bpf --kill-on-exit \
    -ff -ttt -x -v -s 65536 -e trace=process,%file,%desc,%signal,%network,%memory,setsid,setpgid,prctl,unshare,setns \
    -e raw=read,pread64 -o "$proof/trace/git-preload-$setting" /usr/bin/git status --porcelain --untracked-files=normal
done
phase=git-preload-channels
for channel in stdout stderr; do
  cmp "$proof/build/git-preload-false.$channel" "$proof/build/git-preload-true.$channel"
  # stderr에는 tracer 진단도 모인다. nonempty이면 분리/정규화해 허용하지 않는다.
  test ! -s "$proof/build/git-preload-false.$channel" && test ! -s "$proof/build/git-preload-true.$channel"
done
cmp "$proof/paired-554/git-false/command.exit" "$proof/paired-554/git-true/command.exit"
for setting in false true; do
  run "inspect-git-preload-$setting" 180 "${runtime[@]}" "$checker" inspect-trace-summary --directory "$proof/trace" \
    --prefix "git-preload-$setting" --output "$proof/git-preload-status-554/$setting.summary.json"
  phase=git-preload-$setting-bind
  jq -e --arg root "$source" --arg source "$GITHUB_SHA" --arg setting "$setting" \
    --slurpfile expected "$proof/paired-554/git-$setting/expected-environment.json" \
    --slurpfile elapsed "$proof/paired-554/git-$setting/elapsed.json" '
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
    {schema:1,source_commit:$source,root:$root,preload_index:$setting,command_exit:0,
     exec_pid:$call.pid,exec_timestamp:$call.source[0].timestamp,terminal_timestamp:$terminal[0].source[0].timestamp,
     exec_to_terminal_seconds:(($terminal[0].source[0].timestamp|tonumber)-($call.source[0].timestamp|tonumber)),
     collector_elapsed:$elapsed[0],raw_files:.raw_files,selected_raw_bytes:.selected_raw_bytes,
     event_count:.event_count,physical_lines:.physical_lines,event_counts:.event_counts,
     diagnostic_only:true,bpf_activation_proven:false,full_event_index:false,closed_pid:false,closed_cwd:false,
     closed_fd:false,closed_dso:false,R4_acceptance:false,runtime_acceptance:false,retirement_credit:0}' \
    "$proof/git-preload-status-554/$setting.summary.json" > "$proof/git-preload-status-554/$setting.observation.json"
done
phase=git-preload-pair-bind
jq -s -e 'if (length==2 and .[0].preload_index=="false" and .[1].preload_index=="true" and
    .[0].source_commit==.[1].source_commit and .[0].root==.[1].root) then
    . as $pair | ((.[0].event_counts|keys)+(.[1].event_counts|keys)|unique) as $keys |
    {schema:1,source_commit:.[0].source_commit,root:.[0].root,order:["false","true"],status_pairs:1,
     status_channels_equal:true,observations:.,diagnostic_only:true,production_preload_changed:false,
     true_minus_false:{collector_elapsed_cs:(.[1].collector_elapsed.elapsed_cs-.[0].collector_elapsed.elapsed_cs),
       exec_to_terminal_seconds:(.[1].exec_to_terminal_seconds-.[0].exec_to_terminal_seconds),
       event_count:(.[1].event_count-.[0].event_count),
       event_counts:(reduce $keys[] as $key ({}; .[$key]=(($pair[1].event_counts[$key]//0)-($pair[0].event_counts[$key]//0))))},
     performance_claim:false,root_cause_fixed:false,runtime_acceptance:false,R4_acceptance:false,retirement_credit:0}
    else error("Git diagnostic pair differs") end' "$proof/git-preload-status-554/"{false,true}.observation.json \
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
run retained-once 360 "${runtime[@]}" /usr/bin/strace --seccomp-bpf --kill-on-exit -ff -ttt -x -v -s 65536 -e trace=process,%file,%desc,%signal,%network,%memory,setsid,setpgid,prctl,unshare,setns -e raw=read,pread64 -o "$proof/trace/retained-once" "$checker" retained --root "$source" --binary "$probe" --context "$context" --output "$proof/native" --source-commit "$GITHUB_SHA" --binary-sha256 "$NATIVE_LOGGING_RETAINED_BINARY_SHA256"
jq -e --arg source "$GITHUB_SHA" '.fresh_native_packets==250 and .fresh_native_responses==697 and .retained_cpp_packets==249 and .retained_cpp_responses==694 and .cpp_executions==0 and .source_commit==$source and .R4_acceptance==false and .lifecycle_reuse_granted==false and (.historical_to_recovered_console|length)==7 and (.recovered_to_fresh_console|length)==7 and all(.historical_to_recovered_console[]; .projected_equal==true) and all(.recovered_to_fresh_console[]; .projected_equal==true)' "$proof/native/report.json" > "$proof/raw-comparison.txt"
for stem in providers context-inputs source-state; do cmp "$proof/native/$stem.before.json" "$proof/native/$stem.after.json"; done
# 같은 lexer로 모든 원문 줄을 검사하되 세부 syscall Event는 집계로 출력한다.
# 전체 raw는 그대로 보존한다. full event index와 PID/cwd/FD/DSO 정책 수용은 아니다.
mkdir "$proof/inspect"
for name in first_int first_term first_eof runtime_directory_int runtime_directory_term escaped_descendant_int escaped_descendant_term escaped_descendant_eof output_file output_directory output_live_directory output_symlink output_dangling_symlink forbidden_executable_options retained-once; do
  run "inspect-$name" 180 "${runtime[@]}" "$checker" inspect-trace-summary --directory "$proof/trace" --prefix "$name" --output "$proof/inspect/$name.json"
  jq -e '.schema==2 and .raw_immutable==true and .requires_exit_status_zero==true and .full_event_index==false and .lexically_checked_all_events==true and .event_count>0 and .event_count<=1000000 and .physical_lines>=.event_count and .physical_lines<=2000000 and ((.event_counts|to_entries|map(.value)|add)==.event_count) and .closed_pid==false and .closed_cwd==false and .closed_fd==false and .closed_dso==false and .full_acceptance==false and .retirement_credit==false' "$proof/inspect/$name.json" > "$proof/inspect/$name.scope.txt"
done
verify_held
printf '%s\n' 'current held files and bytes unchanged' > "$proof/held-after.txt"
admit final 0
if ((untraced_status != 0)); then phase=lifecycle-untraced-failed; exit "$untraced_status"; fi
phase=candidate-observed
