# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Original youtube-test command/runner with actual bindings and caller-owned paths."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import resource
import sys

# The original detached runner uses -m without -P. Remove only its prepended
# repository CWD; retain the caller's ordered binding/dependency PYTHONPATH.
if sys.path and Path(sys.path[0]).resolve() == Path(__file__).resolve().parents[2]:
  sys.path.pop(0)

from carrot_server_dashcam_catalog import source_modules
from carrot_server_dashcam_upload import save
from original_params_binding import load


def configure(path: Path):
  config = json.loads(path.read_text())
  root = Path(config['owned_root'])
  assert Path(os.environ['PARAMS_ROOT']).is_relative_to(root)
  assert os.environ['OPENPILOT_PREFIX'] == config['prefix']
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  source_modules()
  load(os.environ['ORIGINAL_PARAMS_BINDING'], f'ipc://{root}/owned-log', root / 'logs')
  import msgq

  msgq.__path__.insert(0, config['vision_root'])
  from openpilot.selfdrive.carrot.server.services import youtube_test as source

  source.REPO_ROOT = Path(config['repository'])
  source.STATE_PATH = root / 'test-state.json'
  source.LOG_PATH = root / 'test.log'
  source.REPORT_PATH = root / 'test-report.json'
  source.CARROT_YOUTUBE_LIVE_STATE_PATH = str(root / 'state/youtube_live.json')
  source.CARROT_YOUTUBE_LIVE_SECRET_PATH = str(root / 'state/youtube_live_secret.json')
  source.YOUTUBE_STATUS_URL = config['status_url']
  source.RUNNER_MODULE = 'carrot_server_youtube_test_source'
  source._CAMERAD_SPEC = {'cmd': [config['camera']], 'cwd': config['repository'], 'match': config['camera']}
  source._ENCODER_PATH = config['encoder']
  source._QUALITY_SPECS = {quality: profile.encoder_spec(config['encoder'], cwd=config['repository']) for quality, profile in source.YOUTUBE_PROFILES.items()}
  source._CONFLICT_MATCHES = {
    'camerad': config['camera'],
    'carrot_vision_encoderd': config['encoder'] + '\0--carrot-vision-road',
    **{str(spec['name']): str(spec['match']) for spec in source._QUALITY_SPECS.values()},
  }
  save(
    root / 'cli-source-proof.json',
    {
      'path': source.__file__,
      'sha256': hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
      'params_path': source._params().get_param_path(),
      'original_body_unchanged': True,
      'vision_root': config['vision_root'],
    },
  )
  return source


def main() -> None:
  args = sys.argv[1:]
  if args[:1] == ['--config']:
    path = Path(args[1]).resolve()
    os.environ['OWNED_YOUTUBE_TEST_CONFIG'] = str(path)
    args = args[2:]
  else:
    path = Path(os.environ['OWNED_YOUTUBE_TEST_CONFIG'])
  source = configure(path)
  if args[:1] == ['_run']:
    code = source.main(args)
  else:
    code = source.run_command(args)
  raise SystemExit(code)


if __name__ == '__main__':
  main()
