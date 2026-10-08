from __future__ import annotations

import struct
from pytest import MonkeyPatch

from radarcan_source import normalized


def trace(case):
  import numpy as np
  from opendbc.car.ford.radar_interface import cluster_points
  captured = {}
  dot, maximum = np.dot, np.maximum

  def capture_dot(*args, **kwargs):
    result = dot(*args, **kwargs)
    captured['dot'] = result.copy().tolist()
    captured['dot_bits'] = [struct.pack('<d', value).hex() for value in result.flat]
    return result

  def capture_maximum(*args, **kwargs):
    result = maximum(*args, **kwargs)
    captured['distance'] = result.copy().tolist()
    captured['distance_bits'] = [struct.pack('<d', value).hex() for value in result.flat]
    return result

  with MonkeyPatch.context() as fixture:
    fixture.setattr(np, 'dot', capture_dot)
    fixture.setattr(np, 'maximum', capture_maximum)
    labels = cluster_points(case['previous'], case['current'], case['max_distance'])
  return normalized({'labels': labels, **captured})


def cases():
  scenarios = [
    ('empty-both', [], []),
    ('empty-current', [[1.,2.,3.]], []),
    ('empty-previous', [], [[1.,2.,3.],[4.,5.,6.]]),
    ('dot-one-one', [[20.2,1.1,-.5]], [[20.1,1.2,-.4]]),
    ('dot-many-one', [[20.2,1.1,-.5],[30.3,2.2,-1.0]], [[20.1,1.2,-.4]]),
    ('dot-one-many', [[20.2,1.1,-.5]], [[20.1,1.2,-.4],[30.3,2.2,-1.0]]),
    ('dot-many-many', [[20.2,1.1,-.5],[30.3,2.2,-1.0]], [[20.1,1.2,-.4],[30.4,2.1,-1.1]]),
    ('strict-five-meter-boundary', [[0.,0.,0.]], [[5.,0.,0.],[4.999999999999999,0.,0.],[5.000000000000001,0.,0.]]),
    ('first-tie-and-coordinate-weight', [[20.,2.,0.],[20.,-2.,0.]], [[20.,0.,0.],[20.,4.,0.],[20.,-4.,0.]]),
    ('large-coordinate-cancellation', [[1e8,1e8,1e8],[1e8+10,1e8-10,1e8]], [[1e8+1,1e8-1,1e8],[1e8+10,1e8-9,1e8]]),
    ('all-64-points', [[10. + i*.7, i*.13, -i*.1] for i in range(64)],
                      [[10. + i*.71, i*.131, -i*.11] for i in range(64)]),
  ]
  return [{'name': 'cluster-' + name, 'op': 'cluster', 'previous': previous, 'current': current, 'max_distance': 5.}
          for name, previous, current in scenarios]
