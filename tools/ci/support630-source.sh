#!/usr/bin/env bash
# #630 Actions 전용. 유지된 원본과 실제 native caller를 비교한다.
set -euo pipefail
: "${SOURCE_ROOT:?exact candidate checkout required}"
: "${CARGO_TARGET_DIR:?private target required}"
: "${EVIDENCE:?private evidence directory required}"
: "${SOURCE_PYTHON:?private Python 3.12 venv required}"
: "${RUNNER_TEMP:?private temporary directory required}"
: "${SUPPORT_ROOT:?private job root required}"
[[ ${GITHUB_ACTIONS:-} == true ]] || exit 1
if [[ ${RUNNER_NAME:-} == bin-actions2 && ${RUNNER_ENVIRONMENT:-} == self-hosted ]]; then
  :
else
  # This opt-in must be set by the parent only after explicit hosted ARM approval.
  [[ ${SUPPORT630_HOSTED_ARM_APPROVED:-} == true && ${RUNNER_ENVIRONMENT:-} == github-hosted &&
     ${RUNNER_ARCH:-} == ARM64 && $(uname -m) == aarch64 ]] || exit 1
fi
cd -- "$SOURCE_ROOT"
SOURCE_ROOT=$(pwd -P)
export SOURCE_ROOT CARGO_TARGET_DIR SOURCE_PYTHON RUNNER_TEMP
export SUPPORT630_SOURCE_SCOPE=${SUPPORT630_SOURCE_SCOPE:-all}
case $SUPPORT630_SOURCE_SCOPE in all|baselines) ;; *) exit 2 ;; esac
# Parent RUNNER_TEMP is an Actions directory, not necessarily job-private.
RUNNER_TEMP=$(mktemp -d /tmp/s630.XXXXXX)
chmod 700 "$RUNNER_TEMP"
export RUNNER_TEMP
export PIP_CACHE_DIR="$RUNNER_TEMP/pip" XDG_CACHE_HOME="$RUNNER_TEMP/cache" PIP_CONFIG_FILE=/dev/null
export RUSTUP_TOOLCHAIN=1.94.0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_TERM_COLOR=never RUST_TEST_THREADS="${RUST_TEST_THREADS:-2}"
export PYTHONDONTWRITEBYTECODE=1 PYTHONNOUSERSITE=1 CARROT_EXCEPTION_DISCORD_WEBHOOK_DISABLE=1
export TMPDIR="$RUNNER_TEMP" GIT_LFS_SKIP_SMUDGE=1
mkdir -p -- "$EVIDENCE/source"
EVIDENCE=$(cd -- "$EVIDENCE/source" && pwd -P)
export EVIDENCE
printf '%s\n' "$RUNNER_TEMP" > "$EVIDENCE/PRIVATE_TEMP_DIRECTORY"
mkdir -- "$EVIDENCE/logs" "$EVIDENCE/providers"
source_sha=$(git rev-parse HEAD)
[[ $source_sha =~ ^[0-9a-f]{40}$ ]]
if [[ -n ${SOURCE_SHA:-} ]]; then test "$source_sha" = "$SOURCE_SHA"; fi
printf '%s\n' "$source_sha" > "$EVIDENCE/SOURCE_COMMIT"
git diff --exit-code HEAD -- > "$EVIDENCE/source-before.diff"
git ls-files --stage -z | sha256sum > "$EVIDENCE/index-before.sha256"
"$SOURCE_PYTHON" - <<'PY' > "$EVIDENCE/input-hashes.json"
import hashlib, json, os, subprocess
from pathlib import Path
roots=['rust/tools','rust/Cargo.toml','rust/Cargo.lock','uv.lock','msgq_repo','opendbc_repo',
       'openpilot/common','openpilot/cereal','openpilot/system','openpilot/selfdrive/carrot/bluetooth',
       'openpilot/selfdrive/carrot/web_upload.py']
roots += ['rust/crates/'+name for name in (
    'web-upload','bootlog','tombstoned','soundd','bluetooth','wifi','modem','lpa','process-supervision')]
