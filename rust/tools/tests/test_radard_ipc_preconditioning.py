"""430ec72b input semantics across the additive #338 schema import."""
import base64
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest  # noqa: TID251 - focused stdlib regression; no added test dependency

from radard_ipc import availability_phase, packet, SERVICES, WARMUP_FRAMES, log


class PreconditioningTests(unittest.TestCase):
  def test_packet_import_without_native_ipc_binding(self):
    root = Path(__file__).resolve().parents[3]
    script = """
import hashlib
import importlib.abc
import sys
class NoIpc(importlib.abc.MetaPathFinder):
  def find_spec(self, fullname, path, target=None):
    if fullname == 'msgq' or fullname.startswith('msgq.') or fullname == 'openpilot.cereal.messaging':
      raise ModuleNotFoundError('controlled native IPC unavailable: ' + fullname)
sys.meta_path.insert(0, NoIpc())
from radard_ipc import packet, SERVICES
import radard_lifecycle
import radard_startup_state
import radard_availability_state
value = hashlib.sha256()
for index in range(40):
  for topic in SERVICES:
    value.update(packet(topic, index).to_bytes())
assert value.hexdigest() == '9873e8c127ca345ac3f1ca27e91103eafa0a0dd6a82c003f21cfed1c2a08ea36'
assert 'msgq' not in sys.modules and 'openpilot.cereal.messaging' not in sys.modules
"""
    environment = dict(os.environ, PYTHONPATH=os.pathsep.join(map(str, [root, root / 'opendbc_repo', root / 'rust/tools'])))
    subprocess.run([sys.executable, '-P', '-c', script], cwd=root, env=environment, check=True, timeout=20)

  def test_legacy_160_packets_preserve_every_decoded_field_and_default(self):
    # Generated with unchanged packet() and pre-import b55552db schemas. This
    # fixture retains the old byte fingerprint, independent of current layout.
    path = Path(__file__).with_name('fixtures') / 'radard-legacy-160.json.gz'
    rows = [base64.b64decode(row) for row in json.loads(gzip.decompress(path.read_bytes()))]
    self.assertEqual(len(rows), 160)
    self.assertEqual(hashlib.sha256(b''.join(rows)).hexdigest(),
                     '9b52421501fc4cd5487623d1b61c5489067d2e6e6f1862e4fe82c78df53620ad')
    for offset, raw in enumerate(rows):
      with log.Event.from_bytes(raw) as legacy:
        self.assertEqual(legacy.to_dict(), packet(SERVICES[offset % 4], offset // 4).to_dict(), offset)

  def test_all_160_measured_packets_match_current_schema_bytes(self):
    # vehicleGps adds a CarState pointer slot. The immutable legacy fixture
    # above still verifies every decoded field/default across this wire change.
    digest = hashlib.sha256()
    for index in range(40):
      for topic in SERVICES:
        digest.update(packet(topic, index, stamp_index=index).to_bytes())
    self.assertEqual(digest.hexdigest(), '9873e8c127ca345ac3f1ca27e91103eafa0a0dd6a82c003f21cfed1c2a08ea36')

  def test_preconditioning_precedes_original_scenario_time(self):
    for topic in SERVICES:
      stamps = []
      if topic == 'liveTracks':
        stamps.append(packet(topic, 0, stamp_index=-WARMUP_FRAMES - 1).logMonoTime)
      stamps.extend(packet(topic, max(0, index), stamp_index=index).logMonoTime
                    for index in range(-WARMUP_FRAMES, 40)
                    if not (topic == 'modelV2' and index == 11 or topic == 'liveTracks' and index == 22))
      self.assertTrue(all(a < b for a, b in zip(stamps, stamps[1:], strict=False)), topic)
      self.assertEqual(packet(topic, 0).logMonoTime, 4_990_000_000 if topic == 'liveTracks' else 5_000_000_000)

  def test_stopped_topics_straddle_deadlines_with_25ms_margin(self):
    def scheduled(index):
      return index * 0.05 + availability_phase(index)

    for last, before, after, deadline in [(6, 15, 16, 0.5), (25, 26, 27, 0.1)]:
      self.assertAlmostEqual(deadline - (scheduled(before) - scheduled(last)), 0.025)
      self.assertAlmostEqual(scheduled(after) - scheduled(last) - deadline, 0.025)
    self.assertEqual(availability_phase(6), 0.0)
    self.assertEqual(availability_phase(7), 0.025)
    self.assertEqual(availability_phase(25), 0.025)
    self.assertEqual(availability_phase(26), 0.05)
    self.assertEqual(availability_phase(39), 0.05)

  def test_original_timestamp_anomalies_remain(self):
    self.assertEqual(packet('modelV2', 10).modelV2.timestampEof, 0)
    self.assertEqual(packet('modelV2', 11).logMonoTime, 0)
    self.assertEqual(packet('modelV2', 11).modelV2.timestampEof, 0)
    self.assertEqual(packet('liveTracks', 22).logMonoTime, 5_500_000_000)


if __name__ == '__main__':
  unittest.main()
