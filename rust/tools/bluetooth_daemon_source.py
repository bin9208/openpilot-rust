from pathlib import Path
import sys

from pytest import MonkeyPatch
from openpilot.selfdrive.carrot.bluetooth import daemon, model


def main() -> None:
  runtime, settings, sysfs = (Path(value) for value in sys.argv[1:])

  def redirected_path(path: str) -> Path:
    return sysfs if path == '/sys/class/input' else Path(path)

  with MonkeyPatch.context() as patch:
    patch.setattr(daemon, 'RUNTIME', runtime)
    patch.setattr(daemon, 'Path', redirected_path)
    patch.setattr(daemon, 'config', lambda: model.config(settings))
    patch.setattr(daemon, 'CommandWriter', lambda: model.CommandWriter(runtime))
    daemon.main()


if __name__ == '__main__':
  main()
