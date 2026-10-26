"""Lifecycle 원본 비교용 세 정의만 유지한다. IPC orchestration은 native controller 소유다."""

import time

from openpilot.cereal import log

OUTPUTS = ['longitudinalPlan', 'lateralPlan', 'driverAssistance']


def policy(raw):
  with log.Event.from_bytes(raw) as event:
    service = event.which()
    value = getattr(event, service).to_dict()
    valid = bool(event.valid)
  for key in ('processingDelay', 'solverExecutionTime', 'plannerExecutionTime', 'fastRadarExecutionTime'):
    value.pop(key, None)
  return service, valid, value


def peers_ready(publishers, child):
  deadline = time.monotonic() + 15.0
  while time.monotonic() < deadline:
    if child.poll() is not None:
      raise RuntimeError(f'daemon exited before subscriptions: {child.returncode}')
    if all(socket.all_readers_updated() for socket in publishers.sock.values()):
      return
    time.sleep(0.01)
  raise TimeoutError('original CPP input peers did not observe every planner subscription')


if __name__ == '__main__':
  raise SystemExit('plannerd_ipc orchestration was replaced; use check-plannerd-ci ipc')
