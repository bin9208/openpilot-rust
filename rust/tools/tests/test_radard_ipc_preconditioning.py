"""Regression fixtures bound to the unchanged 430ec72b input constructor."""
import hashlib
import unittest  # noqa: TID251 - focused stdlib regression; no added test dependency

from radard_ipc import packet, SERVICES, WARMUP_FRAMES


class PreconditioningTests(unittest.TestCase):
  def test_all_160_measured_packets_match_original_source_bytes(self):
    digest = hashlib.sha256()
    for index in range(40):
      for topic in SERVICES:
        digest.update(packet(topic, index, stamp_index=index).to_bytes())
    self.assertEqual(digest.hexdigest(), '9b52421501fc4cd5487623d1b61c5489067d2e6e6f1862e4fe82c78df53620ad')

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

  def test_original_timestamp_anomalies_remain(self):
    self.assertEqual(packet('modelV2', 10).modelV2.timestampEof, 0)
    self.assertEqual(packet('modelV2', 11).logMonoTime, 0)
    self.assertEqual(packet('modelV2', 11).modelV2.timestampEof, 0)
    self.assertEqual(packet('liveTracks', 22).logMonoTime, 5_500_000_000)


if __name__ == '__main__':
  unittest.main()
