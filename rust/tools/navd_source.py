from __future__ import annotations

import ast
from collections import deque
import contextlib
import copy
import io
import json
import math
from pathlib import Path
from types import SimpleNamespace

import requests
import numpy as np

ROOT = Path(__file__).resolve().parents[2]


class Fixture:
  def __init__(self, parameters: dict[str, str]):
    from openpilot.cereal import log
    from openpilot.selfdrive.navd import helpers
    self.parameters = dict(parameters)
    self.responses = deque()
    self.effects = []
    self.diagnostics = []
    self.sm = Subscriptions()
    self.log = log
    self.helpers = helpers
    tree = ast.parse((ROOT / 'openpilot/selfdrive/navd/navd.py').read_text())
    names = {'REROUTE_DISTANCE', 'MANEUVER_TRANSITION_THRESHOLD', 'REROUTE_COUNTER_MIN', 'NAV_ROUTE_MAX_POINTS'}
    nodes = [node for node in tree.body if (isinstance(node, (ast.ClassDef, ast.FunctionDef)) and
             node.name in ('RouteEngine', 'limit_route_points')) or (isinstance(node, ast.Assign) and
             any(isinstance(target, ast.Name) and target.id in names for target in node.targets))]
    namespace = {'math': math, 'np': np, 'os': SimpleNamespace(environ={'MAPBOX_TOKEN': 'fixture-token'}),
                 'json': json, 'log': log, 'Params': lambda: Parameters(self),
                 'requests': SimpleNamespace(get=self.request, exceptions=requests.exceptions),
                 'threading': SimpleNamespace(Timer=self.timer),
                 'messaging': SimpleNamespace(new_message=self.new_message),
                 'cloudlog': SimpleNamespace(warning=lambda *args, **kw: self.diagnostic('warning', args, kw),
                                            exception=lambda *args, **kw: self.diagnostic('exception', args, kw),
                                            event=lambda *args, **kw: self.diagnostic('event', args, kw))}
    namespace.update({name: getattr(helpers, name) for name in ('Coordinate', 'coordinate_from_param', 'distance_along_geometry',
                     'maxspeed_to_ms', 'minimum_distance', 'parse_banner_instructions')})
    exec(compile(ast.Module(body=nodes, type_ignores=[]), 'original-navd-methods', 'exec'), namespace)
    self.engine = namespace['RouteEngine'](self.sm, self)

  def diagnostic(self, level, arguments, fields):
    self.diagnostics.append({'level': level, 'arguments': arguments, 'fields': fields})

  def request(self, url, params, timeout):
    prepared = requests.Request('GET', url, params=params).prepare()
    self.effects.append({'request': prepared.url, 'timeout': timeout})
    item = self.responses.popleft()
    if item.get('network_error'):
      raise requests.exceptions.ConnectionError('owned fixture')
    response = requests.Response()
    response.status_code = item.get('status', 200)
    response.url = prepared.url
    response._content = (item['raw'] if 'raw' in item else json.dumps(item['body'])).encode()
    return response

  def timer(self, seconds, callback):
    return SimpleNamespace(start=lambda: self.effects.append({'timer': seconds}))

  def new_message(self, service, valid):
    message = self.log.Event.new_message()
    message.valid = valid
    message.init(service)
    return message

  def send(self, service, message):
    self.effects.append({'service': service, 'valid': message.valid, 'data': getattr(message, service).to_dict()})

  def snapshot(self):
    value = self.engine
    def point(point):
      return point.as_dict() if point is not None else None
    geometry = value.route_geometry
    if geometry is not None:
      geometry = [[dict(point.as_dict(), annotations=dict(point.annotations)) for point in segment] for segment in geometry]
    return {'last_position': point(value.last_position), 'last_bearing': value.last_bearing,
            'gps_ok': value.gps_ok, 'localizer_valid': value.localizer_valid,
            'nav_destination': point(value.nav_destination), 'step_idx': value.step_idx,
            'route': value.route, 'route_geometry': geometry,
            'recompute_backoff': value.recompute_backoff, 'recompute_countdown': value.recompute_countdown,
            'ui_pid': value.ui_pid, 'reroute_counter': value.reroute_counter,
            'carrot_route_active': value.carrot_route_active}

  def execute(self, row):
    self.effects.clear()
    self.diagnostics.clear()
    error = None
    result = None
    output = io.StringIO()
    with contextlib.redirect_stdout(output):
      try:
        match row['op']:
          case 'parameter':
            if row['value'] is None:
              self.parameters.pop(row['key'], None)
            else:
              self.parameters[row['key']] = row['value']
          case 'response':
            self.responses.append(copy.deepcopy(row['value']))
          case 'update':
            self.sm.values['carrotMan'] = SimpleNamespace(**row['position'])
            self.sm.updated['managerState'] = 'manager' in row
            if 'manager' in row:
              self.sm.values['managerState'] = SimpleNamespace(processes=[SimpleNamespace(**value) for value in row['manager']])
            self.engine.update()
          case 'should_recompute':
            result = self.engine.should_recompute()
          case 'instruction':
            self.engine.send_instruction()
          case 'send_route':
            self.engine.send_route()
          case 'clear':
            self.engine.clear_route()
          case 'reset':
            self.engine.reset_recompute_limits()
          case 'calculate':
            self.engine.calculate_route(self.helpers.Coordinate(**row['destination']))
          case _:
            raise AssertionError(row['op'])
      except (KeyError, IndexError, TypeError, ValueError, ZeroDivisionError, RuntimeError) as exception:
        error = type(exception).__name__
    return copy.deepcopy({'state': self.snapshot(), 'parameters': self.parameters, 'effects': self.effects,
                          'result': result, 'error': error, 'stdout': output.getvalue(), 'diagnostics': self.diagnostics})


class Parameters:
  def __init__(self, fixture):
    self.fixture = fixture

  def get(self, key):
    return self.fixture.parameters.get(key)

  def remove(self, key):
    self.fixture.effects.append({'remove': key})
    self.fixture.parameters.pop(key, None)


class Subscriptions:
  def __init__(self):
    self.values = {'carrotMan': SimpleNamespace(xPosLat=0., xPosLon=0., xPosAngle=0.),
                   'managerState': SimpleNamespace(processes=[])}
    self.updated = {'managerState': False}

  def update(self, timeout):
    pass

  def __getitem__(self, service):
    return self.values[service]
