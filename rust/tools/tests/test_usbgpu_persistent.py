"""Persistent buffer records are data ranges, never executable pickle globals."""
import io
import pickle
import unittest
from usbgpu_hcq_data import ArtifactError, DataUnpickler, PersistentBuffer


class PersistentReferences(unittest.TestCase):
  def load(self, pid, arena=(128, 1024)):
    target = object()
    stream = io.BytesIO()
    writer = pickle.Pickler(stream)
    writer.persistent_id = lambda obj: pid if obj is target else None
    writer.dump(target)
    return DataUnpickler(io.BytesIO(stream.getvalue()), [], arena).load()

  def test_byte_ranges_match_pinned_helpers(self):
    value = self.load((16, 256))
    self.assertIsInstance(value, PersistentBuffer)
    self.assertEqual((value.offset, value.size), (384, 16))

  def test_malformed_ranges_fail_closed(self):
    for pid in [(1, -1), (-1, 0), (1025, 0), (4, 1022), (True, 0), 'path', (1,), (1, 2, 3)]:
      with self.subTest(pid=pid), self.assertRaises(ArtifactError):
        self.load(pid)
    with self.assertRaises(ArtifactError):
      self.load((1, 0), None)


if __name__ == '__main__':
  unittest.main()
