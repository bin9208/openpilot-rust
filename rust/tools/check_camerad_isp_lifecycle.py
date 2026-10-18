"""Shared ownership normalization; ISP CLI is native check-camerad-isp-lifecycle."""
from collections import Counter


def normalize(source: list[dict], native: list[dict]) -> tuple[list[dict], dict]:
  allocations = {}
  for record in source:
    if record.get("name") == "camera:274" and record["ret"] == 0:
      before, after = bytes.fromhex(record["before"]), bytes.fromhex(record["after"])
      allocations[int.from_bytes(after[92:96], "little")] = int.from_bytes(before[84:88], "little")
  owned = {fd for fd, flags in allocations.items() if flags != 0x58}
  blobs = {fd for fd, flags in allocations.items() if flags == 0x811}
  closed = Counter(record["fd"] for record in native if record["op"] == "close" and record["fd"] in owned)
  assert closed == Counter(owned), (closed, owned)
  extra_unmaps = [record for record in native if record["op"] == "munmap" and record["fd"] in blobs]
  assert len(extra_unmaps) == len(blobs)
  for fd in blobs:
    observed = [record for record in native if record["op"] == "memory" and record["stage"] == "release" and record["fd"] == fd]
    expected = [dict(record, stage="release") for record in source if record["op"] == "memory" and record["stage"] == "snapshot" and record["fd"] == fd]
    assert observed == expected, f"BPS config allocation {fd} changed between acquire and release"
  output = [
    record
    for record in native
    if not (
      record["op"] == "close"
      and record["fd"] in owned
      or record["op"] == "munmap"
      and record["fd"] in blobs
      or record["op"] == "memory"
      and record["stage"] == "release"
      and record["fd"] in blobs
    )
  ]
  return output, {"native_closes_source_leaked_fds": sorted(owned), "native_unmaps_source_leaked_config": sorted(blobs)}
