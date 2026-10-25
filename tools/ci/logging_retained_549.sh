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
finish() {
  result=$?
  trap - EXIT
  set +e
  verify_held > "$proof/held-final.stdout" 2> "$proof/held-final.stderr"
  held_result=$?
  if ((result == 0 && held_result != 0)); then result=$held_result; fi
  printf '{"phase":"%s","exit_code":%s,"held_final_exit_code":%s,"retirement_credit":0,"closed_trace_admitted":false,"old27_admitted":false}\n' "$phase" "$result" "$held_result" > "$proof/outcome.json"
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
  local label=$1 seconds=$2; shift 2
  phase=$label
  admit "$label" 512
  (ulimit -f 524288; /usr/bin/timeout --signal=TERM --kill-after=30s "${seconds}s" "$@") > "$proof/build/$label.stdout" 2> "$proof/build/$label.stderr"
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
# read/pread64는 FD/주소/길이/결과를 raw hex로 유지한다. 반복 해시의 대형 버퍼 사본만 펼치지 않는다.
# 입력·packet 원문은 별도 context/실제 결과에서 봉인한다. exec/argv/path 및 mapping은 계속 해석 출력한다.
# -x는 비ASCII 문자열의 byte escape를 유지한다. FD 번호·실제 path/metadata 인자는 그대로 두고
# -yy의 반복 파생 FD-path/protocol annotation만 생략한다. closed FD/cwd/DSO 수용은 계속 false다.
started=$(monotonic_cs)
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
phase=candidate-observed
