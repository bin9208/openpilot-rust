# Statistics checker queue allocation readiness (#435)

Tracking: [issue435](https://github.com/bin9208/openpilot-rust/issues/435),
parent[issue296](https://github.com/bin9208/openpilot-rust/issues/296).

Exact source62e8c235ccc1fa6e109d3c02d9bd76102047efba failed the
[actual interrupted-receive gate](https://github.com/bin9208/openpilot-rust/actions/runs/39514415599/job/118943760360)
with `collector queue capacity reduced`; six other tests passed. No queue
lengths were retained, so the failure alone does not identify its cause.

The checker previously authorized subscribing after both queue paths existed.
The queue constructor creates each file before extending its allocation. A
file may therefore exist while smaller than the unchanged10MiB requirement.
The repair waits for both regular files to exceed that requirement within
the original20-second startup deadline. It checks cancellation and collector
liveness before accepting even full files, retains post-wait capacity
validation, and reports observed lengths on capacity timeout.

Four deterministic readiness tests cover staged zero/partial allocations,
full-file success, capacity deadline/cancellation/dead-child rejection and
missing-file classification. The existing actual five-case receive-fault
source/native comparison and original flush/fatal assertions stay required.
Cloud behavioral validation and exact fixed-SHA evidence are pending. This
checker change is not a production msgq capacity change or device result.