rows=[]
for value in subprocess.check_output(['git','ls-files','-z','--',*roots]).split(b'\0'):
    if not value: continue
    path=Path(os.fsdecode(value))
    if path.is_symlink(): digest=hashlib.sha256(os.readlink(path).encode()).hexdigest()
    else:
        with path.open('rb') as stream: digest=hashlib.file_digest(stream,'sha256').hexdigest()
    rows.append(dict(path=str(path),sha256=digest))
print(json.dumps(rows,indent=2))
PY
"$SOURCE_PYTHON" - <<'PY' > "$EVIDENCE/python.json"
import json, os, platform, sys
assert sys.version_info[:2] == (3, 12), sys.version
assert sys.prefix != sys.base_prefix, 'private venv required'
assert os.geteuid() != 0, 'filesystem permission comparisons require an unprivileged user'
print(json.dumps(dict(executable=sys.executable, prefix=sys.prefix, version=sys.version,
                     architecture=platform.machine(), uid=os.geteuid())))
PY
export PATH="$(dirname -- "$SOURCE_PYTHON"):$PATH"
last_step=initialization
finish() {
  local rc=$?
  trap - EXIT
  set +e
  git diff --exit-code HEAD -- > "$EVIDENCE/source-after.diff"
  if [[ $? != 0 ]]; then rc=1; fi
  git ls-files --stage -z | sha256sum > "$EVIDENCE/index-after.sha256"
  if ! cmp -s "$EVIDENCE/index-before.sha256" "$EVIDENCE/index-after.sha256"; then rc=1; fi
  # Only evidence files are copied; special filesystem fixtures are not artifacts.
  "$SOURCE_PYTHON" - "$RUNNER_TEMP/b" "$EVIDENCE/bootlog" <<'PY'
import os, shutil, stat, sys
from pathlib import Path
source,destination=map(Path,sys.argv[1:])
for directory, _, files in os.walk(source):
    for name in files:
        path=Path(directory)/name
        if path.suffix not in ('.json','.jsonl','.log') or not stat.S_ISREG(path.lstat().st_mode): continue
        target=destination/path.relative_to(source)
        target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(path,target)
PY
  if [[ $? != 0 ]]; then rc=1; fi
  SUPPORT630_RC=$rc SUPPORT630_STEP=$last_step "$SOURCE_PYTHON" - <<'PY'
import json, os, platform
from pathlib import Path
root=Path(os.environ['EVIDENCE'])
(root/'outcome.json').write_text(json.dumps(dict(
    source_sha=(root/'SOURCE_COMMIT').read_text().strip(),
    returncode=int(os.environ['SUPPORT630_RC']), last_step=os.environ['SUPPORT630_STEP'],
    architecture=platform.machine(), execution_mode=os.environ.get('OPENPILOT_CI_EXECUTION_MODE','native'),
    validation_scope=os.environ['SUPPORT630_SOURCE_SCOPE'],
    maintained_comparisons_executed=os.environ['SUPPORT630_SOURCE_SCOPE']=='all',
    required_contexts_satisfied=False, device_acceptance=False), indent=2)+'\n')
PY
  exit "$rc"
}
trap finish EXIT
admit() {
  local operation=$1 mib=$2
  shift 2
  "$SOURCE_PYTHON" - "$EVIDENCE/growth.jsonl" "$operation" "$mib" "${@:-$RUNNER_TEMP}" <<'PY'
import json, shutil, sys
from pathlib import Path
rows=[]
for value in sys.argv[4:]:
    path=Path(value)
    while not path.exists(): path=path.parent
    rows.append(dict(path=str(path), free_bytes=shutil.disk_usage(path).free,
                     estimated_growth_bytes=int(sys.argv[3])*1024**2))
with open(sys.argv[1],'a') as out:
    out.write(json.dumps(dict(operation=sys.argv[2],filesystems=rows))+'\n')
assert all(row['free_bytes'] >= row['estimated_growth_bytes'] for row in rows), rows
PY
}
run() {
  local label=$1
  shift
  last_step=$label
  "$SOURCE_PYTHON" - "$EVIDENCE/commands.jsonl" "$label" "$@" <<'PY'
import json, os, sys, time
with open(sys.argv[1],'a') as out:
    out.write(json.dumps(dict(label=sys.argv[2],argv=sys.argv[3:],cwd=os.getcwd(),started=time.time()))+'\n')
PY
  printf '+ '; printf '%q ' "$@"; printf '\n'
  local rc=0
  "$@" 2>&1 | tee "$EVIDENCE/logs/$label.log" || rc=$?
  printf '%s\n' "$rc" > "$EVIDENCE/logs/$label.exit"
  return "$rc"
}
# Metadata is deliberately explicit; pip is restricted to the supplied private venv.
cat > "$EVIDENCE/required-native-packages.txt" <<'DEPS'
build-essential clang libclang-dev cmake pkg-config capnproto libcapnp-dev libzmq3-dev
libavcodec-dev libavformat-dev libavutil-dev libzstd-dev libssl-dev dbus-daemon
strace time openssl curl ca-certificates git python3.12-dev python3.12-venv binutils
DEPS
for tool in cargo rustc capnp c++ cc dbus-daemon strace openssl curl git readelf; do command -v "$tool"; done
admit private-python-dependencies 1536 "$RUNNER_TEMP" "$(dirname -- "$SOURCE_PYTHON")"
run python-dependencies "$SOURCE_PYTHON" -m pip install \
  numpy==2.5.3 pycapnp==2.1.0 Cython==3.2.4 setuptools==82.0.1 \
  pyzmq==27.2.0 zstandard==0.25.0 requests==2.34.2 aiohttp==3.13.3 \
  anyio==4.12.1 PyJWT==2.15.1 cryptography==50.0.1 pyserial==3.5 \
  jeepney==0.9.0 pytest==9.0.2 sentry-sdk==2.55.0 cffi==2.1.1
