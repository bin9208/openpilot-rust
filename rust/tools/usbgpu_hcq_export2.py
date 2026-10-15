"""Persistent-buffer HCQ descriptor ABI. Artifact records remain inert data."""
from usbgpu_hcq_data import ArtifactError, PersistentBuffer, state
from usbgpu_hcq_dispatch import dtype, sources
from usbgpu_hcq_dispatch2 import count, dispatcher


class PersistentExporter:
  def __init__(self, legacy):
    self.legacy = legacy
    self.artifact = legacy.artifact
    self.legacy_width = legacy.width
    self.buffers, self.ids = [], {}
    self.blob_ids = {}

  def width(self, node):
    if self.artifact.op(node) in {'ALLOC','BUFFER','PARAM'}:
      return (dtype(state(node.arguments[2])['dtype'])[1]+7)//8
    return self.legacy_width(node)

  def view(self, node):
    op, src = self.artifact.op(node), sources(node)
    if op in {'AFTER','BITCAST'}:
      return self.view(src[0])
    if op == 'SHRINK':
      view = self.view(src[0])
      view['offset'] += self.legacy.constant(src[1])*self.width(src[0])
      return view
    if op not in {'ALLOC','BUFFER'}:
      raise ArtifactError(f'unsupported persistent static buffer view: {op}')
    if id(node) not in self.ids:
      info = state(node.arguments[2])
      elements = count(node)
      size = elements*self.width(node)
      initial = info['buffer']
      if isinstance(initial,PersistentBuffer):
        key = (initial.offset,initial.size)
        if size != initial.size:
          raise ArtifactError('persistent allocation byte extent mismatch')
        if key in self.blob_ids:
          self.ids[id(node)] = self.blob_ids[key]
          return {'buffer':self.ids[id(node)],'offset':0}
        self.blob_ids[key] = len(self.buffers)
      spec = {} if info['spec'] is None else state(info['spec'])
      if spec.get('external_ptr') is not None:
        raise ArtifactError('external persistent buffer pointer')
      options = {k:spec.get(k,False) for k in ('host','cpu_access','uncached')}
      if not all(type(v) is bool for v in options.values()):
        raise ArtifactError('persistent buffer flags are not boolean')
      # Pinned AMDAllocator enables CPU access when USB has no copy queue.
      if info['device'] == 'AMD': options['cpu_access'] = True
      self.ids[id(node)] = len(self.buffers)
      if initial is not None:
        if info['device'] != 'AMD':
          raise ArtifactError('persistent weight buffer is not AMD')
        buffer = {'kind':'allocation','bytes':size,'initial':self.artifact.blob(initial),**options}
      else:
        tag = node.arguments[3]
        tag = {'amd_usb_host':'usb_host','amd_ring_compute_0':'ring_compute_0',
               'amd_write_ptr_compute_0':'write_ptr_compute_0',
               'amd_doorbell_compute_0':'doorbell_compute_0',
               'amd_put_value_compute_0':'put_value_compute_0','amd_scratch':'scratch'}.get(tag,tag)
        if not isinstance(tag,str):
          raise ArtifactError('persistent allocation lacks owned device tag')
        buffer = {'kind':'placeholder','tag':tag,'bytes':size,'elements':elements,
                  'device':info['device'],**options}
      self.buffers.append(buffer)
    return {'buffer':self.ids[id(node)],'offset':0}

  def expression(self,node):
    op,src = self.artifact.op(node),sources(node)
    if op == 'GETADDR':
      target = src[0]
      device = node.arguments[2]
      if device not in ('CPU',('AMD',),'AMD'):
        raise ArtifactError('persistent link address space')
      space = 'host' if device == 'CPU' else 'device'
      offset = 0
      while self.artifact.op(target) in {'BITCAST','AFTER','SHRINK'}:
        if self.artifact.op(target) == 'SHRINK':
          offset += self.legacy.constant(sources(target)[1])*self.width(sources(target)[0])
        target = sources(target)[0]
      if self.artifact.op(target) == 'PARAM':
        return {'kind':'binding','slot':state(target.arguments[2])['slot'],'offset':offset,'space':space}
      view = self.view(target)
      view['offset'] += offset
      return {'kind':'address','view':view,'space':space}
    if op == 'CONST':
      return {'kind':'constant','value':self.legacy.constant(node)}
    if op in {'ADD','SHR'}:
      return {'kind':op.lower(),'left':self.expression(src[0]),'right':self.expression(src[1])}
    if op in {'CAST','BITCAST','AFTER'}:
      return self.expression(src[0])
    raise ArtifactError(f'unsupported persistent link value: {op}')

  def export(self):
    # Reuse the established shape/alias checks and patch traversal, replacing
    # only the serialized ABI methods. No artifact callable is executed.
    original = self.legacy
    original.view, original.width, original.expression = self.view,self.width,self.expression
    original.buffers, original.ids = self.buffers,self.ids
    return self
