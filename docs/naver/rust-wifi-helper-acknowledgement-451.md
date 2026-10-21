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