run python-freeze "$SOURCE_PYTHON" -m pip freeze
run rust-version rustc +1.94.0 -Vv
packages=(-p openpilot-web-upload -p openpilot-bootlog -p openpilot-tombstoned
          -p openpilot-soundd -p openpilot-bluetooth -p openpilot-wifi
          -p openpilot-modem -p openpilot-lpa -p openpilot-process-supervision
          -p openpilot-logmessaged -p openpilot-portaudio)
admit native-daemons-and-probes 8192 "$CARGO_TARGET_DIR" "$RUNNER_TEMP"
run build-source-callers cargo +1.94.0 build --manifest-path rust/Cargo.toml \
  --locked "${packages[@]}" --features openpilot-modem/helper-lifetime-tests --lib --bins --examples
bin="$CARGO_TARGET_DIR/debug"
export WIFI_PROCESS_CHILD_BINARY="$bin/openpilot-process-child"
export MODEM_PROCESS_CHILD_BINARY="$WIFI_PROCESS_CHILD_BINARY" MODEM_SOURCE_PYTHON="$SOURCE_PYTHON"
admit original-params-msgq-bindings 2048 "$EVIDENCE" "$RUNNER_TEMP"
run build-params "$SOURCE_PYTHON" rust/tools/build_params_python.py \
  --output "$EVIDENCE/providers/params" --zmq-include /usr/include
run build-msgq "$SOURCE_PYTHON" rust/tools/build_msgq_python.py --output "$EVIDENCE/providers/msgq"
params=("$EVIDENCE"/providers/params/params_pyx*.so)
test "${#params[@]}" = 1 && test -f "${params[0]}"
export PYTHONPATH="$EVIDENCE/providers/msgq:$SOURCE_ROOT:$SOURCE_ROOT/rust/tools"
# The retained comparators own their original scenarios, tolerances and deadlines.
if [[ $SUPPORT630_SOURCE_SCOPE == all ]]; then
run web-upload "$SOURCE_PYTHON" rust/tools/check_web_upload.py \
  --binary "$bin/examples/web_upload_trace" --output "$EVIDENCE/web-upload"
run web-upload-timeouts "$SOURCE_PYTHON" rust/tools/check_web_upload_timeouts.py \
  --binary "$bin/examples/web_upload_trace" --output "$EVIDENCE/web-upload-timeouts"
run web-upload-eintr "$SOURCE_PYTHON" rust/tools/web_upload_eintr_compare.py \
  --binary "$bin/examples/web_upload_trace" --output "$EVIDENCE/web-upload-eintr"
run bootlog "$SOURCE_PYTHON" rust/tools/check_bootlog_snapshot.py \
  "$bin/examples/bootlog_snapshot" "${params[0]}" "$RUNNER_TEMP/b" --launcher "$WIFI_PROCESS_CHILD_BINARY"
