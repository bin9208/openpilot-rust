# Bluetooth sudo fixture conversion

`rust/tools/bluetooth_sudo_fixture.py` is the unchanged 20-line source fixture
(SHA-256 `3bdbff27c2e9fe1a582f2089450d5a30c3b59a0a27a1966e3ba246448e248482`).
`check_bluetooth_input.py` and `bluetooth_daemon_fixture.py` currently copy it
to a private `sudo` command path. The actual child calls are
`-n chgrp gpio PATH` and `-n chmod g+rw PATH`.

The native helper appends one Python `json.dumps(sys.argv[1:])` line to
`INPUT_FIXTURE_SUDO_LOG`, using Unix surrogateescape for arbitrary path bytes.
Its modes are `ok` (default), `timeout` (sleep ten seconds), `signal`
(self-SIGTERM), `loud` (1 MiB on each stream), and `fail-<second argument>`
(exit 7 for the matching command). Other mode values succeed. Arguments are
unrestricted: the helper logs them before mode handling, and an absent second
argument fails only after mode handling. A missing log environment fails.
The error text for an absent second argument is not a traceback parity target.
The original script remains in place until live proof and caller migration.

The cloud gate must set `BLUETOOTH_SUDO_ORIGINAL_PYTHON` to the full path of
the unchanged script and `BLUETOOTH_SUDO_EVIDENCE` to a fresh, caller-owned
directory under the uploaded artifact root. The bounded process-pair test
creates that directory and persists each case's argv, mode, raw logs,
stdout/stderr, exit code, elapsed time and a result manifest. It executes both helpers
with identical valid arguments and environments, including arbitrary command
tokens and invalid Unix bytes. It compares append logs byte-for-byte,
stdout/stderr bytes, exits or SIGTERM status, and bounded timing for the
timeout mode. Child supervision has a 20-second deadline. Pin
the source script and both executables before and after the comparison. Then
run the existing input and daemon controllers with the migrated caller against
the same source fixture behaviors. No local functional execution was done.
