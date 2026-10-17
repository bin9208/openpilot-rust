# Portable service tools

`list-service-ports` prints the original sorted service-name/TCP-port rows used
by `tools/scripts/adb_ssh.sh`. It consumes the existing generated native
messaging catalog in `../messaging/src/services_generated.rs`; it does not import
Python or maintain another copy of the service values. The package and tests
have no Cargo dependencies.

The port algorithm retains unsigned FNV-1a over UTF-8 bytes with wrapping
multiplication and the original exclusive upper port bound. Distinct service
names that collide on a port remain separate rows. Producer failure behavior in
the SSH wrapper remains unchanged: the process-substitution failure is reported
but SSH forwarding and SSH may proceed. This conversion does not change that
existing contract.

Validation includes independently captured original 88-row output and FNV
vectors, an actual collision pair, and eight full shell argument/failure cases
with fake ADB/SSH and no Python in PATH. Bootstrap tests execute the actual
helper with controlled toolchain/disk tools for Linux x86_64 and Darwin
arm64/x86_64. Darwin host selection is simulated on Linux; execution on a real
macOS host remains unverified. No real device commands are part of these tests.
