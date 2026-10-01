extern crate cxx;
#[unsafe(no_mangle)]
pub extern "C" fn retain_cxx_runtime() { let _ = cxx::CxxString::to_str; }
