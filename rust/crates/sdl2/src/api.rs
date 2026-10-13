use std::{
    ffi::{c_char, c_int, c_void},
    path::Path,
};

macro_rules! api {
    ($($field:ident: $signature:ty => $symbol:literal),+ $(,)?) => {
        pub(crate) struct Api {
            $(pub $field: $signature,)+
            pub(crate) _library: libloading::Library,
        }
        impl Api {
            pub fn load(path: &Path) -> Result<Self, libloading::Error> {
                // SAFETY: the caller chooses the SDL2 library; all signatures below match its C ABI.
                let library = unsafe { libloading::Library::new(path) }?;
                Ok(Self {
                    // SAFETY: SDL2 exports these stable function signatures; the owned library outlives pointers.
                    $($field: unsafe { *library.get::<$signature>($symbol)? },)+
                    _library: library,
                })
            }
        }
    }
}
api! {
    init_subsystem: unsafe extern "C" fn(u32) -> c_int => b"SDL_InitSubSystem\0",
    quit_subsystem: unsafe extern "C" fn(u32) -> () => b"SDL_QuitSubSystem\0",
    get_error: unsafe extern "C" fn() -> *const c_char => b"SDL_GetError\0",
    num_joysticks: unsafe extern "C" fn() -> c_int => b"SDL_NumJoysticks\0",
    is_controller: unsafe extern "C" fn(c_int) -> c_int => b"SDL_IsGameController\0",
    controller_open: unsafe extern "C" fn(c_int) -> *mut c_void => b"SDL_GameControllerOpen\0",
    controller_close: unsafe extern "C" fn(*mut c_void) -> () => b"SDL_GameControllerClose\0",
    controller_axis: unsafe extern "C" fn(*mut c_void, c_int) -> i16 => b"SDL_GameControllerGetAxis\0",
    controller_button: unsafe extern "C" fn(*mut c_void, c_int) -> u8 => b"SDL_GameControllerGetButton\0",
    joystick_open: unsafe extern "C" fn(c_int) -> *mut c_void => b"SDL_JoystickOpen\0",
    joystick_close: unsafe extern "C" fn(*mut c_void) -> () => b"SDL_JoystickClose\0",
    joystick_axis: unsafe extern "C" fn(*mut c_void, c_int) -> i16 => b"SDL_JoystickGetAxis\0",
    joystick_button: unsafe extern "C" fn(*mut c_void, c_int) -> u8 => b"SDL_JoystickGetButton\0",
    joystick_hat: unsafe extern "C" fn(*mut c_void, c_int) -> u8 => b"SDL_JoystickGetHat\0",
    joystick_axes: unsafe extern "C" fn(*mut c_void) -> c_int => b"SDL_JoystickNumAxes\0",
    joystick_buttons: unsafe extern "C" fn(*mut c_void) -> c_int => b"SDL_JoystickNumButtons\0",
    joystick_hats: unsafe extern "C" fn(*mut c_void) -> c_int => b"SDL_JoystickNumHats\0",
    joystick_name: unsafe extern "C" fn(*mut c_void) -> *const c_char => b"SDL_JoystickName\0",
    controller_name: unsafe extern "C" fn(*mut c_void) -> *const c_char => b"SDL_GameControllerName\0",
    pump_events: unsafe extern "C" fn() -> () => b"SDL_PumpEvents\0",
    joystick_update: unsafe extern "C" fn() -> () => b"SDL_JoystickUpdate\0",
}
