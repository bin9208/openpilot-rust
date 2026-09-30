# Sensor and audio runtime integration

Issue [#128](https://github.com/bin9208/openpilot-rust/issues/128), under full-runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).

This integration combines native LSM6DS3 sensor publication (#123), shared PortAudio callback ownership and alert playback (#124), microphone raw samples and pressure calculation (#126), and active bookmark feedback (#131). Source-disabled audio recording remains disabled.

The mandatory `rust sensor and audio runtime` job builds the actual binaries and original IPC/Params bindings. It compares sensor policy and Linux ABI calls, microphone numeric/raw-byte results, sound asset/sample/state behavior and bookmark publications, and runs continuous processes through owned hardware fixtures. PortAudio callback ownership is checked with strict Miri. The job participates in `rust checks`; inherited push, integration, docs and ARM requirements remain.

Initial composed head 5afd79cb passed sensor/audio, startup services, platform, hardware, support, telemetry, model and route logger jobs in [Actions 36765100151](https://github.com/bin9208/openpilot-rust/actions/runs/36765100151). Startup prerequisites exposed the original collector readiness race tracked and fixed by [#133](https://github.com/bin9208/openpilot-rust/issues/133). Subsequent checks include that deterministic delayed-publisher regression and feedbackd. Fresh exact-head CI remains required before merge.

Component details: [sensord](sensord-validation.md), [soundd](soundd-validation.md), [micd](micd-validation.md), [feedbackd](feedbackd-validation.md). Original msgq/CXX, Linux I2C/GPIO/clock/scheduling interfaces and external PortAudio remain explicit native dependencies. Host ABI fixtures and generic ARM compilation do not demonstrate AGNOS hardware behavior. Manager selection, complete native startup and existing log upload remain pending under #1; no vehicle has been contacted.
