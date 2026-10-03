# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp"]
# ///
# Run with oracle Python: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/card_hyundai_edges_source.py FIXTURE_DIR
import ctypes
from datetime import datetime
import errno
import json
from pathlib import Path
import struct
import sys
from zoneinfo import ZoneInfo

from can_source import load


def main()->None:
    load()
    from opendbc.car.hyundai.carstate import NUMERIC_TO_TZ
    root=Path(sys.argv[1]);times=[]
    dates=((0,1,1,0,0,0),(26,1,15,13,2,49),(26,7,15,13,2,49),(26,3,8,2,30,0),(26,11,1,1,30,0),
        (26,3,29,2,30,0),(26,10,25,2,30,0),(26,10,4,2,30,0),(50,3,13,2,30,0),(50,11,6,1,30,0),
        (26,2,30,0,0,0),(26,13,1,0,0,0),(26,1,1,25,0,0))
    for country in (*NUMERIC_TO_TZ,999):
        for date in dates:
            fields=dict(zip(("YEAR","MONTH","DATE","HOURS","MINUTES","SECONDS"),date,strict=True));timestamp=None
            try:timestamp=int(datetime(date[0]+2000,*date[1:],tzinfo=ZoneInfo(NUMERIC_TO_TZ.get(country,"UTC"))).timestamp()*1000)
            except ValueError:pass
            times.append(dict(country=country,fields=fields,timestamp=timestamp))
    library=ctypes.CDLL(None,use_errno=True);function=library.strtof
    function.argtypes=[ctypes.c_char_p,ctypes.POINTER(ctypes.c_void_p)];function.restype=ctypes.c_float
    values=(None,""," 0.1extra","+12.34e-2tail","-0","1e+oops"," ","junk","1e39","1e-46","1e-38","1.17549435e-38",
        "inf","-infinity","nan","-nan(123)","0x1.8p+2suffix","0XABC.DEFp-10","0x","0x1p","0x0p10000",
        "0x1p-149","0x1.1p-149","0x1.fffffep-127","0x1.ffffffp-127","nan(0x1234)")
    values=(*values,*(f"0x{index*997:x}.{index*331:x}p{index%301-160}tail" for index in range(1,500)),
        *(f"{-1 if index%2 else 1}9.{index*551}e{index%91-50}tail" for index in range(500)))
    floats=[]
    for raw in values:
        error=None;bits=0
        if raw is not None and raw!="":
            buffer=ctypes.create_string_buffer(raw.encode());end=ctypes.c_void_p();ctypes.set_errno(0)
            value=function(ctypes.cast(buffer,ctypes.c_char_p),ctypes.byref(end))
            if end.value==ctypes.addressof(buffer):error="invalid"
            elif ctypes.get_errno()==errno.ERANGE:error="range"
            bits=struct.unpack("I",struct.pack("f",value))[0]
        floats.append(dict(raw=raw,bits=bits,error=error))
    windows=([1e16,1.,-1e16],[2.,1e-15]*20,[1e-20,0.5]*20,[2.0**-50,0.3]*20)
    sums=[dict(values=values,total=sum(values)) for values in windows]
    from opendbc.car import structs
    from opendbc.car.hyundai import carcontroller,hyundaicanfd
    from opendbc.car.hyundai.values import DBC
    from card_hyundai_controller_source import Settings
    from types import SimpleNamespace as NS
    row=json.loads((root / "state.json").read_text())[0];settings=Settings(row)
    carcontroller.Params=hyundaicanfd.Params=lambda:settings
    with structs.CarParams.from_bytes((Path(row["source_dir"]) / "params.bin").read_bytes()) as reader:cp=reader.as_builder()
    invalid=[]
    for token in ("nan","inf","-inf"):
        controller=carcontroller.CarController(DBC[cp.carFingerprint],cp)
        control=structs.CarControl.new_message();control.actuators.torque=float(token)
        try:controller.update(control.as_reader(),NS(out=NS(steeringTorque=0)),0)
        except (ValueError,OverflowError) as error:invalid.append(dict(torque=token,error=type(error).__name__))
    assert len(invalid)==3
    (root / "edges.json").write_text(json.dumps(dict(times=times,floats=floats,sums=sums,invalid_torque=invalid))+"\n")
    print(f"Generated {len(times)} source local-time and {len(floats)} source std::stof conversion cases")


if __name__=="__main__":
    main()
