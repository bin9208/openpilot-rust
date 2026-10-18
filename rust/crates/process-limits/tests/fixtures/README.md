# Owned QA source process-title boundary

`source-title-original.tsv` contains five unchanged `setproctitle` 1.3.7
observations (ordinary, Unicode, 256-byte, empty and embedded-NUL titles).
`source-title-provenance.json` pins the imported external module and capture
script. No native candidate output supplies expected values. The extra
`source-title-noenv-original.json` records its dynamic argv-window rule:
505 initial bytes yield 503 title bytes, while `/proc/self/environ` stays intact.

After a missing-module RED (E0432 with dependent E0282), the first three
real-process tests passed. A subsequent feature-unification review reproduced
two non-source invocations incorrectly claiming title storage. Initialization
now admits only the QA peer's naming mode or `check-managed-entry --managed-source`,
before any allocation or pointer rebinding; other modes/binaries stay untouched.
All four process tests passed in 0.04 seconds, feature-on all-target strict Clippy
passed in 0.37 seconds, and both original non-source RED probes now reject with
Unsupported. The default feature-off library also compiled. Actual parent
observations read `/proc/<pid>/cmdline` and `comm`, with same-PID handshakes before
prepare, after real file prepare and after naming. Failed prepare and embedded
NUL leave both unchanged. Concurrent argument/environment readers and an actual
inheriting `/usr/bin/env` child retain the initial values.

The opt-in `qa-process-title` feature is for owned Linux/glibc QA executables,
not production daemon naming or general shared-library loading. An ELF preinit
callback first checks the explicit owned invocation, then copies startup
argv/environment strings and rebinds their pointer tables before ordinary DSO
constructors and Rust argument initialization. Cargo feature unification does
not authorize modifying a production daemon, outer worker or collector. Only
the original, contiguous kernel title window is later overwritten, through one
main-thread owner. Copies are deliberately retained until process exit. There
is no `/proc/self/mem` access, guessed memory address, exec-time argv0 substitute,
or late modification of storage used by Rust argument readers. The peer has a
`.preinit_array` section; the feature-off test ELF has none.

Primary implementation contracts inspected, without copying external code:

- [glibc preinitialization](https://github.com/bminor/glibc/blob/master/elf/dl-init.c)
- [Rust 1.94 deferred argv reads](https://github.com/rust-lang/rust/blob/1.94.0/library/std/src/sys/args/unix.rs)
- [setproctitle 1.3.7 process-title window](https://github.com/dvarrazzo/py-setproctitle/blob/version-1.3.7/src/spt_status.c)

Ignored evidence: `.analysis/scratch/2026-10-19-managed-entry/title-*` and
`title/`. Managed-source integration and its full original schedule remain
separate gates; this boundary does not claim those tests passed.
