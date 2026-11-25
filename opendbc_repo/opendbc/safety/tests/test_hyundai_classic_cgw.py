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
    self.assertFalse(d.tx(bytes(8), addr=0x5B6)[0])

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


if __name__ == "__main__":
  unittest.main()
