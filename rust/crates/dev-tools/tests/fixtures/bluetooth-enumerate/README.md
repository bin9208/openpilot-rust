# Original Bluetooth enumeration observation

`original-result.json` is the unchanged original Python 3.12 controller's raw
report from GitHub Actions run `39499806776`, artifact `12302620012`. The run's
checked-out source was `0d1700e5a3ad0181ecfdd7f7f25e4fc9d088e1d2`; its
workflow head was `6acd3681`. These are distinct identities. The downloaded
ZIP SHA-256 was
`7363d70dc5fab935242f796a2cd8608e7a70f214b3a21141275bee4747bb46d7`.
The raw report SHA-256 is
`36d25b8684cb814421dd22ada7bc75b3d9959c38433e4a5b75c70e02375ac18c`.
It contains all 40 nodes, 31 accepted entries in filesystem iteration order,
and the original path prefix
`/home/runner/work/_temp/ui-connectivity/bluetooth-enumerate/original/dev/`.

The retained `artifacts.sha256` recorded the actual candidate input-fixture
ELF as `cb3bab053299768cd326d37bd964e8026549f3ddfa0e2bb6c919c68ac36922ff`,
the native checker ELF as
`8b8351a35c2da7f4bf07bd6e755e8ac7350889fd21e927ad0f4101454622a8b7`,
and the original Python checker as
`2adaabcdf0ed4f6ccab1e466fa095e0c32b0b956c7de2cca833fbc0ef55e9fe0`.
The original stdout was
`PASS: 40 sysfs nodes, 31 accepted in source order, missing directory\n`;
stderr was empty. This fixture is original-only. The native controller binds
its bytes by SHA-256, maps only the original output-root prefix to its own
fresh output root, and independently executes the candidate ELF twice. A new
candidate's ELF hash is checked against that run's own binary, not the old ELF.

The historical checker remains tracked. The next cloud gate executes the same
fresh native controller ELF with the present checkout and a copied root missing
only `rust/tools/check_bluetooth_enumerate.py`. The live original Python lane
also remains mandatory. The reference compares full reports, stdout/stderr,
fixture streams and ordered device maps after normalizing only the output-root
prefix. The workflow hashes the executable files before and after the lanes and
uses an `execve` trace to require exactly one controller and two candidate ELF
executions in the absent lane. Its result is pending cloud execution; this
source preparation did not execute either controller locally. The checker is
not deleted and no caller changes are made here.
