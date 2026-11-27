#!/usr/bin/env python3
"""ClassicCgwBlinker (classic CAN, safetyParam 1024): our CGW1 (0x541) rides right behind the stock one.

Uses libsafety directly with hand-built frames (the CAN packer imports openpilot here). Run from opendbc_repo:
  PYTHONPATH=. python -m unittest opendbc.safety.tests.test_hyundai_classic_cgw
"""
import unittest

from opendbc.safety.tests.libsafety import libsafety_py

SAFETY_HYUNDAI = 8
SAFETY_HYUNDAI_LEGACY = 23
CGW_FLAG = 1024
# IONIQ PHEV (edb4204468f19127): HYBRID_GAS | LONG | CAMERA_SCC
PARAM = 2 | 4 | 8
CGW1 = 0x541
# Stock CGW1 of that car, idle (0000000f seg 9, 541.40 s)
IDLE = bytes.fromhex("03044180c0000004")


LAMP = 0x5B6
# Stock 0x5B6 of the K5 DL3 2021 while idle (UnregisteredDevice 000002ac seg 2): all zero, byte 2 0x50 on hazard
LAMP_IDLE = bytes(8)


def lamp(direction: int, stock: bytes = LAMP_IDLE) -> bytes:
  data = bytearray(stock)
  data[2] |= 0x10 if direction == 1 else 0x40
  return bytes(data)


def whl_spd11(counter: int, speed: int = 0) -> bytes:
  """WHL_SPD11 (0x386) with all four wheels at speed, the 4-bit counter and the bit-count checksum panda checks."""
  data = bytearray(8)
  for i in range(4):
    data[2 * i] = speed & 0xFF
    data[2 * i + 1] = (speed >> 8) & 0x3F
  data[1] |= (counter & 3) << 6
  data[3] |= ((counter >> 2) & 3) << 6
  bits = sum(bin(b & (0x3F if i in (1, 3, 5, 7) else 0xFF)).count("1") for i, b in enumerate(data))
  chk = (bits ^ 9) & 15
  data[5] |= (chk & 3) << 6
  data[7] |= (chk >> 2) << 6
  return bytes(data)


def ours(direction: int, lamp: bool, stock: bytes = IDLE) -> bytes:
  """The stock frame with the lever side and, when lamp, that side's lamp bit (1 left, 2 right)."""
  data = bytearray(stock)
  data[2] |= 0x08 if direction == 1 and lamp else 0
  data[7] |= 0x40 if direction == 2 and lamp else 0
  data[5] |= 0x04 if direction == 1 else 0x02
  return bytes(data)


class Device:
  def __init__(self, param=PARAM | CGW_FLAG, model=SAFETY_HYUNDAI):
    self.s = libsafety_py.libsafety
    assert self.s.set_safety_hooks(model, param) == 0
    self.s.init_tests()
    self.t = 1_000_000
    self.s.set_timer(self.t)

  def advance(self, us):
    self.t += us
    self.s.set_timer(self.t)

  def tx(self, data, bus=0, addr=CGW1):
    pkt = libsafety_py.make_CANPacket(addr, bus, data)
    self.s.safety_tx_buffered_for_fwd = False
    allowed = bool(self.s.safety_tx_hook(pkt))
    return allowed, bool(self.s.safety_tx_buffered_for_fwd)

  def wheels(self, speed=0):
    self.whl_counter = (getattr(self, "whl_counter", 0) + 1) % 16
    assert self.s.safety_rx_hook(libsafety_py.make_CANPacket(0x386, 0, whl_spd11(self.whl_counter, speed)))

  def stock_lamp(self, data=LAMP_IDLE):
    """A stock 0x5B6 arrives on bus 0: what panda sends right behind it, or None."""
    pkt = libsafety_py.make_CANPacket(LAMP, 0, data)
    self.s.safety_rx_hook(pkt)
    out = libsafety_py.ffi.new("CANPacket_t *")
    if not self.s.safety_rx_tx_hook(pkt, out):
      return None
    assert out[0].addr == LAMP and out[0].bus == 0
    return bytes(out[0].data)[:8]

  def parked(self):
    """Fresh standstill wheel speeds and a fresh idle stock 0x5B6."""
    self.wheels()
    self.stock_lamp()

  def stock(self, data=IDLE):
    """A stock CGW1 arrives on bus 0: what panda sends right behind it, or None."""
    pkt = libsafety_py.make_CANPacket(CGW1, 0, data)
    self.s.safety_rx_hook(pkt)
    out = libsafety_py.ffi.new("CANPacket_t *")
    if not self.s.safety_rx_tx_hook(pkt, out):
      return None
    assert out[0].addr == CGW1 and out[0].bus == 0
    return bytes(out[0].data)[:8]


