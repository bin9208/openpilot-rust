# Native startup services integration

[#120](https://github.com/bin9208/openpilot-rust/issues/120) continues the complete
runtime conversion after platform integration #116. This branch currently combines
native LPA #115, messaging bridge #121 and AGNOS image/casync/CLI #119;
startup UI #117 and updated #118 are being integrated into the same increment.

The required `rust startup services` Actions job builds native binaries and the
process helper, compiles the unchanged original C++ bridge, then compares owned
serial/TLS LPA exchanges, real bidirectional TCP/msgq traffic and verified AGNOS
image/casync/CLI behavior using owned files and harmless slot commands. The aggregate
`rust checks` requires this job alongside every existing runtime/workspace gate.
Ten CI policy tests reject missing, failed, cancelled or skipped dependencies.
The existing sanitizer job also covers dynamic queued-subscription lifetimes.

Component-focused source/native results are reused from the linked validation
records. Broad workspace, sanitizer, generic ARM and integration checks run in
Actions according to the user's requested faster workflow. All original source
licensing and explicit external dependencies remain in those records.

Normal production startup, complete daemon adoption and existing log upload
comparison remain pending the rest of the full-runtime conversion. No device
access, deployment, CPU savings or device acceptance is claimed here.

Docs-Not-Needed: internal runtime candidates and required CI, with no selected
user setting or workflow change.
