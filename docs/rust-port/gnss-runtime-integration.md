# GNSS runtime integration

Issue [#137](https://github.com/bin9208/openpilot-rust/issues/137) integrates u-blox [#129](https://github.com/bin9208/openpilot-rust/issues/129) and Qualcomm [#130](https://github.com/bin9208/openpilot-rust/issues/130). Full runtime acceptance remains [#1](https://github.com/bin9208/openpilot-rust/issues/1).

The native candidates are `openpilot-pigeond`, `openpilot-ubloxd`, `openpilot-qcomgpsd` and the separate `nmeaport` diagnostic helper. Manager catalog entries expose candidate availability; production daemon selection is unchanged. Original source licensing and provenance remain in the individual component records.

The required `rust GNSS runtime` job builds both packages and runs all source, serial and continuous-process comparisons. The `rust checks` aggregate rejects GNSS failure, cancellation, skipping and missing results. The existing workspace checks, original runtime comparisons, inherited integration gate, mapped docs check and aarch64 build remain intact.

The u-blox checks compare source framing and complete publications, nine receiver policy traces, source/native PTY termios behavior, sanitizer-instrumented modem-line ioctls and both actual daemons with original msgq peers. The Qualcomm checks compare 204 source report cases, setup/teardown transcripts, actual native serial/IPC processes, assistance download/injection/retry paths and the separate NMEA helper. Details and local evidence are in [u-blox validation](ublox-validation.md) and [Qualcomm validation](../naver/rust-qcomgps-130.md). CI retains its synthetic protocol captures under `rust-gnss-evidence`.

Fixtures use owned PTYs, loopback servers, private GPIO files and isolated IPC. They do not access the user's receiver, modem or vehicle. External dependencies include Linux UART/GPIO APIs, receiver/modem firmware, ModemManager, native msgq, logging, Params and HTTP/TLS. Host evidence and generic ARM builds do not establish satellite reception, AGNOS hardware timing, CPU savings or device acceptance. Complete normal startup and the existing log upload path remain the final runtime integration gate before the user's first device comparison.

Docs-Not-Needed: experimental runtime composition and CI coverage; no production setting or public guide behavior changes.
