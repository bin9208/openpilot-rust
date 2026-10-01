import gc
import json
import os
import sys
from types import ModuleType, SimpleNamespace

log = ModuleType('openpilot.common.swaglog')
log.cloudlog = SimpleNamespace(error=lambda *args: None, warning=lambda *args: None, info=lambda *args: None, exception=lambda *args: None)
sys.modules[log.__name__] = log
from openpilot.system.ui.lib import egl

before = len(os.listdir('/proc/self/fd'))
initialized = egl.init_egl()
created = rejected = 0
if initialized:
  fd = os.open('/dev/null', os.O_RDONLY)
  try:
    for _ in range(3):
      image = egl.create_egl_image(1928, 1208, 2048, fd, 2473984)
      if image:
        egl.bind_egl_image_to_texture(17, image)
        created += 1
        egl.destroy_egl_image(image)
      else:
        rejected += 1
  finally:
    os.close(fd)
gc.collect()
print(json.dumps({'initialized': initialized, 'created': created, 'rejected': rejected, 'fd_delta': len(os.listdir('/proc/self/fd')) - before}))