run tombstone-reference "$SOURCE_PYTHON" rust/tools/check_tombstoned_reference.py \
  "$bin/examples/tombstone_trace" "${params[0]}" "$EVIDENCE/tombstone-reference"
# The unchanged checker may return zero with EMULATION_LIMITS. That is not PASS.
"$SOURCE_PYTHON" - "$EVIDENCE/tombstone-reference/manifest.json" <<'PY'
import json, sys
manifest=json.load(open(sys.argv[1]))
assert manifest['result']=='PASS' and not manifest['divergences'], manifest
assert manifest['matching_comparisons']==manifest['comparisons'], manifest
PY
run tombstone-safety "$SOURCE_PYTHON" rust/tools/check_tombstoned_safety.py \
  "$bin/examples/tombstone_trace" "${params[0]}" "$EVIDENCE/tombstone-safety"
run tombstone-sdk "$SOURCE_PYTHON" rust/tools/check_crash_sdk_transport.py \
  "$bin/examples/tombstone_trace" "${params[0]}" "$EVIDENCE/tombstone-sdk"
run tombstone-runtime "$SOURCE_PYTHON" rust/tools/check_tombstoned_runtime.py \
  "$bin/openpilot-tombstoned" "$bin/openpilot-logmessaged" "${params[0]}" "$EVIDENCE/tombstone-runtime"
run tombstone-shutdown "$SOURCE_PYTHON" rust/tools/check_tombstoned_shutdown.py \
  "$bin/openpilot-tombstoned" "${params[0]}" "$EVIDENCE/tombstone-shutdown"
run sound-policy "$SOURCE_PYTHON" rust/tools/check_soundd.py \
  --binary "$bin/examples/sound_trace" --binding "${params[0]}" --output "$EVIDENCE/sound-policy"
run sound-daemon "$SOURCE_PYTHON" rust/tools/check_soundd_daemon.py \
  --binary "$bin/openpilot-soundd" --output "$EVIDENCE/sound-daemon"
run bluetooth "$SOURCE_PYTHON" rust/tools/check_bluetooth_runtime.py \
  --bin-dir "$bin" --msgq-python "$EVIDENCE/providers/msgq" --output "$EVIDENCE/bluetooth"
run wifi-policy "$SOURCE_PYTHON" rust/tools/check_wifi_policy.py \
  --binary "$bin/examples/wifi_trace" --output "$EVIDENCE/wifi-policy"
run wifi-runtime "$SOURCE_PYTHON" rust/tools/check_wifi_runtime.py \
  --binary "$bin/examples/wifi_native" --launcher "$WIFI_PROCESS_CHILD_BINARY" \
  --binding "${params[0]}" --output "$EVIDENCE/wifi-runtime"
run wifi-callbacks "$SOURCE_PYTHON" rust/tools/check_wifi_callback_ordering.py \
  --binary "$bin/examples/wifi_native" --launcher "$WIFI_PROCESS_CHILD_BINARY" \
  --binding "${params[0]}" --output "$EVIDENCE/wifi-callbacks"
run modem-boundary "$SOURCE_PYTHON" rust/tools/check_modem_process_boundary.py \
  --target "$CARGO_TARGET_DIR" --evidence "$EVIDENCE/modem-boundary"
run modem "$SOURCE_PYTHON" rust/tools/check_modem.py --binary "$bin/openpilot-modem" \
  --trace "$bin/examples/modem_trace" --evidence "$EVIDENCE/modem"
run lpa "$SOURCE_PYTHON" rust/tools/check_lpa.py --target "$CARGO_TARGET_DIR" --evidence "$EVIDENCE/lpa"
# New probes use the actual candidate helper/library, not a copied implementation.
admit allocation-probes 512 "$EVIDENCE" "$RUNNER_TEMP"
run build-lpa-allocation rustc +1.94.0 --edition=2021 -O -D warnings \
  rust/tools/lpa_cookie_header_allocation_probe.rs -o "$EVIDENCE/lpa-allocation-probe"
run lpa-allocation "$EVIDENCE/lpa-allocation-probe" --require-improvement
run build-wifi-allocation rustc +1.94.0 --edition=2021 \
  rust/tools/wifi_snapshot_allocation_probe.rs --extern "openpilot_wifi=$bin/libopenpilot_wifi.rlib" \
  -L "dependency=$bin/deps" -o "$EVIDENCE/wifi-allocation-probe"
