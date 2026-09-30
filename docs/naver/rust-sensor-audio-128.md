# Sensor and audio integration

[#128](https://github.com/bin9208/openpilot-rust/issues/128) combines sensord, soundd, micd and feedbackd behind a mandatory native runtime CI job. Source comparisons, actual process IPC, owned Linux/PortAudio ABI fixtures and Miri are described in [the integration ledger](../rust-port/sensor-audio-integration.md). The first composed run passed the new sensor/audio job; the registration fixture startup race exposed elsewhere in the same run is tracked in #133. Complete runtime/device delivery remains under #1.
