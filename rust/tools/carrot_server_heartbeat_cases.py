from __future__ import annotations

from dataclasses import dataclass
import gzip

from carrot_server_heartbeat_peer import Response, response


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  responses: tuple[Response, ...]
  params: tuple[tuple[str, bytes], ...] = (('Version', b'owned-version'), ('GithubUsername', b'owned-user'), ('IsOnroad', b'1'))
  timestamp: float = 1700000000.9
  tls: bool = False
  timeout: bool = False


def cases() -> tuple[Case, ...]:
  encoded = gzip.compress(b'owned compressed body', mtime=0)
  result = [
    Case('success', (response(),)),
    Case('unicode-params', (response('returned 한😀'.encode()),), (('Version', '한😀'.encode()), ('GithubUsername', '유저'.encode()), ('IsOnroad', b'0'))),
    Case('missing-params', (response(),), ()),
    Case('empty-invalid-params', (response(),), (('Version', b''), ('GithubUsername', b'\xff'), ('IsOnroad', b'true'))),
    Case('negative-timestamp', (response(),), timestamp=-1.9),
    Case('utf8-replacement', (response(b'\xf0\x9f\xff\xe1\x80\x00', 201),)),
    Case('encoded-raw', (response(encoded, headers=(('Content-Encoding', 'gzip'),)),)),
    Case('encoded-short', (response(encoded[:8], headers=(('Content-Encoding', 'gzip'),), length=len(encoded)),)),
    Case('http-error', (response(b'bad\xff', 404),)),
    Case('http-error-short', (response(b'bad', 500, length=8),)),
    Case('success-short', (response(b'part', length=9),)),
    Case('no-content', (response(b'', 204),)),
    Case('chunked', (Response(b'HTTP/1.1 200 Owned\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n0\r\n\r\n'),)),
    Case('chunked-short', (Response(b'HTTP/1.1 200 Owned\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n4\r\nxy'),)),
    Case('chunked-missing-crlf', (Response(b'HTTP/1.1 200 Owned\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc'),)),
    Case('redirect-missing', (response(b'no location', 302),)),
    Case('redirect-cycle', (response(b'cycle', 302, (('Location', '/cycle'),)),)),
    Case('redirect-short', (response(b'part', 302, (('Location', '/next'),), 9),)),
    Case('headers-timeout', (Response(response().wire, hold=True),), timeout=True),
    Case('socket-progress', (Response(response(b'', length=5).wire, parts=(b'1', b'2', b'3', b'4', b'5'), interval=1),)),
    Case('unverified-tls', (response(),), tls=True),
  ]
  result.extend(Case(f'redirect-{status}', (response(b'moved', status, (('Location', '/next'),)), response(b'arrived'))) for status in (301, 302, 303, 307, 308))
  result.append(Case('redirect-uri', (response(b'moved', 302, (('URI', '/next'),)), response(b'arrived'))))
  return tuple(result)
