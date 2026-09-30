use openpilot_driving_modeld::{camera::Source, Error};
use openpilot_modeld::camera::{self, CameraStream};
use openpilot_msgq::{VisionClient, VisionStream};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub fn configure() -> Result<(), Error> {
    if std::path::Path::new("/TICI").is_file() {
        // SAFETY: sched_param contains integers/timespecs; zero initializes musl's additional fields.
        let mut settings: libc::sched_param = unsafe { std::mem::zeroed() };
        settings.sched_priority = 54;
        // SAFETY: sched_setscheduler borrows the initialized parameter only during this call.
        if unsafe { libc::sched_setscheduler(0, libc::SCHED_FIFO, &settings) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut cores = rustix::thread::CpuSet::new();
        cores.set(7);
        rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
    }
    Ok(())
}

pub use openpilot_driving_modeld::clock::{monotonic, timestamp};

pub fn cameras(
    stop: &Arc<AtomicBool>,
    use_wide: bool,
) -> Result<Option<(Source, Option<Source>, bool)>, Error> {
    eprintln!("modeld: waiting for camera streams");
    let selected = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let available = VisionClient::available_streams("camerad")?;
        if let Some(selected) = camera::select_streams(
            available.contains(&VisionStream::Road),
            available.contains(&VisionStream::WideRoad),
            use_wide,
        ) {
            break selected;
        }
        thread::sleep(Duration::from_millis(100));
    };
    let main_wide = selected.0 == CameraStream::WideRoad;
    let mut main = Source {
        client: VisionClient::new(
            "camerad",
            if main_wide {
                VisionStream::WideRoad
            } else {
                VisionStream::Road
            },
            true,
        )?,
        stop: Arc::clone(stop),
    };
    let mut extra = if selected.1 {
        Some(Source {
            client: VisionClient::new("camerad", VisionStream::WideRoad, false)?,
            stop: Arc::clone(stop),
        })
    } else {
        None
    };
    for source in std::iter::once(&mut main).chain(extra.iter_mut()) {
        while !stop.load(Ordering::Relaxed) && !source.client.connect()? {
            thread::sleep(Duration::from_millis(100));
        }
    }
    if stop.load(Ordering::Relaxed) {
        return Ok(None);
    }
    eprintln!("modeld: cameras connected");
    Ok(Some((main, extra, main_wide)))
}
