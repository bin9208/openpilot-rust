"""Import unchanged AGNOS/casync source and replace only owned external boundaries."""

import ast
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

import requests


class Observer:
  def __init__(self):
    self.events = []

  def info(self, text):
    self.events.append(['log', 'info', text])

  def warning(self, text):
    self.events.append(['log', 'warning', text])

  def error(self, text):
    self.events.append(['log', 'error', text])

  def exception(self, text):
    self.events.append(['log', 'exception', text])

  def progress(self, stage, progress):
    self.events.append(['progress', stage, max(0, min(progress, 100))])

  def sleep(self, seconds):
    self.events.append(['sleep', seconds])


def load(config, observer=None):
  source = Path(__file__).resolve().parents[2] / 'openpilot/system/hardware/tici/agnos.py'
  spec = importlib.util.spec_from_file_location('source_agnos', source)
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  paths = config['paths']
  module.DOWNLOAD_CACHE_DIR = Path(paths['cache'])
  module.UPDATE_CONFIRMATION_FILE = Path(paths['confirmation'])
  module.UPDATE_LOCK_FILE = Path(paths['lock'])
  module.CAIBX_URL = paths['caibx_url']
  original_path = module.get_partition_path
  module.get_partition_path = lambda slot, partition: str(Path(paths['partitions']) / Path(original_path(slot, partition)).name)

  class ProcessBoundary:
    STDOUT = subprocess.STDOUT
    CalledProcessError = subprocess.CalledProcessError

    @staticmethod
    def check_output(argv, **kwargs):
      assert argv[0] == 'abctl', argv
      return subprocess.check_output([config['abctl'], *argv[1:]], **kwargs)

  class OsBoundary:
    @staticmethod
    def system(command):
      argv = command.split()
      assert argv[:2] == ['abctl', '--set_unbootable'], command
      return subprocess.call([config['abctl'], *argv[1:]])

    def __getattr__(self, name):
      return getattr(os, name)

  module.subprocess = ProcessBoundary
  module.os = OsBoundary()
  if observer:
    module.report_progress = observer.progress

    class Clock:
      sleep = observer.sleep

    module.time = Clock
    import openpilot.system.updated.casync.casync as casync

    casync.time = Clock
  return module, source


def execute(module, request, operation, observer):
  partition = operation.get('partition')
  manifest = request['manifest']
  match operation['op']:
    case 'helpers':
      before = module.update_confirmed(manifest)
      urls = module.manifest_download_urls(manifest)
      module.mark_update_confirmed(manifest)
      after = module.update_confirmed(manifest)
      content = module.UPDATE_CONFIRMATION_FILE.read_text()
      module.clear_update_confirmation()
      return {'before': before, 'after': after, 'content': content, 'urls': urls, 'cleared': not module.UPDATE_CONFIRMATION_FILE.exists()}
    case 'verify':
      return module.verify_partition(1, partition, operation['force'])
    case 'flash':
      return module.flash_agnos_update(manifest, 1, observer, standalone=operation['standalone'], retry_network=operation['retry_network'])
    case 'cache':
      result = module.download_to_cache(partition, observer)
      return result.name if result else None
    case 'compressed':
      return module.extract_compressed_image(1, partition, observer)
    case 'casync':
      return module.extract_casync_image(1, partition, observer)
    case 'clear':
      return module.clear_partition_hash(1, partition)
    case 'swap':
      return module.swap(manifest, 1, observer)
    case 'target':
      return module.get_target_slot_number()
    case 'decompress':
      reader = module.StreamingDecompressor('unused', Path(operation['path']))
      try:
        rows = []
        for length in operation['reads']:
          data = reader.read(length)
          rows.append({'length': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
        return {'reads': rows, 'sha256': reader.sha256.hexdigest()}
      finally:
        reader.close()
    case 'execute':
      raise ValueError('Use the actual source CLI path for CLI verification')
    case _:
      raise ValueError(operation)


def main():
  if sys.argv[1:2] == ['--cli']:
    config = json.loads(Path(sys.argv[2]).read_text())
    module, source = load(config)
    sys.argv = [str(source), *sys.argv[3:]]
    body = ast.parse(source.read_text()).body[-1]
    assert isinstance(body, ast.If)
    exec(compile(ast.Module(body=body.body, type_ignores=[]), str(source), 'exec'), module.__dict__)
    return
  request = json.load(sys.stdin)
  observer = Observer()
  module, _ = load(request['config'], observer)
  rows = []
  for operation in request['operations']:
    try:
      rows.append({'result': execute(module, request, operation, observer)})
    except requests.RequestException as error:
      rows.append({'request': type(error).__name__, 'transient': module.transient_download_error(error)})
    except Exception as error:
      rows.append({'error': str(error)})
  json.dump({'rows': rows, 'events': observer.events}, sys.stdout)


if __name__ == '__main__':
  main()
