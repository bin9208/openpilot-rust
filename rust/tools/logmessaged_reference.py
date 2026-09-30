"""Original source adapters and byte-preserving UUID normalization for log QA."""
from __future__ import annotations

import ast
import json
from logging.handlers import BaseRotatingHandler
import os
from pathlib import Path
import time
from types import SimpleNamespace
import uuid

from openpilot.common.logging_extra import SwagLogFileFormatter

ROOT = Path(__file__).resolve().parents[2]


def rotating_class(clock=time.monotonic):
    path = ROOT / 'openpilot/common/swaglog.py'
    tree = ast.parse(path.read_text())
    tree.body = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'SwaglogRotatingFileHandler']
    namespace = {'BaseRotatingHandler': BaseRotatingHandler, 'os': os, 'time': SimpleNamespace(monotonic=clock)}
    exec(compile(tree, str(path), 'exec'), namespace)
    return namespace['SwaglogRotatingFileHandler']


def normalize_uuid(record: str, validate=True) -> str:
    """Replace only the top-level generated ID token, keeping every other byte."""
    decoder = json.JSONDecoder()
    try:
        decoder.raw_decode(record)
    except json.JSONDecodeError:
        return record
    position = 1
    while position < len(record):
        while record[position] in ' \t\r\n,':
            position += 1
        if record[position] == '}':
            return record
        key, position = decoder.raw_decode(record, position)
        while record[position] in ' \t\r\n:':
            position += 1
        start = position
        value, position = decoder.raw_decode(record, position)
        if key == 'id':
            if validate:
                parsed = uuid.UUID(value)
                assert parsed.version == 4 and parsed.variant == uuid.RFC_4122
            return record[:start] + '"' + '0' * 32 + '"' + record[position:]
    return record


def file_snapshot(directory: Path):
    result = {}
    for path in sorted(directory.iterdir()):
        if path.is_symlink():
            result[os.fsencode(path.name).hex()] = {'symlink': os.readlink(path)}
        elif path.is_file():
            result[os.fsencode(path.name).hex()] = {'content': ''.join(normalize_uuid(line) for line in path.read_text().splitlines(keepends=True))}
        elif path.is_dir():
            result[os.fsencode(path.name).hex()] = {'directory': True}
    return result


def run_original(endpoint: str, root: Path) -> None:
    """Execute the unchanged original daemon main with only Paths adapted."""
    import zmq
    import openpilot.cereal.messaging as messaging
    paths = SimpleNamespace(swaglog_ipc=lambda: endpoint, swaglog_root=lambda: str(root))
    handler_path = ROOT / 'openpilot/common/swaglog.py'
    handler_tree = ast.parse(handler_path.read_text())
    handler_tree.body = [node for node in handler_tree.body if isinstance(node, ast.FunctionDef) and node.name == 'get_file_handler']
    handler_namespace = {'Path': Path, 'Paths': paths, 'os': os, 'SwaglogRotatingFileHandler': rotating_class()}
    exec(compile(handler_tree, str(handler_path), 'exec'), handler_namespace)
    handler = handler_namespace['get_file_handler']
    path = ROOT / 'openpilot/system/logmessaged.py'
    tree = ast.parse(path.read_text())
    tree.body = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main']
    namespace = {'zmq': zmq, 'messaging': messaging, 'Paths': paths, 'get_file_handler': handler,
                 'SwagLogFileFormatter': SwagLogFileFormatter, 'NoReturn': None}
    exec(compile(tree, str(path), 'exec'), namespace)
    namespace['main']()


if __name__ == '__main__':
    import sys
    run_original(sys.argv[1], Path(sys.argv[2]))
