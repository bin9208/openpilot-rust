import sys
from pathlib import Path
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'rust/tools'))
from check_joystickd_runtime import PublicationPairs


def events(*times):
    return [SimpleNamespace(logMonoTime=time) for time in times]


class PublicationPairTests(unittest.TestCase):
    def test_observed_first_state_loss_aligns_before_comparing_and_drains_cleanly(self):
        pairs = PublicationPairs()
        pairs.control.extend(events(100, 200, 300))
        pairs.state.extend(events(201, 301))
        self.assertEqual(tuple(event.logMonoTime for event in pairs.pop()), (200, 201))
        self.assertEqual(pairs.startup_unpaired, [('carControl', 100)])
        self.assertEqual(tuple(event.logMonoTime for event in pairs.pop()), (300, 301))
        self.assertIsNone(pairs.pop())
        pairs.finish()

    def test_first_control_loss_aligns_without_pairing_different_frames(self):
        pairs = PublicationPairs()
        pairs.control.extend(events(200, 300))
        pairs.state.extend(events(101, 201, 301))
        self.assertEqual(tuple(event.logMonoTime for event in pairs.pop()), (200, 201))
        self.assertEqual(pairs.startup_unpaired, [('controlsState', 101)])

    def test_waits_for_the_state_of_a_control_already_read(self):
        pairs = PublicationPairs()
        pairs.control.extend(events(100, 200))
        pairs.state.extend(events(101))
        pairs.pop()
        self.assertIsNone(pairs.pop())
        pairs.state.extend(events(201))
        self.assertEqual(tuple(event.logMonoTime for event in pairs.pop()), (200, 201))
        pairs.finish()

    def test_missing_state_after_startup_still_fails(self):
        pairs = PublicationPairs()
        pairs.control.extend(events(100, 200, 300))
        pairs.state.extend(events(101, 301))
        pairs.pop()
        with self.assertRaisesRegex(AssertionError, 'controlsState missing'):
            pairs.pop()

    def test_missing_control_after_startup_still_fails(self):
        pairs = PublicationPairs()
        pairs.control.extend(events(100, 300))
        pairs.state.extend(events(101, 201, 301))
        pairs.pop()
        with self.assertRaisesRegex(AssertionError, 'carControl missing'):
            pairs.pop()

    def test_unpaired_tail_on_either_service_still_fails(self):
        for service in ('control', 'state'):
            with self.subTest(service=service):
                pairs = PublicationPairs()
                getattr(pairs, service).extend(events(100))
                with self.assertRaisesRegex(AssertionError, 'unpaired publication'):
                    pairs.finish()


if __name__ == '__main__':
    unittest.main()
