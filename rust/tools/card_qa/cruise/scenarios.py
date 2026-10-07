#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0"]
# ///
# How to run: imported by rust/tools/check_card_cruise.py in the verified oracle environment.
from __future__ import annotations

import random
from openpilot.cereal import messaging
from check_card_cruise import DEFAULTS, frame


def event(service: str, **values) -> list[int]:
  message=messaging.new_message(service);message.valid=True;getattr(message,service).from_dict(values)
  return list(message.to_bytes())


def action(button: str, **fields):
  return [frame(buttonEvents=[dict(type=button,pressed=True)],**fields),frame(buttonEvents=[dict(type=button,pressed=False)],**fields)]


def scenarios():
  cases=[]
  def add(name,steps,params=None,pcm=False,enabled=True,metric=True):
    for step in steps:
      step["metric"]=metric
      controls=False
      for payload in step["events"]:
        with messaging.log.Event.from_bytes(bytes(payload)) as received:
          controls |= received.which()=="carControl"
      if not controls: step["events"]=[event("carControl",enabled=enabled)]+step["events"]
    cases.append(dict(name=name,cp=dict(pcmCruise=pcm,openpilotLongitudinalControl=not pcm),params=DEFAULTS | (params or {}),frames=steps))
  for button in ("accelCruise","decelCruise","cancel","gapAdjustCruise","lfaButton","setCruise","resumeCruise","paddleLeft","paddleRight","mainCruise"):
    for metric in (True,False):
      steps=[frame(),frame(buttonEvents=[dict(type=button,pressed=True)],cruiseSpeedBigStep=True)]
      steps += [frame(cruiseSpeedBigStep=i==5) for i in range(90)]
      steps += [frame(buttonEvents=[dict(type=button,pressed=False)]),frame()]
      add(f"held-{button}-{metric}",steps,metric=metric)
  for mode in range(4):
    for button in ("accelCruise","decelCruise"):
      for big in (False,True):
        add(f"swipe-{mode}-{button}-{big}",[frame()]+action(button,cruiseSpeedBigStep=big)+action(button),{"CruiseButtonMode":str(mode)})
  for max_gap in (0,3,4,6):
    for levels in (0,2,3,4,8):
      for pcm in (False,True):
        steps=[frame()]+sum([action("gapAdjustCruise",pcmCruiseGap=4) for _ in range(5)],[])
        add(f"gap-{max_gap}-{levels}-{pcm}",steps,{"LongitudinalPersonalityMax":str(max_gap),"CruiseGapLevels":str(levels)},pcm=pcm)
  for paddle in range(4):
    for lfa in range(3):
      add(f"paddle-lfa-{paddle}-{lfa}",[frame()]+action("paddleLeft")+action("lfaButton")+action("accelCruise"),{"PaddleMode":str(paddle),"LfaButtonMode":str(lfa)})
  for enabled in (False,True):
    for disengage in (False,True):
      for duration in (1,39,40,41):
        steps=[frame()]+[frame(gasPressed=True,gas=.4,aEgo=-.6) for _ in range(duration)]+[frame(),frame()]+action("setCruise")+action("resumeCruise")
        add(f"gas-{enabled}-{disengage}-{duration}",steps,{"DisengageOnAccelerator":str(int(disengage))},enabled=enabled)
  for sync in (0,1):
    steps=[frame()]+[frame(gasPressed=True,gas=.4,vEgoCluster=100/3.6) for _ in range(41)]
    add(f"gas-sync-{sync}",steps,{"AutoGasSyncSpeed":str(sync)})
  for tok in (20,100):
    for cancel in (30,100):
      add(f"gas-release-threshold-{tok}-{cancel}",[frame(),frame(gasPressed=True,gas=.2),frame()],{"AutoGasTokSpeed":str(tok),"AutoGasCancelSpeed":str(cancel)},enabled=False)
  for cancel_hold in (False,True):
    for interlock in ("none","brakeHoldActive","parkingBrake"):
      for final in ("release","gas","set","cancel"):
        steps=[frame(vEgo=0,vEgoCluster=0)]+action("cancel",vEgo=0,vEgoCluster=0)
        fields={} if interlock=="none" else {interlock:True}
        steps += [frame(vEgo=0,vEgoCluster=0,brakePressed=True,**fields) for _ in range(62)]
        if final=="gas": steps += [frame(vEgo=0,vEgoCluster=0,gasPressed=True,gas=.2)]
        elif final=="release": steps += [frame(vEgo=0,vEgoCluster=0)]
        else: steps += action(final if final=="cancel" else "setCruise",vEgo=0,vEgoCluster=0)
        add(f"hold-{cancel_hold}-{interlock}-{final}",steps,{"SoftHoldOnCancel":str(int(cancel_hold))},enabled=False)
  for steering in (19.9,20.,69.99,70.,70.01,85.):
    for turn in (False,True):
      steps=[frame(steeringAngleDeg=steering,leftBlinker=turn)]
      steps[0]["events"]=[event("longitudinalPlan",xState=3,aTarget=.4),event("radarState",leadOne=dict(status=True,dRel=5,vRel=-2,vLeadK=5))]
      steps += [frame(steeringAngleDeg=steering,leftBlinker=turn) for _ in range(3)]
      add(f"steering-{steering}-{turn}",steps,enabled=False)
  for mode in ("model-positive","model-negative","road-positive","road-negative"):
    params={"ApplyModelSpeed":"100" if mode=="model-positive" else "-100" if mode=="model-negative" else "0",
      "AutoRoadSpeedAdjust":"50" if mode=="road-positive" else "-100" if mode=="road-negative" else "0","AutoSpeedUptoRoadSpeedLimit":"100"}
    steps=[]
    for limit in (80,50,0,-1,90):
      step=frame();step["events"]=[event("carrotMan",nRoadLimitSpeed=limit,desiredSpeed=250),event("drivingModelData",action=dict(desiredVelocity=30))];steps.append(step)
    steps += [frame() for _ in range(36)]
    add(mode,steps,params)
  for pcm in (False,True):
    steps=[frame(cruiseState=dict(available=False))]+[frame(cruiseState=dict(available=True,speed=value,speedCluster=value+1)) for value in (0,-1,20,50)]+[frame(cruiseState=dict(available=False))]
    add(f"pcm-speed-{pcm}",steps,{"SpeedFromPCM":"1"},pcm=pcm)
  for offset in (-1,10):
    steps=[]
    for limit in (80,50):
      step=frame();step["events"]=[event("carrotMan",nRoadLimitSpeed=limit,desiredSpeed=250)];steps.append(step)
    steps+=action("resumeCruise")
    add(f"road-offset-{offset}",steps,{"AutoRoadSpeedAdjust":"-100","AutoSpeedUptoRoadSpeedLimit":"100","AutoRoadSpeedLimitOffset":str(offset)})
  for limit in (0,-1,250,251):
    step=frame();step["events"]=[event("carrotMan",nRoadLimitSpeed=50,desiredSpeed=limit,carrotCmdIndex=1,carrotCmd="CRUISE",carrotArg="ON")]
    add(f"navigation-desired-validity-{limit}",[frame(),step,frame()],enabled=False)
  steps=[frame() for _ in range(25)]
  steps[3]["params"]={"UseLaneLineSpeed":"45","CruiseButtonMode":"3","AutoCruiseControl":"0","CruiseSpeed1":"35","CruiseSpeed2":"55","CruiseSpeed3":"75","CruiseSpeed4":"95","CruiseSpeed5":"115"}
  steps[9]=frame(buttonEvents=[dict(type="accelCruise",pressed=True)]);steps[10]=frame(buttonEvents=[dict(type="accelCruise",pressed=False)])
  add("params-refresh-cadence",steps,metric=False)
  for command,arg in (("CRUISE","ON"),("CRUISE","OFF"),("CRUISE","GO"),("CRUISE","STOP"),("SPEED","UP"),("SPEED","DOWN"),("SPEED","99"),("SPEED","0"),("SPEED","200")):
    steps=[frame(),frame(),frame(),frame()]
    steps[1]["events"]=[event("carrotMan",nRoadLimitSpeed=80,desiredSpeed=250,carrotCmdIndex=1,carrotCmd=command,carrotArg=arg)]
    steps[3]["events"]=[event("carrotMan",nRoadLimitSpeed=80,desiredSpeed=250,carrotCmdIndex=2,carrotCmd=command,carrotArg=arg)]
    add(f"command-{command}-{arg}",steps,enabled=False)
  for arg in ("9_9", " +099 ", "９９", "٩٩", "\u200799\u2007", "0"*30+"99", "9"*100, "-"+"9"*100):
    step=frame();step["events"]=[event("carrotMan",nRoadLimitSpeed=80,desiredSpeed=250,carrotCmdIndex=1,carrotCmd="SPEED",carrotArg=arg)]
    add(f"integer-command-{arg}",[frame(),step],enabled=False)
  for remote in ("accelCruise","decelCruise","gapAdjustCruise","lfaButton","cancel","accelCruiseLong","decelCruiseLong","gapAdjustCruiseLong","lfaButtonLong","cancelLong","carrotCruise","paddleDecel"):
    for blocked in ("none","hold","steering","repeat"):
      steps=[frame(),frame(),frame(),frame()]
      created=10.+.031*2
      steps[1]["files"]={"cruise.json":dict(events=[dict(id="session:1",time=created,action=remote,address="00:11:22:33:44:55",repeat=blocked=="repeat")])}
      if blocked=="hold": steps[1]["cs"]=frame(brakeHoldActive=True)["cs"]
      if blocked=="steering": steps[1]["cs"]=frame(steeringAngleDeg=70)["cs"]
      add(f"remote-{remote}-{blocked}",steps,{"AutoCruiseControl":"0"},enabled=False)
  for reject in ("startup","stale","future","cancelled","learning","vehicle-button","unavailable","gear","invalid-can"):
    steps=[frame(),frame(),frame()];created=10.+.031*2
    payload=dict(id="s:1",time=created,action="accelCruise",address="00:11:22:33:44:55")
    if reject=="startup": payload["time"]=9.9
    if reject=="stale": steps[1]["advance"]=.6
    if reject=="future": payload["time"]=20.
    steps[1]["files"]={"cruise.json":dict(events=[payload])}
    if reject=="cancelled": steps[1]["files"]["cancelled.json"]={payload["address"]:created}
    if reject=="learning": steps[1]["files"]["learn.json"]=dict(address=payload["address"],until=20.)
    if reject=="vehicle-button": steps[1]["cs"]=frame(buttonEvents=[dict(type="cancel",pressed=True)])["cs"]
    if reject=="unavailable": steps[1]["cs"]=frame(cruiseState=dict(available=False))["cs"]
    if reject=="gear": steps[1]["cs"]=frame(gearShifter="park")["cs"]
    if reject=="invalid-can": steps[1]["cs"]=frame(canValid=False)["cs"]
    add(f"remote-reject-{reject}",steps,enabled=False)
  rng=random.Random(177)
  for sequence in range(12):
    steps=[]
    for index in range(120):
      button=rng.choice(("accelCruise","decelCruise","cancel","gapAdjustCruise","lfaButton","setCruise","resumeCruise","paddleLeft"))
      steps.append(frame(vEgo=rng.choice((0.,.05,5.,10.,20.,30.)),vEgoCluster=rng.choice((0.,10.,20.)),gasPressed=index%11==0,brakePressed=index%7==0,steeringAngleDeg=rng.choice((0.,20.,70.)),cruiseSpeedBigStep=index%13==0,buttonEvents=[dict(type=button,pressed=index%2==0)]))
      steps[-1]["events"]=[event("carControl",enabled=index%3!=0),event("radarState",leadOne=dict(status=index%5!=0,dRel=rng.choice((0.,5.,20.,80.)),vRel=-3,vLeadK=10)),event("longitudinalPlan",xState=rng.choice((0,3,5)),aTarget=.2)]
    add(f"temporal-seeded-{sequence}",steps,{"CruiseButtonMode":str(sequence%4)},metric=sequence%2==0)
  return cases
