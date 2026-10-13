# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

import json
import random
from pathlib import Path
import sys


def main() -> None:
  output = Path(sys.argv[1])
  output.mkdir(parents=True, exist_ok=True)
  normal = b"MemTotal: 100 kB\nMemAvailable: 20 kB\n"
  cases = {
    "normal": [(normal, b"cpu 0 0 0 0\ncpu0 100 0 0 400\n"), (normal, b"cpu0 120 0 0 400\n")],
    "negative": [(b"MemTotal: 100\nMemAvailable: -20\nMemFree: 80\n", b"cpu0 -10 0 0 20\n"), (b"MemTotal: -100\n", b"cpu0 0 0 0 20\n")],
    "invalid-utf8": [(b"MemTotal: 1\xff00\nMemAvailable: 2\xff0\n", b"cpu0 10\xff0 0 0 400\n"), (normal, b"cpu0 12\xff0 0 0 400\n")],
    "cpu-overflow": [(normal, b"cpu0 18446744073709551615 18446744073709551615 0 0\n")],
    "memory-overflow": [(b"MemTotal: 18446744073709551615\nMemFree: 18446744073709551615\nBuffers: 18446744073709551615\nCached: 18446744073709551615\n", None)],
    "unicode": [(normal, "cpu١ ١_٠٠ 0 0 ٤٠٠\ncpu² +10 0 0 40\n".encode()), (normal, "cpu١ ١٢٠ 0 0 ٤٠٠\ncpu² +20 0 0 40\n".encode())],
    "malformed": [(b"MemTotal: x\nMemTotal: 100\nMemAvailable: nope\nCached: 20\n", b"cpu0 1 nope 2 3\ncpu1 1 2 3\n"), (None, None)],
    "fallback-overflow": [(b"MemTotal: 1\nMemFree: 18014398509481983\nBuffers: 18014398509481983\nCached: 18014398509481983\n", None)],
    "average-rounding": [(normal, b"cpu0 0 0 0 0\ncpu1 0 0 0 0\ncpu2 0 0 0 0\n"), (normal, b"cpu0 1 0 0 99\ncpu1 1 0 0 999999999999999999\ncpu2 1 0 0 999999999999999999\n")],
  }
  huge = 10**500
  cases["huge-ratio"] = [(normal, f"cpu0 {huge} 0 0 {huge * 4}\n".encode()), (normal, f"cpu0 {huge * 2} 0 0 {huge * 7}\n".encode())]
  cases["line-decoding"] = [("MemTotal::\u00a0+１_０_０\r\nMemAvailable:\x1f20\x85".encode() + b"Cached: 2\xe2\x82\n", "cpu²\u00a0+١٠٠ 0 0 ٤٠٠\u2028cpu٠ 10 0 0 40\u2029".encode()), (normal, "cpu²\u00a0١٢٠ 0 0 ٤٠٠\x0bcpu٠ 20 0 0 40\x0c".encode())]
  cases["previous-retention"] = [(normal, b"cpu0 10 0 0 40\n"), (normal, None), (normal, b"cpu0 20 0 0 40\n"), (None, b"cpu0 20 0 0 40\ncpu1 1 0 0 4\n"), (normal, b"cpu0 10 0 0 0\ncpu1 2 0 0 4\n")]
  generator = random.Random(253)
  pairs = [(1, 3), (2, 3), (1, 2**1075), (3, 2**1075), (2**53 + 1, 2**54), (2**53 + 3, 2**54), (1, 2**1074), (2**52 - 1, 2**1074), (2**53 - 1, 2**1075)]
  for bits in (8, 32, 64, 128, 512, 1024, 2048, 4096):
    for _ in range(40):
      total = generator.getrandbits(bits) + 1
      pairs.append((generator.randrange(total + 1), total))
  ratios = []
  for busy, total in pairs:
    ratios.extend(((normal, b"cpu0 0 0 0 0\n"), (normal, f"cpu0 {busy} 0 0 {total - busy}\n".encode())))
  cases["ratio-corpus"] = ratios
  for name, steps in cases.items():
    (output / f"{name}.json").write_text(json.dumps({"steps": [{"memory": list(memory) if memory is not None else None, "cpu": list(cpu) if cpu is not None else None} for memory, cpu in steps]}))


if __name__ == "__main__":
  main()
