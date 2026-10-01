#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys

assert sys.argv[1:] == ['sysctl', 'net.ipv4.ip_forward=0'], sys.argv
Path(os.environ['WIFI_COMMAND_RECORD']).write_text(json.dumps(sys.argv[1:]))
