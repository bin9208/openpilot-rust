#!/usr/bin/env python3
"""SpasSteerTest (phase 13): the parking ECU's SPAS1 steering request, parked, on a car with no stock SPAS sender.

libsafety directly, like test_hyundai_spas_solo. From opendbc_repo:
  PYTHONPATH=. python -m unittest opendbc.safety.tests.test_hyundai_spas_steer
"""
import unittest

from opendbc.safety.tests.libsafety import libsafety_py
from opendbc.safety.tests.test_hyundai_spas_solo import SAFETY_HYUNDAI_CANFD, SPAS_FLAG, SPAS1, SPAS2, canfd, stalk

ASK, ACTIVE, IDLE = 0x10, 0x14, 0x0C  # byte 11: mode 4 / 5 / 3 << 2


def steer(counter: int, mode: int, angle_deci: int, extra: dict | None = None) -> bytes:
  fill = {3: 0x4B, 11: mode}
  raw = angle_deci & 0xFFFF
  fill[12], fill[13] = raw & 0xFF, raw >> 8
  fill.update(extra or {})
  return canfd(SPAS1, 24, counter, fill)


class TestHyundaiSpasSteer(unittest.TestCase):
  def setUp(self):
    self.safety = libsafety_py.libsafety
    self.assertEqual(self.safety.set_safety_hooks(SAFETY_HYUNDAI_CANFD, SPAS_FLAG), 0)
    self.safety.init_tests()
    self.now = 1_000_000
    self.counter = 0
    self.wheel = 0
    self.lever_left = False
    self.torque = 0
    self.c = 0
    self.gear = 0  # P 0, D 5, N 6, R 7
    self.brake = False
    self.avh = False

  def rx(self, address, data, bus=0):
    return self.safety.safety_rx_hook(libsafety_py.make_CANPacket(address, bus, data))

  def tx(self, address, data, bus=0):
    return self.safety.safety_tx_hook(libsafety_py.make_CANPacket(address, bus, data))

  def tick(self, ms: int):
    """Parked inputs at 100 Hz (lever 5 Hz) and the EPS torque frame, as test_hyundai_spas_solo."""
    for _ in range(ms // 10):
      self.now += 10_000
      self.safety.set_timer(self.now)
      self.counter = (self.counter + 1) & 0xFF
      speeds = {8: self.wheel, 10: self.wheel, 12: self.wheel, 14: self.wheel}
      self.rx(0xA0, canfd(0xA0, 24, self.counter, speeds))
      self.rx(0x35, canfd(0x35, 32, self.counter, {24: self.gear}))
      self.rx(0x60, canfd(0x60, 32, self.counter, {24: 1 if self.avh else 0}))
      self.rx(0x175, canfd(0x175, 24, self.counter, {10: 0x02 if self.brake else 0}))
      if self.counter % 20 == 0:
        self.rx(0x3C1, stalk(left=self.lever_left))
      raw = 4095 + self.torque
      eps = bytearray(24)
      eps[10], eps[11] = raw & 0xFF, (raw >> 8) & 0x1F
      self.rx(0xEA, bytes(eps))

  def send(self, mode=ACTIVE, angle=0, step_ms=20, extra=None, counter=None):
    self.tick(step_ms)
    c = self.c if counter is None else counter
    self.c = (c + 1) & 0xFF
    return bool(self.tx(SPAS1, steer(c, mode, angle, extra)))

  def test_flag_off_refuses(self):
    self.assertEqual(self.safety.set_safety_hooks(SAFETY_HYUNDAI_CANFD, 0), 0)
    self.safety.init_tests()
    self.tick(1000)
    self.assertFalse(self.send(ASK))

  def test_arms_on_ask_then_ramps(self):
    self.tick(1000)
    self.assertFalse(self.send(ACTIVE), "arms only on mode 4")
    self.assertTrue(self.send(ASK))
    for _ in range(4):
      self.assertTrue(self.send(ASK))
    angle = 0
    for _ in range(200):  # 100 deg/s at 50 Hz: 2 deg per frame, to 400 deg
      angle += 20
      self.assertTrue(self.send(ACTIVE, angle))
    self.assertTrue(self.send(IDLE, angle))

  def test_step_and_range_limits(self):
    self.tick(1000)
    self.assertTrue(self.send(ASK, 0))
    self.assertFalse(self.send(ACTIVE, 31), "more than 3 deg in one frame")
    self.assertTrue(self.send(ACTIVE, 30))
    self.setUp()
    self.tick(1000)
    self.assertFalse(self.send(ASK, 4601), "past 460 deg")
    self.assertTrue(self.send(ASK, -4600))

  def test_only_the_steering_fields(self):
    self.tick(1000)
    self.assertTrue(self.send(ASK))
    self.assertFalse(self.send(ACTIVE, 0, extra={20: 1}))
    self.assertFalse(self.send(0x18, 0), "mode 6")
    self.assertFalse(self.send(ACTIVE, 0, extra={3: 0x4A}))
    # SPAS2 only all-zero
    self.tick(20)
    self.assertTrue(self.tx(SPAS2, canfd(SPAS2, 32, 0)))
    self.tick(20)
    self.assertFalse(self.tx(SPAS2, canfd(SPAS2, 32, 1, {16: 0x60})))

  def test_not_parked_or_stock_sender_does_not_arm(self):
    self.wheel = 5
    self.tick(1000)
    self.assertFalse(self.send(ASK))
    self.setUp()
    self.tick(1000)
    self.rx(SPAS1, canfd(SPAS1, 24, 1))
    self.assertFalse(self.send(ASK))
    self.setUp()
    self.lever_left = True
    self.tick(1000)
    self.assertFalse(self.send(ASK))

  def test_moving_torque_or_lever_ends_it(self):
    for change in ("wheel", "torque", "lever"):
      self.setUp()
      self.tick(1000)
      self.assertTrue(self.send(ASK))
      self.assertTrue(self.send(ACTIVE, 20))
      if change == "wheel":
        self.wheel = 5
      elif change == "torque":
        self.torque = 300
      else:
        self.lever_left = True
      self.tick(200)
      self.assertFalse(self.send(ACTIVE, 40), change)

  def test_fifteen_seconds_then_five_second_cooldown(self):
    self.tick(1000)
    self.assertTrue(self.send(ASK))
    for _ in range(749):  # up to 14.98 s after arming
      self.assertTrue(self.send(ACTIVE, 0))
    self.assertFalse(self.send(ACTIVE, 0), "15 s limit")
    self.tick(2000)
    self.assertFalse(self.send(ASK), "cooldown")
    self.tick(4000)
    self.assertTrue(self.send(ASK), "re-armed")

  def test_host_gap_ends_it(self):
    self.tick(1000)
    self.assertTrue(self.send(ASK))
    self.assertFalse(self.send(ACTIVE, 0, step_ms=250))

  def test_drive_or_reverse_only_while_held(self):
    for gear, brake, avh, ok in ((5, True, False, True), (5, False, True, True), (7, True, False, True),
                                 (5, False, False, False), (6, True, False, False), (6, False, True, False)):
      self.setUp()
      self.gear, self.brake, self.avh = gear, brake, avh
      self.tick(1000)
      self.assertEqual(self.send(ASK), ok, (gear, brake, avh))

  def test_releasing_the_hold_in_drive_ends_it(self):
    for release in ("brake", "avh"):
      self.setUp()
      self.gear = 5
      self.brake, self.avh = release == "brake", release == "avh"
      self.tick(1000)
      self.assertTrue(self.send(ASK))
      self.assertTrue(self.send(ACTIVE, 20))
      self.brake = self.avh = False
      self.tick(200)
      self.assertFalse(self.send(ACTIVE, 40), release)

  def test_spas2_gv80_body_whole_only(self):
    body = {3: 0x26, 4: 0x41, 5: 0x19, 6: 0x04, 7: 0x01, 15: 0x13}
    self.tick(1000)
    self.assertTrue(self.send(ASK))
    self.tick(20)
    self.assertTrue(self.tx(SPAS2, canfd(SPAS2, 32, 0, body)), "the whole GV80 body")
    partial = dict(body)
    del partial[15]
    self.tick(20)
    self.assertFalse(self.tx(SPAS2, canfd(SPAS2, 32, 1, partial)), "a partial body")
    self.tick(20)
    self.assertFalse(self.tx(SPAS2, canfd(SPAS2, 32, 2, {**body, 20: 1})), "body plus another byte")

  def test_one_refused_frame_does_not_block_the_ramp(self):
    # A frame refused for spacing must not leave every later frame over the step.
    self.tick(1000)
    self.assertTrue(self.send(ASK, 0))
    angle = 0
    for _ in range(5):
      angle += 20
      self.assertTrue(self.send(ACTIVE, angle))
    angle += 20
    self.assertFalse(self.send(ACTIVE, angle, step_ms=10), "too soon")
    for _ in range(10):  # the ramp keeps going at 2 deg per frame
      angle += 20
      self.assertTrue(self.send(ACTIVE, angle), angle)

  def test_step_still_bounded_after_a_gap(self):
    self.tick(1000)
    self.assertTrue(self.send(ASK, 0))
    # after one 20 ms frame the bound is 3 deg; a 40 deg jump is refused
    self.assertFalse(self.send(ACTIVE, 400))

  def test_blinker_trial_still_arms(self):
    # The existing parked blinker trial (all-zero SPAS1) is unchanged.
    self.tick(1000)
    self.assertTrue(self.tx(SPAS1, canfd(SPAS1, 24, 0)))


if __name__ == "__main__":
  unittest.main()
