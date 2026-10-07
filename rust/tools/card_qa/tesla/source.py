from __future__ import annotations

import contextlib
import io
import logging
from can_source import load


class Settings:
    def __init__(self,values:dict[str,str])->None:
        self.values=dict(values);self.writes=[]
    def get_bool(self,key:str)->bool:
        return self.values.get(key)=="1"
    def put_int(self,key:str,value:int)->None:
        self.values[key]=str(value);self.writes.append([key,str(value)])
    def put_nonblocking(self,key:str,value:str)->None:
        self.values[key]=value;self.writes.append([key,value])


class LogCapture(logging.Handler):
    def __init__(self)->None:
        super().__init__();self.rows=[]
    def emit(self,record:logging.LogRecord)->None:
        self.rows.append([record.levelname,record.getMessage()])


def trace(case):
    load()
    from opendbc.car import interfaces,structs
    from opendbc.car.tesla.interface import CarInterface
    from opendbc.can import parser
    from opendbc.car.carlog import carlog
    settings=Settings(case['settings']);interfaces.Params=lambda:settings
    now=case['now'];parser.time=type('Clock',(),{'monotonic_ns':staticmethod(lambda:now)})
    firmware=[structs.CarParams.CarFw.new_message(ecu=fw['ecu'],fwVersion=bytes(fw['fw_version'])) for fw in case['firmware']]
    fingerprints={i:{} for i in range(8)}
    fingerprints.update({bus:dict(rows) for bus,rows in case['fingerprints']})
    try:cp=CarInterface.get_params(case['candidate'],fingerprints,firmware,case['alpha_long'],True,False)
    except KeyError:
        return dict(error='missing_torque')
    if case['op']=='params':return dict(params=list(cp.to_bytes()),writes=settings.writes)
    cp.carFw=firmware
    capture=LogCapture();carlog.addHandler(capture);printed=io.StringIO()
    with contextlib.redirect_stdout(printed),contextlib.redirect_stderr(printed):vehicle=CarInterface(cp)
    calls=[]
    def recv(*args):calls.append(['recv']);return []
    def send(*args):calls.append(['send']);return None
    vehicle.init(cp,recv,send)
    result=[]
    for step in case['steps']:
        now=step['now'];packets=[(p['mono_time'],[(f['address'],bytes(f['data']),f['bus']) for f in p['frames']]) for p in step['packets']]
        capture.rows.clear()
        with contextlib.redirect_stdout(printed),contextlib.redirect_stderr(printed):state=vehicle.update(packets)
        if step.get('soft_hold') is not None:vehicle.CS.softHoldActive=step['soft_hold']
        for key,value in step.get('commit',{}).items():setattr(state,key,value)
        vehicle.CS.out=state
        with structs.CarControl.from_bytes(bytes(step['control'])) as cc:
            actuators,can=vehicle.apply(cc,now)
        extra={key:value for key,value in vars(vehicle.CS).items() if key.startswith(('tesla_','_tesla_')) or key in ('summon','summon_prev','cruise_enabled_prev','fsd14_error_logged','suspected_fsd14','suspected_fsd14_clear_frames','hands_on_level','gas_pressed','steering_disengage','acc_cancel_last','das_accCancel','das_acc_state_last','das_acc_cancel_frames','cruise_override','coop_steering','infotainment_3_finger_press')}
        if extra['tesla_speed_button_template'] is not None:extra['tesla_speed_button_template']=list(extra['tesla_speed_button_template'])
        coop={key:value._last if key.startswith('resume_rate_limiter') else value for key,value in vars(vehicle.CC.coop_steer).items()}
        controller=dict(frame=vehicle.CC.frame,apply_angle_last=vehicle.CC.apply_angle_last,l_jerk=vehicle.CC.tesla_can.l_jerk,coop=coop,speed_limit=vars(vehicle.CC.speed_limit_controller).copy())
        sig=controller['speed_limit']['feedback_blocked_signature']
        if sig is not None:controller['speed_limit']['feedback_blocked_signature']=list(sig)
        result.append(dict(state=list(state.to_bytes()),actuators=list(actuators.to_bytes()),can=[dict(address=a,data=list(d),bus=b) for a,d,b in can],extra=extra,controller=controller,logs=capture.rows[:],soft_hold=vehicle.CS.softHoldActive))
    vehicle.deinit(cp,recv,send);carlog.removeHandler(capture)
    return dict(params=list(cp.to_bytes()),writes=settings.writes,steps=result,lifecycle=calls,prints=printed.getvalue().splitlines())
