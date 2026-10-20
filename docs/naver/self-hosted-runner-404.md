# Ubuntu self-hosted Actions runner (#404)

Issue: https://github.com/bin9208/openpilot-rust/issues/404

## Scope and observed registration (2026-10-20 KST)

GitHub REST reported runner ID 21, name `bin-actions`, Linux/X64,
version 2.337.0, online and idle. Registration does not prove installed
packages, Ubuntu version or build readiness.

The actual readiness run on source commit
`646bde2752f00d2954cb845853f3feec585ccc85` confirmed **Ubuntu 26.04.1 LTS**,
only **4 GiB free**, missing git/unzip/zstd/rustup/cargo/gh/gcc/g++/make/pkg-config,
and password-required sudo. The tool cache is writable.
[Readiness run](https://github.com/bin9208/openpilot-rust/actions/runs/39400253235)
failed as intended on these missing prerequisites.
[Fast checks](https://github.com/bin9208/openpilot-rust/actions/runs/39400253462)
was routed to bin-actions; it was cancelled after the disk shortfall was
confirmed, before allowing further dependency installation/build work.

The user explicitly requested this migration, overriding the older self-hosted
prohibition for this CI task. Work starts from `dev-md` on
`codex/feat-404-self-hosted-actions`; any PR targets `dev-md`.
Do not integrate this track into dev/main or alter the separate #296 checkout.

All trusted X64 jobs use `[self-hosted, Linux, X64]`. Fork pull requests retain
GitHub-hosted X64 routing. Existing ARM64 native jobs and matrix entries retain
`ubuntu-24.04-arm`: the sole registered X64 runner cannot execute native ARM64
validation. Cross-compilation jobs run on X64 as before. Hosted-only cleanup of
Android, GHC, .NET and CodeQL is guarded by `runner.environment`.
Existing triggers, required check names, source-repository guards, disabled
publisher state and release policy remain unchanged. This branch alone does
not change workflows on dev-md; merge requires subsequent verification.

## Required host version

Use **Ubuntu 24.04 amd64** for the current workflows: they pin LLVM 18,
FFmpeg 6.1.1 and noble cross-architecture APT sources. Do not apply the noble
sources to Ubuntu 22.04/26.04. The observed host is 26.04.1; installing basic tools there alone will not
resolve the pinned native dependencies. Prepare an Ubuntu 24.04 VM or
dedicated runner environment and register it for this repository before
full validation. A container-based adaptation is not implemented by this
change. Retire or relabel the incompatible 26.04 runner before making the
24.04 runner eligible for the same Linux/X64 jobs.

Run the following on the runner host as the **runner service account**, not
on a vehicle or an unrelated workstation. These commands are instructions;
the agent has not installed packages on the host.

## Bootstrap commands

**Do not run the installation commands on the current 4 GiB-free host.**
Expand the runner disk or free space first, and prepare the 24.04 environment.
Before each installation, check space. Keep at least 25 GiB plus expected
growth; recover at least 35 GiB if below the floor. The readiness check's
35 GiB minimum is only a bootstrap floor, not a guarantee for AGNOS builds.
Existing per-build growth checks remain authoritative.

```bash
cat /etc/os-release
uname -m
df -h .
sudo apt-get update
sudo apt-get install -y \
  ca-certificates curl git git-lfs gh sudo build-essential pkg-config \
  tar gzip bzip2 xz-utils unzip zip zstd rsync jq \
  python3 python3-pip python3-venv python3-dev \
  libssl-dev libffi-dev libbz2-dev liblzma-dev libsqlite3-dev \
  libreadline-dev zlib1g-dev libncursesw5-dev tk-dev uuid-dev
git lfs install

# Check free space again before downloading toolchains.
df -h .
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/openpilot-rustup-init.sh
sh /tmp/openpilot-rustup-init.sh -y --profile minimal --default-toolchain none
. "$HOME/.cargo/env"
rustup toolchain install 1.94.0 --profile minimal --component rustfmt,clippy
rustup target add --toolchain 1.94.0 aarch64-unknown-linux-gnu aarch64-unknown-linux-musl
```

Existing workflow steps install the pinned nightly when required. No need
to preinstall every nightly, Python package, Node, Java or Android SDK.

### Runner service environment

In the directory containing the runner's `config.sh` and `svc.sh`:

```bash
# cd /actual/path/to/actions-runner
mkdir -p "$HOME/actions-toolcache"
touch .env
sed -i '/^AGENT_TOOLSDIRECTORY=/d' .env
printf 'AGENT_TOOLSDIRECTORY=%s/actions-toolcache\n' "$HOME" >> .env
export PATH="$HOME/.cargo/bin:$PATH"
./env.sh
sudo ./svc.sh stop
sudo ./svc.sh start
sudo ./svc.sh status
```

`env.sh` captures the runner account's PATH for the service; a Rust installation
in another account or an interactive-only `.bashrc` is insufficient.
The writable tool cache allows `setup-python`, `setup-node` and `setup-java`
to install requested versions. If the runner is not installed as a service,
restart `run.sh` from this account with the exported PATH instead.

Current workflows invoke sudo to install system packages and stage native
dependencies. On a dedicated runner account, configure passwordless sudo:

```bash
# Run as the runner service account.
runner_account=$(id -un)
printf '%s ALL=(ALL) NOPASSWD: ALL\n' "$runner_account" | sudo tee /etc/sudoers.d/openpilot-actions >/dev/null
sudo chmod 0440 /etc/sudoers.d/openpilot-actions
sudo visudo -cf /etc/sudoers.d/openpilot-actions
sudo -n true
```

Use a dedicated CI machine/account: existing build steps can modify system
packages, APT architectures and source setup. One registered runner executes
one job at a time; the many Rust jobs will queue rather than run in parallel.

## Dependencies already installed by workflows

The workflows retain their own version-controlled installation steps:

- Host/native Rust: capnproto, libcapnp-dev, libzmq3-dev, GCC/G++,
  clang-18, libclang-18-dev, LLVM 18, CMake, Ninja, nasm, pkg-config.
- Video/audio: FFmpeg and development headers, libyuv-dev, libzstd-dev,
  SDL2; pinned FFmpeg 6.1.1 where required.
- UI: OpenGL/GLES/X11 headers, Mesa tools, Xvfb, xauth, D-Bus.
- ARM/AGNOS: GCC/G++ aarch64 cross tools, qemu-user-static, binutils,
  android-sdk-libsparse-utils, e2fsprogs, cross libc/stdc++ and ARM graphics.
- Diagnostics: strace, iproute2, util-linux, tmux.
- Python 3.12, Node 24, Temurin Java 17/21, Gradle 9 and Android SDK/build-tools
  are installed by existing setup actions or wrappers, along with each job's
  pinned Python requirements.

Inherited release integration also invokes `./tools/op.sh setup`, which
installs project dependencies and configures the environment. It is not a
minimal runner bootstrap and is deliberately left inside that existing job.
Docker is needed only for the inherited Carrot Routes image publisher, whose
repository guard excludes this repository; do not enable that publisher.

## Validation and remaining evidence

Local validation is limited to YAML/shell/Python syntax and diff review.
The new `Self-hosted runner readiness` branch-push workflow inspects the actual
host without package installation; its summary lists missing prerequisites.
The existing fast push checks remain separate from the deep PR/integration
checks. After provisioning, rerun readiness and failed fast checks, then use a
dev-md PR for the existing deep build/test gates. No device deployment or
release publication is authorized by this infrastructure change.

Runner setup references:
- https://docs.github.com/en/actions/reference/runners/self-hosted-runners
- https://github.com/actions/setup-python/blob/main/docs/advanced-usage.md#using-setup-python-with-a-self-hosted-runner