run wifi-allocation "$EVIDENCE/wifi-allocation-probe" --require-improvement
sha256sum "$EVIDENCE/lpa-allocation-probe" "$EVIDENCE/wifi-allocation-probe" > "$EVIDENCE/allocation-probes.sha256"
# Focused regressions explicitly require nonempty test sets in their logs.
run bluetooth-owner cargo +1.94.0 test --manifest-path rust/Cargo.toml --locked \
  -p openpilot-bluetooth --lib bluez::agent::prompt_owner_tests -- --nocapture
grep -F 'test result: ok. 3 passed; 0 failed; 0 ignored;' "$EVIDENCE/logs/bluetooth-owner.log"
run wifi-owner cargo +1.94.0 test --manifest-path rust/Cargo.toml --locked \
  -p openpilot-wifi --lib tether::cleanup_tests -- --nocapture
grep -F 'test result: ok. 3 passed; 0 failed; 0 ignored;' "$EVIDENCE/logs/wifi-owner.log"
run modem-owner cargo +1.94.0 test --manifest-path rust/Cargo.toml --locked \
  -p openpilot-modem --features helper-lifetime-tests --test helper_lifetime -- --nocapture
grep -F 'test result: ok. 15 passed; 0 failed; 0 ignored;' "$EVIDENCE/logs/modem-owner.log"
run tombstone-owner cargo +1.94.0 test --manifest-path rust/Cargo.toml --locked \
  -p openpilot-tombstoned --test apport_owner -- --nocapture
grep -F 'test result: ok. 3 passed; 0 failed; 0 ignored;' "$EVIDENCE/logs/tombstone-owner.log"
run bootlog-xattrs cargo +1.94.0 test --manifest-path rust/Cargo.toml --locked \
  -p openpilot-bootlog --test snapshot_xattrs -- --nocapture
run sound-playback cargo +1.94.0 test --manifest-path rust/Cargo.toml --locked \
  -p openpilot-soundd --test playback -- --nocapture
fi
# Baselines live only in this run's private directory. No candidate file is replaced.
admit baseline-checkouts-and-build 8192 "$RUNNER_TEMP" "$CARGO_TARGET_DIR"
baselines=$(mktemp -d "$RUNNER_TEMP/support630-baselines.XXXXXX")
dev_baseline=e445c9d19c3041bc79d8624589e2eb6ba7d061c6
modem_baseline=1364e48c96e888fa5d178c5eb2132d135cf30765
for sha in "$dev_baseline" "$modem_baseline"; do
  run "fetch-$sha" git -c maintenance.auto=false fetch --no-tags --no-write-fetch-head --depth=1 origin "$sha"
done
run clone-behavior-baseline git clone --no-hardlinks --no-checkout "$SOURCE_ROOT" "$baselines/behavior"
run checkout-behavior-baseline git -C "$baselines/behavior" checkout --detach "$source_sha"
export SUPPORT630_BASELINE="$baselines/behavior" SUPPORT630_OLD_SHA="$dev_baseline"
"$SOURCE_PYTHON" - <<'PY'
import hashlib, json, os, subprocess
from pathlib import Path
root=Path(os.environ['SOURCE_ROOT']); out=Path(os.environ['SUPPORT630_BASELINE'])
paths=['rust/crates/web-upload/src/multipart.rs','rust/crates/soundd/src/policy.rs',
       'rust/crates/tombstoned/src/apport.rs','rust/crates/bluetooth/src/bluez/agent.rs',
       'rust/crates/bluetooth/src/bluez/state.rs']
rows=[]
for path in paths:
    original=subprocess.check_output(['git','show',os.environ['SUPPORT630_OLD_SHA']+':'+path],cwd=root)
    value=original
    if path.endswith('bluez/agent.rs'):
        candidate=(root/path).read_bytes(); marker=b'#[cfg(test)]\nmod prompt_owner_tests {'
        assert candidate.count(marker)==1 and marker not in original
        value+=b'\n'+candidate[candidate.index(marker):]
    (out/path).write_bytes(value)
    rows.append(dict(path=path,old_source_sha256=hashlib.sha256(original).hexdigest(),
                     tested_sha256=hashlib.sha256(value).hexdigest(),candidate_sha256=hashlib.sha256((root/path).read_bytes()).hexdigest()))
