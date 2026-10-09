# Cluster runtime conversion: issue 253

The registered `carrot_cluster` process remains unported. Its conversion is
tracked in [issue 253](https://github.com/bin9208/openpilot-rust/issues/253) under
the complete-runtime gate in [issue 1](https://github.com/bin9208/openpilot-rust/issues/1).
This document records the initial source boundary; it is not implementation or
device evidence.

## Source boundary

`openpilot/system/manager/process_config.py` registers
`openpilot.selfdrive.carrot.cluster_autorun` with the existing HUD predicate and
restart-on-crash behavior. `cluster_autorun.py` owns Params selection, USB
discovery/recovery and GPU startup coordination. It invokes `cluster_run.py`,
which sets the locale/display scheduling and enters `cluster/main.py` with live
input by default. The active main path owns the source's render/input/output
choices through modules in `openpilot/selfdrive/carrot/cluster/`.

The conversion must preserve live Params, camera/navigation/radar presentation,
USB display selection and hotplug, JPEG/hardware/software encoding choices,
worker cleanup and the existing nondefault runtime options. Existing converted
messaging, VisionIPC and graphics/encoder components should be reused where
their contracts match. Kernel/firmware, external graphics, USB and codec
dependencies remain explicit; calling the original Python runtime is not a port.

Preserve onroad core 7 with SCHED_OTHER/nice 19 and offroad little-core placement,
including encoder children. Keep the current 10 FPS rate, reduced to 5 FPS while
UsbGpuActive. Do not restore legacy placement/rate overrides.

## Implementation boundaries checked before assignment

- The existing `ui-application::scheduling::Scheduler` exposes its core and child
  PID sweep. A cluster consumer must select core 7; its default core 6 belongs to
  the main UI. Reuse the sweep and offroad restoration contract rather than
  introducing another affinity implementation.
- `cluster_renderer.py` uses external raylib through pyray, while
  `cluster_scene.py` owns the geometry and cache rules. Reuse the native graphics
  boundary where its API fits, but retain the cluster's own scene, camera,
  navigation, labels, themes and display preferences. Main-UI rendering does not
  establish cluster rendering equivalence.
- `cluster_h264_pipeline.py` selects the project-owned
  `system/loggerd/libcluster_h264_encoder_bridge.so`, software FFmpeg, NV12 input
  buffers and output queues. The existing `encoderd` V4L2/software implementation
  is a reuse candidate, not evidence that this bridge is already converted.
  Its project-owned C++ behavior remains inside this issue's conversion scope.
- `cluster_h264_decoder.py` also loads a project-owned bridge, backed by
  `system/loggerd/decoder/cluster_h264_decoder{,_bridge}.cc`. Preserve decoder
  generation, DMA-buffer release and disable-on-import-failure behavior.
  `cluster_gles_dmabuf.py` and `cluster_gles_readback.py` own EGL image, framebuffer,
  PBO and fence lifetimes around external EGL/GLES providers.
- `cluster_usb_display.py` owns TURZX commands, chunking, acknowledgement,
  display setup and USB-disconnect handling. External libusb is a provider;
  these Python protocol and recovery rules still require Rust ownership. Keep
  the shared USB GPU bus-lock coordination.
- The normal autorun path enters live input. The command also supports random,
  gamepad, route and navigation inputs, plus window, USB and combined output.
  Account for these runtime choices explicitly; offline review/validation
  scripts are separate from the installed runtime dependency closure.

The first integration evidence must exercise the actual cluster entry point
with owned Params/IPC and a recorded USB recipient, plus rendered frames from
the source and native implementation. Isolated geometry or USB traces alone
cannot change the registered process to complete.

## Delivery gate

Implementation and owned host/render/IPC evidence remain pending. Exact-commit
host/aarch64 checks, separate post-merge validation, and composition with normal
startup and existing log upload precede the user's first device comparison.
Do not connect to or deploy to the C3X or NAS. The runtime inventory stays
`not_ported` until the actual implementation and corresponding evidence exist.
