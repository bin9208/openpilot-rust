use std::{
    ffi::{c_int, c_ulong, c_void},
    sync::Mutex,
};
pub type Render = Box<dyn FnMut(&mut [f32], u64) -> bool + Send>;
pub type Capture = Box<dyn FnMut(&[f32], u64) -> bool + Send>;
pub enum Handler {
    Output(Render),
    Input(Capture),
}
pub struct Callback(pub Mutex<Handler>);
pub unsafe extern "C" fn render(
    input: *const c_void,
    output: *mut c_void,
    frames: c_ulong,
    _time: *const c_void,
    flags: c_ulong,
    user: *mut c_void,
) -> c_int {
    let Ok(count) = usize::try_from(frames) else {
        return 2;
    };
    if user.is_null() || count > isize::MAX.unsigned_abs() / size_of::<f32>() {
        return 2;
    }
    let completed = std::panic::catch_unwind(|| {
        // SAFETY: Pa_OpenStream receives this boxed Callback, retained through
        // Pa_CloseStream. The callback only shares its synchronized handler.
        let callback = unsafe { &*user.cast::<Callback>() };
        let Ok(mut handler) = callback.0.lock() else {
            return false;
        };
        match &mut *handler {
            Handler::Output(function) => {
                if output.is_null() {
                    return false;
                }
                // SAFETY: PortAudio owns an exclusive aligned mono float32 output
                // allocation of frameCount samples; initialize before borrowing.
                let samples = unsafe {
                    output.cast::<f32>().write_bytes(0, count);
                    std::slice::from_raw_parts_mut(output.cast::<f32>(), count)
                };
                function(samples, flags)
            }
            Handler::Input(function) => {
                if input.is_null() {
                    return false;
                }
                // SAFETY: PortAudio supplies frameCount initialized, aligned,
                // mono float32 samples, readable until this callback returns.
                let samples = unsafe { std::slice::from_raw_parts(input.cast::<f32>(), count) };
                function(samples, flags)
            }
        }
    });
    if matches!(completed, Ok(true)) {
        0
    } else {
        2
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_initializes_and_exclusively_mutates_foreign_buffer() {
        let callback = Box::new(Callback(Mutex::new(Handler::Output(Box::new(
            |samples, flags| {
                assert!(samples.iter().all(|sample| *sample == 0.));
                assert_eq!(flags, 4);
                samples.fill(0.25);
                true
            },
        )))));
        let mut output = [std::mem::MaybeUninit::<f32>::uninit(); 4096];
        // SAFETY: owned aligned allocation and stable Callback live for the call.
        let result = unsafe {
            render(
                std::ptr::null(),
                output.as_mut_ptr().cast(),
                4096,
                std::ptr::null(),
                4,
                std::ptr::from_ref(callback.as_ref()).cast_mut().cast(),
            )
        };
        assert_eq!(result, 0);
        for sample in output {
            // SAFETY: render initializes all samples before invoking the closure.
            assert_eq!(unsafe { sample.assume_init() }, 0.25);
        }
    }
    #[test]
    fn input_callback_borrows_initialized_foreign_samples() {
        let callback = Box::new(Callback(Mutex::new(Handler::Input(Box::new(
            |samples, flags| {
                assert_eq!(samples, &[0.25; 800]);
                assert_eq!(flags, 2);
                true
            },
        )))));
        let input = [0.25_f32; 800];
        // SAFETY: immutable input and stable boxed Callback live for the call.
        assert_eq!(
            unsafe {
                render(
                    input.as_ptr().cast(),
                    std::ptr::null_mut(),
                    800,
                    std::ptr::null(),
                    2,
                    std::ptr::from_ref(callback.as_ref()).cast_mut().cast(),
                )
            },
            0
        );
    }
    #[test]
    fn callback_panic_aborts_instead_of_unwinding_across_c() {
        let callback = Box::new(Callback(Mutex::new(Handler::Input(Box::new(|_, _| {
            panic!("fixture")
        })))));
        let input = [0_f32];
        // SAFETY: owned initialized input and callback remain alive.
        assert_eq!(
            unsafe {
                render(
                    input.as_ptr().cast(),
                    std::ptr::null_mut(),
                    1,
                    std::ptr::null(),
                    0,
                    std::ptr::from_ref(callback.as_ref()).cast_mut().cast(),
                )
            },
            2
        );
    }
}