(Path(os.environ['EVIDENCE'])/'baseline-sources.json').write_text(json.dumps(rows,indent=2)+'\n')
PY
baseline_target="$baselines/behavior-target"
run build-multipart-baseline env CARGO_TARGET_DIR="$baseline_target" \
  cargo +1.94.0 build --manifest-path "$baselines/behavior/rust/Cargo.toml" \
  --locked -p openpilot-web-upload --example web_upload_trace
run multipart-allocation "$SOURCE_PYTHON" rust/tools/check_multipart_allocation_351.py \
  --baseline "$baseline_target/debug/examples/web_upload_trace" --candidate "$bin/examples/web_upload_trace" \
  --output "$EVIDENCE/multipart-allocation"
red() {
  local label=$1 expected=$2
  shift 2
  run "$label-compile" env CARGO_TARGET_DIR="$baseline_target" "$@" --no-run
  # Libtest prints captured panic diagnostics for failed cases after stable
  # per-test results; keep concurrent test output from splitting control lines.
  if run "$label" env CARGO_TARGET_DIR="$baseline_target" "$@"; then
    echo "Expected actual old-source runtime failure: $label" >&2
    return 1
  fi
  grep -F 'test result: FAILED' "$EVIDENCE/logs/$label.log"
  grep -F "$expected" "$EVIDENCE/logs/$label.log"
  if grep -E '^error\[E[0-9]+\]|could not compile' "$EVIDENCE/logs/$label.log"; then return 1; fi
}
red sound-red '    finite_clip_stops_at_its_end_inside_a_callback' \
  cargo +1.94.0 test --manifest-path "$baselines/behavior/rust/Cargo.toml" --locked \
  -p openpilot-soundd --test playback
grep -F 'test result: FAILED. 3 passed; 5 failed; 0 ignored;' "$EVIDENCE/logs/sound-red.log"
grep -F 'test sample_counter_overflow_is_reported_without_wrapping ... ok' "$EVIDENCE/logs/sound-red.log"
red bluetooth-red 'old reply erased live new prompt' \
  cargo +1.94.0 test --manifest-path "$baselines/behavior/rust/Cargo.toml" --locked \
  -p openpilot-bluetooth --lib bluez::agent::prompt_owner_tests
grep -F '2 passed; 1 failed; 0 ignored;' "$EVIDENCE/logs/bluetooth-red.log"
red tombstone-red 'group identity released before final numeric group signal' \
  cargo +1.94.0 test --manifest-path "$baselines/behavior/rust/Cargo.toml" --locked \
  -p openpilot-tombstoned --test apport_owner invalid_utf8_cleanup_retains_waitable_group_identity_until_last_signal
run clone-modem-baseline git clone --no-hardlinks --no-checkout "$SOURCE_ROOT" "$baselines/modem"
run checkout-modem-baseline git -C "$baselines/modem" checkout --detach "$modem_baseline"
export MODEM398_BASELINE_ROOT="$baselines/modem" MODEM_HELPER_EVIDENCE="$EVIDENCE/modem-red-green"
run modem-red-green env CARGO_TARGET_DIR="$baselines/modem-target" \
  bash rust/tools/check_modem_helper_lifetime.sh
# Provider provenance, exact source hashes and ELF headers accompany result logs.
"$SOURCE_PYTHON" - "$EVIDENCE/native-artifacts.json" "$bin" <<'PY'
import hashlib, json, subprocess, sys
from pathlib import Path
rows=[]
for path in sorted(Path(sys.argv[2]).rglob('*')):
    if path.parent.name not in ('debug','examples') or not path.is_file(): continue
    with path.open('rb') as stream: magic=stream.read(4)
    if magic==b'\x7fELF':
        with path.open('rb') as stream: digest=hashlib.file_digest(stream,'sha256').hexdigest()
        rows.append(dict(path=str(path),sha256=digest,elf=subprocess.check_output(['readelf','-h',str(path)],text=True)))
Path(sys.argv[1]).write_text(json.dumps(rows,indent=2)+'\n')
PY
last_step=completed
