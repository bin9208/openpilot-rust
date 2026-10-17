# Independent original service-port and SSH-wrapper captures

`original.json` records the unchanged `tools/scripts/adb_ssh.sh` heredoc and its
88 service rows, eight independent FNV-1a vectors, and eight complete shell
runs using controlled fake `adb`, `ss`, and `ssh`. No device commands ran.
The original pure `openpilot/cereal/services.py` module was loaded unchanged;
only the `cereal` package initializer was isolated to avoid unrelated native
Cap'n Proto imports. Source SHA256 values and the heredoc hash are in JSON.
The capture program remains private scratch:
`.analysis/scratch/2026-10-15-adb-service-ports/capture.py`.

`ports.txt` pins exact name ordering, port values, and final newline. `fnv.tsv`
uses hexadecimal UTF-8 name bytes, original unsigned FNV hash, and original
port. `tuple-output.txt` comes from the original set/sort behavior with duplicate
names. `collision.tsv` is an independently found pair with equal original ports;
distinct names must both remain in output.

Each case directory freezes exit status, stdout/stderr, and NUL-separated tool
argument records. Cases cover success, two occupied SSH ports, failures at the
first and last service forwards, failed SSH forwarding, failed SSH, failed `ss`,
and failed service-port production. The original intentionally observable
process-substitution contract remains: producer failure is printed to stderr,
but the wrapper still forwards SSH and calls `ssh`. The migration preserves it.
The actual migrated shell tests exclude Python from PATH and use a source-matched
native binary selected through `OPENPILOT_SERVICE_PORTS`.
