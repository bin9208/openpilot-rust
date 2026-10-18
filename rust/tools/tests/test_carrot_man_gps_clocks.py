import unittest

from carrot_man_gps_clocks import normalize


class GpsClocks(unittest.TestCase):
  def test_independent_epochs_preserve_identical_semantics(self):
    rows = [dict(positionFixMonoTime=t, positionMonoTime=t+1_000_000,
                 positionSource='device', positionHeadingValid=True, xPosLat=37.) for t in (10_000_000_000, 100_000_000_000)]
    for row in rows:
      normalize(row)
    self.assertEqual(rows[0], rows[1])
    self.assertEqual(rows[0]['positionSource'], 'device')

  def test_no_fix_has_zero_receipt_and_no_invented_course(self):
    row = dict(positionFixMonoTime=0, positionMonoTime=10_000_000_000, positionSource='none', positionHeadingValid=False)
    normalize(row)
    self.assertEqual(row, dict(positionSource='none', positionHeadingValid=False))

  def test_future_stale_wrong_type_or_unknown_source_is_not_normalized_away(self):
    for source, fix, now, heading in [('device', 2, 1, True), ('vehicle', 1, 2_500_000_001, True),
      ('phone', 1, 3_000_000_001, True), ('none', 1, 2, False), ('none', 0, 1, True),
      ('unknown', 1, 2, False), ('device', True, 2, True), ('device', 0, 1, True)]:
      with self.subTest(source=source, fix=fix, now=now), self.assertRaises(AssertionError):
        normalize(dict(positionFixMonoTime=fix, positionMonoTime=now, positionSource=source, positionHeadingValid=heading))


if __name__ == '__main__':
  unittest.main()