class TestClassicCgw(unittest.TestCase):
  def test_off_without_the_flag(self):
    # Every other car: CGW1 refused, nothing held, nothing ever sent behind a stock frame.
    for model in (SAFETY_HYUNDAI, SAFETY_HYUNDAI_LEGACY):
      d = Device(PARAM, model)
      d.stock()
      d.advance(10_000)
      self.assertEqual(d.tx(ours(2, True)), (False, False))
      self.assertIsNone(d.stock())

  def test_flag_does_not_change_the_other_tx_rules(self):
    # LKAS11 / CLU11 etc. keep their lists; an unknown id stays blocked.
    d = Device()
    self.assertFalse(d.tx(bytes(8), addr=0x123)[0])
    self.assertFalse(d.tx(bytes(8), addr=0x5B6)[0])  # 0x5B6 only for the parked lamp test (TestClassicLampTest)

  def test_held_and_sent_behind_the_next_stock_frame(self):
    for model in (SAFETY_HYUNDAI, SAFETY_HYUNDAI_LEGACY):
      d = Device(model=model)
      d.stock()
      d.advance(20_000)
      self.assertEqual(d.tx(ours(2, True)), (True, True))  # accepted, but held
      d.advance(30_000)
      self.assertEqual(d.stock(), ours(2, True))
      self.assertIsNone(d.stock())  # sent once
      d.advance(60_000)
      self.assertEqual(d.tx(ours(1, False)), (True, True))
      self.assertEqual(d.stock(), ours(1, False))

  def test_needs_a_fresh_stock_template(self):
    d = Device()
    self.assertEqual(d.tx(ours(2, True)), (False, False))  # no stock frame seen yet
    d.stock()
    d.advance(260_000)
    self.assertEqual(d.tx(ours(2, True)), (False, False))  # template too old

  def test_only_lever_and_lamp_bits_may_differ(self):
    d = Device()
    d.stock()
    bad = bytearray(ours(2, True))
    bad[0] ^= 0x01
    self.assertFalse(d.tx(bytes(bad))[0])
    # lamp on the other side, both sides, no lever side
    data = bytearray(IDLE)
    data[5] |= 0x02
    data[2] |= 0x08
    self.assertFalse(d.tx(bytes(data))[0])
    data = bytearray(IDLE)
    data[5] |= 0x06
    self.assertFalse(d.tx(bytes(data))[0])
    data = bytearray(IDLE)
    data[7] |= 0x40
    self.assertFalse(d.tx(bytes(data))[0])
    # hazard switch value set by us
    data = bytearray(ours(1, True))
    data[4] |= 0x02
    self.assertFalse(d.tx(bytes(data))[0])
    # wrong bus or length
    self.assertFalse(d.tx(ours(1, True), bus=2)[0])
    self.assertFalse(d.tx(ours(1, True)[:7])[0])

  def test_driver_lever_or_hazard_in_the_stock_frame_wins(self):
    d = Device()
    lever = bytearray(IDLE)
    lever[5] |= 0x04  # the driver holds the left lever
    d.stock(bytes(lever))
    self.assertFalse(d.tx(ours(2, True, bytes(lever)))[0])
    hazard = bytearray(IDLE)
    hazard[4] |= 0x02
    d.stock(bytes(hazard))
    self.assertFalse(d.tx(ours(2, True, bytes(hazard)))[0])
    # accepted, then the driver's lever shows up in the frame it would ride behind: dropped
    d.stock()
    d.advance(60_000)
    self.assertTrue(d.tx(ours(2, True))[0])
    self.assertIsNone(d.stock(bytes(lever)))

  def test_a_stale_held_frame_is_dropped(self):
    d = Device()
    d.stock()
    self.assertTrue(d.tx(ours(2, True))[0])
    d.advance(160_000)
    self.assertIsNone(d.stock())

  def test_rate_limit(self):
    d = Device()
    d.stock()
    self.assertTrue(d.tx(ours(2, True))[0])
    d.advance(40_000)
    self.assertFalse(d.tx(ours(2, True))[0])
    d.advance(10_000)
    self.assertTrue(d.tx(ours(2, True))[0])

  def test_trial_cap_and_rearm(self):
    d = Device()
    accepted = 0
    for _ in range(int(31e6 / 60_000)):
      d.stock()
      accepted += d.tx(ours(2, True))[0]
      d.advance(60_000)
    self.assertLessEqual(accepted * 60_000, 30_000_000 + 60_000)
    self.assertFalse(d.tx(ours(2, True))[0])  # cut off
    # refused requests keep it from rearming
    for _ in range(20):
      d.advance(60_000)
      d.stock()
      self.assertFalse(d.tx(ours(2, True))[0])
    d.advance(1_000_000)
    d.stock()
    self.assertTrue(d.tx(ours(2, True))[0])  # 1 s with no request: rearmed

  def test_relay_malfunction_blocks(self):
    d = Device()
    d.stock()
    d.s.set_relay_malfunction(True)
    self.assertFalse(d.tx(ours(2, True))[0])



