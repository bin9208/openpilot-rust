# Native APT preparation isolation (#503)

Tracking: [#503](https://github.com/bin9208/openpilot-rust/issues/503), under
[issue audit #497](https://github.com/bin9208/openpilot-rust/issues/497).

## Observed failure

Registered bin-actions at source `56c05072d7a7014eeda92a0377496371f8d45341`
failed its focused updater dependency step before Rust validation:
[job120039002064](https://github.com/bin9208/openpilot-rust/actions/runs/39840230215/job/120039002064).
APT requested noble, noble-updates and noble-security binary-arm64 indexes from
archive.ubuntu.com and exited100 on404. The log establishes the inappropriate
architecture/index request, not who introduced the persistent configuration or
a general runner failure rate. The updater tests did not execute.

## Bounded correction

`tools/ci/apt-native` runs only in Actions. For native update/install operations,
it reads the package architecture with `dpkg --print-architecture`, accepts the
existing amd64/arm64 contexts, and gives sudo apt-get a private temporary config.
The config clears inherited APT::Architectures before selecting that native
package architecture. Argument bytes, package/version pins, APT exit status,
source mirrors and signature policy are preserved. Private config is removed
on success, failure and ordinary shell signals. The wrapper neither changes
dpkg architectures nor edits shared source/config files. Disabling list cleanup
preserves other architectures' cached indexes.

This isolates architecture defaults, not explicit source-level architecture
options. Sources with explicit Architectures/arch options keep their intended
meaning. Existing cross-library operations with private multiarch sources and
:arm64 packages remain explicit callers; they must not be rewritten to this
native-only wrapper. No arbitrary source rewrite or ignored APT error is used.

The config ordering and list-clearing behavior follow primary APT documentation:
[apt-get config-file](https://manpages.debian.org/bookworm/apt/apt-get.8.en.html#OPTIONS),
[apt.conf](https://manpages.debian.org/bookworm/apt/apt.conf.5.en.html), and
[source architecture options](https://manpages.debian.org/bookworm/apt/sources.list.5.en.html).

## Verification and integration

Tests use the actual apt-config parser with an owned foreign-architecture
configuration. Only sudo/dpkg command boundaries are stubbed: no packages are
installed and no shared configuration is changed by those tests. They check
amd64/arm64 defaults, literal version/glob arguments, original exit status,
private-file cleanup and failure before APT outside Actions/unsupported contexts.
A dedicated bin-actions job then runs actual update and installs the already
required capnproto/libcapnp-dev/pkg-config dependencies.

Local verification is Bash/Python/YAML syntax and diff review only. Functional
parser tests and actual package preparation remain pending in registered Actions.
The proof branch starts from stable dev with no dev-md runtime content. The
neutral helper can be integrated into the audit candidates after exact source
and workflow review; no new device or release operation is authorized.

All new jobs run on bin-actions. The separately owned #502 local ARM QEMU
foundation is not implemented here. ARM emulation must retain actual ARM code
and must not be reported as native ARM hardware validation. Existing running
checks are preserved; no successful unchanged benchmark is repeated for routing.

The proof branch excludes only its own push from the inherited broad Fast
workflow; its dedicated job validates the changed setup on bin-actions. The
base PR trigger and required fast check are not changed or reported successful
through a conditional skip. No PR is opened from this intermediate proof branch
before #502 supplies the all-local foundation. The integration candidates must
still pass all their exact-head required checks after adopting the neutral fix.
