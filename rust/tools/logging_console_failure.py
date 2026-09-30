import json
import io
from contextlib import redirect_stderr
from pathlib import Path

from logging_producer_native import receiver, receive
from logging_producer_reference import console_handler, original_socket_handler, probe, source


def check(binary: Path, output: Path) -> dict:
  output.mkdir(parents=True)
  with receiver() as (endpoint, pull), io.TextIOWrapper(Path('/dev/full').open('wb', buffering=0), write_through=True) as full:
    logger, _ = source()
    console, _ = console_handler('warning')
    console.setStream(full)
    logger.addHandler(console)
    transport = original_socket_handler(endpoint, logger)
    logger.addHandler(transport)
    try:
      with redirect_stderr(full):
        logger.warning('console I/O failure')
      raw = pull.recv()
      assert raw[0] == 30 and json.loads(raw[1:])['msg'] == 'console I/O failure'
      (output / 'original.packet').write_bytes(raw)
    finally:
      logger.removeHandler(console)
      logger.removeHandler(transport)
      transport.close()
  with receiver() as (endpoint, pull), probe(binary, output / 'rust', endpoint, stderr_path=Path('/dev/full')) as client:
    response = client.command({'action': 'emit', 'level': 30, 'text': 'console I/O failure', 'exception': None})
    packet = receive(pull, client, 'console-failure')
    assert packet['msg'] == 'console I/O failure'
    assert response == {'delivery': 'sent'}, response
    client.finish()
  report = {'result': 'pass', 'original': 'console OSError suppressed, IPC delivered', 'rust': 'console OSError suppressed, IPC delivered'}
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  return report