class TestClassicLampTest(unittest.TestCase):
  """ClassicCgwBlinkerTest 3..6: 0x5B6 with one side bit, parked only, held behind the stock 0x5B6."""

  def test_off_without_the_flag(self):
    for model in (SAFETY_HYUNDAI, SAFETY_HYUNDAI_LEGACY):
      d = Device(PARAM, model)
      d.parked()
      self.assertEqual(d.tx(lamp(1), addr=LAMP), (False, False))
      self.assertIsNone(d.stock_lamp())

  def test_parked_frame_is_held_and_sent_behind_the_stock_one(self):
    for model in (SAFETY_HYUNDAI, SAFETY_HYUNDAI_LEGACY):
      d = Device(model=model)
      d.parked()
      self.assertEqual(d.tx(lamp(1), addr=LAMP), (True, True))
      d.advance(50_000)
      d.wheels()
      self.assertEqual(d.stock_lamp(), lamp(1))
      self.assertIsNone(d.stock_lamp())  # sent once
      d.advance(60_000)
      d.wheels()
      self.assertEqual(d.tx(lamp(2), addr=LAMP), (True, True))
      self.assertEqual(d.stock_lamp(), lamp(2))

  def test_cgw1_and_lamp_frames_are_held_independently(self):
    d = Device()
    d.parked()
    d.stock()
    self.assertTrue(d.tx(ours(1, True))[0])
    self.assertTrue(d.tx(lamp(1), addr=LAMP)[0])
    self.assertEqual(d.stock(), ours(1, True))
    self.assertEqual(d.stock_lamp(), lamp(1))

  def test_needs_fresh_stock_and_fresh_standstill(self):
    d = Device()
    self.assertFalse(d.tx(lamp(1), addr=LAMP)[0])  # nothing seen yet
    d.stock_lamp()
    self.assertFalse(d.tx(lamp(1), addr=LAMP)[0])  # no wheel speeds yet
    d.wheels()
    d.advance(110_000)
    self.assertFalse(d.tx(lamp(1), addr=LAMP)[0])  # wheel speeds too old
    d.wheels()
    d.advance(60_000)
    self.assertTrue(d.tx(lamp(1), addr=LAMP)[0])
    d.advance(450_000)
    d.wheels()
    self.assertFalse(d.tx(lamp(1), addr=LAMP)[0])  # stock 0x5B6 older than 500 ms

  def test_moving_or_engaged_refuses(self):
    d = Device()
    d.stock_lamp()
    d.wheels(speed=100)
    self.assertFalse(d.tx(lamp(1), addr=LAMP)[0])
    d = Device()
    d.parked()
    d.s.set_controls_allowed(True)
    self.assertFalse(d.tx(lamp(1), addr=LAMP)[0])

  def test_held_frame_dropped_when_the_car_moves_or_engages_or_it_is_stale(self):
    d = Device()
    d.parked()
    self.assertTrue(d.tx(lamp(1), addr=LAMP)[0])
    d.wheels(speed=100)
    self.assertIsNone(d.stock_lamp())
    d = Device()
    d.parked()
    self.assertTrue(d.tx(lamp(1), addr=LAMP)[0])
    d.s.set_controls_allowed(True)
    self.assertIsNone(d.stock_lamp())
    d = Device()
    d.parked()
    self.assertTrue(d.tx(lamp(1), addr=LAMP)[0])
    d.advance(310_000)
    d.wheels()
    self.assertIsNone(d.stock_lamp())

  def test_only_one_side_bit_on_an_idle_stock_frame(self):
    d = Device()
    d.parked()
    self.assertFalse(d.tx(bytes([0, 0, 0x50, 0, 0, 0, 0, 0]), addr=LAMP)[0])  # both sides (hazard value)
    self.assertFalse(d.tx(bytes([0, 0, 0x20, 0, 0, 0, 0, 0]), addr=LAMP)[0])  # some other bit
    self.assertFalse(d.tx(bytes([1, 0, 0x10, 0, 0, 0, 0, 0]), addr=LAMP)[0])  # another byte changed
    self.assertFalse(d.tx(lamp(1), bus=2, addr=LAMP)[0])
    self.assertFalse(d.tx(lamp(1)[:7], addr=LAMP)[0])
    # the stock frame shows a real flash or the hazard: refused, and a held frame is dropped behind it
    d.stock_lamp(lamp(2))
    self.assertFalse(d.tx(lamp(1, lamp(2)), addr=LAMP)[0])
    d.stock_lamp()
    d.advance(60_000)
    d.wheels()
    self.assertTrue(d.tx(lamp(1), addr=LAMP)[0])
    self.assertIsNone(d.stock_lamp(bytes([0, 0, 0x50, 0, 0, 0, 0, 0])))

  def test_three_second_trial_then_one_second_quiet_rearms(self):
    d = Device()
    accepted = 0
    for _ in range(int(4e6 / 60_000)):
      d.wheels()
      d.stock_lamp()  # the stock frame (and our held one behind it)
      accepted += d.tx(lamp(2), addr=LAMP)[0]
      d.advance(60_000)
    self.assertLessEqual(accepted * 60_000, 3_000_000 + 60_000)
    self.assertGreater(accepted, 40)
    d.advance(1_000_000)
    d.parked()
    self.assertTrue(d.tx(lamp(2), addr=LAMP)[0])

  def test_relay_malfunction_blocks(self):
    d = Device()
    d.parked()
    d.s.set_relay_malfunction(True)
    self.assertFalse(d.tx(lamp(1), addr=LAMP)[0])


if __name__ == "__main__":
  unittest.main()
