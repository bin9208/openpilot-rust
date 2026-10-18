@0xbadafeabcde78945;
struct Event {
  logMonoTime @0 :UInt64;
  valid @1 :Bool = true;
  message @2 :Text;
  extra @3 :Data;
  union {
    roadCameraState @4 :Frame;
    wideRoadCameraState @5 :Frame;
    driverCameraState @6 :Frame;
    second @7 :UInt64;
    nothing @8 :Void;
  }
  frames @9 :List(Frame);
  scalar @10 :Float64;
  big @11 :Int64;
  grouped :group { flag @12 :Bool; stamp @13 :UInt64; }
}
struct Frame { image @0 :Data; processing @1 :Float32; label @2 :Text; count @3 :UInt64; }
