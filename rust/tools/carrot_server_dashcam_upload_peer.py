from __future__ import annotations

import hashlib
import threading
from urllib.parse import unquote

from check_dashcam_runtime import Receiver

class HeldReceiver(Receiver):
    def __init__(self):
        self.release=threading.Event()
        super().__init__('held-cancel')
        owner=self
        def put(handler):
            body=handler.body()
            owner.captures.append({'method':'PUT','path':unquote(handler.path),'size':len(body),'sha256':hashlib.sha256(body).hexdigest(),'auth':handler.headers.get('Authorization')})
            owner.started.set()
            if not owner.release.wait(10):
                raise TimeoutError('owned upload response was not released')
            handler.reply(200,{'ok':True,'size':len(body),'error':''})
        self.server.RequestHandlerClass.do_PUT=put
    def close(self):
        self.release.set()
        super().close()
