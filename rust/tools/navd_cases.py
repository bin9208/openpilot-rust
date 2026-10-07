import copy
import json


def position(longitude=127., latitude=37., angle=359.5):
  return {'xPosLat': latitude, 'xPosLon': longitude, 'xPosAngle': angle}


def route():
  steps = []
  points = [[127., 37.], [127.0014, 37.], [127.0023, 37.], [127.0023, 37.]]
  for index, distance in enumerate((120., 80., 0.)):
    steps.append({'distance': distance, 'duration': 8. - index,
                  'duration_typical': 10. - index if index != 1 else None,
                  'geometry': {'coordinates': points[index:index+2]},
                  'speedLimitSign': 'vienna' if index == 1 else 'mutcd',
                  'bannerInstructions': [] if index == 2 else [{'distanceAlongGeometry': 100.,
                    'primary': {'text': f'Turn {index}', 'type': 'turn', 'modifier': 'right'},
                    'sub': {'components': [{'type': 'lane', 'active': True, 'directions': ['right'], 'active_direction': 'right'}]}}]})
  return {'routes': [{'legs': [{'steps': steps, 'annotation': {'maxspeed': [
    {'unit': 'km/h', 'speed': 80}, {'unit': 'mph', 'speed': 30}, {'unknown': True}]}}]}]}


def parameters():
  return {'LastGPSPosition': json.dumps({'latitude': 37., 'longitude': 127.}),
          'NavDestination': json.dumps({'latitude': 37., 'longitude': 127.0023}),
          'LanguageSetting': 'main_ko'}


def response(body=None):
  return {'op': 'response', 'value': {'status': 200, 'body': route() if body is None else body}}


def update(longitude=127., latitude=37., manager=None):
  row = {'op': 'update', 'position': position(longitude, latitude)}
  if manager is not None:
    row['manager'] = [{'name': 'ui', 'pid': manager, 'running': True}]
  return row


def cases():
  output = [
    {'name': 'position-unavailable', 'parameters': {}, 'steps': [update(0., 0.), {'op': 'send_route'}, {'op': 'clear'}]},
    {'name': 'normal-transitions', 'parameters': parameters(), 'steps': [response(), update(manager=10),
      update(127.0003), update(127.0017), update(127.0024), update(127.003), update(0., 0.), {'op': 'clear'}]},
    {'name': 'ui-restart-route-timer', 'parameters': parameters(), 'steps': [response(), update(manager=10),
      update(manager=10), update(manager=11), {'op': 'send_route'}, update(manager=-1), update(manager=0), update(manager=12),
      {'op': 'parameter', 'key': 'NavDestination', 'value': None}, update()]},
    {'name': 'reroute-backoff', 'parameters': parameters(), 'steps': [response(), update(),
      {'op': 'response', 'value': {'status': 503, 'raw': 'unavailable'}},
      update(latitude=38.), update(latitude=38.), update(latitude=38.), update(latitude=38.),
      response({'routes': []}), update(latitude=38.), update(latitude=38.), update(latitude=38.),
      response(), update(latitude=38.), update(latitude=38.), update(latitude=38.), update(latitude=38.),
      {'op': 'reset'}]},
    {'name': 'external-destination', 'parameters': parameters(), 'steps': [response(), update(),
      {'op': 'parameter', 'key': 'NavDestination', 'value': json.dumps({'latitude': 38., 'longitude': 128., 'place_name': 'External Navi'})},
      update(latitude=38.), update(latitude=38.), update(latitude=38.), update(latitude=38.), update(latitude=38.),
      update(latitude=0., longitude=0.)]},
  ]
  waypoints = parameters()
  waypoints.update(NavDestination=json.dumps({'latitude': 37, 'longitude': 128}),
                   NavDestinationWaypoints=json.dumps([[127.25, 37], [127.5, 37.5]]), LanguageSetting='main_ko-main_en')
  output.append({'name': 'request-coordinates-language', 'parameters': waypoints, 'steps': [response(), update(), {'op': 'send_route'}]})
  for name, value in (('empty-route', {'status': 200, 'body': {'routes': []}}),
                      ('request-error', {'network_error': True}),
                      ('invalid-json-response', {'status': 200, 'raw': 'invalid'}),
                      ('status-error', {'status': 401, 'raw': 'denied'})):
    initial = parameters()
    initial['NavDestinationWaypoints'] = '[[127.1,37.0]]'
    output.append({'name': name, 'parameters': initial, 'steps': [{'op': 'response', 'value': value}, update(), {'op': 'send_route'}]})
  malformed = route()
  del malformed['routes'][0]['legs'][0]['annotation']
  output.append({'name': 'partial-response-state', 'parameters': parameters(), 'steps': [response(malformed), update(), {'op': 'send_route'}, {'op': 'clear'}]})
  invalid = parameters()
  invalid['NavDestinationWaypoints'] = '[invalid'
  output.append({'name': 'invalid-waypoints', 'parameters': invalid, 'steps': [update(), {'op': 'send_route'}, {'op': 'clear'}]})
  large = route()
  step = large['routes'][0]['legs'][0]['steps'][0]
  step['geometry']['coordinates'] = [[127. + i * .000001, 37.] for i in range(5003)]
  large['routes'][0]['legs'][0]['steps'] = [step]
  large['routes'][0]['legs'][0]['annotation']['maxspeed'] = []
  output.append({'name': 'route-point-cap', 'parameters': parameters(), 'steps': [response(large), update(), {'op': 'send_route'}]})
  output.append({'name': 'non-200-success', 'parameters': parameters(),
                 'steps': [{'op': 'response', 'value': {'status': 201, 'body': route()}}, update()]})
  cancellation = route()
  steps = cancellation['routes'][0]['legs'][0]['steps']
  steps.append(copy.deepcopy(steps[0]))
  for step, distance in zip(steps, (200., 1e16, 1., -1e16), strict=True):
    step['distance'] = distance
    step['bannerInstructions'] = copy.deepcopy(steps[0]['bannerInstructions'])
  output.append({'name': 'compensated-maneuver-sum', 'parameters': parameters(), 'steps': [response(cancellation), update()]})
  return copy.deepcopy(output)
