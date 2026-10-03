"""Owned PTY modem and loopback assistance peers for native daemon checks."""
import binascii
import contextlib
import fcntl
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import pty
import select
import struct
import threading
import time
import tty


def crc(data):
  reverse = lambda value: int(f'{value:08b}'[::-1], 2)
  ccitt = binascii.crc_hqx(bytes(reverse(value) for value in data), 0xffff)
  return int(f'{ccitt:016b}'[::-1], 2) ^ 0xffff


def encode(data):
  data += struct.pack('<H', crc(data))
  return data.replace(b'\x7d', b'\x7d\x5d').replace(b'\x7e', b'\x7d\x5e') + b'\x7e'


def decode(data):
  assert data.endswith(b'\x7e')
  data = data[:-1].replace(b'\x7d\x5e', b'\x7e').replace(b'\x7d\x5d', b'\x7d')
  assert data[-2:] == struct.pack('<H', crc(data[:-2]))
  return data[:-2]


class Peer:
  def __init__(self, root):
    self.root = root
    self.at_master, self.at_slave = pty.openpty()
    self.diag_master, self.diag_slave = pty.openpty()
    for fd in [self.at_slave, self.diag_slave]:
      tty.setraw(fd)
    self.stop = threading.Event()
    self.assist = threading.Event()
    self.at = []
    self.diag = []
    self.errors = []
    self.setups = 0
    self.gps = True
    self.empty_query = False
    self.drop_commands = 0
    self.write_lock = threading.Lock()
    owner = self
    class Handler(BaseHTTPRequestHandler):
      def do_GET(self):
        owner.assist.wait(10)
        self.send_response(200)
        self.end_headers()
        with contextlib.suppress(BrokenPipeError):
          self.wfile.write(b'private synthetic assistance')
      def log_message(self, *_):
        pass
    self.http = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    self.http.daemon_threads = True
    self.threads = [threading.Thread(target=self.serve_at, daemon=True), threading.Thread(target=self.serve_diag, daemon=True),
                    threading.Thread(target=self.http.serve_forever, daemon=True)]
    for thread in self.threads:
      thread.start()
    gpio = root / 'sys/class/gpio/gpio34'
    gpio.mkdir(parents=True)
    (gpio / 'direction').write_text('in')
    (gpio / 'value').write_text('0')
    mmcli = root / 'mmcli'
    mmcli.write_text(f'#!/bin/sh\nprintf "%s\\n" "$@" >> "{root}/injections"\nexit 0\n')
    mmcli.chmod(0o755)
    self.config = {'at': {'path': os.ttyname(self.at_slave), 'lock': str(root / 'modem.lock')},
       'diagnostic': os.ttyname(self.diag_slave), 'nmea': os.ttyname(self.at_slave), 'root': str(root),
       'assistance': str(root / 'xtra3grc.bin'), 'assistance_url': f'http://127.0.0.1:{self.http.server_port}/assist',
       'alternate': None, 'mmcli': str(mmcli), 'systemd': str(root / 'systemd'), 'cold_start': False}

  def serve_at(self):
    pending = b''
    try:
      while not self.stop.is_set():
        if not select.select([self.at_master], [], [], .05)[0]:
          continue
        pending += os.read(self.at_master, 8192)
        while b'\r' in pending:
          raw, pending = pending.split(b'\r', 1)
          command = raw.decode()
          self.at.append(command)
          if self.drop_commands:
            self.drop_commands -= 1
            continue
          response = f'+QGPS: {int(self.gps)}' if command == 'AT+QGPS?' and not self.empty_query else ''
          if command == 'AT+QGPSEND':
            self.gps = False
          if command == 'AT+QGPS=1':
            self.gps = True
          os.write(self.at_master, f'{command}\r\n{response}\r\nOK\r\n'.encode())
    except OSError as error:
      if not self.stop.is_set():
        self.errors.append(repr(error))

  def serve_diag(self):
    pending = b''
    try:
      while not self.stop.is_set():
        if not select.select([self.diag_master], [], [], .05)[0]:
          continue
        pending += os.read(self.diag_master, 65536)
        while b'\x7e' in pending:
          framed, pending = pending.split(b'\x7e', 1)
          packet = decode(framed + b'\x7e')
          opcode, data = packet[0], packet[1:]
          self.diag.append(list(packet))
          response = data
          if opcode == 115:
            operation, = struct.unpack_from('<I', data, 3)
            response = struct.pack('<3xII', operation, 0)
            if operation == 1:
              response += struct.pack('<16I', 0, 0x500, 0, 9, *([0] * 12))
          self.send(opcode, response)
          if opcode == 75:
            self.setups += 1
    except (OSError, AssertionError) as error:
      if not self.stop.is_set():
        self.errors.append(repr(error))

  def send(self, opcode, payload, fragmented=False):
    raw = encode(bytes([opcode]) + payload)
    with self.write_lock:
      if fragmented:
        for part in [raw[:2], raw[2:9], raw[9:]]:
          os.write(self.diag_master, part)
          time.sleep(.01)
      else:
        os.write(self.diag_master, raw)

  def assert_exclusive(self):
    with open(self.config['diagnostic'], 'rb') as fd:
      try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
      except BlockingIOError:
        return
      raise AssertionError('native diagnostic serial was not exclusively locked')

  def close(self):
    self.stop.set()
    self.assist.set()
    self.http.shutdown()
    self.http.server_close()
    for thread in self.threads:
      thread.join(timeout=2)
    for fd in [self.at_master, self.at_slave, self.diag_master, self.diag_slave]:
      if fd >= 0:
        os.close(fd)


def wait(predicate, process, timeout=5):
  end = time.monotonic() + timeout
  while time.monotonic() < end:
    if predicate():
      return
    assert process.poll() is None, f'child exited {process.returncode}'
    time.sleep(.01)
  raise AssertionError('condition deadline exceeded')
