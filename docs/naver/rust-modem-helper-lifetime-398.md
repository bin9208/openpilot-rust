# Modem helper ownership and cancellation (#398)

Issue [#398](https://github.com/bin9208/openpilot-rust/issues/398), under
[#296](https://github.com/bin9208/openpilot-rust/issues/296), covers the modem's
external-command ownership. The implementation and regressions are source-ready;
actual baseline/fixed execution and existing source comparisons remain **CI
pending**. Local validation is limited to formatting, diff and shell syntax
checks under the user's cloud-validation workflow. No device validation or
performance benefit is claimed. LPA is unchanged.

The reviewed Python and Rust implementations both wait indefinitely for ordinary
sudo/network helpers without observing cancellation during the wait. Both can
leave descendants behind when killing only a timed-out direct `ip` child. Those
are inherited lifetime hazards. The native implementation additionally used a
temporary stdout file and accepted leader exit without captured EOF. In contrast,
the retained Python `subprocess.run(capture_output=True, text=True, timeout=2)`
waits for stdout and stderr EOF and decodes both streams. A valid `inet` line
followed by leader exit while a descendant retains a pipe must still time out.
The actual-original regression exercises these distinctions in CI; it has not
been run locally.

The native change starts each owned helper in a separate session and uses
`waitid(NOWAIT)` to retain the leader until group cleanup. Normal completion,
timeout, cancellation, reconnect and dropped PPP sessions terminate remaining
group members before reaping the leader. Cleanup is marked started before the
first signal/reap attempt, so an error does not make Drop signal the identity
again after a reap attempt. This owns ordinary descendants in the inherited
session; it does not claim control over a process deliberately escaping into a
different session.

The interface operation retains its **two-second** deadline, includes both pipe
EOFs, and validates both streams' UTF-8. A nonzero exit with valid interface text
is still accepted, as in the original. Other commands receive **no new normal
deadline**. Runtime startup, route, DNS and interface commands observe the daemon
cancellation flag. The first signal ends the runtime phase and preserves the
normal route/service restoration sequence. A subsequent signal can interrupt a
blocked restoration helper too; that explicit interruption does not promise
completion of remaining external service restoration. The public uncontrolled
trace methods retain their existing signatures and normal behavior.

`rust/crates/modem/tests/helper_lifetime.rs` contains 14 independent integration
tests using private helper scripts, acknowledged leaders/descendants and held
pidfds, plus an inherited-stdin control. The tests signal only the actual daemon leader, never its whole group.
Cleanup assertions occur before fallback pidfd cleanup. Controls include finite
EOF, retained stdout/stderr, malformed UTF-8, valid output with exit 7, ordinary
exit 17 after more than two seconds, dropped/exited PPP sessions, startup SIGINT
and SIGTERM, and a subsequent signal during service restoration. An unchanged
original-source test independently observes the original EOF/decoding behavior
and its inherited descendant leak. It never invokes host sudo, ip, pppd,
systemctl, NetworkManager, serial devices or network services.

Mandatory CI must retain `check_modem_process_boundary.py` and `check_modem.py`,
including the existing actual original/native state, reconnect, command-order,
descriptor and no-shell-fallback comparisons. Add the new integration target and
strict Clippy to the existing Linux platform-runtime job with the fresh native
launcher and Python 3.12 source environment. The narrow shared supervision API addition preserves inherited stdin while
allowing separate stdout/stderr in a new caller-owned session. No original Python file is retired by this runtime
correctness unit.

Reviewed original source identity:

| Source | SHA-256 |
| --- | --- |
| `openpilot/system/hardware/tici/modem.py` | `c193b3f5c669dab5e488ee982cbe565539a0410193b4302d524f68c157d47a13` |
| `rust/tools/modem_source.py` | `72c75ec2185003a861e1512edf06921352bb3ff02f5b27d3e1a5a743a3964060` |
| `rust/tools/modem_fixture.py` | `07ab85b2f6dcb66750a557c7b9ed4dae32503c8b2f482d2a28e984a65c63d22a` |

Exact commit, Actions URLs and baseline/fixed results must be recorded after the
grouped CI batch. Source readiness is not a successful runtime gate.
