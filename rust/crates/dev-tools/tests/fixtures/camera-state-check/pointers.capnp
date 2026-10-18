@0xdadadadaed785643;
struct Event { union { object @0 :AnyPointer; capability @1 :Service; nothing @2 :Void; } defaultText @3 :Text = "foo"; unused @4 :AnyPointer; }
interface Service {}
struct Other { value @0 :UInt64; }
