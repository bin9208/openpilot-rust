# Explicit Android SDK and Maven preparation (#508)

Tracking: [#508](https://github.com/bin9208/openpilot-rust/issues/508),
[PR #500](https://github.com/bin9208/openpilot-rust/pull/500) → `dev`.

The actual `2765b4ab1c15a05c72d23c4ca32c033011c14543` integration run
[39855085548](https://github.com/bin9208/openpilot-rust/actions/runs/39855085548)
failed before its application checks at two inherited hosted-image assumptions:

- [Android job120091100797](https://github.com/bin9208/openpilot-rust/actions/runs/39855085548/job/120091100797)
  reported `SDK location not found` during Gradle app lint/build task dependency
  resolution. Java17 was already selected. This is not a failed app assertion.
- [Phone mapping job120091101306](https://github.com/bin9208/openpilot-rust/actions/runs/39855085548/job/120091101306)
  reported `mvn: command not found`, exit127. No mapping test ran.

`jetlink.yml` now prepares a job-owned SDK before the original Gradle command.
The neutral provisioning block is reused from the other chat's common #502
source `77663836a95a9e2aff9aa66592d8b06ed9c87cd9`, without importing its commit
history or any dev-md runtime source. The corresponding common
[Android job120114000688](https://github.com/bin9208/openpilot-rust/actions/runs/39853578189/job/120114000688)
succeeded on `bin-actions`; that is bounded setup evidence, not acceptance of
this new stable-dev candidate. The existing hash-pinned setup action installs
platform35 and build-tools35.0.0, exactly as declared by this Jetlink app.
Python3.12 is selected explicitly for the unchanged strict APK inspector.
Both SDK environment paths point inside the owned job temporary directory and
are removed through its existing ownership-checked cleanup.

`naver-ci.yml` now prepares Maven using `tools/ci/apt-native`, retaining foreign
architecture registration and the existing Java21 selection. The original
Maven mapping tests and subsequent Java-produced strict wire check are unchanged.
Both preparation steps check observed free space against estimated operation
growth before installation: 4 GiB for SDK/app work, 1 GiB for Maven/dependencies.
These are operation estimates, not a restored fixed runner reserve. Both existing
job deadlines, Gradle tasks, APK checks, application sources and trust gates stay
unchanged. No vehicle, Android device or #296 conversion work is involved.

The fast policy regression authenticates both failed historical workflow blobs,
confirms the missing preparation, then verifies preparation order, SDK versions
against the app source, owned SDK paths and the unchanged real consumer commands.
The actual application checks still run in their original integration jobs.
Local validation is limited to syntax, YAML parsing and diff inspection. New
exact-source fast checks, full Android/Java comparisons, required gates, normal
merge and separate post-merge validation remain pending.
