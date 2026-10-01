"""Owned PTY and loopback TLS carrier fixture with synthetic profile material only."""

import base64
from collections import Counter
import http.server
import json
import os
import pty
import select
import ssl
import subprocess
import sys
import threading


def tlv(tag, value):
  raw = tag.to_bytes(2 if tag > 255 else 1, 'big')
  size = len(value)
  length = bytes([size]) if size < 128 else bytes([0x82]) + size.to_bytes(2, 'big')
  return raw + length + value


def b64(value):
  return base64.b64encode(value).decode()


def tbcd(value):
  padded = value + ('F' if len(value) % 2 else '')
  return bytes(int(padded[i + 1] + padded[i], 16) for i in range(0, len(padded), 2))


BPP = tlv(
  0xBF36,
  tlv(0xBF23, b'fixture-init')
  + tlv(0xA0, b'configure')
  + tlv(0xA1, tlv(0x87, b'M' * 145))
  + tlv(0xA2, b'keys')
  + tlv(0xA3, tlv(0x88, b'fixture-profile-elements')),
)
SIGNED2 = tlv(0x30, tlv(0x80, b'fixture-transaction') + tlv(1, b'\0'))


def certificate(root):
  ca, ca_key = root / 'ca.pem', root / 'ca-key.pem'
  key, csr, cert = root / 'key.pem', root / 'leaf.csr', root / 'leaf.pem'
  subprocess.run(
    [
      'openssl',
      'req',
      '-x509',
      '-newkey',
      'rsa:2048',
      '-nodes',
      '-keyout',
      ca_key,
      '-out',
      ca,
      '-days',
      '1',
      '-subj',
      '/CN=LPA Fixture CA',
      '-addext',
      'basicConstraints=critical,CA:TRUE',
    ],
    check=True,
    capture_output=True,
  )
  subprocess.run(['openssl', 'req', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', csr, '-subj', '/CN=localhost'], check=True, capture_output=True)
  extensions = root / 'extensions.cnf'
  extensions.write_text('subjectAltName=DNS:localhost\nbasicConstraints=critical,CA:FALSE\nextendedKeyUsage=serverAuth\n')
  subprocess.run(
    ['openssl', 'x509', '-req', '-in', csr, '-CA', ca, '-CAkey', ca_key, '-CAcreateserial', '-out', cert, '-days', '1', '-extfile', extensions],
    check=True,
    capture_output=True,
  )
  return ca, key


class Fixture:
  def __init__(self, root, cert, key, launcher, mode='normal'):
    self.root, self.mode = root, mode
    self.commands, self.requests, self.errors = [], [], []
    self.counts = Counter()
    self.buffer = b''
    self.more = b''
    self.master, self.slave = pty.openpty()
    self.device = root / "at"
    self.device.symlink_to(os.ttyname(self.slave))
    self.stop = threading.Event()
    self.thread = threading.Thread(target=self.serve, daemon=True)
    self.thread.start()
    reset = root / 'reset'
    reset.write_text(
      f'#!{sys.executable}\nfrom pathlib import Path\np=Path({str(root / "resets")!r})\np.write_text(p.read_text()+"reset\\n" if p.exists() else "reset\\n")\n'
    )
    reset.chmod(0o755)
    self.config = {'device': str(self.device), 'timeout_ms': 100, 'lock': str(root / 'lock'), 'reset': str(reset), 'launcher': str(launcher)}
    fixture = self

    class Handler(http.server.BaseHTTPRequestHandler):
      protocol_version = 'HTTP/1.1'

      def log_message(self, *args):
        pass

      def do_POST(self):
        payload = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        endpoint = self.path.rsplit('/', 1)[1]
        fixture.requests.append(
          {
            'endpoint': endpoint,
            'payload': payload,
            'cookie': self.headers.get('Cookie'),
            'user_agent': self.headers.get('User-Agent'),
            'protocol': self.headers.get('X-Admin-Protocol'),
            'content_type': self.headers.get('Content-Type'),
          }
        )
        if fixture.mode == 'http_timeout':
          fixture.stop.wait(3)
          self.close_connection = True
          return
        reply = fixture.http_reply(endpoint)
        body = json.dumps(reply).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Set-Cookie', 'session=fixture; Path=/; Secure')
        self.end_headers()
        self.wfile.write(body)

    self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert.parent / "leaf.pem", key)
    self.server.socket = context.wrap_socket(self.server.socket, server_side=True)
    self.address = f'localhost:{self.server.server_port}'
    self.http_thread = threading.Thread(target=self.server.serve_forever, daemon=True)
    self.http_thread.start()

  def http_reply(self, endpoint):
    if endpoint == 'initiateAuthentication':
      return {
        'transactionId': b64(b'fixture-transaction'),
        'serverSigned1': b64(tlv(0x30, b'signed1')),
        'serverSignature1': b64(tlv(0x5F37, b'signature')),
        'euiccCiPKIdToBeUsed': b64(tlv(0x04, b'key')),
        'serverCertificate': b64(tlv(0x30, b'cert')),
      }
    if endpoint == 'authenticateClient':
      if self.mode == 'cancel':
        return {'header': {'functionExecutionStatus': {'status': 'Failed', 'statusCodeData': {'reasonCode': '3.8', 'subjectCode': '8.2.6'}}}}
      return {
        'profileMetadata': b64(tlv(0xBF25, tlv(0x5A, tbcd('123456789012345')))),
        'smdpSigned2': b64(SIGNED2),
        'smdpSignature2': b64(tlv(0x5F37, b'sig2')),
        'smdpCertificate': b64(tlv(0x30, b'cert2')),
      }
    if endpoint == 'getBoundProfilePackage':
      return {'boundProfilePackage': b64(BPP)}
    if endpoint in ('handleNotification', 'cancelSession', 'fixture'):
      return {}
    raise AssertionError(endpoint)

  def result(self, data):
    tag = int.from_bytes(data[:2], 'big') if data[0] & 31 == 31 else data[0]
    self.counts[tag] += 1
    if tag == 0xBF2D:
      profiles = b''
      for iccid, provider in [('8985235123456789012', b'Webbing'), ('123456789012345', b'Fixture')]:
        fields = tlv(0x5A, tbcd(iccid)) + tlv(0x91, provider) + tlv(0x90, '별명'.encode()) + tlv(0x9F70, b'\1')
        profiles += tlv(0xE3, fields)
      response = tlv(tag, tlv(0xA0, profiles))
      self.more = response[9:]
      return response[:9], 0x61, len(self.more)
    if tag in (0xBF29, 0xBF33, 0xBF30):
      return tlv(tag, tlv(0x80, b'\0')), 0x90, 0
    if tag == 0xBF31:
      return tlv(tag, tlv(0x80, b'\5' if self.counts[tag] == 1 else b'\2')), 0x90, 0
    if tag == 0xBF2E:
      return tlv(tag, tlv(0x80, b'0123456789ABCDEF')), 0x90, 0
    if tag == 0xBF20:
      return tlv(tag, b'fixture-euicc-info'), 0x90, 0
    if tag in (0xBF38, 0xBF21, 0xBF41):
      return tlv(tag, tlv(0xA0, b'fixture-response')), 0x90, 0
    if tag == 0xBF28:
      entries = b''
      for seq in (1, 2):
        entries += tlv(0xBF2F, tlv(0x80, bytes([seq])) + tlv(0x81, b'\0\x80') + tlv(0x0C, self.address.encode()))
      return tlv(tag, tlv(0xA0, entries)), 0x90, 0
    if tag == 0xBF2B:
      # First pending notification is malformed; the second must still be sent and removed.
      value = b'' if self.counts[tag] == 1 else tlv(0x30, b'fixture-notification')
      return tlv(tag, tlv(0xA0, value)), 0x90, 0
    if tag in (0xBF36, 0xA0, 0xA1, 0x87, 0xA2, 0xA3):
      return b'', 0x90, 0
    if tag == 0x88:
      final = tlv(0xA1, tlv(0x80, b'\5') + tlv(0x81, b'\x09')) if self.mode == 'install_error' else tlv(0xA0, b'')
      inner = tlv(0xBF2F, tlv(0x80, b'\7')) + tlv(0xA2, final)
      return tlv(0xBF37, tlv(0xBF27, inner)), 0x90, 0
    raise AssertionError(f'unknown command {data.hex()}')

  def respond(self, command):
    self.commands.append(command)
    self.counts[command] += 1
    if command.startswith('AT+CCHO='):
      if self.mode == 'open_retry' and self.counts[command] <= 6:
        return ['ERROR']
      return ['+CCHO: 1', 'OK']
    if command.startswith('AT+CCHC='):
      return ['OK']
    if command == 'AT+RECONNECT':
      if self.counts[command] == 1:
        old_master, old_slave = self.master, self.slave
        self.master, self.slave = pty.openpty()
        self.device.unlink()
        self.device.symlink_to(os.ttyname(self.slave))
        os.close(old_master)
        os.close(old_slave)
        return []
      return ['RECONNECTED', 'OK']
    if command == 'AT+TIMEOUT':
      return []
    if command == 'AT+BAD':
      return ['+CME ERROR: 10']
    if command.startswith('AT+CGLA='):
      apdu = bytes.fromhex(command.split('"')[1])
      if self.mode == 'apdu_retry' and self.counts['apdu_failures'] < 2:
        self.counts['apdu_failures'] += 1
        return ['+CGLA: 4,"ZZZZ"', 'OK']
      if apdu[1] == 0xC0:
        data, sw1, sw2 = self.more, 0x90, 0
        self.more = b''
      else:
        if apdu[3] == 0:
          self.buffer = b''
        self.buffer += apdu[5:]
        if apdu[2] == 0x91:
          data, sw1, sw2 = self.result(self.buffer)
        else:
          data, sw1, sw2 = b'', 0x90, 0
      payload = (data + bytes([sw1, sw2])).hex().upper()
      return [f'+CGLA: {len(payload)},"{payload}"', 'OK']
    raise AssertionError(command)

  def serve(self):
    buffered = b''
    while not self.stop.is_set():
      if not select.select([self.master], [], [], 0.05)[0]:
        continue
      try:
        buffered += os.read(self.master, 4096)
        while b'\r' in buffered:
          raw, buffered = buffered.split(b'\r', 1)
          command = raw.decode().strip()
          if command:
            lines = self.respond(command)
            if lines:
              os.write(self.master, ('\r\n' + '\r\n'.join(lines) + '\r\n').encode())
      except Exception as error:
        self.errors.append(str(error))
        return

  def close(self):
    self.stop.set()
    self.thread.join(timeout=2)
    self.server.shutdown()
    self.server.server_close()
    self.http_thread.join(timeout=2)
    os.close(self.master)
    os.close(self.slave)
