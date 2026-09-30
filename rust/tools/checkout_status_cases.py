import math
import sys

from checkout_peer import Peer

A, B, C = (letter * 40 for letter in 'abc')


def packaged_status(peer: Peer):
  peer.metadata(A)
  assert peer.op('capture')['running_commit'] == A
  rows = []
  for now, commit, expected in [(0.0, B, False), (4.999, B, False), (5.0, B, True),
                                 (10.0, A, False), (15.0, B, False), (20.0, B, True)]:
    peer.metadata(commit)
    response = peer.op('update', now)
    assert response['returned'] == expected and response['running_commit'] == A
    rows.append({'now': now, 'required': response['returned'], 'running': response['running_commit']})
  return rows


def fixture_status(peer: Peer):
  peer.repo.joinpath('.git').mkdir()
  results = []

  def installed(commit: str, exit_code: int = 0) -> None:
    peer.fixture(stdout=list(commit.encode()), exit_code=exit_code)

  def capture(commit: str, exit_code: int = 0) -> int:
    installed(commit, exit_code)
    before = len(peer.calls())
    response = peer.op('capture')
    assert response['running_commit'] == (commit if exit_code == 0 else None)
    assert len(peer.calls()) == before + 1
    return before

  def update(now: float, expected: bool, calls: int, running: str | None = A) -> None:
    response = peer.op('update', now)
    assert response['returned'] == expected and response['reboot_required'] == expected, response
    assert response['running_commit'] == running, response
    assert len(peer.calls()) == calls, (now, len(peer.calls()), calls)
    results.append({'now_hex': now.hex(), 'returned': response['returned'], 'running_commit': running, 'calls': calls})

  base = capture(A)
  update(-math.inf, False, base + 1)
  update(-sys.float_info.min, False, base + 1)
  update(-0.0, False, base + 2)
  installed(B)
  update(math.nextafter(5.0, -math.inf), False, base + 2)
  update(5.0, False, base + 3)
  installed(C)
  update(4.0, False, base + 3)
  update(10.0, False, base + 4)
  installed(B)
  update(15.0, False, base + 5)
  update(20.0, True, base + 6)
  installed(B, 7)
  update(25.0, False, base + 7)
  installed(B)
  update(30.0, False, base + 8)
  update(35.0, True, base + 9)
  installed(A)
  update(40.0, False, base + 10)
  installed(B)
  update(45.0, False, base + 11)
  update(50.0, True, base + 12)

  for interruption, exit_code in [(B, 7), (A, 0)]:
    base = capture(A)
    installed(B)
    update(0.0, False, base + 2)
    installed(interruption, exit_code)
    update(5.0, False, base + 3)
    installed(B)
    update(10.0, False, base + 4)
    update(15.0, True, base + 5)

  base = capture(A, 7)
  installed(B)
  update(0.0, False, base + 2, None)
  update(5.0, False, base + 3, None)
  installed(C)
  update(10.0, False, base + 4, None)
  update(15.0, False, base + 5, None)

  base = capture(A)
  installed(B)
  update(math.nan, False, base + 2)
  update(-math.inf, True, base + 3)
  installed(C)
  update(-math.inf, False, base + 4)
  update(0.0, True, base + 5)
  installed(B)
  update(math.inf, False, base + 6)
  update(sys.float_info.max, False, base + 6)
  update(math.inf, True, base + 7)
  installed(A)
  update(math.nan, False, base + 8)
  installed(B)
  update(math.inf, False, base + 9)
  update(-math.inf, False, base + 9)
  update(math.nan, True, base + 10)

  base = capture(A)
  installed(B)
  update(sys.float_info.max, False, base + 2)
  update(sys.float_info.max, True, base + 3)
  update(math.nextafter(sys.float_info.max, 0), True, base + 3)

  base = capture(A)
  installed(B)
  large = 1e16
  update(large, False, base + 2)
  update(math.nextafter(large + 5.0, -math.inf), False, base + 2)
  update(large + 5.0, True, base + 3)
  return results
