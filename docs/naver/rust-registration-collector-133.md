# Registration collector readiness

Issue [#133](https://github.com/bin9208/openpilot-rust/issues/133). Actions run [36765100151](https://github.com/bin9208/openpilot-rust/actions/runs/36765100151), startup job 110057164136, lost all three expected `errorLogMessage` records while the unchanged source's `logMessage` stream retained the records and end marker.

`Peer.start` waits for queue files. Original msgq creates these files before `msgq_init_publisher` resets reader IDs. The error publisher is created second. A source collector can therefore invalidate its early error subscriber; its first late read reconnects at the current write pointer and discards the records. This is a fixture startup race, not evidence of changed registration policy.

`registration_collector_delay.c` intercepts only the original collector's error-queue truncation and pauses after the real file resize, before publisher initialization. With an 800 ms delay, the unmodified registration fixture reproduces the identical error-record equality failure in `clock_year_ten_thousand`. The fix sends and consumes the existing debug barrier immediately after starting the collector, priming the error subscriber before starting either registration producer. It retains exact error equality and all runtime timeouts.

The targeted delayed source/native comparison passes after the fix. The required startup job repeats this case and confirms both original collector processes actually took the delayed path. Raw before/after host evidence is kept in the integration scratch archive; Actions retains the corresponding job artifact. No production runtime or device behavior changed.
