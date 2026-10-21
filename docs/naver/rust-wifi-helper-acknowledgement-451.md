# Complete Wi-Fi helper acknowledgement publication (#451)

Full #296 PR39552825804/job119065662797 failed the completed-helper cleanup test:
its readiness record contained zero fields, where PID, descendant PID and nonce
were required. The helper redirected printf directly into the final path; shell
redirection created that path before printf wrote any bytes. Waiting for is_file
could therefore observe an empty record. This fails before ownership/cleanup
assertions and does not establish a runtime descendant leak.

Commit4b1da110 writes and closes a private sibling `.pending` file before `/bin/mv`
publishes the complete record in the same directory. The same three-field, PID,
nonce, pidfd identity, surviving-descendant, FIFO, cleanup, deadline and nonzero
exit17 assertions remain. Production Wi-Fi, scheduling and helper lifetime code
are unchanged. The observed full-job failure supplies the original failure
control; no redundant test of shell rename itself was added.

A complete maintained UI/connectivity rerun is pending in39554819822, checking
3a90dc52732b9afcde31c31ae20578bf3109e532 under workflow
ae1c64b69cbcca9a8f65d98cdc07f46d8f306edf. Local checks were syntax/format/diff;
there was no local functional execution or device test. Keep the issue open until
the actual regression and all remaining connectivity comparisons are inspected.


## Accepted hosted regression

The complete affected UI/connectivity job
[39554819822/job119072022342](https://github.com/bin9208/openpilot-rust/actions/runs/39554819822/job/119072022342)
passed source `3a90dc52732b9afcde31c31ae20578bf3109e532` under workflow
`ae1c64b69cbcca9a8f65d98cdc07f46d8f306edf`. The parent read the complete log:
the exact previously failing completed-helper test passed, as did the full Wi-Fi
package/strict Clippy, actual private NetworkManager methods/settings/signals/
snapshots/cleanup, seven-event callback-ordering gate, complete shared UI pixel/
input/lifecycle,20actual cweb HTTP cases, six cweb modes, and final Bluetooth
input/BlueZ/prompt/lifecycle checks. The expected old buffered-write negative
control still failed as required; it is not a current test failure.

Artifact12327774829 ZIP SHA256 is
`bfd0782284fe0b09bbd6dff81d97a679405aea7894da297435ba779a67f287e3`
(67,683,616bytes,2,004members; parent CRC audit). This accepts the narrow
acknowledgement publication repair. It does not resolve the independent historical
intermittent UI #384 cause, latest #296 aggregate gate, or vehicle acceptance.
