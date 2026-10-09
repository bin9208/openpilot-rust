# Stock UAS result-window correction (#160)

Issue: https://github.com/bin9208/openpilot-rust/issues/160
Runtime conversion: https://github.com/bin9208/openpilot-rust/issues/154

The inherited `USB3._send_batch_locked` clears `tr_window` after a completed
31-command window but retains `op_window`. Later submissions append results
for old slots again. This is a source defect, not a Rust equivalence target.
The original Python source remains unchanged.

`rust/tools/reproduce_usbgpu_uas_window.py` executes the unchanged source methods
with an owned transfer-completion boundary. One-byte sequential reads produce:

| Input commands | Original returned results |
| --- | --- |
| 1 | 1 |
| 30 | 30 |
| 31 | 31 |
| 32 | 63 |
| 33 | 96 |
| 62 | 1488 |

In the 33-command case, result index 32 contains stale byte 1 rather than byte
32. The native `Usb3::send_batch` scopes read indices to each submitted window
and returns exactly one ordered result per command. Regression cases cover
1, 30, 31, 32, 33, 62 and 63 reads plus persistent command-template tails.

The only repository runtime caller of `USB3.send_batch` is
`ASM24Controller.exec_ops`. Its `read` consumer joins the ordered responses and
truncates to the requested byte count; stale duplicate entries corrupt this
contract. Write-only callers discard response entries. No inspected source
consumer relies on duplicate response metadata. Native `StockAsm` through its
actual `Usb3` layer matches the unchanged stock controller at an owned SCSI
boundary for 49 scenarios, including 31/32/33/62 read chunks, cached writes,
PCIe requests and SRAM transfer sizes. This validates consumer semantics without
claiming equivalence to the defective source USB3 window implementation.

Reproduction and regression:

```sh
python3 rust/tools/reproduce_usbgpu_uas_window.py --evidence .omo/evidence/usbgpu-154/uas-source-defect.json
cargo test --manifest-path rust/Cargo.toml -p openpilot-usbgpu --test usb3
python3 rust/tools/check_usbgpu_stock_asm.py --binary <target>/debug/examples/stock_asm_trace --evidence .omo/evidence/usbgpu-154/stock-asm-traces.json
```

Captured host artifacts: `uas-source-defect.json`, `uas-window-tests.log`,
`uas-consumer-audit.json`, and `stock-asm-traces.json` under
`.omo/evidence/usbgpu-154/` in the issue worktree. Original hardware, full runtime
startup and device acceptance are not established by these fixtures. The issue
stays open pending committed-SHA integration/CI and the applicable acceptance
gate; no vehicle test was requested or performed.
