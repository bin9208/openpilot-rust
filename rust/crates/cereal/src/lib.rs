//! Bindings generated from the complete, unmodified repository schemas.
pub mod log_capnp {
    include!(concat!(env!("OUT_DIR"), "/log_capnp.rs"));
}
pub mod car_capnp {
    include!(concat!(env!("OUT_DIR"), "/car_capnp.rs"));
}
pub mod custom_capnp {
    include!(concat!(env!("OUT_DIR"), "/custom_capnp.rs"));
}
pub mod deprecated_capnp {
    include!(concat!(env!("OUT_DIR"), "/deprecated_capnp.rs"));
}
