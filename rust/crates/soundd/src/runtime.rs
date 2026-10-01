use crate::{assets::Assets, settings, Error, Input, Policy};
use openpilot_beepd::{Clock, SystemClock};
use openpilot_cereal::{car_capnp::car_state::button_event::Type, log_capnp::event};
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_messaging::{runtime::SubMaster, state::Options};
use openpilot_params::Params;
use openpilot_portaudio::Stream;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
pub struct Config {
    pub assets: PathBuf,
    pub library: PathBuf,
    pub cycles: Option<u64>,
}
fn monotonic() -> Result<f64, Error> {
    SystemClock
        .monotonic()
        .map_err(|_| Error::Contract("monotonic clock"))
}
fn input(subscriber: &SubMaster) -> Result<Input, Error> {
    let selfdrive = subscriber.state.topic("selfdriveState")?;
    let carrot = subscriber.state.topic("carrotMan")?;
    let pressure = subscriber.state.topic("soundPressure")?;
    let car = subscriber.state.topic("carState")?;
    let mut input = Input {
        now: monotonic()?,
        updated_selfdrive: selfdrive.updated,
        updated_carrot: carrot.updated,
        selfdrive_received: selfdrive.receive_time,
        ..Input::default()
    };
    if let event::Which::SelfdriveState(message) = selfdrive
        .event()?
        .which()
        .map_err(|_| Error::Contract("selfdriveState union"))?
    {
        let message = message?;
        input.enabled = message.get_enabled();
        input.alert = message
            .get_alert_sound()
            .map(u16::from)
            .unwrap_or_else(|capnp::NotInSchema(value)| value);
    }
    if let event::Which::CarrotMan(message) = carrot
        .event()?
        .which()
        .map_err(|_| Error::Contract("carrotMan union"))?
    {
        input.countdown = message?.get_left_sec();
    }
    if pressure.updated {
        if let event::Which::SoundPressure(message) = pressure
            .event()?
            .which()
            .map_err(|_| Error::Contract("soundPressure union"))?
        {
            input.pressure = Some(f64::from(message?.get_sound_pressure_weighted_db()));
        }
    }
    if let event::Which::CarState(message) = car
        .event()?
        .which()
        .map_err(|_| Error::Contract("carState union"))?
    {
        for button in message?.get_button_events()? {
            if button.get_type() == Ok(Type::MainCruise) {
                input.main_buttons.push(button.get_pressed());
            }
        }
    }
    Ok(input)
}
pub fn run(config: Config, stop: Arc<AtomicBool>) -> Result<(), Error> {
    let factory = Factory::for_runtime()?;
    let mut logger = factory.logger();
    let params = Params::for_runtime()?;
    let tizi = openpilot_hardware_info::for_runtime().get_device_type()? == "tizi";
    let assets = Assets {
        root: config.assets,
        tizi,
        engage_volume: f64::from(settings::integer(&params, "SoundVolumeAdjustEngage")?) / 100.,
    };
    let mut language = settings::language(&params, &mut logger)?;
    let policy = Arc::new(Mutex::new(Policy::new(
        assets.load(&language, &mut logger)?,
        tizi,
    )));
    let status = Arc::new(AtomicU64::new(0));
    let callback_policy = Arc::clone(&policy);
    let callback_status = Arc::clone(&status);
    let mut stream = Stream::load(
        &config.library,
        Box::new(move |output, flags| {
            callback_status.fetch_or(flags, Ordering::Relaxed);
            match callback_policy.lock() {
                Ok(mut state) => state.playback.render(output).is_ok(),
                Err(_) => false,
            }
        }),
    )?;
    let mut subscriber = SubMaster::for_runtime(
        &["selfdriveState", "soundPressure", "carrotMan", "carState"],
        Options::default(),
    )?;
    for attempt in 0..10 {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match stream.open() {
            Ok(()) => break,
            Err(error) => {
                println!("get_stream failed, trying again");
                for _ in 0..150 {
                    if stop.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                if attempt == 9 {
                    return Err(error.into());
                }
            }
        }
    }
    stream.start()?;
    let message = format!(
        "soundd stream started: samplerate=48000 channels=1 dtype=float32 device={} blocksize=4096",
        stream.device()?
    );
    logger.emit(log_site!(), Record::text(Level::Info, message.clone()))?;
    println!("{message}");
    let mut next = monotonic()? + 0.05;
    let mut language_check = 0.;
    let mut cycles = 0_u64;
    while !stop.load(Ordering::Relaxed) {
        subscriber.update(Duration::ZERO)?;
        let now = monotonic()?;
        if now >= language_check {
            language_check = now + 1.;
            let candidate = settings::language(&params, &mut logger)?;
            if language != candidate {
                language = candidate;
                let sounds = assets.load(&language, &mut logger)?;
                let mut state = policy
                    .lock()
                    .map_err(|_| Error::Contract("sound state lock poisoned"))?;
                state.playback.sounds = sounds;
                state.playback.frame = 0;
                logger.emit(
                    log_site!(),
                    Record::text(
                        Level::Info,
                        format!(
                            "soundd language changed: {language} ({})",
                            settings::directory(&language)
                        ),
                    ),
                )?;
            }
        }
        let unsupported = policy
            .lock()
            .map_err(|_| Error::Contract("sound state lock poisoned"))?
            .step(&input(&subscriber)?);
        if let Some(alert) = unsupported {
            logger.emit(
                log_site!(),
                Record::text(
                    Level::Error,
                    format!("soundd received unsupported alert {alert}"),
                ),
            )?;
        }
        let flags = status.swap(0, Ordering::Relaxed);
        if flags != 0 {
            logger.emit(
                log_site!(),
                Record::text(
                    Level::Warning,
                    format!("soundd stream over/underflow: {flags}"),
                ),
            )?;
        }
        let remaining = next - monotonic()?;
        next += 0.05;
        if remaining > 0. {
            std::thread::sleep(Duration::from_secs_f64(remaining));
        } else {
            println!("soundd lagging by {:.2} ms", -remaining * 1000.);
        }
        if !stream.active()? {
            return Err(Error::Contract("soundd stream inactive"));
        }
        policy
            .lock()
            .map_err(|_| Error::Contract("sound state lock poisoned"))?
            .adjust = f64::from(settings::integer(&params, "SoundVolumeAdjust")?) / 100.;
        cycles += 1;
        if config.cycles.is_some_and(|limit| cycles >= limit) {
            break;
        }
    }
    Ok(())
}
