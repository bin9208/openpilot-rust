use crate::{
    analysis::{self, Analyzer, Pressure},
    wire, Error, RATE,
};
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_messaging::runtime::PubMaster;
use openpilot_portaudio::Stream;
use std::{
    cell::RefCell,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

thread_local! {
    static RAW_PUBLISHER: RefCell<Option<PubMaster>> = const { RefCell::new(None) };
}

fn capture(samples: &[f32], bytes: &mut Vec<u8>) -> Result<(), Error> {
    analysis::raw_audio(samples, bytes);
    let packet = wire::raw(bytes)?;
    RAW_PUBLISHER.with(|publisher| {
        let mut publisher = publisher.borrow_mut();
        if publisher.is_none() {
            *publisher = Some(PubMaster::for_runtime(&["rawAudioData"])?);
        }
        publisher
            .as_mut()
            .ok_or(Error::Contract("raw publisher missing"))?
            .send("rawAudioData", &packet)?;
        Ok(())
    })
}

fn clear_publisher() {
    RAW_PUBLISHER.with(|publisher| *publisher.borrow_mut() = None);
}

pub fn run(library: &Path, stop: Arc<AtomicBool>, cycles: Option<u64>) -> Result<(), Error> {
    let factory = Factory::for_runtime()?;
    let mut logger = factory.logger();
    let mut publisher = PubMaster::for_runtime(&["soundPressure"])?;
    let shared = Arc::new(Mutex::new(Pressure::default()));
    let callback_shared = Arc::clone(&shared);
    let callback_stop = Arc::clone(&stop);
    let mut analyzer = Analyzer::default();
    let mut bytes = Vec::with_capacity(crate::SAMPLE_BUFFER * 2);
    let mut stream = Stream::load_input(
        library,
        Box::new(move |samples, _flags| {
            if callback_stop.load(Ordering::Relaxed) {
                clear_publisher();
                return false;
            }
            let result = (|| -> Result<(), Error> {
                capture(samples, &mut bytes)?;
                let mut pressure = callback_shared
                    .lock()
                    .map_err(|_| Error::Contract("pressure lock poisoned"))?;
                analyzer.append(samples);
                *pressure = analyzer.pressure();
                Ok(())
            })();
            match result {
                Ok(()) => true,
                Err(error) => {
                    eprintln!("micd callback: {error}");
                    clear_publisher();
                    false
                }
            }
        }),
    )?;
    let mut opened = false;
    for _ in 0..10 {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match stream.open() {
            Ok(()) => {
                opened = true;
                break;
            }
            Err(_) => {
                println!("get_stream failed, trying again");
                if sleep_until(Instant::now() + Duration::from_secs(3), &stop) {
                    return Ok(());
                }
            }
        }
    }
    if !opened {
        return Err(Error::Contract("get_stream failed after retry"));
    }
    stream.start()?;
    logger.emit(
        log_site!(),
        Record::text(Level::Info, format!("micd stream started: stream.samplerate=16000.0 stream.channels=1 stream.dtype='float32' stream.device={}, stream.blocksize=800", stream.device()?)),
    )?;
    let mut next = None;
    let mut frames = 0_u64;
    let name = std::env::var("MANAGER_DAEMON").unwrap_or_else(|_| "micd".into());
    while !stop.load(Ordering::Relaxed) {
        let pressure = *shared
            .lock()
            .map_err(|_| Error::Contract("pressure lock poisoned"))?;
        publisher.send("soundPressure", &wire::pressure(pressure)?)?;
        let deadline = *next
            .get_or_insert_with(|| Instant::now() + Duration::from_secs_f64(1. / f64::from(RATE)));
        next = Some(deadline + Duration::from_secs_f64(1. / f64::from(RATE)));
        let now = Instant::now();
        if now > deadline {
            println!(
                "{name} lagging by {:.2} ms",
                (now - deadline).as_secs_f64() * 1000.
            );
        }
        sleep_until(deadline, &stop);
        frames += 1;
        if cycles.is_some_and(|limit| frames >= limit) {
            break;
        }
    }
    stop.store(true, Ordering::Relaxed);
    Ok(())
}

fn sleep_until(deadline: Instant, stop: &AtomicBool) -> bool {
    while !stop.load(Ordering::Relaxed) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(20)));
    }
    true
}
