use crate::{config::Config, framing, reader::Reader, runtime::pause, serial::Diagnostic, Error};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
pub fn retry<T>(
    attempts: usize,
    stop: &AtomicBool,
    mut action: impl FnMut() -> Result<T, Error>,
) -> Result<T, Error> {
    for attempt in 0..attempts {
        match action() {
            Ok(value) => return Ok(value),
            Err(Error::Stopped) => return Err(Error::Stopped),
            Err(error) => {
                pause(Duration::from_secs(1), stop)?;
                if attempt + 1 == attempts {
                    return Err(error);
                }
            }
        }
    }
    Err(Error::Protocol("empty retry budget"))
}
pub fn logs(diag: &mut Diagnostic, types: &[u16], stop: &AtomicBool) -> Result<(), Error> {
    let (_, payload) = diag.exchange(framing::LOG_CONFIG, &[0, 0, 0, 1, 0, 0, 0], stop)?;
    let mut reader = Reader::new(
        payload
            .get(3..)
            .ok_or(Error::Protocol("log config header"))?,
    );
    if reader.u32()? != 1 || reader.u32()? != 0 {
        return Err(Error::Protocol("log range response"));
    }
    let mut ranges = [0; 16];
    for range in &mut ranges {
        *range = reader.u32()?;
    }
    for (kind, count) in ranges
        .into_iter()
        .enumerate()
        .filter(|(_, count)| *count != 0)
    {
        let kind = u32::try_from(kind).map_err(|_| Error::Protocol("log kind"))?;
        // The ID is 12 bits; a larger firmware range cannot name additional log IDs.
        let size =
            usize::try_from(count.div_ceil(8)).map_err(|_| Error::Protocol("log mask size"))?;
        let mut request = vec![0, 0, 0];
        for word in [3, kind, count] {
            request.extend(word.to_le_bytes());
        }
        let mut mask = vec![0; size];
        for &typ in types {
            let index = u32::from(typ) & 0xfff;
            if u32::from(typ) >> 12 == kind && index < count {
                mask[usize::try_from(index / 8)
                    .map_err(|_| Error::Protocol("log mask index"))?] |= 1 << (index % 8);
            }
        }
        request.extend(mask);
        let (opcode, payload) = diag.exchange(framing::LOG_CONFIG, &request, stop)?;
        let mut reader = Reader::new(payload.get(3..).ok_or(Error::Protocol("log mask header"))?);
        if opcode != framing::LOG_CONFIG || reader.u32()? != 3 || reader.u32()? != 0 {
            return Err(Error::Protocol("log mask response"));
        }
    }
    Ok(())
}
pub fn wait(config: &Config, stop: &AtomicBool) -> Result<(), Error> {
    while !config.at.path.exists() {
        pause(Duration::from_millis(500), stop)?;
    }
    loop {
        match config.at.command("AT+QGPS?", stop) {
            Ok(response) if response.contains("+QGPS:") => return Ok(()),
            Err(Error::Stopped) => return Err(Error::Stopped),
            Ok(_) | Err(_) => pause(Duration::from_millis(500), stop)?,
        }
    }
}
pub fn quectel(diag: &mut Diagnostic, config: &Config, stop: &AtomicBool) -> Result<bool, Error> {
    retry(5, stop, || {
        diag.exchange(39, &[0xfd, 0x1b, 1, 0, 0, 0], stop)?;
        diag.exchange(38, &[0xfd, 0x1b], stop)?;
        retry(10, stop, || logs(diag, &framing::LOG_TYPES, stop))?;
        if config.at.command("AT+QGPS?", stop)?.contains("QGPS: 1") {
            config.at.command("AT+QGPSEND", stop)?;
        }
        config.at.command(
            if config.cold_start {
                "AT+QGPSDEL=0"
            } else {
                "AT+QGPSDEL=1"
            },
            stop,
        )?;
        for command in [
            "AT+QGPSCFG=\"dpoenable\",0",
            "AT+QGPSCFG=\"autogps\",0",
            "AT+QGPSXTRA=1",
            "AT+QGPSSUPLURL=\"NULL\"",
        ] {
            config.at.command(command, stop)?;
        }
        let assistance = config.assistance.exists();
        if assistance {
            crate::assistance::inject(config, stop)?;
            std::fs::remove_file(&config.assistance)?;
        }
        if openpilot_timed::clock::valid(&openpilot_timed::clock::SystemClock, &config.systemd)? {
            config.at.command(
                &format!(
                    "AT+QGPSXTRATIME=0,\"{}\",1,1,1000",
                    chrono::Utc::now().format("%Y/%m/%d,%H:%M:%S")
                ),
                stop,
            )?;
        }
        config
            .at
            .command("AT+QGPSCFG=\"outport\",\"usbnmea\"", stop)?;
        config.at.command("AT+QGPS=1", stop)?;
        diag.exchange(
            75,
            &[
                13, 100, 0, 202, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            stop,
        )?;
        Ok(assistance)
    })
}
pub fn teardown(diag: &mut Diagnostic, config: &Config) -> Result<(), Error> {
    let stop = AtomicBool::new(false);
    config
        .at
        .command("AT+QGPSCFG=\"outport\",\"none\"", &stop)?;
    if config.at.command("AT+QGPS?", &stop)?.contains("QGPS: 1") {
        config.at.command("AT+QGPSEND", &stop)?;
    }
    retry(10, &stop, || logs(diag, &[], &stop))
}
pub fn stopping(stop: &AtomicBool) -> bool {
    stop.load(Ordering::Relaxed)
}
