# dev-md camera wait isolation (#338)

Source: `ajouatom/openpilot:carrot-wip` at
`e8e152dbcfd9574a1791ed12026d1880e87689b3`. Work remains on `dev-md`;
`dev` remains `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.
No camera device, vehicle or real sensor was accessed.

The Rust camera owner now captures fence IDs and all stress decisions before
submitting only the driver IFE/BPS wait to a persistent worker. An eventfd wakes
the original owner, which retains synchronization, recovery, buffer recycling,
exposure and publication. Road/wide waits retain their original synchronous
policy. Driver events queue in order, with the original 32-event oldest-drop
limit; existing raw/request gap recovery handles overflow. Completion processing
precedes the next request-manager event. Camera shutdown joins the wait before
fence/device destruction. A separately owned duplicate sync descriptor prevents
worker FD lifetime from depending on a borrowed camera object.

An absent driver sensor node disables that stream while road/wide startup
continues. Original configured-camera synchronization and its raw-frame >40
fallback remain unchanged, including when a configured sensor is missing.
No validity limits, wait durations, exposure values or scheduling were relaxed.

## Verification on 2026-10-16

- The final source C++ lifecycle initially failed the old native trace on its
  first frame: all stress decisions now precede waiting, including the BPS
  decision when IFE fails. After the split, 512 scenarios / 72,493 transitions
  match complete state, frame float bits and ordered I/O. This passes HOST,
  Rust ARM versus HOST source, and actual ARM C++ versus ARM Rust.
- Four unchanged upstream AsyncFrameWait regressions pass HOST and ARM. Native
  tests verify a blocked worker leaves the owner free, one completion per each
  of 500 reused requests, error propagation back to the owner, worker reuse,
  and joining an in-flight wait before resource cleanup.
- Five native runtime library tests pass HOST/ARM, including missing driver
  node without any sensor probe/allocation; missing road/wide remains fatal.
  The broader camera/kernel/request/packet/frame-state test suites also pass
  on both architectures. Strict all-target Clippy passes all three crates.
- Ten complete daemon scenarios per architecture pass with actual VisionIPC,
  cereal camera-state publication, native logging, and inert camera kernel ABI
  recipients. Cases cover TERM/INT/PWR, disabled/missing road, missing driver
  node, blocked driver, failed wait/poll, and publication order. All exposure
  register writes and published fields match the original source state owner;
  device/session/link/buffer/fence cleanup is audited.
- The blocked-driver case holds the wait until both road/wide have handled at
  least 36 events. No driver publication occurs while blocked. Queue overflow
  is observed, release recovers, and all three streams subsequently publish.
  HOST kernel fixtures run with address/undefined-behavior sanitizers. ARM
  uses actual ARM daemon, fixture and original state oracle under QEMU.

The first ARM integration run exposed a fixture association error: register
writes were keyed by the latest global dequeue round, which can advance during
an asynchronous driver wait. The fixture now derives the completed request ID
from the owner's actual scheduled replacement request. No runtime output or
comparison tolerance changed. Fresh runs pass all 20 scenarios with this fix.

The inherited camera CI now includes both missing-driver-node and blocked-driver
cases, final pinned lifecycle extraction, and multi-descriptor eventfd polling.
Receipts are in `.analysis/scratch/2026-10-16-dev-md-camera/`.
These gates establish owned HOST/ARM behavior, not real camera timing, CPU savings
or vehicle acceptance. Full integrated runtime packaging and exact-SHA CI remain
separate outstanding gates.
