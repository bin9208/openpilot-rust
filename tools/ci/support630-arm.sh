#!/usr/bin/env bash
# #630 영향9package GNU ARM 빌드. 기존 provider 도구를 고정 SHA로 읽기 재사용한다.
set -euo pipefail
: "${SOURCE_ROOT:?exact candidate checkout required}"
: "${CARGO_TARGET_DIR:?private target required}"
: "${EVIDENCE:?private evidence directory required}"
: "${SOURCE_PYTHON:?private Python 3.12 venv required}"
: "${SUPPORT_ROOT:?private job root required}"
[[ ${GITHUB_ACTIONS:-} == true && ${GITHUB_REPOSITORY:-} == bin9208/openpilot-rust ]]
[[ ${RUNNER_NAME:-} == bin-actions2 && ${RUNNER_ENVIRONMENT:-} == self-hosted && ${RUNNER_ARCH:-} == X64 ]]
[[ $(uname -m) == x86_64 && $(id -u) != 0 ]]
case ${SUPPORT630_ARM_MODE:-gnu-cross-build} in
  gnu-cross-build) ;;
  *) echo 'Only affected GNU cross-build is authorized; no native/hosted/QEMU execution' >&2; exit 2 ;;
esac
cd -- "$SOURCE_ROOT"
SOURCE_ROOT=$(pwd -P)
mkdir -p -- "$EVIDENCE/arm"
arm_evidence=$(cd -- "$EVIDENCE/arm" && pwd -P)
arm_job=$(mktemp -d "$SUPPORT_ROOT/arm630.XXXXXX")
chmod 700 "$arm_job"
export RUNNER_TEMP="$arm_job/tmp" TMPDIR="$arm_job/tmp"
export XDG_CACHE_HOME="$arm_job/cache" PIP_CACHE_DIR="$arm_job/cache/pip"
mkdir -p "$RUNNER_TEMP" "$arm_job/cache" "$arm_evidence/logs" "$arm_evidence/provider"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" RUSTUP_TOOLCHAIN=1.94.0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_TERM_COLOR=never
export GIT_LFS_SKIP_SMUDGE=1 PYTHONDONTWRITEBYTECODE=1 PYTHONNOUSERSITE=1
source_sha=$(git rev-parse HEAD)
[[ $source_sha =~ ^[0-9a-f]{40}$ ]]
if [[ -n ${SOURCE_SHA:-} ]]; then test "$source_sha" = "$SOURCE_SHA"; fi
if [[ -n ${REQUESTED_SOURCE:-} ]]; then test "$source_sha" = "$REQUESTED_SOURCE"; fi
printf '%s\n' "$source_sha" > "$arm_evidence/SOURCE_COMMIT"
printf '%s\n' "$arm_job" > "$arm_evidence/OWNED_ARM_DIRECTORY"
printf '%s\n' "$GITHUB_SHA" > "$arm_evidence/CONTROLLER_COMMIT"
git diff --exit-code HEAD -- > "$arm_evidence/source-before.diff"
git diff --cached --exit-code > "$arm_evidence/index-before.diff"
git ls-files --stage -z | sha256sum > "$arm_evidence/index-before.sha256"
provider_root="$arm_job/ffmpeg-gnu"
helper_root="$arm_job/provider-tools"
last_step=initialization
finish() {
  local rc=$?
  trap - EXIT
  set +e
  git diff --exit-code HEAD -- > "$arm_evidence/source-after.diff"
  if [[ $? != 0 ]]; then rc=1; fi
  git diff --cached --exit-code > "$arm_evidence/index-after.diff"
  if [[ $? != 0 ]]; then rc=1; fi
  git ls-files --stage -z | sha256sum > "$arm_evidence/index-after.sha256"
  if ! cmp -s "$arm_evidence/index-before.sha256" "$arm_evidence/index-after.sha256"; then rc=1; fi
  # Preserve provider receipts on both success and a failed native build.
  if [[ -d $provider_root ]]; then
    for file in "$provider_root"/*.json "$provider_root"/*.log; do
      if [[ -f $file ]]; then cp -- "$file" "$arm_evidence/provider/" || rc=1; fi
    done
    if [[ -f $provider_root/build/ffbuild/config.log ]]; then
      cp -- "$provider_root/build/ffbuild/config.log" "$arm_evidence/provider/ffmpeg-config.log" || rc=1
    fi
  fi
  if [[ -d $helper_root/.git ]]; then
    git -C "$helper_root" diff --exit-code HEAD -- > "$arm_evidence/helper-after.diff"
    if [[ $? != 0 ]]; then rc=1; fi
    git -C "$helper_root" diff --cached --exit-code > "$arm_evidence/helper-index-after.diff"
    if [[ $? != 0 ]]; then rc=1; fi
  fi
  if [[ -f $arm_evidence/helper-and-abi.json ]]; then
    "$SOURCE_PYTHON" - "$arm_evidence/helper-and-abi.json" <<'PY'
import hashlib, json, sys
from pathlib import Path
record=json.loads(Path(sys.argv[1]).read_text())
for tool in record['tools'].values():
    with Path(tool['path']).open('rb') as stream: actual=hashlib.file_digest(stream,'sha256').hexdigest()
    assert actual==tool['sha256'],tool
PY
    if [[ $? != 0 ]]; then rc=1; fi
  fi
  "$SOURCE_PYTHON" - "$arm_evidence/outcome.json" "$rc" "$last_step" "$source_sha" <<'PY'
import json, os, sys
from pathlib import Path
Path(sys.argv[1]).write_text(json.dumps(dict(
    source_sha=sys.argv[4],controller_sha=os.environ['GITHUB_SHA'],runner=os.environ['RUNNER_NAME'],
    returncode=int(sys.argv[2]),last_step=sys.argv[3],scope='affected-nine-package-gnu-aarch64-build',
    arm_execution=False,agnos_abi_validation=False,device_acceptance=False,
    full_gnu_musl_gate=False,required_contexts_satisfied=False),indent=2)+'\n')
PY
  exit "$rc"
}
trap finish EXIT
admit() {
  local operation=$1 mib=$2
  shift 2
  "$SOURCE_PYTHON" - "$arm_evidence/growth.jsonl" "$operation" "$mib" "$@" <<'PY'
import json, shutil, sys
from pathlib import Path
rows=[]
for name in sys.argv[4:]:
    path=Path(name)
    while not path.exists(): path=path.parent
    rows.append(dict(path=str(path),free_bytes=shutil.disk_usage(path).free,
                     estimated_growth_bytes=int(sys.argv[3])*1024**2))
with open(sys.argv[1],'a') as out: out.write(json.dumps(dict(operation=sys.argv[2],filesystems=rows))+'\n')
assert rows and all(row['free_bytes']>=row['estimated_growth_bytes'] for row in rows),rows
PY
}
run() {
  local label=$1
  shift
  last_step=$label
  { printf '%s ' "$label"; printf '%q ' "$@"; printf '\n'; } >> "$arm_evidence/commands.txt"
  local rc=0
  "$@" 2>&1 | tee "$arm_evidence/logs/$label.log" || rc=$?
  printf '%s\n' "$rc" > "$arm_evidence/logs/$label.exit"
  return "$rc"
}
# Match the original Ubuntu 24.04 GNU runner ABI. No AGNOS/older-glibc claim.
"$SOURCE_PYTHON" - <<'PY' > "$arm_evidence/host.json"
import json, os, platform, sys
from pathlib import Path
release=dict(line.split('=',1) for line in Path('/etc/os-release').read_text().splitlines() if '=' in line)
assert release['ID'].strip('"')=='ubuntu' and release['VERSION_ID'].strip('"')=='24.04',release
assert sys.version_info[:2]==(3,12) and sys.prefix!=sys.base_prefix,'private Python 3.12 venv required'
print(json.dumps(dict(os_release=release,python=sys.version,python_executable=sys.executable,
                     machine=platform.machine(),uid=os.getuid()),indent=2))
PY
# Native package installation only; no other CI/cache removal or apt lock interruption.
# Major compiler version is fixed; the selected apt versions and binaries are receipted.
native=(gcc-13-aarch64-linux-gnu g++-13-aarch64-linux-gnu binutils-aarch64-linux-gnu
        libc6-dev-arm64-cross libstdc++-13-dev-arm64-cross make cmake pkg-config
        clang libclang-dev capnproto libcapnp-dev libzmq3-dev curl git xz-utils
        liblzma-dev libssl-dev zlib1g-dev)
admit native-cross-prerequisites 4096 /usr /var "$SUPPORT_ROOT"
# Reuse controller c918f07a7's private official Noble scope. Never update the
# host-wide PPA lists or rewrite /etc/apt. Parent preparation must precede us.
"$SOURCE_PYTHON" - "$SUPPORT_ROOT/ubuntu.sources" "$arm_evidence/apt-scope.json" <<'PY'
import hashlib, json, os, sys
from pathlib import Path
path=Path(sys.argv[1]); assert path.is_file() and not path.is_symlink()
assert path.stat().st_uid==os.getuid()
sections=[]
for block in path.read_text().strip().split('\n\n'):
    fields=dict(line.split(':',1) for line in block.splitlines() if line.strip())
    sections.append({key:value.strip() for key,value in fields.items()})
common={'Types':'deb','Components':'main restricted universe multiverse',
        'Signed-By':'/usr/share/keyrings/ubuntu-archive-keyring.gpg'}
assert sections==[
    common|{'URIs':'https://archive.ubuntu.com/ubuntu/','Suites':'noble noble-updates noble-backports'},
    common|{'URIs':'https://security.ubuntu.com/ubuntu/','Suites':'noble-security'}],sections
keyring=Path(common['Signed-By']); assert keyring.is_file()
Path(sys.argv[2]).write_text(json.dumps(dict(source=str(path),sections=sections,
    source_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),keyring=str(keyring),
    keyring_sha256=hashlib.sha256(keyring.read_bytes()).hexdigest()),indent=2)+'\n')
PY
test -d "$SUPPORT_ROOT/apt-lists/partial"
test -d "$SUPPORT_ROOT/apt-archives/partial"
apt=(sudo -n env DEBIAN_FRONTEND=noninteractive apt-get
     -o DPkg::Lock::Timeout=600 -o APT::Architecture=amd64 -o APT::Architectures=amd64
     -o "Dir::Etc::sourcelist=$SUPPORT_ROOT/ubuntu.sources" -o Dir::Etc::sourceparts=-
     -o "Dir::State::lists=$SUPPORT_ROOT/apt-lists" -o "Dir::Cache::archives=$SUPPORT_ROOT/apt-archives")
run apt-update "${apt[@]}" update
run apt-install "${apt[@]}" install -y --no-install-recommends --no-upgrade --no-remove "${native[@]}"
run native-package-versions dpkg-query -W "${native[@]}"
cc=$(command -v aarch64-linux-gnu-gcc-13)
cxx=$(command -v aarch64-linux-gnu-g++-13)
ar=$(command -v aarch64-linux-gnu-ar)
ranlib=$(command -v aarch64-linux-gnu-ranlib)
export SUPPORT630_CC="$cc" SUPPORT630_CXX="$cxx" SUPPORT630_AR="$ar" SUPPORT630_RANLIB="$ranlib"
run compiler-version "$cc" -v
run compiler-target "$cc" -dumpmachine
run binutils-version "$ar" --version
admit rust-target-and-host-provider-tool 4096 "$SUPPORT_ROOT" "$CARGO_TARGET_DIR" "${RUSTUP_HOME:-$HOME/.rustup}" "${CARGO_HOME:-$HOME/.cargo}"
run rust-toolchain rustup toolchain install 1.94.0 --profile minimal
run rust-arm-target rustup target add --toolchain 1.94.0 aarch64-unknown-linux-gnu
run rust-version rustc +1.94.0 -Vv
# Reuse #611 unchanged, outside the runtime candidate. Original dev has a 25 GiB
# build-side floor, while this exact source has growth-only admission.
helper_sha=fb33b6503bfd74b75e262d7148505b1381e8f8da
admit pinned-provider-tool-checkout 2048 "$SUPPORT_ROOT"
run helper-init git init "$helper_root"
run helper-origin git -C "$helper_root" remote add origin https://github.com/bin9208/openpilot-rust.git
run helper-fetch git -C "$helper_root" -c maintenance.auto=false fetch --no-tags --depth=1 origin "$helper_sha"
run helper-checkout git -C "$helper_root" -c core.hooksPath=/dev/null checkout --detach "$helper_sha"
test "$(git -C "$helper_root" rev-parse HEAD)" = "$helper_sha"
export SUPPORT630_HELPER_ROOT="$helper_root" SUPPORT630_PROVIDER_ROOT="$provider_root"
"$SOURCE_PYTHON" - "$arm_evidence/helper-and-abi.json" <<'PY'
import hashlib, json, os, re, subprocess, sys
from pathlib import Path
root=Path(os.environ['SUPPORT630_HELPER_ROOT'])
expected={
 'rust/crates/dev-tools/src/ffmpeg_build.rs':'c9935c88da7212cbad9e9ee51997ea4e764fe17360cd83bb98b4d88b5049e490',
 'rust/crates/disk-space/src/lib.rs':'160390714225d2fc2d425182baed86343a133800c54055e44f88ee12cb6e1a94',
 'rust/Cargo.lock':'c78df868dbff23470a79445719dd6a078d9f5cb2a6a9704cd6bfdf0a899598f8',
}
for name,digest in expected.items(): assert hashlib.sha256((root/name).read_bytes()).hexdigest()==digest,name
lib=(root/'rust/crates/dev-tools/src/lib.rs').read_text()
assert 'pub use openpilot_disk_space as disk_space;' in lib
cc=os.environ['SUPPORT630_CC']
assert subprocess.check_output([cc,'-dumpmachine'],text=True).strip()=='aarch64-linux-gnu'
macros=subprocess.check_output([cc,'-dM','-E','-x','c','-'],input='#include <features.h>\n',text=True)
defines=dict(re.findall(r'^#define (\w+) (.+)$',macros,re.M))
for key,value in {'__aarch64__':'1','__GNUC__':'13','__LP64__':'1','__SIZEOF_POINTER__':'8',
                  '__BYTE_ORDER__':'__ORDER_LITTLE_ENDIAN__','__GLIBC__':'2','__GLIBC_MINOR__':'39'}.items():
    assert defines.get(key)==value,(key,defines.get(key),value)
tools={}
for key in ('CC','CXX','AR','RANLIB'):
    path=Path(os.environ['SUPPORT630_'+key]).resolve()
    with path.open('rb') as stream: digest=hashlib.file_digest(stream,'sha256').hexdigest()
    tools[key]=dict(path=str(path),sha256=digest)
Path(sys.argv[1]).write_text(json.dumps(dict(
    helper_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
    helper_inputs=expected,tools=tools,c_predefines=macros,
    rust_toolchain='1.94.0',target='aarch64-unknown-linux-gnu',abi='ELF64 little-endian AArch64 LP64 / GNU glibc 2.39',
    ffmpeg_version='6.1.1',ffmpeg_sha256='8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968'),indent=2)+'\n')
PY
# Use the existing tool/codec flags/source pin; do not reimplement FFmpeg configure.
admit existing-ffmpeg-provider-build 6144 "$SUPPORT_ROOT" "$arm_job/provider-host-target"
run build-pinned-provider-tool env CARGO_TARGET_DIR="$arm_job/provider-host-target" \
  cargo +1.94.0 build --manifest-path "$helper_root/rust/Cargo.toml" --locked \
  -p openpilot-dev-tools --bin build-loggerd-ffmpeg
# Reuse the already implemented #628 downloader, including its bounded zero-body
# retry/mirror policy. This adapter supplies inputs; it implements no transport.
cat > "$arm_job/fetch-ffmpeg.rs" <<'RUST'
use openpilot_runtime_build::assets::{Asset, Curl};
use std::{error::Error, path::PathBuf};
fn main() -> Result<(), Box<dyn Error>> {
    let destination = PathBuf::from(std::env::args_os().nth(1).ok_or("archive path required")?);
    let asset = Asset {
        url: "https://ffmpeg.org/releases/ffmpeg-6.1.1.tar.xz".into(),
        sha256: "8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968".into(),
        bytes: 10458600,
    };
    let transferred = Curl.fetch_with_mirror(
        &asset,
        &destination,
        "https://sources.buildroot.net/ffmpeg/ffmpeg-6.1.1.tar.xz",
    )?;
    println!("{{\"transferred_bytes\":{transferred},\"expected_bytes\":10458600}}");
    Ok(())
}
RUST
runtime_build_rlib=$(find "$arm_job/provider-host-target/debug/deps" -maxdepth 1 \
  -type f -name 'libopenpilot_runtime_build-*.rlib' -print -quit)
test -n "$runtime_build_rlib"
run build-source-fetch-adapter rustc +1.94.0 --edition=2021 -D warnings \
  "$arm_job/fetch-ffmpeg.rs" --crate-name support630_ffmpeg_fetch \
  --extern "openpilot_runtime_build=$runtime_build_rlib" \
  -L "dependency=$arm_job/provider-host-target/debug/deps" -o "$arm_job/fetch-ffmpeg"
run fetch-original-source "$arm_job/fetch-ffmpeg" "$arm_job/ffmpeg-6.1.1.tar.xz"
test "$(stat -c %s "$arm_job/ffmpeg-6.1.1.tar.xz")" = 10458600
printf '%s  %s\n' 8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968 \
  "$arm_job/ffmpeg-6.1.1.tar.xz" | sha256sum --check -
sha256sum "$arm_job/fetch-ffmpeg.rs" "$arm_job/fetch-ffmpeg" "$runtime_build_rlib" \
  > "$arm_evidence/source-fetch-adapter.sha256"
run build-pinned-ffmpeg "$arm_job/provider-host-target/debug/build-loggerd-ffmpeg" \
  --root "$provider_root" --archive "$arm_job/ffmpeg-6.1.1.tar.xz" \
  --cc "$cc" --ar "$ar" --ranlib "$ranlib" --jobs "$CARGO_BUILD_JOBS"
export FFMPEG_DIR="$provider_root/install"
"$SOURCE_PYTHON" - "$arm_evidence/ffmpeg-provider.json" <<'PY'
import hashlib, json, os, re, subprocess, sys
from pathlib import Path
root=Path(os.environ['SUPPORT630_PROVIDER_ROOT']); prefix=root/'install'
record=json.loads((root/'build-record.json').read_text())
assert record['completed'] is True and record['headers_only'] is False
assert record['profile']=='logger-minimal-static' and record['source_version']=='6.1.1'
assert record['source_sha256']=='8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968'
assert record['compiler']['command']==os.environ['SUPPORT630_CC']
for relative,expected in record['files'].items():
    with (prefix/relative).open('rb') as stream: actual=hashlib.file_digest(stream,'sha256').hexdigest()
    assert actual==expected,relative
archives={}
for name in ('avcodec','avformat','avutil'):
    path=prefix/f'lib/lib{name}.a'
    headers=subprocess.check_output(['aarch64-linux-gnu-readelf','-h',str(path)],text=True)
    machines=re.findall(r'Machine:\s*(.+)',headers)
    assert machines and set(machines)=={'AArch64'},(path,set(machines))
    archives[str(path)]=dict(sha256=record['files'][f'lib/lib{name}.a'],elf_objects=len(machines))
Path(sys.argv[1]).write_text(json.dumps(dict(prefix=str(prefix),archives=archives,provider_record=str(root/'build-record.json')),indent=2)+'\n')
PY
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$cc"
export CC_aarch64_unknown_linux_gnu="$cc" CXX_aarch64_unknown_linux_gnu="$cxx"
export AR_aarch64_unknown_linux_gnu="$ar"
export PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_LIBDIR="$FFMPEG_DIR/lib/pkgconfig"
export PKG_CONFIG_PATH='' PKG_CONFIG_SYSROOT_DIR=/usr/aarch64-linux-gnu
export BINDGEN_EXTRA_CLANG_ARGS='--target=aarch64-unknown-linux-gnu --sysroot=/usr/aarch64-linux-gnu -I/usr/aarch64-linux-gnu/include'
command=(cargo +1.94.0 build --manifest-path rust/Cargo.toml --locked
  -p openpilot-web-upload -p openpilot-bootlog -p openpilot-tombstoned
  -p openpilot-soundd -p openpilot-bluetooth -p openpilot-wifi
  -p openpilot-modem -p openpilot-lpa -p openpilot-process-supervision
  --features openpilot-bootlog/static-ffmpeg,openpilot-modem/helper-lifetime-tests
  --bins --examples --release --target aarch64-unknown-linux-gnu --message-format=json)
admit affected-nine-package-arm-build 8192 "$CARGO_TARGET_DIR" "$SUPPORT_ROOT"
run affected-build "${command[@]}"
cp -- "$arm_evidence/logs/affected-build.log" "$arm_evidence/build.jsonl"
"$SOURCE_PYTHON" - "$arm_evidence/build.jsonl" "$arm_evidence/elf-artifacts.json" <<'PY'
import hashlib, json, os, re, subprocess, sys
from pathlib import Path
root=Path(os.environ['SOURCE_ROOT']).resolve()
packages={'web-upload','bootlog','tombstoned','soundd','bluetooth','wifi','modem','lpa','process-supervision'}
expected={str(root/'rust/crates'/name/'Cargo.toml'):name for name in packages}
seen=set(); rows=[]
for line in Path(sys.argv[1]).read_text().splitlines():
    try: value=json.loads(line)
    except json.JSONDecodeError: continue
    if value.get('reason')!='compiler-artifact': continue
    name=expected.get(value.get('manifest_path'))
    if name is None or not value.get('executable'): continue
    path=Path(value['executable']).resolve()
    target=Path(os.environ['CARGO_TARGET_DIR']).resolve()/'aarch64-unknown-linux-gnu/release'
    assert path.is_relative_to(target),path
    header=subprocess.check_output(['aarch64-linux-gnu-readelf','-h',str(path)],text=True)
    assert re.search(r'Class:\s+ELF64',header) and re.search(r'Data:.*little endian',header)
    assert re.search(r'Machine:\s+AArch64\s*\n',header),header
    abi=subprocess.check_output(['aarch64-linux-gnu-readelf','-l','-d','-V',str(path)],text=True)
    interpreter=re.findall(r'Requesting program interpreter:\s*([^\]]+)',abi)
    assert interpreter==['/lib/ld-linux-aarch64.so.1'],(path,interpreter)
    versions=[tuple(map(int,value.split('.'))) for value in re.findall(r'GLIBC_(\d+\.\d+)',abi)]
    assert versions and max(versions)<=(2,39),(path,versions)
    with path.open('rb') as stream: digest=hashlib.file_digest(stream,'sha256').hexdigest()
    rows.append(dict(package=name,path=str(path),sha256=digest,elf_header=header,abi=abi))
    seen.add(name)
assert seen==packages,dict(missing=sorted(packages-seen))
Path(sys.argv[2]).write_text(json.dumps(rows,indent=2)+'\n')
PY
last_step=completed
printf '%s\n' 'Affected nine-package GNU AArch64 build only; no execution, full GNU/musl gate, AGNOS ABI or device acceptance.' > "$arm_evidence/BUILD_ONLY"
