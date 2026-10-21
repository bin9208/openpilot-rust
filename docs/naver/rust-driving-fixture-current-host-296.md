# Driving fixture and current-host catalog (#296, #470, #471)

Scoped packaging/staging/export gates pass in
[39654500423](https://github.com/bin9208/openpilot-rust/actions/runs/39654500423),
sourcece59159439648dddbe59904205bff97520904731,
workflowaf92c8df458b0639220eaa56c7edcf08f8601e21. The overall run is failed:
the private fault builder rejected its plain binary restoration (#473) before
retained replay, two-root model traffic and Desire began. Four actual Python
controller deletions therefore remain pending full functional acceptance.

Parent and reviewer verified all632 CRCs in artifact12371773017,138,442,842
bytes, ZIP SHA256
`afea67e1f2eb608545bf84a6416903f57d0f0bbf6892c494cb9b0b0639a0b624`.
Strict Clippy and retirement-harness compilation pass before traffic. All28 family
units, both native archive integrations, three independent original Cereal/schema
comparisons and two exporter-path regressions pass. The first full archive
integration takes196.22s; preserve this successful unchanged proof during builder
diagnosis rather than rerunning it. The regular stage also regenerates all86
images and authenticates every full original SHA256 and byte count. No expected
model packet is generated from candidate output.

#470 keeps exactly one gzip/XZ stream, validates compressed checksum/end, rejects
additional compressed bytes/streams and nonzero decoded content after the tar
terminator, and permits legitimate zero tar padding. The focused termination
regression runs before manifest rejection can hide a tail acceptance bug. Memory,
expansion, member/path and foreign-output protections remain strict.

#471 resolves existing output parents before testing historical-fixture/catalog/
receipt containment. Symlink-parent and dot-dot escapes fail before export writes.
The actual current export then succeeds into a separate output, preserving all
historical manifests. Source14 and original PKL5 pins match exact Git/frozen data;
parent also verifies all574 current catalog file sizes/hashes. Its four bundles
use the current CPU/LLVM provider instead of executing historical native objects.
Current receipt SHA256 is
`999a7e998d9f7f118f6d268c8503da1031dbd07077709636096548775554051a`;
generation is `9ff4819f0b64c157085bd3744b0c3d5e3226860bbbfeffd27c247c15a9c15812`.
The maintained native admission additionally requires CPU flags, LLVM library/
package identity and held-file rechecks around both roots; that full two-root
execution has not started in this failed run.

This accepts the bounded #470/#471 defects and staging/export evidence only.
#473 needs a builder-only diagnostic retaining before/after ELFs, Cargo commands,
logging build-script/fingerprint outputs and Git identity without relaxing the
restoration SHA check. Source compiler, driver/shared-helper retirement, final
required integration and user boot/recording/upload/driving stay separate.

The bounded native-only diagnostic39658670847 executes the same failing source
ce59159439648dddbe59904205bff97520904731 under workflow
74a34fb2ca9192b97240372d8b672c416d726fd0. Artifact12372832634 is45,354,523bytes,
ZIP SHA2564c6495afe9184020bee84d923c0c9f95d81eca5a3c4a0ce68ae4b7906110ea9b.
Parent and independent review checked all102 member CRCs, command phases,
Git states, logging build outputs and the complete pair of27,205,864-byte plain
ELFs. The unlocked package-clean command changed only tracked rust/Cargo.lock.
The next plain build embedded source_tree=dirty instead of clean. Exactly25
bytes differ: the five-character stamp and20-byte GNU build ID; every other
byte matches. The original final lock seal also failed. This identifies the
observed #473 failure; it does not prove the cause of every older build failure.

The focused candidate adds --locked to package cleanup and requires the
original lock digest before rebuilding. It retains exact plain SHA restoration,
fault distinction, all final source seals and production ELF isolation. A new
regression injects cleanup-time lock mutation and requires no later build or
success receipt. The fix is independently reviewed but hosted execution is
still required. The next bounded run retains full lock bytes/diffs per phase;
a locked-clean resolution error must fail for explicit diagnosis rather than
silently modifying source inputs. No model traffic or source-oracle comparison
was repeated for this diagnostic.
