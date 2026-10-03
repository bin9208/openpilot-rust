import errno
import os
import select
import struct
import threading

from ublox_fixture import frame


class Receiver:
  def __init__(self, fd):
    self.fd = fd
    self.commands = []
    self.failures = []
    self.stopped = threading.Event()
    self.thread = threading.Thread(target=self.run)
    self.thread.start()

  def run(self):
    buffer = b''
    try:
      while not self.stopped.is_set():
        if not select.select([self.fd], [], [], 0.05)[0]:
          continue
        try:
          data = os.read(self.fd, 4096)
        except OSError as error:
          if error.errno == errno.EIO:
            continue
          raise
        buffer += data
        while buffer:
          if buffer.startswith(b'$'):
            position = buffer.find(b'\r\n')
            if position < 0:
              break
            packet, buffer = buffer[: position + 2], buffer[position + 2 :]
            self.commands.append(list(packet))
            continue
          assert buffer[:1] == b'\xb5', buffer
          if len(buffer) < 6:
            break
          size = struct.unpack_from('<H', buffer, 4)[0] + 8
          if len(buffer) < size:
            break
          packet, buffer = buffer[:size], buffer[size:]
          assert frame(int.from_bytes(packet[2:4], 'big'), packet[6:-2]) == packet
          self.commands.append(list(packet))
          match packet[2:4]:
            case b'\x09\x14':
              reply = frame(0x0914, bytes([3, 0, 0, 0, 3, 0, 0, 0]))
            case b'\x13\x40' | b'\x13\x02':
              reply = frame(0x1360, bytes(8))
            case b'\x06\x04':
              continue
            case _:
              reply = frame(0x0501, packet[2:4])
          os.write(self.fd, reply)
    except Exception as error:
      self.failures.append(repr(error))

  def close(self):
    self.stopped.set()
    self.thread.join(timeout=2)
    assert not self.thread.is_alive()
    assert not self.failures, self.failures
