# Feedback runtime port

[#131](https://github.com/bin9208/openpilot-rust/issues/131) preserves the source's active bookmark loop and disabled LKAS audio-recording branch. The native daemon and unchanged-source process both passed the same live IPC scenario with four bookmarks and no audio recording, including when the recording Param is enabled. See [validation](../rust-port/feedbackd-validation.md) for the command, dependencies and limits. Full-runtime delivery remains under [#1](https://github.com/bin9208/openpilot-rust/issues/1); this is host evidence only.
