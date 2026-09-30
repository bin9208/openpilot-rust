#!/usr/bin/env python3
"""Run unchanged original timed.main on native msgq with fixture clocks and commands."""
import json
import sys
import time

import openpilot.cereal.messaging as messaging
from logging_producer_reference import original_socket_handler
from timed_reference import Source
from pathlib import Path
import os

config = json.loads(sys.stdin.readline())
source = Source(config, Path(config['params_root']) / os.environ['OPENPILOT_PREFIX'])
source.scope['time'].monotonic = time.monotonic
source.scope['time'].sleep = time.sleep
source.scope['messaging'] = messaging
handler = original_socket_handler('ipc:///tmp/logmessage' + os.environ['OPENPILOT_PREFIX'], source.logger)
source.logger.addHandler(handler)
source.scope['main']()
